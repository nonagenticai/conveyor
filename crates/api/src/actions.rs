#![forbid(unsafe_code)]
//! The action HTTP surface: `GET /api/actions` and the per-action invocation routes.
//!
//! One literal `POST /api/actions/<name>/invocations` route is registered per action (from
//! the registry — S4-D2), plus the `spec_exempt` fallback `/api/actions/{name}/invocations`
//! that answers `action-not-found`. Both land in [`invoke_http`]: the action name is the
//! path segment the client sent, and the registry lookup is the only authority, so a
//! literal route and the fallback cannot diverge.
//!
//! The licence gate ([`RequireLicence`]) wraps the manifest and every invocation **except the
//! andon cord's own two actions**: a blocked tenant may still learn who it is (`/api/me` is
//! exempt) and may still stop or restart its own work — a stop control must not depend on the
//! entitlement it may need to stop. Both andon actions stay authenticated and scope-checked.

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

/// The invocation body `{"input": …, "approval_id": "…"}`. Parsed by hand so every failure is
/// RFC 9457.
///
/// `approval_id` is a property of the CALL, not of the payload (#282): a destructive action whose
/// spec declares an approval target is refused without it, and every other action ignores it. It
/// lives beside `input` rather than inside it because every action input is
/// `deny_unknown_fields` — a field the engine adds could not be added to 27 schemas at once.
fn parse_body(bytes: &Bytes) -> Result<(Value, Option<uuid::Uuid>), ActionError> {
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
    let approval_id = match value.get("approval_id") {
        None | Some(Value::Null) => None,
        Some(Value::String(text)) => Some(text.parse::<uuid::Uuid>().map_err(|_| {
            ActionError::invalid_input(vec![conveyor_actions::FieldError {
                field: "approval_id".into(),
                code: "invalid_uuid".into(),
                detail: "approval_id must be the uuid of an approved approval".into(),
            }])
        })?),
        Some(_) => {
            return Err(ActionError::invalid_input(vec![
                conveyor_actions::FieldError {
                    field: "approval_id".into(),
                    code: "invalid_type".into(),
                    detail: "approval_id must be a uuid string".into(),
                },
            ]));
        }
    };
    Ok((input.clone(), approval_id))
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
    // ⛔ A STOP CONTROL MUST NOT DEPEND ON THE ENTITLEMENT IT MAY NEED TO STOP. The cord's own
    // two actions are exempt from the licence gate: while the cord is pulled `licence.sync` is
    // the only writer of `app_licence.checked_at`, and if the entitlement has gone stale (or
    // licences is down) the gate would refuse the very call that releases the cord — measured
    // 2026-11-16, the only way out was a database edit. Both actions still authenticate the
    // caller and still check `andon:write`; only this gate is skipped.
    if !matches!(name.as_str(), "andon.engage" | "andon.release")
        && let Err(error) = crate::extract::require_licence(&state, caller.tenant).await
    {
        return error.into_response();
    }
    let instance = format!("/api/actions/{name}/invocations");
    let trace_id = headers
        .get("traceparent")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split('-').nth(1))
        .map(str::to_owned);

    let (input, approval_id) = match parse_body(&body) {
        Ok(parsed) => parsed,
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
        approval_id,
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
