use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// The one message shape on every stream (CONTRACT §7).
///
/// Serialised with exactly these field names — the envelope is read by the process runtime, the
/// tool runtime and (through the relay) the `event` row, so a renamed field is a wire break, not a
/// refactor. The optional members are `Option` because not every topic carries them; they are
/// **not** skipped when empty, so every envelope on the wire has the same eight keys.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    /// The idempotency key: delivery is at-least-once, so a handler de-duplicates on this.
    pub id: Uuid,
    /// One of the crate's `topic` constants (`work.requested`, `tool.call`, …).
    pub topic: String,
    /// The emitting role — `process`, `tool` or `relay`. Constants, not free text, keep a
    /// consumer's logs readable.
    pub from: String,
    /// Ties the envelopes of one run (or one tool round trip) together.
    #[serde(default)]
    pub correlation_id: Option<Uuid>,
    /// `name@version` of the procedure the envelope belongs to.
    #[serde(default)]
    pub procedure: Option<String>,
    /// The tenant the stream is scoped to; a consumer may refuse an envelope whose `tenant_id`
    /// disagrees with [`crate::stream_for`]'s.
    pub tenant_id: Uuid,
    /// The job document the step's handler needs. Large on purpose: the tool runtimes resolve it
    /// from here, not from a side channel, so a redelivery is self-contained.
    #[serde(default)]
    pub job: Option<Value>,
    /// The message itself.
    pub payload: Value,
}
