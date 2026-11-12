#![forbid(unsafe_code)]
//! The action HTTP surface: `GET /api/actions` and the per-action invocation routes.
//!
//! One literal `POST /api/actions/<name>/invocations` route is registered per action (from
//! the registry — S4-D2), plus the `spec_exempt` fallback `/api/actions/{name}/invocations`
//! that answers `action-not-found`. Both land in [`invoke_http`]: the action name is the
//! path segment the client sent, and the registry lookup is the only authority, so a
//! literal route and the fallback cannot diverge.
//!
//! The licence gate ([`RequireLicence`]) wraps both the manifest and every invocation: a
//! blocked tenant may still learn who it is (`/api/me` is exempt), but not call anything.

use std::time::Instant;

use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use conveyor_actions::registry::registry;
use conveyor_actions::spec::sha256_hex;
use conveyor_actions::views::ActionManifest;
use conveyor_actions::{ActionError, Authority, InvokeRequest, Precondition};
use serde_json::Value;

use crate::extract::{Caller, RequireLicence};
use crate::request_id::RequestId;
use crate::state::AppState;

/// The manifest's ETag: sha256 over the role-filtered entries, so it is stable across
/// calls (the document's `generated_at` is deliberately not part of it).
pub fn manifest_etag(entries: &Value) -> String {
    format!(
        "\"{}\"",
        sha256_hex(&serde_json::to_vec(entries).unwrap_or_default())
    )
}

/// `GET /api/actions`.
///
/// Open question 1's default is applied: the manifest is role-filtered, so a viewer never
/// learns the name of an action it cannot call. `If-None-Match` matching the current ETag
/// answers `304` with no body.
pub async fn manifest(
    State(_state): State<AppState>,
    caller: Caller,
    _licence: RequireLicence,
    headers: HeaderMap,
) -> Response {
    let entries = Value::Array(registry().manifest(caller.role));
    let etag = manifest_etag(&entries);
    if headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.trim() == etag)
    {
        return (
            StatusCode::NOT_MODIFIED,
            [(header::ETAG, etag)],
            axum::body::Body::empty(),
        )
            .into_response();
    }
    let actions = serde_json::from_value(entries).unwrap_or_default();
    let document = ActionManifest {
        actions,
        generated_at: Utc::now(),
    };
    (
        StatusCode::OK,
        [
            (header::ETAG, etag),
            (header::CACHE_CONTROL, "no-cache".to_owned()),
        ],
        Json(document),
    )
        .into_response()
}

/// The invocation body `{"input": …}`. Parsed by hand so every failure is RFC 9457.
fn parse_body(bytes: &Bytes) -> Result<Value, ActionError> {
    let value: Value = serde_json::from_slice(bytes).map_err(|error| {
        ActionError::invalid_input(vec![conveyor_actions::FieldError {
            field: "body".into(),
            code: "invalid_json".into(),
            detail: error.to_string(),
        }])
    })?;
    let Some(input) = value.get("input") else {
        return Err(ActionError::invalid_input(vec![
            conveyor_actions::FieldError {
                field: "input".into(),
                code: "required".into(),
                detail: "the invocation body is {\"input\": …}".into(),
            },
        ]));
    };
    Ok(input.clone())
}

/// `If-Match: "<version>"` (CONTRACT §3).
///
/// A bare integer is a row `version`. Any other value is carried through verbatim as an
/// opaque ETag: S07's `outline` / `outline_node` have no `version` column, so their
/// precondition is the row's `updated_at` rendered as RFC 3339 and only the action that
/// owns the table can decide whether it matches. The route does not guess — a malformed
/// value that matches no row is the action's `412`, not a `400` here.
fn parse_if_match(headers: &HeaderMap) -> Result<Option<Precondition>, ActionError> {
    let Some(raw) = headers.get(header::IF_MATCH) else {
        return Ok(None);
    };
    let raw = raw.to_str().map_err(|_| {
        ActionError::new(400, "bad_request", "If-Match is not a valid header value")
    })?;
    let trimmed = raw.trim().trim_start_matches('"').trim_end_matches('"');
    if trimmed == "*" {
        return Ok(None);
    }
    if trimmed.is_empty() {
        return Err(ActionError::new(
            400,
            "bad_request",
            "If-Match must be a version number or an ETag",
        ));
    }
    Ok(Some(match trimmed.parse::<i64>() {
        Ok(version) => Precondition::Version(version),
        Err(_) => Precondition::Tag(trimmed.to_owned()),
    }))
}

fn problem(error: &ActionError, instance: &str, request_id: &str) -> Response {
    let body = error.problem(Some(instance), Some(request_id));
    (
        StatusCode::from_u16(error.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
        [(header::CONTENT_TYPE, "application/problem+json")],
        Json(body),
    )
        .into_response()
}

/// `POST /api/actions/<name>/invocations`.
///
/// The name is read from the path, so the literal routes and the fallback run this one
/// handler; an unknown name is `action-not-found`.
pub async fn invoke_http(
    State(state): State<AppState>,
    caller: Caller,
    _licence: RequireLicence,
    uri: axum::http::Uri,
    request_id: axum::Extension<RequestId>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let request_id = request_id.0.0.clone();
    // The name is the path segment the client sent: a literal route has no `{name}`
    // placeholder, and this is what keeps the literal routes and the fallback on ONE
    // handler (the registry lookup is the only authority).
    let name = uri.path().split('/').nth(3).unwrap_or_default().to_owned();
    let instance = format!("/api/actions/{name}/invocations");
    let trace_id = headers
        .get("traceparent")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split('-').nth(1))
        .map(str::to_owned);

    let input = match parse_body(&body) {
        Ok(input) => input,
        Err(error) => return problem(&error, &instance, &request_id),
    };
    let precondition = match parse_if_match(&headers) {
        Ok(precondition) => precondition,
        Err(error) => return problem(&error, &instance, &request_id),
    };
    let idempotency_key = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);

    let request = InvokeRequest {
        action: name.clone(),
        input,
        precondition,
        idempotency_key,
        actor_id: caller.principal.user_id.as_uuid(),
        actor_kind: caller.principal.kind.as_actor_kind().to_owned(),
        role: caller.role,
        policy: caller.policy.clone(),
        authority: Authority::of(&caller),
        request_id: Some(request_id.clone()),
        trace_id,
        depth: 0,
        app_key: state.config.app_key.clone(),
        usage: None,
    };
    let started = Instant::now();
    match conveyor_actions::invoke(&state.pool, registry(), caller.tenant, request).await {
        Ok(outcome) => {
            state
                .metrics
                .observe_action(&name, outcome.status, started.elapsed());
            let status = StatusCode::from_u16(outcome.status).unwrap_or(StatusCode::OK);
            // `204` carries no body by definition (RFC 9110); every other success carries
            // the one `{output, request_id, duration_ms}` envelope.
            let mut response = if status == StatusCode::NO_CONTENT {
                status.into_response()
            } else {
                (status, Json(outcome.body)).into_response()
            };
            if let Some(etag) = outcome.etag
                && let Ok(value) = axum::http::HeaderValue::from_str(&etag)
            {
                response.headers_mut().insert(header::ETAG, value);
            }
            response
        }
        Err(error) => {
            state
                .metrics
                .observe_action(&name, error.status, started.elapsed());
            problem(&error, &instance, &request_id)
        }
    }
}
