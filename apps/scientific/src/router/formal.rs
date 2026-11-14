//! `/api/formal` — translation to formal logic, verification, and the proof store.
//!
//! Two decisions shape this module.
//!
//! **The prover answers back-pressure, not failure.** A saturated GPU is `503 gpu_busy` with a
//! `Retry-After`; the caller re-drives. That is a `FormalFailure::Busy`, never a `500`.
//!
//! **Only Lean 4 is verified here.** The old service evaluated `z3` and `sympy` expressions through
//! its Python sandbox executor; there is no faithful Rust equivalent, and the one live prover in
//! the estate is Lean 4. A `z3` or `sympy` verification is therefore refused by name
//! (`422 language_not_supported`) instead of answered with a guess. Translation to those languages
//! still works — that is a model call, not an evaluation — which is why `translate` accepts all
//! three and `verify` accepts one.

use std::time::Instant;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::ToSchema;

use crate::clients::inference::InferenceClient;
use crate::clients::lean4::{Lean4Client, Lean4Error};
use crate::clients::objects;
use crate::router::{Failure, caller_tenant};
use crate::state::Shared;

/// How many times the Lean 4 translation retries after a failed elaboration — once, with the
/// compiler's errors in the prompt, which is what the old service did.
const LEAN4_RETRIES: u32 = 1;

const TRANSLATE_PROMPT: &str = r#"You are a formal logic translator. Convert the following natural language claim into a {language} expression.

Claim: {claim_text}

Rules for Z3:
- Use z3 Python API syntax (e.g., And, Or, Not, Implies, ForAll, Exists)
- Declare variables with appropriate sorts (Int, Real, Bool, etc.)
- Return ONLY the Python expression, no explanation
- Example: Implies(And(Bool('p'), Implies(Bool('p'), Bool('q'))), Bool('q'))

Rules for SymPy:
- Use sympy syntax (e.g., Eq, simplify, symbols, Symbol)
- Declare symbols inline with Symbol('x') or symbols('x y')
- Return ONLY the Python expression, no explanation
- Example: Eq(Symbol('x')**2 - 1, (Symbol('x') - 1)*(Symbol('x') + 1))

Expression:"#;

const LEAN4_TRANSLATE_PROMPT: &str = r#"You are a Lean 4 formal proof translator. Convert the following natural language claim into a Lean 4 theorem statement with proof.

Claim: {claim_text}

Rules:
- Use Lean 4 syntax (not Lean 3)
- Include both the theorem statement and proof
- Use tactics like simp, norm_num, omega, ring, linarith where appropriate
- Return ONLY valid Lean 4 code, no explanation
- Example: theorem test : 1 + 1 = 2 := by norm_num

Lean 4 code:"#;

const LEAN4_RETRY_PROMPT: &str = r#"The Lean 4 code you generated had elaboration errors. Please fix it.

Original claim: {claim_text}
Your previous code: {previous_code}
Elaboration errors: {errors}

Return ONLY the corrected Lean 4 code:"#;

/// A refusal from these routes: either the contract's `{"detail": …}` body, or the prover's
/// back-pressure answer, which carries a header and so cannot be a plain tuple.
#[derive(Debug)]
pub enum FormalFailure {
    Refusal(Failure),
    Busy { retry_after: u32 },
}

impl From<Failure> for FormalFailure {
    fn from(failure: Failure) -> Self {
        Self::Refusal(failure)
    }
}

impl IntoResponse for FormalFailure {
    fn into_response(self) -> Response {
        match self {
            Self::Refusal((status, body)) => (status, body).into_response(),
            Self::Busy { retry_after } => (
                StatusCode::SERVICE_UNAVAILABLE,
                [(axum::http::header::RETRY_AFTER, retry_after.to_string())],
                Json(json!({"error": "gpu_busy", "retry_after": retry_after})),
            )
                .into_response(),
        }
    }
}

fn detail(status: StatusCode, message: impl Into<String>) -> FormalFailure {
    FormalFailure::Refusal((status, Json(json!({"detail": message.into()}))))
}

fn gpu_busy(retry_after: u32) -> FormalFailure {
    FormalFailure::Busy { retry_after }
}

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct FormalTranslateRequest {
    pub claim_text: String,
    #[serde(default = "default_language")]
    pub target_language: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct FormalTranslateResponse {
    pub claim_text: String,
    pub formal_expression: String,
    pub target_language: String,
    pub confidence: f64,
    pub computation_time_ms: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct FormalVerifyRequest {
    pub formal_expression: String,
    #[serde(default = "default_language")]
    pub language: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct FormalVerifyResponse {
    pub expression: String,
    pub language: String,
    pub is_valid: Option<bool>,
    pub counterexample: Option<String>,
    pub proof: Option<String>,
    pub lean_elaboration: Option<Value>,
    pub computation_time_ms: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct ProofStoreRequest {
    pub expression: String,
    #[serde(default)]
    pub is_valid: Option<bool>,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default)]
    pub proof: Option<String>,
}

/// The one field the store route answers with.
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct ProofStored {
    pub key: String,
}

fn default_language() -> String {
    "z3".to_owned()
}

fn known_language(language: &str) -> bool {
    matches!(language, "lean4" | "z3" | "sympy")
}

/// `POST /api/formal/translate`
#[utoipa::path(
    post,
    path = "/api/formal/translate",
    operation_id = "formal_translate",
    request_body = FormalTranslateRequest,
    responses(
        (status = 200, body = FormalTranslateResponse),
        (status = 422, body = Value),
        (status = 503, body = Value)
    ),
    tag = "formal"
)]
pub async fn translate(
    State(state): State<Shared>,
    headers: HeaderMap,
    Json(body): Json<FormalTranslateRequest>,
) -> Result<Json<FormalTranslateResponse>, FormalFailure> {
    if body.claim_text.chars().count() > 5000 {
        return Err(detail(
            StatusCode::UNPROCESSABLE_ENTITY,
            "claim_text exceeds 5000 characters",
        ));
    }
    if !known_language(&body.target_language) {
        return Err(detail(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("unknown target_language: {}", body.target_language),
        ));
    }
    let started = Instant::now();
    let inference = state.inference.as_ref().ok_or_else(|| {
        detail(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Formal translation failed",
        )
    })?;
    let tenant = optional_tenant(&headers);

    let response = if body.target_language == "lean4" {
        translate_lean4(
            inference,
            state.lean4.as_ref(),
            &body.claim_text,
            tenant,
            started,
        )
        .await?
    } else {
        let prompt = TRANSLATE_PROMPT
            .replace("{language}", &body.target_language)
            .replace("{claim_text}", &body.claim_text);
        let raw = inference
            .chat(
                &prompt,
                1024,
                &format!("sp-formal-translate-{}", body.target_language),
                tenant,
            )
            .await
            .map_err(|_| {
                detail(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Formal translation failed",
                )
            })?;
        FormalTranslateResponse {
            claim_text: body.claim_text.clone(),
            formal_expression: extract_expression(&raw),
            target_language: body.target_language.clone(),
            confidence: 0.8,
            computation_time_ms: millis(started),
        }
    };
    Ok(Json(response))
}

async fn translate_lean4(
    inference: &InferenceClient,
    lean4: Option<&Lean4Client>,
    claim_text: &str,
    tenant: Option<&str>,
    started: Instant,
) -> Result<FormalTranslateResponse, FormalFailure> {
    let prompt = LEAN4_TRANSLATE_PROMPT.replace("{claim_text}", claim_text);
    let raw = inference
        .chat(&prompt, 2048, "sp-formal-translate-lean4", tenant)
        .await
        .map_err(|_| {
            detail(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Formal translation failed",
            )
        })?;
    let mut expression = extract_expression(&raw);

    let Some(lean4) = lean4 else {
        return Ok(FormalTranslateResponse {
            claim_text: claim_text.to_owned(),
            formal_expression: expression,
            target_language: "lean4".to_owned(),
            confidence: 0.8,
            computation_time_ms: millis(started),
        });
    };

    let mut confidence = 0.9;
    for attempt in 0..=LEAN4_RETRIES {
        match lean4.elaborate(&expression).await {
            Ok(result) if result.get("success").and_then(Value::as_bool) == Some(true) => {
                return Ok(FormalTranslateResponse {
                    claim_text: claim_text.to_owned(),
                    formal_expression: expression,
                    target_language: "lean4".to_owned(),
                    confidence,
                    computation_time_ms: millis(started),
                });
            }
            Ok(result) => {
                if attempt == LEAN4_RETRIES {
                    return Ok(FormalTranslateResponse {
                        claim_text: claim_text.to_owned(),
                        formal_expression: expression,
                        target_language: "lean4".to_owned(),
                        confidence: 0.0,
                        computation_time_ms: millis(started),
                    });
                }
                let errors = result
                    .get("errors")
                    .map(errors_as_text)
                    .unwrap_or_else(|| "elaboration failed".to_owned());
                let retry = LEAN4_RETRY_PROMPT
                    .replace("{claim_text}", claim_text)
                    .replace("{previous_code}", &expression)
                    .replace("{errors}", &errors);
                let raw = inference
                    .chat(&retry, 2048, "sp-formal-translate-lean4", tenant)
                    .await
                    .map_err(|_| {
                        detail(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            "Formal translation failed",
                        )
                    })?;
                expression = extract_expression(&raw);
                confidence = 0.0;
            }
            Err(Lean4Error::Busy { retry_after }) => return Err(gpu_busy(retry_after)),
            Err(Lean4Error::MissingLicence | Lean4Error::Unconfigured) => {
                return Err(detail(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Formal translation failed",
                ));
            }
            // A transport failure is an elaboration failure, not a shelving: the old client made
            // the same distinction and retried once with "elaboration request failed".
            Err(Lean4Error::Upstream(_)) => {
                if attempt == LEAN4_RETRIES {
                    return Ok(FormalTranslateResponse {
                        claim_text: claim_text.to_owned(),
                        formal_expression: expression,
                        target_language: "lean4".to_owned(),
                        confidence: 0.0,
                        computation_time_ms: millis(started),
                    });
                }
                confidence = 0.0;
            }
        }
    }

    Ok(FormalTranslateResponse {
        claim_text: claim_text.to_owned(),
        formal_expression: expression,
        target_language: "lean4".to_owned(),
        confidence,
        computation_time_ms: millis(started),
    })
}

/// `POST /api/formal/verify`
#[utoipa::path(
    post,
    path = "/api/formal/verify",
    operation_id = "formal_verify",
    request_body = FormalVerifyRequest,
    responses(
        (status = 200, body = FormalVerifyResponse),
        (status = 422, body = Value),
        (status = 503, body = Value)
    ),
    tag = "formal"
)]
pub async fn verify(
    State(state): State<Shared>,
    Json(body): Json<FormalVerifyRequest>,
) -> Result<Json<FormalVerifyResponse>, FormalFailure> {
    if body.formal_expression.chars().count() > 5000 {
        return Err(detail(
            StatusCode::UNPROCESSABLE_ENTITY,
            "formal_expression exceeds 5000 characters",
        ));
    }
    if !known_language(&body.language) {
        return Err(detail(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("unknown language: {}", body.language),
        ));
    }
    if body.language != "lean4" {
        return Err(detail(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!(
                "language_not_supported: {} expressions were evaluated by the retired Python \
                 sandbox; only lean4 is verified here",
                body.language
            ),
        ));
    }

    let started = Instant::now();
    let Some(lean4) = state.lean4.as_ref() else {
        return Ok(Json(FormalVerifyResponse {
            expression: body.formal_expression,
            language: body.language,
            is_valid: None,
            counterexample: None,
            proof: Some("Lean4 prover not configured".to_owned()),
            lean_elaboration: None,
            computation_time_ms: millis(started),
        }));
    };

    let result = lean4
        .check(&body.formal_expression)
        .await
        .map_err(|error| match error {
            Lean4Error::Busy { retry_after } => gpu_busy(retry_after),
            _ => detail(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Formal verification failed",
            ),
        })?;

    let is_valid = result.get("success").and_then(Value::as_bool);
    let errors = result.get("errors").cloned().unwrap_or(Value::Null);
    let messages = result.get("messages").cloned().unwrap_or(Value::Null);
    let proof = messages
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join("\n")
        })
        .filter(|text| !text.is_empty());

    Ok(Json(FormalVerifyResponse {
        expression: body.formal_expression,
        language: body.language,
        is_valid,
        counterexample: None,
        proof,
        lean_elaboration: Some(json!({"errors": errors, "messages": messages})),
        computation_time_ms: millis(started),
    }))
}

/// `POST /api/formal/proofs/{claim_id}`
#[utoipa::path(
    post,
    path = "/api/formal/proofs/{claim_id}",
    operation_id = "formal_create_proof",
    params(("claim_id" = String, Path, description = "Claim identifier")),
    request_body = ProofStoreRequest,
    responses((status = 200, body = ProofStored), (status = 400, body = Value), (status = 500, body = Value)),
    tag = "formal"
)]
pub async fn create_proof(
    State(state): State<Shared>,
    headers: HeaderMap,
    Path(claim_id): Path<String>,
    Json(body): Json<ProofStoreRequest>,
) -> Result<Json<ProofStored>, FormalFailure> {
    let tenant = caller_tenant(&headers)?;
    if !crate::router::safe_segment(&claim_id) {
        return Err(detail(
            StatusCode::BAD_REQUEST,
            "claim_id is not a claim identifier",
        ));
    }
    let created_at = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let key = format!("proofs/{claim_id}/{created_at}.json");
    let record = json!({
        "claim_id": claim_id,
        "key": key,
        "created_at": created_at,
        "tenant_id": tenant,
        "expression": body.expression,
        "language": body.language,
        "is_valid": body.is_valid,
        "proof": body.proof,
    });
    objects::put_json(state.objects.as_ref(), &key, &record, false)
        .await
        .map_err(|_| detail(StatusCode::INTERNAL_SERVER_ERROR, "Proof storage failed"))?;
    Ok(Json(ProofStored { key }))
}

/// `GET /api/formal/proofs/{claim_id}`
///
/// The proofs are one tenant's, by the record's own `tenant_id` field: the old key layout is
/// shared across tenants, so the filter is what keeps a claim's proofs from being everyone's. Old
/// records carry the default tenant they were written with, which is the honest owner.
#[utoipa::path(
    get,
    path = "/api/formal/proofs/{claim_id}",
    operation_id = "formal_get_proofs",
    params(("claim_id" = String, Path, description = "Claim identifier")),
    responses((status = 200, body = Value), (status = 400, body = Value), (status = 500, body = Value)),
    tag = "formal"
)]
pub async fn get_proofs(
    State(state): State<Shared>,
    headers: HeaderMap,
    Path(claim_id): Path<String>,
) -> Result<Json<Value>, FormalFailure> {
    let tenant = caller_tenant(&headers)?;
    if !crate::router::safe_segment(&claim_id) {
        return Err(detail(
            StatusCode::BAD_REQUEST,
            "claim_id is not a claim identifier",
        ));
    }
    let prefix = format!("proofs/{claim_id}/");
    let keys = objects::list(state.objects.as_ref(), &prefix)
        .await
        .map_err(|_| detail(StatusCode::INTERNAL_SERVER_ERROR, "Proof retrieval failed"))?;

    let mut proofs = Vec::new();
    for key in keys {
        let body = objects::get(state.objects.as_ref(), &key)
            .await
            .map_err(|_| detail(StatusCode::INTERNAL_SERVER_ERROR, "Proof retrieval failed"))?;
        let Ok(value) = serde_json::from_slice::<Value>(&body) else {
            continue;
        };
        let owner = value.get("tenant_id").and_then(Value::as_str);
        if owner == Some(tenant.as_str()) {
            proofs.push(value);
        }
    }
    proofs.sort_by_key(|value| {
        value
            .get("created_at")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned()
    });
    Ok(Json(Value::Array(proofs)))
}

fn optional_tenant(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("x-tenant-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
}

fn millis(started: Instant) -> f64 {
    let millis = started.elapsed().as_secs_f64() * 1000.0;
    (millis * 100.0).round() / 100.0
}

/// Strip a markdown fence from a model's answer, which is what the old service did.
fn extract_expression(raw: &str) -> String {
    let text = raw.trim();
    if text.starts_with("```") {
        return text
            .lines()
            .filter(|line| !line.trim_start().starts_with("```"))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_owned();
    }
    text.to_owned()
}

fn errors_as_text(value: &Value) -> String {
    match value {
        Value::Array(items) => items
            .iter()
            .map(|item| match item {
                Value::String(text) => text.clone(),
                other => other.to_string(),
            })
            .collect::<Vec<_>>()
            .join("; "),
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::state::AppState;
    use std::sync::Arc;

    fn state() -> Shared {
        Arc::new(AppState::new(
            Config::from_lookup(&[]).expect("defaults parse"),
        ))
    }

    fn with_tenant(tenant: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("x-tenant-id", tenant.parse().expect("header value"));
        headers
    }

    #[tokio::test]
    async fn a_non_lean_language_is_refused_by_name() {
        let result = verify(
            State(state()),
            Json(FormalVerifyRequest {
                formal_expression: "x > 0".to_owned(),
                language: "z3".to_owned(),
            }),
        )
        .await;
        let failure = result.expect_err("refused");
        match failure {
            FormalFailure::Refusal((status, body)) => {
                assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
                assert!(
                    body.0["detail"]
                        .as_str()
                        .unwrap_or_default()
                        .contains("language_not_supported")
                );
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn an_unconfigured_prover_answers_the_documented_shape() {
        let Json(body) = verify(
            State(state()),
            Json(FormalVerifyRequest {
                formal_expression: "theorem t : True := by trivial".to_owned(),
                language: "lean4".to_owned(),
            }),
        )
        .await
        .expect("answers");
        assert_eq!(body.is_valid, None);
        assert_eq!(body.proof.as_deref(), Some("Lean4 prover not configured"));
    }

    #[tokio::test]
    async fn a_proof_write_needs_a_tenant_and_a_store() {
        let body = ProofStoreRequest {
            expression: "x = x".to_owned(),
            is_valid: Some(true),
            language: "lean4".to_owned(),
            proof: None,
        };
        let result = create_proof(
            State(state()),
            HeaderMap::new(),
            Path("c1".to_owned()),
            Json(body.clone()),
        )
        .await;
        assert!(matches!(
            result.expect_err("no tenant"),
            FormalFailure::Refusal((StatusCode::BAD_REQUEST, _))
        ));

        let result = create_proof(
            State(state()),
            with_tenant("t1"),
            Path("c1".to_owned()),
            Json(body),
        )
        .await;
        assert!(matches!(
            result.expect_err("no store"),
            FormalFailure::Refusal((StatusCode::INTERNAL_SERVER_ERROR, _))
        ));
    }

    #[tokio::test]
    async fn a_claim_id_that_cannot_be_a_key_segment_is_a_400() {
        let result = get_proofs(
            State(state()),
            with_tenant("t1"),
            Path("../other".to_owned()),
        )
        .await;
        match result.expect_err("refused") {
            FormalFailure::Refusal((status, _)) => assert_eq!(status, StatusCode::BAD_REQUEST),
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_fence_is_stripped_and_nothing_else_is() {
        assert_eq!(
            extract_expression("```lean\ntheorem t : True := by trivial\n```"),
            "theorem t : True := by trivial"
        );
        assert_eq!(extract_expression(" x = x "), "x = x");
    }
}
