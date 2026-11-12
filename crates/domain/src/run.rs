//! The run state machine.
//!
//! `queued → running → waiting_approval → running → succeeded | failed | cancelled`.
//! `succeeded`, `failed` and `cancelled` are terminal and **terminal wins**: no transition
//! out of them exists, including one that names the same status. The store layer refuses a
//! status this function rejects; the database's CHECK guards the vocabulary only (data-arch
//! §3).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::Timestamp;
use crate::ids::Version;

/// The six run statuses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    /// Accepted, not started.
    Queued,
    /// Executing.
    Running,
    /// Blocked on an approval gate.
    WaitingApproval,
    /// Finished successfully; terminal.
    Succeeded,
    /// Finished with an error; terminal.
    Failed,
    /// Withdrawn before finishing; terminal.
    Cancelled,
}

impl RunStatus {
    /// Every status.
    pub const ALL: [RunStatus; 6] = [
        RunStatus::Queued,
        RunStatus::Running,
        RunStatus::WaitingApproval,
        RunStatus::Succeeded,
        RunStatus::Failed,
        RunStatus::Cancelled,
    ];

    /// The terminal statuses. A run that holds one accepts no transition.
    pub const TERMINAL: [RunStatus; 3] = [
        RunStatus::Succeeded,
        RunStatus::Failed,
        RunStatus::Cancelled,
    ];

    /// The stored `run.status`.
    pub fn as_str(self) -> &'static str {
        match self {
            RunStatus::Queued => "queued",
            RunStatus::Running => "running",
            RunStatus::WaitingApproval => "waiting_approval",
            RunStatus::Succeeded => "succeeded",
            RunStatus::Failed => "failed",
            RunStatus::Cancelled => "cancelled",
        }
    }

    /// True for `succeeded | failed | cancelled`.
    pub fn is_terminal(self) -> bool {
        Self::TERMINAL.contains(&self)
    }

    /// The documented transition table (data-arch §3.2).
    pub fn may_transition_to(self, to: RunStatus) -> bool {
        matches!(
            (self, to),
            (RunStatus::Queued, RunStatus::Running)
                | (RunStatus::Queued, RunStatus::Cancelled)
                | (RunStatus::Running, RunStatus::WaitingApproval)
                | (RunStatus::Running, RunStatus::Succeeded)
                | (RunStatus::Running, RunStatus::Failed)
                | (RunStatus::Running, RunStatus::Cancelled)
                | (RunStatus::WaitingApproval, RunStatus::Running)
                | (RunStatus::WaitingApproval, RunStatus::Cancelled)
        )
    }
}

impl fmt::Display for RunStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Why a string is not a run status.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("unknown run status {0:?}")]
pub struct RunStatusError(pub String);

impl FromStr for RunStatus {
    type Err = RunStatusError;
    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        RunStatus::ALL
            .into_iter()
            .find(|status| status.as_str() == raw)
            .ok_or_else(|| RunStatusError(raw.to_owned()))
    }
}

/// One execution of a card through a gate, process or workflow.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Run {
    /// The row id.
    pub id: uuid::Uuid,
    /// The card this run executes, when it is card-scoped.
    pub card_id: Option<uuid::Uuid>,
    /// The owning project.
    pub project_id: uuid::Uuid,
    /// The lifecycle status.
    pub status: RunStatus,
    /// The attempt number, starting at 1.
    pub attempt: i32,
    /// A stable machine-readable failure class.
    pub error_class: Option<String>,
    /// A human-readable failure message.
    pub error_message: Option<String>,
    /// When execution started; set by the store when the status becomes `running`.
    pub started_at: Option<Timestamp>,
    /// When execution finished; set by the store when the status becomes terminal.
    pub finished_at: Option<Timestamp>,
    /// The optimistic-concurrency version.
    pub version: Version,
}

/// Why a transition is refused.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RunError {
    /// A terminal run accepts no transition at all.
    #[error("run is terminal ({from}); a terminal run cannot transition")]
    Terminal {
        /// The terminal status the run holds.
        from: RunStatus,
    },
    /// The pair is not in the documented transition table.
    #[error("a run cannot transition from {from} to {to}")]
    Illegal {
        /// The status being left.
        from: RunStatus,
        /// The status being entered.
        to: RunStatus,
    },
}

impl Run {
    /// Apply a transition, or refuse it.
    ///
    /// The returned value has the new status and the next version; timestamps are the store's
    /// (they come from the database clock, never from a Rust clock).
    pub fn transition(mut self, to: RunStatus) -> Result<Self, RunError> {
        if self.status.is_terminal() {
            return Err(RunError::Terminal { from: self.status });
        }
        if !self.status.may_transition_to(to) {
            return Err(RunError::Illegal {
                from: self.status,
                to,
            });
        }
        self.status = to;
        self.version = self.version.next();
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn run(status: RunStatus) -> Run {
        Run {
            id: uuid::Uuid::now_v7(),
            card_id: None,
            project_id: uuid::Uuid::now_v7(),
            status,
            attempt: 1,
            error_class: None,
            error_message: None,
            started_at: None,
            finished_at: None,
            version: Version::INITIAL,
        }
    }

    #[test]
    fn the_documented_transitions_are_the_only_ones() {
        for from in RunStatus::ALL {
            for to in RunStatus::ALL {
                let result = run(from).transition(to);
                if from.is_terminal() {
                    assert_eq!(result, Err(RunError::Terminal { from }), "{from} -> {to}");
                } else if from.may_transition_to(to) {
                    let next = result.expect("allowed transition");
                    assert_eq!(next.status, to);
                    assert_eq!(next.version, Version::new(2).expect("version"));
                } else {
                    assert_eq!(
                        result,
                        Err(RunError::Illegal { from, to }),
                        "{from} -> {to}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_terminal_run_rejects_every_transition_including_its_own_status() {
        for terminal in RunStatus::TERMINAL {
            for to in RunStatus::ALL {
                assert_eq!(
                    run(terminal).transition(to),
                    Err(RunError::Terminal { from: terminal }),
                    "{terminal} -> {to} must be refused"
                );
            }
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        /// Gate property (d), terminal arm: every terminal status rejects every target.
        #[test]
        fn terminal_runs_never_transition(
            terminal in prop::sample::select(RunStatus::TERMINAL.to_vec()),
            to in prop::sample::select(RunStatus::ALL.to_vec()),
        ) {
            prop_assert_eq!(
                run(terminal).transition(to),
                Err(RunError::Terminal { from: terminal })
            );
        }

        /// And the table is exactly the documented one for every non-terminal pair.
        #[test]
        fn non_terminal_transitions_follow_the_table(
            from in prop::sample::select(
                vec![RunStatus::Queued, RunStatus::Running, RunStatus::WaitingApproval]
            ),
            to in prop::sample::select(RunStatus::ALL.to_vec()),
        ) {
            let outcome = run(from).transition(to);
            if from.may_transition_to(to) {
                prop_assert_eq!(outcome.expect("allowed").status, to);
            } else {
                prop_assert_eq!(outcome, Err(RunError::Illegal { from, to }));
            }
        }
    }
}
