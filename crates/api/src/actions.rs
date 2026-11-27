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
use axum::extract::{FromRequest, FromRequestParts, Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header, request::Parts};
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use conveyor_actions::registry::registry;
use conveyor_actions::spec::sha256_hex;
use conveyor_actions::views::ActionManifest;
use conveyor_actions::{ActionError, Authority, InvokeRequest, Precondition, UsageContext};
use serde_json::Value;

use crate::error::ApiError;
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
    State(state): State<AppState>,
    caller: Caller,
    _licence: RequireLicence,
    headers: HeaderMap,
) -> Response {
    let mut entries = registry().manifest(caller.role);
    // S04 P1: the tenant's approved imported actions are entries of the SAME manifest, and the
    // ETag covers them too, so a refresh or a revocation changes the document.
    if let Ok(imported) = conveyor_actions::resolve::imported(&state.pool, caller.tenant).await {
        for action in imported {
            if action.permits_role(caller.role)
                && let Ok(value) = serde_json::to_value(action.manifest_entry())
            {
                entries.push(value);
            }
        }
    }
    let entries = Value::Array(entries);
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

/// The licence gate, as a parts extractor so it runs before the body is parsed.
///
/// ⛔ A STOP CONTROL MUST NOT DEPEND ON THE ENTITLEMENT IT MAY NEED TO STOP. The andon cord's
/// own two actions are exempt: while the cord is pulled `licence.sync` is the only writer of
/// `app_licence.checked_at`, and if the entitlement has gone stale (or licences is down) the
/// gate would refuse the very call that releases the cord — measured 2026-11-16, the only way
/// out was a database edit. Both actions still authenticate and still check `andon:write`.
pub struct Invoiced;

impl FromRequestParts<AppState> for Invoiced {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let name = parts.uri.path().split('/').nth(3).unwrap_or_default();
        if matches!(name, "andon.engage" | "andon.release") {
            return Ok(Invoiced);
        }
        let caller = Caller::from_request_parts(parts, state).await?;
        crate::extract::require_licence(state, caller.tenant).await?;
        Ok(Invoiced)
    }
}

/// The invocation body, deserialized by axum's [`Json`] extractor on the original request, so
/// the router's 16 MiB transport limit still applies. Callers have always sent the protocol's
/// JSON whatever the `Content-Type`, so the header is normalised before the framework parses.
/// Keeping the parse inside axum keeps deserialization out of this crate's own surface; the
/// rejection is the same RFC 9457 `invalid_json` problem the handler always returned.
pub struct ActionBody(Value);

impl FromRequest<AppState> for ActionBody {
    type Rejection = Response;

    async fn from_request(mut request: Request, state: &AppState) -> Result<Self, Self::Rejection> {
        let instance = request.uri().path().to_owned();
        let request_id = request
            .extensions()
            .get::<RequestId>()
            .map(|id| id.0.clone())
            .unwrap_or_default();
        request.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        match Json::<Value>::from_request(request, state).await {
            Ok(Json(value)) => Ok(ActionBody(value)),
            Err(rejection) => {
                let error = ActionError::invalid_input(vec![conveyor_actions::FieldError {
                    field: "body".into(),
                    code: "invalid_json".into(),
                    detail: rejection.body_text(),
                }]);
                Err(problem(&error, &instance, &request_id))
            }
        }
    }
}

/// The invocation body `{"input": …, "approval_id": "…"}`. Parsed by hand so every failure is
/// RFC 9457.
///
/// `approval_id` is a property of the CALL, not of the payload (#282): a destructive action whose
/// spec declares an approval target is refused without it, and every other action ignores it. It
/// lives beside `input` rather than inside it because every action input is
/// `deny_unknown_fields` — a field the engine adds could not be added to 27 schemas at once.
fn parse_body(value: Value) -> Result<(Value, Option<uuid::Uuid>), ActionError> {
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
    _licensed: Invoiced,
    body: ActionBody,
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

    let (input, approval_id) = match parse_body(body.0) {
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
        credential_alias: None,
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
        // S16 W7: an ordinary HTTP call still names who called. `Other` is the one kind the
        // schema allows to carry no join key; the request id is the transport correlation, not
        // a ledger join. The engine records it only when the action captures usage.
        usage: Some(UsageContext {
            caller_kind: conveyor_domain::inference::CallerKind::Other,
            run_id: None,
            run_step_id: None,
            conversation_turn_id: None,
            eval_run_id: None,
            prompt_ref: None,
            prompt_version: None,
            prompt_sha256: None,
            correlation_id: Some(request_id.clone()),
        }),
    };
    // ── async submit (#579) ──────────────────────────────────────────────────────────────────
    // A caller behind Cloudflare cannot hold one response open past ~100 s, and this estate's
    // answer is submit → 202 + id → poll (w2v, agent, mcp, coding all do it). `precheck` is the
    // SAME first stage `invoke` runs, so an invalid input or a forbidden caller still fails
    // HERE, synchronously: the work is never queued and nobody polls to learn their request was
    // bad.
    if wants_async(&headers) {
        if let Err(error) =
            conveyor_actions::invoke::precheck(&state.pool, registry(), caller.tenant, &request)
                .await
        {
            return problem(&error, &instance, &request_id);
        }
        let input_hash = conveyor_store::idempotency::request_hash(&request.input);
        let reserved = conveyor_store::async_invocation::start(
            &state.pool,
            caller.tenant,
            &name,
            request.actor_id,
            &input_hash,
            request.idempotency_key.as_deref(),
        )
        .await;
        let invocation_id = match reserved {
            Ok((id, _created)) => id,
            Err(error) => {
                tracing::error!(%error, action = %name, "reserve an async invocation");
                return ApiError::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal",
                    "the invocation could not be started",
                )
                .into_response();
            }
        };
        let pool = state.pool.clone();
        let tenant = caller.tenant;
        let job = request.clone();
        tokio::spawn(async move {
            let settled = conveyor_actions::invoke(&pool, registry(), tenant, job).await;
            let (outcome, status, body) = match settled {
                Ok(outcome) => ("done", outcome.status as i32, outcome.body),
                Err(error) => ("failed", error.status as i32, error.problem(None, None)),
            };
            if let Err(error) = conveyor_store::async_invocation::finish(
                &pool,
                tenant,
                invocation_id,
                outcome,
                status,
                &body,
            )
            .await
            {
                // The caller will keep seeing `202` until the sweep removes the row. Loud,
                // because the only way out of that state is an operator.
                tracing::error!(%error, %invocation_id, "record an async invocation");
            }
        });
        return (
            StatusCode::ACCEPTED,
            [(
                header::LOCATION,
                format!("/api/invocations/{invocation_id}"),
            )],
            Json(serde_json::json!({
                "invocation_id": invocation_id,
                "status": "running",
            })),
        )
            .into_response();
    }

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

/// Does the caller want the work queued rather than waited for?
///
/// `Prefer: respond-async` (RFC 7240), matched per preference token and case-insensitively, so
/// `Prefer: wait=30, respond-async` counts and `respond-asyncs` does not.
fn wants_async(headers: &HeaderMap) -> bool {
    headers
        .get_all("prefer")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .any(|token| token.trim().eq_ignore_ascii_case("respond-async"))
}
