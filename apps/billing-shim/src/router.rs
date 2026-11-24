//! The HTTP surface: `POST /api/usage/rollup` and `GET /health`.
//!
//! The rollup handler validates what it can see locally (dates, dimensions, the tenant
//! header), forwards the translated query, and reshapes the upstream rows into the fixed
//! shim contract. It adds no dimensions, no rows and no totals.

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::{get, post};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Number, Value};

use crate::error::ShimError;
use crate::upstream::{BillingService, Breakdown, RollupQuery};

/// The one rollup route.
pub const ROLLUP_PATH: &str = "/api/usage/rollup";

/// The dimensions the contract exposes, in their default order. The upstream accepts exactly
/// these names (plus `funding`, which this contract deliberately does not expose).
pub const DEFAULT_GROUP_BY: [&str; 6] = ["day", "project", "actor", "app", "provider", "model"];

#[derive(Debug, Clone)]
pub struct AppState {
    pub billing: BillingService,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route(ROLLUP_PATH, post(usage_rollup))
        .with_state(state)
}

#[derive(Debug, Serialize)]
struct Health {
    status: &'static str,
}

/// `GET /health` — process liveness only; it never touches the upstream.
async fn health() -> Json<Health> {
    Json(Health { status: "ok" })
}

/// The fixed request body.
#[derive(Debug, Deserialize)]
pub struct RollupRequest {
    pub since: String,
    pub until: String,
    #[serde(default)]
    pub group_by: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
struct RollupResponse {
    rows: Vec<RollupRow>,
    total_usd: Number,
    source: &'static str,
}

#[derive(Debug, Serialize)]
struct RollupRow {
    dimensions: Map<String, Value>,
    cost_usd: Number,
}

async fn usage_rollup(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<RollupRequest>,
) -> Result<Json<RollupResponse>, ShimError> {
    let (from, to) = window(&request)?;
    let group_by = selected_dimensions(request.group_by.as_deref())?;
    let tenant_id = tenant(&headers)?;
    let breakdown = state
        .billing
        .rollup(&RollupQuery {
            from,
            to,
            group_by: group_by.clone(),
            tenant_id,
        })
        .await?;
    Ok(Json(translate(&group_by, breakdown)?))
}

/// `since`/`until` are `YYYY-MM-DD`; normalize them so the upstream receives the canonical
/// spelling, and refuse an inverted window here rather than as a misleading 502.
fn window(request: &RollupRequest) -> Result<(String, String), ShimError> {
    let since = parse_day("since", &request.since)?;
    let until = parse_day("until", &request.until)?;
    if until < since {
        return Err(ShimError::Invalid(
            "until must be on or after since".to_owned(),
        ));
    }
    Ok((
        since.format("%Y-%m-%d").to_string(),
        until.format("%Y-%m-%d").to_string(),
    ))
}

fn parse_day(field: &str, raw: &str) -> Result<NaiveDate, ShimError> {
    NaiveDate::parse_from_str(raw.trim(), "%Y-%m-%d")
        .map_err(|_| ShimError::Invalid(format!("{field} must be YYYY-MM-DD, got {raw:?}")))
}

/// Default to the six contract dimensions; otherwise keep the caller's order, drop duplicates
/// and refuse names this contract does not expose. An explicit empty list is the upstream's
/// ungrouped grand total and is passed through as an empty CSV.
fn selected_dimensions(requested: Option<&[String]>) -> Result<Vec<String>, ShimError> {
    let requested: Vec<&str> = match requested {
        Some(values) => values
            .iter()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .collect(),
        None => DEFAULT_GROUP_BY.to_vec(),
    };
    let mut dimensions: Vec<String> = Vec::with_capacity(requested.len());
    let mut unknown: Vec<&str> = Vec::new();
    for dimension in requested {
        if !DEFAULT_GROUP_BY.contains(&dimension) {
            unknown.push(dimension);
        } else if !dimensions.iter().any(|kept| kept.as_str() == dimension) {
            dimensions.push(dimension.to_owned());
        }
    }
    if !unknown.is_empty() {
        return Err(ShimError::Invalid(format!(
            "unknown group_by dimension(s) {unknown:?}; allowed: {:?}",
            DEFAULT_GROUP_BY
        )));
    }
    Ok(dimensions)
}

/// The upstream is tenant-scoped; the shim carries the caller's tenant through unchanged.
fn tenant(headers: &HeaderMap) -> Result<String, ShimError> {
    headers
        .get("x-tenant-id")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or(ShimError::TenantRequired)
}

/// Reshape the upstream breakdown into the shim's response. A requested dimension missing
/// from an upstream row is a contract breach (502) — it is never filled with null.
fn translate(dimensions: &[String], breakdown: Breakdown) -> Result<RollupResponse, ShimError> {
    let mut rows = Vec::with_capacity(breakdown.rows.len());
    for row in breakdown.rows {
        let mut dims = Map::with_capacity(dimensions.len());
        for dimension in dimensions {
            let value = row.dims.get(dimension).ok_or_else(|| {
                ShimError::upstream(format!(
                    "billing-service omitted the requested dimension {dimension:?} from a row"
                ))
            })?;
            dims.insert(
                dimension.clone(),
                match value {
                    Some(value) => Value::String(value.clone()),
                    None => Value::Null,
                },
            );
        }
        rows.push(RollupRow {
            dimensions: dims,
            cost_usd: row.cost_usd,
        });
    }
    Ok(RollupResponse {
        rows,
        total_usd: breakdown.total_cost_usd,
        source: crate::SOURCE,
    })
}
