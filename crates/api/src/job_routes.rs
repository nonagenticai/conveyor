//! The job byte routes: how a dispatched pod reads its inputs and reports back (#144, ADR 0007
//! §Decision 4).
//!
//! | route | what the pod does |
//! |---|---|
//! | `GET  /api/jobs/{id}/inputs/{name}` | read one declared input's bytes |
//! | `PUT  /api/jobs/{id}/progress` | post a bounded progress object |
//! | `POST /api/jobs/{id}/outputs/{name}` | upload one declared output |
//! | `POST /api/jobs/{id}/result` | report the run's verdict |
//!
//! **No session, no licence, no presigned URL.** A pod holds one short-lived job token
//! ([`conveyor_auth::job_token`]) and nothing else; every byte it receives or sends crosses this
//! service, which is the only authorizer. The store credential never leaves conveyor.
//!
//! **The token alone is not enough.** A verified token proves the claims were minted here, not
//! that the job is still in that state, so every route is fenced a second time on the row: the job
//! must still be `running`, `job.attempts` must equal the token's attempt, and the lease must be
//! live. A finished, superseded or cancelled job therefore answers 403 to a token that is still
//! cryptographically valid — the "token is refused after the job finishes" acceptance.
//!
//! Every refusal on the credential path answers the SAME 403 body, so the route is not an oracle
//! for which check failed.
//!
//! **The pod never finishes the job.** `POST /result` records the verdict on the attempt row; the
//! poll handler, which holds the lease, re-validates it and finishes the job. One finish authority.

use axum::body::{Body, Bytes};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use conveyor_auth::job_token::{self, JobClaims};
use conveyor_domain::runner::{Instruction, RunResult};
use conveyor_store as store;
use conveyor_store::runner::{ArtefactRow, Inserted, Run};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;

/// The longest progress document a run may post. Progress is a status line, not a data channel.
pub const MAX_PROGRESS_BYTES: usize = 4 * 1024;

/// The one body every credential refusal returns.
fn refused() -> ApiError {
    ApiError::new(
        StatusCode::FORBIDDEN,
        "job_token_refused",
        "the job token is not valid for this job",
    )
}

/// What a fenced request established: the job row, the live attempt, and the instruction both
/// agree with.
struct Fenced {
    claims: JobClaims,
    run: Run,
    instruction: Instruction,
}

fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|token| !token.is_empty())
}

/// The whole fence, in one place: token → claims → job row → live attempt → instruction.
///
/// Any failure is the same 403, except a missing key, which is a 503: an unconfigured deployment
/// is an operator fault and must not look like a rejected credential.
async fn fence(state: &AppState, headers: &HeaderMap, job_id: Uuid) -> Result<Fenced, ApiError> {
    let Some(key) = state.config.job_token_key.as_ref() else {
        return Err(ApiError::service_unavailable(
            "the job token key is not configured (CONVEYOR_JOB_TOKEN_KEY)",
        ));
    };
    let token = bearer(headers).ok_or_else(refused)?;
    let claims = job_token::verify(key, token, Utc::now()).map_err(|_| refused())?;
    if claims.job_id != job_id {
        return Err(refused());
    }

    let job = store::jobs::get(&state.pool, claims.tenant, job_id)
        .await?
        .ok_or_else(refused)?;
    // The pod's authorization is its ATTEMPT, not the worker's claim.
    //
    // A runner job is deferred between poll cycles (`store::jobs::defer` leaves the row `pending`
    // and clears the lease so the next poll can re-claim it), while the pod runs for minutes. So
    // requiring `running` with a live lease here would refuse every pod write that lands between
    // two polls. Measured live 2026-11-16 on the first dispatch: the executor finished in 10 s and
    // its `POST /result` answered 403, leaving the job `pending` for ever.
    //
    // `continuation` is the flag `defer` sets and only a re-claim of the same attempt clears, so
    // it means "between two polls of the attempt whose number is in the token". A finished job is
    // `succeeded`/`failed`/`dead`, a cancelled job is `dead`, and a superseded attempt carries a
    // different `attempts` — all three still fall through to the same 403.
    let attempt_is_live = match job.status.as_str() {
        "running" => job.lease_until.is_some_and(|until| until > Utc::now()),
        "pending" => job.continuation,
        _ => false,
    };
    if !attempt_is_live || job.attempts != claims.attempt {
        return Err(refused());
    }

    let run = store::runner::attempt(&state.pool, claims.tenant, job_id, claims.attempt)
        .await?
        .ok_or_else(refused)?;
    if run.is_terminal() {
        return Err(refused());
    }

    let instruction: Instruction =
        serde_json::from_value(run.instruction.clone()).map_err(|error| {
            ApiError::internal(format!("the stored instruction is not readable: {error}"))
        })?;

    Ok(Fenced {
        claims,
        run,
        instruction,
    })
}

/// One declared input's bytes.
#[utoipa::path(
    get,
    path = "/api/jobs/{id}/inputs/{name}",
    tag = "jobs",
    params(
        ("id" = Uuid, Path, description = "The job id"),
        ("name" = String, Path, description = "The input name the instruction declares")
    ),
    responses(
        (status = 200, description = "The input bytes"),
        (status = 403, description = "The job token is not valid for this job", body = crate::error::ProblemDetail),
        (status = 404, description = "The instruction declares no such input, or it has no bytes", body = crate::error::ProblemDetail),
        (status = 503, description = "The job token key or object storage is not configured", body = crate::error::ProblemDetail)
    )
)]
pub async fn job_input(
    State(state): State<AppState>,
    Path((id, name)): Path<(Uuid, String)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let fenced = fence(&state, &headers, id).await?;
    let input = fenced
        .instruction
        .inputs
        .iter()
        .find(|input| input.name == name)
        .ok_or_else(|| {
            ApiError::not_found(format!("the instruction declares no input {name:?}"))
        })?;

    let mut tx = store::begin_scoped(&state.pool, fenced.claims.tenant).await?;
    let loaded = conveyor_actions::documents::actions::fetch_bytes(
        &mut tx,
        fenced.claims.tenant,
        input.document_id,
        "jobs.inputs",
    )
    .await
    .map_err(|error| ApiError::internal(error.detail))?;
    tx.commit().await?;

    if loaded.content_hash != input.sha256 {
        return Err(ApiError::internal(format!(
            "input {name:?} does not match the hash the instruction declared"
        )));
    }

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, input.content_type.clone()),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_owned()),
            (header::ETAG, format!("\"{}\"", loaded.content_hash)),
            (header::CACHE_CONTROL, "no-store".to_owned()),
        ],
        Body::from(loaded.bytes),
    )
        .into_response())
}

/// A bounded progress object. The poll side reads it; nothing else does.
#[utoipa::path(
    put,
    path = "/api/jobs/{id}/progress",
    tag = "jobs",
    request_body(content = String, description = "A JSON object, at most four kilobytes"),
    params(("id" = Uuid, Path, description = "The job id")),
    responses(
        (status = 202, description = "Progress recorded"),
        (status = 403, description = "The job token is not valid for this job", body = crate::error::ProblemDetail),
        (status = 409, description = "The attempt has already reported its result", body = crate::error::ProblemDetail),
        (status = 413, description = "The progress document is larger than four kilobytes", body = crate::error::ProblemDetail),
        (status = 422, description = "The body is not a JSON object", body = crate::error::ProblemDetail)
    )
)]
pub async fn job_progress(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let fenced = fence(&state, &headers, id).await?;
    if body.len() > MAX_PROGRESS_BYTES {
        return Err(ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "progress_too_large",
            format!("progress is limited to {MAX_PROGRESS_BYTES} bytes"),
        ));
    }
    let progress: Value = serde_json::from_slice(&body)
        .map_err(|error| ApiError::unprocessable(format!("the progress is not JSON: {error}")))?;
    if !progress.is_object() {
        return Err(ApiError::unprocessable(
            "the progress must be a JSON object",
        ));
    }

    let accepted = store::runner::record_progress(
        &state.pool,
        fenced.claims.tenant,
        id,
        fenced.claims.attempt,
        &progress,
    )
    .await?;
    if !accepted {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "job_moved_on",
            "the attempt has already reported its result",
        ));
    }
    Ok(StatusCode::ACCEPTED.into_response())
}

/// Upload one declared output. Bytes go to conveyor's own object store, keyed by tenant and
/// attempt; the response carries the hash the pod must repeat in its result.
#[utoipa::path(
    post,
    path = "/api/jobs/{id}/outputs/{name}",
    tag = "jobs",
    request_body(content = Vec<u8>, content_type = "application/octet-stream"),
    params(
        ("id" = Uuid, Path, description = "The job id"),
        ("name" = String, Path, description = "The output name the instruction declares")
    ),
    responses(
        (status = 201, description = "The output was stored"),
        (status = 403, description = "The job token is not valid for this job", body = crate::error::ProblemDetail),
        (status = 409, description = "The output was already stored with different bytes", body = crate::error::ProblemDetail),
        (status = 413, description = "The output exceeds the size the instruction declared", body = crate::error::ProblemDetail),
        (status = 422, description = "The instruction declares no such output", body = crate::error::ProblemDetail),
        (status = 503, description = "Object storage is not configured", body = crate::error::ProblemDetail)
    )
)]
pub async fn job_output(
    State(state): State<AppState>,
    Path((id, name)): Path<(Uuid, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let fenced = fence(&state, &headers, id).await?;
    if fenced.run.status == "reported" {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "job_reported",
            "the attempt has already reported its result",
        ));
    }
    let declared = fenced
        .instruction
        .outputs
        .iter()
        .find(|output| output.name == name)
        .ok_or_else(|| {
            ApiError::unprocessable(format!("the instruction declares no output {name:?}"))
        })?;
    if body.len() as i64 > declared.max_bytes {
        return Err(ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "output_too_large",
            format!(
                "output {name:?} is limited to {} bytes by the instruction",
                declared.max_bytes
            ),
        ));
    }

    let objects =
        conveyor_actions::integrations::objects::Objects::from_env().ok_or_else(|| {
            ApiError::service_unavailable("object storage is not configured (CONVEYOR_S3_*)")
        })?;
    let tenant = fenced.claims.tenant;
    let sha256 = hex(&Sha256::digest(&body));
    // The object adapter serves only keys under `tenant/<tenant>/` (§11.10); a short `t/`
    // prefix is refused before the store is reached.
    let key = format!(
        "{}/{}/jobs/{}/{}/{}",
        conveyor_actions::storage::TENANT_PREFIX,
        tenant.as_uuid(),
        id,
        fenced.claims.attempt,
        name
    );
    objects
        .put(&key, body.clone())
        .await
        .map_err(|error| ApiError::service_unavailable(format!("object storage: {error}")))?;

    let artefact = ArtefactRow {
        name: name.clone(),
        sha256: sha256.clone(),
        bytes: body.len() as i64,
        content_type: declared.content_type.clone(),
        object_key: key,
    };
    let outcome =
        store::runner::insert_artefact(&state.pool, tenant, fenced.run.id, &artefact).await?;
    match outcome {
        Inserted::New | Inserted::Repeated => Ok((
            StatusCode::CREATED,
            axum::Json(json!({ "name": name, "sha256": sha256, "bytes": artefact.bytes })),
        )
            .into_response()),
        Inserted::Conflict => Err(ApiError::new(
            StatusCode::CONFLICT,
            "output_changed",
            format!("output {name:?} was already stored with different bytes"),
        )),
    }
}

/// Report the attempt's verdict. The shape is checked against the stored instruction; the job is
/// NOT finished here — the poll handler that holds the lease does that.
#[utoipa::path(
    post,
    path = "/api/jobs/{id}/result",
    tag = "jobs",
    request_body(content = String, description = "A runner/v1 result document"),
    params(("id" = Uuid, Path, description = "The job id")),
    responses(
        (status = 202, description = "The result was recorded for the poll to consume"),
        (status = 403, description = "The job token is not valid for this job", body = crate::error::ProblemDetail),
        (status = 409, description = "A result was already recorded for this attempt", body = crate::error::ProblemDetail),
        (status = 422, description = "The result does not satisfy the runner/v1 contract", body = crate::error::ProblemDetail)
    )
)]
pub async fn job_result(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let fenced = fence(&state, &headers, id).await?;
    let result: RunResult = serde_json::from_slice(&body).map_err(|error| {
        ApiError::unprocessable(format!("the result is not runner/v1: {error}"))
    })?;
    result
        .validate(&fenced.instruction)
        .map_err(|error| ApiError::unprocessable(error.to_string()))?;

    let value = serde_json::to_value(&result)
        .map_err(|error| ApiError::internal(format!("the result does not serialise: {error}")))?;
    let accepted = store::runner::record_result(
        &state.pool,
        fenced.claims.tenant,
        id,
        fenced.claims.attempt,
        &value,
    )
    .await?;
    if !accepted {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "job_moved_on",
            "the attempt has already reported its result",
        ));
    }
    Ok(StatusCode::ACCEPTED.into_response())
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}
