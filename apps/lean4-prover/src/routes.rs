//! HTTP surface: health, metrics, docs, and the three compute endpoints.
//!
//! Route paths are unversioned (`/api/<resource>`), per the estate convention.

use std::sync::atomic::Ordering;

use axum::Json;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde_json::json;

use crate::error::{AppError, AppResult};
use crate::models::{
    CheckRequest, CheckResponse, ElaborateRequest, ElaborateResponse, ProveRequest, ProveResponse,
};
use crate::state::AppState;

/// Every documented operation, as `"METHOD path"`. `--check-lock` compares this set with the
/// captured contract; the document flags run without any configuration or network.
pub const ROUTES: &[(&str, &str)] = &[
    ("GET", "/health"),
    ("GET", "/health/live"),
    ("GET", "/health/ready"),
    ("GET", "/metrics"),
    ("POST", "/api/check"),
    ("POST", "/api/elaborate"),
    ("POST", "/api/prove"),
];

pub fn router(state: AppState) -> axum::Router {
    let mut router = axum::Router::new()
        .route("/health", get(health))
        .route("/health/live", get(liveness))
        .route("/health/ready", get(readiness))
        .route("/metrics", get(metrics))
        .route("/api/check", post(check_source))
        .route("/api/elaborate", post(elaborate_term))
        .route("/api/prove", post(prove_theorem));

    if state.cfg.docs_enabled() {
        router = router
            .route("/openapi.json", get(openapi_handler))
            .route("/docs", get(docs));
    }
    router.with_state(state)
}

async fn openapi_handler() -> Json<serde_json::Value> {
    Json(openapi())
}

/// Outermost layer: counts every request, including the ones the licence gate
/// refuses. The per-call attribution line is emitted by the licence layer
/// instead, because that is the layer that knows the tenant.
pub async fn observe(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let response = next.run(req).await;
    state.metrics.requests_total.fetch_add(1, Ordering::Relaxed);
    response
}

async fn health(State(state): State<AppState>) -> Response {
    let (available, detail) = state.runner.health_check().await;
    if available {
        Json(json!({
            "status": "healthy",
            "service": state.cfg.service_name,
            "lean_available": true,
            "lean_version": detail,
        }))
        .into_response()
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "status": "degraded",
                "service": state.cfg.service_name,
                "lean_available": false,
                "error": detail,
            })),
        )
            .into_response()
    }
}

async fn liveness() -> Json<serde_json::Value> {
    Json(json!({ "status": "alive" }))
}

async fn readiness(State(state): State<AppState>) -> Response {
    let (available, detail) = state.runner.health_check().await;
    if available {
        Json(json!({ "status": "ready", "lean_available": true, "lean_version": detail }))
            .into_response()
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "status": "not_ready", "lean_available": false, "error": detail })),
        )
            .into_response()
    }
}

async fn metrics(State(state): State<AppState>) -> Response {
    (
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4; charset=utf-8")],
        state.metrics.render(),
    )
        .into_response()
}

async fn check_source(
    State(state): State<AppState>,
    Json(body): Json<CheckRequest>,
) -> AppResult<Json<CheckResponse>> {
    body.validate().map_err(AppError::Validation)?;
    let _permit = state.gate.acquire().await?;
    let result = state
        .runner
        .check(&body)
        .await
        .map_err(|err| AppError::Internal(format!("Lean typecheck failed: {err}")))?;
    Ok(Json(result))
}

async fn elaborate_term(
    State(state): State<AppState>,
    Json(body): Json<ElaborateRequest>,
) -> AppResult<Json<ElaborateResponse>> {
    body.validate().map_err(AppError::Validation)?;
    let _permit = state.gate.acquire().await?;
    let result = state
        .runner
        .elaborate(&body)
        .await
        .map_err(|err| AppError::Internal(format!("Lean elaboration failed: {err}")))?;
    Ok(Json(result))
}

async fn prove_theorem(
    State(state): State<AppState>,
    Json(body): Json<ProveRequest>,
) -> AppResult<Json<ProveResponse>> {
    body.validate().map_err(AppError::Validation)?;
    let _permit = state.gate.acquire().await?;
    let result = state
        .runner
        .prove(&body)
        .await
        .map_err(|err| AppError::Internal(format!("Lean proof attempt failed: {err}")))?;
    Ok(Json(result))
}

async fn docs() -> Response {
    (
        StatusCode::OK,
        [("content-type", "text/html; charset=utf-8")],
        "<!doctype html><title>Lean4 Prover API</title><body>\
         <h1>Lean4 Prover API</h1>\
         <p>OpenAPI: <a href=\"/openapi.json\">/openapi.json</a></p>\
         <p>Every compute call requires <code>X-Tenant-Id: Bearer &lt;licence token&gt;</code>.\
         </p></body>",
    )
        .into_response()
}

/// A static document describing the three compute routes. The previous service
/// generated this from FastAPI's route table; the contract callers and the
/// docs page need is the path list plus the `LicenceToken` security scheme.
pub fn openapi() -> serde_json::Value {
    let protected = |summary: &str, request: &str, response: &str| {
        json!({
            "post": {
                "summary": summary,
                "security": [{ "LicenceToken": [] }],
                "requestBody": {
                    "required": true,
                    "content": { "application/json": { "schema": { "$ref": format!("#/components/schemas/{request}") } } }
                },
                "responses": {
                    "200": {
                        "description": "Result",
                        "content": { "application/json": { "schema": { "$ref": format!("#/components/schemas/{response}") } } }
                    },
                    "401": { "description": "Missing, invalid or inactive licence" },
                    "422": { "description": "Request violates a documented bound" },
                    "503": { "description": "Prove lane saturated, or licences service unavailable" }
                }
            }
        })
    };

    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Lean4 Prover API",
            "version": "0.1.0",
            "description": "Lean 4 formal verification as a REST API. Every compute call must carry a licence token (X-Tenant-Id: Bearer <token>), introspected against the licenses service."
        },
        "paths": {
            "/api/check": protected("Typecheck Lean 4 source", "CheckRequest", "CheckResponse"),
            "/api/elaborate": protected("Elaborate a Lean 4 term", "ElaborateRequest", "ElaborateResponse"),
            "/api/prove": protected("Prove a theorem with tactics", "ProveRequest", "ProveResponse"),
            "/health": { "get": { "summary": "Full health check", "security": [] } },
            "/health/live": { "get": { "summary": "Liveness probe", "security": [] } },
            "/health/ready": { "get": { "summary": "Readiness probe", "security": [] } },
            "/metrics": { "get": { "summary": "Prometheus metrics", "security": [] } }
        },
        "components": {
            "securitySchemes": {
                "LicenceToken": {
                    "type": "apiKey",
                    "in": "header",
                    "name": "X-Tenant-Id",
                    "description": "Licence token as 'Bearer <token>'. Introspected against the licenses service; the tenant is taken from the licence, never from a header. 'Authorization: Bearer <token>' is also accepted."
                }
            },
            "schemas": {
                "CheckRequest": {
                    "type": "object",
                    "required": ["source"],
                    "properties": {
                        "source": { "type": "string", "maxLength": 100000 },
                        "timeout": { "type": "integer", "minimum": 1, "maximum": 120 }
                    }
                },
                "ElaborateRequest": {
                    "type": "object",
                    "required": ["term"],
                    "properties": {
                        "term": { "type": "string", "maxLength": 50000 },
                        "context": { "type": "string", "maxLength": 100000 },
                        "timeout": { "type": "integer", "minimum": 1, "maximum": 120 }
                    }
                },
                "ProveRequest": {
                    "type": "object",
                    "required": ["theorem"],
                    "properties": {
                        "theorem": { "type": "string", "maxLength": 50000 },
                        "tactics": { "type": "array", "items": { "type": "string" }, "maxItems": 50 },
                        "imports": { "type": "array", "items": { "type": "string" }, "maxItems": 50 },
                        "timeout": { "type": "integer", "minimum": 1, "maximum": 120 }
                    }
                },
                "LeanDiagnostic": {
                    "type": "object",
                    "required": ["severity", "message"],
                    "properties": {
                        "severity": { "type": "string" },
                        "line": { "type": ["integer", "null"] },
                        "column": { "type": ["integer", "null"] },
                        "message": { "type": "string" }
                    }
                },
                "CheckResponse": { "$ref": "#/components/schemas/DiagnosticResponse" },
                "ElaborateResponse": { "$ref": "#/components/schemas/DiagnosticResponse" },
                "ProveResponse": { "$ref": "#/components/schemas/DiagnosticResponse" },
                "DiagnosticResponse": {
                    "type": "object",
                    "properties": {
                        "valid": { "type": "boolean" },
                        "proven": { "type": "boolean" },
                        "successful_tactic": { "type": ["string", "null"] },
                        "elaborated_type": { "type": ["string", "null"] },
                        "diagnostics": {
                            "type": "array",
                            "items": { "$ref": "#/components/schemas/LeanDiagnostic" }
                        },
                        "stdout": { "type": "string" },
                        "stderr": { "type": "string" },
                        "computation_time_ms": { "type": "number" }
                    }
                }
            }
        }
    })
}
