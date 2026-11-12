//! `GET /api/events?topics=…` — the replaying SSE stream (CONTRACT §7, data-arch §4).
//!
//! `Last-Event-ID` is `event.seq`. A connection replays
//! `seq > $last AND topic = ANY($topics) ORDER BY seq LIMIT 500` inside the subscriber's own
//! tenant scope, then follows the LISTEN doorbell; every wake-up re-runs the same scoped
//! query, so a notification can never leak another tenant's row and a missed wake-up costs
//! nothing but one poll.
//!
//! Three boundedness rules, each visible in the response rather than silent:
//! - a replay page of 500 means the client is further behind than one page → a `resync`
//!   event tells it to refetch state;
//! - a cursor below the tenant's oldest retained `seq` (a prune ran under it) or above the
//!   newest (the cursor is not from this database) → `resync`;
//! - a slow client fills the 64-slot channel → after a bounded wait the connection is
//!   closed, and a best-effort `resync` is the last frame it sees. Nothing is buffered
//!   without limit and nothing is skipped silently.
//!
//! Keepalive comments every 15 s come from axum's `KeepAlive`, so a proxy cannot mistake an
//! idle subscription for a dead one.

use std::convert::Infallible;
use std::time::Duration;

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::sse::{Event, KeepAlive};
use axum::response::{IntoResponse, Response, Sse};
use chrono::{DateTime, Utc};
use conveyor_store::outbox::{self, EventRow};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::{ApiError, FieldError, ProblemDetail};
use crate::extract::{Caller, RequireLicence};
use crate::state::AppState;

/// Frames buffered for one connection before it is treated as slow.
pub const CHANNEL_CAPACITY: usize = 64;

/// How long one frame may wait for the client before the connection is closed.
pub const SEND_TIMEOUT: Duration = Duration::from_secs(10);

/// The keepalive interval.
pub const KEEPALIVE: Duration = Duration::from_secs(15);

/// How long the last-resort `resync` may wait on a full channel before the stream just
/// closes.
pub const RESYNC_TIMEOUT: Duration = Duration::from_secs(1);

/// The most topics one subscription may name.
pub const MAX_TOPICS: usize = 32;

/// The subscription query string.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct EventsQuery {
    /// Comma-separated topics, e.g. `board:<uuid>,run:<uuid>`. At least one is required;
    /// subscribing to every aggregate in the tenant is deliberately not a mode.
    pub topics: Option<String>,
}

/// The SSE `data` frame for one event. `type` is the CONTRACT §7 event type.
#[derive(Debug, Serialize)]
pub struct EventFrame {
    /// `event.seq` — also the SSE `id`.
    pub seq: i64,
    /// The event's own id (consumers dedupe on it; delivery is at-least-once).
    pub id: String,
    /// The stream this event belongs to.
    pub topic: String,
    /// `<aggregate>.<past_tense_verb>`.
    #[serde(rename = "type")]
    pub event_type: String,
    /// The table that owns the emitter.
    pub aggregate_type: String,
    /// The aggregate the event is about.
    pub aggregate_id: String,
    /// The event document.
    pub payload: serde_json::Value,
    /// Operation correlation id.
    pub correlation_id: Option<String>,
    /// Causal predecessor.
    pub causation_id: Option<String>,
    /// When the row was written (RFC 3339, UTC).
    pub created_at: String,
}

impl From<&EventRow> for EventFrame {
    fn from(row: &EventRow) -> Self {
        Self {
            seq: row.seq,
            id: row.id.to_string(),
            topic: row.topic.clone(),
            event_type: row.event_type.clone(),
            aggregate_type: row.aggregate_type.clone(),
            aggregate_id: row.aggregate_id.to_string(),
            payload: row.payload.clone(),
            correlation_id: row.correlation_id.map(|id| id.to_string()),
            causation_id: row.causation_id.map(|id| id.to_string()),
            created_at: row.created_at.to_rfc3339(),
        }
    }
}

/// The `resync` frame: the client must refetch state, then continue from `last_event_id`.
#[derive(Debug, Serialize, ToSchema)]
pub struct ResyncFrame {
    /// Why the client must refetch (`history_pruned`, `cursor_ahead`,
    /// `replay_window_exceeded`, `slow_client`).
    pub reason: &'static str,
    /// The last `seq` the server had delivered when it decided to resync.
    pub last_event_id: i64,
}

fn sse(row: &EventRow) -> Event {
    let frame = EventFrame::from(row);
    let data = serde_json::to_string(&frame).unwrap_or_else(|_| "{}".to_owned());
    // `event` is the CONTRACT §7 type, which the outbox validated: no whitespace, one dot.
    Event::default()
        .id(row.seq.to_string())
        .event(row.event_type.clone())
        .data(data)
}

fn resync(reason: &'static str, last: i64) -> Event {
    let frame = ResyncFrame {
        reason,
        last_event_id: last,
    };
    let data = serde_json::to_string(&frame).unwrap_or_else(|_| "{}".to_owned());
    Event::default().event("resync").data(data)
}

/// Deliver one frame, or — when the client has stopped reading — send a best-effort
/// `resync` and end the stream. Returns false when the producer must stop.
async fn deliver(tx: &mpsc::Sender<Result<Event, Infallible>>, event: Event, last: i64) -> bool {
    deliver_within(tx, event, last, SEND_TIMEOUT, RESYNC_TIMEOUT).await
}

/// [`deliver`] with injectable patience, so the slow-client path is testable without a
/// ten-second wait. Both waits are parameters: a test can make the normal send time out on
/// its first poll while leaving the resync send room to complete once a slot frees.
async fn deliver_within(
    tx: &mpsc::Sender<Result<Event, Infallible>>,
    event: Event,
    last: i64,
    patience: Duration,
    resync_patience: Duration,
) -> bool {
    match tokio::time::timeout(patience, tx.send(Ok(event))).await {
        Ok(Ok(())) => true,
        _ => {
            // The channel is full (slow client) or the receiver is gone. One bounded
            // attempt at telling it why; if that fails too, the close itself is the signal.
            let _ = tokio::time::timeout(resync_patience, tx.send(Ok(resync("slow_client", last))))
                .await;
            false
        }
    }
}

fn parse_topics(raw: Option<&str>) -> Result<ParsedTopics, ApiError> {
    let raw = raw.ok_or_else(|| {
        ApiError::bad_request(
            "topics is required, e.g. /api/events?topics=board:0195f48b-d000-7000-8000-000000000099",
        )
        .with_errors(vec![FieldError {
            field: "topics".into(),
            code: "required".into(),
            detail: "name at least one topic".into(),
        }])
    })?;
    let mut parsed = ParsedTopics::default();
    for part in raw.split(',') {
        let topic = part.trim();
        if topic.is_empty() {
            continue;
        }
        if !outbox::valid_topic(topic) {
            return Err(ApiError::bad_request(format!(
                "topic {topic:?} is not <aggregate> or <aggregate>:<uuid>"
            ))
            .with_errors(vec![FieldError {
                field: "topics".into(),
                code: "invalid".into(),
                detail: topic.to_owned(),
            }]));
        }
        // `card:{id}` is a convenience subscription: it is RESOLVED to the card's board
        // topic plus an `aggregate_id` filter (S5-D4), so it never needs a second outbox row
        // and never inspects a payload. Every other topic is subscribed whole.
        if let Some(card_id) = topic.strip_prefix("card:").and_then(|id| id.parse().ok()) {
            if !parsed.cards.contains(&card_id) {
                parsed.cards.push(card_id);
            }
            continue;
        }
        if !parsed.topics.iter().any(|known| known == topic) {
            parsed.topics.push(topic.to_owned());
        }
    }
    if parsed.topics.is_empty() && parsed.cards.is_empty() {
        return Err(ApiError::bad_request("topics must name at least one topic"));
    }
    if parsed.topics.len() + parsed.cards.len() > MAX_TOPICS {
        return Err(ApiError::bad_request(format!(
            "at most {MAX_TOPICS} topics per subscription"
        )));
    }
    Ok(parsed)
}

/// The scope a topic's frames expose. `None` falls back to `events:read`, which every role holds
/// by default and which a tenant may still revoke.
///
/// This is the stream half of the RBAC promise: without it a viewer denied `runs:read` would
/// still receive run frames, and the screen's refusal would be false.
fn topic_scope(aggregate: &str) -> Option<conveyor_actions::Scope> {
    use conveyor_actions::Scope;
    Some(match aggregate {
        "board" | "list" | "label" | "custom_field" | "comment" => Scope::BoardsRead,
        "card" | "task" | "task_list" => Scope::CardsRead,
        "run" | "run_step" => Scope::RunsRead,
        "approval" => Scope::ApprovalsRead,
        "benchmark_run" | "benchmark_case" | "benchmark_suite" => Scope::BenchmarksRead,
        "eval" | "eval_version" | "eval_case" | "eval_metric" | "eval_run" | "eval_result"
        | "eval_finding" | "eval_audit" | "eval_climb" => Scope::EvalsRead,
        "prompt_audit" | "prompt_audit_finding" | "inference_call" => Scope::InferenceRead,
        _ => return None,
    })
}

/// Refuse a subscription naming a topic whose scope the caller's policy denies.
///
/// `card:{id}` is resolved to a board topic plus an aggregate filter, so it is gated on
/// `cards:read` here, before the resolution query runs.
fn ensure_topics_permitted(
    session: &conveyor_auth::Session,
    parsed: &ParsedTopics,
) -> Result<(), ApiError> {
    let permitted = |scope: conveyor_actions::Scope| {
        session.allows(scope.as_str(), scope.granted_to(session.role))
    };
    if !parsed.cards.is_empty() && !permitted(conveyor_actions::Scope::CardsRead) {
        return Err(scope_denied("cards:read"));
    }
    for topic in &parsed.topics {
        let aggregate = topic.split(':').next().unwrap_or(topic);
        let Some(scope) = topic_scope(aggregate) else {
            continue;
        };
        if !permitted(scope) {
            return Err(scope_denied(scope.as_str()));
        }
    }
    Ok(())
}

fn scope_denied(scope: &str) -> ApiError {
    ApiError::new(
        axum::http::StatusCode::FORBIDDEN,
        "scope_denied",
        format!("this subscription needs the {scope} scope, which this tenant's policy denies"),
    )
}

/// The topics a connection named, before `card:{id}` is resolved against the database.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParsedTopics {
    /// Topics subscribed in full (`board:{id}`, `run:{id}`, `board`).
    pub topics: Vec<String>,
    /// Cards whose `card.*` events are wanted (the `card:{id}` form).
    pub cards: Vec<Uuid>,
}

/// Resolve every `card:{id}` to its board topic plus the aggregate filter.
///
/// The card is read in the caller's tenant scope, so a card that exists only in another
/// tenant is a `404` — the same answer a mistyped id gets, and never an empty stream that
/// looks healthy. A `board:{id}` topic is NOT resolved: a subscriber may legitimately open
/// the stream before the board exists, and the replay is tenant-scoped regardless.
async fn resolve(
    pool: &sqlx::PgPool,
    tenant: conveyor_store::TenantId,
    parsed: ParsedTopics,
) -> Result<outbox::ReplayScope, ApiError> {
    let mut scope = outbox::ReplayScope {
        topics: parsed.topics,
        aggregate_topics: Vec::new(),
        aggregate_ids: Vec::new(),
    };
    if parsed.cards.is_empty() {
        return Ok(scope);
    }
    let mut tx = conveyor_store::begin_scoped(pool, tenant).await?;
    for card_id in parsed.cards {
        let card = conveyor_store::card::get_card_tx(&mut tx, tenant, card_id)
            .await?
            .ok_or_else(|| ApiError::not_found("no such card in this tenant"))?;
        let topic = format!("board:{}", card.board_id);
        if !scope.aggregate_topics.iter().any(|known| known == &topic) {
            scope.aggregate_topics.push(topic);
        }
        scope.aggregate_ids.push(card_id);
    }
    tx.commit().await?;
    Ok(scope)
}

fn parse_last_event_id(headers: &HeaderMap) -> Result<i64, ApiError> {
    let Some(value) = headers.get("last-event-id") else {
        return Ok(0);
    };
    let raw = value
        .to_str()
        .map_err(|_| ApiError::bad_request("Last-Event-ID must be a non-negative event seq"))?;
    raw.trim()
        .parse::<i64>()
        .ok()
        .filter(|seq| *seq >= 0)
        .ok_or_else(|| ApiError::bad_request("Last-Event-ID must be a non-negative event seq"))
}

/// Subscribe to the tenant's event stream.
#[utoipa::path(
    get,
    path = "/api/events",
    tag = "realtime",
    params(EventsQuery),
    responses(
        (
            status = 200,
            description = "An SSE stream: `id` is event.seq, `event` is the event type, \
                          and a `resync` frame means the client must refetch state",
            content_type = "text/event-stream"
        ),
        (status = 400, description = "Missing or invalid topics / Last-Event-ID",
         body = ProblemDetail),
        (status = 401, description = "No credential, or one that is refused", body = ProblemDetail),
        (status = 403, description = "The tenant has no active conveyor entitlement",
         body = ProblemDetail)
    )
)]
pub async fn events(
    State(state): State<AppState>,
    caller: Caller,
    _licence: RequireLicence,
    headers: HeaderMap,
    Query(query): Query<EventsQuery>,
) -> Result<Response, ApiError> {
    let parsed = parse_topics(query.topics.as_deref())?;
    ensure_topics_permitted(&caller.0, &parsed)?;
    let last = parse_last_event_id(&headers)?;
    // Resolution touches the database, so it happens HERE, before the stream opens: an
    // unknown card is a 404 problem, not a healthy stream that will never carry an event.
    let scope = resolve(&state.pool, caller.tenant, parsed).await?;

    let (tx, rx) = mpsc::channel(CHANNEL_CAPACITY);
    tokio::spawn(produce(
        state.pool.clone(),
        state.listen.clone(),
        caller.tenant,
        scope,
        last,
        tx,
    ));

    // `keep_alive` wraps the stream in `KeepAliveStream`, so the response is returned as
    // the erased `Response` rather than naming axum's internal combinator type.
    Ok(Sse::new(ReceiverStream::new(rx))
        .keep_alive(KeepAlive::new().interval(KEEPALIVE))
        .into_response())
}

/// The producer: cursor sanity, the replay page, then the doorbell loop.
async fn produce(
    pool: sqlx::PgPool,
    listen: conveyor_store::EventListen,
    tenant: conveyor_auth::TenantId,
    scope: outbox::ReplayScope,
    after_seq: i64,
    tx: mpsc::Sender<Result<Event, Infallible>>,
) {
    // Subscribe BEFORE the replay: a doorbell that fires during the replay must not be
    // lost between "the replay query returned" and "the live loop started".
    let mut doorbell = listen.subscribe();
    let mut last = after_seq;
    if last > 0 {
        match outbox::bounds(&pool, tenant).await {
            Ok((min, max)) => {
                if max.is_some_and(|max| last > max) {
                    // The cursor is ahead of this database (a restored DB, or another
                    // tenant's seq): nothing after it will ever arrive.
                    if !deliver(&tx, resync("cursor_ahead", last), last).await {
                        return;
                    }
                    last = max.unwrap_or(0);
                } else if min.is_some_and(|min| min > last + 1) {
                    // Rows between the cursor and the oldest retained row were pruned.
                    if !deliver(&tx, resync("history_pruned", last), last).await {
                        return;
                    }
                    last = min.unwrap_or(last) - 1;
                }
            }
            Err(error) => {
                tracing::warn!(%error, tenant = %tenant, "sse: cursor bounds failed");
                return;
            }
        }
    }

    // Replay in pages of 500. A full page means the client asked for more history than one
    // page carries: tell it to refetch, then keep streaming from the last delivered seq.
    loop {
        match outbox::replay_scoped(&pool, tenant, &scope, last, outbox::REPLAY_LIMIT).await {
            Ok(rows) if rows.is_empty() => break,
            Ok(rows) => {
                let full_page = rows.len() as i64 == outbox::REPLAY_LIMIT;
                for row in &rows {
                    last = row.seq;
                    if !deliver(&tx, sse(row), last).await {
                        return;
                    }
                }
                if full_page && !deliver(&tx, resync("replay_window_exceeded", last), last).await {
                    return;
                }
            }
            Err(error) => {
                tracing::warn!(%error, tenant = %tenant, "sse: replay failed; closing");
                return;
            }
        }
    }

    // Live: every doorbell re-runs the same scoped query. `changed()` can coalesce several
    // notifications into one wake-up, and the query catches up on all of them.
    loop {
        // `closed()` is the client-disconnect signal while the stream is idle: without it a
        // disconnected subscriber would leave this task parked on the doorbell until the
        // next event in the whole database woke it.
        tokio::select! {
            changed = doorbell.changed() => {
                if changed.is_err() {
                    return;
                }
            }
            _ = tx.closed() => return,
        }
        loop {
            match outbox::replay_scoped(&pool, tenant, &scope, last, outbox::REPLAY_LIMIT).await {
                Ok(rows) if rows.is_empty() => break,
                Ok(rows) => {
                    let full_page = rows.len() as i64 == outbox::REPLAY_LIMIT;
                    for row in &rows {
                        last = row.seq;
                        if !deliver(&tx, sse(row), last).await {
                            return;
                        }
                    }
                    if full_page
                        && !deliver(&tx, resync("replay_window_exceeded", last), last).await
                    {
                        return;
                    }
                }
                Err(error) => {
                    tracing::warn!(%error, tenant = %tenant, "sse: live replay failed");
                    return;
                }
            }
        }
    }
}

/// Format an event timestamp the way the frames do (used by the route's unit tests).
pub fn rfc3339(at: DateTime<Utc>) -> String {
    at.to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.append(*name, HeaderValue::from_str(value).unwrap());
        }
        map
    }

    #[test]
    fn topics_are_required_and_validated() {
        let id = "0195f48b-d000-7000-8000-000000000099";
        let board = format!("board:{id}");
        let run = format!("run:{id}");
        let both = parse_topics(Some(&format!("{board}, {run},{board}"))).unwrap();
        assert_eq!(both.topics, vec![board.clone(), run.clone()]);
        assert!(both.cards.is_empty());
        assert_eq!(
            parse_topics(Some(&board)).unwrap().topics,
            vec![board.clone()]
        );
        assert_eq!(
            parse_topics(Some("board")).unwrap().topics,
            vec!["board".to_owned()]
        );
        // `card:{id}` is parsed into the card list, never into the plain topics: it is
        // resolved to a board topic plus an aggregate filter before the stream opens.
        let card = format!("card:{id}");
        let parsed = parse_topics(Some(&format!("{board},{card}"))).unwrap();
        assert_eq!(parsed.topics, vec![board.clone()]);
        assert_eq!(parsed.cards, vec![id.parse::<uuid::Uuid>().expect("uuid")]);
        assert_eq!(
            parse_topics(Some(&format!("{card},{card}")))
                .unwrap()
                .cards
                .len(),
            1,
            "a repeated card subscription is one filter"
        );
        assert_eq!(
            parse_topics(Some(&card)).unwrap().cards.len(),
            1,
            "a card-only subscription is valid"
        );
        assert!(parse_topics(None).is_err());
        assert!(parse_topics(Some("")).is_err());
        assert!(parse_topics(Some(" , ")).is_err());
        assert!(parse_topics(Some("Board")).is_err());
        assert!(parse_topics(Some("board:notauuid")).is_err());
        assert!(parse_topics(Some("card:notauuid")).is_err());
        assert!(parse_topics(Some("board:a b")).is_err());
        let too_many = (0..=MAX_TOPICS)
            .map(|i| format!("board:0195f48b-d000-7000-8000-{i:012x}"))
            .collect::<Vec<_>>()
            .join(",");
        assert!(parse_topics(Some(&too_many)).is_err());
    }

    #[test]
    fn last_event_id_is_an_optional_non_negative_integer() {
        assert_eq!(parse_last_event_id(&headers(&[])).unwrap(), 0);
        assert_eq!(
            parse_last_event_id(&headers(&[("last-event-id", "42")])).unwrap(),
            42
        );
        for bad in ["", "-1", "abc", "1.5", " 1 2"] {
            assert!(
                parse_last_event_id(&headers(&[("last-event-id", bad)])).is_err(),
                "{bad:?}"
            );
        }
    }

    #[tokio::test]
    async fn a_slow_client_gets_one_resync_attempt_and_the_stream_ends() {
        // Capacity 1, already full, and the receiver is never read: neither send can
        // complete. `deliver_within` must therefore give up after its two bounded waits and
        // close the stream; nothing here depends on a timer/scheduler race.
        let (tx, _rx) = mpsc::channel(1);
        tx.send(Ok(Event::default().data("buffered")))
            .await
            .expect("fill the channel");

        let started = std::time::Instant::now();
        let kept = deliver_within(
            &tx,
            Event::default().data("next"),
            7,
            Duration::from_millis(10),
            Duration::from_millis(10),
        )
        .await;
        assert!(
            !kept,
            "a slow client ends the stream rather than buffering forever"
        );
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the close is bounded by the two waits"
        );
    }

    #[test]
    fn the_resync_frame_names_why_and_the_cursor() {
        let frame = ResyncFrame {
            reason: "history_pruned",
            last_event_id: 7,
        };
        let value = serde_json::to_value(&frame).unwrap();
        assert_eq!(value["reason"], "history_pruned");
        assert_eq!(value["last_event_id"], 7);
    }
}
