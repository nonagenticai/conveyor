#![forbid(unsafe_code)]
//! The Telegram adapter (S06d) — deliberately thin.
//!
//! It authenticates the Bot API secret-token header, reads the `Update` shape and posts
//! `sendMessage`. Everything else — the service identity, the grant check, the command
//! grammar, multicast, retries, the delivery state — is core. The bot token and the inbound
//! secret are **tenant secrets named in the binding config**, so a tenant configures its own
//! bot through the web UI; there is no deployment-wide token or webhook secret.

use std::collections::BTreeMap;

use http::HeaderMap;
use schemars::Schema;
use serde::Deserialize;
use serde_json::Value;

use super::{Capabilities, ConnectorSpec, Identity, Inbound, SendFuture, SendOutcome, SendRequest};
use crate::channel::CONNECTOR_DEFAULT_ACTIONS;

/// The header Telegram sends when the webhook was registered with a secret token.
pub const SECRET_TOKEN_HEADER: &str = "x-telegram-bot-api-secret-token";

/// The Bot API base when the binding does not override it (tests point it at a stub).
pub const DEFAULT_API_BASE: &str = "https://api.telegram.org";

/// One Telegram binding's config. Secret fields hold **names**, never values.
#[derive(Clone, Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// The chat this binding answers, as Telegram reports it (a string to allow the `-100…` ids).
    pub chat_id: String,
    /// The forum topic, when the chat is a forum.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic_id: Option<i64>,
    /// The name of the sealed secret holding the bot token.
    pub bot_token_secret: String,
    /// The name of the sealed secret Telegram echoes in the secret-token header.
    pub incoming_secret: String,
    /// The Bot API base; defaults to [`DEFAULT_API_BASE`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_base: Option<String>,
}

/// The subset of the Bot API `Update` this adapter reads.
#[derive(Clone, Debug, Default, Deserialize)]
struct Update {
    #[serde(default)]
    update_id: Option<i64>,
    #[serde(default)]
    message: Option<Message>,
}

#[derive(Clone, Debug, Deserialize)]
struct Message {
    chat: Chat,
    #[serde(default)]
    message_thread_id: Option<i64>,
    #[serde(default)]
    from: Option<From>,
    #[serde(default)]
    text: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct Chat {
    id: i64,
}

#[derive(Clone, Debug, Deserialize)]
struct From {
    #[serde(default)]
    id: Option<i64>,
}

fn parse_config(config: &Value) -> Result<Config, String> {
    serde_json::from_value(config.clone()).map_err(|error| error.to_string())
}

fn validate(config: &Value) -> Result<(), String> {
    parse_config(config).map(|_| ())
}

fn schema() -> Schema {
    schemars::schema_for!(Config)
}

fn authenticate(headers: &HeaderMap, config: &Value, secrets: &BTreeMap<String, String>) -> bool {
    let Ok(config) = parse_config(config) else {
        return false;
    };
    let Some(expected) = secrets.get(&config.incoming_secret) else {
        return false;
    };
    let Some(provided) = headers
        .get(SECRET_TOKEN_HEADER)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    constant_time_eq(provided.as_bytes(), expected.as_bytes())
}

fn parse(body: &[u8], config: &Value) -> Option<Inbound> {
    let config = parse_config(config).ok()?;
    let update = serde_json::from_slice::<Update>(body).ok()?;
    let message = update.message?;
    // The binding names the chat (and forum topic) it answers. Anything else is not this
    // binding's message: consumed, no action.
    if message.chat.id.to_string() != config.chat_id {
        return None;
    }
    if let Some(topic) = config.topic_id
        && message.message_thread_id != Some(topic)
    {
        return None;
    }
    let sender_ref = message
        .from
        .and_then(|from| from.id)
        .map(|id| format!("tg:{id}"));
    let text = message.text?;
    Some(Inbound {
        sender_ref,
        text: Some(text),
        structured: None,
        idempotency_key: update.update_id.map(|id| format!("telegram:{id}")),
        request_id: update.update_id.map(|id| format!("telegram-update-{id}")),
    })
}

fn send(request: SendRequest) -> SendFuture {
    Box::pin(async move {
        let Ok(config) = parse_config(&request.config) else {
            return SendOutcome::failed(None, "the telegram binding config is invalid");
        };
        let Some(token) = request.secrets.get(&config.bot_token_secret) else {
            return SendOutcome::failed(None, "the telegram bot token secret is missing");
        };
        let api_base = config.api_base.as_deref().unwrap_or(DEFAULT_API_BASE);
        let outcome = crate::control::channels::send_telegram_message(
            api_base,
            token,
            &config.chat_id,
            config.topic_id,
            &request.text,
        )
        .await;
        if outcome.ok {
            SendOutcome::ok(outcome.status_code.unwrap_or(200))
        } else {
            SendOutcome::failed(
                outcome.status_code,
                outcome
                    .error
                    .unwrap_or_else(|| "the telegram send failed".to_owned()),
            )
        }
    })
}

/// Compare two byte strings without an early exit on the first differing byte.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut difference = 0u8;
    for (left, right) in a.iter().zip(b.iter()) {
        difference |= left ^ right;
    }
    difference == 0
}

/// A JSON scalar as the string Telegram's numbers and a binding's config both reduce to.
pub fn scalar_string(value: &Value) -> Option<String> {
    match value {
        Value::String(raw) => Some(raw.clone()),
        Value::Number(raw) => Some(raw.to_string()),
        _ => None,
    }
}

/// The one connector declaration.
pub const SPEC: ConnectorSpec = ConnectorSpec {
    kind: conveyor_domain::channel::TELEGRAM,
    label: "Telegram",
    capabilities: Capabilities {
        can_send: true,
        can_receive: true,
        identity: Identity::Provisioned,
    },
    config_schema: schema,
    secret_fields: &["bot_token_secret", "incoming_secret"],
    default_actions: &CONNECTOR_DEFAULT_ACTIONS,
    instructions: "Create a bot with BotFather, store its token as a secret, and register this \
                   binding's inbound URL with `setWebhook` using the same secret-token value. \
                   The bot answers the one chat/topic named here.",
    validate,
    authenticate,
    parse,
    send,
};

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn config() -> Value {
        json!({
            "chat_id": "42",
            "bot_token_secret": "telegram-token",
            "incoming_secret": "telegram-incoming",
        })
    }

    #[test]
    fn the_secret_token_header_is_compared_constant_time() {
        let secrets = BTreeMap::from([("telegram-incoming".to_owned(), "s3cret".to_owned())]);
        let mut headers = HeaderMap::new();
        headers.insert(SECRET_TOKEN_HEADER, "s3cret".parse().expect("header"));
        assert!(authenticate(&headers, &config(), &secrets));
        headers.insert(SECRET_TOKEN_HEADER, "s3cres".parse().expect("header"));
        assert!(!authenticate(&headers, &config(), &secrets));
        assert!(!constant_time_eq(b"", b"s3cret"));
    }

    #[test]
    fn only_the_bound_chat_and_topic_are_this_binding() {
        let body = json!({
            "update_id": 7,
            "message": {"chat": {"id": 42}, "text": "run 01900000-0000-7000-8000-000000000000"}
        });
        let inbound = parse(&serde_json::to_vec(&body).expect("json"), &config()).expect("parsed");
        assert_eq!(inbound.idempotency_key.as_deref(), Some("telegram:7"));
        assert!(inbound.text.is_some());

        let foreign = json!({"message": {"chat": {"id": 43}, "text": "approve x"}});
        assert!(parse(&serde_json::to_vec(&foreign).expect("json"), &config()).is_none());

        let mut forum = config();
        forum["topic_id"] = json!(9);
        let wrong_topic =
            json!({"message": {"chat": {"id": 42}, "message_thread_id": 8, "text": "help"}});
        assert!(parse(&serde_json::to_vec(&wrong_topic).expect("json"), &forum).is_none());
        let right_topic =
            json!({"message": {"chat": {"id": 42}, "message_thread_id": 9, "text": "help"}});
        assert!(parse(&serde_json::to_vec(&right_topic).expect("json"), &forum).is_some());
    }

    #[test]
    fn the_sender_is_audit_evidence_and_an_unknown_config_is_refused() {
        let body =
            json!({"message": {"chat": {"id": 42}, "from": {"id": 99}, "text": "approve 1"}});
        let inbound = parse(&serde_json::to_vec(&body).expect("json"), &config()).expect("parsed");
        assert_eq!(inbound.sender_ref.as_deref(), Some("tg:99"));
        assert!(validate(&json!({"chat_id": "42", "unexpected": true})).is_err());
    }
}
