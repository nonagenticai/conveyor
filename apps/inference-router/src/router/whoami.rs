//! `GET /api/whoami` — what the licence server says about the caller's own token.
//!
//! ⭐ WHY THIS EXISTS: a tenant pastes its inference-router token into worker_management, whose
//! key test needs a cheap, side-effect-free way to ask "is this token good for inference-router,
//! and whose is it?". A completion would spend tokens and be metered; this does neither.
//!
//! * It introspects in EVERY mode, including `off` — it is a question about the token, not a gate.
//! * It is FRESH, not cached: a token revoked at licences reads 401 here at once.
//! * It is NOT metered: no cost event, no ledger row.
//! * It is RATE-LIMITED per CLIENT HOST, never per token (a token-keyed limit is bypassed by
//!   rotating tokens). The bucket is process-local, so every caller behind one gateway pod
//!   shares it — the reference's explicit trade.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::{ConnectInfo, Request, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::auth::{self, Verdict};
use crate::contract::schemas::WhoamiResponse;
use crate::routing::Refusal;
use crate::state::AppState;

const WINDOW: Duration = Duration::from_secs(60);
const MAX_TRACKED_CALLERS: usize = 10_000;

struct Window {
    started: Instant,
    count: u32,
}

fn limiter() -> &'static Mutex<HashMap<String, Window>> {
    static LIMITER: OnceLock<Mutex<HashMap<String, Window>>> = OnceLock::new();
    LIMITER.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The reference's fixed one-minute window, decided from the STORED start so a caller polling
/// every 30 s still sees its count reset.
fn allow_at(key: &str, limit: u32, now: Instant) -> bool {
    let mut hits = limiter().lock().expect("whoami limiter");
    if !hits.contains_key(key) && hits.len() >= MAX_TRACKED_CALLERS {
        hits.retain(|_, window| now.duration_since(window.started) < WINDOW);
    }
    let window = hits.entry(key.to_owned()).or_insert(Window {
        started: now,
        count: 0,
    });
    if now.duration_since(window.started) >= WINDOW {
        window.started = now;
        window.count = 0;
    }
    window.count += 1;
    window.count <= limit
}

fn caller_key(peer: Option<SocketAddr>) -> String {
    format!(
        "host:{}",
        peer.map(|address| address.ip().to_string())
            .unwrap_or_default()
    )
}

#[utoipa::path(
    get,
    path = "/api/whoami",
    tag = "whoami",
    summary = "Resolve the caller's licence token",
    responses((status = 200, body = WhoamiResponse))
)]
pub async fn whoami(State(state): State<AppState>, request: Request) -> Response {
    let headers = request.headers();
    // The peer address is present only when the server was built with connect info; tests and
    // a service behind an in-process router share the empty bucket, exactly as intended.
    let peer = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0);
    // ⛔ THE HOST, NEVER THE TOKEN — see the module docstring.
    if !allow_at(
        &caller_key(peer),
        state.config().whoami_rate_limit_per_minute,
        Instant::now(),
    ) {
        let mut refusal = Refusal::detail(
            StatusCode::TOO_MANY_REQUESTS,
            "whoami rate limit exceeded",
            "whoami_rate_limited",
        );
        refusal
            .headers
            .push(("Retry-After", WINDOW.as_secs().to_string()));
        return refusal.into_response();
    }

    let presentation = auth::read_license_token(headers);
    let Some(token) = presentation.token else {
        return unauthorized();
    };
    match state.introspector().resolve_fresh(&token).await {
        Verdict::Resolved(licence) => Json(WhoamiResponse {
            active: true,
            app: licence.app,
            expires_at: licence.expires_at,
            license_id: licence.license_id,
            tenant_id: licence.tenant_id,
        })
        .into_response(),
        Verdict::Unavailable => Refusal::detail(
            StatusCode::SERVICE_UNAVAILABLE,
            "Licence introspection unavailable",
            "licence_unavailable",
        )
        .into_response(),
        Verdict::Denied => unauthorized(),
    }
}

fn unauthorized() -> Response {
    let mut refusal = Refusal::detail(
        StatusCode::UNAUTHORIZED,
        "No licence token presented, or the licence server did not vouch for it",
        "whoami_unlicensed",
    );
    refusal
        .headers
        .push(("WWW-Authenticate", "Bearer".to_owned()));
    refusal.into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_is_per_key_and_resets_after_a_minute() {
        let start = Instant::now();
        // A unique key per test: the limiter is process-global on purpose.
        let key = "host:198.51.100.7";
        assert!(allow_at(key, 2, start));
        assert!(allow_at(key, 2, start));
        assert!(!allow_at(key, 2, start), "third call in the window is over");
        // Another host has its own bucket.
        assert!(allow_at("host:198.51.100.8", 2, start));
        // The window is decided from the stored start, not the last write.
        let later = start + WINDOW + Duration::from_secs(1);
        assert!(allow_at(key, 2, later));
    }

    #[test]
    fn the_key_is_the_host_never_the_token() {
        let a: SocketAddr = "10.0.0.1:1234".parse().expect("address");
        let b: SocketAddr = "10.0.0.2:1234".parse().expect("address");
        assert_eq!(caller_key(Some(a)), "host:10.0.0.1");
        assert_eq!(caller_key(Some(b)), "host:10.0.0.2");
        assert_eq!(caller_key(None), "host:");
    }
}
