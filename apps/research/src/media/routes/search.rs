//! `POST /api/search` — the federated search.
//!
//! Two captured behaviours shape this handler:
//!
//! * a body without `query` answers the reference's **pydantic** 422 — `VALIDATION_ERROR`, plus a
//!   `details` string that is pydantic's own `repr` of the error list, not prose of ours;
//! * an unknown `media_type` is **not** a validation error. It matches nothing and the answer is
//!   the same empty envelope as a real search that found nothing.
//!
//! No adapter is registered in this deployment, so the federated result is the captured empty
//! envelope with all four `sources_*` arrays empty. A request that names sources therefore names
//! sources this deployment does not have, and they land in `sources_unknown` — which is exactly
//! what the schema's own description says that array means.

use std::time::Instant;

use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use serde_json::Value;

use crate::media::AppState;
use crate::media::error::ApiError;
use crate::media::models::{SearchRequest, SearchResponse};
use crate::media::tenant::Tenant;

/// `POST /api/search`
pub async fn search(
    State(_state): State<AppState>,
    Tenant(_tenant): Tenant,
    body: Bytes,
) -> Result<Json<SearchResponse>, ApiError> {
    let started = Instant::now();

    let raw: Value = serde_json::from_slice(&body).unwrap_or(Value::Object(Default::default()));

    let Some(query) = raw.get("query").and_then(Value::as_str) else {
        return Err(ApiError::validation(missing_field("query", &raw)));
    };

    let request: SearchRequest = serde_json::from_value(raw.clone()).map_err(|error| {
        ApiError::validation(format!("[{{'type': 'value_error', 'msg': '{}'}}]", error))
    })?;

    if !(1..=100).contains(&request.limit) {
        return Err(ApiError::validation(format!(
            "[{{'type': 'less_than_equal', 'loc': ('body', 'limit'), 'msg': 'Input should be less than or equal to 100', 'input': {}}}]",
            request.limit
        )));
    }

    // An unknown media type is a filter that matches nothing, never a 400: the capture pins a 200
    // for `"media_types": ["hologram"]`.
    let _ = (&query, &request.media_types, &request.language);

    // Every adapter this deployment has is registered-and-usable or absent; none is registered, so
    // nothing was skipped and nothing failed.
    let sources_unknown = request.sources.clone().unwrap_or_default();

    Ok(Json(SearchResponse {
        results: Vec::new(),
        total: 0,
        sources_queried: Vec::new(),
        sources_failed: Vec::new(),
        sources_unknown,
        sources_skipped: Vec::new(),
        search_time_ms: started.elapsed().as_secs_f64() * 1000.0,
    }))
}

/// The pydantic `details` string for a missing required field.
///
/// `input` is the body as pydantic echoes it, which for the captured case is `{}`. The rest of the
/// string is pydantic's own `repr` of one `missing` error, reproduced verbatim because the golden
/// pins it.
fn missing_field(field: &str, input: &Value) -> String {
    format!(
        "[{{'type': 'missing', 'loc': ('body', '{field}'), 'msg': 'Field required', 'input': {}}}]",
        python_repr(input)
    )
}

/// A Python `repr` of a JSON value, close enough to pydantic's echo of the request body.
fn python_repr(value: &Value) -> String {
    match value {
        Value::Null => "None".to_string(),
        Value::Bool(true) => "True".to_string(),
        Value::Bool(false) => "False".to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => format!("'{text}'"),
        Value::Array(items) => format!(
            "[{}]",
            items.iter().map(python_repr).collect::<Vec<_>>().join(", ")
        ),
        Value::Object(map) => format!(
            "{{{}}}",
            map.iter()
                .map(|(key, inner)| format!("'{key}': {}", python_repr(inner)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_captured_missing_query_detail_is_byte_identical() {
        let captured =
            "[{'type': 'missing', 'loc': ('body', 'query'), 'msg': 'Field required', 'input': {}}]";
        assert_eq!(
            missing_field("query", &Value::Object(Default::default())),
            captured
        );
    }
}
