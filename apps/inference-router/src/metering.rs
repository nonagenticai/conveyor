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
pub trait CostSink: Send + Sync {
    fn emit<'a>(&'a self, event: &'a CostEvent) -> BoxFuture<'a, ()>;
}

/// Metering is off: no URL configured, or a test that does not assert the bus.
pub struct NoopCostSink;

impl CostSink for NoopCostSink {
    fn emit<'a>(&'a self, _event: &'a CostEvent) -> BoxFuture<'a, ()> {
        Box::pin(async {})
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
    fn emit<'a>(&'a self, event: &'a CostEvent) -> BoxFuture<'a, ()> {
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
            if let Err(error) = result {
                tracing::warn!(%error, stream = %self.stream, "cost event was not written");
            }
        })
    }
}
