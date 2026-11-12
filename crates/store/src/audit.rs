//! The append-only audit log: [`record`] writes into the CALLER's transaction.
//!
//! Data-architecture §2.2/§6 and CONTRACT §6: every mutation writes its `audit_event` in the
//! same transaction, so an audited write and its record commit or roll back together.
//! Append-only is enforced by GRANT (`REVOKE UPDATE, DELETE … FROM conveyor_runtime`), not
//! by convention: the runtime role physically cannot rewrite a row.
//!
//! `details` is scrubbed and capped at write (8 KiB, object only). The scrub is deliberately
//! a refusal, not a redactor: a value under a secret-looking key, or an email-shaped value,
//! is a bug in the caller and is rejected. Silently rewriting it would let a caller believe
//! a field was recorded when it was not.

use conveyor_domain::TenantId;
use serde_json::{Map, Value};
use sqlx::PgConnection;
use uuid::Uuid;

/// The serde cap from the CHECK constraint.
pub const MAX_DETAILS_BYTES: usize = 8 * 1024;

/// The action grammar `^[a-z0-9_]+\.[a-z0-9_]+$` (CONTRACT §6/§7).
pub fn valid_action(action: &str) -> bool {
    let Some((resource, verb)) = action.split_once('.') else {
        return false;
    };
    let word = |s: &str| {
        !s.is_empty()
            && s.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    };
    !verb.contains('.') && word(resource) && word(verb)
}

/// The status vocabulary from the CHECK constraint.
pub const STATUSES: [&str; 3] = ["ok", "refused", "error"];

/// The actor kinds this build writes: a person, an agent, or the service itself.
pub const ACTOR_KINDS: [&str; 3] = ["user", "agent", "service"];

/// One audit fact.
#[derive(Clone, Debug)]
pub struct AuditEntry {
    /// `app_user.id`, or `None` for the service itself.
    pub actor_id: Option<Uuid>,
    /// `user | agent | service`.
    pub actor_kind: String,
    /// `<resource>.<verb>`.
    pub action: String,
    /// `ok | refused | error`.
    pub status: String,
    /// The table the action touched, if any.
    pub resource_type: Option<String>,
    /// The row the action touched, if any.
    pub resource_id: Option<Uuid>,
    /// The request id the middleware accepted or minted (CONTRACT §5).
    pub request_id: Option<String>,
    /// Scrubbed, capped detail object.
    pub details: Value,
}

impl AuditEntry {
    /// A minimal `ok` entry for the service actor.
    pub fn service(action: impl Into<String>, status: impl Into<String>) -> Self {
        Self {
            actor_id: None,
            actor_kind: "service".to_owned(),
            action: action.into(),
            status: status.into(),
            resource_type: None,
            resource_id: None,
            request_id: None,
            details: Value::Object(Map::new()),
        }
    }
}

/// Why an audit row was refused.
#[derive(Debug, thiserror::Error)]
pub enum AuditError {
    /// The insert failed (the caller's transaction should roll back).
    #[error("database: {0}")]
    Database(#[from] sqlx::Error),
    /// The action is not `<resource>.<verb>`.
    #[error("audit action {0:?} must be exactly <resource>.<verb> in lower_snake_case")]
    InvalidAction(String),
    /// The status is outside the CHECK vocabulary.
    #[error("audit status {0:?} must be one of {STATUSES:?}")]
    InvalidStatus(String),
    /// The actor kind is outside this build's vocabulary.
    #[error("audit actor_kind {0:?} must be one of {ACTOR_KINDS:?}")]
    InvalidActorKind(String),
    /// The details value is not a JSON object.
    #[error("audit details must be a JSON object")]
    DetailsNotObject,
    /// The serialized details exceed the CHECK cap.
    #[error("audit details are {size} bytes; the cap is {MAX_DETAILS_BYTES}")]
    DetailsTooLarge {
        /// Measured serialized size.
        size: usize,
    },
    /// A key or value that must never reach the audit log.
    #[error("audit details carry a forbidden {what}: {key:?}")]
    ForbiddenDetail {
        /// `key` or `value`.
        what: &'static str,
        /// The offending key or value.
        key: String,
    },
}

/// Keys whose presence means the caller is about to persist a credential.
const FORBIDDEN_KEYS: [&str; 11] = [
    "password",
    "passwd",
    "token",
    "secret",
    "authorization",
    "api_key",
    "apikey",
    "access_token",
    "refresh_token",
    "private_key",
    "credential",
];

fn key_is_forbidden(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    FORBIDDEN_KEYS
        .iter()
        .any(|bad| lower == *bad || lower.ends_with(&format!("_{bad}")))
}

fn value_looks_forbidden(value: &str) -> bool {
    // An email address is personal data the log does not need; the caller records the
    // domain instead (the agent service's rule).
    value.contains('@') && value.contains('.') && !value.contains(' ')
}

fn scrub(value: &Value) -> Result<(), AuditError> {
    match value {
        Value::Object(map) => {
            for (key, nested) in map {
                if key_is_forbidden(key) {
                    return Err(AuditError::ForbiddenDetail {
                        what: "key",
                        key: key.clone(),
                    });
                }
                scrub(nested)?;
            }
            Ok(())
        }
        Value::Array(items) => {
            for item in items {
                scrub(item)?;
            }
            Ok(())
        }
        Value::String(text) => {
            if value_looks_forbidden(text) {
                return Err(AuditError::ForbiddenDetail {
                    what: "value",
                    key: text.clone(),
                });
            }
            Ok(())
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => Ok(()),
    }
}

/// Write one audit row into the caller's transaction.
///
/// The row's `tenant_id` is bound explicitly and the policy's `WITH CHECK` refuses a value
/// that differs from the transaction's `app.current_tenant`, so a mismatched tenant is a
/// database error rather than a cross-tenant write.
pub async fn record(
    conn: &mut PgConnection,
    tenant: TenantId,
    entry: &AuditEntry,
) -> Result<Uuid, AuditError> {
    if !valid_action(&entry.action) {
        return Err(AuditError::InvalidAction(entry.action.clone()));
    }
    if !STATUSES.contains(&entry.status.as_str()) {
        return Err(AuditError::InvalidStatus(entry.status.clone()));
    }
    if !ACTOR_KINDS.contains(&entry.actor_kind.as_str()) {
        return Err(AuditError::InvalidActorKind(entry.actor_kind.clone()));
    }
    if !entry.details.is_object() {
        return Err(AuditError::DetailsNotObject);
    }
    scrub(&entry.details)?;
    let serialized = serde_json::to_vec(&entry.details)
        .map_err(|e| AuditError::Database(sqlx::Error::Encode(Box::new(e))))?;
    if serialized.len() > MAX_DETAILS_BYTES {
        return Err(AuditError::DetailsTooLarge {
            size: serialized.len(),
        });
    }

    let (id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO audit_event
             (tenant_id, actor_id, actor_kind, action, status, resource_type, resource_id,
              request_id, details)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(entry.actor_id)
    .bind(&entry.actor_kind)
    .bind(&entry.action)
    .bind(&entry.status)
    .bind(&entry.resource_type)
    .bind(entry.resource_id)
    .bind(&entry.request_id)
    .bind(&entry.details)
    .fetch_one(conn)
    .await?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn action_grammar_is_one_dot() {
        assert!(valid_action("cards.move"));
        assert!(valid_action("licence_sync.succeeded"));
        for bad in [
            "cards",
            "Cards.move",
            "cards.Move",
            "cards.moved.now",
            "",
            ".move",
            "cards.",
        ] {
            assert!(!valid_action(bad), "{bad:?}");
        }
    }

    #[test]
    fn scrub_refuses_secret_keys_nested_and_email_values() {
        assert!(scrub(&json!({"card_id": "x", "nested": {"ok": 1}})).is_ok());
        assert!(matches!(
            scrub(&json!({"password": "x"})).unwrap_err(),
            AuditError::ForbiddenDetail { what: "key", .. }
        ));
        assert!(matches!(
            scrub(&json!({"nested": [{"access_token": "x"}]})).unwrap_err(),
            AuditError::ForbiddenDetail { what: "key", .. }
        ));
        assert!(matches!(
            scrub(&json!({"who": "alice@example.com"})).unwrap_err(),
            AuditError::ForbiddenDetail { what: "value", .. }
        ));
    }
}
