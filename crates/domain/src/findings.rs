//! The checked-in V1 conformance findings ledger.
//!
//! `docs/evidence/v1-conformance-findings.json` is the audit's committed verdict, not a
//! generated register: a finding is open until it carries `resolved_by`, and an owner-accepted
//! residual is suppressed only while its waiver has not expired. The close gate reads this file
//! directly (v1-close-1), so a closure cannot pass while the project it closes owns an open P0.
//!
//! The ledger is embedded at build time, exactly like the output standard: a missing or
//! malformed file fails the build rather than silently disabling the rule.

use chrono::NaiveDate;
use serde::Deserialize;
use serde_json::Value;

use crate::Timestamp;

/// The embedded ledger. `include_str!` fails the build if the file moves or disappears.
const LEDGER: &str = include_str!("../../../docs/evidence/v1-conformance-findings.json");

/// One finding of the V1 conformance audit.
#[derive(Clone, Debug, Deserialize)]
pub struct Finding {
    /// Stable finding id (`v1-close-1`).
    pub id: String,
    /// `P0 | P1 | P2`.
    pub severity: String,
    /// The project key the finding is charged to, when it has an owner.
    #[serde(default)]
    pub owner_project: Option<String>,
    /// The resolution receipt; absent while the finding is open.
    #[serde(default)]
    pub resolved_by: Option<Value>,
    /// The owner-accepted residual, when one was granted.
    #[serde(default)]
    pub waiver: Option<Waiver>,
}

/// An owner-accepted residual on a finding.
#[derive(Clone, Debug, Deserialize)]
pub struct Waiver {
    /// The reassessment date (`YYYY-MM-DD`); after it the finding is open again.
    #[serde(default)]
    pub expires: Option<String>,
}

#[derive(Deserialize)]
struct Ledger {
    findings: Vec<Finding>,
}

/// Every finding in the committed ledger.
pub fn findings() -> Vec<Finding> {
    let ledger: Ledger = serde_json::from_str(LEDGER)
        .expect("docs/evidence/v1-conformance-findings.json is valid JSON");
    ledger.findings
}

/// The P0 findings that belong to `owner_project` and are open at `now`.
///
/// A finding is open when it has no `resolved_by` receipt and no waiver whose `expires` date is
/// later than `now`. An unparseable or absent expiry never suppresses the finding: a broken
/// waiver is an open P0, not a permission to close.
pub fn open_p0_findings(owner_project: &str, now: Timestamp) -> Vec<Finding> {
    findings()
        .into_iter()
        .filter(|finding| is_open_p0(finding, owner_project, now))
        .collect()
}

/// Whether one finding is an open P0 for `owner_project` at `now`.
///
/// The predicate is separated from the ledger so the rule can be proved on constructed rows:
/// a test that pins a particular live finding goes stale the day that finding is resolved
/// (measured 2026-11-25: v1-s06-2 was resolved and two tests that cited it as "the open P0"
/// turned red).
fn is_open_p0(finding: &Finding, owner_project: &str, now: Timestamp) -> bool {
    finding.severity == "P0"
        && finding.owner_project.as_deref() == Some(owner_project)
        && finding.resolved_by.is_none()
        && !waived(finding, now)
}

fn waived(finding: &Finding, now: Timestamp) -> bool {
    finding
        .waiver
        .as_ref()
        .and_then(|waiver| waiver.expires.as_deref())
        .and_then(parse_date)
        .is_some_and(|expires| expires > now.date_naive())
}

fn parse_date(raw: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(raw.trim(), "%Y-%m-%d").ok()
}

#[cfg(test)]
mod tests {
    use super::{Finding, Waiver, findings, is_open_p0, open_p0_findings, parse_date, waived};
    use chrono::{TimeZone, Utc};

    fn at(day: &str) -> chrono::DateTime<Utc> {
        let date = parse_date(day).expect("a test date");
        Utc.from_utc_datetime(&date.and_hms_opt(12, 0, 0).expect("a noon"))
    }

    fn finding(id: &str, severity: &str, owner: &str) -> Finding {
        Finding {
            id: id.to_owned(),
            severity: severity.to_owned(),
            owner_project: Some(owner.to_owned()),
            resolved_by: None,
            waiver: None,
        }
    }

    /// The rule on constructed rows: severity, owner, resolution and waiver decide openness.
    /// A test that named a live finding went stale when that finding was resolved; the
    /// ledger-level invariant below is the live half.
    #[test]
    fn an_unresolved_p0_is_open_for_its_owner() {
        let open = finding("v1-test-open", "P0", "S06");
        assert!(is_open_p0(&open, "S06", at("2026-11-22")));
        assert!(
            !is_open_p0(&open, "NOT-A-PROJECT", at("2026-11-22")),
            "a finding is charged only to the project that owns it"
        );
        let p1 = finding("v1-test-p1", "P1", "S06");
        assert!(
            !is_open_p0(&p1, "S06", at("2026-11-22")),
            "only a P0 gates closure"
        );
        let mut resolved = finding("v1-test-resolved", "P0", "S06");
        resolved.resolved_by = Some(serde_json::json!({"commit": "0".repeat(40)}));
        assert!(
            !is_open_p0(&resolved, "S06", at("2026-11-22")),
            "a resolution receipt closes the finding"
        );
    }

    /// The live half: the committed ledger carries no open P0 — the state the programme must
    /// hold at its final acceptance, and the reason a test cannot cite a live P0 fixture.
    #[test]
    fn the_ledger_carries_no_open_p0() {
        let open: Vec<String> = findings()
            .into_iter()
            .filter(|row| row.severity == "P0" && row.resolved_by.is_none() && row.waiver.is_none())
            .map(|row| row.id)
            .collect();
        assert!(open.is_empty(), "open P0 findings in the ledger: {open:?}");
        assert!(
            open_p0_findings("S06", at("2026-11-22")).is_empty(),
            "no project carries an open ledger P0"
        );
    }

    /// The suppression MECHANISM, on synthetic rows: the committed ledger no longer carries a
    /// waiver (decision 0012 — no deferrals, no waivers), so the rule is proved on constructed
    /// findings instead of on live ledger content that would make the test stale the day the
    /// residual is resolved.
    #[test]
    fn an_unexpired_waiver_suppresses_and_an_expired_one_does_not() {
        let residual = Finding {
            id: "v1-test-waiver".to_owned(),
            severity: "P0".to_owned(),
            owner_project: Some("S14".to_owned()),
            resolved_by: None,
            waiver: Some(Waiver {
                expires: Some("2027-02-18".to_owned()),
            }),
        };
        assert!(
            waived(&residual, at("2026-11-22")),
            "the residual is still in date"
        );
        assert!(
            !waived(&residual, at("2027-02-19")),
            "after the reassessment date the P0 is open again"
        );
        // An absent or unparseable expiry never suppresses: a broken waiver is an open P0.
        let broken = Finding {
            waiver: Some(Waiver { expires: None }),
            ..residual.clone()
        };
        assert!(
            !waived(&broken, at("2026-11-22")),
            "an absent expiry never suppresses"
        );
        let unparseable = Finding {
            waiver: Some(Waiver {
                expires: Some("not-a-date".to_owned()),
            }),
            ..residual.clone()
        };
        assert!(
            !waived(&unparseable, at("2026-11-22")),
            "a broken waiver is open"
        );
    }

    /// The live invariant the mechanism exists under: decision 0012 removed every waiver, so
    /// no finding in the committed ledger may carry one.
    #[test]
    fn the_ledger_carries_no_waiver() {
        let waived_rows: Vec<String> = findings()
            .into_iter()
            .filter(|finding| finding.waiver.is_some())
            .map(|finding| finding.id)
            .collect();
        assert!(
            waived_rows.is_empty(),
            "no deferral or waiver of programme work (decision 0012): {waived_rows:?}"
        );
    }
}
