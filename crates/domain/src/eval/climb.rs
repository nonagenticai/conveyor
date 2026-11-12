#![forbid(unsafe_code)]
//! The hillclimb's pure core (S20; SPEC D26, CONTRACT §20).
//!
//! A climb is a sequence of `eval_run` variants over **one** eval version, and this module is the
//! machine that decides what each round meant. Nothing here performs IO: the store writes what
//! these functions resolved, and `eval_climbs.*` is the only caller.
//!
//! Four decisions live here and nowhere else, because each is the kind that gets re-litigated in
//! a handler if it is allowed to spread:
//!
//! - **Keep or revert** — [`keep_or_revert`]. The headline is the **held-out** number and the
//!   train split is the corroboration, never the other way round. A change that lifts train and
//!   leaves test flat is reverted: that is the shape of fitting the visible set.
//! - **When to stop** — [`stopping_reached`]. A plateau is `plateau_k` consecutive flat rounds,
//!   and `plateau_k >= 3` because two flat rounds is still noise (CONTRACT §20, trap 3).
//! - **When content has stopped working** — [`triage_required`] / [`content_round_refused`].
//!   After two flat rounds the next round must categorise the failure; a fourth consecutive
//!   content round against the same wall is refused.
//! - **What is comparable** — [`comparable`]. Two runs from different versions, grader sets or
//!   splits did not measure the same thing, and a delta between them is a number with no meaning.
//!
//! The one thing this module deliberately does **not** own is the exact test: the paired
//! comparison calls [`crate::benchmark::mcnemar::compare`], the estate's single implementation
//! (S17). A second McNemar would be a silent divergence in a published number.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::eval::split::means_within_noise;

/// The metric name that turns a climb into a `cost-hillclimb`.
pub const COST_TARGET: &str = "cost_usd";

/// The cost walk's lever order, from `cost-hillclimb.md`.
///
/// It is a **state machine on `eval_climb.goal`**, not advice: `goal.lever` names the position,
/// a round must be fought at the current lever, and only a kept round advances it. A resumed
/// session therefore does not restart the staircase at the top.
pub const LEVERS: [&str; 5] = [
    "caching_health",
    "prompt_audit",
    "model_effort",
    "prompt_climb",
    "joint_confirm",
];

/// How many consecutive flat rounds are allowed before a round must categorise.
pub const FLAT_ROUNDS_BEFORE_TRIAGE: i32 = 2;

/// How many consecutive flat rounds make a further content round a refusal.
pub const FLAT_ROUNDS_BEFORE_REFUSAL: i32 = 3;

/// Why a climb value was refused. Vocabulary problems, never legality of a stored row.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum ClimbError {
    /// A field is not in its documented vocabulary.
    #[error("unknown {what} {value:?}")]
    Vocabulary {
        /// The field or vocabulary that refused the value.
        what: &'static str,
        /// The refused spelling.
        value: String,
    },
    /// The `goal` document is not a well-formed goal.
    #[error("goal: {0}")]
    Goal(String),
    /// The pre-registered prediction is not usable.
    #[error("prediction: {0}")]
    Prediction(String),
    /// A categorisation round's bucket report is not usable.
    #[error("buckets: {0}")]
    Buckets(String),
    /// `delta` is not a positive finite number.
    #[error("delta must be a positive finite number")]
    Delta,
}

/// Which way a goal improves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// A larger number is better (accuracy, pass rate).
    Higher,
    /// A smaller number is better (cost, latency).
    Lower,
}

impl Direction {
    /// Parse the column and wire spelling.
    pub fn parse(raw: &str) -> Result<Self, ClimbError> {
        match raw {
            "higher" => Ok(Direction::Higher),
            "lower" => Ok(Direction::Lower),
            other => Err(ClimbError::Vocabulary {
                what: "direction",
                value: other.to_owned(),
            }),
        }
    }

    /// The column spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Direction::Higher => "higher",
            Direction::Lower => "lower",
        }
    }

    /// How `after` compares with `before`, as an improvement: `Up` is better, whatever the sign.
    ///
    /// `delta` is the climb's noise-band width, so a movement smaller than it is `Flat` — the
    /// whole point of setting `delta` above the measured floor is that a smaller move cannot be
    /// told from noise.
    ///
    /// The band's edge is **inclusive** ("moved by at least `delta`"), and the comparison carries
    /// a relative tolerance because `0.55 - 0.60` is `-0.04999999999999993` in f64: without the
    /// tolerance a movement of exactly one delta lands one ulp inside the band and reads as a tie,
    /// silently turning a real improvement into a `no_change` revert.
    pub fn movement(self, before: f64, after: f64, delta: f64) -> Movement {
        let signed = match self {
            Direction::Higher => after - before,
            Direction::Lower => before - after,
        };
        let band = delta - delta.abs() * 1e-9;
        if signed >= band {
            Movement::Up
        } else if signed <= -band {
            Movement::Down
        } else {
            Movement::Flat
        }
    }
}

/// One signal's direction of travel between two rounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Movement {
    /// Improved by at least the climb's `delta`.
    Up,
    /// Moved by less than `delta` in either direction.
    Flat,
    /// Worsened by at least the climb's `delta`.
    Down,
}

/// A guardrail for a cost walk: a quality metric that must not fall below `min`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hold {
    /// The `eval_metric.key` being protected.
    pub metric: String,
    /// The floor it must stay at or above.
    pub min: f64,
}

/// What the climb is trying to move, and what must not move with it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Goal {
    /// The metric key being moved, or [`COST_TARGET`].
    pub target: String,
    /// Which way it improves.
    pub direction: Direction,
    /// Guardrails. Required (non-empty) when `target = cost_usd`.
    #[serde(default)]
    pub hold: Vec<Hold>,
    /// The cost walk's position in [`LEVERS`]; always `0` for a quality goal.
    #[serde(default)]
    pub lever: usize,
    /// Set once, by `eval_climbs.stop`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop: Option<StopRecord>,
}

/// Why and when the climb closed, kept in `goal` because §2.20 has no reason column.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StopRecord {
    /// The operator's reason.
    pub reason: String,
    /// When it was recorded.
    pub at: String,
}

impl Goal {
    /// Parse and validate a goal document from the wire.
    pub fn parse(value: &serde_json::Value) -> Result<Self, ClimbError> {
        let object = value
            .as_object()
            .ok_or_else(|| ClimbError::Goal("`goal` must be an object".to_owned()))?;
        let target = object
            .get("target")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|target| !target.is_empty())
            .ok_or_else(|| ClimbError::Goal("`goal.target` is required".to_owned()))?
            .to_owned();
        let direction = object
            .get("direction")
            .and_then(|value| value.as_str())
            .map(Direction::parse)
            .transpose()?
            .unwrap_or(Direction::Higher);

        let mut hold = Vec::new();
        if let Some(entries) = object.get("hold") {
            let entries = entries
                .as_array()
                .ok_or_else(|| ClimbError::Goal("`goal.hold` must be an array".to_owned()))?;
            for entry in entries {
                let metric = entry
                    .get("metric")
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|metric| !metric.is_empty())
                    .ok_or_else(|| ClimbError::Goal("every hold needs a `metric`".to_owned()))?;
                let min = entry
                    .get("min")
                    .and_then(|value| value.as_f64())
                    .ok_or_else(|| {
                        ClimbError::Goal("every hold needs a numeric `min`".to_owned())
                    })?;
                if !min.is_finite() {
                    return Err(ClimbError::Goal("a hold's `min` must be finite".to_owned()));
                }
                hold.push(Hold {
                    metric: metric.to_owned(),
                    min,
                });
            }
        }

        let lever = object
            .get("lever")
            .and_then(|value| value.as_u64())
            .map(|value| value as usize)
            .unwrap_or(0);
        if lever >= LEVERS.len() {
            return Err(ClimbError::Goal(format!(
                "`goal.lever` must be 0..{}",
                LEVERS.len()
            )));
        }

        let stop = object
            .get("stop")
            .filter(|value| !value.is_null())
            .map(|value| {
                serde_json::from_value::<StopRecord>(value.clone())
                    .map_err(|error| ClimbError::Goal(format!("`goal.stop`: {error}")))
            })
            .transpose()?;

        let goal = Goal {
            target,
            direction,
            hold,
            lever,
            stop,
        };
        goal.validate()?;
        Ok(goal)
    }

    /// The rules that do not depend on a caller: a cost walk carries at least one guardrail, and a
    /// quality goal never pretends to be at a lever.
    pub fn validate(&self) -> Result<(), ClimbError> {
        if self.is_cost() {
            if self.hold.is_empty() {
                return Err(ClimbError::Goal(
                    "a `cost_usd` climb needs at least one quality `hold` — a cost search with no \
                     guardrail is a race to the cheapest wrong answer"
                        .to_owned(),
                ));
            }
            if self.direction != Direction::Lower {
                return Err(ClimbError::Goal(
                    "a `cost_usd` climb always improves downward".to_owned(),
                ));
            }
        } else {
            if self.lever != 0 {
                return Err(ClimbError::Goal(
                    "`goal.lever` is the cost walk's position and is unused by a quality goal"
                        .to_owned(),
                ));
            }
            if self.hold.iter().any(|hold| hold.metric == self.target) {
                return Err(ClimbError::Goal(format!(
                    "`{}` cannot be both the target and its own guardrail",
                    self.target
                )));
            }
        }
        Ok(())
    }

    /// True when this is a `cost-hillclimb`.
    pub fn is_cost(&self) -> bool {
        self.target == COST_TARGET
    }

    /// The lever a round must be fought at, when this is a cost walk.
    pub fn current_lever(&self) -> Option<&'static str> {
        if self.is_cost() {
            LEVERS.get(self.lever).copied()
        } else {
            None
        }
    }

    /// Whether `named` is the lever the walk is currently standing on.
    ///
    /// Skipping a lever is refused: the order exists because a cache fix costs nothing and a
    /// model swap costs a re-measurement of everything.
    pub fn lever_allows(&self, named: Option<&str>) -> bool {
        match (self.current_lever(), named) {
            (None, _) => true,
            (Some(current), Some(named)) => named == current,
            (Some(_), None) => false,
        }
    }

    /// Advance the staircase after a kept round. A reverted round stays where it is.
    pub fn advance_lever(&mut self) {
        if self.is_cost() && self.lever + 1 < LEVERS.len() {
            self.lever += 1;
        }
    }
}

/// A round's pre-registered prediction. Registering it before the run is what makes the round a
/// test of a claim rather than a story told afterwards.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Prediction {
    /// The metric this round claims to move. Must be the climb's target.
    pub metric: String,
    /// Which way it claims to move it.
    pub direction: Direction,
    /// The predicted range of the gain, `[low, high]` in the metric's own units.
    pub interval: [f64; 2],
    /// What observation would prove the prediction wrong.
    pub falsifier: String,
}

impl Prediction {
    /// Parse and validate a prediction from the wire.
    pub fn parse(value: &serde_json::Value) -> Result<Self, ClimbError> {
        let object = value
            .as_object()
            .ok_or_else(|| ClimbError::Prediction("`prediction` must be an object".to_owned()))?;
        let metric = object
            .get("metric")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|metric| !metric.is_empty())
            .ok_or_else(|| ClimbError::Prediction("`prediction.metric` is required".to_owned()))?;
        let direction = object
            .get("direction")
            .and_then(|value| value.as_str())
            .map(Direction::parse)
            .transpose()?
            .unwrap_or(Direction::Higher);
        let interval = object
            .get("interval")
            .and_then(|value| value.as_array())
            .filter(|values| values.len() == 2)
            .and_then(|values| Some([values[0].as_f64()?, values[1].as_f64()?]))
            .ok_or_else(|| {
                ClimbError::Prediction(
                    "`prediction.interval` must be `[low, high]`, two numbers".to_owned(),
                )
            })?;
        if !interval[0].is_finite() || !interval[1].is_finite() || interval[0] > interval[1] {
            return Err(ClimbError::Prediction(
                "`prediction.interval` must be `[low, high]` with low <= high".to_owned(),
            ));
        }
        if interval[0] < 0.0 {
            return Err(ClimbError::Prediction(
                "`prediction.interval` is a pair of gain MAGNITUDES in the improving direction; \
                 express a `lower` metric's expected fall as a positive number, not a negative one"
                    .to_owned(),
            ));
        }
        let falsifier = object
            .get("falsifier")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .ok_or_else(|| {
                ClimbError::Prediction("`prediction.falsifier` is required".to_owned())
            })?;
        Ok(Prediction {
            metric: metric.to_owned(),
            direction,
            interval,
            falsifier: falsifier.to_owned(),
        })
    }

    /// The best case the change claims — the **upper** end of the predicted gain.
    ///
    /// `interval` is always a pair of gain **magnitudes** in the improving direction (`[0.10,
    /// 0.30]` reads "moves the target by between 0.10 and 0.30, in the good direction"), whichever
    /// way the metric improves. Expressing a `lower` metric's gain as a negative interval is
    /// refused at parse, because the sign would then double as the direction and two callers
    /// could mean opposite things with the same numbers.
    ///
    /// The **optimistic** end is the one that matters: if even the best case sits inside the noise
    /// floor the keep/revert decision is a coin flip.
    pub fn best_case(&self) -> f64 {
        self.interval[1]
    }
}

/// One settled round's numbers, all computed in Rust from the run's result rows.
#[derive(Clone, Debug, PartialEq)]
pub struct RoundOutcome {
    /// The climb round this came from (`eval_run.round`).
    pub round: i32,
    /// Mean score over the `train` split.
    pub train: f64,
    /// Mean score over the `test` split — the headline.
    pub test: f64,
    /// Mean cost per case in cents, for a cost walk.
    pub cost_cents: Option<f64>,
    /// How many results each mean is over.
    pub n_train: usize,
    /// How many results the test mean is over.
    pub n_test: usize,
    /// The guardrails' readings.
    pub holds: Vec<HoldReading>,
}

/// One guardrail's value on one run.
#[derive(Clone, Debug, PartialEq)]
pub struct HoldReading {
    /// The metric key.
    pub metric: String,
    /// Its mean on this run.
    pub value: f64,
    /// The floor it must stay at or above.
    pub min: f64,
}

impl HoldReading {
    /// Whether the guardrail still holds.
    pub fn satisfied(&self) -> bool {
        self.value >= self.min
    }
}

/// What the machine decided about a round.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    /// Keep the change: the next round builds on it.
    Keep,
    /// Revert the change: the next round starts from the last kept run.
    Revert(RevertReason),
}

impl Decision {
    /// The `eval_run.summary.decision` spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Decision::Keep => "keep",
            Decision::Revert(_) => "revert",
        }
    }

    /// Whether this round was flat — the plateau and stall counters both read it.
    pub fn is_flat(self) -> bool {
        matches!(
            self,
            Decision::Revert(RevertReason::HeldOutFlat | RevertReason::NoChange)
        )
    }
}

/// Why a round was reverted. The reason is recorded on the run, because "reverted" with no reason
/// is indistinguishable from a bug.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RevertReason {
    /// Train moved, the held-out split did not: the change did not transfer.
    HeldOutFlat,
    /// A single round's pattern with no predecessor: the one-off rule.
    OneOff,
    /// Nothing moved at all.
    NoChange,
    /// The trained split got worse.
    TrainDown,
    /// A cost walk broke one of its quality guardrails.
    HoldBroken,
    /// A cost walk did not move cost by the required delta.
    CostNotCleared,
}

impl RevertReason {
    /// The `eval_run.summary.reason` spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            RevertReason::HeldOutFlat => "held_out_flat",
            RevertReason::OneOff => "one_off",
            RevertReason::NoChange => "no_change",
            RevertReason::TrainDown => "train_down",
            RevertReason::HoldBroken => "hold_broken",
            RevertReason::CostNotCleared => "cost_not_cleared",
        }
    }
}

/// The **previous** round's movement, as it was decided. Its one consumer is the one-off rule:
/// `train down / held-out up` is kept only when it repeats, and re-deriving that pattern from two
/// more rows would make it depend on which base the caller happened to load.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PriorPattern {
    /// How the previous round's train mean moved against its own base.
    pub train: Movement,
    /// How the previous round's held-out mean moved against its own base.
    pub test: Movement,
    /// Whether that round was kept.
    pub kept: bool,
}

/// **The keep/revert rule.** `base` is the run the candidate was measured against — the previous
/// round, or the baseline for round 1 — and `current` is the candidate.
///
/// `prior` is the previous round's recorded pattern, which exists for one rule only: `train down /
/// held-out up` is kept **only if it repeats**, because that pattern is also exactly what an
/// overfitted change looks like when the split is small.
pub fn keep_or_revert(
    goal: &Goal,
    base: &RoundOutcome,
    current: &RoundOutcome,
    prior: Option<PriorPattern>,
    delta: f64,
) -> Decision {
    if delta <= 0.0 || !delta.is_finite() {
        return Decision::Revert(RevertReason::NoChange);
    }

    // A guardrail is checked before any gain can be claimed: a cost walk that broke quality has
    // nothing to keep, however much cheaper it got.
    if goal.is_cost() && current.holds.iter().any(|hold| !hold.satisfied()) {
        return Decision::Revert(RevertReason::HoldBroken);
    }

    // A cost walk keeps on one signal — cost fell by at least `delta` — after the guardrail check
    // above. Its quality control is an *absolute floor*, not a train/test pattern, and reading the
    // held-out quality mean through `goal.direction` (which is `lower`, because cost improves
    // downward) would invert the very number it is protecting. The table below is a quality
    // goal's, where train corroborates and test is the headline.
    if goal.is_cost() {
        let before = base.cost_cents.unwrap_or(0.0);
        let after = current.cost_cents.unwrap_or(0.0);
        return if Direction::Lower.movement(before, after, delta) == Movement::Up {
            Decision::Keep
        } else {
            Decision::Revert(RevertReason::CostNotCleared)
        };
    }

    let train_move = goal.direction.movement(base.train, current.train, delta);
    let test_move = goal.direction.movement(base.test, current.test, delta);

    let repeats = prior.is_some_and(|prior: PriorPattern| {
        prior.kept && prior.train == Movement::Down && prior.test == Movement::Up
    });

    match (train_move, test_move) {
        // Progress on both signals is the only unambiguous keep.
        (Movement::Up, Movement::Up) => Decision::Keep,
        // Train moved, held out did not: the change fitted the visible set.
        (Movement::Up, Movement::Flat) => Decision::Revert(RevertReason::HeldOutFlat),
        (Movement::Up, Movement::Down) => Decision::Revert(RevertReason::TrainDown),
        // The pattern that needs a second witness.
        (Movement::Down, Movement::Up) => {
            if repeats {
                Decision::Keep
            } else {
                Decision::Revert(RevertReason::OneOff)
            }
        }
        (Movement::Flat, Movement::Up) => {
            if repeats {
                Decision::Keep
            } else {
                Decision::Revert(RevertReason::OneOff)
            }
        }
        (Movement::Down, _) => Decision::Revert(RevertReason::TrainDown),
        (_, Movement::Down) => Decision::Revert(RevertReason::TrainDown),
        (Movement::Flat, Movement::Flat) => Decision::Revert(RevertReason::NoChange),
    }
}

/// Which stopping condition the climb declared.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stopping {
    /// `plateau_k` consecutive flat rounds.
    Plateau {
        /// How many flat rounds end it (>= 3).
        k: i32,
    },
    /// A fixed number of rounds.
    Rounds {
        /// The maximum.
        max_rounds: i32,
    },
    /// Only `eval_climbs.stop` ends it.
    Manual,
}

impl Stopping {
    /// Parse the column's vocabulary.
    pub fn parse(
        stopping: &str,
        plateau_k: i32,
        max_rounds: Option<i32>,
    ) -> Result<Self, ClimbError> {
        match stopping {
            "plateau" => {
                if plateau_k < 3 {
                    return Err(ClimbError::Vocabulary {
                        what: "plateau_k (two flat rounds is still noise)",
                        value: plateau_k.to_string(),
                    });
                }
                Ok(Stopping::Plateau { k: plateau_k })
            }
            "rounds" => {
                let max_rounds = max_rounds.ok_or_else(|| ClimbError::Vocabulary {
                    what: "max_rounds (required when stopping = rounds)",
                    value: "null".to_owned(),
                })?;
                if max_rounds < 1 {
                    return Err(ClimbError::Vocabulary {
                        what: "max_rounds",
                        value: max_rounds.to_string(),
                    });
                }
                Ok(Stopping::Rounds { max_rounds })
            }
            "manual" => Ok(Stopping::Manual),
            other => Err(ClimbError::Vocabulary {
                what: "stopping",
                value: other.to_owned(),
            }),
        }
    }
}

/// Why a climb stopped on its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopReason {
    /// `plateau_k` consecutive flat rounds.
    Plateau,
    /// The declared round count is exhausted.
    RoundsExhausted,
}

impl StopReason {
    /// The machine code an operator greps.
    pub fn as_str(self) -> &'static str {
        match self {
            StopReason::Plateau => "plateau",
            StopReason::RoundsExhausted => "rounds_exhausted",
        }
    }
}

/// Whether the climb's own stopping condition has been met.
///
/// `flat_streak` counts the settled rounds the machine called flat, most recent first; a
/// categorisation round resets it to zero, because it moved the diagnosis rather than the number.
pub fn stopping_reached(
    stopping: Stopping,
    rounds_run: i32,
    flat_streak: i32,
) -> Option<StopReason> {
    match stopping {
        Stopping::Plateau { k } => (flat_streak >= k).then_some(StopReason::Plateau),
        Stopping::Rounds { max_rounds } => {
            (rounds_run >= max_rounds).then_some(StopReason::RoundsExhausted)
        }
        Stopping::Manual => None,
    }
}

/// One row of a categorisation round: a bucket, how many failures it explains, and one line of why.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BucketCount {
    /// The bucket.
    pub bucket: Bucket,
    /// How many of the observed failures it covers.
    pub count: u32,
    /// One line naming the evidence.
    pub line: String,
}

/// The stall-triage vocabulary. Exactly these five: a categorisation round that invents a sixth
/// is a round that has not diagnosed anything.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Bucket {
    /// The answer is not in the material the flow was given.
    ArtifactGap,
    /// The grader is wrong, so the number is not the model's.
    GraderDisagreement,
    /// The harness — serving, timeouts, a mismatch — loses the case.
    Harness,
    /// The flow's shape prevents it: a missing step, a dead branch.
    Structural,
    /// The cases disagree with themselves.
    Variance,
}

impl Bucket {
    /// The wire and `change_md` spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Bucket::ArtifactGap => "artifact_gap",
            Bucket::GraderDisagreement => "grader_disagreement",
            Bucket::Harness => "harness",
            Bucket::Structural => "structural",
            Bucket::Variance => "variance",
        }
    }

    /// Parse the bucket vocabulary.
    pub fn parse(raw: &str) -> Result<Self, ClimbError> {
        match raw {
            "artifact_gap" => Ok(Bucket::ArtifactGap),
            "grader_disagreement" => Ok(Bucket::GraderDisagreement),
            "harness" => Ok(Bucket::Harness),
            "structural" => Ok(Bucket::Structural),
            "variance" => Ok(Bucket::Variance),
            other => Err(ClimbError::Vocabulary {
                what: "bucket",
                value: other.to_owned(),
            }),
        }
    }
}

/// A categorisation round's payload.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BucketReport {
    /// One entry per bucket the operator could name. At least one.
    pub buckets: Vec<BucketCount>,
}

impl BucketReport {
    /// Parse and validate a bucket report.
    pub fn parse(value: &serde_json::Value) -> Result<Self, ClimbError> {
        let entries = value
            .as_array()
            .ok_or_else(|| ClimbError::Buckets("`buckets` must be an array".to_owned()))?;
        if entries.is_empty() {
            return Err(ClimbError::Buckets(
                "a categorisation round names at least one bucket".to_owned(),
            ));
        }
        let mut buckets = Vec::with_capacity(entries.len());
        for entry in entries {
            let bucket = entry
                .get("bucket")
                .and_then(|value| value.as_str())
                .map(Bucket::parse)
                .transpose()?
                .ok_or_else(|| ClimbError::Buckets("every entry needs a `bucket`".to_owned()))?;
            let count = entry
                .get("count")
                .and_then(|value| value.as_u64())
                .ok_or_else(|| ClimbError::Buckets("every entry needs a `count`".to_owned()))?;
            let line = entry
                .get("line")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .ok_or_else(|| {
                    ClimbError::Buckets("every entry needs a one-line `line`".to_owned())
                })?;
            buckets.push(BucketCount {
                bucket,
                count: u32::try_from(count).map_err(|_| {
                    ClimbError::Buckets("a bucket's `count` is out of range".to_owned())
                })?,
                line: line.to_owned(),
            });
        }
        if buckets.iter().all(|bucket| bucket.count == 0) {
            return Err(ClimbError::Buckets(
                "a categorisation round that counts nothing has not categorised anything"
                    .to_owned(),
            ));
        }
        Ok(BucketReport { buckets })
    }

    /// The report as the markdown that replaces a diff in `change_md`.
    pub fn to_markdown(&self) -> String {
        let mut out = String::from("## Stall triage\n\n| bucket | count | why |\n|---|---|---|\n");
        for entry in &self.buckets {
            out.push_str(&format!(
                "| `{}` | {} | {} |\n",
                entry.bucket.as_str(),
                entry.count,
                entry.line.replace('|', "\\|")
            ));
        }
        out
    }
}

/// Whether the next round must categorise rather than change content.
pub fn triage_required(flat_streak: i32) -> bool {
    flat_streak >= FLAT_ROUNDS_BEFORE_TRIAGE
}

/// Whether a content round is refused outright — the fourth against the same wall.
///
/// The count is the number of consecutive flat rounds already settled: three flat rounds mean the
/// round now being proposed would be the fourth content round, and D26 refuses it.
pub fn content_round_refused(flat_streak: i32) -> bool {
    flat_streak >= FLAT_ROUNDS_BEFORE_REFUSAL
}

/// The noise floor a `delta` must sit above, from the held-out sample's own size.
///
/// Delegates to [`means_within_noise`], S19's predicate: the half-width is `1/sqrt(n·reps)`, the
/// standard error of a proportion at `p = 0.5` before any baseline exists. Reusing it is the point
/// — a second arithmetic here would let the two disagree about what "noise" means.
pub fn noise_floor(test_cases: usize, reps: u32) -> f64 {
    means_within_noise(0.0, 0.0, test_cases, reps).1
}

/// Whether a `delta` is measurable at this sample size.
///
/// `Err(floor)` carries the number to say out loud: the method's rule is that the floor is stated
/// **before round 1**, not discovered by a coin-flip result.
pub fn delta_above_floor(delta: f64, test_cases: usize, reps: u32) -> Result<(), f64> {
    let floor = noise_floor(test_cases, reps);
    if !delta.is_finite() || delta <= 0.0 || delta < floor {
        Err(floor)
    } else {
        Ok(())
    }
}

/// A run's measurement identity, for the comparability refusal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunSignature {
    /// The frozen instrument.
    pub version_id: Uuid,
    /// The instrument digest **recomputed from the version's rows as they are now**
    /// (`store::eval::version_digest_tx`), which covers the graders, their rubrics, the prices and
    /// the harness paths. A metric edited after the version was created changes it — which is
    /// exactly the case where two runs' numbers stopped being the same scale.
    pub digest: String,
    /// The version's locked split hash.
    pub split_sha256: Option<String>,
}

/// Why two runs cannot be compared.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotComparable {
    /// Different instrument versions: scores from before and after an edit are two scales.
    Version,
    /// Same version, different graders: the number changed meaning without the cases moving.
    Graders,
    /// Different splits: the held-out halves are different questions.
    Split,
}

impl NotComparable {
    /// The machine code and the sentence a caller can act on.
    pub fn as_str(self) -> &'static str {
        match self {
            NotComparable::Version => "version",
            NotComparable::Graders => "graders",
            NotComparable::Split => "split",
        }
    }
}

/// **The refusal, before any counting.** Two runs measure the same thing only when the frozen
/// instrument, the graders and the split all agree.
pub fn comparable(a: &RunSignature, b: &RunSignature) -> Result<(), NotComparable> {
    if a.version_id != b.version_id {
        return Err(NotComparable::Version);
    }
    if a.digest != b.digest {
        return Err(NotComparable::Graders);
    }
    if a.split_sha256 != b.split_sha256 {
        return Err(NotComparable::Split);
    }
    Ok(())
}

/// Whether a case counts as resolved at `threshold`.
pub fn resolved(score: f64, threshold: f64) -> bool {
    score >= threshold
}

/// A paired delta and its interval: `delta ± z · sd / sqrt(n)` over the per-case differences.
///
/// This is the climb's own arithmetic, not the test: significance is McNemar's
/// ([`crate::benchmark::mcnemar::compare`]) and the two must not be confused for each other. A
/// zero `n` has no interval at all, so the caller gets `(0.0, 0.0)` rather than a fabricated one.
pub fn paired_delta_ci(differences: &[f64], z: f64) -> (f64, f64, f64) {
    let n = differences.len();
    if n == 0 {
        return (0.0, 0.0, 0.0);
    }
    let mean = differences.iter().sum::<f64>() / n as f64;
    if n == 1 {
        return (mean, mean, mean);
    }
    let variance = differences
        .iter()
        .map(|difference| (difference - mean) * (difference - mean))
        .sum::<f64>()
        / (n as f64 - 1.0);
    let standard_error = (variance / n as f64).sqrt();
    (mean, mean - z * standard_error, mean + z * standard_error)
}
