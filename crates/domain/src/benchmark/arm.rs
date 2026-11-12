//! Arm identity for a benchmark run.
//!
//! Ported verbatim from `conveyor-bench/scripts/report-arms.py:84-89`. The arm is read from the
//! predictions label (`model_name_or_path`), because that is the only per-row place the oracle
//! and the prompt are recorded.
//!
//! ```text
//! oracle   = files+test  iff the label ends with "+failing-test", else files
//! prompt   = bug_fix     iff the label ends with "+failing-test", else finding_only
//! pipeline = conveyor     iff the label starts with "conveyor-", else direct
//! direct arms are always bug_fix (the one-shot arm uses SYS_DEFAULT)
//! ```
//!
//! ⛔ **Comparable means oracle AND prompt agree.** The pipeline is what is being compared, so it
//! must never be part of the refusal — a rule that also required the pipeline to match would
//! refuse every conveyor-vs-baseline comparison, which is the only comparison the instrument
//! exists to make. This is the `report-arms.py:160` rule, not the looser wording CONTRACT §19
//! used before 2026-11-09.

use std::collections::BTreeSet;

use thiserror::Error;

/// The producer under test.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pipeline {
    /// This application.
    Conveyor,
    /// The one-shot model control (the `baseline` arm).
    Direct,
}

/// What the producer is allowed to see of the answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Oracle {
    /// The files the gold patch touches.
    Files,
    /// Those files **and** the first `FAIL_TO_PASS` test id.
    FilesAndTest,
}

/// Which system prompt the producer was given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prompt {
    BugFix,
    FindingOnly,
}

/// A run's measurement identity, derived from its label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Arm {
    pub pipeline: Pipeline,
    pub oracle: Oracle,
    pub prompt: Prompt,
}

/// Why an arm could not be established.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ArmError {
    /// One predictions file carried more than one label. The oracle and the prompt are read
    /// from the label, so such a file cannot describe one arm (`report-arms.py:76-82`).
    #[error(
        "{count} arm labels in one predictions file ({labels}); one file cannot describe one arm"
    )]
    MixedLabels { count: usize, labels: String },
}

impl Arm {
    /// Derive the arm from a predictions label. Unknown labels still have an arm: the same
    /// defaulting the report generator applies (`files`, `bug_fix`, `direct`).
    pub fn derive(label: &str) -> Self {
        let files_and_test = label.ends_with("+failing-test");
        let pipeline = if label.starts_with("conveyor-") {
            Pipeline::Conveyor
        } else {
            Pipeline::Direct
        };
        Self {
            pipeline,
            oracle: if files_and_test {
                Oracle::FilesAndTest
            } else {
                Oracle::Files
            },
            prompt: if pipeline == Pipeline::Direct || files_and_test {
                Prompt::BugFix
            } else {
                Prompt::FindingOnly
            },
        }
    }

    /// Two runs are comparable only when their oracle and prompt agree. The pipeline may
    /// differ — that is the comparison.
    pub fn comparable(&self, other: &Self) -> bool {
        self.oracle == other.oracle && self.prompt == other.prompt
    }
}

/// Resolve the single label of a predictions file.
///
/// An empty input yields `""` (the report generator's `next(iter(labels), "")`); more than one
/// distinct label is [`ArmError::MixedLabels`].
pub fn single_label<'a>(labels: impl IntoIterator<Item = &'a str>) -> Result<&'a str, ArmError> {
    let set: BTreeSet<&str> = labels.into_iter().collect();
    match set.len() {
        0 => Ok(""),
        1 => Ok(set.into_iter().next().expect("len one")),
        count => Err(ArmError::MixedLabels {
            count,
            labels: set.into_iter().collect::<Vec<_>>().join(", "),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_five_banked_labels_derive_their_identity() {
        // The labels actually present in the 2026-09-17 artefacts (predictions.jsonl).
        let cases = [
            (
                "conveyor-oracle-files",
                Arm {
                    pipeline: Pipeline::Conveyor,
                    oracle: Oracle::Files,
                    prompt: Prompt::FindingOnly,
                },
            ),
            (
                "conveyor-oracle-files+failing-test",
                Arm {
                    pipeline: Pipeline::Conveyor,
                    oracle: Oracle::FilesAndTest,
                    prompt: Prompt::BugFix,
                },
            ),
            (
                "conveyor-as-is",
                Arm {
                    pipeline: Pipeline::Conveyor,
                    oracle: Oracle::Files,
                    prompt: Prompt::FindingOnly,
                },
            ),
            (
                "qwen-oneshot-oracle-files",
                Arm {
                    pipeline: Pipeline::Direct,
                    oracle: Oracle::Files,
                    prompt: Prompt::BugFix,
                },
            ),
            (
                "qwen-oneshot-oracle-files+failing-test",
                Arm {
                    pipeline: Pipeline::Direct,
                    oracle: Oracle::FilesAndTest,
                    prompt: Prompt::BugFix,
                },
            ),
        ];
        for (label, want) in cases {
            assert_eq!(Arm::derive(label), want, "label {label}");
        }
    }

    #[test]
    fn a_direct_arm_without_the_failing_test_is_still_a_bug_fix() {
        // report-arms.py:87-89 — the one-shot arm always uses SYS_DEFAULT.
        let arm = Arm::derive("qwen-oneshot-oracle-files");
        assert_eq!(arm.prompt, Prompt::BugFix);
    }

    #[test]
    fn comparability_is_oracle_and_prompt_not_pipeline() {
        let conveyor_ft = Arm::derive("conveyor-oracle-files+failing-test");
        let qwen_ft = Arm::derive("qwen-oneshot-oracle-files+failing-test");
        assert!(conveyor_ft.comparable(&qwen_ft), "the banked valid pair");
        assert!(qwen_ft.comparable(&conveyor_ft));

        let qwen_plain = Arm::derive("qwen-oneshot-oracle-files");
        assert!(
            !conveyor_ft.comparable(&qwen_plain),
            "oracle differs (files+test vs files)"
        );

        let conveyor_plain = Arm::derive("conveyor-oracle-files");
        assert!(
            !conveyor_plain.comparable(&qwen_ft),
            "oracle differs (files vs files+test)"
        );

        // Same pipeline, different prompt: still not comparable.
        let conveyor_as_is = Arm::derive("conveyor-as-is");
        assert!(conveyor_plain.comparable(&conveyor_as_is));
        assert!(!conveyor_as_is.comparable(&qwen_plain));
    }

    #[test]
    fn one_label_may_be_repeated_but_two_labels_are_refused() {
        assert_eq!(single_label(["a", "a"]).expect("one distinct"), "a");
        assert_eq!(single_label(std::iter::empty()).expect("empty"), "");
        let err = single_label(["conveyor-oracle-files", "qwen-oneshot-oracle-files"])
            .expect_err("two labels");
        assert_eq!(
            err,
            ArmError::MixedLabels {
                count: 2,
                labels: "conveyor-oracle-files, qwen-oneshot-oracle-files".to_owned()
            }
        );
    }
}
