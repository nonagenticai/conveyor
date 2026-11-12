//! Exact two-sided McNemar over the instances two runs both graded.
//!
//! Ported from `conveyor-bench/scripts/report-arms.py:150-193`, including the refusal rule:
//!
//! ```text
//! p = min(1, 2 · Σ_{i=0..min(a_only, b_only)} C(n, i) / 2^n),   n = a_only + b_only
//! ```
//!
//! The pairing is over the **intersection** of the two verdict maps; instances graded in only
//! one arm are counted in `missing_in_one` and never silently dropped (CONTRACT §19).

use std::collections::BTreeMap;

use thiserror::Error;

use super::arm::Arm;

/// The 2×2 table and the pair bookkeeping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PairedCounts {
    /// Instances both runs resolved.
    pub both: usize,
    /// Instances neither run resolved.
    pub neither: usize,
    /// Resolved by `a` only.
    pub a_only: usize,
    /// Resolved by `b` only.
    pub b_only: usize,
    /// Instances present in both verdict maps (`both + neither + a_only + b_only`).
    pub paired: usize,
    /// Instances graded in exactly one arm and therefore excluded from the pairing.
    pub missing_in_one: usize,
}

/// The paired comparison's result.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct McNemar {
    pub counts: PairedCounts,
    pub p_value: f64,
    /// No discordant pair: the arms agree on every instance, so no test is performed (`p = 1`).
    pub identical: bool,
}

/// Why a comparison was refused.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum CompareError {
    /// The runs' oracles and/or prompts differ, so they did not measure the same task
    /// (`report-arms.py:160-168`). No p-value is computed.
    #[error("not comparable: oracle/prompt differ (a: {a:?}, b: {b:?}); no p-value is computed")]
    NotComparable { a: Arm, b: Arm },
}

/// Compare two graded runs. `a` and `b` map instance id → resolved.
pub fn compare(
    a_arm: Arm,
    b_arm: Arm,
    a: &BTreeMap<String, bool>,
    b: &BTreeMap<String, bool>,
) -> Result<McNemar, CompareError> {
    if !a_arm.comparable(&b_arm) {
        return Err(CompareError::NotComparable { a: a_arm, b: b_arm });
    }

    let paired: Vec<(&String, bool, bool)> = a
        .iter()
        .filter_map(|(id, a_resolved)| b.get(id).map(|b_resolved| (id, *a_resolved, *b_resolved)))
        .collect();

    let both = paired.iter().filter(|(_, x, y)| *x && *y).count();
    let neither = paired.iter().filter(|(_, x, y)| !*x && !*y).count();
    let a_only = paired.iter().filter(|(_, x, y)| *x && !*y).count();
    let b_only = paired.iter().filter(|(_, x, y)| !*x && *y).count();
    let missing_in_one = a.len() + b.len() - 2 * paired.len();

    let counts = PairedCounts {
        both,
        neither,
        a_only,
        b_only,
        paired: paired.len(),
        missing_in_one,
    };

    let n = a_only + b_only;
    if n == 0 {
        return Ok(McNemar {
            counts,
            p_value: 1.0,
            identical: true,
        });
    }

    let k = a_only.min(b_only);
    // Exact integer arithmetic: at the 50-instance scale both C(n, i) and 2^n fit in f64
    // exactly (< 2^53); above 127 the shift would overflow, so fall back to the f64 form.
    let tail: u128 = (0..=k).map(|i| binomial(n, i)).sum();
    let p_value = if n < 128 {
        let denom = 1u128 << n;
        (2.0 * tail as f64 / denom as f64).min(1.0)
    } else {
        (2.0 * tail as f64 / 2f64.powi(n as i32)).min(1.0)
    };

    Ok(McNemar {
        counts,
        p_value,
        identical: false,
    })
}

/// `C(n, k)` in exact integer arithmetic. `k <= n` is the caller's invariant.
fn binomial(n: usize, k: usize) -> u128 {
    let k = k.min(n - k);
    let mut out: u128 = 1;
    for i in 0..k {
        out = out * (n - i) as u128 / (i + 1) as u128;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::benchmark::arm::Arm;

    fn verdicts(resolved: &[&str], unresolved: &[&str]) -> BTreeMap<String, bool> {
        let mut m = BTreeMap::new();
        for id in resolved {
            m.insert((*id).to_owned(), true);
        }
        for id in unresolved {
            m.insert((*id).to_owned(), false);
        }
        m
    }

    fn arm_ft() -> Arm {
        Arm::derive("conveyor-oracle-files+failing-test")
    }

    fn arm_ft_other() -> Arm {
        Arm::derive("qwen-oneshot-oracle-files+failing-test")
    }

    #[test]
    fn the_first_banked_vector_is_p_0180() {
        // report-arms.py:150-158: 21 both, 20 neither, 2 a-only, 7 b-only -> 9 discordant.
        let mut a = BTreeMap::new();
        let mut b = BTreeMap::new();
        for i in 0..21 {
            a.insert(format!("both-{i}"), true);
            b.insert(format!("both-{i}"), true);
        }
        for i in 0..20 {
            a.insert(format!("neither-{i}"), false);
            b.insert(format!("neither-{i}"), false);
        }
        for i in 0..2 {
            a.insert(format!("a-only-{i}"), true);
            b.insert(format!("a-only-{i}"), false);
        }
        for i in 0..7 {
            a.insert(format!("b-only-{i}"), false);
            b.insert(format!("b-only-{i}"), true);
        }
        let m = compare(arm_ft(), arm_ft_other(), &a, &b).expect("comparable");
        assert_eq!(m.counts.both, 21);
        assert_eq!(m.counts.neither, 20);
        assert_eq!(m.counts.a_only, 2);
        assert_eq!(m.counts.b_only, 7);
        assert_eq!(m.counts.paired, 50);
        assert_eq!(m.counts.missing_in_one, 0);
        assert!((m.p_value - 0.180).abs() < 0.0005, "p = {}", m.p_value);
        assert!(!m.identical);
    }

    #[test]
    fn the_second_banked_vector_is_p_0289() {
        // The valid 2026-09-17 pair: 22 both, 20 neither, 2 conveyor-only, 6 qwen-only.
        let mut a = BTreeMap::new();
        let mut b = BTreeMap::new();
        for i in 0..22 {
            a.insert(format!("both-{i}"), true);
            b.insert(format!("both-{i}"), true);
        }
        for i in 0..20 {
            a.insert(format!("neither-{i}"), false);
            b.insert(format!("neither-{i}"), false);
        }
        for i in 0..2 {
            a.insert(format!("a-only-{i}"), true);
            b.insert(format!("a-only-{i}"), false);
        }
        for i in 0..6 {
            a.insert(format!("b-only-{i}"), false);
            b.insert(format!("b-only-{i}"), true);
        }
        let m = compare(arm_ft(), arm_ft_other(), &a, &b).expect("comparable");
        assert_eq!(
            (
                m.counts.both,
                m.counts.neither,
                m.counts.a_only,
                m.counts.b_only
            ),
            (22, 20, 2, 6)
        );
        assert!((m.p_value - 0.289).abs() < 0.0005, "p = {}", m.p_value);
    }

    #[test]
    fn a_pair_of_incomparable_arms_is_refused_before_any_counting() {
        let a = verdicts(&["x"], &[]);
        let b = verdicts(&["x"], &[]);
        let err = compare(
            Arm::derive("conveyor-oracle-files"),
            Arm::derive("qwen-oneshot-oracle-files+failing-test"),
            &a,
            &b,
        )
        .expect_err("oracle differs");
        match err {
            CompareError::NotComparable { a, b } => {
                assert_eq!(a.oracle, crate::benchmark::arm::Oracle::Files);
                assert_eq!(b.oracle, crate::benchmark::arm::Oracle::FilesAndTest);
            }
        }
    }

    #[test]
    fn missing_pairs_are_reported_not_dropped() {
        let a = verdicts(&["x", "only-a"], &["y"]);
        let b = verdicts(&["x"], &["y", "only-b"]);
        let m = compare(arm_ft(), arm_ft_other(), &a, &b).expect("comparable");
        assert_eq!(m.counts.paired, 2);
        assert_eq!(m.counts.missing_in_one, 2);
        assert_eq!(m.counts.both, 1);
        assert_eq!(m.counts.neither, 1);
        assert!(m.identical);
        assert_eq!(m.p_value, 1.0);
    }

    #[test]
    fn an_empty_intersection_still_reports_the_missing_pairs() {
        let a = verdicts(&["a"], &[]);
        let b = verdicts(&["b"], &[]);
        let m = compare(arm_ft(), arm_ft_other(), &a, &b).expect("comparable");
        assert_eq!(m.counts.paired, 0);
        assert_eq!(m.counts.missing_in_one, 2);
        assert_eq!(m.p_value, 1.0);
    }
}
