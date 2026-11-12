#![forbid(unsafe_code)]
//! The shared command grammar (S06d).
//!
//! **This is the standard, and it lives once.** An adapter extracts text (or a structured
//! `{action, input}` for JSON gateways) and hands it here; it never parses a command and never
//! authorizes. The same grammar therefore works for Telegram today and for an email or WhatsApp
//! bridge tomorrow, and a new connector inherits the full operator lifecycle for free.
//!
//! A command that is not understood is a no-op (the caller acknowledges it), never an error:
//! webhook senders retry non-2xx, so a typo must not become a retry loop.
//!
//! Authorization happens after parsing, in `invoke`: the target action is checked against the
//! tenant's grant set and the machine floor exactly like an HTTP call.

use conveyor_auth::ChannelAuthority;
use serde_json::{Value, json};
use uuid::Uuid;

/// One parsed inbound text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Parsed {
    /// Invoke this action with this input.
    Command { action: String, input: Value },
    /// The caller asked for the command list; the caller answers from the authority.
    Help,
    /// Nothing to do (an unknown command, a malformed id, or an empty message).
    NoOp,
}

/// Parse the shared command grammar.
pub fn parse(text: &str) -> Parsed {
    let text = text.trim();
    let text = text.strip_prefix('/').unwrap_or(text).trim_start();
    if text.is_empty() {
        return Parsed::NoOp;
    }
    let (verb, rest) = match text.split_once(char::is_whitespace) {
        Some((verb, rest)) => (verb.to_ascii_lowercase(), rest.trim()),
        None => (text.to_ascii_lowercase(), ""),
    };
    if verb == "help" {
        return Parsed::Help;
    }
    match verb.as_str() {
        "approve" | "reject" => {
            let Some((approval_id, note)) = split_id(rest) else {
                return Parsed::NoOp;
            };
            let mut input = json!({
                "approval_id": approval_id,
                "decision": if verb == "approve" { "approve" } else { "reject" },
            });
            if !note.is_empty() {
                input["note"] = Value::String(note.to_owned());
            }
            Parsed::Command {
                action: "approvals.decide".to_owned(),
                input,
            }
        }
        // A question is an approval: answering it is approving it with the answer as the note.
        "answer" => {
            let Some((approval_id, note)) = split_id(rest) else {
                return Parsed::NoOp;
            };
            let mut input = json!({ "approval_id": approval_id, "decision": "approve" });
            if !note.is_empty() {
                input["note"] = Value::String(note.to_owned());
            }
            Parsed::Command {
                action: "approvals.decide".to_owned(),
                input,
            }
        }
        // `run` and `rerun` are the same action: a run is created for a card and the executor
        // starts it. A rerun after the previous attempt is terminal is admitted by the same
        // one-live-run-per-card rule; while one is live the action returns its `409`.
        "run" | "rerun" => {
            let Some((card_id, _)) = split_id(rest) else {
                return Parsed::NoOp;
            };
            Parsed::Command {
                action: "runs.create".to_owned(),
                input: json!({ "card_id": card_id }),
            }
        }
        "start" => {
            let Some((run_id, _)) = split_id(rest) else {
                return Parsed::NoOp;
            };
            Parsed::Command {
                action: "runs.start".to_owned(),
                input: json!({ "id": run_id }),
            }
        }
        "finish" => {
            let Some((run_id, tail)) = split_id(rest) else {
                return Parsed::NoOp;
            };
            let (outcome, message) = match tail.split_once(char::is_whitespace) {
                Some((outcome, message)) => (outcome, message.trim()),
                None => (tail, ""),
            };
            if !matches!(outcome, "succeeded" | "failed") {
                return Parsed::NoOp;
            }
            let mut input = json!({ "id": run_id, "outcome": outcome });
            if !message.is_empty() {
                input["error_message"] = Value::String(message.to_owned());
            }
            Parsed::Command {
                action: "runs.finish".to_owned(),
                input,
            }
        }
        "cancel" => {
            let Some((run_id, reason)) = split_id(rest) else {
                return Parsed::NoOp;
            };
            let mut input = json!({ "id": run_id });
            if !reason.is_empty() {
                input["reason"] = Value::String(reason.to_owned());
            }
            Parsed::Command {
                action: "runs.cancel".to_owned(),
                input,
            }
        }
        "close" => {
            let Some((project_id, reason)) = split_id(rest) else {
                return Parsed::NoOp;
            };
            let mut input = json!({ "id": project_id });
            if !reason.is_empty() {
                input["reason"] = Value::String(reason.to_owned());
            }
            Parsed::Command {
                action: "projects.close".to_owned(),
                input,
            }
        }
        // The title is the rest of the message; a project type is a tenant catalogue choice the
        // web form supplies, so the chat default is "the tenant's default project type".
        "create" if !rest.is_empty() => Parsed::Command {
            action: "projects.create".to_owned(),
            input: json!({ "title": rest }),
        },
        // The escape hatch: any granted action with an explicit object input. It is still
        // authorized by the same grant set, so it widens no one's reach.
        "do" => {
            let Some((action, json_text)) = rest.split_once(char::is_whitespace) else {
                return Parsed::NoOp;
            };
            let Ok(input) = serde_json::from_str::<Value>(json_text.trim()) else {
                return Parsed::NoOp;
            };
            if !input.is_object() {
                return Parsed::NoOp;
            }
            Parsed::Command {
                action: action.to_owned(),
                input,
            }
        }
        _ => Parsed::NoOp,
    }
}

/// The id + note shape shared by approve/reject/answer/run/start/cancel/close.
fn split_id(rest: &str) -> Option<(Uuid, &str)> {
    let (raw, tail) = match rest.split_once(char::is_whitespace) {
        Some((raw, tail)) => (raw, tail.trim()),
        None => (rest, ""),
    };
    Some((Uuid::parse_str(raw).ok()?, tail))
}

/// The command list a channel may actually use, one line per command.
///
/// `help` must not advertise an action the tenant revoked, so every line is filtered through
/// the same authority the invocation would use.
pub fn help_text(authority: &ChannelAuthority) -> String {
    const COMMANDS: [(&str, &str); 11] = [
        ("approve <id> [note]", "approvals.decide"),
        ("reject <id> [reason]", "approvals.decide"),
        ("answer <id> [text]", "approvals.decide"),
        ("run <card_id>", "runs.create"),
        ("rerun <card_id>", "runs.create"),
        ("start <run_id>", "runs.start"),
        ("finish <run_id> succeeded|failed [msg]", "runs.finish"),
        ("cancel <run_id> [reason]", "runs.cancel"),
        ("close <project_id> [reason]", "projects.close"),
        ("create <title>", "projects.create"),
        ("do <action> <json>", ""),
    ];
    let mut lines = Vec::new();
    for (usage, action) in COMMANDS {
        // `do` is the escape hatch: list it only when at least one non-default action is granted.
        let allowed = if action.is_empty() {
            authority.overrides().values().any(|allowed| *allowed)
        } else {
            super::allows(authority, action)
        };
        if allowed {
            lines.push(usage.to_owned());
        }
    }
    if lines.is_empty() {
        "This channel has no commands enabled. A tenant-admin can grant them in Connectors."
            .to_owned()
    } else {
        format!("Commands:\n{}", lines.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(text: &str) -> (String, Value) {
        match parse(text) {
            Parsed::Command { action, input } => (action, input),
            other => panic!("{text:?} parsed as {other:?}"),
        }
    }

    #[test]
    fn approve_and_reject_keep_the_legacy_text_shape() {
        let id = Uuid::now_v7();
        let (action, input) = command(&format!("approve {id} looks right"));
        assert_eq!(action, "approvals.decide");
        assert_eq!(input["approval_id"], json!(id));
        assert_eq!(input["decision"], "approve");
        assert_eq!(input["note"], "looks right");
        let (_, input) = command(&format!("/reject {id}"));
        assert_eq!(input["decision"], "reject");
        assert!(input.get("note").is_none());
    }

    #[test]
    fn the_lifecycle_verbs_map_to_the_registered_actions() {
        let id = Uuid::now_v7();
        assert_eq!(command(&format!("run {id}")).0, "runs.create");
        assert_eq!(command(&format!("rerun {id}")).0, "runs.create");
        assert_eq!(command(&format!("start {id}")).0, "runs.start");
        assert_eq!(command(&format!("cancel {id} why")).0, "runs.cancel");
        assert_eq!(command(&format!("close {id} done")).0, "projects.close");
        let (action, input) = command(&format!("finish {id} failed boom"));
        assert_eq!(action, "runs.finish");
        assert_eq!(input["outcome"], "failed");
        assert_eq!(input["error_message"], "boom");
        let (action, input) = command("create A new project");
        assert_eq!(action, "projects.create");
        assert_eq!(input["title"], "A new project");
    }

    #[test]
    fn a_structured_escape_hatch_requires_an_object() {
        let (action, input) = command(r#"do cards.create {"board_id":"b"}"#);
        assert_eq!(action, "cards.create");
        assert_eq!(input["board_id"], "b");
        assert_eq!(parse("do cards.create [1]"), Parsed::NoOp);
        assert_eq!(parse("do cards.create not-json"), Parsed::NoOp);
    }

    #[test]
    fn anything_else_is_a_no_op_and_help_is_a_verb() {
        assert_eq!(parse("hello there"), Parsed::NoOp);
        assert_eq!(parse("approve not-a-uuid"), Parsed::NoOp);
        assert_eq!(parse("approve"), Parsed::NoOp);
        assert_eq!(parse(""), Parsed::NoOp);
        assert_eq!(parse("/help"), Parsed::Help);
    }

    #[test]
    fn help_never_advertises_a_revoked_command() {
        use conveyor_auth::ChannelAuthority;
        use std::collections::BTreeMap;
        let none = ChannelAuthority::new(
            "telegram",
            None,
            [
                ("runs.create".to_owned(), false),
                ("approvals.decide".to_owned(), false),
            ]
            .into_iter()
            .collect::<BTreeMap<_, _>>(),
        );
        let help = help_text(&none);
        assert!(!help.contains("run <card_id>"));
        assert!(!help.contains("approve <id>"));
        assert!(help.contains("close <project_id>"));
    }
}
