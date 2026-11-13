//! The six `/api/internal/*` reads over `inference_call`.
//!
//! Ported 1:1 from the reference's `routers/ledger.py` (2026-11-13). Two choices keep the port
//! honest:
//!
//! * Where the reference assembled a dict in Python, these build the document **in SQL**
//!   (`json_build_object` / `json_agg`), so a timestamp, a NULL and the NULL-vs-zero distinction
//!   serialise exactly as Postgres renders them — the same values the reference read.
//! * A share over zero rows is **NULL, never 0**. A fabricated zero means "measured, and none
//!   were", which is a different fact from "nothing to measure".
//!
//! Optional filters are written as `($n IS NULL OR …)` rather than dynamic SQL: the bind list is
//! constant, which makes every query reviewable against the reference in one screen.

use chrono::{DateTime, NaiveDate, Utc};
use serde_json::{Value, json};
use sqlx::{AssertSqlSafe, PgPool};

use crate::ledger::{canonical_json, sha256_hex};

/// Hard ceiling on `limit` (the contract's `limit<=500`).
pub const MAX_LIMIT: i64 = 500;
/// Hard ceiling on `days` — the ledger's row retention default.
pub const MAX_DAYS: i32 = 90;
/// The repeat-groups / pinning default window when `since` is not given.
pub const DEFAULT_WINDOW_DAYS: i32 = 7;
/// `source=` values; `unknown` = written before migration 0012.
pub const TRAFFIC_SOURCES: [&str; 3] = ["benchmark", "live", "unknown"];

/// `GET /api/internal/inference-calls/summary`.
pub async fn summary(pool: &PgPool, days: i32) -> Result<Value, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT json_build_object(
            'as_of', now(),
            'source', 'inference_call',
            'window', json_build_object('days', $1::int),
            'last_row_at', max(created_at),
            'rows_24h', count(*) FILTER (WHERE created_at >= now() - interval '24 hours'),
            'rows_window', count(*) FILTER (WHERE created_at >= now() - make_interval(days => $1::int)),
            'rows_total', count(*)
         ) FROM inference_call",
    )
    .bind(days)
    .fetch_one(pool)
    .await
}

/// Filters for the call list.
#[derive(Debug, Default)]
pub struct CallFilters {
    pub correlation_id: Option<String>,
    pub caller: Option<String>,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
    pub limit: i64,
}

/// `GET /api/internal/inference-calls`.
pub async fn calls(pool: &PgPool, filters: &CallFilters) -> Result<Value, sqlx::Error> {
    // The reference's `_ROW_COLUMNS`: finish_reason falls back to a captured response for rows
    // written before 0010, and `captured` says whether /capture has anything at all.
    const WHERE: &str = "($1::text IS NULL OR correlation_id = $1)
        AND ($2::text IS NULL OR caller_worker = $2)
        AND ($3::timestamptz IS NULL OR created_at >= $3)
        AND ($4::timestamptz IS NULL OR created_at < $4)";
    let total: i64 = sqlx::query_scalar(AssertSqlSafe(format!(
        "SELECT count(*) FROM inference_call WHERE {WHERE}"
    )))
    .bind(filters.correlation_id.clone())
    .bind(filters.caller.clone())
    .bind(filters.since)
    .bind(filters.until)
    .fetch_one(pool)
    .await?;

    let page: Value = sqlx::query_scalar(AssertSqlSafe(format!(
        "SELECT json_build_object(
            'as_of', now(),
            'rows', COALESCE(json_agg(r ORDER BY r.id DESC), '[]'::json)
         ) FROM (
            SELECT id, created_at AS at, correlation_id, caller_worker AS caller, tenant_id,
              licence_id, licence_source, requested_model, served_provider, served_model,
              served_model_root, temperature, seed, max_tokens, repetition_penalty,
              frequency_penalty,
              COALESCE(finish_reason, response_json->'choices'->0->>'finish_reason') AS finish_reason,
              status, latency_ms,
              NULLIF(btrim(request_sha256), '') AS request_sha256,
              NULLIF(btrim(response_sha256), '') AS response_sha256,
              (request_json IS NOT NULL OR response_json IS NOT NULL) AS captured
            FROM inference_call WHERE {WHERE}
            ORDER BY id DESC LIMIT $5
         ) r"
    )))
    .bind(filters.correlation_id.clone())
    .bind(filters.caller.clone())
    .bind(filters.since)
    .bind(filters.until)
    .bind(filters.limit)
    .fetch_one(pool)
    .await?;

    Ok(json!({
        "as_of": page["as_of"],
        "source": "inference_call",
        "total": total,
        "limit": filters.limit,
        "rows": page["rows"],
    }))
}

/// `GET /api/internal/inference-calls/{id}/capture`; `None` when the row has no captured body.
pub async fn capture(pool: &PgPool, call_id: i64) -> Result<Option<Value>, sqlx::Error> {
    let row: Option<Value> = sqlx::query_scalar(
        "SELECT json_build_object(
            'id', id, 'at', created_at, 'correlation_id', correlation_id,
            'requested_model', requested_model,
            'request_sha256', NULLIF(btrim(request_sha256), ''),
            'response_sha256', NULLIF(btrim(response_sha256), ''),
            'request', request_json, 'response', response_json,
            'request_text', request_text, 'response_text', response_text,
            'request_text_sha256', NULLIF(btrim(request_text_sha256), '')
         ) FROM inference_call WHERE id = $1",
    )
    .bind(call_id)
    .fetch_optional(pool)
    .await?;
    let Some(mut row) = row else {
        return Ok(None);
    };
    if ["request", "response", "request_text", "response_text"]
        .iter()
        .all(|key| row[*key].is_null())
    {
        return Ok(None);
    }
    // `request_sha256` hashes the stored body WITHOUT `model`, so a consumer freezing a tuple
    // from this response knows whether the stored JSONB round-tripped exactly.
    let request_verified = row["request"].as_object().map(|_| {
        let mut fingerprint = row["request"].clone();
        if let Some(object) = fingerprint.as_object_mut() {
            object.remove("model");
        }
        sha256_hex(&canonical_json(&fingerprint)) == row["request_sha256"].as_str().unwrap_or("")
    });
    let text_verified = row["request_text"]
        .as_str()
        .map(|text| sha256_hex(text) == row["request_text_sha256"].as_str().unwrap_or(""));
    row["request_sha256_verified"] = json!(request_verified);
    row["request_text_verified"] = json!(text_verified);
    Ok(Some(row))
}

/// `GET /api/internal/served-models/timeline` — one row per UTC day × model × root × provider,
/// with empty days present and every share NULL rather than zero.
pub async fn timeline(pool: &PgPool, days: i32) -> Result<Value, sqlx::Error> {
    sqlx::query_scalar(
        "WITH days AS (
            SELECT generate_series(
              (now() AT TIME ZONE 'UTC')::date - ($1::int - 1),
              (now() AT TIME ZONE 'UTC')::date, interval '1 day')::date AS day
         ), agg AS (
            SELECT (created_at AT TIME ZONE 'UTC')::date AS day, requested_model,
              served_model_root, served_provider,
              count(*) AS n,
              count(*) FILTER (WHERE temperature = 0) AS n_t0,
              count(*) FILTER (WHERE seed IS NOT NULL) AS n_seed,
              count(*) FILTER (WHERE licence_source IS NOT NULL) AS n_licence_recorded,
              count(*) FILTER (WHERE licence_id IS NOT NULL) AS n_licensed
            FROM inference_call
            WHERE created_at >= ((now() AT TIME ZONE 'UTC')::date - ($1::int - 1)) AT TIME ZONE 'UTC'
            GROUP BY 1, 2, 3, 4
         )
         SELECT json_build_object(
            'as_of', now(),
            'source', 'inference_call',
            'window', json_build_object('days', $1::int, 'tz', 'UTC'),
            'series', COALESCE(json_agg(json_build_object(
                'day', to_char(d.day, 'YYYY-MM-DD'),
                'requested_model', a.requested_model,
                'served_model_root', a.served_model_root,
                'served_provider', a.served_provider,
                'n', COALESCE(a.n, 0),
                't0_share', CASE WHEN COALESCE(a.n, 0) = 0 THEN NULL
                                 ELSE COALESCE(a.n_t0, 0)::float8 / a.n::float8 END,
                'seed_share', CASE WHEN COALESCE(a.n, 0) = 0 THEN NULL
                                   ELSE COALESCE(a.n_seed, 0)::float8 / a.n::float8 END,
                'licensed_share', CASE WHEN a.n_licence_recorded IS NULL OR a.n_licence_recorded = 0
                                       THEN NULL
                                       ELSE COALESCE(a.n_licensed, 0)::float8 / a.n_licence_recorded::float8 END,
                'n_licence_recorded', COALESCE(a.n_licence_recorded, 0)
              ) ORDER BY d.day, a.requested_model NULLS FIRST, a.served_model_root NULLS FIRST,
                a.served_provider NULLS FIRST), '[]'::json)
         ) FROM days d LEFT JOIN agg a ON a.day = d.day",
    )
    .bind(days)
    .fetch_one(pool)
    .await
}

/// Filters for the repeat-group report.
#[derive(Debug, Default)]
pub struct RepeatFilters {
    pub since: Option<DateTime<Utc>>,
    pub temperature: Option<f64>,
    pub caller: Option<String>,
    pub served_provider: Option<String>,
    /// `benchmark` | `live` | `unknown` | `all`.
    pub source: String,
    pub seeded: Option<bool>,
    pub limit: i64,
}

/// The rows repeat-groups may compare: `status='ok'`, a hashed response, and the window.
const REPEAT_ROWS: &str = "SELECT *, COALESCE(traffic_source, 'unknown') AS src FROM inference_call
    WHERE status = 'ok' AND response_sha256 IS NOT NULL
      AND created_at >= GREATEST(COALESCE($1::timestamptz, now() - make_interval(days => 7)),
                                 now() - make_interval(days => $2::int))
      AND ($3::float8 IS NULL OR temperature = $3)
      AND ($4::text IS NULL OR caller_worker = $4)
      AND ($5::text IS NULL OR served_provider = $5)
      AND ($6::text IS NULL OR COALESCE(traffic_source, 'unknown') = $6)
      AND ($7::boolean IS NULL OR (seed IS NOT NULL) = $7)";

const REPEAT_GROUPS_CTE: &str = "WITH r AS (REPEAT_ROWS), g AS (
    SELECT request_sha256,
      count(*) AS n_calls,
      count(DISTINCT response_sha256) AS n_distinct_responses,
      count(DISTINCT COALESCE(served_model, '<null>')) AS n_served_models,
      array_agg(DISTINCT requested_model ORDER BY requested_model) AS requested_models,
      array_agg(DISTINCT served_model_root ORDER BY served_model_root) AS served_roots,
      array_agg(DISTINCT served_provider ORDER BY served_provider) AS served_providers,
      array_agg(DISTINCT caller_worker ORDER BY caller_worker) AS callers,
      array_agg(DISTINCT src ORDER BY src) AS sources,
      min(created_at) AS first_at, max(created_at) AS last_at
    FROM r GROUP BY request_sha256 HAVING count(*) >= 2
)";

/// `GET /api/internal/inference-calls/repeat-groups`.
pub async fn repeat_groups(pool: &PgPool, filters: &RepeatFilters) -> Result<Value, sqlx::Error> {
    let source = (filters.source != "all").then_some(filters.source.clone());
    let cte = REPEAT_GROUPS_CTE.replace("REPEAT_ROWS", REPEAT_ROWS);

    let groups_sql = format!(
        "{cte}
         SELECT COALESCE(json_agg(json_build_object(
            'request_sha256', btrim(g.request_sha256),
            'n_calls', g.n_calls,
            'n_distinct_responses', g.n_distinct_responses,
            'served_models_n', g.n_served_models,
            'requested_models', g.requested_models,
            'served_roots', g.served_roots,
            'served_providers', g.served_providers,
            'callers', g.callers,
            'sources', g.sources,
            'first_at', g.first_at,
            'last_at', g.last_at
         ) ORDER BY (g.n_distinct_responses > 1) DESC, g.last_at DESC, g.request_sha256), '[]'::json)
         FROM (SELECT * FROM g
               ORDER BY (n_distinct_responses > 1) DESC, last_at DESC, request_sha256
               LIMIT $8) g"
    );
    let groups: Value = sqlx::query_scalar(AssertSqlSafe(groups_sql))
        .bind(filters.since)
        .bind(MAX_DAYS)
        .bind(filters.temperature)
        .bind(filters.caller.clone())
        .bind(filters.served_provider.clone())
        .bind(source.clone())
        .bind(filters.seeded)
        .bind(filters.limit)
        .fetch_one(pool)
        .await?;

    let aggregate_sql = format!(
        "{cte}
         SELECT json_build_object(
            'as_of', now(),
            'groups', count(*),
            'disagreeing', count(*) FILTER (WHERE n_distinct_responses > 1),
            'straddling_groups', count(*) FILTER (WHERE n_served_models > 1),
            'calls', COALESCE(sum(n_calls), 0),
            'effective_since',
              GREATEST(COALESCE($1::timestamptz, now() - make_interval(days => 7)),
                       now() - make_interval(days => $2::int))
         ) FROM g"
    );
    let aggregate: Value = sqlx::query_scalar(AssertSqlSafe(aggregate_sql))
        .bind(filters.since)
        .bind(MAX_DAYS)
        .bind(filters.temperature)
        .bind(filters.caller.clone())
        .bind(filters.served_provider.clone())
        .bind(source.clone())
        .bind(filters.seeded)
        .fetch_one(pool)
        .await?;

    let unhashed: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM inference_call
         WHERE status = 'ok' AND response_sha256 IS NULL
           AND created_at >= GREATEST(COALESCE($1::timestamptz, now() - make_interval(days => 7)),
                                      now() - make_interval(days => $2::int))
           AND ($3::float8 IS NULL OR temperature = $3)
           AND ($4::text IS NULL OR caller_worker = $4)
           AND ($5::text IS NULL OR served_provider = $5)
           AND ($6::text IS NULL OR COALESCE(traffic_source, 'unknown') = $6)
           AND ($7::boolean IS NULL OR (seed IS NOT NULL) = $7)",
    )
    .bind(filters.since)
    .bind(MAX_DAYS)
    .bind(filters.temperature)
    .bind(filters.caller.clone())
    .bind(filters.served_provider.clone())
    .bind(source.clone())
    .bind(filters.seeded)
    .fetch_one(pool)
    .await?;

    // by_source: every filter EXCEPT `source`, so a consumer asking source=live still sees how
    // much benchmark and unknown traffic it excluded.
    let by_source_rows: Value = sqlx::query_scalar(AssertSqlSafe(format!(
        "{cte}
         SELECT COALESCE(json_agg(json_build_object(
            'k', x.k, 'groups', x.groups, 'disagreeing', x.disagreeing, 'calls', x.calls)
            ORDER BY x.calls DESC, x.k), '[]'::json)
         FROM (SELECT k, count(*) AS groups,
                 count(*) FILTER (WHERE n_distinct_responses > 1) AS disagreeing,
                 sum(n_calls) AS calls
               FROM (SELECT src AS k, request_sha256, count(*) AS n_calls,
                       count(DISTINCT response_sha256) AS n_distinct_responses
                     FROM r GROUP BY src, request_sha256 HAVING count(*) >= 2) pg
               GROUP BY k) x"
    )))
    .bind(filters.since)
    .bind(MAX_DAYS)
    .bind(filters.temperature)
    .bind(filters.caller.clone())
    .bind(filters.served_provider.clone())
    .bind(None::<String>)
    .bind(filters.seeded)
    .fetch_one(pool)
    .await?;

    let callers_sql = format!(
        "{cte}
         SELECT COALESCE(json_agg(json_build_object(
            'caller', x.k, 'groups', x.groups, 'disagreeing', x.disagreeing, 'calls', x.calls)
            ORDER BY x.calls DESC, x.k NULLS LAST), '[]'::json)
         FROM (SELECT k, count(*) AS groups,
                 count(*) FILTER (WHERE n_distinct_responses > 1) AS disagreeing,
                 sum(n_calls) AS calls
               FROM (SELECT caller_worker AS k, request_sha256, count(*) AS n_calls,
                       count(DISTINCT response_sha256) AS n_distinct_responses
                     FROM r GROUP BY caller_worker, request_sha256 HAVING count(*) >= 2) pg
               GROUP BY k) x"
    );
    let callers: Value = sqlx::query_scalar(AssertSqlSafe(callers_sql))
        .bind(filters.since)
        .bind(MAX_DAYS)
        .bind(filters.temperature)
        .bind(filters.caller.clone())
        .bind(filters.served_provider.clone())
        .bind(source.clone())
        .bind(filters.seeded)
        .fetch_one(pool)
        .await?;

    let mut by_source = serde_json::Map::new();
    for name in TRAFFIC_SOURCES {
        by_source.insert(
            name.to_owned(),
            json!({"groups": 0, "disagreeing": 0, "calls": 0}),
        );
    }
    for row in by_source_rows.as_array().cloned().unwrap_or_default() {
        if let Some(name) = row["k"].as_str() {
            by_source.insert(
                name.to_owned(),
                json!({
                    "groups": row["groups"],
                    "disagreeing": row["disagreeing"],
                    "calls": row["calls"],
                }),
            );
        }
    }

    Ok(json!({
        "as_of": aggregate["as_of"],
        "source": "inference_call",
        "grouped_on": "request_sha256",
        "window": {
            "since": aggregate["effective_since"],
            "temperature": filters.temperature,
            "caller": filters.caller,
            "served_provider": filters.served_provider,
            "traffic_source": filters.source,
            "seeded": filters.seeded,
        },
        "limit": filters.limit,
        "groups": groups,
        "summary": {
            "groups": aggregate["groups"],
            "disagreeing": aggregate["disagreeing"],
            "straddling_groups": aggregate["straddling_groups"],
            "calls": aggregate["calls"],
            "unhashed_calls": unhashed,
            "by_source": by_source,
            "callers": callers,
        },
    }))
}

/// Filters for the pinning summary.
#[derive(Debug, Default)]
pub struct PinningFilters {
    pub since: Option<DateTime<Utc>>,
    /// `benchmark` | `live` | `unknown` | `all`.
    pub source: String,
}

/// `GET /api/internal/inference-calls/pinning-summary` — counts EVERY call in the window, not
/// only `status='ok'`: pinning is a property of the request the caller sent.
pub async fn pinning_summary(
    pool: &PgPool,
    filters: &PinningFilters,
) -> Result<Value, sqlx::Error> {
    let source = (filters.source != "all").then_some(filters.source.clone());
    const WINDOW: &str =
        "created_at >= GREATEST(COALESCE($1::timestamptz, now() - make_interval(days => 7)),
        now() - make_interval(days => $2::int))
        AND ($3::text IS NULL OR COALESCE(traffic_source, 'unknown') = $3)";

    let totals: Value = sqlx::query_scalar(AssertSqlSafe(format!(
        "SELECT json_build_object(
            'n', count(*),
            'temperature_zero_n', count(*) FILTER (WHERE temperature = 0),
            'pinned_n', count(*) FILTER (WHERE temperature = 0 AND seed IS NOT NULL),
            'effective_since', GREATEST(COALESCE($1::timestamptz, now() - make_interval(days => 7)),
                                        now() - make_interval(days => $2::int))
         ) FROM inference_call WHERE {WINDOW}"
    )))
    .bind(filters.since)
    .bind(MAX_DAYS)
    .bind(source.clone())
    .fetch_one(pool)
    .await?;

    let callers: Value = sqlx::query_scalar(AssertSqlSafe(format!(
        "SELECT COALESCE(json_agg(json_build_object(
            'caller', x.caller, 'n', x.n, 'temperature_zero_n', x.temperature_zero_n,
            'pinned_n', x.pinned_n, 'last_at', x.last_at)
            ORDER BY x.n DESC, x.caller NULLS LAST), '[]'::json)
         FROM (SELECT caller_worker AS caller, count(*) AS n,
                 count(*) FILTER (WHERE temperature = 0) AS temperature_zero_n,
                 count(*) FILTER (WHERE temperature = 0 AND seed IS NOT NULL) AS pinned_n,
                 max(created_at) AS last_at
               FROM inference_call WHERE {WINDOW}
               GROUP BY caller_worker) x"
    )))
    .bind(filters.since)
    .bind(MAX_DAYS)
    .bind(source)
    .fetch_one(pool)
    .await?;

    Ok(json!({
        "since": totals["effective_since"],
        "source": filters.source,
        "totals": {
            "n": totals["n"],
            "temperature_zero_n": totals["temperature_zero_n"],
            "pinned_n": totals["pinned_n"],
        },
        "callers": callers,
    }))
}

/// Parse a query timestamp the way Pydantic does: an RFC 3339 instant, or a bare date at UTC
/// midnight.
pub fn parse_timestamp(raw: &str) -> Option<DateTime<Utc>> {
    let raw = raw.trim();
    if let Ok(parsed) = DateTime::parse_from_rfc3339(raw) {
        return Some(parsed.with_timezone(&Utc));
    }
    NaiveDate::parse_from_str(raw, "%Y-%m-%d")
        .ok()
        .and_then(|date| date.and_hms_opt(0, 0, 0))
        .map(|naive| naive.and_utc())
}
