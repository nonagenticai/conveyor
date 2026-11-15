//! The executor side of `runner/v1`: one instruction in, one result out (ADR 0007 §3–4).
//!
//! A media image is not an HTTP service. It is handed one [`Instruction`] — declared inputs, a
//! closed set of outputs, a deadline — and this module is the whole loop around the capability
//! itself:
//!
//! 1. refuse what can be refused without touching the network (a different contract tag, an
//!    instruction the contract itself rejects, a deadline that has already passed, a kind this
//!    image does not implement);
//! 2. fetch every declared input through conveyor's byte route, verifying the declared hash
//!    before the capability ever sees the bytes;
//! 3. run the capability through the same [`Dispatch`] table the CLI uses;
//! 4. upload every declared output through conveyor's own route, reporting the hash the route
//!    returned rather than one computed here — the route is the writer, so its hash is the record;
//! 5. post one [`RunResult`], built from the outcome of steps 2–4.
//!
//! **The job token is read from `RUNNER_TOKEN` and is never logged**, never placed in an error
//! string and never written to the result. The routes carry it in one `Authorization` header.
//!
//! **A result that cannot be posted is not a silent success.** The failure is logged and the
//! process still exits failed; the dispatcher's poll is what notices a pod that died
//! (`apps/server/src/runner_dispatch.rs`). Retries belong to the dispatcher, never here.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Utc};
use conveyor_domain::runner::{
    Artefact, CONTRACT, ErrorClass, ErrorInfo, Input, Instruction, Output, RunResult, Status,
};
use reqwest::StatusCode;
use reqwest::header::CONTENT_TYPE;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::common::entry::Dispatch;
use crate::common::{CapabilityError, Config};

/// The environment variable the pod's job token arrives in. Never argv: a process listing is a
/// second, unmanaged place a credential would live.
pub const TOKEN_ENV: &str = "RUNNER_TOKEN";

/// Execute one instruction: prechecks, inputs, dispatch, outputs, and exactly one posted result.
///
/// The result is returned even when the network is unavailable, so the caller can exit with a
/// status that matches what it would have reported.
pub async fn execute(instruction: Instruction, config: &Config, dispatch: Dispatch) -> RunResult {
    let started = Utc::now();
    if let Some(result) = precheck(&instruction, dispatch, started) {
        return result;
    }
    let token = std::env::var(TOKEN_ENV)
        .map(|token| token.trim().to_owned())
        .unwrap_or_default();
    if token.is_empty() {
        return failed(
            &instruction,
            started,
            info(
                ErrorClass::Permanent,
                "job_token_missing",
                format!("{TOKEN_ENV} is not set"),
            ),
            json!({}),
        );
    }
    execute_with(&instruction, config, dispatch, &token, started).await
}

/// The refusals that need neither a credential nor a socket, in the order they are cheapest to
/// explain: contract tag, contract validity, deadline, kind.
fn precheck(
    instruction: &Instruction,
    dispatch: Dispatch,
    started: DateTime<Utc>,
) -> Option<RunResult> {
    if instruction.schema != CONTRACT {
        return Some(failed(
            instruction,
            started,
            info(
                ErrorClass::Input,
                "unsupported_contract",
                format!(
                    "this executor speaks {CONTRACT}, not {:?}",
                    instruction.schema
                ),
            ),
            json!({}),
        ));
    }
    if let Err(error) = instruction.validate() {
        return Some(failed(
            instruction,
            started,
            info(ErrorClass::Input, "invalid_instruction", error.to_string()),
            json!({}),
        ));
    }
    match instruction.deadline() {
        None => {
            return Some(failed(
                instruction,
                started,
                info(
                    ErrorClass::Input,
                    "deadline_invalid",
                    format!(
                        "{} is not a representable deadline",
                        instruction.deadline_unix_ms
                    ),
                ),
                json!({}),
            ));
        }
        Some(deadline) if deadline <= Utc::now() => {
            return Some(failed(
                instruction,
                started,
                info(
                    ErrorClass::Input,
                    "deadline_passed",
                    format!(
                        "the deadline {} passed before this image started",
                        instruction.deadline_unix_ms
                    ),
                ),
                json!({}),
            ));
        }
        Some(_) => {}
    }
    if !dispatch.kinds.contains(&instruction.kind.as_str()) {
        return Some(failed(
            instruction,
            started,
            info(
                ErrorClass::Input,
                "unknown_kind",
                format!(
                    "this image does not implement {:?}; it implements {:?}",
                    instruction.kind, dispatch.kinds
                ),
            ),
            json!({}),
        ));
    }
    None
}

/// The loop proper. `started` is the process's own start, not the first request's.
/// The same loop as [`execute`] with the token supplied by the caller, so a test never has to
/// mutate the process environment (which is shared by every test in the binary).
pub async fn execute_with(
    instruction: &Instruction,
    config: &Config,
    dispatch: Dispatch,
    token: &str,
    started: DateTime<Utc>,
) -> RunResult {
    let inputs_dir = config.work_dir.join("inputs");
    if let Err(error) = std::fs::create_dir_all(&inputs_dir) {
        return failed(
            instruction,
            started,
            info(
                ErrorClass::Permanent,
                "work_dir_unavailable",
                format!("cannot create {:?}: {error}", inputs_dir),
            ),
            json!({}),
        );
    }

    let session = match Session::new(instruction, config, token) {
        Ok(session) => session,
        Err(error) => return failed(instruction, started, error, json!({})),
    };

    let mut input_bytes: i64 = 0;
    for declared in &instruction.inputs {
        let Some(path) = safe_file(&inputs_dir, &declared.name) else {
            return session
                .fail(
                    instruction,
                    started,
                    info(
                        ErrorClass::Input,
                        "input_unsafe_name",
                        format!("input name {:?} is not a plain file name", declared.name),
                    ),
                )
                .await;
        };
        match session.fetch_input(declared, &path).await {
            Ok(bytes) => input_bytes += bytes,
            Err(error) => return session.fail(instruction, started, error).await,
        }
    }

    match (dispatch.call)(
        instruction.kind.clone(),
        instruction.params.clone(),
        config.clone(),
    )
    .await
    {
        Ok(_) => {}
        Err(error) => {
            return session.fail(instruction, started, classify(&error)).await;
        }
    }

    let mut artefacts = Vec::with_capacity(instruction.outputs.len());
    let mut output_bytes: i64 = 0;
    for declared in &instruction.outputs {
        let Some(path) = safe_file(&config.out_dir, &declared.name) else {
            return session
                .fail(
                    instruction,
                    started,
                    info(
                        ErrorClass::Input,
                        "output_unsafe_name",
                        format!("output name {:?} is not a plain file name", declared.name),
                    ),
                )
                .await;
        };
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return session
                    .fail(
                        instruction,
                        started,
                        info(
                            ErrorClass::Permanent,
                            "output_missing",
                            format!(
                                "the capability succeeded but wrote no output {name:?} at {path:?}",
                                name = declared.name
                            ),
                        ),
                    )
                    .await;
            }
            Err(error) => {
                return session
                    .fail(
                        instruction,
                        started,
                        info(
                            ErrorClass::Permanent,
                            "output_unreadable",
                            format!("cannot read {path:?}: {error}"),
                        ),
                    )
                    .await;
            }
        };
        if bytes.len() as i64 > declared.max_bytes {
            return session
                .fail(
                    instruction,
                    started,
                    info(
                        ErrorClass::Permanent,
                        "output_too_large",
                        format!(
                            "output {:?} is {} bytes; the instruction allows {}",
                            declared.name,
                            bytes.len(),
                            declared.max_bytes
                        ),
                    ),
                )
                .await;
        }
        match session.upload_output(declared, bytes).await {
            Ok(artefact) => {
                output_bytes += artefact.bytes;
                artefacts.push(artefact);
            }
            Err(error) => return session.fail(instruction, started, error).await,
        }
    }

    let result = RunResult {
        schema: CONTRACT.to_owned(),
        job_id: instruction.job_id,
        attempt: instruction.attempt,
        status: Status::Succeeded,
        error: None,
        artefacts,
        metrics: json!({
            "input_bytes": input_bytes,
            "output_bytes": output_bytes,
            "duration_ms": elapsed_ms(started),
        }),
        started_at: started,
        finished_at: Utc::now(),
    };
    session.post_result(&result).await;
    result
}

/// One attempt's HTTP session: the client, the base route, and the credential every call carries.
struct Session<'a> {
    client: reqwest::Client,
    base: String,
    job_id: Uuid,
    token: &'a str,
    service: &'a str,
}

impl<'a> Session<'a> {
    fn new(
        instruction: &Instruction,
        config: &'a Config,
        token: &'a str,
    ) -> Result<Self, ErrorInfo> {
        let remaining = instruction
            .deadline_unix_ms
            .saturating_sub(Utc::now().timestamp_millis());
        if remaining <= 0 {
            return Err(info(
                ErrorClass::Input,
                "deadline_passed",
                "the deadline passed before the first request could be sent",
            ));
        }
        // One request can never outlive the instruction's own deadline: the Job's
        // `activeDeadlineSeconds` is the backstop, this is the ordinary bound.
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(remaining as u64))
            .build()
            .map_err(|error| {
                info(
                    ErrorClass::Permanent,
                    "http_client_unavailable",
                    format!("cannot build the HTTP client: {error}"),
                )
            })?;
        Ok(Self {
            client,
            base: instruction
                .conveyor_base_url
                .trim_end_matches('/')
                .to_owned(),
            job_id: instruction.job_id,
            token,
            service: &config.service,
        })
    }

    /// `GET /api/jobs/{id}/inputs/{name}` into `path`, after verifying the declared hash.
    async fn fetch_input(&self, declared: &Input, path: &Path) -> Result<i64, ErrorInfo> {
        let url = self.url(&["inputs", &declared.name])?;
        let response = self
            .client
            .get(url)
            .bearer_auth(self.token)
            .send()
            .await
            .map_err(|error| request_error("input_fetch_failed", &error))?;
        if !response.status().is_success() {
            return Err(match response.status() {
                StatusCode::FORBIDDEN => info(
                    ErrorClass::Permanent,
                    "job_token_refused",
                    "conveyor refused the job token while serving an input",
                ),
                StatusCode::NOT_FOUND | StatusCode::UNPROCESSABLE_ENTITY => info(
                    ErrorClass::Input,
                    "input_unavailable",
                    format!(
                        "conveyor holds no bytes for input {:?} (HTTP {})",
                        declared.name,
                        response.status().as_u16()
                    ),
                ),
                status if status.is_server_error() => info(
                    ErrorClass::Transient,
                    "input_fetch_failed",
                    format!("conveyor answered {} for an input", status.as_u16()),
                ),
                status => info(
                    ErrorClass::Permanent,
                    "input_fetch_failed",
                    format!("conveyor answered {} for an input", status.as_u16()),
                ),
            });
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|error| request_error("input_fetch_failed", &error))?;
        let actual = hex(&Sha256::digest(&bytes));
        if actual != declared.sha256 {
            return Err(info(
                ErrorClass::Input,
                "input_hash_mismatch",
                format!(
                    "input {:?} has sha256 {actual}, not the declared {}",
                    declared.name, declared.sha256
                ),
            ));
        }
        std::fs::write(path, &bytes).map_err(|error| {
            info(
                ErrorClass::Permanent,
                "work_dir_unavailable",
                format!("cannot write {path:?}: {error}"),
            )
        })?;
        Ok(bytes.len() as i64)
    }

    /// `POST /api/jobs/{id}/outputs/{name}`; the artefact carries the hash the route returned.
    async fn upload_output(
        &self,
        declared: &Output,
        bytes: Vec<u8>,
    ) -> Result<Artefact, ErrorInfo> {
        let url = self.url(&["outputs", &declared.name])?;
        let response = self
            .client
            .post(url)
            .bearer_auth(self.token)
            // The route stores bytes and takes the artefact's type from the instruction, so the
            // header is informational — but it is the declared type, not a generic one: a proxy
            // log that says `video/mp4` is worth more than one that says octets.
            .header(CONTENT_TYPE, declared.content_type.clone())
            .body(bytes)
            .send()
            .await
            .map_err(|error| request_error("output_upload_failed", &error))?;
        if !response.status().is_success() {
            return Err(match response.status() {
                StatusCode::FORBIDDEN => info(
                    ErrorClass::Permanent,
                    "job_token_refused",
                    "conveyor refused the job token for an output",
                ),
                StatusCode::PAYLOAD_TOO_LARGE => info(
                    ErrorClass::Permanent,
                    "output_too_large",
                    format!(
                        "conveyor refused output {:?} as larger than the instruction allows",
                        declared.name
                    ),
                ),
                StatusCode::CONFLICT => info(
                    ErrorClass::Permanent,
                    "output_conflict",
                    format!(
                        "conveyor already holds different bytes for output {:?}",
                        declared.name
                    ),
                ),
                StatusCode::UNPROCESSABLE_ENTITY => info(
                    ErrorClass::Permanent,
                    "output_refused",
                    format!(
                        "conveyor refused output {:?}: the instruction does not declare it",
                        declared.name
                    ),
                ),
                status if status.is_server_error() => info(
                    ErrorClass::Transient,
                    "output_upload_failed",
                    format!("conveyor answered {} for an output", status.as_u16()),
                ),
                status => info(
                    ErrorClass::Permanent,
                    "output_refused",
                    format!("conveyor answered {} for an output", status.as_u16()),
                ),
            });
        }
        #[derive(serde::Deserialize)]
        struct Stored {
            sha256: String,
            bytes: i64,
        }
        let stored: Stored = response.json().await.map_err(|error| {
            info(
                ErrorClass::Transient,
                "output_upload_failed",
                format!("the output route answered with an unreadable body: {error}"),
            )
        })?;
        Ok(Artefact {
            name: declared.name.clone(),
            sha256: stored.sha256,
            bytes: stored.bytes,
            content_type: declared.content_type.clone(),
        })
    }

    /// `POST /api/jobs/{id}/result`. A refusal is logged, never retried here: the dispatcher's
    /// poll owns the attempt's fate.
    async fn post_result(&self, result: &RunResult) {
        let url = match self.url(&["result"]) {
            Ok(url) => url,
            Err(error) => {
                eprintln!(
                    "{}: cannot address the result route: {}",
                    self.service, error.message
                );
                return;
            }
        };
        match self
            .client
            .post(url)
            .bearer_auth(self.token)
            .json(result)
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => {}
            Ok(response) => eprintln!(
                "{}: the result was refused with HTTP {}",
                self.service,
                response.status().as_u16()
            ),
            Err(error) => eprintln!("{}: the result could not be posted: {error}", self.service),
        }
    }

    /// Build the failed result and post it, so every early return reports the same way.
    async fn fail(
        &self,
        instruction: &Instruction,
        started: DateTime<Utc>,
        error: ErrorInfo,
    ) -> RunResult {
        let result = failed(
            instruction,
            started,
            error,
            json!({"duration_ms": elapsed_ms(started)}),
        );
        self.post_result(&result).await;
        result
    }

    fn url(&self, tail: &[&str]) -> Result<reqwest::Url, ErrorInfo> {
        let mut url = reqwest::Url::parse(&self.base).map_err(|error| {
            info(
                ErrorClass::Input,
                "conveyor_url_invalid",
                format!("{:?} is not a URL: {error}", self.base),
            )
        })?;
        {
            let mut segments = url.path_segments_mut().map_err(|_| {
                info(
                    ErrorClass::Input,
                    "conveyor_url_invalid",
                    format!("{:?} cannot carry a path", self.base),
                )
            })?;
            segments.pop_if_empty();
            segments.extend(["api", "jobs", &self.job_id.to_string()]);
            segments.extend(tail);
        }
        Ok(url)
    }
}

/// The capability error vocabulary, projected onto the contract's classes with a stable code.
pub fn classify(error: &CapabilityError) -> ErrorInfo {
    match error {
        CapabilityError::Refused { code, message } => ErrorInfo {
            class: ErrorClass::Input,
            code: code.clone(),
            message: message.clone(),
        },
        CapabilityError::Invalid(message) => {
            info(ErrorClass::Input, "invalid_params", message.clone())
        }
        CapabilityError::UnknownKind(kind) => info(
            ErrorClass::Input,
            "unknown_kind",
            format!("this image does not implement {kind:?}"),
        ),
        CapabilityError::ToolMissing { tool, source } => info(
            ErrorClass::Permanent,
            "tool_missing",
            format!("{tool} is not available: {source}"),
        ),
        CapabilityError::Failed(message) => {
            info(ErrorClass::Transient, "capability_failed", message.clone())
        }
    }
}

fn failed(
    instruction: &Instruction,
    started: DateTime<Utc>,
    error: ErrorInfo,
    metrics: Value,
) -> RunResult {
    RunResult {
        schema: CONTRACT.to_owned(),
        job_id: instruction.job_id,
        attempt: instruction.attempt,
        status: Status::Failed,
        error: Some(error),
        artefacts: Vec::new(),
        metrics,
        started_at: started,
        finished_at: Utc::now(),
    }
}

fn info(class: ErrorClass, code: &str, message: impl Into<String>) -> ErrorInfo {
    ErrorInfo {
        class,
        code: code.to_owned(),
        message: message.into(),
    }
}

/// A request that never reached an answer. A timeout here can only be the deadline: the client's
/// own timeout is the instruction's remaining wall clock.
fn request_error(code: &str, error: &reqwest::Error) -> ErrorInfo {
    if error.is_timeout() {
        return info(
            ErrorClass::Input,
            "deadline_passed",
            "conveyor did not answer before the instruction's deadline",
        );
    }
    info(
        ErrorClass::Transient,
        code,
        format!("conveyor is unreachable: {error}"),
    )
}

/// A single path segment, never an escape: `..`, an absolute path and a nested path all fail.
fn safe_file(root: &Path, name: &str) -> Option<PathBuf> {
    Path::new(name)
        .file_name()
        .filter(|file| *file == OsStr::new(name))
        .map(|file| root.join(file))
}

fn elapsed_ms(started: DateTime<Utc>) -> i64 {
    (Utc::now() - started).num_milliseconds().max(0)
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use axum::Router;
    use axum::body::{Body, Bytes};
    use axum::extract::{Path as AxumPath, State};
    use axum::http::{HeaderMap, StatusCode as HttpStatus, header};
    use axum::response::{IntoResponse, Response};
    use axum::routing::{get, post};
    use conveyor_domain::runner::Limits;
    use futures_util::future::BoxFuture;
    use serde_json::Value as JsonValue;

    use super::*;
    use crate::common::entry::Dispatch;

    const KIND: &str = "media.render_html";

    #[derive(Default)]
    struct Stub {
        token: String,
        input: Vec<u8>,
        hits: AtomicUsize,
        outputs: Mutex<Vec<(String, Vec<u8>)>>,
        results: Mutex<Vec<JsonValue>>,
    }

    impl Stub {
        fn new(input: &[u8]) -> Self {
            Self {
                token: "test-token".to_owned(),
                input: input.to_vec(),
                ..Self::default()
            }
        }

        fn hits(&self) -> usize {
            self.hits.fetch_add(0, Ordering::SeqCst)
        }
    }

    fn authorized(headers: &HeaderMap, token: &str) -> bool {
        let expected = format!("Bearer {token}");
        headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            == Some(expected.as_str())
    }

    async fn stub_input(
        State(stub): State<Arc<Stub>>,
        headers: HeaderMap,
        AxumPath((_id, _name)): AxumPath<(String, String)>,
    ) -> Response {
        stub.hits.fetch_add(1, Ordering::SeqCst);
        if !authorized(&headers, &stub.token) {
            return HttpStatus::FORBIDDEN.into_response();
        }
        (
            HttpStatus::OK,
            [(header::CONTENT_TYPE, "text/plain")],
            Body::from(stub.input.clone()),
        )
            .into_response()
    }

    async fn stub_output(
        State(stub): State<Arc<Stub>>,
        headers: HeaderMap,
        AxumPath((_id, name)): AxumPath<(String, String)>,
        body: Bytes,
    ) -> Response {
        stub.hits.fetch_add(1, Ordering::SeqCst);
        if !authorized(&headers, &stub.token) {
            return HttpStatus::FORBIDDEN.into_response();
        }
        let sha256 = hex(&Sha256::digest(&body));
        let bytes = body.len() as i64;
        stub.outputs_lock().push((name.clone(), body.to_vec()));
        (
            HttpStatus::CREATED,
            axum::Json(json!({ "name": name, "sha256": sha256, "bytes": bytes })),
        )
            .into_response()
    }

    async fn stub_result(
        State(stub): State<Arc<Stub>>,
        headers: HeaderMap,
        axum::Json(value): axum::Json<JsonValue>,
    ) -> HttpStatus {
        stub.hits.fetch_add(1, Ordering::SeqCst);
        if !authorized(&headers, &stub.token) {
            return HttpStatus::FORBIDDEN;
        }
        stub.results_lock().push(value);
        HttpStatus::ACCEPTED
    }

    impl Stub {
        fn outputs_lock(&self) -> std::sync::MutexGuard<'_, Vec<(String, Vec<u8>)>> {
            self.outputs.lock().expect("outputs")
        }

        fn results_lock(&self) -> std::sync::MutexGuard<'_, Vec<JsonValue>> {
            self.results.lock().expect("results")
        }
    }

    async fn start_stub(stub: Arc<Stub>) -> (String, tokio::task::JoinHandle<()>) {
        let app = Router::new()
            .route("/api/jobs/{id}/inputs/{name}", get(stub_input))
            .route("/api/jobs/{id}/outputs/{name}", post(stub_output))
            .route("/api/jobs/{id}/result", post(stub_result))
            .with_state(stub);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind an ephemeral port");
        let address = listener.local_addr().expect("local address");
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        (format!("http://{address}"), server)
    }

    fn instruction(base: &str, deadline_unix_ms: i64, input_sha256: String) -> Instruction {
        Instruction {
            schema: CONTRACT.to_owned(),
            job_id: Uuid::now_v7(),
            tenant: Uuid::now_v7(),
            kind: KIND.to_owned(),
            attempt: 1,
            correlation_id: Uuid::now_v7(),
            deadline_unix_ms,
            params: json!({"html": "<p>hi</p>"}),
            inputs: vec![Input {
                name: "source".to_owned(),
                document_id: Uuid::now_v7(),
                sha256: input_sha256,
                content_type: "text/plain".to_owned(),
                bytes: 5,
            }],
            outputs: vec![Output {
                name: "page.png".to_owned(),
                content_type: "image/png".to_owned(),
                max_bytes: 1024 * 1024,
            }],
            limits: Limits {
                cpu_millis: 500,
                memory_bytes: 512 * 1024 * 1024,
                wall_secs: 60,
                max_output_bytes: 1024 * 1024,
            },
            conveyor_base_url: base.to_owned(),
            trace_id: "trace-1".to_owned(),
        }
    }

    fn config() -> Config {
        let root = std::env::temp_dir().join(format!("conveyor-media-runner-{}", Uuid::now_v7()));
        std::fs::create_dir_all(&root).expect("temp root");
        Config {
            service: "conveyor-media-test".to_owned(),
            work_dir: root.join("work"),
            out_dir: root.join("out"),
        }
    }

    fn renders(
        kind: String,
        _params: JsonValue,
        config: Config,
    ) -> BoxFuture<'static, Result<JsonValue, CapabilityError>> {
        Box::pin(async move {
            assert_eq!(kind, KIND);
            std::fs::create_dir_all(&config.out_dir).expect("out dir");
            std::fs::write(config.out_dir.join("page.png"), b"rendered").expect("write output");
            Ok(json!({"ok": true}))
        })
    }

    /// A dispatch that must never be reached: any precheck failure or fetch failure that ran the
    /// capability would panic the test rather than return a tidy result.
    fn must_not_run(
        _kind: String,
        _params: JsonValue,
        _config: Config,
    ) -> BoxFuture<'static, Result<JsonValue, CapabilityError>> {
        Box::pin(async move { panic!("the capability must not run") })
    }

    const KINDS: &[&str] = &[KIND];

    fn table(
        call: fn(
            String,
            JsonValue,
            Config,
        ) -> BoxFuture<'static, Result<JsonValue, CapabilityError>>,
    ) -> Dispatch {
        Dispatch { kinds: KINDS, call }
    }

    fn future_deadline() -> i64 {
        Utc::now().timestamp_millis() + 60_000
    }

    #[tokio::test]
    async fn a_declared_input_is_fetched_and_a_rendered_output_is_reported() {
        let stub = Arc::new(Stub::new(b"hello"));
        let (base, server) = start_stub(stub.clone()).await;
        let config = config();
        let instruction = instruction(&base, future_deadline(), hex(&Sha256::digest(b"hello")));

        let result = execute_with(
            &instruction,
            &config,
            table(renders),
            "test-token",
            Utc::now(),
        )
        .await;

        assert_eq!(result.status, Status::Succeeded, "{:?}", result.error);
        assert_eq!(result.artefacts.len(), 1);
        assert_eq!(result.artefacts[0].name, "page.png");
        assert_eq!(
            result.artefacts[0].sha256,
            hex(&Sha256::digest(b"rendered"))
        );
        assert_eq!(result.artefacts[0].bytes, 8);
        assert_eq!(result.artefacts[0].content_type, "image/png");

        assert_eq!(
            std::fs::read(config.work_dir.join("inputs/source")).expect("downloaded input"),
            b"hello"
        );
        {
            let outputs = stub.outputs_lock();
            assert_eq!(outputs.len(), 1);
            assert_eq!(outputs[0].0, "page.png");
            assert_eq!(outputs[0].1, b"rendered");
        }
        {
            let results = stub.results_lock();
            assert_eq!(results.len(), 1);
            assert_eq!(results[0]["schema"], json!(CONTRACT));
            assert_eq!(results[0]["status"], json!("succeeded"));
            assert_eq!(results[0]["job_id"], json!(instruction.job_id));
            assert_eq!(
                results[0]["artefacts"][0]["sha256"],
                json!(hex(&Sha256::digest(b"rendered")))
            );
        }
        server.abort();
    }

    #[tokio::test]
    async fn an_input_that_does_not_match_its_declared_hash_fails_as_input() {
        let stub = Arc::new(Stub::new(b"hello"));
        let (base, server) = start_stub(stub.clone()).await;
        let config = config();
        let instruction = instruction(&base, future_deadline(), hex(&Sha256::digest(b"other")));

        let result = execute_with(
            &instruction,
            &config,
            table(must_not_run),
            "test-token",
            Utc::now(),
        )
        .await;

        assert_eq!(result.status, Status::Failed);
        let error = result.error.expect("a failed result carries its error");
        assert_eq!(error.class, ErrorClass::Input);
        assert_eq!(error.code, "input_hash_mismatch");
        assert!(stub.outputs_lock().is_empty(), "nothing may be uploaded");
        let results = stub.results_lock();
        assert_eq!(results.len(), 1, "the failure is still reported");
        assert_eq!(results[0]["status"], json!("failed"));
        assert_eq!(results[0]["error"]["class"], json!("input"));
        assert_eq!(results[0]["error"]["code"], json!("input_hash_mismatch"));
        assert_eq!(results[0]["artefacts"], json!([]));
        server.abort();
    }

    #[tokio::test]
    async fn a_past_deadline_fails_as_input_without_any_http_call() {
        let stub = Arc::new(Stub::new(b"hello"));
        let (base, server) = start_stub(stub.clone()).await;
        let config = config();
        let instruction = instruction(
            &base,
            Utc::now().timestamp_millis() - 1,
            hex(&Sha256::digest(b"hello")),
        );

        let result = execute(instruction, &config, table(must_not_run)).await;

        assert_eq!(result.status, Status::Failed);
        let error = result.error.expect("a failed result carries its error");
        assert_eq!(error.class, ErrorClass::Input);
        assert_eq!(error.code, "deadline_passed");
        assert_eq!(stub.hits(), 0, "no request may leave the pod");
        server.abort();
    }

    #[tokio::test]
    async fn an_unknown_kind_is_refused_before_any_http_call() {
        let stub = Arc::new(Stub::new(b"hello"));
        let (base, server) = start_stub(stub.clone()).await;
        let config = config();
        let mut instruction = instruction(&base, future_deadline(), hex(&Sha256::digest(b"hello")));
        instruction.kind = "media.not_implemented".to_owned();

        let result = execute(instruction, &config, table(must_not_run)).await;

        assert_eq!(result.status, Status::Failed);
        let error = result.error.expect("a failed result carries its error");
        assert_eq!(error.class, ErrorClass::Input);
        assert_eq!(error.code, "unknown_kind");
        assert_eq!(stub.hits(), 0, "no request may leave the pod");
        server.abort();
    }

    #[test]
    fn the_capability_error_vocabulary_maps_to_the_contract() {
        let refused = classify(&CapabilityError::refused(
            "wait_until_not_supported",
            "no headless equivalent",
        ));
        assert_eq!(refused.class, ErrorClass::Input);
        assert_eq!(refused.code, "wait_until_not_supported");

        let invalid = classify(&CapabilityError::Invalid("bad params".to_owned()));
        assert_eq!(invalid.class, ErrorClass::Input);
        assert_eq!(invalid.code, "invalid_params");

        let unknown = classify(&CapabilityError::UnknownKind("x.y".to_owned()));
        assert_eq!(unknown.class, ErrorClass::Input);
        assert_eq!(unknown.code, "unknown_kind");

        let missing = classify(&CapabilityError::ToolMissing {
            tool: "ffmpeg".to_owned(),
            source: std::io::Error::from(std::io::ErrorKind::NotFound),
        });
        assert_eq!(missing.class, ErrorClass::Permanent);
        assert_eq!(missing.code, "tool_missing");

        let failed = classify(&CapabilityError::Failed("the browser died".to_owned()));
        assert_eq!(failed.class, ErrorClass::Transient);
        assert_eq!(failed.code, "capability_failed");
    }
}
