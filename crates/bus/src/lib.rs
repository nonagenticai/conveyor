#![forbid(unsafe_code)]
//! The care-bus port (CONTRACT §7): Redis Streams carry an [`Envelope`] between the process and
//! tool runtimes, with consumer groups, autoclaim for the entries a dead consumer left pending,
//! and a dead-letter stream after [`MAX_DELIVERIES`].
//!
//! **The bus is a nudge, never the record.** `run`, `run_step` and `event` hold the truth in
//! Postgres; this crate only moves the notification that work is available, so a Redis restart
//! loses nothing that matters and a re-publish of the unacked rows restores the stream. A handler
//! that writes state only to Redis would be relying on the one component whose loss the design
//! already tolerates.
//!
//! Delivery is **at-least-once**. A consumer group balances a stream between its consumers; it does
//! not de-duplicate it, so the same envelope can reach two consumers (or the same one twice) and
//! every handler must be idempotent on [`Envelope::id`]. Acking is the caller's act, after its
//! transaction commits — never before.
//!
//! Streams are per tenant and per consumer group: [`stream_for`] is the one naming rule, so two
//! tenants can never share a stream even when they run the same procedure.

mod bus;
mod envelope;

pub use bus::{Bus, BusError};
pub use envelope::Envelope;

/// The consumer group that executes process steps.
pub const GROUP_PROCESS: &str = "process";
/// The consumer group that runs tool calls.
pub const GROUP_TOOL: &str = "tool";

/// A step is ready to run.
pub const TOPIC_WORK_REQUESTED: &str = "work.requested";
/// The step's outcome.
pub const TOPIC_WORK_SUCCEEDED: &str = "work.succeeded";
/// The step's failure (the retry ladder decides what happens next).
pub const TOPIC_WORK_FAILED: &str = "work.failed";
/// The process asks for a tool.
pub const TOPIC_TOOL_CALL: &str = "tool.call";
/// The tool answers.
pub const TOPIC_TOOL_RESPONSE: &str = "tool.response";
/// Every entry on [`DEAD_STREAM`].
pub const TOPIC_DEAD_LETTER: &str = "dead.letter";

/// How many deliveries a consumer tolerates before dead-lettering an entry.
///
/// The bus does not enforce this: a consumer that has lost the entry cannot be the one to count
/// it. The consumer reads [`Bus::deliveries`] after a read or an autoclaim and, past this ceiling,
/// calls [`Bus::dead_letter`] and acks the source entry. Why five: enough that a redeploy or a
/// transient handler bug is retried, few enough that a poison message stops consuming a stream.
pub const MAX_DELIVERIES: u64 = 5;

/// The one dead-letter stream (`bus:dead`). It carries no tenant in its name: each entry holds the
/// original envelope, which carries [`Envelope::tenant_id`], and the reason.
pub const DEAD_STREAM: &str = "bus:dead";

/// The stream an envelope for `tenant` is published to for `group`: `bus:<tenant>:<group>`.
///
/// One stream per (tenant, consumer group) is the isolation rule: a group's cursor then cannot
/// skip another tenant's work, and a `FLUSHALL` plus a replay of the unacked `event` rows
/// reproduces the same run state.
pub fn stream_for(tenant: uuid::Uuid, group: &str) -> String {
    format!("bus:{tenant}:{group}")
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use uuid::Uuid;

    use super::{Envelope, GROUP_PROCESS, GROUP_TOOL, TOPIC_WORK_REQUESTED, stream_for};

    #[test]
    fn the_stream_name_is_the_tenant_then_the_group() {
        let tenant = Uuid::now_v7();
        assert_eq!(
            stream_for(tenant, GROUP_PROCESS),
            format!("bus:{tenant}:process")
        );
        assert_eq!(stream_for(tenant, GROUP_TOOL), format!("bus:{tenant}:tool"));
    }

    /// The envelope is a wire format: a renamed or added field breaks every consumer, and the
    /// ported care-bus's `env` field must not come back (CONTRACT §7 has no per-envelope
    /// environment — the tenant is `tenant_id`).
    #[test]
    fn the_envelope_serialises_exactly_the_eight_contract_fields() {
        let tenant = Uuid::now_v7();
        let envelope = Envelope {
            id: Uuid::now_v7(),
            topic: TOPIC_WORK_REQUESTED.to_owned(),
            from: "process".to_owned(),
            correlation_id: Some(Uuid::now_v7()),
            procedure: Some("process.card.build@3".to_owned()),
            tenant_id: tenant,
            job: None,
            payload: json!({"step": "read"}),
        };
        let value = serde_json::to_value(&envelope).expect("serialise the envelope");
        let object = value.as_object().expect("the envelope is an object");
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "correlation_id",
                "from",
                "id",
                "job",
                "payload",
                "procedure",
                "tenant_id",
                "topic",
            ]
        );
        assert!(!object.contains_key("env"));

        let back: Envelope = serde_json::from_value(value).expect("deserialise the envelope");
        assert_eq!(back, envelope);
    }
}
