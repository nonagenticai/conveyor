//! The delivery state machine.
//!
//! `pending → sent → delivered | failed | expired`. `delivered`, `failed` and `expired` are
//! terminal and **terminal wins**: no transition out of them exists, including one that
//! names the same state. The store layer refuses a state this function rejects; the
//! database's CHECK guards the vocabulary only (data-arch §3).
//!
//! Two closure edges are deliberately part of the table: `pending → failed` and
//! `pending → expired`. A delivery can end before it is ever handed to a channel — the
//! first-response-counts rule expires the other recipients of a closed approval (S06 §5.4),
//! and a channel that refuses the recipient fails it (S06a §5.2) — and a machine that could
//! only close a `sent` row would leave those rows live forever. §3.6's chain is the main
//! path, not a licence to leak a row; the same shape appears in §3.2, whose states line
//! omits `queued → cancelled` while the transition table carries it.
//!
//! Timestamps are the database's (`now()`), never a Rust clock: these values carry the state
//! and the version, and the store writes `sent_at`/`delivered_at` in the same statement
//! (data-arch §3, §4).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::Timestamp;
use crate::ids::Version;

// ── connector kinds (S06d) ──────────────────────────────────────────────────────────────
//
// A kind is an open, format-checked vocabulary: the database stores the name, and the
// connector registry in `conveyor-actions` decides which names this build serves. New
// connectors therefore need no migration. These constants name the kinds the core itself
// reasons about; adapters may declare others.

/// The in-app channel: a human session, authorized by its tenant role.
pub const WEB: &str = "web";
/// A tenant's Telegram bot binding.
pub const TELEGRAM: &str = "telegram";
/// The MCP surface; its identity is the licence principal.
pub const MCP: &str = "mcp";
/// The zero-code JSON gateway adapter: any external app that can POST an inbound envelope.
pub const WEBHOOK: &str = "webhook";

/// Whether `kind` is a legal connector-kind spelling (`^[a-z][a-z0-9_]{1,31}$`).
pub fn is_valid_kind(kind: &str) -> bool {
    let bytes = kind.as_bytes();
    (2..=32).contains(&bytes.len())
        && bytes[0].is_ascii_lowercase()
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
}

/// The five delivery states.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryState {
    /// Queued, never handed to a channel.
    Pending,
    /// Handed to the channel; at least one send attempt was made.
    Sent,
    /// The channel confirmed delivery; terminal.
    Delivered,
    /// The attempt budget is spent, or the channel refused it permanently; terminal.
    Failed,
    /// The deadline passed, or the delivery set closed before it was confirmed; terminal.
    Expired,
}

impl DeliveryState {
    /// Every state.
    pub const ALL: [DeliveryState; 5] = [
        DeliveryState::Pending,
        DeliveryState::Sent,
        DeliveryState::Delivered,
        DeliveryState::Failed,
        DeliveryState::Expired,
    ];

    /// The terminal states. A delivery that holds one accepts no transition.
    pub const TERMINAL: [DeliveryState; 3] = [
        DeliveryState::Delivered,
        DeliveryState::Failed,
        DeliveryState::Expired,
    ];

    /// The stored `delivery.state`.
    pub fn as_str(self) -> &'static str {
        match self {
            DeliveryState::Pending => "pending",
            DeliveryState::Sent => "sent",
            DeliveryState::Delivered => "delivered",
            DeliveryState::Failed => "failed",
            DeliveryState::Expired => "expired",
        }
    }

    /// True for `delivered | failed | expired`.
    pub fn is_terminal(self) -> bool {
        Self::TERMINAL.contains(&self)
    }

    /// The documented transition table (data-arch §3.6).
    pub fn may_transition_to(self, to: DeliveryState) -> bool {
        matches!(
            (self, to),
            (DeliveryState::Pending, DeliveryState::Sent)
                | (DeliveryState::Pending, DeliveryState::Failed)
                | (DeliveryState::Pending, DeliveryState::Expired)
                | (DeliveryState::Sent, DeliveryState::Delivered)
                | (DeliveryState::Sent, DeliveryState::Failed)
                | (DeliveryState::Sent, DeliveryState::Expired)
        )
    }
}

impl fmt::Display for DeliveryState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Why a string is not a delivery state.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("unknown delivery state {0:?}")]
pub struct DeliveryStateError(pub String);

impl FromStr for DeliveryState {
    type Err = DeliveryStateError;
    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        DeliveryState::ALL
            .into_iter()
            .find(|state| state.as_str() == raw)
            .ok_or_else(|| DeliveryStateError(raw.to_owned()))
    }
}

/// One row per (message, channel, recipient): the unit the drainer works on.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Delivery {
    /// The row id; the `delivery.send` job's dedupe key and its idempotency key.
    pub id: uuid::Uuid,
    /// The message this delivery fans out from.
    pub message_id: uuid::Uuid,
    /// `web`, `telegram` or `mcp` (the vocabulary is the database's CHECK).
    pub channel_kind: String,
    /// The channel-native recipient: a user ref, a chat id, an agent address.
    pub recipient_ref: String,
    /// The lifecycle state.
    pub state: DeliveryState,
    /// Send attempts so far. The budget belongs to the caller (`max_attempts`).
    pub attempts: i32,
    /// The last failure text, when an attempt failed.
    pub last_error: Option<String>,
    /// When the first send attempt was made; set by the store.
    pub sent_at: Option<Timestamp>,
    /// When the channel confirmed delivery; set by the store.
    pub delivered_at: Option<Timestamp>,
    /// When the delivery stops being worth attempting; set by the store.
    pub expires_at: Option<Timestamp>,
    /// The optimistic-concurrency version.
    pub version: Version,
}

/// Why a transition is refused.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DeliveryError {
    /// A terminal delivery accepts no transition at all.
    #[error("delivery is terminal ({from}); a terminal delivery cannot transition")]
    Terminal {
        /// The terminal state the delivery holds.
        from: DeliveryState,
    },
    /// The pair is not in the documented transition table.
    #[error("a delivery cannot transition from {from} to {to}")]
    Illegal {
        /// The state being left.
        from: DeliveryState,
        /// The state being entered.
        to: DeliveryState,
    },
}

impl Delivery {
    /// Apply a transition, or refuse it.
    ///
    /// The returned value has the new state and the next version; timestamps are the
    /// store's (they come from the database clock, never from a Rust clock).
    fn transition(mut self, to: DeliveryState) -> Result<Self, DeliveryError> {
        if self.state.is_terminal() {
            return Err(DeliveryError::Terminal { from: self.state });
        }
        if !self.state.may_transition_to(to) {
            return Err(DeliveryError::Illegal {
                from: self.state,
                to,
            });
        }
        self.state = to;
        self.version = self.version.next();
        Ok(self)
    }

    /// Mark the delivery confirmed by the channel (`sent → delivered`).
    pub fn deliver(self) -> Result<Self, DeliveryError> {
        self.transition(DeliveryState::Delivered)
    }

    /// Mark the delivery permanently failed (`pending | sent → failed`).
    pub fn fail(self) -> Result<Self, DeliveryError> {
        self.transition(DeliveryState::Failed)
    }

    /// Mark the delivery expired (`pending | sent → expired`).
    pub fn expire(self) -> Result<Self, DeliveryError> {
        self.transition(DeliveryState::Expired)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn delivery(state: DeliveryState) -> Delivery {
        Delivery {
            id: uuid::Uuid::now_v7(),
            message_id: uuid::Uuid::now_v7(),
            channel_kind: "web".to_owned(),
            recipient_ref: "user:1".to_owned(),
            state,
            attempts: 0,
            last_error: None,
            sent_at: None,
            delivered_at: None,
            expires_at: None,
            version: Version::INITIAL,
        }
    }

    /// The documented table, spelled out independently of `may_transition_to`, so the test
    /// fails if the function and the documentation drift apart.
    fn documented(from: DeliveryState, to: DeliveryState) -> bool {
        matches!(
            (from, to),
            (DeliveryState::Pending, DeliveryState::Sent)
                | (DeliveryState::Pending, DeliveryState::Failed)
                | (DeliveryState::Pending, DeliveryState::Expired)
                | (DeliveryState::Sent, DeliveryState::Delivered)
                | (DeliveryState::Sent, DeliveryState::Failed)
                | (DeliveryState::Sent, DeliveryState::Expired)
        )
    }

    fn outcome(from: DeliveryState, to: DeliveryState) -> Result<Delivery, DeliveryError> {
        let value = delivery(from);
        match to {
            DeliveryState::Delivered => value.deliver(),
            DeliveryState::Failed => value.fail(),
            DeliveryState::Expired => value.expire(),
            DeliveryState::Sent | DeliveryState::Pending => {
                // No public mutator targets a non-terminal state; only `mark_sent` does, and
                // it goes through `may_transition_to`. Exercise the same private path.
                value.transition(to)
            }
        }
    }

    #[test]
    fn the_documented_transitions_are_the_only_ones() {
        for from in DeliveryState::ALL {
            for to in DeliveryState::ALL {
                let result = outcome(from, to);
                if from.is_terminal() {
                    assert_eq!(
                        result,
                        Err(DeliveryError::Terminal { from }),
                        "{from} -> {to}"
                    );
                } else if documented(from, to) {
                    let next = result.expect("allowed transition");
                    assert_eq!(next.state, to);
                    assert_eq!(next.version, Version::new(2).expect("version"));
                } else {
                    assert_eq!(
                        result,
                        Err(DeliveryError::Illegal { from, to }),
                        "{from} -> {to}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_terminal_delivery_rejects_every_transition_including_its_own_state() {
        for terminal in DeliveryState::TERMINAL {
            for to in DeliveryState::ALL {
                assert_eq!(
                    outcome(terminal, to),
                    Err(DeliveryError::Terminal { from: terminal }),
                    "{terminal} -> {to} must be refused"
                );
            }
        }
    }

    #[test]
    fn a_message_and_recipient_pair_is_the_delivery_identity() {
        // The state machine is about one (message, channel, recipient) row: a delivery that
        // is delivered once cannot be delivered again by a replay.
        let d = delivery(DeliveryState::Sent).deliver().expect("delivered");
        assert_eq!(
            d.deliver(),
            Err(DeliveryError::Terminal {
                from: DeliveryState::Delivered
            })
        );
    }

    #[test]
    fn the_vocabulary_round_trips_and_refuses_anything_else() {
        for state in DeliveryState::ALL {
            assert_eq!(state.to_string(), state.as_str());
            assert_eq!(DeliveryState::from_str(state.as_str()), Ok(state));
        }
        assert!(DeliveryState::from_str("delivered ").is_err());
        assert!(DeliveryState::from_str("bounced").is_err());
        assert!(DeliveryState::from_str("").is_err());
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        /// Terminal arm: every terminal state rejects every target.
        #[test]
        fn terminal_deliveries_never_transition(
            terminal in prop::sample::select(DeliveryState::TERMINAL.to_vec()),
            to in prop::sample::select(DeliveryState::ALL.to_vec()),
        ) {
            prop_assert_eq!(
                outcome(terminal, to),
                Err(DeliveryError::Terminal { from: terminal })
            );
        }

        /// And the table is exactly the documented one for every non-terminal pair.
        #[test]
        fn non_terminal_transitions_follow_the_table(
            from in prop::sample::select(
                vec![DeliveryState::Pending, DeliveryState::Sent]
            ),
            to in prop::sample::select(DeliveryState::ALL.to_vec()),
        ) {
            let result = outcome(from, to);
            if documented(from, to) {
                prop_assert_eq!(result.expect("allowed").state, to);
            } else {
                prop_assert_eq!(result, Err(DeliveryError::Illegal { from, to }));
            }
        }
    }
}
