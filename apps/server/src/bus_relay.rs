#![forbid(unsafe_code)]
//! The outbox relay (CONTRACT §7): unpublished `event` rows → Redis Streams.
//!
//! The `event` row is the publish source and Postgres is the record. This task drains each
//! active tenant's unpublished rows by `seq` — woken by the existing `LISTEN` doorbell and, as
//! the fallback that makes a missed wake-up harmless, once per [`RELAY_POLL`] — XADDs one
//! [`Envelope`] per row, and marks the pair `(published_at, bus_stream_id)` in the same
//! transaction it read them in. A crash between the XADD and the commit re-delivers the
//! envelope, which is the at-least-once contract every consumer already de-duplicates on
//! [`Envelope::id`].
//!
//! **A Redis outage never blocks the server.** The task is spawned, not awaited; a failed
//! connect or a lost connection is logged and retried, and because the rows stay unpublished
//! the bus catches up from Postgres when Redis returns. This is the meaning of "the bus is a
//! nudge": nothing here is the only copy of anything.
//!
//! One mapping is not a pass-through: `run_step.requested` becomes `work.requested` carrying
//! the `process.step` job document, because that is the one event that names work this build
//! executes. `card.moved` is mapped to [`TOPIC_CARD_MOVED`] — the same spelling, named once so
//! the consumer's match cannot drift from the emitter's constant. Every other event travels
//! under its own type and carries no job, so a subscriber can read it without a second mapping
//! table. The payload always gains `event_type` and `aggregate_id`, which the envelope itself
//! does not carry.

use std::time::Duration;

use conveyor_bus::{Bus, Envelope, GROUP_PROCESS, TOPIC_WORK_REQUESTED, stream_for};
use conveyor_domain::TenantId;
use conveyor_store as store;
use conveyor_store::EventRow;
use serde_json::{Map, Value};
use sqlx::PgPool;
use tokio::sync::watch;

use crate::shutdown;

/// The fallback poll. The doorbell is the fast path; a missed or coalesced notification costs at
/// most this long, and the re-read is `published_at IS NULL` so a poll with nothing new is one
/// indexed statement per tenant.
pub const RELAY_POLL: Duration = Duration::from_secs(1);

/// The most rows one page marks. Bounded so one tenant's backlog cannot hold a transaction (and
/// a pgbouncer server connection) open across an unbounded number of XADDs.
pub const RELAY_PAGE: i64 = 500;

/// The topic a `card.moved` event is relayed under (S13 §5.8's `wip.pull` trigger).
///
/// The bus spellings and the store's are deliberately the same string, but they are two
/// vocabularies: this one is the consumer's match key. Keeping it here — beside the one function
/// that maps event types to topics — is what lets the worker match a constant instead of
/// re-typing the literal.
pub const TOPIC_CARD_MOVED: &str = "card.moved";

/// How long to wait before retrying a failed Redis connect or a lost connection. One second is
/// the poll's own cadence: a Redis that comes back is picked up on the next tick.
pub const RELAY_RECONNECT: Duration = Duration::from_secs(1);

/// `REDIS_URL`, trimmed; `None` when it is unset or blank.
///
/// Unset is a supported deployment (Postgres-only): the relay does not start and the `process.step`
/// job arm is the only driver. The consumer reads the same variable through this helper, so the
/// two tasks can never disagree about whether the bus is on.
pub fn redis_url() -> Option<String> {
    std::env::var("REDIS_URL")
        .ok()
        .map(|url| url.trim().to_owned())
        .filter(|url| !url.is_empty())
}

/// Why one relay pass stopped.
#[derive(Debug, thiserror::Error)]
pub enum RelayError {
    /// The outbox could not be read or marked. The rows stay unpublished; the next pass retries.
    #[error("database: {0}")]
    Database(#[from] sqlx::Error),
    /// Redis is unreachable. The task reconnects from the same cursor — never from a copied one.
    #[error("bus: {0}")]
    Bus(#[from] conveyor_bus::BusError),
}

/// Start the relay in the background: the `LISTEN` doorbell wakes it, [`RELAY_POLL`] is the
/// fallback.
///
/// `doorbell` is [`conveyor_store::EventListen::subscribe`]'s receiver. Its value is never
/// compared with anything: the doorbell says "something was written", and the relay answers by
/// re-reading its own tenant-scoped rows, exactly as the SSE subscriber does.
pub fn spawn(pool: PgPool, doorbell: watch::Receiver<i64>) {
    let Some(url) = redis_url() else {
        tracing::info!(
            "REDIS_URL is not set: the bus relay does not start; the outbox stays local"
        );
        return;
    };
    tokio::spawn(async move {
        tokio::select! {
            () = run(pool, url, doorbell) => {},
            () = shutdown::signal() => tracing::info!("bus relay stopped"),
        }
    });
}

/// Connect, drain, reconnect — forever. `select!` in [`spawn`] is what ends it.
async fn run(pool: PgPool, url: String, mut doorbell: watch::Receiver<i64>) {
    loop {
        let bus = match Bus::connect(&url).await {
            Ok(bus) => bus,
            Err(error) => {
                tracing::warn!(%error, "the bus relay cannot reach Redis; retrying");
                tokio::time::sleep(RELAY_RECONNECT).await;
                continue;
            }
        };
        tracing::info!("bus relay connected");
        loop {
            wake(&mut doorbell).await;
            match drain(&pool, &bus).await {
                Ok(0) => {}
                Ok(published) => tracing::info!(published, "bus relay published"),
                // A database failure is not a reason to drop Redis: retry the read.
                Err(RelayError::Database(error)) => {
                    tracing::warn!(%error, "the bus relay could not read the outbox; retrying");
                    tokio::time::sleep(RELAY_RECONNECT).await;
                }
                // A Redis failure is: reconnect and re-read from the same cursor.
                Err(RelayError::Bus(error)) => {
                    tracing::warn!(%error, "the bus relay lost Redis; reconnecting");
                    break;
                }
            }
        }
    }
}

/// Wait for the next doorbell, or [`RELAY_POLL`] when none arrives in time.
async fn wake(doorbell: &mut watch::Receiver<i64>) {
    tokio::select! {
        result = doorbell.changed() => {
            // A closed channel means the LISTEN task is gone (shutdown); sleep so this loop
            // cannot spin while `spawn`'s select finishes.
            if result.is_err() {
                tokio::time::sleep(RELAY_POLL).await;
            }
        },
        () = tokio::time::sleep(RELAY_POLL) => {},
    }
}

/// One pass over every active tenant. Returns how many envelopes were published.
async fn drain(pool: &PgPool, bus: &Bus) -> Result<usize, RelayError> {
    let mut published = 0;
    for tenant in store::tenants::active_ids(pool).await? {
        published += drain_tenant(pool, bus, tenant).await?;
    }
    Ok(published)
}

/// Drain one tenant in pages. A full page means there may be more, so the read repeats; a short
/// page is the end of the backlog.
async fn drain_tenant(pool: &PgPool, bus: &Bus, tenant: TenantId) -> Result<usize, RelayError> {
    let stream = stream_for(tenant.as_uuid(), GROUP_PROCESS);
    let mut published = 0;
    loop {
        let mut tx = store::begin_scoped(pool, tenant).await?;
        let rows = store::bus::unpublished_tx(&mut tx, tenant, RELAY_PAGE).await?;
        if rows.is_empty() {
            tx.commit().await?;
            break;
        }
        let full_page = i64::try_from(rows.len()).unwrap_or(RELAY_PAGE) >= RELAY_PAGE;
        for row in &rows {
            let envelope = envelope(row);
            let stream_id = bus.publish(&stream, &envelope).await?;
            store::bus::mark_published_tx(&mut tx, tenant, row.id, &stream_id).await?;
        }
        tx.commit().await?;
        published += rows.len();
        if !full_page {
            break;
        }
    }
    Ok(published)
}

/// One event as the bus's [`Envelope`].
///
/// `run_step.requested` is the one type that names work this build executes, so it becomes
/// `work.requested` with a `process.step` job. `card.moved` keeps its spelling but is named
/// through [`TOPIC_CARD_MOVED`] here: it is the one board event whose arrival on the bus is a
/// behavioural contract (`wip.pull`), not a notification. Every other type keeps its own name
/// as the topic. `from` is the aggregate type, `procedure` is unused on this stream, and the
/// payload is the event's document plus the two facts the envelope does not repeat.
fn envelope(row: &EventRow) -> Envelope {
    let is_step = row.event_type == store::process::RUN_STEP_REQUESTED;
    let topic = if is_step {
        TOPIC_WORK_REQUESTED.to_owned()
    } else if row.event_type == conveyor_actions::board::events::CARD_MOVED {
        TOPIC_CARD_MOVED.to_owned()
    } else {
        row.event_type.clone()
    };
    let job = if is_step {
        Some(serde_json::json!({ "kind": store::process::PROCESS_STEP_KIND }))
    } else {
        None
    };
    Envelope {
        id: row.id,
        topic,
        from: row.aggregate_type.clone(),
        correlation_id: row.correlation_id,
        procedure: None,
        tenant_id: row.tenant_id,
        job,
        payload: payload(row),
    }
}

/// The event's payload plus `event_type` and `aggregate_id`.
///
/// The envelope carries neither field, and a consumer that reads `work.requested` needs to know
/// which event produced the nudge. A payload that is not an object (no current emitter writes
/// one) is preserved under `value` rather than silently replaced.
fn payload(row: &EventRow) -> Value {
    let mut object = match row.payload.clone() {
        Value::Object(map) => map,
        other => {
            let mut map = Map::new();
            map.insert("value".to_owned(), other);
            map
        }
    };
    object.insert(
        "event_type".to_owned(),
        Value::String(row.event_type.clone()),
    );
    object.insert(
        "aggregate_id".to_owned(),
        Value::String(row.aggregate_id.to_string()),
    );
    Value::Object(object)
}
#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    /// One `event` row, as the outbox writes it.
    fn row(event_type: &str, event_id: Uuid) -> EventRow {
        EventRow {
            seq: 7,
            id: Uuid::now_v7(),
            tenant_id: Uuid::now_v7(),
            topic: format!("run:{event_id}"),
            event_type: event_type.to_owned(),
            aggregate_type: "run".to_owned(),
            aggregate_id: event_id,
            payload: serde_json::json!({"run_id": event_id, "position": 2, "attempt": 1}),
            correlation_id: None,
            causation_id: None,
            created_at: chrono::Utc::now(),
        }
    }

    /// The one mapping the consumer acts on: a step request becomes work for the `process`
    /// group, carrying the job document the executor is dispatched on.
    #[test]
    fn a_step_request_becomes_work_requested_with_a_process_step_job() {
        let run_id = Uuid::now_v7();
        let event = row(store::process::RUN_STEP_REQUESTED, run_id);
        let envelope = envelope(&event);

        assert_eq!(envelope.id, event.id);
        assert_eq!(envelope.topic, TOPIC_WORK_REQUESTED);
        assert_eq!(envelope.from, "run");
        assert_eq!(envelope.tenant_id, event.tenant_id);
        assert_eq!(
            envelope.job,
            Some(serde_json::json!({"kind": store::process::PROCESS_STEP_KIND}))
        );
        assert_eq!(envelope.payload["run_id"], serde_json::json!(run_id));
        assert_eq!(envelope.payload["position"], serde_json::json!(2));
        assert_eq!(envelope.payload["attempt"], serde_json::json!(1));
        assert_eq!(
            envelope.payload["event_type"],
            serde_json::json!(store::process::RUN_STEP_REQUESTED)
        );
        assert_eq!(
            envelope.payload["aggregate_id"],
            serde_json::json!(event.aggregate_id.to_string())
        );
    }

    /// `card.moved` is the one board event the worker acts on (`wip.pull` when the lane entered
    /// is active). It keeps its spelling, and this asserts the spelling the consumer matches.
    #[test]
    fn a_card_move_is_relayed_under_the_card_moved_topic() {
        let card_id = Uuid::now_v7();
        let mut event = row(conveyor_actions::board::events::CARD_MOVED, card_id);
        event.aggregate_type = "card".to_owned();
        event.payload = serde_json::json!({
            "card_id": card_id,
            "from_list_id": Uuid::now_v7(),
            "to_list_id": Uuid::now_v7(),
            "position": 1024.0,
        });
        let envelope = envelope(&event);

        assert_eq!(envelope.topic, TOPIC_CARD_MOVED);
        assert_eq!(TOPIC_CARD_MOVED, "card.moved");
        assert_eq!(
            TOPIC_CARD_MOVED,
            conveyor_actions::board::events::CARD_MOVED,
            "the bus topic and the emitter's constant are one spelling"
        );
        assert_eq!(envelope.job, None, "a move is a nudge, not a job document");
        assert_eq!(envelope.payload["to_list_id"], event.payload["to_list_id"]);
        assert_eq!(
            envelope.payload["event_type"],
            serde_json::json!(conveyor_actions::board::events::CARD_MOVED)
        );
    }

    /// Everything else is a notification, not work: its own type is the topic and it names no
    /// job. The two added payload fields are present on every envelope.
    #[test]
    fn every_other_event_keeps_its_type_as_the_topic_and_names_no_job() {
        let event = row("run.started", Uuid::now_v7());
        let envelope = envelope(&event);

        assert_eq!(envelope.topic, "run.started");
        assert_eq!(envelope.job, None);
        assert_eq!(envelope.from, "run");
        assert_eq!(
            envelope.payload["event_type"],
            serde_json::json!("run.started")
        );
        assert!(envelope.payload.get("aggregate_id").is_some());
        assert_eq!(envelope.procedure, None);
    }

    /// A payload that is not an object is kept, not dropped.
    #[test]
    fn a_non_object_payload_is_preserved_under_value() {
        let mut event = row("run.started", Uuid::now_v7());
        event.payload = serde_json::json!([1, 2, 3]);
        let envelope = envelope(&event);
        assert_eq!(envelope.payload["value"], serde_json::json!([1, 2, 3]));
        assert_eq!(
            envelope.payload["event_type"],
            serde_json::json!("run.started")
        );
    }
}
