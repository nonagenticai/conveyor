//! `/api/pipeline` — the process status and the verification history the object store holds.
//!
//! Three read routes survive the graph slice. Two of them (`history`, `anomalies`) read the
//! append-only record the old service wrote — one immutable object per verification under
//! `history/{tenant}/{entity}/…` — because S3 has no atomic append and a per-object write is
//! race-free by construction. An unconfigured store answers empty rather than failing, which is
//! what makes these routes usable before the cutover.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::{IntoParams, ToSchema};

use crate::clients::objects;
use crate::router::{Failure, caller_tenant};
use crate::state::Shared;

/// The route's answer. Every field carries a default in the captured contract, so an idle process
/// answers a complete document rather than a sparse one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct PipelineStatusResponse {
    pub running: bool,
    pub queue_depth: u64,
    pub processed_total: u64,
    pub failed_total: u64,
    pub last_run_at: Option<String>,
    pub throughput_per_minute: f64,
}

#[derive(Debug, Clone, Deserialize, IntoParams)]
pub struct AnomaliesQuery {
    /// Score drop that counts as an anomaly; the old default was 0.2.
    pub threshold: Option<f64>,
}

/// `GET /api/pipeline/status`
#[utoipa::path(
    get,
    path = "/api/pipeline/status",
    operation_id = "pipeline_status",
    responses((status = 200, body = PipelineStatusResponse)),
    tag = "pipeline"
)]
pub async fn status(State(state): State<Shared>) -> Json<PipelineStatusResponse> {
    let snapshot = state.pipeline.snapshot();
    Json(PipelineStatusResponse {
        running: snapshot.running,
        queue_depth: snapshot.queue_depth,
        processed_total: snapshot.processed_total,
        failed_total: snapshot.failed_total,
        last_run_at: snapshot.last_run_at,
        throughput_per_minute: snapshot.throughput_per_minute,
    })
}

/// `GET /api/pipeline/history/{entity_id}`
#[utoipa::path(
    get,
    path = "/api/pipeline/history/{entity_id}",
    operation_id = "pipeline_history",
    params(("entity_id" = String, Path, description = "Graph entity identifier")),
    responses((status = 200, body = Value), (status = 400, body = Value), (status = 500, body = Value)),
    tag = "pipeline"
)]
pub async fn history(
    State(state): State<Shared>,
    headers: HeaderMap,
    Path(entity_id): Path<String>,
) -> Result<Json<Value>, Failure> {
    let tenant = caller_tenant(&headers)?;
    if !crate::router::safe_segment(&entity_id) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"detail": "entity_id is not an entity identifier"})),
        ));
    }
    let prefix = format!("history/{tenant}/{entity_id}/");
    let entries = load(&state, &prefix).await?;
    let total = entries.len();
    Ok(Json(
        json!({"entity_id": entity_id, "entries": entries, "total": total}),
    ))
}

/// `GET /api/pipeline/anomalies`
#[utoipa::path(
    get,
    path = "/api/pipeline/anomalies",
    operation_id = "pipeline_anomalies",
    params(AnomaliesQuery),
    responses((status = 200, body = Value), (status = 400, body = Value), (status = 500, body = Value)),
    tag = "pipeline"
)]
pub async fn anomalies(
    State(state): State<Shared>,
    headers: HeaderMap,
    Query(query): Query<AnomaliesQuery>,
) -> Result<Json<Value>, Failure> {
    let tenant = caller_tenant(&headers)?;
    let threshold = query.threshold.unwrap_or(0.2);
    let prefix = format!("history/{tenant}/");

    let keys = objects::list(state.objects.as_ref(), &prefix)
        .await
        .map_err(upstream_failure)?;

    let mut by_entity: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for key in keys {
        let rest = key.strip_prefix(&prefix).unwrap_or(&key);
        if let Some((entity, _)) = rest.split_once('/')
            && !entity.is_empty()
        {
            by_entity
                .entry(entity.to_owned())
                .or_default()
                .push(key.clone());
        }
    }

    let mut anomalies = Vec::new();
    for (entity_id, keys) in by_entity {
        let entries = load_keys(&state, &keys).await?;
        if entries.len() < 2 {
            continue;
        }
        let previous = &entries[entries.len() - 2];
        let current = &entries[entries.len() - 1];
        let (Some(previous_score), Some(current_score)) = (
            previous.get("integrity_score").and_then(Value::as_f64),
            current.get("integrity_score").and_then(Value::as_f64),
        ) else {
            continue;
        };
        let delta = current_score - previous_score;
        if delta < -threshold {
            anomalies.push(json!({
                "entity_id": entity_id,
                "previous_score": previous_score,
                "current_score": current_score,
                "delta": round4(delta),
                "timestamp": current.get("timestamp").cloned().unwrap_or(Value::Null),
            }));
        }
    }

    let total = anomalies.len();
    Ok(Json(json!({"anomalies": anomalies, "total": total})))
}

/// Every record under `prefix`, parsed and sorted chronologically.
async fn load(state: &Shared, prefix: &str) -> Result<Vec<Value>, Failure> {
    let keys = objects::list(state.objects.as_ref(), prefix)
        .await
        .map_err(upstream_failure)?;
    load_keys(state, &keys).await
}

async fn load_keys(state: &Shared, keys: &[String]) -> Result<Vec<Value>, Failure> {
    let mut entries = Vec::new();
    for key in keys {
        let body = objects::get(state.objects.as_ref(), key)
            .await
            .map_err(upstream_failure)?;
        for line in String::from_utf8_lossy(&body).lines() {
            if line.trim().is_empty() {
                continue;
            }
            // A malformed record is skipped, not fatal: one bad object must not hide the history
            // behind it, and the old service made the same choice.
            if let Ok(value) = serde_json::from_str::<Value>(line) {
                entries.push(value);
            }
        }
    }
    entries.sort_by_key(|entry| {
        entry
            .get("timestamp")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned()
    });
    Ok(entries)
}

fn round4(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}

fn upstream_failure(error: objects::ObjectError) -> Failure {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"detail": format!("history store unavailable: {error}")})),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::state::AppState;
    use object_store::ObjectStore;
    use object_store::memory::InMemory;
    use std::sync::Arc;

    fn state_without_store() -> Shared {
        Arc::new(AppState::new(
            Config::from_lookup(&[]).expect("defaults parse"),
        ))
    }

    fn state_with_store() -> Shared {
        Arc::new(AppState {
            objects: Some(Arc::new(InMemory::new()) as Arc<dyn ObjectStore>),
            ..AppState::new(Config::from_lookup(&[]).expect("defaults parse"))
        })
    }

    fn with_tenant(tenant: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("x-tenant-id", tenant.parse().expect("header value"));
        headers
    }

    #[tokio::test]
    async fn status_reports_the_process_and_needs_no_tenant() {
        let Json(body) = status(State(state_without_store())).await;
        assert!(!body.running);
        assert_eq!(body.processed_total, 0);
    }

    #[tokio::test]
    async fn history_without_a_tenant_is_a_400() {
        let result = history(
            State(state_without_store()),
            HeaderMap::new(),
            Path("e1".to_owned()),
        )
        .await;
        let (status, _) = result.expect_err("refused");
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn history_without_a_store_is_empty_not_an_error() {
        let Json(body) = history(
            State(state_without_store()),
            with_tenant("t1"),
            Path("e1".to_owned()),
        )
        .await
        .expect("empty");
        assert_eq!(body["total"], 0);
        assert_eq!(body["entity_id"], "e1");
    }

    #[tokio::test]
    async fn an_entity_id_that_cannot_be_a_key_segment_is_a_400() {
        let result = history(
            State(state_without_store()),
            with_tenant("t1"),
            Path("../other".to_owned()),
        )
        .await;
        let (status, _) = result.expect_err("refused");
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn anomalies_compare_the_last_two_records() {
        let state = state_with_store();
        let object =
            serde_json::json!({"integrity_score": 0.9, "timestamp": "2026-01-01T00:00:00Z"});
        let later =
            serde_json::json!({"integrity_score": 0.4, "timestamp": "2026-01-02T00:00:00Z"});
        objects::put_json(
            state.objects.as_ref(),
            "history/t1/e1/20260101T000000Z-a.jsonl",
            &object,
            true,
        )
        .await
        .expect("written");
        objects::put_json(
            state.objects.as_ref(),
            "history/t1/e1/20260102T000000Z-b.jsonl",
            &later,
            true,
        )
        .await
        .expect("written");

        let Json(body) = anomalies(
            State(state),
            with_tenant("t1"),
            Query(AnomaliesQuery { threshold: None }),
        )
        .await
        .expect("answered");
        assert_eq!(body["total"], 1);
        assert_eq!(body["anomalies"][0]["entity_id"], "e1");
        assert_eq!(body["anomalies"][0]["delta"], -0.5);
    }

    #[tokio::test]
    async fn a_rise_is_not_an_anomaly() {
        let state = state_with_store();
        for (ts, score) in [("20260101T000000Z-a", 0.4), ("20260102T000000Z-b", 0.9)] {
            objects::put_json(
                state.objects.as_ref(),
                &format!("history/t1/e1/{ts}.jsonl"),
                &serde_json::json!({"integrity_score": score, "timestamp": ts}),
                true,
            )
            .await
            .expect("written");
        }
        let Json(body) = anomalies(
            State(state),
            with_tenant("t1"),
            Query(AnomaliesQuery { threshold: None }),
        )
        .await
        .expect("answered");
        assert_eq!(body["total"], 0);
    }
}
