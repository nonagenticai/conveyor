//! The cost bus: one event per provider attempt.
//!
//! The reference emits a Redis stream entry for every attempt a request makes, including the ones
//! that failed over. The reader (`billing-service`) settles a reservation by the handle carried in
//! the event, so an attempt that never emits is an attempt that never settles.
//!
//! This module owns the trait and the event shape; the Redis writer and the field-for-field wire
//! mapping are the ledger slice's, and the shape here was taken from the oracle's own
//! `billing:costs` entry (`contract/golden/cost_*.txt`), not from the Python source.

use crate::provider::BoxFuture;

/// The fixed decimal the reference sends (`0.00000000`), never a float.
pub const ZERO_COST_USD: &str = "0.00000000";

/// One provider attempt, as the cost bus sees it.
///
/// Optional fields are optional because the reference omits them rather than sending nulls: an
/// absent `correlation_id` is "the caller sent none", which is a different statement from an empty
/// one and must stay distinguishable on the wire.
#[derive(Debug, Clone, Default)]
pub struct CostEvent {
    pub tenant_id: Option<String>,
    pub license_id: Option<String>,
    pub subject_id: Option<String>,
    pub correlation_id: Option<String>,
    pub stage: Option<String>,
    pub worker_name: Option<String>,
    /// Always `inference-router`: the emitter, not the caller.
    pub service_key: String,
    pub cost_source: String,
    pub provider: String,
    /// The model the caller ASKED for (the policy hop's model when a policy routed the request).
    pub model: String,
    /// The model that actually served it.
    pub served_model: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_creation_input_tokens: i64,
    pub cache_read_input_tokens: i64,
    /// The reference's fixed-width decimal string (`0.00000000`), not a float.
    pub cost_usd: String,
    pub latency_ms: i64,
    pub schema_id: Option<String>,
    pub fallback_used: bool,
    pub provider_request_id: Option<String>,
    pub usage_extra: Option<String>,
    /// Set only when billing minted a handle for this request.
    pub reservation_id: Option<String>,
    pub project_id: Option<String>,
    pub card_id: Option<String>,
    pub execution_id: Option<String>,
    pub job_id: Option<String>,
    pub app_id: Option<String>,
    pub actor_id: Option<String>,
    pub spend_scope: Option<String>,
    pub delegated_by: Option<String>,
}

/// The write side of the cost bus.
///
/// `emit` is fire-and-forget by contract: the reference never fails a request because metering
/// failed, and neither may this port. Implementations log and count; they do not return errors.
///
/// The returned `bool` is delivery evidence, not control flow: `true` iff the event reached the
/// stream. Callers book per-attempt token totals only on `true`, exactly as the reference does
/// after its XADD returns (`metering.py:1011-1035`), so a dropped event cannot inflate the token
/// families. An implementation never fails a request either way.
pub trait CostSink: Send + Sync {
    fn emit<'a>(&'a self, event: &'a CostEvent) -> BoxFuture<'a, bool>;
}

/// Metering is off: no URL configured, or a test that does not assert the bus.
///
/// Nothing is delivered, so `emit` answers `false` — the honest delivery verdict, which keeps the
/// token families from moving on an event that went nowhere.
pub struct NoopCostSink;

impl CostSink for NoopCostSink {
    fn emit<'a>(&'a self, _event: &'a CostEvent) -> BoxFuture<'a, bool> {
        Box::pin(async { false })
    }
}

fn text(value: &Option<String>) -> String {
    value.clone().unwrap_or_default()
}

impl CostEvent {
    /// The stream entry, in the reference's field order. Order is not part of the consumer's
    /// contract (it reads by field name) but it IS what the captured golden records, so keeping it
    /// costs nothing and makes the comparison a byte diff.
    ///
    /// The six identity fields and `stage`/`worker_name` are ALWAYS present — empty when absent.
    /// Only the attribution fields at the end are omitted entirely.
    pub fn field_pairs(&self) -> Vec<(&'static str, String)> {
        let mut fields = vec![
            ("tenant_id", text(&self.tenant_id)),
            ("license_id", text(&self.license_id)),
            ("subject_id", text(&self.subject_id)),
            ("correlation_id", text(&self.correlation_id)),
            ("stage", text(&self.stage)),
            ("worker_name", text(&self.worker_name)),
            ("service_key", self.service_key.clone()),
            ("cost_source", self.cost_source.clone()),
            ("provider", self.provider.clone()),
            ("model", self.model.clone()),
            ("served_model", text(&self.served_model)),
            ("input_tokens", self.input_tokens.to_string()),
            ("output_tokens", self.output_tokens.to_string()),
            (
                "cache_creation_input_tokens",
                self.cache_creation_input_tokens.to_string(),
            ),
            (
                "cache_read_input_tokens",
                self.cache_read_input_tokens.to_string(),
            ),
            ("cost_usd", self.cost_usd.clone()),
            ("schema_id", text(&self.schema_id)),
            (
                "fallback_used",
                if self.fallback_used { "true" } else { "false" }.to_owned(),
            ),
            ("provider_request_id", text(&self.provider_request_id)),
            ("usage_extra", text(&self.usage_extra)),
            ("latency_ms", self.latency_ms.to_string()),
            (
                "timestamp",
                chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
            ),
            ("event_key", uuid::Uuid::now_v7().to_string()),
        ];
        for (name, value) in [
            ("reservation_id", &self.reservation_id),
            ("project_id", &self.project_id),
            ("card_id", &self.card_id),
            ("execution_id", &self.execution_id),
            ("job_id", &self.job_id),
            ("app_id", &self.app_id),
            ("actor_id", &self.actor_id),
            ("spend_scope", &self.spend_scope),
            ("delegated_by", &self.delegated_by),
        ] {
            if let Some(value) = value {
                fields.push((name, value.clone()));
            }
        }
        fields
    }
}

/// The Redis writer: `XADD <stream> MAXLEN ~100000 * <fields…>`.
///
/// Fire-and-forget by contract — the reference never fails a request because metering failed, and
/// neither does this. A write error is logged and dropped.
pub struct RedisCostSink {
    connection: redis::aio::MultiplexedConnection,
    stream: String,
}

impl RedisCostSink {
    pub async fn connect(url: &str, stream: String) -> Result<Self, redis::RedisError> {
        let client = redis::Client::open(url)?;
        let connection = client.get_multiplexed_async_connection().await?;
        Ok(Self { connection, stream })
    }
}

impl CostSink for RedisCostSink {
    fn emit<'a>(&'a self, event: &'a CostEvent) -> BoxFuture<'a, bool> {
        Box::pin(async move {
            // Raw command, not `xadd_maxlen`: the argument order is the reference's and does not
            // move when the client crate changes its helper signature.
            let mut command = redis::cmd("XADD");
            command
                .arg(&self.stream)
                .arg("MAXLEN")
                .arg("~")
                .arg(100_000_i64)
                .arg("*");
            for (field, value) in event.field_pairs() {
                command.arg(field).arg(value);
            }
            let mut connection = self.connection.clone();
            let result: Result<String, redis::RedisError> =
                command.query_async(&mut connection).await;
            match result {
                Ok(_) => true,
                Err(error) => {
                    tracing::warn!(%error, stream = %self.stream, "cost event was not written");
                    false
                }
            }
        })
    }
}

/// The reference's `licence_is_absent` (`metering.py:332`): no licence, a blank one, or the
/// literal `unknown` (case-insensitively) all mean "nobody paid for this attempt".
///
/// The literal is a real value the licence server emits when it could not resolve one, so it must
/// be folded into the unlicensed bucket rather than treated as an identifier.
pub fn licence_is_absent(license_id: &Option<String>) -> bool {
    match license_id {
        None => true,
        Some(value) => {
            let value = value.trim();
            value.is_empty() || value.eq_ignore_ascii_case("unknown")
        }
    }
}

/// The reference's `CREDENTIAL_TENANT_ID` (`metering.py:135`): the value a credential-shaped
/// tenant is replaced with at admission. It is never a tenant, so an event carrying it must be
/// refused and counted rather than billed.
const CREDENTIAL_TENANT_ID: &str = "__credential_redacted__";

/// The literal renderings that are not identifiers (`metering.py:239-255`), matched
/// case-insensitively after trimming. A closed set, deliberately not a length rule: the shortest
/// real legacy tenant label is only six characters today.
const NOT_AN_IDENTIFIER_LITERALS: [&str; 11] = [
    "t",
    "f",
    "true",
    "false",
    "0",
    "1",
    "null",
    "none",
    "nil",
    "undefined",
    "nan",
];

/// The reference's `_MAX_TENANT_ID_LEN` (`metering.py:90`). **Not** a UUID check: billing accepts
/// a UUID OR a registered legacy tenant NAME (longest 33 characters), so a UUID-only rule would
/// refuse billable events.
const MAX_TENANT_ID_LEN: usize = 64;

/// The reference's refusal at `metering.py:574`: no tenant, a blank one, or the nil-UUID sentinel
/// in any spelling means the event cannot be attributed, so billing would dead-letter it.
pub fn tenant_is_unattributed(tenant: &Option<String>) -> bool {
    match tenant {
        None => true,
        Some(value) => value.trim().is_empty() || crate::auth::is_unattributable(value),
    }
}

/// Why `value` cannot be a tenant identifier (`metering.py:269-298`), or `None` when it can be.
///
/// Returns a member of the reference's CLOSED set and never caller text — the caller puts the
/// result on a Prometheus label, so caller-controlled cardinality must not reach it. The order is
/// the reference's own: a credential is reported as `credential_shaped`, never merely as
/// `contains_whitespace`, because the credential class is the security event.
pub fn tenant_id_rejection_reason(value: &str) -> Option<&'static str> {
    if value == CREDENTIAL_TENANT_ID || crate::auth::credential_scheme(value).is_some() {
        return Some("credential_shaped");
    }
    let literal = value.trim().to_ascii_lowercase();
    if NOT_AN_IDENTIFIER_LITERALS.contains(&literal.as_str()) {
        return Some("not_an_identifier_literal");
    }
    if value.chars().count() > MAX_TENANT_ID_LEN {
        return Some("too_long");
    }
    if value.chars().any(char::is_whitespace) {
        return Some("contains_whitespace");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{licence_is_absent, tenant_id_rejection_reason, tenant_is_unattributed};

    #[test]
    fn licence_is_absent_covers_the_reference_three_ways() {
        assert!(licence_is_absent(&None), "no licence at all");
        assert!(licence_is_absent(&Some(String::new())), "empty licence");
        assert!(licence_is_absent(&Some("   ".to_owned())), "blank licence");
        assert!(
            licence_is_absent(&Some("unknown".to_owned())),
            "the licence server's unresolved literal"
        );
        assert!(
            licence_is_absent(&Some("UnKnOwN".to_owned())),
            "the literal is matched case-insensitively"
        );
        assert!(
            !licence_is_absent(&Some("lic-0123456789abcdef".to_owned())),
            "a real licence is present"
        );
    }

    #[test]
    fn tenant_id_rejection_reason_mirrors_the_reference_closed_set() {
        for credential in ["Bearer x", "__credential_redacted__"] {
            assert_eq!(
                tenant_id_rejection_reason(credential),
                Some("credential_shaped"),
                "a credential sent as a tenant is reported as one, never as whitespace: {credential}"
            );
        }
        for literal in ["null", "TRUE", "0"] {
            assert_eq!(
                tenant_id_rejection_reason(literal),
                Some("not_an_identifier_literal"),
                "a rendered literal is not an identifier: {literal}"
            );
        }
        let too_long = "a".repeat(65);
        assert_eq!(tenant_id_rejection_reason(&too_long), Some("too_long"));
        assert_eq!(
            tenant_id_rejection_reason("a b"),
            Some("contains_whitespace")
        );
        assert_eq!(
            tenant_id_rejection_reason("tenant-1"),
            None,
            "a registered legacy tenant NAME is billable"
        );
        assert_eq!(
            tenant_id_rejection_reason("7d9c2b6e-0f3a-4a57-9c1e-2b6a1f0e4d55"),
            None,
            "a uuid is billable"
        );
    }

    #[test]
    fn tenant_is_unattributed_covers_none_blank_and_the_nil_sentinel() {
        assert!(tenant_is_unattributed(&None), "no tenant at all");
        assert!(tenant_is_unattributed(&Some(String::new())), "empty tenant");
        assert!(
            tenant_is_unattributed(&Some("   ".to_owned())),
            "blank tenant"
        );
        assert!(
            tenant_is_unattributed(&Some("00000000-0000-0000-0000-000000000000".to_owned())),
            "the nil sentinel is never a real tenant"
        );
        assert!(!tenant_is_unattributed(&Some("tenant-1".to_owned())));
    }
}
