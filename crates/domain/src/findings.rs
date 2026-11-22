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
        .filter(|finding| finding.severity == "P0")
        .filter(|finding| finding.owner_project.as_deref() == Some(owner_project))
        .filter(|finding| finding.resolved_by.is_none())
        .filter(|finding| !waived(finding, now))
        .collect()
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
    use super::{open_p0_findings, parse_date};
    use chrono::{TimeZone, Utc};

    fn at(day: &str) -> chrono::DateTime<Utc> {
        let date = parse_date(day).expect("a test date");
        Utc.from_utc_datetime(&date.and_hms_opt(12, 0, 0).expect("a noon"))
    }

    #[test]
    fn an_unresolved_p0_is_open_for_its_owner() {
        let open = open_p0_findings("S06", at("2026-11-22"));
        assert!(
            open.iter().any(|finding| finding.id == "v1-s06-2"),
            "S06's unresolved P0 is open: {open:?}"
        );
        assert!(
            open_p0_findings("NOT-A-PROJECT", at("2026-11-22")).is_empty(),
            "a finding is charged only to the project that owns it"
        );
    }

    #[test]
    fn an_unexpired_waiver_suppresses_and_an_expired_one_does_not() {
        assert!(
            open_p0_findings("S14", at("2026-11-22")).is_empty(),
            "the owner-accepted residual is still in date"
        );
        let expired = open_p0_findings("S14", at("2027-02-19"));
        assert!(
            expired.iter().any(|finding| finding.id == "v1-s14-1"),
            "after the reassessment date the P0 is open again: {expired:?}"
        );
    }
}
