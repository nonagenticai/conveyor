//! The Prometheus recorder and the scrape endpoint.
//!
//! One `OnceLock` handle per process (the template/`strata_rag` pattern), so every test and
//! every clone records into the same registry. A gauge that queries the database is
//! evaluated under an explicit timeout: a scrape must answer `db_up 0` promptly during an
//! outage, never hang until the client gives up and report the whole service down.

use std::sync::OnceLock;
use std::time::Duration;

use axum::extract::State;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use metrics::{describe_counter, describe_gauge, describe_histogram};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};

use crate::state::AppState;

/// How long a scrape waits for the database before reporting it down.
pub const GAUGE_TIMEOUT: Duration = Duration::from_secs(1);

/// The process-wide recorder handle.
#[derive(Clone, Debug)]
pub struct Metrics {
    handle: PrometheusHandle,
}

impl Metrics {
    /// Install the recorder once and return a handle to it. Calling this again (a test
    /// binary, a second state) reuses the same registry.
    pub fn install() -> Self {
        static HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();
        let handle = HANDLE.get_or_init(|| {
            describe_gauge!(
                "conveyor_db_up",
                "1 when the database answered the scrape probe inside the budget, else 0"
            );
            describe_gauge!(
                "conveyor_outbox_seq",
                "The highest committed event.seq across this process's tenants (0 when none)"
            );
            describe_gauge!(
                "conveyor_andon_engaged",
                "1 when the tenant's andon cord is engaged, else 0"
            );
            describe_gauge!(
                "conveyor_andon_engaged_at_seconds",
                "Seconds since the tenant's cord was engaged (0 when it is not engaged)"
            );
            describe_counter!(
                "conveyor_http_requests_total",
                "HTTP requests by method and final status"
            );
            describe_histogram!(
                "conveyor_http_request_duration_seconds",
                "HTTP request latency by method"
            );
            describe_counter!(
                "conveyor_action_invocations_total",
                "Action invocations by action name and final status"
            );
            describe_histogram!(
                "conveyor_action_invocation_duration_seconds",
                "Action invocation latency by action name"
            );
            PrometheusBuilder::new()
                .install_recorder()
                .expect("installing the Prometheus recorder once per process")
        });
        Self {
            handle: handle.clone(),
        }
    }

    /// Record one action invocation. The label set is the fixed registry, so it is bounded.
    pub fn observe_action(&self, action: &str, status: u16, elapsed: Duration) {
        metrics::counter!(
            "conveyor_action_invocations_total",
            "action" => action.to_owned(),
            "status" => status.to_string()
        )
        .increment(1);
        metrics::histogram!(
            "conveyor_action_invocation_duration_seconds",
            "action" => action.to_owned()
        )
        .record(elapsed.as_secs_f64());
    }

    /// The exposition text.
    pub fn render(&self) -> String {
        self.handle.render()
    }
}

/// `GET /metrics`. Always 200: metrics exist precisely when the database does not.
pub async fn metrics(State(state): State<AppState>) -> Response {
    let probe = tokio::time::timeout(GAUGE_TIMEOUT, conveyor_store::ping(&state.pool)).await;
    let up = matches!(probe, Ok(Ok(())));
    metrics::gauge!("conveyor_db_up").set(if up { 1.0 } else { 0.0 });

    // The outbox gauge is evaluated under the same budget. It is computed per tenant in a
    // scoped transaction because `event` is RLS'd: an unscoped `max(seq)` is 0 rows for the
    // runtime role, and reading the sequence's `last_value` instead would report a
    // rolled-back insert as progress (the sequence is not transactional).
    if tokio::time::timeout(GAUGE_TIMEOUT, outbox_gauge(&state))
        .await
        .is_err()
    {
        tracing::warn!("outbox gauge exceeded the scrape budget; keeping the last value");
    }

    // The andon gauges are the estate-wide stop's safety signal (S06 §5.5, decision 5). They
    // are read at scrape time from `andon_state`, never from a request-driven cache: fr's
    // gauge had no sample at all after a restore, so S14 alarms on absence as well as on 1.
    // A failure inside `gauge_snapshot` omits the affected samples (it warns) and a timeout
    // leaves every previous value in place — a scrape must answer during an outage.
    if tokio::time::timeout(GAUGE_TIMEOUT, andon_gauge(&state))
        .await
        .is_err()
    {
        tracing::warn!("andon gauge exceeded the scrape budget; keeping the last value");
    }

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain; version=0.0.4; charset=utf-8"),
        )],
        state.metrics.render(),
    )
        .into_response()
}

/// The highest committed `event.seq` visible across the tenants this process serves.
///
/// One scoped read per tenant: a fleet with many tenants would make this expensive, and
/// that is deliberate — the alternative (a `BYPASSRLS` reader or the sequence's
/// `last_value`) either breaks the tenancy model or lies after a rollback. A future
/// per-tenant gauge with a bounded label set is the fix if the tenant count grows.
async fn outbox_gauge(state: &AppState) {
    let tenants = match conveyor_store::tenants::active_ids(&state.pool).await {
        Ok(tenants) => tenants,
        Err(error) => {
            tracing::warn!(%error, "outbox gauge: tenant enumeration failed");
            return;
        }
    };
    let mut highest = 0i64;
    for tenant in tenants {
        match conveyor_store::outbox::max_seq(&state.pool, tenant).await {
            Ok(Some(seq)) => highest = highest.max(seq),
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(%error, tenant = %tenant, "outbox gauge: read failed");
                return;
            }
        }
    }
    metrics::gauge!("conveyor_outbox_seq").set(highest as f64);
}

/// The per-tenant andon state, for [`metrics`]'s scrape path.
///
/// One scoped read per active tenant lives in `store::andon::gauge_snapshot` (it is RLS'd;
/// an unscoped read returns zero rows for the runtime role and would look exactly like "no
/// tenant has ever pulled the cord"). A tenant with no row is `engaged = 0`, so the sample
/// always exists; a tenant whose read failed is omitted and keeps its previous value, which
/// is what S14's `absent()` alarm is for.
async fn andon_gauge(state: &AppState) {
    let now = Utc::now();
    for (tenant, engaged, engaged_at) in conveyor_store::andon::gauge_snapshot(&state.pool).await {
        let label = tenant.to_string();
        metrics::gauge!("conveyor_andon_engaged", "tenant_id" => label.clone()).set(if engaged {
            1.0
        } else {
            0.0
        });
        // Seconds since the cord was pulled; 0 while not engaged (and 0 for an engaged row
        // whose `engaged_at` was never stamped — absence must not read as a huge age).
        let seconds = if engaged {
            engaged_at.map_or(0.0, |at| (now - at).num_seconds().max(0) as f64)
        } else {
            0.0
        };
        metrics::gauge!("conveyor_andon_engaged_at_seconds", "tenant_id" => label).set(seconds);
    }
}
