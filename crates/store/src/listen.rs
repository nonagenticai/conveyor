//! The SSE doorbell: one dedicated DIRECT connection running `LISTEN conveyor_event`.
//!
//! `LISTEN` is session-scoped and PgBouncer here is transaction-pooled, so a LISTEN issued
//! through the pooler would be handed back between statements and never fire (S02 trap 3).
//! The listener therefore connects to `DATABASE_LISTEN_URL`
//! (`postgres://…@db:5439/…`), never to `DATABASE_URL`.
//!
//! The notification is a DOORBELL (S2-D4). The payload is `seq::text`; every subscriber
//! wakes on [`EventListen::subscribe`] and re-queries its OWN tenant scope and topics, so
//! the channel itself carries no tenant, no topic and no content. A missed wake-up is
//! therefore harmless: the re-query uses `seq > last` and catches up.
//!
//! **Startup proves the URL is direct.** A pooled URL CONNECTS successfully — it is a real
//! PostgreSQL client socket — and then never delivers a notification, so a plain connect
//! check cannot detect it. [`probe_direct`] therefore takes two connections from the same
//! URL, `LISTEN`s on one, `NOTIFY`s from the other, and requires the notification within
//! [`PROBE_TIMEOUT`]. A direct connection always passes; a transaction-pooled one fails
//! (PgBouncer hands the LISTENed server connection back between statements), and the
//! process refuses to start naming `DATABASE_LISTEN_URL`.
//!
//! After the probe, the connection is supervised. A dropped LISTEN link is logged and
//! re-established with capped exponential backoff; the probe is a startup check, not a
//! per-reconnect one (re-probing would mask a temporary database outage as a
//! misconfiguration).

use std::time::Duration;

use sqlx::Connection;
use sqlx::postgres::{PgConnection, PgListener};
use tokio::sync::watch;

use crate::outbox::NOTIFY_CHANNEL;

/// The first reconnect delay after a dropped link.
pub const RECONNECT_MIN: Duration = Duration::from_millis(500);

/// The longest reconnect delay.
pub const RECONNECT_MAX: Duration = Duration::from_secs(30);

/// The channel the startup probe uses. Distinct from [`NOTIFY_CHANNEL`] so a real event
/// can never be mistaken for the probe's notification.
pub const PROBE_CHANNEL: &str = "conveyor_listen_probe";

/// How long the probe waits for its own notification.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// Why the LISTEN connection cannot be used.
#[derive(Debug, thiserror::Error)]
pub enum ListenError {
    /// The connection could not be opened or a statement failed.
    #[error("database: {0}")]
    Database(#[from] sqlx::Error),
    /// The URL is reachable but not direct: a two-connection probe delivered nothing.
    #[error(
        "DATABASE_LISTEN_URL is not a direct PostgreSQL connection: a LISTEN on one \
         connection received no NOTIFY from another within two seconds. A transaction-pooled \
         PgBouncer URL connects fine and then silently delivers nothing, leaving SSE dead. \
         Point DATABASE_LISTEN_URL at PostgreSQL directly (compose: db:5439), never at the \
         pooler"
    )]
    NotDirect,
}

/// Prove that `url` is a direct connection by round-tripping one NOTIFY between two
/// connections taken from it.
///
/// Both connections come from `DATABASE_LISTEN_URL` (the reviewer's shape): a URL that can
/// only borrow a session for one statement fails, even though each individual connection
/// succeeds.
pub async fn probe_direct(url: &str) -> Result<(), ListenError> {
    // Connection establishment failures are reported as themselves (an unreachable host is
    // not a pooled URL). Everything AFTER both connections exist is a verdict on whether
    // this URL can carry a session: a pooler hands the LISTENed backend back and the
    // exchange either times out or poisons the notifier with an unexpected
    // `NotificationResponse`.
    let mut listening = PgListener::connect(url).await?;
    let mut notifier = PgConnection::connect(url).await?;

    match exchange(&mut listening, &mut notifier).await {
        Ok(true) => {
            let _ = notifier.close().await;
            Ok(())
        }
        Ok(false) => {
            tracing::warn!(
                channel = PROBE_CHANNEL,
                "the LISTEN probe timed out: no NOTIFY crossed between two connections from \
                 DATABASE_LISTEN_URL"
            );
            Err(ListenError::NotDirect)
        }
        Err(error) => Err(classify(error)),
    }
}

/// One LISTEN on `listening`, one NOTIFY from `notifier`, wait at most [`PROBE_TIMEOUT`].
async fn exchange(
    listening: &mut PgListener,
    notifier: &mut PgConnection,
) -> Result<bool, sqlx::Error> {
    listening.listen(PROBE_CHANNEL).await?;
    sqlx::query("SELECT pg_notify($1, $2)")
        .bind(PROBE_CHANNEL)
        .bind("probe")
        .execute(&mut *notifier)
        .await?;

    let deadline = tokio::time::Instant::now() + PROBE_TIMEOUT;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return Ok(false);
        }
        match tokio::time::timeout(remaining, listening.recv()).await {
            Ok(Ok(notification)) if notification.payload() == "probe" => return Ok(true),
            // A notification on another channel is not ours; keep waiting.
            Ok(Ok(_)) => continue,
            Ok(Err(error)) => return Err(error),
            Err(_) => return Ok(false),
        }
    }
}

/// A real server/transport failure stays a [`ListenError::Database`]; a protocol error is
/// the pooler signature and becomes [`ListenError::NotDirect`] (with the raw error logged).
fn classify(error: sqlx::Error) -> ListenError {
    let transport = matches!(&error, sqlx::Error::Database(_) | sqlx::Error::Io(_));
    if transport {
        ListenError::Database(error)
    } else {
        tracing::warn!(%error, "the LISTEN probe saw a non-session connection; treating it as pooled");
        ListenError::NotDirect
    }
}

/// A running LISTEN connection and the latest `seq` it has announced.
///
/// Cloned cheaply: the watcher handle is shared, and one connection serves every SSE
/// subscriber in the process.
#[derive(Clone, Debug)]
pub struct EventListen {
    latest: watch::Sender<i64>,
}

impl EventListen {
    /// Prove the URL is direct, then connect, `LISTEN`, and start the supervisor.
    pub async fn connect(url: &str) -> Result<Self, ListenError> {
        probe_direct(url).await?;
        let mut listener = PgListener::connect(url).await?;
        listener.listen(NOTIFY_CHANNEL).await?;
        let (latest, _) = watch::channel(0);
        tokio::spawn(supervise(listener, url.to_owned(), latest.clone()));
        tracing::info!(channel = NOTIFY_CHANNEL, "listening for outbox doorbells");
        Ok(Self { latest })
    }

    /// Wake on every announced `seq`. A receiver that starts after a notification simply
    /// re-queries from its own cursor, so a missed doorbell costs nothing.
    pub fn subscribe(&self) -> watch::Receiver<i64> {
        self.latest.subscribe()
    }

    /// The highest `seq` announced so far in this process.
    pub fn latest(&self) -> i64 {
        *self.latest.borrow()
    }
}

/// Receive forever; on a dropped link, reconnect with backoff and keep the same watch
/// channel, so subscribers never see the outage as "no more events".
async fn supervise(mut listener: PgListener, url: String, latest: watch::Sender<i64>) {
    loop {
        loop {
            match listener.recv().await {
                Ok(notification) => {
                    // A malformed payload is a bug in the writer, not a reason to drop the
                    // link; log it and keep listening.
                    match notification.payload().parse::<i64>() {
                        Ok(seq) => {
                            let _ = latest.send(seq);
                        }
                        Err(error) => {
                            tracing::warn!(
                                payload = notification.payload(),
                                %error,
                                "ignoring a non-numeric conveyor_event payload"
                            );
                        }
                    }
                }
                Err(error) => {
                    tracing::warn!(%error, "the LISTEN connection dropped; reconnecting");
                    break;
                }
            }
        }

        let mut delay = RECONNECT_MIN;
        loop {
            tokio::time::sleep(delay).await;
            match PgListener::connect(&url).await {
                Ok(mut fresh) => match fresh.listen(NOTIFY_CHANNEL).await {
                    Ok(()) => {
                        tracing::info!("LISTEN connection re-established");
                        listener = fresh;
                        break;
                    }
                    Err(error) => {
                        tracing::warn!(%error, "re-LISTEN failed");
                    }
                },
                Err(error) => {
                    tracing::warn!(%error, "LISTEN reconnect failed");
                }
            }
            delay = (delay * 2).min(RECONNECT_MAX);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_backoff_is_capped() {
        let mut delay = RECONNECT_MIN;
        for _ in 0..12 {
            delay = (delay * 2).min(RECONNECT_MAX);
        }
        assert_eq!(delay, RECONNECT_MAX);
        assert!(RECONNECT_MIN < RECONNECT_MAX);
    }
}
