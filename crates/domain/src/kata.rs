//! Kata — the measured system-improvement loop (SPEC_V2 §6.3, §9.3–§9.4; T5, issue #77).
//!
//! Pure rules only. The store writes what these agree to and the action layer is the transport;
//! the database refuses the vocabulary.
//!
//! P1's two properties live here in their arithmetic form:
//!
//! - **the held-out wall** ([`HELD_OUT_SPLIT`]): a measurement reads one side of the locked split
//!   and says which side it read; a training-split number is not a kata measurement;
//! - **the movement** ([`Movement`]): per-case means first, then the case mean, so a case with
//!   five reps cannot outvote a case with one. The delta's sign is raw; the metric's `better`
//!   direction is what turns it into "improved".

use std::collections::BTreeMap;
use std::fmt;

use uuid::Uuid;

/// The only split a kata measurement reads (§9.4, P1.2).
pub const HELD_OUT_SPLIT: &str = "test";

/// The surface a kata changes (§6.3). The adoption gate and the instrument are per surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    Procedure,
    Prompt,
    Knowledge,
    Tool,
    JobConfig,
    Code,
}

impl Surface {
    /// The database and wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Procedure => "procedure",
            Self::Prompt => "prompt",
            Self::Knowledge => "knowledge",
            Self::Tool => "tool",
            Self::JobConfig => "job_config",
            Self::Code => "code",
        }
    }

    /// Parse the wire spelling; `None` is a caller error the action reports.
    pub fn parse(raw: &str) -> Option<Self> {
        Some(match raw {
            "procedure" => Self::Procedure,
            "prompt" => Self::Prompt,
            "knowledge" => Self::Knowledge,
            "tool" => Self::Tool,
            "job_config" => Self::JobConfig,
            "code" => Self::Code,
            _ => return None,
        })
    }
}

impl fmt::Display for Surface {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The six Gets (§6.3). The record stores the ones a stage has completed; the order is the
/// procedure's, not the caller's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Get {
    /// Go to See.
    Observe,
    /// Grasp the Situation.
    Analyse,
    /// The pre-registered prediction and instrument version — before the candidate exists.
    Hypothesis,
    /// Get to Solution: the candidate change.
    Solution,
    /// Get to Standardization.
    Standardisation,
    /// Get to Sustainability.
    Sustainability,
    /// Get to Stretch (yokoten).
    Stretch,
}

impl Get {
    /// The database and wire spelling.
    pub fn key(self) -> &'static str {
        match self {
            Self::Observe => "observe",
            Self::Analyse => "analyse",
            Self::Hypothesis => "hypothesis",
            Self::Solution => "solution",
            Self::Standardisation => "standardisation",
            Self::Sustainability => "sustainability",
            Self::Stretch => "stretch",
        }
    }

    /// Parse the wire spelling.
    pub fn parse(raw: &str) -> Option<Self> {
        Some(match raw {
            "observe" => Self::Observe,
            "analyse" => Self::Analyse,
            "hypothesis" => Self::Hypothesis,
            "solution" => Self::Solution,
            "standardisation" => Self::Standardisation,
            "sustainability" => Self::Sustainability,
            "stretch" => Self::Stretch,
            _ => return None,
        })
    }

    /// The kata status this Get requires before it may be recorded.
    pub fn requires_status(self) -> Option<&'static str> {
        match self {
            Self::Observe => Some("open"),
            Self::Analyse => Some("observed"),
            Self::Hypothesis => Some("analysed"),
            Self::Solution => Some("hypothesised"),
            // The last three Gets come after the person's adoption: the standard is updated,
            // then its monitoring is set, then it is propagated. They all require `adopted`;
            // their order among themselves is the store's predecessor check, because the
            // status vocabulary has one post-adoption value.
            Self::Standardisation => Some("adopted"),
            Self::Sustainability => Some("adopted"),
            Self::Stretch => Some("adopted"),
        }
    }

    /// The kata status this Get completes.
    pub fn completes_status(self) -> &'static str {
        match self {
            Self::Observe => "observed",
            Self::Analyse => "analysed",
            Self::Hypothesis => "hypothesised",
            Self::Solution => "proposed",
            // The standardisation and its monitoring leave the kata adopted; the propagation
            // (yokoten) is what closes it.
            Self::Standardisation | Self::Sustainability => "adopted",
            Self::Stretch => "closed",
        }
    }
}

/// One graded result for one case (a repetition). Aggregation happens in [`aggregate`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CaseScore {
    /// The case the score belongs to.
    pub case_id: Uuid,
    /// The metric's score for this repetition.
    pub score: f64,
}

/// A split's number: the per-case mean over repetitions, then the mean over cases.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aggregate {
    /// How many distinct cases contributed.
    pub cases: usize,
    /// The mean of the per-case means.
    pub mean: f64,
}

/// Aggregate graded results the way the instrument reports a split: one mean per case (so a
/// case with many reps cannot outvote a case with one), then the mean over cases.
///
/// Returns `None` when no case contributed — "no data, no improvement" is a refusal, never a
/// zero.
pub fn aggregate(scores: &[CaseScore]) -> Option<Aggregate> {
    let mut per_case: BTreeMap<Uuid, (f64, usize)> = BTreeMap::new();
    for score in scores {
        if !score.score.is_finite() {
            continue;
        }
        let entry = per_case.entry(score.case_id).or_insert((0.0, 0));
        entry.0 += score.score;
        entry.1 += 1;
    }
    if per_case.is_empty() {
        return None;
    }
    let means: Vec<f64> = per_case
        .values()
        .map(|(sum, count)| sum / *count as f64)
        .collect();
    Some(Aggregate {
        cases: means.len(),
        mean: means.iter().sum::<f64>() / means.len() as f64,
    })
}

/// The candidate's movement against its baseline on one split and metric.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Movement {
    /// The candidate's aggregate.
    pub candidate: Aggregate,
    /// The baseline's aggregate.
    pub baseline: Aggregate,
    /// `candidate.mean - baseline.mean` (raw, never direction-corrected).
    pub delta: f64,
}

impl Movement {
    /// Whether the number moved at all. A flat result is never an adoption (SPEC §9.3).
    pub fn moved(&self) -> bool {
        self.delta.is_finite() && self.delta != 0.0
    }

    /// The delta expressed so that positive always means better.
    pub fn directed_delta(&self, better: &str) -> f64 {
        if better == "lower" {
            -self.delta
        } else {
            self.delta
        }
    }
}

/// The movement between two measured splits. `None` when either side contributed no case — the
/// pair is incomparable and P2's rule adopts nothing.
pub fn movement(candidate: &[CaseScore], baseline: &[CaseScore]) -> Option<Movement> {
    let candidate = aggregate(candidate)?;
    let baseline = aggregate(baseline)?;
    Some(Movement {
        candidate,
        baseline,
        delta: candidate.mean - baseline.mean,
    })
}

/// What a measurement's directed movement means against the kata's recorded threshold
/// (SPEC_V2 §9.3; P2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// The metric moved in the better direction: the candidate is a measured improvement and
    /// may go to the person's adoption gate.
    Improved,
    /// The metric did not move, or moved worse by less than the recorded threshold. Nothing is
    /// adopted (P2.2).
    Flat,
    /// The metric moved worse by more than the recorded threshold: the candidate is reverted.
    Regressed,
}

impl Verdict {
    /// The database and wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Improved => "improved",
            Self::Flat => "flat",
            Self::Regressed => "regressed",
        }
    }
}

/// Compare a directed delta (`positive` always means better) against the kata's recorded
/// threshold. The threshold is the regression boundary: a movement worse than `-threshold` is
/// a regression; any movement worse than zero but within the threshold is flat — it adopts
/// nothing, and it does not revert either.
pub fn verdict(directed_delta: f64, threshold: f64) -> Verdict {
    if !directed_delta.is_finite() || directed_delta == 0.0 {
        return Verdict::Flat;
    }
    if directed_delta > 0.0 {
        return Verdict::Improved;
    }
    if -directed_delta > threshold.abs() {
        Verdict::Regressed
    } else {
        Verdict::Flat
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case(n: u8, score: f64) -> CaseScore {
        CaseScore {
            case_id: Uuid::from_u128(n as u128),
            score,
        }
    }

    #[test]
    fn a_case_with_many_reps_cannot_outvote_a_case_with_one() {
        // Case 1: three reps at 0.0. Case 2: one rep at 1.0. The split is 0.5, not 0.25.
        let scores = [case(1, 0.0), case(1, 0.0), case(1, 0.0), case(2, 1.0)];
        let aggregate = aggregate(&scores).expect("two cases");
        assert_eq!(aggregate.cases, 2);
        assert!((aggregate.mean - 0.5).abs() < 1e-12);
    }

    #[test]
    fn no_cases_is_none_never_zero() {
        assert!(aggregate(&[]).is_none());
        assert!(movement(&[], &[case(1, 0.5)]).is_none());
    }

    #[test]
    fn the_delta_is_raw_and_the_direction_is_the_metrics() {
        let up = movement(&[case(1, 0.8)], &[case(1, 0.5)]).expect("comparable");
        assert!((up.delta - 0.3).abs() < 1e-12);
        assert!(up.moved());
        assert!((up.directed_delta("higher") - 0.3).abs() < 1e-12);
        assert!((up.directed_delta("lower") + 0.3).abs() < 1e-12);
        let flat = movement(&[case(1, 0.5)], &[case(1, 0.5)]).expect("comparable");
        assert!(!flat.moved(), "a flat result adopts nothing");
    }

    #[test]
    fn the_threshold_is_the_regression_boundary_not_the_adoption_bar() {
        assert_eq!(verdict(0.4, 0.1), Verdict::Improved);
        assert_eq!(verdict(0.05, 0.1), Verdict::Improved);
        assert_eq!(verdict(0.0, 0.1), Verdict::Flat);
        assert_eq!(verdict(-0.05, 0.1), Verdict::Flat);
        assert_eq!(
            verdict(-0.1, 0.1),
            Verdict::Flat,
            "at the boundary is not past it"
        );
        assert_eq!(verdict(-0.11, 0.1), Verdict::Regressed);
        assert_eq!(verdict(-0.01, 0.0), Verdict::Regressed);
    }
}
