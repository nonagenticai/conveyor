//! The health and status routes.
//!
//! `/health` and `/health/ready` probe the configured downstreams, and the verdict is what the
//! Deployment's probes read: `/health/ready` answers **503** when a critical dependency is
//! unhealthy, because an httpGet probe reads the status code and never the body. The old service
//! learned that the hard way; the port keeps the lesson.

use std::collections::BTreeMap;
use std::time::Duration;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::ToSchema;

use crate::state::Shared;

/// How long one downstream probe may take before it is unhealthy.
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct HealthStatus {
    /// Service status: healthy, degraded, or unhealthy.
    pub status: String,
    /// Timestamp (ISO 8601).
    pub timestamp: String,
    /// Status of backend services.
    pub services: BTreeMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct LivenessStatus {
    /// Whether service is alive.
    pub alive: bool,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ReadinessStatus {
    /// Whether service is ready.
    pub ready: bool,
    /// Reason if not ready — `null` when ready, never absent: the old service serialised the
    /// field and a client that reads it must see the same document.
    pub reason: Option<String>,
}

/// One configured downstream, by the name the aggregate report uses.
fn downstreams(state: &Shared) -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    if let Some(url) = &state.config.lean4_prover_url {
        out.push(("lean4_prover", url.clone()));
    }
    if let Some(url) = &state.config.inference_router_url {
        out.push(("inference_router", url.clone()));
    }
    if let Some(url) = &state.config.pytorch_backend_url {
        out.push(("pytorch_backend", url.clone()));
    }
    out
}

/// Probe `<url>/health`; any non-5xx answer counts as up (a 4xx is a live service refusing an
/// unauthenticated probe, not an outage), a transport error or a 5xx is down.
async fn probe(client: &reqwest::Client, name: &'static str, url: &str) -> (String, Value) {
    let target = format!("{}/health", url.trim_end_matches('/'));
    match client.get(&target).send().await {
        Ok(response) if response.status().is_server_error() || response.status().as_u16() == 0 => (
            name.to_owned(),
            json!({"status": "unhealthy", "code": response.status().as_u16()}),
        ),
        Ok(response) => (
            name.to_owned(),
            json!({"status": "healthy", "code": response.status().as_u16()}),
        ),
        Err(error) => (
            name.to_owned(),
            json!({"status": "unhealthy", "error": error.to_string()}),
        ),
    }
}

async fn probe_all(state: &Shared) -> BTreeMap<String, Value> {
    let targets = downstreams(state);
    if targets.is_empty() {
        return BTreeMap::new();
    }
    let client = match reqwest::Client::builder().timeout(PROBE_TIMEOUT).build() {
        Ok(client) => client,
        Err(error) => {
            let mut out = BTreeMap::new();
            out.insert(
                "probe_client".to_owned(),
                json!({"status": "unhealthy", "error": error.to_string()}),
            );
            return out;
        }
    };
    let mut out = BTreeMap::new();
    for (name, url) in targets {
        let (key, value) = probe(&client, name, &url).await;
        out.insert(key, value);
    }
    out
}

/// `GET /health` — the aggregate verdict over the configured downstreams.
#[utoipa::path(
    get,
    path = "/health",
    operation_id = "health_check",
    responses((status = 200, body = HealthStatus)),
    tag = "health"
)]
pub async fn health_check(State(state): State<Shared>) -> Json<HealthStatus> {
    let services = probe_all(&state).await;
    let unhealthy = services
        .values()
        .filter(|value| value.get("status").and_then(Value::as_str) == Some("unhealthy"))
        .count();
    let status = if unhealthy == 0 {
        "healthy"
    } else if unhealthy < services.len() {
        "degraded"
    } else {
        "unhealthy"
    };
    Json(HealthStatus {
        status: status.to_owned(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        services,
    })
}

/// `GET /health/live` — the process is up; it probes nothing.
#[utoipa::path(
    get,
    path = "/health/live",
    operation_id = "health_liveness",
    responses((status = 200, body = LivenessStatus)),
    tag = "health"
)]
pub async fn liveness() -> Json<LivenessStatus> {
    Json(LivenessStatus { alive: true })
}

/// `GET /health/ready` — 200 when every configured downstream is healthy, else 503.
#[utoipa::path(
    get,
    path = "/health/ready",
    operation_id = "health_readiness",
    responses((status = 200, body = ReadinessStatus), (status = 503, body = ReadinessStatus)),
    tag = "health"
)]
pub async fn readiness(State(state): State<Shared>) -> Response {
    let services = probe_all(&state).await;
    if let Some((name, _)) = services
        .iter()
        .find(|(_, value)| value.get("status").and_then(Value::as_str) == Some("unhealthy"))
    {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ReadinessStatus {
                ready: false,
                reason: Some(format!("{name} is unhealthy")),
            }),
        )
            .into_response();
    }
    Json(ReadinessStatus {
        ready: true,
        reason: None,
    })
    .into_response()
}

/// `GET /status` — the lightweight answer a gateway can cheaply ask for.
#[utoipa::path(
    get,
    path = "/status",
    operation_id = "health_status",
    responses((status = 200, body = Value)),
    tag = "health"
)]
pub async fn status() -> Json<Value> {
    Json(json!({"status": "ok"}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn state(pairs: &[(&str, &str)]) -> Shared {
        let vars: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        Shared::new(crate::state::AppState::new(
            Config::from_lookup(&vars).expect("config parses"),
        ))
    }

    #[tokio::test]
    async fn readiness_with_no_configured_downstream_is_ready() {
        let response = readiness(State(state(&[]))).await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn readiness_with_an_unreachable_downstream_is_503() {
        // Port 1 on loopback refuses immediately; no name lookup, no waiting.
        let response = readiness(State(state(&[(
            "SCIENTIFIC_PROCESSOR_LEAN4_PROVER_URL",
            "http://127.0.0.1:1",
        )])))
        .await;
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn status_is_the_two_word_document_the_gateway_expects() {
        let Json(value) = status().await;
        assert_eq!(value, json!({"status": "ok"}));
    }
}
