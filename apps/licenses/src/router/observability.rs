//! `GET /metrics` — the Prometheus scrape endpoint.
//!
//! The live Python service exposes `/metrics` outside its OpenAPI schema; the router serves the
//! same path here. It is in the auth middleware's public set and is deliberately NOT in `ROUTES`
//! or the `OpenApi` document: `--check-lock` compares documented JSON operations, and a scrape
//! endpoint is neither.
//!
//! ⭐ The exporter handle renders the crate's own counters. The licence-expiry family is rendered
//! here instead of through a gauge because it is a FRESH SNAPSHOT of one small table per scrape:
//! a cached gauge would go stale exactly when it matters (a token minted or revoked between
//! ticks), and the handler's contract is "always 200, even when the database does not answer".
//!
//! ⛔ The Python reference's other families (`nonagentic_license_expiry_timestamp_seconds` for
//! licences, `_coverage_rows`, `_expired_rows`) are NOT ported yet — this slice restores the one
//! family the gitops alert bands read. `nonagentic_license_expiry_collector_up` IS emitted: it is
//! what `LicenseExpiryCollectorDown` arms on, and a family that silently stops answering must not
//! read as "nothing is due".

use std::sync::OnceLock;

use axum::extract::State;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use sqlx::{PgPool, Row};

use crate::state::AppState;

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

/// Fleet-wide cap on the token family. The live table is small (hundreds of rows) and hourly
/// bundle tokens are re-minted constantly, so the cap only bounds a pathological minting loop.
const MAX_TOKEN_SERIES: i64 = 500;

/// The snapshot query. SELECT-only, exactly the reference's population and classification.
///
/// `lifetime = expires_at - created_at`; a NULL `created_at`, a non-positive lifetime, or a
/// lifetime above the last bound is `standard` — the class every reissue band alerts on. The day
/// buckets subtract the 5-minute mint-order tolerance because the mint path stamps `expires_at`
/// before `created_at`, so a nominal N-day token is a fraction of a second short of N days.
///
/// One `&'static str` (not a `format!`): sqlx 0.9's `SqlSafeStr` bound accepts only a literal or
/// a hand-audited `AssertSqlSafe`, and this query has no dynamic part to justify the escape.
const TOKEN_EXPIRY_SQL: &str = "
    SELECT t.id::text                        AS token_id,
           COALESCE(t.token_type, 'unknown') AS token_type,
           COALESCE(a.name, 'unknown')       AS app,
           COALESCE(tn.name, 'unknown')      AS tenant,
           EXTRACT(EPOCH FROM t.expires_at)::float8 AS expiry_epoch,
           CASE
             WHEN t.created_at IS NOT NULL AND t.expires_at > t.created_at
                  AND t.expires_at - t.created_at <= interval '65 minutes' THEN 'short'
             WHEN t.created_at IS NOT NULL AND t.expires_at > t.created_at
                  AND t.expires_at - t.created_at - interval '5 minutes'
                      < interval '7 days' THEN 'lt7d'
             WHEN t.created_at IS NOT NULL AND t.expires_at > t.created_at
                  AND t.expires_at - t.created_at - interval '5 minutes'
                      < interval '30 days' THEN 'lt30d'
             WHEN t.created_at IS NOT NULL AND t.expires_at > t.created_at
                  AND t.expires_at - t.created_at - interval '5 minutes'
                      < interval '60 days' THEN 'lt60d'
             ELSE 'standard'
           END                               AS lifetime_class
      FROM token t
      JOIN license l  ON l.id = t.license_id
      LEFT JOIN app a ON a.id = l.app_id
      LEFT JOIN tenants tn ON tn.id = l.tenant_id
     WHERE t.revoked = false AND t.expires_at IS NOT NULL
     ORDER BY t.expires_at ASC
     LIMIT $1";

/// One non-revoked, expiring token. Non-revoked and `expires_at IS NOT NULL` are the reference's
/// population: a NULL expiry means PERPETUAL, which is the normal state of most licences and the
/// population every expiry alert is deliberately blind to.
struct TokenExpiry {
    token_id: String,
    token_type: String,
    app: String,
    tenant: String,
    expiry_epoch: f64,
    lifetime_class: String,
}

/// `GET /metrics`. Always 200: the scrape must answer even when the database does not.
pub async fn metrics(State(state): State<AppState>) -> Response {
    let mut body = handle().render();
    if !body.is_empty() && !body.ends_with('\n') {
        body.push('\n');
    }
    match token_expiry(&state.pool).await {
        Some(rows) => {
            body.push_str(
                "# HELP nonagentic_license_expiry_collector_up 1 when the last licence-expiry \
                 snapshot succeeded, 0 when the query failed.\n\
                 # TYPE nonagentic_license_expiry_collector_up gauge\n\
                 nonagentic_license_expiry_collector_up 1\n",
            );
            body.push_str(&render_token_expiry(&rows));
        }
        None => {
            // The query failed (or there is no database). Emit collector_up 0 and NO expiry
            // series: a stale or empty-but-confident set is how a dead collector reads as
            // "nothing is due". LicenseExpiryCollectorDown is the alert that notices.
            body.push_str(
                "# HELP nonagentic_license_expiry_collector_up 1 when the last licence-expiry \
                 snapshot succeeded, 0 when the query failed.\n\
                 # TYPE nonagentic_license_expiry_collector_up gauge\n\
                 nonagentic_license_expiry_collector_up 0\n",
            );
        }
    }
    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain; version=0.0.4; charset=utf-8"),
        )],
        body,
    )
        .into_response()
}

/// The snapshot, or `None` when the database is absent or the query fails.
async fn token_expiry(pool: &Option<PgPool>) -> Option<Vec<TokenExpiry>> {
    let pool = pool.as_ref()?;
    let rows = sqlx::query(TOKEN_EXPIRY_SQL)
        .bind(MAX_TOKEN_SERIES)
        .fetch_all(pool)
        .await
        .ok()?;
    Some(
        rows.into_iter()
            .map(|row| TokenExpiry {
                token_id: row.get("token_id"),
                token_type: row.get("token_type"),
                app: row.get("app"),
                tenant: row.get("tenant"),
                expiry_epoch: row.get("expiry_epoch"),
                lifetime_class: row.get("lifetime_class"),
            })
            .collect(),
    )
}

/// Prometheus exposition text for the family. Label values are escaped per the text format.
fn render_token_expiry(rows: &[TokenExpiry]) -> String {
    let mut out = String::from(
        "# HELP nonagentic_license_token_expiry_timestamp_seconds Unix timestamp at which a \
         non-revoked licence TOKEN expires. Absolute, never a countdown: the alert bands compute \
         remaining time at query time. lifetime_class short = whole lifetime <= 65 minutes \
         (re-minted automatically); lt7d/lt30d/lt60d = whole lifetime under 7/30/60 days; each \
         reissue band excludes every class shorter than its own window.\n\
         # TYPE nonagentic_license_token_expiry_timestamp_seconds gauge\n",
    );
    for row in rows {
        out.push_str(&format!(
            "nonagentic_license_token_expiry_timestamp_seconds{{token_id=\"{}\",token_type=\"{}\",app=\"{}\",tenant=\"{}\",lifetime_class=\"{}\"}} {}\n",
            escape(&row.token_id),
            escape(&row.token_type),
            escape(&row.app),
            escape(&row.tenant),
            escape(&row.lifetime_class),
            row.expiry_epoch,
        ));
    }
    out
}

/// Escape a label value: backslash, double quote and newline, per the exposition format.
fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_token_family_is_absolute_and_escapes_label_values() {
        let rows = vec![TokenExpiry {
            token_id: "t\"1".to_owned(),
            token_type: "api".to_owned(),
            app: "conveyor".to_owned(),
            tenant: "acme\\labs".to_owned(),
            expiry_epoch: 1_800_000_000.0,
            lifetime_class: "standard".to_owned(),
        }];
        let text = render_token_expiry(&rows);
        assert!(
            text.contains(
                "nonagentic_license_token_expiry_timestamp_seconds{token_id=\"t\\\"1\",\
                 token_type=\"api\""
            ),
            "the family is named and a quote in a label is escaped: {text}"
        );
        assert!(
            text.contains("tenant=\"acme\\\\labs\""),
            "a backslash in a label is escaped: {text}"
        );
        // The gauge is the Unix expiry itself, never a countdown: the bands threshold at query
        // time, so a pre-computed "days left" would go stale between scrapes.
        assert!(
            text.contains("lifetime_class=\"standard\"} 1800000000"),
            "the value is the absolute expiry: {text}"
        );
        assert!(text.contains("# TYPE nonagentic_license_token_expiry_timestamp_seconds gauge"));
    }

    #[test]
    fn a_newline_in_a_label_does_not_break_the_exposition() {
        let rows = vec![TokenExpiry {
            token_id: "a\nb".to_owned(),
            token_type: "api".to_owned(),
            app: "x".to_owned(),
            tenant: "y".to_owned(),
            expiry_epoch: 1.0,
            lifetime_class: "lt7d".to_owned(),
        }];
        let text = render_token_expiry(&rows);
        assert!(
            text.contains("token_id=\"a\\nb\""),
            "newline escaped: {text}"
        );
        assert_eq!(
            text.lines()
                .filter(|l| l.starts_with("nonagentic_license"))
                .count(),
            1
        );
    }
}
