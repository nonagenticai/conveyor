#![forbid(unsafe_code)]
//! The generic webhook adapter (S06d) — the zero-Rust on-ramp.
//!
//! Any gateway that can POST a small JSON envelope (`{text}` or `{action, input}`, with an
//! optional `id` and `sender`) and accept a JSON `{text}` POST becomes a connector by adding a
//! binding in the web UI. Email, WhatsApp or another provider then needs an adapter only when
//! its API cannot be fronted by such a gateway.

use std::collections::BTreeMap;
use std::time::Duration;

use http::HeaderMap;
use schemars::Schema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{Capabilities, ConnectorSpec, Identity, Inbound, SendFuture, SendOutcome, SendRequest};

/// How long one outbound POST may take, including the response body.
pub const SEND_TIMEOUT: Duration = Duration::from_secs(10);

/// The longest platform error echoed into `delivery.last_error`.
const ERROR_CHARS: usize = 200;

/// One webhook binding's config. Secret fields hold **names**, never values.
#[derive(Clone, Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// The URL the conveyor POSTs outbound messages to.
    pub endpoint_url: String,
    /// The header the inbound caller presents the secret in (default `authorization`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_header: Option<String>,
    /// The name of the sealed secret inbound callers must present.
    pub incoming_secret: String,
    /// The name of the sealed secret sent on outbound calls (optional).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outbound_secret: Option<String>,
    /// The header the outbound secret is sent in (default `authorization`, as `Bearer <value>`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outbound_auth_header: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct Envelope {
    #[serde(default)]
    id: Option<Value>,
    #[serde(default)]
    sender: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    action: Option<String>,
    #[serde(default)]
    input: Option<Value>,
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
    let header = config.auth_header.as_deref().unwrap_or("authorization");
    let Some(provided) = headers.get(header).and_then(|value| value.to_str().ok()) else {
        return false;
    };
    // A gateway may send a bearer credential or the bare secret; both are the same proof.
    let provided = provided
        .strip_prefix("Bearer ")
        .or_else(|| provided.strip_prefix("bearer "))
        .unwrap_or(provided);
    constant_time_eq(provided.as_bytes(), expected.as_bytes())
}

fn parse(body: &[u8], _config: &Value) -> Option<Inbound> {
    let envelope = serde_json::from_slice::<Envelope>(body).ok()?;
    let idempotency_key = envelope.id.map(|id| match id {
        Value::String(raw) => format!("webhook:{raw}"),
        other => format!("webhook:{other}"),
    });
    if let Some(action) = envelope.action {
        let input = envelope.input.unwrap_or_else(|| json!({}));
        if !input.is_object() {
            return None;
        }
        return Some(Inbound {
            sender_ref: envelope.sender,
            text: None,
            structured: Some(json!({ "action": action, "input": input })),
            idempotency_key,
            request_id: None,
        });
    }
    Some(Inbound {
        sender_ref: envelope.sender,
        text: envelope.text,
        structured: None,
        idempotency_key,
        request_id: None,
    })
    .filter(|inbound| {
        inbound
            .text
            .as_deref()
            .is_some_and(|text| !text.trim().is_empty())
    })
}

fn send(request: SendRequest) -> SendFuture {
    Box::pin(async move {
        let Ok(config) = parse_config(&request.config) else {
            return SendOutcome::failed(None, "the webhook binding config is invalid");
        };
        let client = match reqwest::Client::builder().timeout(SEND_TIMEOUT).build() {
            Ok(client) => client,
            Err(error) => {
                return SendOutcome::failed(
                    None,
                    format!(
                        "the HTTP client could not be built: {}",
                        error.without_url()
                    ),
                );
            }
        };
        let mut builder = client
            .post(config.endpoint_url.as_str())
            .json(&json!({ "text": request.text }));
        if let Some(secret_name) = &config.outbound_secret {
            let Some(secret) = request.secrets.get(secret_name) else {
                return SendOutcome::failed(None, "the outbound secret is missing");
            };
            let header = config
                .outbound_auth_header
                .as_deref()
                .unwrap_or("authorization");
            let value = format!("Bearer {secret}");
            match reqwest::header::HeaderName::from_bytes(header.as_bytes())
                .ok()
                .zip(reqwest::header::HeaderValue::from_str(&value).ok())
            {
                Some((name, value)) => {
                    builder = builder.header(name, value);
                }
                None => {
                    return SendOutcome::failed(None, "the outbound auth header is not valid");
                }
            }
        }
        let response = match tokio::time::timeout(SEND_TIMEOUT, builder.send()).await {
            Err(_) => {
                return SendOutcome::failed(
                    None,
                    format!("the webhook did not answer within {SEND_TIMEOUT:?}"),
                );
            }
            Ok(Err(error)) => {
                return SendOutcome::failed(
                    None,
                    format!("the webhook request failed: {}", error.without_url()),
                );
            }
            Ok(Ok(response)) => response,
        };
        let status = response.status();
        if status.is_success() {
            return SendOutcome::ok(status.as_u16());
        }
        let detail = response
            .text()
            .await
            .unwrap_or_default()
            .chars()
            .take(ERROR_CHARS)
            .collect::<String>();
        let detail = if detail.is_empty() {
            format!("the webhook answered {status}")
        } else {
            format!("the webhook answered {status}: {detail}")
        };
        SendOutcome::failed(Some(status.as_u16()), detail)
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

/// The one connector declaration.
pub const SPEC: ConnectorSpec = ConnectorSpec {
    kind: conveyor_domain::channel::WEBHOOK,
    label: "Webhook",
    capabilities: Capabilities {
        can_send: true,
        can_receive: true,
        identity: Identity::Provisioned,
    },
    config_schema: schema,
    secret_fields: &["incoming_secret", "outbound_secret"],
    default_actions: &crate::channel::CONNECTOR_DEFAULT_ACTIONS,
    instructions: "Point the gateway's outbound webhook at this binding's inbound URL with the \
                   inbound secret, and give the conveyor the URL it should POST replies to. The \
                   envelope is `{id?, sender?, text?}` or `{id?, sender?, action, input}`.",
    validate,
    authenticate,
    parse,
    send,
};

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Value {
        json!({
            "endpoint_url": "http://127.0.0.1:9/hook",
            "incoming_secret": "in",
        })
    }

    #[test]
    fn a_bearer_or_bare_secret_authenticates() {
        let secrets = BTreeMap::from([("in".to_owned(), "s3cret".to_owned())]);
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer s3cret".parse().expect("header"));
        assert!(authenticate(&headers, &config(), &secrets));
        headers.insert("authorization", "s3cret".parse().expect("header"));
        assert!(authenticate(&headers, &config(), &secrets));
        headers.insert("authorization", "wrong".parse().expect("header"));
        assert!(!authenticate(&headers, &config(), &secrets));
    }

    #[test]
    fn text_and_structured_envelopes_are_both_accepted() {
        let inbound = parse(
            br#"{"id":"evt-1","sender":"mailto:a@b","text":"run 01900000-0000-7000-8000-000000000000"}"#,
            &config(),
        )
        .expect("text envelope");
        assert_eq!(inbound.idempotency_key.as_deref(), Some("webhook:evt-1"));
        assert!(inbound.text.is_some());

        let inbound = parse(
            br#"{"id":2,"action":"approvals.decide","input":{"approval_id":"x","decision":"approve"}}"#,
            &config(),
        )
        .expect("structured envelope");
        assert!(inbound.structured.is_some());
        assert_eq!(inbound.idempotency_key.as_deref(), Some("webhook:2"));

        assert!(parse(br#"{"text":"   "}"#, &config()).is_none());
        assert!(parse(br#"{"action":"x","input":[]}"#, &config()).is_none());
    }

    #[test]
    fn an_unknown_config_field_is_refused() {
        assert!(
            validate(&json!({"endpoint_url":"http://x","incoming_secret":"in","nope":1})).is_err()
        );
    }
}
