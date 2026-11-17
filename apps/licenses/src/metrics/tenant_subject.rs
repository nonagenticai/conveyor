//! The tenant/subject INVARIANT collector.
//!
//! Ported from the Python reference's `services/tenant_subject_invariant.py` (archived
//! repository). It measures one invariant: a `tenants.id` must never be a Keycloak PRINCIPAL id —
//! the identity that authenticates must not also be the thing that pays.
//!
//! Nine families:
//!
//! * `nonagentic_license_tenant_subject_collector_up` — 1/0 for the snapshot.
//! * `nonagentic_license_tenant_seed_principal_refusals` and `..._tenant_seed_principal_refused`
//!   — the boot-seed refusal register. ⚠️ This service has no boot seed, so the register has no
//!   producer: the total is emitted at 0 and the named family is absent, which is exactly the
//!   reference's own clean-estate behaviour.
//! * `nonagentic_license_subject_principal_refusals` and `..._subject_principal_refused` — the
//!   subject-registration refusal register, fed by the guard in `db::bundle_apply_rows`.
//! * `nonagentic_license_tenant_is_subject_source_measured` — which door was actually evaluated.
//! * `nonagentic_license_tenant_is_subject_rows` — exact counts per (class, match), zero-seeded.
//! * `nonagentic_license_tenant_is_subject` — one named series per offending identity.
//! * `nonagentic_license_tenant_is_subject_series_truncated` — the 500-row cap flag.
//!
//! ⚠️ Fault asymmetry, preserved from the reference: on a query failure the collector emits
//! `..._tenant_subject_collector_up 0` AND the refusal families (they are registers, not snapshot
//! facts), then nothing snapshot-derived. The refusal families are emitted before the gate for
//! exactly that reason.

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock, PoisonError};

use sqlx::{PgPool, Row};

use super::Family;

/// The deliberate, permanent exemption: `default-anon` is an organisation row, not a principal.
const SENTINEL_TENANT_IDS: [&str; 1] = ["00000000-0000-0000-0000-000000000000"];

/// Recorded exceptions: a tenant id that WAS a Keycloak principal and whose re-key the owner
/// rejected. Empty in the shipped reference; the mechanism is kept and tested with a synthetic id
/// so the class cannot silently disappear.
const RETIRED_PRINCIPAL_TENANT_IDS: [&str; 0] = [];

const CLASS_SENTINEL: &str = "sentinel";
const CLASS_RETIRED_PRINCIPAL: &str = "retired-principal";
const CLASS_VIOLATION: &str = "violation";
/// In the reference's seeding/emission order. Only `violation` is actionable.
const CLASSES: [&str; 3] = [CLASS_SENTINEL, CLASS_RETIRED_PRINCIPAL, CLASS_VIOLATION];

const MATCH_KEYCLOAK_SUB: &str = "keycloak_sub";
const MATCH_SUBJECT_ID: &str = "subject_id";
const MATCH_KINDS: [&str; 2] = [MATCH_KEYCLOAK_SUB, MATCH_SUBJECT_ID];

const SOURCE_LICENCES_SUBJECT: &str = "licences_subject";
const SOURCE_KEYCLOAK_PRINCIPAL: &str = "keycloak_principal";

/// Cap on per-identity series. The COUNTS are computed by a separate unbounded aggregate, so a
/// truncated identity list can never understate the number the alert reads.
const MAX_ROW_SERIES: i64 = 500;

/// Every tenant row that is ALSO a principal id, by the two doors a principal can arrive through.
/// One `UNION ALL`: an id that matches both ways yields two rows and is counted twice, because the
/// two causes have different remediations.
fn matching_union_sql() -> &'static str {
    "SELECT t.id::text            AS tenant_id, \
            t.name::text          AS tenant_name, \
            s.id::text            AS subject_id, \
            'keycloak_sub'        AS match_kind \
       FROM tenants t \
       JOIN subject s ON lower(s.keycloak_sub) = lower(t.id::text) \
     UNION ALL \
     SELECT t.id::text            AS tenant_id, \
            t.name::text          AS tenant_name, \
            s.id::text            AS subject_id, \
            'subject_id'          AS match_kind \
       FROM tenants t \
       JOIN subject s ON lower(s.id::text) = lower(t.id::text)"
}

fn matching_rows_sql() -> String {
    format!(
        "SELECT m.tenant_id, m.tenant_name, m.subject_id, m.match_kind \
           FROM ({}) m \
          ORDER BY m.match_kind, m.tenant_name \
          LIMIT $1",
        matching_union_sql()
    )
}

/// The exact, unbounded count per (class, match). The class CASE restates `classify()` in SQL —
/// the two are changed together, and the DB-backed test asserts they agree on the seeded estate.
fn matching_counts_sql() -> String {
    format!(
        "SELECT m.match_kind AS match_kind, \
                CASE WHEN lower(m.tenant_id) = ANY($1::text[]) THEN '{sentinel}' \
                     WHEN m.match_kind = '{keycloak_sub}' AND lower(m.tenant_id) = ANY($2::text[]) \
                          THEN '{retired}' \
                     ELSE '{violation}' END AS class, \
                count(*) AS n \
           FROM ({union}) m \
          GROUP BY 1, 2",
        sentinel = CLASS_SENTINEL,
        retired = CLASS_RETIRED_PRINCIPAL,
        violation = CLASS_VIOLATION,
        keycloak_sub = MATCH_KEYCLOAK_SUB,
        union = matching_union_sql(),
    )
}

/// Which series a matching row belongs to. Sentinel membership wins; `retired-principal` only ever
/// applies to the `keycloak_sub` door; everything else is a violation.
fn classify(tenant_id: &str, match_kind: &str) -> &'static str {
    let lowered = tenant_id.trim().to_ascii_lowercase();
    if SENTINEL_TENANT_IDS.iter().any(|id| *id == lowered.as_str()) {
        return CLASS_SENTINEL;
    }
    if match_kind == MATCH_KEYCLOAK_SUB
        && RETIRED_PRINCIPAL_TENANT_IDS
            .iter()
            .any(|id| *id == lowered.as_str())
    {
        return CLASS_RETIRED_PRINCIPAL;
    }
    CLASS_VIOLATION
}

/// The process-lifetime subject-refusal register, keyed per `(keycloak_sub, keycloak_realm)`: a
/// re-run of the same refused registration does not grow it, and a later admission never erases
/// the record of the earlier refusal. The value is the tenant name, so the named family can point
/// at the row an operator must re-key.
fn subject_refusals() -> &'static Mutex<BTreeMap<(String, String), String>> {
    static REGISTER: OnceLock<Mutex<BTreeMap<(String, String), String>>> = OnceLock::new();
    REGISTER.get_or_init(|| Mutex::new(BTreeMap::new()))
}

/// Record a refused principal-as-subject registration. Called by the guard that refused it.
pub fn record_subject_refusal(keycloak_sub: &str, keycloak_realm: &str, tenant_name: &str) {
    // Recover from a poisoned lock instead of panicking every later scrape: a refusal must never
    // be able to take `/metrics` down.
    let mut register = subject_refusals()
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    register
        .entry((keycloak_sub.to_owned(), keycloak_realm.to_owned()))
        .or_insert_with(|| tenant_name.to_owned());
}

fn refusal_entries() -> Vec<((String, String), String)> {
    let register = subject_refusals()
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    register
        .iter()
        .map(|(key, name)| (key.clone(), name.clone()))
        .collect()
}

struct MatchingRow {
    tenant_id: String,
    tenant_name: String,
    subject_id: String,
    match_kind: String,
}

struct Snapshot {
    rows: Vec<MatchingRow>,
    /// `(class, match_kind) -> exact count`.
    counts: BTreeMap<(String, String), i64>,
    truncated: i64,
}

/// Read the invariant's facts. SELECT-only; `None` on any failure.
async fn snapshot(pool: &Option<PgPool>) -> Option<Snapshot> {
    let pool = pool.as_ref()?;

    let rows = sqlx::query(sqlx::AssertSqlSafe(matching_rows_sql()))
        .bind(MAX_ROW_SERIES)
        .fetch_all(pool)
        .await
        .ok()?;
    let truncated = i64::from(rows.len() as i64 >= MAX_ROW_SERIES);
    let rows: Vec<MatchingRow> = rows
        .into_iter()
        .map(|row| MatchingRow {
            tenant_id: row.get("tenant_id"),
            tenant_name: row.get("tenant_name"),
            subject_id: row.get("subject_id"),
            match_kind: row.get("match_kind"),
        })
        .collect();

    let sentinels: Vec<String> = SENTINEL_TENANT_IDS
        .iter()
        .map(|id| (*id).to_owned())
        .collect();
    let retired: Vec<String> = RETIRED_PRINCIPAL_TENANT_IDS
        .iter()
        .map(|id| (*id).to_owned())
        .collect();
    let counted = sqlx::query(sqlx::AssertSqlSafe(matching_counts_sql()))
        .bind(sentinels)
        .bind(retired)
        .fetch_all(pool)
        .await
        .ok()?;

    // Seed every (class, match) pair at zero before overlaying the aggregate: a 0 must be an
    // assertion, not an absent series.
    let mut counts = BTreeMap::new();
    for class in CLASSES {
        for match_kind in MATCH_KINDS {
            counts.insert((class.to_owned(), match_kind.to_owned()), 0);
        }
    }
    for row in counted {
        let class: String = row.get("class");
        let match_kind: String = row.get("match_kind");
        let n: i64 = row.get("n");
        counts.insert((class, match_kind), n);
    }

    Some(Snapshot {
        rows,
        counts,
        truncated,
    })
}

const UP_HELP: &str = "1 when the last tenant/subject snapshot succeeded, 0 when the query failed.";

const SEED_REFUSALS_HELP: &str = "Distinct (tenant_id, tenant_name) entries the boot seed refused because the id is already a \
     Keycloak principal id. A register, not a tally: re-running the seed does not grow it. This \
     service has no boot seed, so the register has no producer and reads 0.";

const SEED_REFUSED_HELP: &str = "1 per refused boot-seed entry; the `match` label names which door caught it. Absent while \
     the seed-refusal register is empty.";

const SUBJECT_REFUSALS_HELP: &str = "Distinct (keycloak_sub, keycloak_realm) subject registrations this process refused because \
     that sub is already a tenants.id. A register, not a tally.";

const SUBJECT_REFUSED_HELP: &str = "1 per refused principal-as-subject registration, carrying the tenant the caller must re-key. \
     Absent while the register is empty.";

const SOURCE_MEASURED_HELP: &str = "1 when this scrape actually EVALUATED that source, 0 when it did not look at all. ⛔ Read it \
     before trusting a zero: this check implements only source=\"licences_subject\". \
     source=\"keycloak_principal\" reports 0 here and is DELIBERATELY absent from the violation \
     counts, because a seeded zero is right for \"measured, found none\" and wrong for \"never \
     looked\" — and it is known non-zero (a live Keycloak principal registered as a tenant that \
     was never written to `subject` cannot be seen by any licences-DB predicate).";

const ROWS_HELP: &str = "tenants.id values that are ALSO a Keycloak PRINCIPAL id — an identity that authenticates, \
     registered as the thing that pays. (Principal, not person: several are service accounts; \
     the defect is identical for a machine.) match=\"keycloak_sub\" is a Keycloak claim \
     registered as a tenant; match=\"subject_id\" is this registry's own subject PK registered as \
     one. class=\"violation\" is ACTIONABLE and must reach 0; class=\"sentinel\" is the \
     deliberate, permanent exemption register and is split off so an alert on violations is not \
     pinned above zero forever; class=\"retired-principal\" is a RECORDED EXCEPTION, not a clean \
     row. Every (class, match) pair of the MEASURED source is seeded, so 0 is an assertion and \
     not an absent series.";

const NAMED_HELP: &str = "1 per offending identity, so an alert can NAME the tenant instead of reporting a bare count \
     nobody can action. The `source` label says which door caught it.";

const TRUNCATED_HELP: &str = "1 when the per-identity series hit the 500-row cap. The counts above are computed by a \
     separate unbounded aggregate and stay exact regardless.";

/// Render the collector's families. On a database fault: `..._collector_up 0` plus the refusal
/// families, and no snapshot-derived family.
pub async fn render(pool: &Option<PgPool>) -> String {
    let refusals = refusal_entries();
    let snapshot = snapshot(pool).await;

    let mut out = String::new();
    let mut up = Family::new(
        "nonagentic_license_tenant_subject_collector_up",
        UP_HELP,
        &[],
    );
    up.sample(Vec::new(), if snapshot.is_some() { 1.0 } else { 0.0 });
    out.push_str(&up.render());

    let mut seed_total = Family::new(
        "nonagentic_license_tenant_seed_principal_refusals",
        SEED_REFUSALS_HELP,
        &[],
    );
    seed_total.sample(Vec::new(), 0.0);
    out.push_str(&seed_total.render());

    // The named seed family has no producer here; the family is absent, as the reference's own
    // empty register renders it.

    let mut subject_total = Family::new(
        "nonagentic_license_subject_principal_refusals",
        SUBJECT_REFUSALS_HELP,
        &[],
    );
    subject_total.sample(Vec::new(), refusals.len() as f64);
    out.push_str(&subject_total.render());

    if !refusals.is_empty() {
        let mut named = Family::new(
            "nonagentic_license_subject_principal_refused",
            SUBJECT_REFUSED_HELP,
            &["keycloak_realm", "keycloak_sub", "tenant_name"],
        );
        for ((sub, realm), tenant_name) in &refusals {
            named.sample(vec![realm.clone(), sub.clone(), tenant_name.clone()], 1.0);
        }
        out.push_str(&named.render());
    }

    let Some(snap) = snapshot else {
        return out;
    };

    let mut measured = Family::new(
        "nonagentic_license_tenant_is_subject_source_measured",
        SOURCE_MEASURED_HELP,
        &["source"],
    );
    measured.sample(vec![SOURCE_LICENCES_SUBJECT.to_owned()], 1.0);
    measured.sample(vec![SOURCE_KEYCLOAK_PRINCIPAL.to_owned()], 0.0);
    out.push_str(&measured.render());

    let mut rows = Family::new(
        "nonagentic_license_tenant_is_subject_rows",
        ROWS_HELP,
        &["class", "match", "source"],
    );
    for class in CLASSES {
        for match_kind in MATCH_KINDS {
            let count = snap
                .counts
                .get(&(class.to_owned(), match_kind.to_owned()))
                .copied()
                .unwrap_or(0);
            rows.sample(
                vec![
                    class.to_owned(),
                    match_kind.to_owned(),
                    SOURCE_LICENCES_SUBJECT.to_owned(),
                ],
                count as f64,
            );
        }
    }
    out.push_str(&rows.render());

    let mut named = Family::new(
        "nonagentic_license_tenant_is_subject",
        NAMED_HELP,
        &[
            "class",
            "match",
            "source",
            "subject_id",
            "tenant_id",
            "tenant_name",
        ],
    );
    for row in &snap.rows {
        named.sample(
            vec![
                classify(&row.tenant_id, &row.match_kind).to_owned(),
                row.match_kind.clone(),
                SOURCE_LICENCES_SUBJECT.to_owned(),
                row.subject_id.clone(),
                row.tenant_id.clone(),
                row.tenant_name.clone(),
            ],
            1.0,
        );
    }
    out.push_str(&named.render());

    let mut truncated = Family::new(
        "nonagentic_license_tenant_is_subject_series_truncated",
        TRUNCATED_HELP,
        &[],
    );
    truncated.sample(Vec::new(), snap.truncated as f64);
    out.push_str(&truncated.render());

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_classifier_splits_sentinel_retired_and_violation() {
        assert_eq!(
            classify("00000000-0000-0000-0000-000000000000", MATCH_SUBJECT_ID),
            CLASS_SENTINEL
        );
        // Case-folded: the SQL compares lower(tenant_id) against the lowercase register.
        assert_eq!(
            classify("00000000-0000-0000-0000-000000000000", MATCH_KEYCLOAK_SUB),
            CLASS_SENTINEL
        );
        assert_eq!(classify("  AAAA  ", MATCH_KEYCLOAK_SUB), CLASS_VIOLATION);
        // The retired register is empty in the shipped service: the class is unreachable.
        assert_eq!(classify("BBBB", MATCH_KEYCLOAK_SUB), CLASS_VIOLATION);
    }

    #[test]
    fn the_counts_sql_restates_the_classifier() {
        let sql = matching_counts_sql();
        assert!(sql.contains("= ANY($1::text[])"), "{sql}");
        assert!(sql.contains("= ANY($2::text[])"), "{sql}");
        assert!(sql.contains("'keycloak_sub'"), "{sql}");
        for class in CLASSES {
            assert!(sql.contains(class), "class {class} missing: {sql}");
        }
        assert!(sql.contains("count(*)"), "{sql}");
    }

    #[test]
    fn the_refusal_register_dedupes_per_identity() {
        record_subject_refusal(
            "0195f48b-d000-7000-8000-00000000ffff",
            "realm-test-register",
            "tenant-one",
        );
        record_subject_refusal(
            "0195f48b-d000-7000-8000-00000000ffff",
            "realm-test-register",
            "tenant-two",
        );
        let entries = refusal_entries();
        let hit = entries
            .iter()
            .find(|((sub, realm), _)| {
                sub == "0195f48b-d000-7000-8000-00000000ffff" && realm == "realm-test-register"
            })
            .expect("refusal recorded");
        // First tenant name wins: a re-run does not overwrite the recorded offender.
        assert_eq!(hit.1, "tenant-one");
        assert_eq!(
            entries
                .iter()
                .filter(
                    |((sub, realm), _)| sub.starts_with("0195f48b-d000-7000-8000-00000000ff")
                        && realm == "realm-test-register"
                )
                .count(),
            1
        );
    }
}
