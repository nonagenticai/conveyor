//! `produce.script` — the storyline and its shot list, and the duration budget it must satisfy.
//!
//! The content service answered narrative and script requests over HTTP (its 23+2 operations are
//! classified in [`crate::capture`]). ADR 0007 makes that capability one action rather than a
//! service, and the action calls a model through the inference router. Everything in *this*
//! module is deliberately free of the network, so the parts that must be reproducible — the shot
//! identity and the budget check — are testable without a model, and two runs of the same action
//! cannot disagree about which shot is which.
//!
//! ## Why the ids are derived, not generated
//!
//! `P3.1` requires that the same brief yields the same shot ids. A model cannot promise that, so
//! the ids are a function of the brief and the shot's position: `sha256(brief ‖ U+001F ‖ index)`
//! truncated to twelve hex digits. The model supplies words; the identity of a shot belongs to the
//! request, not to the completion.
//!
//! ## The budget
//!
//! [`validate`] compares the sum of the shots against the caller's target. The comparison is
//! deliberately *naming* rather than *repairing*: an over-budget script reports the ids of the
//! shots that cross the line, in order, so a caller (or a model retry) knows exactly what to cut.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// How far under or over the target a script may be before it is refused.
///
/// Ten percent either way: a shot list is an approximation of a duration, not a schedule. Both
/// directions are errors — a script far *under* the budget wastes the production slot just as an
/// over-run does, but only an over-run has shots to name.
pub const TOLERANCE: f64 = 0.10;

/// The shortest shot worth keeping. Below this a shot is a transition, not a shot.
pub const MIN_SHOT_MS: u32 = 1_000;

/// One shot in the list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shot {
    /// Derived from the brief and the position — see the module docs. Stable across runs.
    pub id: String,
    /// Position in the list, zero-based, in the order the shots are to be produced.
    pub index: u32,
    /// What the shot shows. Empty in [`skeleton`]; the model fills it.
    pub description: String,
    pub duration_ms: u32,
}

/// A storyline and the shots it decomposes into.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Script {
    /// The through-line the shots serve. Empty in a skeleton.
    pub storyline: String,
    pub shots: Vec<Shot>,
    /// The budget the caller asked for, echoed so a consumer of the returned script needs no
    /// second source for it.
    pub target_duration_ms: u32,
}

/// Why a script was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BudgetError {
    /// The shots do not fit the target. `offending` names, in order, every shot at or after the
    /// point the running total first crosses the upper bound.
    #[error(
        "the shot list runs {actual_ms}ms against a {target_ms}ms budget (±{tolerance_pct}%); \
         shots to cut: {offending:?}"
    )]
    Over {
        target_ms: u32,
        actual_ms: u32,
        tolerance_pct: u32,
        offending: Vec<String>,
    },
    /// The shots fall short of the target.
    #[error(
        "the shot list runs {actual_ms}ms against a {target_ms}ms budget; it is {short_by_ms}ms short"
    )]
    Under {
        target_ms: u32,
        actual_ms: u32,
        short_by_ms: u32,
    },
    /// A script with no shots at all cannot be produced.
    #[error("the script has no shots")]
    Empty,
}

/// The stable identity of a shot.
///
/// `sha256(brief ‖ U+001F ‖ index)`, first twelve hex digits. The separator is the ASCII unit
/// separator, which cannot appear in a brief typed by a person but is cheap: it makes
/// `("ab", 1)` and `("a", 11)` different inputs, which concatenation alone would not.
pub fn shot_id(brief: &str, index: u32) -> String {
    let mut hasher = Sha256::new();
    hasher.update(brief.as_bytes());
    hasher.update([0x1f]);
    hasher.update(index.to_be_bytes());
    let digest = hasher.finalize();
    let mut out = String::with_capacity(12);
    for byte in &digest[..6] {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// How many shots a target duration is split into.
///
/// One shot per [`MIN_SHOT_MS`] of the target, floor, at least one. This is the whole rule: a
/// ten-second brief is ten shots, a minute is sixty. It is a rule rather than a model choice so
/// that the skeleton is reproducible; a caller that wants fewer, longer shots supplies its own.
pub fn shot_count(target_duration_ms: u32) -> u32 {
    (target_duration_ms / MIN_SHOT_MS).max(1)
}

/// The empty shot list for a brief and a target: the right number of shots, with the right ids and
/// an even share of the budget, and no words in them.
///
/// The action renders these as JSON, asks the model for a storyline and one description per shot
/// id, and binds the answers back by id — never by position, so a model that reorders or drops a
/// line cannot silently move a description onto the wrong shot.
pub fn skeleton(brief: &str, target_duration_ms: u32) -> Vec<Shot> {
    let count = shot_count(target_duration_ms);
    let base = target_duration_ms / count;
    let remainder = target_duration_ms % count;
    (0..count)
        .map(|index| Shot {
            id: shot_id(brief, index),
            index,
            description: String::new(),
            // The remainder goes to the first shots so the sum is exactly the target.
            duration_ms: base + u32::from(index < remainder),
        })
        .collect()
}

/// Compare a script's shots against its budget.
///
/// The bounds are `target × (1 ∓ TOLERANCE)`. On an over-run every shot at or after the point the
/// running total first exceeds the upper bound is `offending`: cutting those is sufficient, and
/// naming them is the whole point of the error.
pub fn validate(script: &Script) -> Result<(), BudgetError> {
    if script.shots.is_empty() {
        return Err(BudgetError::Empty);
    }
    let target_ms = script.target_duration_ms;
    let actual_ms: u32 = script.shots.iter().map(|shot| shot.duration_ms).sum();
    let upper = (f64::from(target_ms) * (1.0 + TOLERANCE)).round() as u32;
    let lower = (f64::from(target_ms) * (1.0 - TOLERANCE)).round() as u32;

    if actual_ms > upper {
        // Name the shot whose cumulative duration crosses the line, and every shot after it:
        // cutting that suffix is sufficient, and it is the smallest such suffix.
        let mut running = 0u32;
        let mut crossed = false;
        let mut offending = Vec::new();
        for shot in &script.shots {
            if crossed {
                offending.push(shot.id.clone());
                continue;
            }
            running += shot.duration_ms;
            if running > upper {
                crossed = true;
                offending.push(shot.id.clone());
            }
        }
        return Err(BudgetError::Over {
            target_ms,
            actual_ms,
            tolerance_pct: (TOLERANCE * 100.0) as u32,
            offending,
        });
    }

    if actual_ms < lower {
        return Err(BudgetError::Under {
            target_ms,
            actual_ms,
            short_by_ms: lower - actual_ms,
        });
    }

    Ok(())
}

/// Bind model-supplied text to a skeleton.
///
/// `storyline` and `descriptions` are the model's output; a description is matched to a shot by
/// id. A shot the model did not answer for keeps its empty description rather than borrowing its
/// neighbour's — a wrong description is worse than an absent one, and the caller can see the gap.
pub fn assemble(
    brief: &str,
    target_duration_ms: u32,
    storyline: &str,
    descriptions: &[(String, String)],
) -> Script {
    let by_id: std::collections::BTreeMap<&str, &str> = descriptions
        .iter()
        .map(|(id, text)| (id.as_str(), text.as_str()))
        .collect();
    let shots = skeleton(brief, target_duration_ms)
        .into_iter()
        .map(|mut shot| {
            if let Some(text) = by_id.get(shot.id.as_str()) {
                shot.description = (*text).to_string();
            }
            shot
        })
        .collect();
    Script {
        storyline: storyline.to_string(),
        shots,
        target_duration_ms,
    }
}
