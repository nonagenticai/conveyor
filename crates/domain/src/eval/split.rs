#![forbid(unsafe_code)]
//! The eval's deterministic, stratified train/test split (S19 Part A step 6; CONTRACT §20).
//!
//! The split is drawn **without scores**. [`draw_split`]'s argument list is ids and tags — it
//! cannot see a baseline result — so "the split was not chosen by performance" is not a rule
//! anyone has to remember, it is a shape the compiler enforces.
//!
//! Determinism comes from SHA-256, not from an RNG: a case's position inside its stratum is
//! ordered by `content_hash([hex(seed), case_id])`, so the same `(cases, ratio, seed)` produces
//! the same assignment on any machine, on any day, with no dependency to pin and nothing to
//! replay. Stratification is by the **first tag**; a case with no tags is its own stratum (`""`).
//!
//! Two facts the store relies on:
//!
//! - every stratum of two or more cases contributes at least one `test` case
//!   (`train_n = clamp(round(n·ratio), 1, n-1)`); a singleton goes `train`, because a one-case
//!   stratum has no room for both sides;
//! - [`split_sha256`] is order-insensitive and is the same function the store recomputes at
//!   readiness time, so "the split still matches the version" has one implementation, not two.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::EvalError;
use crate::compare::content_hash;

/// Which side of the split a case is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseSplit {
    /// The side a proposed change may be developed against.
    Train,
    /// The side that decides whether the change generalised.
    Test,
}

impl CaseSplit {
    /// The stored `eval_case.split` value.
    pub fn as_str(self) -> &'static str {
        match self {
            CaseSplit::Train => "train",
            CaseSplit::Test => "test",
        }
    }
}

impl fmt::Display for CaseSplit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for CaseSplit {
    type Err = EvalError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw {
            "train" => Ok(CaseSplit::Train),
            "test" => Ok(CaseSplit::Test),
            _ => Err(EvalError::Vocabulary {
                what: "split",
                value: raw.to_owned(),
            }),
        }
    }
}

/// One case as the split sees it: an id and its tags. No score, no input, no answer key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SplitCase<'a> {
    /// The `eval_case.id`.
    pub id: Uuid,
    /// The case's tags; the first one is the stratum.
    pub tags: &'a [String],
}

/// How one stratum was divided.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagCounts {
    /// The stratum: the cases' first tag, or `""` for the untagged group.
    pub tag: String,
    /// Cases in the stratum.
    pub total: usize,
    /// Cases assigned to `train`.
    pub train: usize,
    /// Cases assigned to `test`.
    pub test: usize,
}

/// The result of a draw: every case's side, the canonical hash of the assignment, and the
/// per-stratum counts the response shows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SplitAssignment {
    /// One `(case id, side)` per case, sorted by id.
    pub per_case: Vec<(Uuid, CaseSplit)>,
    /// [`split_sha256`] over `per_case`; the value the version locks and readiness recomputes.
    pub split_sha256: String,
    /// The per-stratum counts, sorted by tag.
    pub per_tag: Vec<TagCounts>,
}

/// Draw a stratified train/test split from ids, tags, a ratio and a seed.
///
/// The signature takes **ids and tags only** — no scores — which is the structural proof that a
/// split cannot be drawn by performance (CONTRACT §20).
///
/// - `train_ratio` must be finite and strictly inside `(0, 1)`; anything else is
///   [`EvalError::Ratio`], and an empty case list is [`EvalError::NoCases`].
/// - Cases are grouped by `tags[0]` (an empty or absent tag list forms the `""` group) and each
///   group is ordered by `content_hash([hex(seed), case_id])`, with the case id as a tie-break,
///   so the order is a total one and never depends on the caller's row order.
/// - Within a group of `n ≥ 2`, `train_n = clamp(round(n · train_ratio), 1, n-1)`; a group of one
///   goes `train`.
///
/// The returned `per_case` is sorted by id and `per_tag` by tag, so two callers that agree on the
/// inputs produce byte-identical results.
pub fn draw_split(
    cases: &[SplitCase<'_>],
    train_ratio: f64,
    seed: &[u8],
) -> Result<SplitAssignment, EvalError> {
    if !train_ratio.is_finite() || train_ratio <= 0.0 || train_ratio >= 1.0 {
        return Err(EvalError::Ratio);
    }
    if cases.is_empty() {
        return Err(EvalError::NoCases);
    }

    let seed_hex = hex(seed);
    let mut groups: BTreeMap<&str, Vec<(String, Uuid)>> = BTreeMap::new();
    for case in cases {
        let tag = case.tags.first().map_or("", String::as_str);
        let order = content_hash(&serde_json::json!([seed_hex.as_str(), case.id.to_string()]));
        groups.entry(tag).or_default().push((order, case.id));
    }

    let mut per_case: Vec<(Uuid, CaseSplit)> = Vec::with_capacity(cases.len());
    let mut per_tag: Vec<TagCounts> = Vec::with_capacity(groups.len());
    for (tag, mut members) in groups {
        members.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
        let total = members.len();
        let train_n = if total < 2 {
            total
        } else {
            let ideal = (total as f64 * train_ratio).round() as usize;
            ideal.clamp(1, total - 1)
        };
        for (index, (_, id)) in members.iter().enumerate() {
            let side = if index < train_n {
                CaseSplit::Train
            } else {
                CaseSplit::Test
            };
            per_case.push((*id, side));
        }
        per_tag.push(TagCounts {
            tag: tag.to_owned(),
            total,
            train: train_n,
            test: total - train_n,
        });
    }

    let hash = split_sha256(&per_case);
    per_case.sort_by_key(|(id, _)| *id);
    Ok(SplitAssignment {
        per_case,
        split_sha256: hash,
        per_tag,
    })
}

/// The canonical hash of an assignment: SHA-256 of the JSON of `[(id, side)]` sorted by id.
///
/// Order-insensitive on purpose — the store recomputes it from whatever row order a query
/// returned and must get the version's locked `split_sha256` back.
pub fn split_sha256(assignments: &[(Uuid, CaseSplit)]) -> String {
    let mut rows: Vec<(Uuid, CaseSplit)> = assignments.to_vec();
    rows.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.as_str().cmp(right.1.as_str()))
    });
    let value = serde_json::json!(
        rows.iter()
            .map(|(id, side)| serde_json::json!([id.to_string(), side.as_str()]))
            .collect::<Vec<_>>()
    );
    content_hash(&value)
}

/// Whether two means are within the noise the run's sample size implies.
///
/// The half-width is `1 / sqrt(n · reps)` — the standard error of a proportion at `p = 0.5`
/// scaled by the repetitions, which is the estimate available before any baseline data exists.
/// Returns `(within, half_width)`; `within` is `|train - test| <= half_width`.
///
/// A zero `n` or `reps` has no measurement to compare, so the half-width is
/// [`f64::INFINITY`] and `within` is true: no data has never proven a difference. In Part A this
/// only produces a warning; §20's refusal is the sign-off gate, not this predicate.
pub fn means_within_noise(train_mean: f64, test_mean: f64, n: usize, reps: u32) -> (bool, f64) {
    let half_width = if n == 0 || reps == 0 {
        f64::INFINITY
    } else {
        1.0 / ((n as f64) * (reps as f64)).sqrt()
    };
    ((train_mean - test_mean).abs() <= half_width, half_width)
}

/// Lowercase hex of the seed bytes, so the order key is one JSON string the canonical form
/// renders identically everywhere.
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Deterministic ids: a failure is reproducible from the test source alone.
    fn case_ids(count: usize) -> Vec<Uuid> {
        (0..count)
            .map(|index| Uuid::from_u128(index as u128 + 1))
            .collect()
    }

    fn group<'a>(ids: &[Uuid], tags: &'a [String]) -> Vec<SplitCase<'a>> {
        ids.iter().map(|id| SplitCase { id: *id, tags }).collect()
    }

    #[test]
    fn eval_case_split_vocabulary_round_trips() {
        for side in [CaseSplit::Train, CaseSplit::Test] {
            assert_eq!(side.as_str().parse::<CaseSplit>(), Ok(side));
            assert_eq!(side.to_string(), side.as_str());
        }
        assert_eq!(CaseSplit::Train.as_str(), "train");
        assert_eq!(CaseSplit::Test.as_str(), "test");
        assert_eq!(
            serde_json::to_value(CaseSplit::Train).expect("split serialises"),
            json!("train")
        );
        assert_eq!(
            "none".parse::<CaseSplit>(),
            Err(EvalError::Vocabulary {
                what: "split",
                value: "none".to_owned(),
            })
        );
    }

    #[test]
    fn eval_split_is_deterministic_for_the_same_seed() {
        let ids = case_ids(8);
        let tags = vec!["suite".to_owned()];
        let cases = group(&ids, &tags);

        let first = draw_split(&cases, 0.5, b"seed-one").expect("first draw");
        let again = draw_split(&cases, 0.5, b"seed-one").expect("second draw");
        assert_eq!(first, again, "same inputs, identical assignment");
        assert_eq!(first.split_sha256, again.split_sha256);
        assert_eq!(first.split_sha256, split_sha256(&first.per_case));
        assert_eq!(first.per_case.len(), 8);
        assert_eq!(first.per_case, {
            let mut sorted = first.per_case.clone();
            sorted.sort_by_key(|(id, _)| *id);
            sorted
        });
        // A caller that shuffles its rows gets the same draw.
        let shuffled: Vec<SplitCase<'_>> = cases.iter().rev().copied().collect();
        assert_eq!(
            draw_split(&shuffled, 0.5, b"seed-one").expect("shuffled draw"),
            first
        );
    }

    #[test]
    fn eval_split_is_stratified_by_the_first_tag() {
        let ids = case_ids(9);
        let alpha = vec!["alpha".to_owned()];
        let beta = vec!["beta".to_owned(), "second".to_owned()];
        let untagged: Vec<String> = Vec::new();

        let mut cases = group(&ids[0..4], &alpha);
        cases.extend(group(&ids[4..8], &beta));
        cases.push(SplitCase {
            id: ids[8],
            tags: &untagged,
        });

        let assignment = draw_split(&cases, 0.5, b"strata").expect("draw");
        assert_eq!(
            assignment.per_tag,
            vec![
                TagCounts {
                    tag: String::new(),
                    total: 1,
                    train: 1,
                    test: 0,
                },
                TagCounts {
                    tag: "alpha".to_owned(),
                    total: 4,
                    train: 2,
                    test: 2,
                },
                TagCounts {
                    tag: "beta".to_owned(),
                    total: 4,
                    train: 2,
                    test: 2,
                },
            ],
            "a tag group of four at 0.5 is two and two; the untagged case is its own group"
        );

        // Every stratum of two or more has both sides; the singleton is on train.
        for counts in &assignment.per_tag {
            if counts.total >= 2 {
                assert!(counts.train >= 1, "{counts:?}");
                assert!(counts.test >= 1, "{counts:?}");
            }
            assert_eq!(counts.train + counts.test, counts.total, "{counts:?}");
        }
        let side_of = |id: Uuid| {
            assignment
                .per_case
                .iter()
                .find(|(case_id, _)| *case_id == id)
                .map(|(_, side)| *side)
                .expect("every case is assigned")
        };
        assert_eq!(side_of(ids[8]), CaseSplit::Train, "a singleton goes train");
    }

    #[test]
    fn eval_split_sends_a_singleton_to_train_at_any_ratio() {
        let ids = case_ids(1);
        let tags = vec!["only".to_owned()];
        let cases = group(&ids, &tags);
        for ratio in [0.01, 0.5, 0.99] {
            let assignment = draw_split(&cases, ratio, b"one").expect("draw");
            assert_eq!(assignment.per_case, vec![(ids[0], CaseSplit::Train)]);
            assert_eq!(assignment.per_tag[0].train, 1);
            assert_eq!(assignment.per_tag[0].test, 0);
        }
    }

    #[test]
    fn eval_split_changes_with_the_seed() {
        let ids = case_ids(4);
        let tags = vec!["suite".to_owned()];
        let cases = group(&ids, &tags);

        let mut distinct: Vec<SplitAssignment> = Vec::new();
        for seed in [
            &b"seed-a"[..],
            &b"seed-b"[..],
            &b"seed-c"[..],
            &b"seed-d"[..],
            &b"seed-e"[..],
            &b"seed-f"[..],
        ] {
            let drawn = draw_split(&cases, 0.5, seed).expect("draw");
            assert_eq!(drawn.per_case.len(), 4);
            if !distinct.contains(&drawn) {
                distinct.push(drawn);
            }
        }
        assert!(
            distinct.len() > 1,
            "the seed must reach the whole assignment, not one id: {distinct:?}"
        );
    }

    #[test]
    fn eval_split_rejects_a_ratio_outside_the_open_unit_interval() {
        let ids = case_ids(4);
        let tags = vec!["suite".to_owned()];
        let cases = group(&ids, &tags);

        for ratio in [
            0.0,
            1.0,
            -0.5,
            1.5,
            100.0,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ] {
            assert_eq!(
                draw_split(&cases, ratio, b"seed"),
                Err(EvalError::Ratio),
                "ratio {ratio} must be refused"
            );
        }
        for ratio in [0.01, 0.25, 0.5, 0.75, 0.99] {
            assert!(draw_split(&cases, ratio, b"seed").is_ok(), "ratio {ratio}");
        }
    }

    #[test]
    fn eval_split_needs_at_least_one_case() {
        assert_eq!(draw_split(&[], 0.5, b"seed"), Err(EvalError::NoCases));
    }

    #[test]
    fn eval_split_hash_is_order_insensitive_and_covers_every_case() {
        let first = Uuid::from_u128(1);
        let second = Uuid::from_u128(2);
        let forward = vec![(first, CaseSplit::Train), (second, CaseSplit::Test)];
        let reverse = vec![(second, CaseSplit::Test), (first, CaseSplit::Train)];
        assert_eq!(split_sha256(&forward), split_sha256(&reverse));

        let flipped = vec![(first, CaseSplit::Test), (second, CaseSplit::Test)];
        assert_ne!(
            split_sha256(&forward),
            split_sha256(&flipped),
            "moving one case to the other side must change the hash"
        );
        let dropped = vec![(first, CaseSplit::Train)];
        assert_ne!(split_sha256(&forward), split_sha256(&dropped));

        let hash = split_sha256(&forward);
        assert_eq!(hash.len(), 64);
        assert!(
            hash.chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
            "lowercase hex: {hash}"
        );
        assert_eq!(split_sha256(&[]), content_hash(&json!([])));
    }

    #[test]
    fn eval_means_within_noise_boundary() {
        let (within, half_width) = means_within_noise(0.0, 0.5, 4, 1);
        assert!(within, "exactly at the half-width is within noise");
        assert_eq!(half_width, 0.5);

        let (within, half_width) = means_within_noise(0.0, 0.6, 4, 1);
        assert!(!within);
        assert_eq!(half_width, 0.5);

        let (within, half_width) = means_within_noise(2.0, 1.5, 4, 1);
        assert!(within, "the comparison is on the absolute difference");
        assert_eq!(half_width, 0.5);

        let (within, half_width) = means_within_noise(0.0, 0.1, 25, 4);
        assert!(within);
        assert_eq!(half_width, 0.1);

        let (within, half_width) = means_within_noise(0.0, 0.11, 25, 4);
        assert!(!within);
        assert_eq!(half_width, 0.1);

        let (within, half_width) = means_within_noise(0.0, 10.0, 0, 1);
        assert!(within, "no cases cannot prove a difference");
        assert_eq!(half_width, f64::INFINITY);

        let (_, half_width) = means_within_noise(0.0, 0.0, 4, 0);
        assert_eq!(half_width, f64::INFINITY, "no repetitions cannot either");
    }
}
