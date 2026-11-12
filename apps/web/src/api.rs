//! The action client: one path for every read and every write.
//!
//! Writes go through `POST /api/actions/{name}/invocations` with `{"input": …}`; reads use
//! the literal GET routes (`/api/boards/{id}/cards`, `/api/cards/{id}`, …). The manifest from
//! `GET /api/actions` is the UI's contract: a control whose action is absent is not rendered.
//!
//! Built on `web_sys::fetch`, not `EventSource`/`gloo-net`, because every call must carry the
//! bearer token (ADR 0001). The API has no CORS layer: the app is same-origin behind the
//! ingress in production and behind `dev-server.mjs` locally.

use serde::Deserialize;
use serde_json::Value;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{Headers, Request, RequestCredentials, RequestInit, RequestMode, Response};

/// The caller's credential and selected tenant, copied per request.
#[derive(Clone, Debug, Default)]
pub struct Client {
    pub token: Option<String>,
    pub tenant: Option<String>,
}

/// RFC 9457 problem document, as the API emits it.
#[allow(dead_code)]
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Problem {
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub status: Option<u16>,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub instance: Option<String>,
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub errors: Vec<FieldError>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Default, Deserialize)]
pub struct FieldError {
    #[serde(default)]
    pub field: String,
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub detail: String,
}

/// A failed call, already classified by the status the API returned.
#[derive(Clone, Debug)]
pub enum ApiError {
    Unauthenticated,
    Forbidden(String),
    NotFound(String),
    Conflict(String),
    /// 412: the object moved. The screen refetches rather than guessing a new version.
    Precondition(String),
    Invalid {
        detail: String,
        errors: Vec<FieldError>,
    },
    Server {
        status: u16,
        detail: String,
        request_id: Option<String>,
    },
    Network(String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unauthenticated => write!(f, "not signed in"),
            Self::Forbidden(detail) => write!(f, "not permitted: {detail}"),
            Self::NotFound(detail) => write!(f, "not found: {detail}"),
            Self::Conflict(detail) => write!(f, "conflict: {detail}"),
            Self::Precondition(detail) => write!(f, "the object changed: {detail}"),
            Self::Invalid { detail, errors } => {
                write!(f, "{detail}")?;
                for error in errors {
                    write!(f, " · {}: {}", error.field, error.detail)?;
                }
                Ok(())
            }
            Self::Server {
                status,
                detail,
                request_id,
            } => match request_id {
                Some(id) => write!(f, "server {status}: {detail} (request {id})"),
                None => write!(f, "server {status}: {detail}"),
            },
            Self::Network(detail) => write!(f, "network: {detail}"),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct InvokeOptions {
    /// The object's `version`; sent as `If-Match`.
    pub if_match: Option<i64>,
    /// A replay-safe key; sent as `Idempotency-Key`.
    pub idempotency_key: Option<String>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Default)]
pub struct Invocation {
    pub output: Value,
    pub request_id: Option<String>,
    pub duration_ms: Option<f64>,
    pub etag: Option<String>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Deserialize)]
pub struct Manifest {
    pub actions: Vec<ActionEntry>,
    #[serde(default)]
    pub generated_at: Option<String>,
}

/// One manifest row. Deliberately the exact server field set (S04 §5.1): no input schema,
/// no `effect`, no precondition flag — those live on `actions.describe`.
#[allow(dead_code)]
#[derive(Clone, Debug, Deserialize)]
pub struct ActionEntry {
    pub name: String,
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub spec_hash: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub idempotent: bool,
    #[serde(default)]
    pub timeout_ms: u32,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Deserialize)]
pub struct Page<T> {
    #[serde(default = "Vec::new")]
    pub items: Vec<T>,
    #[serde(default)]
    pub next_cursor: Option<String>,
}

struct Raw {
    status: u16,
    text: String,
    etag: Option<String>,
}

impl Client {
    pub async fn get(&self, path: &str) -> Result<Value, ApiError> {
        let raw = self.send("GET", path, None, &[]).await?;
        if !(200..300).contains(&raw.status) {
            return Err(problem(raw.status, &raw.text));
        }
        parse_json(&raw.text)
    }

    /// `POST path` with a JSON body — the Platform routes and the billing read.
    ///
    /// These are not actions: provisioning is a multi-step authenticated orchestration and
    /// the billing read is a tenant-admin projection, so neither fits an invocation.
    pub async fn post(&self, path: &str, body: Value) -> Result<Value, ApiError> {
        let raw = self
            .send(
                "POST",
                path,
                Some(body.to_string()),
                &[("content-type", "application/json".to_owned())],
            )
            .await?;
        if !(200..300).contains(&raw.status) {
            return Err(problem(raw.status, &raw.text));
        }
        if raw.text.trim().is_empty() {
            return Ok(Value::Null);
        }
        parse_json(&raw.text)
    }

    /// `GET /api/actions` — the role-filtered manifest.
    pub async fn manifest(&self) -> Result<Manifest, ApiError> {
        let raw = self.send("GET", "/api/actions", None, &[]).await?;
        if !(200..300).contains(&raw.status) {
            return Err(problem(raw.status, &raw.text));
        }
        serde_json::from_str(&raw.text)
            .map_err(|error| ApiError::Network(format!("manifest: {error}")))
    }

    /// `POST /api/actions/{name}/invocations`.
    pub async fn invoke(
        &self,
        name: &str,
        input: Value,
        options: InvokeOptions,
    ) -> Result<Invocation, ApiError> {
        let body = serde_json::json!({ "input": input }).to_string();
        let mut headers: Vec<(&str, String)> =
            vec![("content-type", "application/json".to_owned())];
        if let Some(version) = options.if_match {
            headers.push(("if-match", version.to_string()));
        }
        if let Some(key) = options.idempotency_key {
            headers.push(("idempotency-key", key));
        }
        let path = format!("/api/actions/{name}/invocations");
        let raw = self.send("POST", &path, Some(body), &headers).await?;
        if !(200..300).contains(&raw.status) {
            return Err(problem(raw.status, &raw.text));
        }
        if raw.text.trim().is_empty() {
            return Ok(Invocation {
                output: Value::Null,
                etag: raw.etag,
                ..Invocation::default()
            });
        }
        let envelope: Value = parse_json(&raw.text)?;
        Ok(Invocation {
            output: envelope.get("output").cloned().unwrap_or(Value::Null),
            request_id: envelope
                .get("request_id")
                .and_then(Value::as_str)
                .map(str::to_owned),
            duration_ms: envelope.get("duration_ms").and_then(Value::as_f64),
            etag: raw.etag,
        })
    }

    async fn send(
        &self,
        method: &str,
        path: &str,
        body: Option<String>,
        extra: &[(&str, String)],
    ) -> Result<Raw, ApiError> {
        let window = web_sys::window().ok_or_else(|| ApiError::Network("no window".into()))?;
        let headers = Headers::new().map_err(js)?;
        headers.set("accept", "application/json").map_err(js)?;
        if let Some(token) = &self.token {
            headers
                .set("authorization", &format!("Bearer {token}"))
                .map_err(js)?;
        }
        if let Some(tenant) = &self.tenant {
            headers.set("x-active-tenant", tenant).map_err(js)?;
        }
        for (name, value) in extra {
            headers.set(name, value).map_err(js)?;
        }

        let init = RequestInit::new();
        init.set_method(method);
        init.set_headers(&headers);
        init.set_mode(RequestMode::SameOrigin);
        init.set_credentials(RequestCredentials::SameOrigin);
        if let Some(body) = body {
            init.set_body(&JsValue::from_str(&body));
        }

        let request = Request::new_with_str_and_init(path, &init).map_err(js)?;
        let response = JsFuture::from(window.fetch_with_request(&request))
            .await
            .map_err(js)?;
        let response: Response = response
            .dyn_into()
            .map_err(|_| ApiError::Network("fetch did not return a Response".into()))?;
        let status = response.status();
        let text = JsFuture::from(response.text().map_err(js)?)
            .await
            .map_err(js)?;
        let text = text.as_string().unwrap_or_default();
        let etag = response.headers().get("etag").ok().flatten();
        Ok(Raw { status, text, etag })
    }
}

/// Map a non-2xx problem body. Kept public for the SSE path, which has no `Raw`.
pub fn problem(status: u16, text: &str) -> ApiError {
    let problem: Problem = serde_json::from_str(text).unwrap_or_default();
    let detail = problem
        .detail
        .clone()
        .or_else(|| problem.title.clone())
        .unwrap_or_else(|| format!("HTTP {status}"));
    match status {
        401 => ApiError::Unauthenticated,
        403 => ApiError::Forbidden(detail),
        404 => ApiError::NotFound(detail),
        409 => ApiError::Conflict(detail),
        412 => ApiError::Precondition(detail),
        422 => ApiError::Invalid {
            detail,
            errors: problem.errors,
        },
        _ => ApiError::Server {
            status,
            detail,
            request_id: problem.request_id,
        },
    }
}

fn parse_json(text: &str) -> Result<Value, ApiError> {
    serde_json::from_str(text).map_err(|error| ApiError::Network(format!("response: {error}")))
}

fn js(value: JsValue) -> ApiError {
    ApiError::Network(value.as_string().unwrap_or_else(|| format!("{value:?}")))
}
