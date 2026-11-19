#![forbid(unsafe_code)]
//! T11 — the pure rules behind `metrics.*` (SPEC_V2 §7.5, §15.1 D37/D50; issue #84).
//!
//! No I/O and no policy numbers live here: the threshold values are tenant rows
//! (`metric_threshold`), the evidence is read from the ledgers, and the only decisions are the
//! closed set of evidence names, the C0–C5 ordering and the reconciliation verdict.

use std::collections::BTreeMap;

/// The evidence metrics a tenant threshold may name. The set is closed: a configuration naming
/// an unknown metric would otherwise read as a met gate while measuring nothing.
pub const EVIDENCE_METRICS: [&str; 9] = [
    "approvals.decided",
    "approvals.approved",
    "approvals.rejected",
    "approvals.approval_rate",
    "approvals.evidence_completeness",
    "runs.total",
    "runs.failed",
    "benefits.realised",
    "benefits.planned",
];

/// Whether `name` is one of the closed evidence metrics.
pub fn known_metric(name: &str) -> bool {
    EVIDENCE_METRICS.contains(&name)
}

/// The maturity ladder. Ordered: C0 < C1 < … < C5.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MaturityLevel {
    C0,
    C1,
    C2,
    C3,
    C4,
    C5,
}

impl MaturityLevel {
    /// Every level, in order.
    pub const ALL: [MaturityLevel; 6] = [
        MaturityLevel::C0,
        MaturityLevel::C1,
        MaturityLevel::C2,
        MaturityLevel::C3,
        MaturityLevel::C4,
        MaturityLevel::C5,
    ];

    /// The stored spelling (`C0` … `C5`).
    pub fn as_str(self) -> &'static str {
        match self {
            MaturityLevel::C0 => "C0",
            MaturityLevel::C1 => "C1",
            MaturityLevel::C2 => "C2",
            MaturityLevel::C3 => "C3",
            MaturityLevel::C4 => "C4",
            MaturityLevel::C5 => "C5",
        }
    }

    /// Parse the stored spelling; anything else is not a level.
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|level| level.as_str() == value)
    }

    /// The ordinal used to key a configuration by level.
    pub fn rank(self) -> usize {
        match self {
            MaturityLevel::C0 => 0,
            MaturityLevel::C1 => 1,
            MaturityLevel::C2 => 2,
            MaturityLevel::C3 => 3,
            MaturityLevel::C4 => 4,
            MaturityLevel::C5 => 5,
        }
    }
}

/// One tenant threshold row, already parsed.
#[derive(Debug, Clone, PartialEq)]
pub struct Threshold {
    pub level: MaturityLevel,
    pub version: i64,
    pub metrics: BTreeMap<String, f64>,
}

/// The computed state of one configured level.
#[derive(Debug, Clone, PartialEq)]
pub struct LevelState {
    pub level: MaturityLevel,
    pub version: i64,
    pub met: bool,
    /// The named metrics that are missing from the evidence or below their recorded minimum,
    /// sorted. A missing metric is never met: an unmeasured figure cannot lift a level.
    pub unmet: Vec<String>,
}

/// Every configured level, in C0–C5 order, each judged against the recorded evidence.
pub fn levels(thresholds: &[Threshold], evidence: &BTreeMap<String, f64>) -> Vec<LevelState> {
    let mut by_level: BTreeMap<usize, &Threshold> = BTreeMap::new();
    for threshold in thresholds {
        by_level.insert(threshold.level.rank(), threshold);
    }
    let mut states = Vec::new();
    for level in MaturityLevel::ALL {
        let Some(threshold) = by_level.get(&level.rank()) else {
            break;
        };
        let mut unmet: Vec<String> = threshold
            .metrics
            .iter()
            .filter_map(|(name, minimum)| match evidence.get(name) {
                Some(value) if value >= minimum => None,
                _ => Some(name.clone()),
            })
            .collect();
        unmet.sort();
        states.push(LevelState {
            level,
            version: threshold.version,
            met: unmet.is_empty(),
            unmet,
        });
    }
    states
}

/// The highest contiguous met level and the threshold version it used.
///
/// A gap never lets a higher level stand — the same rule as a gate that cannot refuse: the
/// ladder is climbed one recorded step at a time.
pub fn current_level(states: &[LevelState]) -> Option<(MaturityLevel, i64)> {
    let mut current = None;
    for state in states {
        if !state.met {
            break;
        }
        current = Some((state.level, state.version));
    }
    current
}

/// Validate one threshold row before it is stored.
pub fn validate_threshold(
    level: &str,
    metrics: &BTreeMap<String, f64>,
) -> Result<MaturityLevel, String> {
    let parsed = MaturityLevel::parse(level)
        .ok_or_else(|| format!("unknown maturity level {level:?}; expected C0–C5"))?;
    if metrics.is_empty() {
        return Err("a level with no evidence metric can neither be met nor refuse".to_owned());
    }
    for (name, minimum) in metrics {
        if !known_metric(name) {
            return Err(format!("unknown evidence metric {name:?}"));
        }
        if !minimum.is_finite() || *minimum < 0.0 {
            return Err(format!("metric {name:?} must be a finite value >= 0"));
        }
    }
    Ok(parsed)
}

/// The reconciliation verdict (P2). `Unavailable` is constructed by the caller when the
/// upstream cannot be read; it is never derived from a zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconciliationState {
    Matched,
    Difference,
    Unavailable,
}

impl ReconciliationState {
    /// The stored spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ReconciliationState::Matched => "matched",
            ReconciliationState::Difference => "difference",
            ReconciliationState::Unavailable => "unavailable",
        }
    }
}

/// Compare the conveyor's derived cost with billing's roll-up, to the column's six decimals.
///
/// A difference is never rounded away: anything but an exact match at that precision is a
/// difference, and the caller opens an incident rather than adjusting a figure.
pub fn reconcile(conveyor_usd: f64, billing_usd: f64) -> (ReconciliationState, f64) {
    let delta = ((conveyor_usd - billing_usd) * 1_000_000.0).round() / 1_000_000.0;
    if delta == 0.0 {
        (ReconciliationState::Matched, 0.0)
    } else {
        (ReconciliationState::Difference, delta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thresholds() -> Vec<Threshold> {
        vec![
            Threshold {
                level: MaturityLevel::C0,
                version: 1,
                metrics: BTreeMap::from([("runs.total".to_owned(), 0.0)]),
            },
            Threshold {
                level: MaturityLevel::C1,
                version: 2,
                metrics: BTreeMap::from([("approvals.approval_rate".to_owned(), 0.8)]),
            },
        ]
    }

    #[test]
    fn a_level_below_its_threshold_does_not_move() {
        let evidence = BTreeMap::from([
            ("runs.total".to_owned(), 3.0),
            ("approvals.approval_rate".to_owned(), 0.5),
        ]);
        let states = levels(&thresholds(), &evidence);
        assert_eq!(current_level(&states), Some((MaturityLevel::C0, 1)));
        assert_eq!(states[1].unmet, vec!["approvals.approval_rate".to_owned()]);
    }

    #[test]
    fn crossing_the_threshold_moves_and_names_the_version() {
        let evidence = BTreeMap::from([
            ("runs.total".to_owned(), 3.0),
            ("approvals.approval_rate".to_owned(), 0.9),
        ]);
        let states = levels(&thresholds(), &evidence);
        assert_eq!(current_level(&states), Some((MaturityLevel::C1, 2)));
    }

    #[test]
    fn a_missing_metric_never_lifts_a_level() {
        let evidence = BTreeMap::from([("runs.total".to_owned(), 3.0)]);
        let states = levels(&thresholds(), &evidence);
        assert_eq!(current_level(&states), Some((MaturityLevel::C0, 1)));
    }

    #[test]
    fn an_unknown_metric_or_empty_level_is_refused() {
        assert!(validate_threshold("C1", &BTreeMap::new()).is_err());
        assert!(validate_threshold("C1", &BTreeMap::from([("wishes".to_owned(), 1.0)])).is_err());
        assert!(
            validate_threshold("C1", &BTreeMap::from([("runs.total".to_owned(), -1.0)])).is_err()
        );
        assert!(
            validate_threshold("C1", &BTreeMap::from([("runs.total".to_owned(), 0.0)])).is_ok()
        );
    }

    #[test]
    fn reconciliation_is_exact_to_six_decimals() {
        assert_eq!(reconcile(1.0, 1.0), (ReconciliationState::Matched, 0.0));
        assert_eq!(
            reconcile(1.000001, 1.0),
            (ReconciliationState::Difference, 0.000001)
        );
        // A sub-micro difference is below the stored precision and is a match.
        assert_eq!(
            reconcile(1.0000004, 1.0),
            (ReconciliationState::Matched, 0.0)
        );
    }
}
