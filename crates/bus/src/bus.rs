use std::collections::BTreeMap;

use redis::aio::MultiplexedConnection;
use redis::streams::{
    StreamAutoClaimReply, StreamId, StreamPendingCountReply, StreamRangeReply, StreamReadReply,
};
use thiserror::Error;

use crate::{DEAD_STREAM, Envelope};

/// A care-bus failure.
///
/// The Redis variant is deliberately thin: the caller keeps the envelope and the stream id, so its
/// remedy is a retry (the entry is still pending) or a dead letter — not a message it has to parse.
#[derive(Debug, Error)]
pub enum BusError {
    #[error("redis: {0}")]
    Redis(#[from] redis::RedisError),
    #[error("the entry {entry_id} on {stream} has no `envelope` field")]
    MissingEnvelope { stream: String, entry_id: String },
    #[error("the envelope of {entry_id} on {stream} is not valid JSON: {source}")]
    Envelope {
        stream: String,
        entry_id: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("serialise the envelope for {stream}: {source}")]
    Serialise {
        stream: String,
        #[source]
        source: serde_json::Error,
    },
}

/// The care-bus's Redis Streams operations, over one multiplexed async connection.
///
/// One connection serves every command; [`connect`](Self::connect) is a single handshake and
/// `clone` is cheap, so a task that blocks in a loop takes its own `Bus` rather than sharing this
/// one. A **blocking** [`read_group`](Self::read_group) parks the connection it runs on for up to
/// `block_ms`, which is harmless for a dedicated `Bus` and wrong for a shared one.
#[derive(Clone)]
pub struct Bus {
    conn: MultiplexedConnection,
}

impl Bus {
    /// Connect to `url` (`redis://host:port[/db]`).
    ///
    /// The response timeout is removed rather than tuned: `XREADGROUP … BLOCK n` is a legitimate
    /// command here, and redis-rs's default timeout would abort it mid-block. The consequence is
    /// honest and bounded by design — a Redis that stops answering parks this connection instead
    /// of erroring, which the caller survives because Postgres, not Redis, is the record.
    pub async fn connect(url: &str) -> Result<Self, BusError> {
        let client = redis::Client::open(url)?;
        let config = redis::AsyncConnectionConfig::new().set_response_timeout(None);
        let conn = client
            .get_multiplexed_async_connection_with_config(&config)
            .await?;
        Ok(Self { conn })
    }

    /// `XADD` the envelope to `stream` as its one `envelope` field; returns the new entry id.
    ///
    /// The stream is created by the `XADD` if it does not exist, and no `MAXLEN` is applied: the
    /// `event` row is the record and an over-long stream is an operational fact (an ack that never
    /// happened), not something this method may silently truncate.
    pub async fn publish(&self, stream: &str, envelope: &Envelope) -> Result<String, BusError> {
        let json = serde_json::to_string(envelope).map_err(|source| BusError::Serialise {
            stream: stream.to_owned(),
            source,
        })?;
        let mut conn = self.conn.clone();
        let id: String = redis::cmd("XADD")
            .arg(stream)
            .arg("*")
            .arg("envelope")
            .arg(json)
            .query_async(&mut conn)
            .await?;
        Ok(id)
    }

    /// `FLUSHALL` — the Redis-loss rehearsal.
    ///
    /// Production never calls this; the test that proves the replay property does. The bus holds
    /// no state the run depends on, so wiping it must leave Postgres able to reproduce the same
    /// outcomes: that is exactly what the replay test asserts.
    pub async fn flush_all(&self) -> Result<(), BusError> {
        let mut conn = self.conn.clone();
        redis::cmd("FLUSHALL").query_async::<()>(&mut conn).await?;
        Ok(())
    }

    /// `XGROUP CREATE <stream> <group> 0 MKSTREAM`, tolerating the group already existing.
    ///
    /// `BUSYGROUP` is the one error that means "somebody else created it first" — the normal case
    /// when several replicas boot together — and is a success here. Every other error, including a
    /// group created at a different id, is returned.
    pub async fn ensure_group(&self, stream: &str, group: &str) -> Result<(), BusError> {
        let mut conn = self.conn.clone();
        let created: redis::RedisResult<()> = redis::cmd("XGROUP")
            .arg("CREATE")
            .arg(stream)
            .arg(group)
            .arg("0")
            .arg("MKSTREAM")
            .query_async(&mut conn)
            .await;
        match created {
            Ok(()) => Ok(()),
            Err(error) if error.code() == Some("BUSYGROUP") => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    /// `XREADGROUP` up to `count` **new** entries of `stream` as `consumer` in `group`.
    ///
    /// `block_ms: None` returns immediately (possibly empty); `Some(ms)` blocks up to `ms` for an
    /// entry — `Some(0)` blocks until one arrives, which is why a blocking caller owns its `Bus`.
    /// A read with `>` moves the group's shared cursor, so two consumers of one group split the
    /// stream instead of copying it; the entries this returns are **pending** until
    /// [`ack`](Self::ack) or [`autoclaim`](Self::autoclaim).
    ///
    /// An entry without a parsable envelope is an error naming its id, not a skip: skipping would
    /// leave it pending forever with nothing to dead-letter.
    pub async fn read_group(
        &self,
        stream: &str,
        group: &str,
        consumer: &str,
        count: usize,
        block_ms: Option<usize>,
    ) -> Result<Vec<(String, Envelope)>, BusError> {
        let mut conn = self.conn.clone();
        let mut command = redis::cmd("XREADGROUP");
        // ⛔ The argument order is Redis's: COUNT and BLOCK precede STREAMS, never follow it.
        command
            .arg("GROUP")
            .arg(group)
            .arg(consumer)
            .arg("COUNT")
            .arg(count);
        if let Some(ms) = block_ms {
            command.arg("BLOCK").arg(ms);
        }
        command.arg("STREAMS").arg(stream).arg(">");
        let reply: Option<StreamReadReply> = command.query_async(&mut conn).await?;
        let mut entries = Vec::new();
        for key in reply.map(|reply| reply.keys).unwrap_or_default() {
            for entry in key.ids {
                entries.push(decode(stream, &entry)?);
            }
        }
        Ok(entries)
    }

    /// `XACK` one entry: the handler's transaction has committed, so it must never be redelivered.
    pub async fn ack(&self, stream: &str, group: &str, id: &str) -> Result<(), BusError> {
        let mut conn = self.conn.clone();
        // The reply is the number of entries actually removed from the PEL (0 if it was already
        // acked); both are success, because the post-condition is "not pending".
        let _acked: i64 = redis::cmd("XACK")
            .arg(stream)
            .arg(group)
            .arg(id)
            .query_async(&mut conn)
            .await?;
        Ok(())
    }

    /// `XAUTOCLAIM` the entries `consumer` may take over from a dead peer in `group`.
    ///
    /// `min_idle_ms` is the claim's only safety valve: an entry that has been pending at least
    /// that long belongs to a consumer that is not making progress, and `0` therefore claims
    /// everything pending (right for tests and a fencing-free first pass, wrong for a live handler
    /// that shares a stream with a slow-but-alive peer). The claim is a delivery — it increments
    /// the entry's delivery counter, which is what [`deliveries`](Self::deliveries) reads.
    pub async fn autoclaim(
        &self,
        stream: &str,
        group: &str,
        consumer: &str,
        min_idle_ms: u64,
        count: usize,
    ) -> Result<Vec<(String, Envelope)>, BusError> {
        let mut conn = self.conn.clone();
        let reply: StreamAutoClaimReply = redis::cmd("XAUTOCLAIM")
            .arg(stream)
            .arg(group)
            .arg(consumer)
            .arg(min_idle_ms)
            .arg("0-0")
            .arg("COUNT")
            .arg(count)
            .query_async(&mut conn)
            .await?;
        let mut entries = Vec::with_capacity(reply.claimed.len());
        for entry in &reply.claimed {
            entries.push(decode(stream, entry)?);
        }
        Ok(entries)
    }

    /// How many times `group` has delivered entry `id`, from `XPENDING`'s extended form; `0` when
    /// the entry is not pending (never delivered, or already acked).
    ///
    /// This is the counter a consumer compares with [`MAX_DELIVERIES`](crate::MAX_DELIVERIES).
    pub async fn deliveries(&self, stream: &str, group: &str, id: &str) -> Result<u64, BusError> {
        let mut conn = self.conn.clone();
        let pending: StreamPendingCountReply = redis::cmd("XPENDING")
            .arg(stream)
            .arg(group)
            .arg(id)
            .arg(id)
            .arg(1)
            .query_async(&mut conn)
            .await?;
        Ok(pending.ids.first().map_or(0, |entry| {
            u64::try_from(entry.times_delivered).unwrap_or(u64::MAX)
        }))
    }

    /// `XADD bus:dead` with `{reason, envelope}`; returns the dead letter's entry id.
    ///
    /// The original envelope is stored whole, not summarised: a replay is then a re-publish of the
    /// same document, and an operator can see the payload that poisoned the handler. Acking the
    /// **source** entry is the caller's next act — this method does not know which stream it came
    /// from (or whether it was a claim), and a dead letter that leaves the source pending is
    /// redelivered forever.
    pub async fn dead_letter(&self, envelope: &Envelope, reason: &str) -> Result<String, BusError> {
        let json = serde_json::to_string(envelope).map_err(|source| BusError::Serialise {
            stream: DEAD_STREAM.to_owned(),
            source,
        })?;
        let mut conn = self.conn.clone();
        let id: String = redis::cmd("XADD")
            .arg(DEAD_STREAM)
            .arg("*")
            .arg("reason")
            .arg(reason)
            .arg("envelope")
            .arg(json)
            .query_async(&mut conn)
            .await?;
        Ok(id)
    }

    /// One stream entry's raw fields (`XRANGE <stream> <id> <id>`); empty when the id is not on
    /// the stream.
    ///
    /// No consumer group reads `bus:dead`, so this is how a dead letter is inspected: its fields
    /// are `reason` and `envelope`, and [`dead_letter`](Self::dead_letter) returns the id to ask
    /// for. Reading by id rather than by range keeps a busy dead-letter stream from truncating the
    /// entry an operator (or a test) is looking for.
    pub async fn read_entry(
        &self,
        stream: &str,
        id: &str,
    ) -> Result<BTreeMap<String, String>, BusError> {
        let mut conn = self.conn.clone();
        let reply: StreamRangeReply = redis::cmd("XRANGE")
            .arg(stream)
            .arg(id)
            .arg(id)
            .arg("COUNT")
            .arg(1)
            .query_async(&mut conn)
            .await?;
        Ok(reply
            .ids
            .first()
            .map(|entry| {
                entry
                    .map
                    .keys()
                    .filter_map(|key| entry.get::<String>(key).map(|value| (key.clone(), value)))
                    .collect()
            })
            .unwrap_or_default())
    }
}

/// One stream entry as `(entry_id, envelope)`, or the error naming which entry is unusable.
fn decode(stream: &str, entry: &StreamId) -> Result<(String, Envelope), BusError> {
    let raw: String = entry
        .get("envelope")
        .ok_or_else(|| BusError::MissingEnvelope {
            stream: stream.to_owned(),
            entry_id: entry.id.clone(),
        })?;
    let envelope: Envelope = serde_json::from_str(&raw).map_err(|source| BusError::Envelope {
        stream: stream.to_owned(),
        entry_id: entry.id.clone(),
        source,
    })?;
    Ok((entry.id.clone(), envelope))
}
