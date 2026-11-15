//! P2.3 — an artefact lands under conveyor's tenant-scoped key with a content hash, and no store
//! credential reaches the job.
//!
//! The pod cannot choose where its bytes live: it names a **declared output**, and conveyor derives
//! the object key from the tenant and attempt in the job token. This test holds the executor to
//! that by watching exactly one request: `POST /api/jobs/{job_id}/outputs/{name}`, with the declared
//! content type, and a result whose sha256 is the one the route returned.
//!
//! The credential half is structural. `Config` has three fields and none of them can hold a store
//! credential; the crate has no dependency on the object-store adapter. The exhaustive destructuring
//! below fails to compile the day a fourth field appears, which is the point.

use std::sync::{Arc, Mutex};

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use conveyor_domain::runner::{Instruction, Limits, Output, Status};
use conveyor_media::common::entry::Dispatch;
use conveyor_media::common::runner::execute_with;
use conveyor_media::common::{CapabilityError, Config};
use futures_util::future::BoxFuture;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// One recorded upload: the route path, the declared content type and the bytes.
type Upload = (String, String, Vec<u8>);

#[derive(Default, Clone)]
struct Stub {
    uploads: Arc<Mutex<Vec<Upload>>>,
}

async fn upload(
    State(stub): State<Stub>,
    Path((id, name)): Path<(Uuid, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let content_type = headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let sha = Sha256::digest(&body);
    let sha256: String = sha.iter().map(|byte| format!("{byte:02x}")).collect();
    stub.uploads.lock().expect("stub lock").push((
        format!("/api/jobs/{id}/outputs/{name}"),
        content_type,
        body.to_vec(),
    ));
    (
        StatusCode::CREATED,
        Json(json!({"name": name, "sha256": sha256, "bytes": body.len()})),
    )
}

async fn result() -> impl IntoResponse {
    StatusCode::ACCEPTED
}

/// The capability the stub instruction runs: it writes one file into the output directory.
fn write_master(
    _kind: String,
    _params: Value,
    config: Config,
) -> BoxFuture<'static, Result<Value, CapabilityError>> {
    Box::pin(async move {
        std::fs::create_dir_all(&config.out_dir)
            .map_err(|error| CapabilityError::Failed(error.to_string()))?;
        std::fs::write(config.out_dir.join("master.mp4"), b"not really an mp4")
            .map_err(|error| CapabilityError::Failed(error.to_string()))?;
        Ok(json!({"bytes": 16}))
    })
}

fn instruction(base: String, job_id: Uuid) -> Instruction {
    Instruction {
        schema: "runner/v1".to_string(),
        job_id,
        tenant: Uuid::from_u128(11),
        kind: "media.compile_video".to_string(),
        attempt: 1,
        correlation_id: Uuid::from_u128(13),
        deadline_unix_ms: chrono::Utc::now().timestamp_millis() + 60_000,
        params: json!({}),
        inputs: Vec::new(),
        outputs: vec![Output {
            name: "master.mp4".to_string(),
            content_type: "video/mp4".to_string(),
            max_bytes: 1024,
        }],
        limits: Limits {
            cpu_millis: 500,
            memory_bytes: 256 * 1024 * 1024,
            wall_secs: 60,
            max_output_bytes: 1024,
        },
        conveyor_base_url: base,
        trace_id: "trace-p23".to_string(),
    }
}

#[tokio::test]
async fn artefact_lands_under_tenant_prefix() {
    let stub = Stub::default();
    let app = Router::new()
        .route("/api/jobs/{id}/outputs/{name}", post(upload))
        .route("/api/jobs/{id}/result", post(result))
        .with_state(stub.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind the stub");
    let base = format!("http://{}", listener.local_addr().expect("addr"));
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let job_id = Uuid::from_u128(7);
    let dir = std::env::temp_dir().join(format!("conveyor-p23-{}", std::process::id()));
    let config = Config {
        service: "conveyor-media-ffmpeg".to_string(),
        work_dir: dir.clone(),
        out_dir: dir.clone(),
    };
    let dispatch = Dispatch {
        kinds: &["media.compile_video"],
        call: write_master,
    };

    let result = execute_with(
        &instruction(base, job_id),
        &config,
        dispatch,
        "the-job-token",
        chrono::Utc::now(),
    )
    .await;
    server.abort();

    assert_eq!(result.status, Status::Succeeded, "{result:?}");
    assert_eq!(result.artefacts.len(), 1);
    assert_eq!(result.artefacts[0].name, "master.mp4");
    assert_eq!(result.artefacts[0].content_type, "video/mp4");
    let expected: String = Sha256::digest(b"not really an mp4")
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    assert_eq!(
        result.artefacts[0].sha256, expected,
        "the reported hash is the bytes conveyor received"
    );

    // One upload, to the job route — and no key the pod invented: the path carries the job id the
    // token was minted for, and the name the instruction declared. Nothing else.
    let uploads = stub.uploads.lock().expect("stub lock");
    assert_eq!(uploads.len(), 1, "one declared output, one upload");
    let (path, content_type, bytes) = &uploads[0];
    assert_eq!(path, &format!("/api/jobs/{job_id}/outputs/master.mp4"));
    assert_eq!(content_type, "video/mp4");
    assert_eq!(bytes, b"not really an mp4");
}

/// The executor's configuration cannot carry a store credential — the type has three fields and
/// this destructuring is exhaustive, so a fourth fails to compile rather than shipping quietly.
#[test]
fn no_store_credential_can_reach_the_job() {
    let config = Config::from_env("test");
    let Config {
        service: _,
        work_dir: _,
        out_dir: _,
    } = config;
}
