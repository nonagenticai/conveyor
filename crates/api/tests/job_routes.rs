//! #144 P3: the pod-side byte routes, fenced by a job token AND the job row.
//!
//! The properties this file exists for:
//!
//! 1. **A verified token is not enough.** A finished job, a foreign tenant and a stale attempt all
//!    answer the same 403, even when the MAC is intact and the expiry has not passed.
//! 2. **The manifest is the authority.** A name the instruction did not declare is refused, and an
//!    artefact's sha256 is the one conveyor computed from the bytes it received.
//! 3. **The pod never finishes the job.** `POST /result` records; only the poll handler, holding
//!    the lease, may finish the row.
//!
//! The harness is `crates/actions/tests/support` by `#[path]`: one ephemeral Postgres and object
//! store per test binary, the REAL server over HTTP, and `dev:` authentication for the setup calls
//! that create the document a job reads.

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use chrono::{Duration as ChronoDuration, Utc};
use conveyor_api::Config;
use conveyor_auth::job_token::{self, JobClaims, JobTokenKey};
use conveyor_domain::TenantId;
use conveyor_store as store;
use conveyor_store::runner::NewRun;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// The key this test process configures, in hex. The same value goes to the server's `Config` and
/// to the minting side, which is exactly the relationship a deployment has.
const JOB_KEY_HEX: &str = "0f1e2d3c4b5a69788796a5b4c3d2e1f00f1e2d3c4b5a69788796a5b4c3d2e1f0";

fn key() -> JobTokenKey {
    JobTokenKey::parse(JOB_KEY_HEX).expect("a 32-byte key")
}

fn config(pg: &support::Pg) -> Config {
    let runtime_url = pg.runtime_url.clone();
    let owner_url = pg.owner_url.clone();
    Config::from_lookup(move |key| match key {
        "DATABASE_URL" | "DATABASE_LISTEN_URL" => Some(runtime_url.clone()),
        "DATABASE_MIGRATE_URL" => Some(owner_url.clone()),
        "CONVEYOR_DEV_AUTH" => Some("1".into()),
        "BIND_ADDR" => Some("127.0.0.1:0".into()),
        "APP_KEY" => Some("conveyor".into()),
        "CONVEYOR_JOB_TOKEN_KEY" => Some(JOB_KEY_HEX.into()),
        _ => None,
    })
    .expect("test config")
}

fn hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Everything one live dispatch needs on the server side.
struct Harness {
    /// Held for the whole test: dropping it tears the database down under the running server.
    _pg: support::Pg,
    running: support::Running,
    tenant: TenantId,
    job_id: Uuid,
    input_bytes: Vec<u8>,
}

/// Start the server, create the input document through the served action, then enqueue, claim and
/// begin one attempt — the same rows `runner.dispatch` writes, without a Kubernetes API.
async fn harness() -> Harness {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start_with_config(config(&pg)).await;

    let input_bytes = b"the contract, in bytes".to_vec();
    let (status, created) = running
        .invoke(
            "documents.create",
            "dev:alice",
            json!({
                "title": "An input",
                "content_type": "text/plain",
                "content_base64": base64::Engine::encode(
                    &base64::engine::general_purpose::STANDARD,
                    &input_bytes,
                ),
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "documents.create: {created}");
    let document_id = created["output"]["id"]
        .as_str()
        .expect("the created document's id")
        .parse::<Uuid>()
        .expect("a uuid");

    let job_id = store::jobs::enqueue(
        &pg.runtime,
        seeded.tenant,
        "media.compile",
        "job-routes",
        json!({}),
        None,
        3,
    )
    .await
    .expect("enqueue")
    .expect("a fresh dedupe key");
    let job = store::jobs::claim(&pg.runtime, seeded.tenant, 60)
        .await
        .expect("claim")
        .expect("the job is due");

    let instruction = json!({
        "schema": "runner/v1",
        "job_id": job_id,
        "tenant": seeded.tenant_id,
        "kind": "media.compile",
        "attempt": job.attempts,
        "correlation_id": Uuid::now_v7(),
        "deadline_unix_ms": (Utc::now() + ChronoDuration::minutes(10)).timestamp_millis(),
        "params": {},
        "inputs": [{
            "name": "source.txt",
            "document_id": document_id,
            "sha256": hex(&input_bytes),
            "content_type": "text/plain",
            "bytes": input_bytes.len() as i64,
        }],
        "outputs": [{ "name": "report.txt", "content_type": "text/plain", "max_bytes": 1024 }],
        "limits": {
            "cpu_millis": 500,
            "memory_bytes": 268_435_456,
            "wall_secs": 600,
            "max_output_bytes": 1024,
        },
        "conveyor_base_url": "http://conveyor.invalid",
        "trace_id": "test-trace",
    });
    store::runner::begin(
        &pg.runtime,
        seeded.tenant,
        &NewRun {
            job_id,
            attempt: job.attempts,
            image: "registry.invalid/conveyor-media-ffmpeg:test",
            namespace: "conveyor-runners",
            job_name: "run-test",
            instruction,
        },
    )
    .await
    .expect("begin")
    .expect("a new attempt inserts");

    Harness {
        _pg: pg,
        running,
        tenant: seeded.tenant,
        job_id,
        input_bytes,
    }
}

fn token_for(harness: &Harness, tenant: TenantId, ttl: ChronoDuration) -> String {
    job_token::mint(
        &key(),
        &JobClaims {
            job_id: harness.job_id,
            tenant,
            attempt: 1,
            expires_at: job_token::expires_at(Utc::now(), ttl.to_std().expect("positive")),
        },
    )
}

async fn get(
    harness: &Harness,
    path: &str,
    token: &str,
) -> (u16, Option<String>, Vec<u8>) {
    let response = reqwest::Client::new()
        .get(format!("{}{path}", harness.running.base()))
        .header("authorization", format!("Bearer {token}"))
        .send()
        .await
        .expect("the route answers");
    let status = response.status().as_u16();
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    (status, content_type, response.bytes().await.unwrap_or_default().to_vec())
}

async fn post_bytes(harness: &Harness, path: &str, token: &str, body: Vec<u8>) -> (u16, Value) {
    let response = reqwest::Client::new()
        .post(format!("{}{path}", harness.running.base()))
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/octet-stream")
        .body(body)
        .send()
        .await
        .expect("the route answers");
    let status = response.status().as_u16();
    let value = response.json::<Value>().await.unwrap_or(Value::Null);
    (status, value)
}

#[tokio::test(flavor = "multi_thread")]
async fn the_input_bytes_are_the_document_the_instruction_named() {
    let harness = harness().await;
    let token = token_for(&harness, harness.tenant, ChronoDuration::minutes(5));

    let (status, content_type, bytes) = get(
        &harness,
        &format!("/api/jobs/{}/inputs/source.txt", harness.job_id),
        &token,
    )
    .await;
    assert_eq!(
        status,
        200,
        "a declared input is served: {}",
        String::from_utf8_lossy(&bytes)
    );
    assert_eq!(content_type.as_deref(), Some("text/plain"));
    assert_eq!(bytes, harness.input_bytes, "exactly the document's bytes");
}

#[tokio::test(flavor = "multi_thread")]
async fn foreign_tenant_job_is_refused() {
    let harness = harness().await;
    let other = TenantId::new();
    let token = token_for(&harness, other, ChronoDuration::minutes(5));

    let (status, _, _) = get(
        &harness,
        &format!("/api/jobs/{}/inputs/source.txt", harness.job_id),
        &token,
    )
    .await;
    assert_eq!(
        status, 403,
        "a token minted for another tenant reads nothing, though its MAC is valid"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn token_is_refused_after_the_job_finishes() {
    let harness = harness().await;
    let token = token_for(&harness, harness.tenant, ChronoDuration::minutes(5));

    // The poll side finishes the row while the pod's token is still unexpired.
    let job = store::jobs::get(&harness.running.state.pool, harness.tenant, harness.job_id)
        .await
        .expect("read the job")
        .expect("the job exists");
    let finished = store::jobs::finish_succeeded(
        &harness.running.state.pool,
        harness.tenant,
        harness.job_id,
        job.lease_token.expect("a live lease"),
    )
    .await
    .expect("finish");
    assert!(finished, "the lease holder may finish");

    let (status, _, _) = get(
        &harness,
        &format!("/api/jobs/{}/inputs/source.txt", harness.job_id),
        &token,
    )
    .await;
    assert_eq!(status, 403, "the row fence, not the MAC, refuses it");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_expired_token_is_refused() {
    let harness = harness().await;
    let expired = job_token::mint(
        &key(),
        &JobClaims {
            job_id: harness.job_id,
            tenant: harness.tenant,
            attempt: 1,
            expires_at: Utc::now() - ChronoDuration::seconds(1),
        },
    );
    let (status, _, _) = get(
        &harness,
        &format!("/api/jobs/{}/inputs/source.txt", harness.job_id),
        &expired,
    )
    .await;
    assert_eq!(status, 403);
}

#[tokio::test(flavor = "multi_thread")]
async fn undeclared_output_key_is_refused() {
    let harness = harness().await;
    let token = token_for(&harness, harness.tenant, ChronoDuration::minutes(5));
    let (status, body) = post_bytes(
        &harness,
        &format!("/api/jobs/{}/outputs/secret.mp4", harness.job_id),
        &token,
        b"not declared".to_vec(),
    )
    .await;
    assert_eq!(
        status, 422,
        "the declared output set is closed: {body}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_output_is_stored_under_its_computed_hash() {
    let harness = harness().await;
    let token = token_for(&harness, harness.tenant, ChronoDuration::minutes(5));
    let path = format!("/api/jobs/{}/outputs/report.txt", harness.job_id);
    let body = b"the report".to_vec();

    let (status, stored) = post_bytes(&harness, &path, &token, body.clone()).await;
    assert_eq!(status, 201, "{stored}");
    assert_eq!(
        stored["sha256"].as_str(),
        Some(hex(&body).as_str()),
        "the hash is conveyor's, not the pod's claim"
    );

    // The same bytes again are an idempotent retry; different bytes under the same name are not.
    let (status, _) = post_bytes(&harness, &path, &token, body.clone()).await;
    assert_eq!(status, 201);
    let (status, _) = post_bytes(&harness, &path, &token, b"different".to_vec()).await;
    assert_eq!(status, 409, "a name may not silently change its bytes");

    let runs = store::runner::latest(
        &harness.running.state.pool,
        harness.tenant,
        harness.job_id,
    )
    .await
    .expect("read the attempt")
    .expect("the attempt exists");
    let artefacts = store::runner::artefacts(
        &harness.running.state.pool,
        harness.tenant,
        runs.id,
    )
    .await
    .expect("list artefacts");
    assert_eq!(artefacts.len(), 1);
    assert_eq!(artefacts[0].sha256, hex(&body));
    assert!(
        artefacts[0].object_key.starts_with(&format!("t/{}/jobs/", harness.tenant.as_uuid())),
        "the key is tenant-prefixed: {}",
        artefacts[0].object_key
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_result_is_recorded_without_finishing_the_job() {
    let harness = harness().await;
    let token = token_for(&harness, harness.tenant, ChronoDuration::minutes(5));

    // A successful result must carry every declared output, so the pod uploads first.
    let bytes = b"the report".to_vec();
    let (status, stored) = post_bytes(
        &harness,
        &format!("/api/jobs/{}/outputs/report.txt", harness.job_id),
        &token,
        bytes.clone(),
    )
    .await;
    assert_eq!(status, 201, "{stored}");

    let started = Utc::now();
    let result = json!({
        "schema": "runner/v1",
        "job_id": harness.job_id,
        "attempt": 1,
        "status": "succeeded",
        "error": null,
        "artefacts": [{
            "name": "report.txt",
            "sha256": hex(&bytes),
            "bytes": bytes.len() as i64,
            "content_type": "text/plain",
        }],
        "metrics": {},
        "started_at": started.to_rfc3339(),
        "finished_at": (started + ChronoDuration::seconds(2)).to_rfc3339(),
    });

    let response = reqwest::Client::new()
        .post(format!(
            "{}/api/jobs/{}/result",
            harness.running.base(),
            harness.job_id
        ))
        .header("authorization", format!("Bearer {token}"))
        .json(&result)
        .send()
        .await
        .expect("the route answers");
    let status = response.status().as_u16();
    let body = response.text().await.unwrap_or_default();
    assert_eq!(status, 202, "{body}");

    let job = store::jobs::get(&harness.running.state.pool, harness.tenant, harness.job_id)
        .await
        .expect("read the job")
        .expect("the job exists");
    assert_eq!(
        job.status, "running",
        "the pod recorded a verdict; it did not finish the job"
    );
    let run = store::runner::latest(
        &harness.running.state.pool,
        harness.tenant,
        harness.job_id,
    )
    .await
    .expect("read the attempt")
    .expect("the attempt exists");
    assert_eq!(run.status, "reported");
    assert!(run.result.is_some());
}
