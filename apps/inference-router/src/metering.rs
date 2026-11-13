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
