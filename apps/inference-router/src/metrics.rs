//! `GET /metrics` — the dedicated Prometheus scrape listener, the 15 alert-keyed metric families,
//! their bounded label normalisers, and the boot-time zero pre-seed.
//!
//! The scrape is deliberately NOT on the API listener: the API document is the captured contract
//! (`contract/openapi.lock.json`), and a scrape endpoint is not a documented operation. The
//! listener is enabled by `INFERENCE_ROUTER_METRICS_PORT`; `0` (the default) means off.
//!
//! Names, label names and allow-lists are copied verbatim from the Python reference
//! (`inference_router/src/observability/metrics.py`) so the two services scrape identically.

use std::sync::OnceLock;
use std::time::Duration;

use ::metrics::{counter, gauge};
use axum::Router;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};

/// The process-wide recorder. `install_recorder` may run once per process; tests that build
/// several routers reuse the same registry.
fn handle() -> &'static PrometheusHandle {
    static HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();
    HANDLE.get_or_init(|| {
        PrometheusBuilder::new()
            .install_recorder()
            .expect("installing the Prometheus recorder once per process")
    })
}

/// Every alert-keyed family this service exports. A family absent from the scrape is
/// indistinguishable from a family that is legitimately zero, so `preseed` materialises all 15
/// at boot and `tests/metrics_families.rs` pins that.
pub const FAMILIES: [&str; 15] = [
    "inference_router_provider_attempts_total",
    "inference_router_provider_attempt_seconds_total",
    "inference_router_provider_errors_total",
    "inference_router_requests_received_total",
    "inference_router_tokens_total",
    "inference_router_cost_events_unlicensed_total",
    "inference_router_cost_usd_unlicensed_total",
    "inference_router_cost_tokens_unlicensed_total",
    "inference_router_cost_events_unattributed_total",
    "inference_router_cost_events_malformed_tenant_total",
    "inference_router_effective_tenant_total",
    "inference_router_project_id_header_total",
    "inference_router_license_introspection_total",
    "inference_router_license_introspection_by_caller_total",
    "inference_router_inference_call_write_failed_total",
];

/// The bounded `caller` label range — copied verbatim from the reference's `_ALLOWED_CALLERS`.
const ALLOWED_CALLERS: [&str; 15] = [
    "karma",
    "kag-graph",
    "aa-authoring",
    "canary",
    "replay",
    "chat-bridge",
    "chat-bridge-rmt",
    "chat-bridge-merqi",
    "core-app",
    "workers",
    "marketing",
    "kag-orchestrator",
    "w2v",
    "agent",
    "care-bus",
];

/// The bounded `worker` label range — copied verbatim from the reference's `_ALLOWED_WORKERS`.
const ALLOWED_WORKERS: [&str; 11] = [
    "principle_consistency",
    "decision_quality",
    "core-app",
    "reply_polish",
    "arch_description_quality",
    "requirement_quality_llm",
    "core-app-knowledge-expansion",
    "core-app-matcher",
    "marketing-entity-extractor",
    "core-app-slot-extraction",
    "core-app-engine-proposal",
];

const LABEL_UNKNOWN: &str = "unknown";
const LABEL_OTHER: &str = "other";

/// The one `service_key` every cost family on this path emits.
const SERVICE_KEY: &str = "inference-router";

/// Normalise a raw `X-App-Id` onto the bounded `caller` label.
///
/// Blank or absent → `"unknown"` (the header did not arrive); an allow-list member → itself; anything
/// else — including a literal `"unknown"` — → `"other"`. The literal is deliberately NOT folded into
/// the absent bucket, exactly as in the reference.
pub fn caller_label(raw: Option<&str>) -> String {
    bounded_label(raw, &ALLOWED_CALLERS)
}

/// Normalise a raw `X-Worker-Name` onto the bounded `worker` label. Same three-way rule as
/// [`caller_label`], against the worker allow-list.
pub fn worker_label(raw: Option<&str>) -> String {
    bounded_label(raw, &ALLOWED_WORKERS)
}

fn bounded_label(raw: Option<&str>, allowed: &[&str]) -> String {
    let value = raw.unwrap_or("").trim();
    if value.is_empty() {
        return LABEL_UNKNOWN.to_string();
    }
    if allowed.contains(&value) {
        value.to_string()
    } else {
        LABEL_OTHER.to_string()
    }
}

/// Book ONE provider attempt into the three alert-keyed attempt families.
///
/// Counts and seconds are incremented **together**, from one call, so no site can record an
/// attempt and forget its wall clock — a mean latency that silently dropped the failures is how
/// "6% of calls are empty" turns into "14.6% of the spend is empty". `reason` is supplied only
/// for a failed attempt (its classification) and for a blank completion; `empty_result` is an
/// observation, not an error, and books no reason.
///
/// Every label value here is bounded: the provider comes from the enabled catalog, the path and
/// outcome from the closed sets this crate defines, and the caller through [`caller_label`] — a
/// caller-supplied header never reaches a series raw.
pub fn record_attempt(
    provider: &str,
    path: &str,
    outcome: &str,
    caller: &str,
    latency: Duration,
    reason: Option<&str>,
) {
    let provider = provider.to_owned();
    let path = path.to_owned();
    let outcome = outcome.to_owned();
    let caller = caller.to_owned();
    counter!(
        "inference_router_provider_attempts_total",
        "provider" => provider.clone(),
        "path" => path.clone(),
        "outcome" => outcome.clone(),
        "caller" => caller.clone()
    )
    .increment(1);
    // ⚠️ A GAUGE, and only because the `metrics` crate's `Counter` is integer-only while this
    // series must carry fractional seconds. The reference increments a float counter; the
    // estate's rules only ever `increase()` this family over a range, and `increase()` on a
    // monotonic, never-reset series is the same number for a gauge as for a counter. Series name
    // and labels are the reference's own — only the exposed `# TYPE` differs.
    gauge!(
        "inference_router_provider_attempt_seconds_total",
        "provider" => provider.clone(),
        "path" => path.clone(),
        "outcome" => outcome,
        "caller" => caller
    )
    .increment(latency.as_secs_f64());
    if let Some(reason) = reason {
        counter!(
            "inference_router_provider_errors_total",
            "provider" => provider,
            "path" => path,
            "reason" => reason.to_owned()
        )
        .increment(1);
    }
}

/// Count one request that reached a completion route.
///
/// Booked by [`count_completion_request`], which wraps the matched route rather than the body
/// parse: the reference's middleware counts a POST on either completion path whatever the outcome,
/// so a malformed-body 422 and a licence-refused 401 are both counted here. (The project-id family
/// below is the one that skips a refused caller — it is booked after the licence gate.)
pub fn record_request_received() {
    counter!("inference_router_requests_received_total").increment(1);
}

/// Book the caller's `X-Project-Id` onto the admission family.
///
/// `outcome` is one of `absent` | `valid` | `malformed` and `caller` is already normalised through
/// [`caller_label`]; both are closed sets, so a caller-supplied header never reaches a series raw.
pub fn record_project_id(outcome: &str, caller: &str) {
    counter!(
        "inference_router_project_id_header_total",
        "outcome" => outcome.to_owned(),
        "caller" => caller.to_owned()
    )
    .increment(1);
}

/// The route layer over both completion paths: count a matched POST, then pass the request through
/// untouched.
///
/// The matched route set IS the scope — there is no path matching here — so the count can never
/// drift from what the router serves. A non-POST never reaches a completion handler, and is not
/// counted, exactly as in the reference.
pub async fn count_completion_request(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    if request.method() == axum::http::Method::POST {
        record_request_received();
    }
    next.run(request).await
}

/// Pre-seed every one of the 15 families at zero, so a scrape never has to distinguish "nothing
/// happened" from "this build never registered the instrument".
///
/// Provider-dependent series use the enabled catalog names (config-bounded, never traffic), and
/// every other value is a fixed literal from the reference's closed sets. Idempotent: a second call
/// — a router rebuild in a test, a host restart — is a lookup, never a reset.
pub fn preseed(providers: &[String]) {
    handle();

    // Label-free: the denominator for "zero requests reached the handler".
    counter!("inference_router_requests_received_total").increment(0);

    for provider in providers
        .iter()
        .map(String::as_str)
        .filter(|name| !name.is_empty())
    {
        counter!(
            "inference_router_provider_attempts_total",
            "provider" => provider.to_string(),
            "path" => "routed",
            "outcome" => "ok",
            "caller" => LABEL_UNKNOWN
        )
        .increment(0);
        // Same reason as `record_attempt`: the seconds family is a monotonic gauge.
        gauge!(
            "inference_router_provider_attempt_seconds_total",
            "provider" => provider.to_string(),
            "path" => "routed",
            "outcome" => "ok",
            "caller" => LABEL_UNKNOWN
        )
        .increment(0.0);
        counter!(
            "inference_router_provider_errors_total",
            "provider" => provider.to_string(),
            "path" => "routed",
            "reason" => "upstream_empty_response"
        )
        .increment(0);
        for kind in ["prompt", "completion"] {
            counter!(
                "inference_router_tokens_total",
                "service_key" => SERVICE_KEY,
                "provider" => provider.to_string(),
                "kind" => kind
            )
            .increment(0);
        }
        for family in [
            "inference_router_cost_events_unlicensed_total",
            "inference_router_cost_usd_unlicensed_total",
            "inference_router_cost_tokens_unlicensed_total",
        ] {
            counter!(
                family,
                "service_key" => SERVICE_KEY,
                "provider" => provider.to_string(),
                "caller" => LABEL_UNKNOWN,
                "worker" => LABEL_UNKNOWN
            )
            .increment(0);
        }
        counter!(
            "inference_router_cost_events_unattributed_total",
            "service_key" => SERVICE_KEY,
            "provider" => provider.to_string()
        )
        .increment(0);
        counter!(
            "inference_router_cost_events_malformed_tenant_total",
            "service_key" => SERVICE_KEY,
            "provider" => provider.to_string(),
            "reason" => "not_an_identifier_literal"
        )
        .increment(0);
    }

    for outcome in ["absent", "self", "honoured", "unvouched", "invalid"] {
        counter!("inference_router_effective_tenant_total", "outcome" => outcome).increment(0);
    }

    let callers = ALLOWED_CALLERS
        .iter()
        .copied()
        .chain([LABEL_UNKNOWN, LABEL_OTHER]);
    for outcome in ["absent", "valid", "malformed"] {
        for caller in callers.clone() {
            counter!(
                "inference_router_project_id_header_total",
                "outcome" => outcome,
                "caller" => caller
            )
            .increment(0);
        }
    }

    for outcome in [
        "resolved",
        "cached",
        "absent",
        "authorization_jwt_ignored",
        "sentinel",
        "not_licence_shaped",
        "wrong_app",
        "tenant_mismatch",
    ] {
        for enforced in ["false", "true"] {
            counter!(
                "inference_router_license_introspection_total",
                "outcome" => outcome,
                "enforced" => enforced
            )
            .increment(0);
        }
    }

    counter!(
        "inference_router_license_introspection_by_caller_total",
        "outcome" => "resolved",
        "caller" => LABEL_UNKNOWN,
        "worker" => LABEL_UNKNOWN
    )
    .increment(0);

    for reason in [
        "db_not_initialised",
        "db_error",
        "backpressure",
        "stream_capture",
    ] {
        counter!(
            "inference_router_inference_call_write_failed_total",
            "reason" => reason
        )
        .increment(0);
    }
}

/// `GET /metrics`. Always 200: the scrape must answer even when the database does not.
pub async fn metrics() -> Response {
    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain; version=0.0.4; charset=utf-8"),
        )],
        handle().render(),
    )
        .into_response()
}

/// The metrics listener's router: only `GET /metrics`, nothing else is reachable on this port.
///
/// Building the router installs the recorder, so families recorded after boot reach the scrape.
pub fn router() -> Router {
    handle();
    Router::new().route("/metrics", get(metrics))
}
