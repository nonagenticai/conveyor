//! The licence/token EXPIRY collector.
//!
//! Ported from the Python reference's `services/expiry_metrics.py` (archived repository). Six
//! families:
//!
//! * `nonagentic_license_token_expiry_timestamp_seconds` — one sample per non-revoked token with
//!   an expiry, carrying the ABSOLUTE Unix expiry and the token's whole-lifetime class.
//! * `nonagentic_license_expiry_timestamp_seconds` — one sample per ACTIVE licence with an expiry.
//! * `nonagentic_license_expiry_coverage_rows` — live rows split by whether they carry an expiry.
//! * `nonagentic_license_expired_rows` — exact counts of rows already past expiry, zero-seeded.
//! * `nonagentic_license_expiry_series_truncated` — per-class truncation flags, zero-seeded.
//! * `nonagentic_license_expiry_collector_up` — 1 when the snapshot succeeded, 0 when it failed.
//!
//! ⚠️ NULL `expires_at` means PERPETUAL, which is the normal state of most licences. A perpetual
//! row emits NO expiry sample (emitting 0 would page every day forever; +Inf would silently pad
//! the series) but it IS counted under `coverage_rows{has_expiry="false"}` so a quiet dashboard
//! can be told apart from an unlooked-at estate.
//!
//! ⚠️ Revoked rows are excluded from the expiry SERIES (a revoked credential is already dead and
//! must not win the earliest-wins alert) but still counted in `expired_rows{revoked="true"}`.
//!
//! ⚠️ Per-class caps, not one shared cap. Hourly bundle tokens are re-minted constantly and sort
//! first under `ORDER BY expires_at ASC`; a single cap would let a few hundred of them evict the
//! long-lived rows the reissue bands exist for. Each class has its own fetch, its own cap, and its
//! own truncation flag, and every fetch is ordered by `expires_at ASC` in SQL so truncation can
//! only ever drop the LEAST urgent rows.
//!
//! On a database fault the collector emits `..._expiry_collector_up 0` and NOTHING else: a
//! partial or empty-but-confident snapshot would read as "nothing is due". `LicenseExpiryCollectorDown`
//! is the rule that notices.

use std::collections::BTreeMap;

use sqlx::{PgPool, Row};

use super::Family;

/// Cap on per-row series for the standard / lt30d / lt60d classes and for licences.
const MAX_SERIES_PER_KIND: i64 = 2000;
/// The short class is exported for visibility only and always sorts first: its own small cap.
const MAX_SHORT_TOKEN_SERIES: i64 = 200;
/// `lt7d` is alerted by no band; visibility only, so it gets the small budget too.
const MAX_LT7D_TOKEN_SERIES: i64 = 500;

/// A token whose whole lifetime fits in this window is `lifetime_class="short"` (hourly tokens,
/// re-minted automatically). 65 = the 60-minute mint cap + the 5 minutes of clock skew the schema's
/// `ck_token_granted_roles_short_lived` CHECK already allows.
const SHORT_TOKEN_MAX_LIFETIME_MINUTES: i64 = 65;

/// Mint-order tolerance. The mint path stamps `expires_at` before `created_at`, so a nominal N-day
/// token is a fraction of a second short of N days; a bucket is chosen on `lifetime - 5 minutes`.
const LIFETIME_BUCKET_TOLERANCE_MINUTES: i64 = 5;

/// Every token lifetime class. Licences are always `standard` (they have no short-lived class, and
/// one metric name must have one label-name set).
const LIFETIME_CLASSES: [&str; 5] = ["short", "lt7d", "lt30d", "lt60d", "standard"];

/// The lifetime-class SQL, built from the constants above so the boundaries cannot drift from the
/// numbers the docs and tests name. Every non-matching case — a NULL `created_at`, a zero or
/// negative lifetime — falls to `standard`, the class every reissue band alerts on: an
/// ambiguity must make an alert louder, never hide a credential from it.
fn lifetime_class_case() -> String {
    let mut sql = format!(
        "CASE \
         WHEN t.created_at IS NOT NULL AND t.expires_at > t.created_at \
              AND t.expires_at - t.created_at <= interval '{SHORT_TOKEN_MAX_LIFETIME_MINUTES} minutes' \
              THEN 'short'"
    );
    for (class, days) in [("lt7d", 7), ("lt30d", 30), ("lt60d", 60)] {
        sql.push_str(&format!(
            " WHEN t.created_at IS NOT NULL AND t.expires_at > t.created_at \
                  AND t.expires_at - t.created_at - interval '{LIFETIME_BUCKET_TOLERANCE_MINUTES} minutes' \
                      < interval '{days} days' THEN '{class}'"
        ));
    }
    sql.push_str(" ELSE 'standard' END");
    sql
}

fn token_rows_sql() -> String {
    format!(
        "SELECT c.token_id, c.token_type, c.app, c.tenant, c.expiry_epoch, c.lifetime_class \
           FROM ( \
                SELECT t.id::text                              AS token_id, \
                       COALESCE(t.token_type, 'unknown')       AS token_type, \
                       COALESCE(a.name, 'unknown')             AS app, \
                       COALESCE(tn.name, 'unknown')            AS tenant, \
                       EXTRACT(EPOCH FROM t.expires_at)::float8 AS expiry_epoch, \
                       t.expires_at                            AS expires_at, \
                       {case}                                  AS lifetime_class \
                  FROM token t \
                  JOIN license l  ON l.id = t.license_id \
                  LEFT JOIN app a ON a.id = l.app_id \
                  LEFT JOIN tenants tn ON tn.id = l.tenant_id \
                 WHERE t.revoked = false \
                   AND t.expires_at IS NOT NULL \
           ) c \
          WHERE c.lifetime_class = $2 \
          ORDER BY c.expires_at ASC \
          LIMIT $1",
        case = lifetime_class_case()
    )
}

const LICENSE_ROWS_SQL: &str = "
    SELECT l.id::text                          AS license_id,
           COALESCE(l.status, 'unknown')       AS status,
           COALESCE(a.name, 'unknown')         AS app,
           COALESCE(tn.name, 'unknown')        AS tenant,
           EXTRACT(EPOCH FROM l.expires_at)::float8 AS expiry_epoch
      FROM license l
      LEFT JOIN app a ON a.id = l.app_id
      LEFT JOIN tenants tn ON tn.id = l.tenant_id
     WHERE l.status = 'active'
       AND l.expires_at IS NOT NULL
     ORDER BY l.expires_at ASC
     LIMIT $1";

/// Coverage is measured over the LIVE scope only — the rows a lapse could actually hurt. Counting
/// revoked/expired rows in the denominator would make the ratio drift with history.
const TOKEN_COVERAGE_SQL: &str =
    "SELECT count(*) AS total, count(expires_at) AS with_expiry FROM token WHERE revoked = false";
const LICENSE_COVERAGE_SQL: &str = "SELECT count(*) AS total, count(expires_at) AS with_expiry FROM license WHERE status = 'active'";

fn token_expired_sql() -> String {
    format!(
        "SELECT t.revoked AS revoked, {case} AS lifetime_class, count(*) AS n \
           FROM token t \
          WHERE t.expires_at IS NOT NULL AND t.expires_at <= now() \
          GROUP BY 1, 2",
        case = lifetime_class_case()
    )
}

/// An exact count per revoked state — NOT derived from the capped row fetch. Licence rows have no
/// lifetime class: they are always `standard`.
const LICENSE_EXPIRED_SQL: &str = "
    SELECT (l.status = 'revoked') AS revoked, count(*) AS n
      FROM license l
     WHERE l.expires_at IS NOT NULL AND l.expires_at <= now()
     GROUP BY 1";

struct TokenRow {
    token_id: String,
    token_type: String,
    app: String,
    tenant: String,
    expiry_epoch: f64,
    lifetime_class: String,
}

struct LicenseRow {
    license_id: String,
    status: String,
    app: String,
    tenant: String,
    expiry_epoch: f64,
}

#[derive(Default)]
struct Snapshot {
    token_rows: Vec<TokenRow>,
    license_rows: Vec<LicenseRow>,
    /// `(kind, has_expiry) -> count`.
    coverage: BTreeMap<(String, String), i64>,
    /// `(kind, revoked, lifetime_class) -> count`, every pair seeded at zero.
    expired: BTreeMap<(String, String, String), i64>,
    /// `(kind, lifetime_class) -> 1 when that class hit its own cap`.
    truncated: BTreeMap<(String, String), i64>,
}

/// Read every expiry fact this collector exposes. SELECT-only; `None` on any failure.
async fn snapshot(pool: &Option<PgPool>) -> Option<Snapshot> {
    let pool = pool.as_ref()?;
    let mut snap = Snapshot::default();

    // Seed every (revoked, class) pair and every truncation flag at zero BEFORE the queries: an
    // absent series cannot be alerted on with `> 0` or `== 0`, and reads identically to a
    // collector that never ran.
    for revoked in ["false", "true"] {
        for class in LIFETIME_CLASSES {
            snap.expired.insert(
                ("token".to_owned(), revoked.to_owned(), class.to_owned()),
                0,
            );
        }
    }
    for revoked in ["false", "true"] {
        snap.expired.insert(
            (
                "license".to_owned(),
                revoked.to_owned(),
                "standard".to_owned(),
            ),
            0,
        );
    }
    for class in LIFETIME_CLASSES {
        snap.truncated
            .insert(("token".to_owned(), class.to_owned()), 0);
    }
    snap.truncated
        .insert(("license".to_owned(), "standard".to_owned()), 0);

    // One fetch per class, each under its own cap. The class filter is inside the SQL, before
    // ORDER BY/LIMIT: classifying after a shared LIMIT would be the crowding-out bug itself.
    let rows_sql = token_rows_sql();
    for (class, cap) in [
        ("standard", MAX_SERIES_PER_KIND),
        ("short", MAX_SHORT_TOKEN_SERIES),
        ("lt7d", MAX_LT7D_TOKEN_SERIES),
        ("lt30d", MAX_SERIES_PER_KIND),
        ("lt60d", MAX_SERIES_PER_KIND),
    ] {
        let rows = sqlx::query(sqlx::AssertSqlSafe(rows_sql.clone()))
            .bind(cap)
            .bind(class)
            .fetch_all(pool)
            .await
            .ok()?;
        let fetched = rows.len() as i64;
        for row in rows {
            snap.token_rows.push(TokenRow {
                token_id: row.get("token_id"),
                token_type: row.get("token_type"),
                app: row.get("app"),
                tenant: row.get("tenant"),
                expiry_epoch: row.get("expiry_epoch"),
                lifetime_class: row.get("lifetime_class"),
            });
        }
        // `>=`, not `>`: exactly-at-cap is flagged, as the reference does.
        snap.truncated.insert(
            ("token".to_owned(), class.to_owned()),
            i64::from(fetched >= cap),
        );
    }

    let rows = sqlx::query(LICENSE_ROWS_SQL)
        .bind(MAX_SERIES_PER_KIND)
        .fetch_all(pool)
        .await
        .ok()?;
    snap.truncated.insert(
        ("license".to_owned(), "standard".to_owned()),
        i64::from(rows.len() as i64 >= MAX_SERIES_PER_KIND),
    );
    for row in rows {
        snap.license_rows.push(LicenseRow {
            license_id: row.get("license_id"),
            status: row.get("status"),
            app: row.get("app"),
            tenant: row.get("tenant"),
            expiry_epoch: row.get("expiry_epoch"),
        });
    }

    for (kind, sql) in [
        ("token", TOKEN_COVERAGE_SQL),
        ("license", LICENSE_COVERAGE_SQL),
    ] {
        let row = sqlx::query(sql).fetch_one(pool).await.ok()?;
        let total: i64 = row.get("total");
        let with_expiry: i64 = row.get("with_expiry");
        snap.coverage
            .insert((kind.to_owned(), "true".to_owned()), with_expiry);
        // Emitted explicitly rather than left implicit: a reader must be able to see the
        // population we are NOT watching without knowing the total by heart.
        snap.coverage
            .insert((kind.to_owned(), "false".to_owned()), total - with_expiry);
    }

    let rows = sqlx::query(sqlx::AssertSqlSafe(token_expired_sql()))
        .fetch_all(pool)
        .await
        .ok()?;
    for row in rows {
        let revoked: bool = row.get("revoked");
        let class: String = row.get("lifetime_class");
        let n: i64 = row.get("n");
        snap.expired
            .insert(("token".to_owned(), revoked.to_string(), class), n);
    }
    let rows = sqlx::query(LICENSE_EXPIRED_SQL)
        .fetch_all(pool)
        .await
        .ok()?;
    for row in rows {
        let revoked: bool = row.get("revoked");
        let n: i64 = row.get("n");
        snap.expired.insert(
            (
                "license".to_owned(),
                revoked.to_string(),
                "standard".to_owned(),
            ),
            n,
        );
    }

    Some(snap)
}

const TOKEN_EXPIRY_HELP: &str = "Unix timestamp at which a non-revoked licence TOKEN expires. \
Absolute, never a countdown: the alert bands compute remaining time at query time. \
lifetime_class short = whole lifetime <= 65 minutes (re-minted automatically); \
lt7d/lt30d/lt60d = whole lifetime under 7/30/60 days; each reissue band excludes every class \
shorter than its own window.";

const LICENSE_EXPIRY_HELP: &str = "Unix timestamp at which an ACTIVE licence expires. \
A NULL expires_at means perpetual and is deliberately not represented here.";

const COVERAGE_HELP: &str = "Live rows split by whether they carry an expiry at all. \
has_expiry=\"false\" is the population every expiry alert is BLIND to (NULL expires_at = \
perpetual); read it beside a quiet dashboard to tell \"nothing is due\" from \"we are not \
looking\".";

const EXPIRED_HELP: &str = "Rows whose expiry is already in the past, split by revoked. \
revoked=\"false\" is actionable; revoked=\"true\" is benign history and is surfaced only so it \
stays visible. For kind=\"token\", lifetime_class=\"short\" counts lapsed hourly tokens awaiting \
the sweep (expected, not actionable); alert on lifetime_class!=\"short\". Licences are always \
\"standard\".";

const TRUNCATED_HELP: &str = "1 when a class's per-row series hit its own cap (2000; 200 for \
kind=\"token\",lifetime_class=\"short\"; 500 for lt7d). Each class is capped separately and \
ordered by expires_at ASC, so min() within a class stays correct even at the cap.";

/// Render the collector's families. On a database fault: `..._collector_up 0` and nothing else.
pub async fn render(pool: &Option<PgPool>) -> String {
    let Some(snap) = snapshot(pool).await else {
        let mut up = Family::new(
            "nonagentic_license_expiry_collector_up",
            "1 when the last licence-expiry snapshot succeeded, 0 when the query failed.",
            &[],
        );
        up.sample(Vec::new(), 0.0);
        return up.render();
    };

    let mut out = String::new();
    let mut up = Family::new(
        "nonagentic_license_expiry_collector_up",
        "1 when the last licence-expiry snapshot succeeded, 0 when the query failed.",
        &[],
    );
    up.sample(Vec::new(), 1.0);
    out.push_str(&up.render());

    let mut token = Family::new(
        "nonagentic_license_token_expiry_timestamp_seconds",
        TOKEN_EXPIRY_HELP,
        &["app", "lifetime_class", "tenant", "token_id", "token_type"],
    );
    for row in &snap.token_rows {
        token.sample(
            vec![
                row.app.clone(),
                row.lifetime_class.clone(),
                row.tenant.clone(),
                row.token_id.clone(),
                row.token_type.clone(),
            ],
            row.expiry_epoch,
        );
    }
    out.push_str(&token.render());

    let mut licence = Family::new(
        "nonagentic_license_expiry_timestamp_seconds",
        LICENSE_EXPIRY_HELP,
        &["app", "license_id", "status", "tenant"],
    );
    for row in &snap.license_rows {
        licence.sample(
            vec![
                row.app.clone(),
                row.license_id.clone(),
                row.status.clone(),
                row.tenant.clone(),
            ],
            row.expiry_epoch,
        );
    }
    out.push_str(&licence.render());

    let mut coverage = Family::new(
        "nonagentic_license_expiry_coverage_rows",
        COVERAGE_HELP,
        &["has_expiry", "kind"],
    );
    for ((kind, has_expiry), count) in &snap.coverage {
        coverage.sample(vec![has_expiry.clone(), kind.clone()], *count as f64);
    }
    out.push_str(&coverage.render());

    let mut expired = Family::new(
        "nonagentic_license_expired_rows",
        EXPIRED_HELP,
        &["kind", "lifetime_class", "revoked"],
    );
    for ((kind, revoked, class), count) in &snap.expired {
        expired.sample(
            vec![kind.clone(), class.clone(), revoked.clone()],
            *count as f64,
        );
    }
    out.push_str(&expired.render());

    let mut truncated = Family::new(
        "nonagentic_license_expiry_series_truncated",
        TRUNCATED_HELP,
        &["kind", "lifetime_class"],
    );
    for ((kind, class), flag) in &snap.truncated {
        truncated.sample(vec![kind.clone(), class.clone()], *flag as f64);
    }
    out.push_str(&truncated.render());

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The CASE is one source of truth: the constants the docs and tests name must appear in it.
    #[test]
    fn the_lifetime_case_pins_its_constants() {
        let sql = lifetime_class_case();
        assert!(sql.contains("interval '65 minutes'"), "{sql}");
        assert!(sql.contains("interval '5 minutes'"), "{sql}");
        for days in ["7 days", "30 days", "60 days"] {
            assert!(sql.contains(&format!("interval '{days}'")), "{sql}");
        }
        assert!(sql.contains("ELSE 'standard'"), "{sql}");
        assert_eq!(
            LIFETIME_CLASSES,
            ["short", "lt7d", "lt30d", "lt60d", "standard"]
        );
    }

    #[test]
    fn the_token_fetch_is_ordered_and_capped_per_class() {
        let sql = token_rows_sql();
        assert!(sql.contains("WHERE c.lifetime_class = $2"));
        assert!(sql.contains("ORDER BY c.expires_at ASC"));
        assert!(sql.contains("LIMIT $1"));
    }
}
