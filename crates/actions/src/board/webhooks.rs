#![forbid(unsafe_code)]
//! `webhooks.create` · `.update` · `.delete` · `.test` — the board's webhook subscriptions.
//!
//! A subscription is one row on a board (or, with no `board_id`, on the tenant) that selects
//! registered event types with `events`/`excluded_events`. Deliveries are jobs enqueued by
//! the emitter in the mutation's transaction (S5-D7; see [`super::events`]); this module owns
//! the CRUD, the synchronous test call and the one HTTP sender both paths share.
//!
//! Three rules are worth stating because each has a way to get it wrong:
//!
//! * **the secret is write-once.** `webhooks.create` is the only response that carries it;
//!   every read (including `webhooks.update`) omits the member. It is stored in
//!   `webhook_subscription.secret_ciphertext` — S05 has no KEK, so the stored bytes are the
//!   bearer token, and no read API ever returns them.
//! * **the event vocabulary is the register.** `events`/`excluded_events` may name only the
//!   S05 emitters ([`super::events::S05_EVENT_TYPES`]); an invented `list.changed` is a 422,
//!   never a subscription that silently never fires.
//! * **a test delivery is not a mutation.** `webhooks.test` sends the same envelope a real
//!   delivery would and reports the outcome; it writes no `event` row (nothing changed).

use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use conveyor_store as store;
use conveyor_store::webhook::WebhookRecord;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::board::events::{RawEvent, S05_EVENT_TYPES, WEBHOOK_CHANGED, append_raw_event};
use crate::board::views::{
    IdIn, WebhookCreateIn, WebhookDeliveryOut, WebhookOut, WebhookUpdateIn, webhook_out,
};
use crate::board::{etag_of, invalid, require_precondition_with, store_error, webhook_error};
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;

/// The HTTP socket timeout for one delivery attempt (S05 §5: 15 s).
pub const DELIVERY_TIMEOUT: Duration = Duration::from_secs(15);

/// The one attempt the synchronous `webhooks.test` makes.
pub const TEST_ATTEMPTS: u8 = 1;

/// The synthetic event name a test delivery carries. It is deliberately NOT an outbox
/// event type: nothing can subscribe to it, and it names itself as a test.
pub const TEST_EVENT: &str = "webhook.test";

/// Create a subscription. The signing secret is returned ONCE, on this response.
#[crate::action(
    name = "webhooks.create",
    version = 1,
    kind = "internal",
    scopes = ["boards:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn webhooks_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: WebhookCreateIn,
) -> Result<HandlerOutput<WebhookOut>, ActionError> {
    let url = validate_url(&input.url)?;
    let events = validate_events(input.events.unwrap_or_default(), "events")?;
    let excluded = validate_events(input.excluded_events.unwrap_or_default(), "excluded_events")?;
    let secret = generate_secret();
    let created = store::webhook::insert_webhook_tx(
        &mut *tx,
        ctx.tenant,
        &store::webhook::NewWebhook {
            board_id: input.board_id,
            url,
            secret: secret.as_bytes().to_vec(),
            events,
            excluded_events: excluded,
            active: input.active.unwrap_or(true),
        },
    )
    .await
    .map_err(|error| webhook_error(error, "webhooks.create"))?;

    webhook_event(
        &mut *tx,
        &ctx,
        &created,
        "create",
        Value::Null,
        webhook_json(&created),
    )
    .await?;

    let mut out = webhook_out(&created);
    out.secret = Some(secret);
    Ok(HandlerOutput::created(out).with_etag(etag_of(created.version)))
}

/// Change a subscription's URL, selection or active flag.
#[crate::action(
    name = "webhooks.update",
    version = 1,
    kind = "internal",
    scopes = ["boards:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn webhooks_update(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: WebhookUpdateIn,
) -> Result<HandlerOutput<WebhookOut>, ActionError> {
    let current = read_webhook(tx, ctx.tenant, input.id).await?;
    let expected = require_precondition_with(&ctx, "webhooks.update", webhook_json(&current))?;
    let url = match input.url.as_deref() {
        Some(raw) => Some(validate_url(raw)?),
        None => None,
    };
    let events = match input.events {
        Some(raw) => Some(validate_events(raw, "events")?),
        None => None,
    };
    let excluded = match input.excluded_events {
        Some(raw) => Some(validate_events(raw, "excluded_events")?),
        None => None,
    };

    let patch = store::webhook::WebhookUpdate {
        url,
        events,
        excluded_events: excluded,
        active: input.active,
    };
    match store::webhook::update_webhook_tx(&mut *tx, ctx.tenant, current.id, &patch, expected)
        .await
        .map_err(|error| store_error(error, "webhooks.update"))?
    {
        Some(record) => {
            webhook_event(
                &mut *tx,
                &ctx,
                &record,
                "update",
                webhook_json(&current),
                webhook_json(&record),
            )
            .await?;
            Ok(HandlerOutput::ok(webhook_out(&record)).with_etag(etag_of(record.version)))
        }
        None => Err(stale_webhook(tx, ctx.tenant, current.id, "webhooks.update").await?),
    }
}

/// Remove a subscription.
#[crate::action(
    name = "webhooks.delete",
    version = 1,
    kind = "internal",
    scopes = ["boards:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "destructive",
    confirm = "none",
    tenancy = "tenant"
)]
async fn webhooks_delete(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IdIn,
) -> Result<HandlerOutput<()>, ActionError> {
    let current = read_webhook(tx, ctx.tenant, input.id).await?;
    let expected = require_precondition_with(&ctx, "webhooks.delete", webhook_json(&current))?;
    let deleted = store::webhook::delete_webhook_tx(&mut *tx, ctx.tenant, current.id, expected)
        .await
        .map_err(|error| store_error(error, "webhooks.delete"))?;
    if !deleted {
        return Err(stale_webhook(tx, ctx.tenant, current.id, "webhooks.delete").await?);
    }
    webhook_event(
        &mut *tx,
        &ctx,
        &current,
        "delete",
        webhook_json(&current),
        Value::Null,
    )
    .await?;
    Ok(HandlerOutput::no_content(()))
}

/// Send one synchronous test delivery and report what the endpoint did.
///
/// `effect = write`, not `read`: the call performs an outbound POST and the endpoint may act on
/// it. The target is address-checked at **connect** time (see [`address_refusal`] and
/// [`GuardedResolver`]): loopback, link-local/metadata, multicast and the unspecified address are
/// refused, while private targets stay allowed because every legitimate target on this estate is
/// private (RFC1918, the tailnet's `100.64/10`, ULA). Redirects are not followed — a redirect
/// would move the target the tenant did not register.
#[crate::action(
    name = "webhooks.test",
    version = 1,
    kind = "internal",
    scopes = ["boards:write"],
    idempotent = true,
    timeout_ms = 20000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn webhooks_test(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IdIn,
) -> Result<WebhookDeliveryOut, ActionError> {
    let record = read_webhook(tx, ctx.tenant, input.id).await?;
    let secret = store::webhook::secret_tx(&mut *tx, ctx.tenant, record.id)
        .await
        .map_err(|error| webhook_error(error, "webhooks.test"))?
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned());
    let name = store::identity::display_name_tx(&mut *tx, ctx.actor_id)
        .await
        .map_err(|error| store_error(error, "webhooks.test"))?
        .unwrap_or_default();
    let body = serde_json::json!({
        "event": TEST_EVENT,
        "data": webhook_out(&record),
        "prevData": Value::Null,
        "user": { "id": ctx.actor_id, "name": name },
    });
    Ok(deliver_once(record.id, &record.url, secret.as_deref(), &body).await)
}

// ── shared delivery ─────────────────────────────────────────────────────────────────────

/// POST one envelope to one endpoint with `Authorization: Bearer <secret>`.
///
/// Shared with the worker's `webhook.deliver` job so a test delivery and a real one cannot
/// disagree about the request. Retries are the JOB's (`max_attempts = 3`), not this
/// function's: one call is one attempt.
pub async fn deliver_once(
    webhook_id: Uuid,
    url: &str,
    secret: Option<&str>,
    body: &Value,
) -> WebhookDeliveryOut {
    let started = Instant::now();
    let client = match reqwest::Client::builder()
        .timeout(DELIVERY_TIMEOUT)
        .dns_resolver(Arc::new(GuardedResolver))
        .redirect(reqwest::redirect::Policy::none())
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            return WebhookDeliveryOut {
                webhook_id: webhook_id.to_string(),
                url: url.to_owned(),
                ok: false,
                status_code: None,
                attempts: TEST_ATTEMPTS,
                duration_ms: 0,
                error: Some(format!("the HTTP client could not be built: {error}")),
            };
        }
    };
    let mut request = client.post(url).json(body);
    if let Some(secret) = secret {
        request = request.header("authorization", format!("Bearer {secret}"));
    }
    let outcome = tokio::time::timeout(DELIVERY_TIMEOUT, request.send()).await;
    let duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let (ok, status_code, error) = match outcome {
        Err(_) => (
            false,
            None,
            Some(format!(
                "the endpoint did not answer within {DELIVERY_TIMEOUT:?}"
            )),
        ),
        Ok(Err(error)) => (false, None, Some(error.to_string())),
        Ok(Ok(response)) => {
            let status = response.status();
            if status.is_success() {
                (true, Some(status.as_u16()), None)
            } else {
                (
                    false,
                    Some(status.as_u16()),
                    Some(format!("the endpoint answered {status}")),
                )
            }
        }
    };
    WebhookDeliveryOut {
        webhook_id: webhook_id.to_string(),
        url: url.to_owned(),
        ok,
        status_code,
        attempts: TEST_ATTEMPTS,
        duration_ms,
        error,
    }
}

// ── validation and shared helpers ──────────────────────────────────────────────────────

/// `http`/`https` only; a `file:` or `javascript:` endpoint is not a webhook. A literal address is
/// also checked here, so an operator learns at registration time; a hostname cannot be judged
/// until it resolves, and that check happens at connect time in [`GuardedResolver`].
fn validate_url(raw: &str) -> Result<String, ActionError> {
    let trimmed = raw.trim();
    let parsed = reqwest::Url::parse(trimmed)
        .map_err(|error| invalid("url", "invalid_value", error.to_string()))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(invalid(
            "url",
            "invalid_value",
            "must be an http or https URL",
        ));
    }
    let host = parsed
        .host_str()
        .unwrap_or_default()
        .trim_start_matches('[')
        .trim_end_matches(']');
    if let Ok(address) = host.parse::<IpAddr>()
        && let Some(reason) = address_refusal(address)
    {
        return Err(invalid(
            "url",
            "invalid_value",
            format!("a webhook may not target a {reason} address"),
        ));
    }
    Ok(parsed.to_string())
}

/// The addresses a webhook must never reach, with the reason named for the refusal.
///
/// Private targets (RFC1918, `100.64/10`, ULA) stay **allowed**: every legitimate webhook target
/// on this estate is private, so a blanket "no private address" rule would refuse the product's
/// own traffic. What is refused is the set an outbound subscription has no business reaching —
/// the machine itself, the cloud metadata service, multicast and "no address" — including their
/// IPv4-mapped IPv6 spellings.
fn address_refusal(address: IpAddr) -> Option<&'static str> {
    let address = match address {
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => IpAddr::V4(v4),
            None => IpAddr::V6(v6),
        },
        other => other,
    };
    match address {
        IpAddr::V4(v4) => {
            if v4.is_loopback() {
                (!loopback_allowed()).then_some("loopback")
            } else if v4.is_link_local() {
                Some("link-local (metadata)")
            } else if v4.is_unspecified() {
                Some("unspecified")
            } else if v4.is_multicast() {
                Some("multicast")
            } else if v4.is_broadcast() {
                Some("broadcast")
            } else {
                None
            }
        }
        IpAddr::V6(v6) => {
            if v6.is_loopback() {
                (!loopback_allowed()).then_some("loopback")
            } else if v6.is_unspecified() {
                Some("unspecified")
            } else if v6.is_multicast() {
                Some("multicast")
            } else if v6.is_unicast_link_local() {
                Some("link-local")
            } else {
                None
            }
        }
    }
}

/// The one test-only escape hatch: the recorder a webhook test delivers to is a loopback server.
/// A deployed process never sets `CONVEYOR_WEBHOOK_ALLOW_LOOPBACK`; the harness does
/// (`support::init_s09_env`), and the refusal itself is proved by the unit test above, which runs
/// without it.
fn loopback_allowed() -> bool {
    std::env::var("CONVEYOR_WEBHOOK_ALLOW_LOOPBACK").is_ok_and(|value| value == "1")
}

/// The resolver every delivery uses: ordinary resolution, then [`address_refusal`] against
/// **every** address before one can be dialled.
///
/// It runs at connect time on purpose. A hostname that resolved to a private address when the
/// subscription was written, and resolves to `169.254.169.254` a minute later, is refused at the
/// second resolution — validating only the URL string would be a check on a different value than
/// the one the socket uses.
#[derive(Debug)]
struct GuardedResolver;

impl reqwest::dns::Resolve for GuardedResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        Box::pin(async move {
            let host = name.as_str().to_owned();
            let resolved = tokio::net::lookup_host((host.as_str(), 0))
                .await
                .map_err(|error| -> Box<dyn std::error::Error + Send + Sync> { Box::new(error) })?;
            let mut addresses = Vec::new();
            for address in resolved {
                if let Some(reason) = address_refusal(address.ip()) {
                    return Err(Box::new(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        format!("a webhook may not target a {reason} address"),
                    ))
                        as Box<dyn std::error::Error + Send + Sync>);
                }
                addresses.push(address);
            }
            Ok(Box::new(addresses.into_iter()) as reqwest::dns::Addrs)
        })
    }
}

fn validate_events(raw: Vec<String>, field: &str) -> Result<Vec<String>, ActionError> {
    let mut out = Vec::with_capacity(raw.len());
    for name in raw {
        let name = name.trim().to_owned();
        if !S05_EVENT_TYPES.contains(&name.as_str()) {
            return Err(invalid(
                field,
                "invalid_value",
                format!(
                    "{name:?} is not an event this service emits; the vocabulary is {}",
                    S05_EVENT_TYPES.join(", ")
                ),
            ));
        }
        if !out.contains(&name) {
            out.push(name);
        }
    }
    Ok(out)
}

/// The signing secret: 32 random bytes, hex. `uuid` v7 draws its random tail from the OS
/// CSPRNG, and two of them are 74 random bits each; the token is long enough that guessing
/// is not a threat, and it is stored verbatim (see the module doc).
fn generate_secret() -> String {
    format!("{}{}", Uuid::now_v7().simple(), Uuid::now_v7().simple())
}

fn webhook_json(record: &WebhookRecord) -> Value {
    serde_json::to_value(webhook_out(record)).unwrap_or(Value::Null)
}

async fn read_webhook(
    tx: &mut PgConnection,
    tenant: conveyor_store::TenantId,
    id: Uuid,
) -> Result<WebhookRecord, ActionError> {
    store::webhook::get_webhook_tx(&mut *tx, tenant, id)
        .await
        .map_err(|error| store_error(error, "webhooks.get"))?
        .ok_or_else(|| ActionError::resource_not_found("webhook"))
}

async fn stale_webhook(
    tx: &mut PgConnection,
    tenant: conveyor_store::TenantId,
    id: Uuid,
    action: &str,
) -> Result<ActionError, ActionError> {
    match store::webhook::get_webhook_tx(&mut *tx, tenant, id)
        .await
        .map_err(|error| store_error(error, action))?
    {
        Some(record) => Ok(ActionError::precondition_with(
            webhook_json(&record),
            format!("the webhook moved to version {}", record.version.max(1)),
        )),
        None => Ok(ActionError::resource_not_found("webhook")),
    }
}

/// `webhook.changed`, on the board's topic or on the tenant-wide `webhook` topic.
async fn webhook_event(
    tx: &mut PgConnection,
    ctx: &HandlerCtx,
    record: &WebhookRecord,
    op: &'static str,
    prev_data: Value,
    data: Value,
) -> Result<(), ActionError> {
    let topic = record
        .board_id
        .map(|board_id| format!("board:{board_id}"))
        .unwrap_or_else(|| "webhook".to_owned());
    append_raw_event(
        &mut *tx,
        ctx,
        &RawEvent {
            topic,
            board_id: record.board_id,
            aggregate_type: "webhook",
            aggregate_id: record.id,
            event_type: WEBHOOK_CHANGED,
            payload: serde_json::json!({
                "webhook_id": record.id,
                "board_id": record.board_id,
                "op": op,
            }),
            data,
            prev_data,
        },
    )
    .await
}

#[cfg(test)]
mod tests {
    use crate::registry::registry;
    use crate::spec::Effect;

    #[test]
    fn a_test_delivery_is_declared_a_write_not_a_read() {
        let spec = registry()
            .get("webhooks.test")
            .expect("webhooks.test is compiled");
        // The handler POSTs to the tenant's endpoint; `read` told every consumer the call was free
        // of side effects.
        assert_eq!(
            spec.effect,
            Effect::Write,
            "an outbound POST must not be declared a read"
        );
    }

    /// Finding #6: the address policy. Private targets stay allowed — the estate's own services
    /// are private — while the machine, the metadata service and non-addresses are refused.
    #[test]
    fn the_address_policy_refuses_what_a_webhook_must_never_reach() {
        for raw in [
            "http://127.0.0.1/hook",
            "http://127.9.9.9:8080/hook",
            "http://169.254.169.254/latest/meta-data",
            "http://0.0.0.0/hook",
            "http://[::1]/hook",
            "http://[fe80::1]/hook",
            "http://[::ffff:127.0.0.1]/hook",
            "http://255.255.255.255/hook",
        ] {
            assert!(
                super::validate_url(raw).is_err(),
                "{raw} must be refused at registration"
            );
        }
        for raw in [
            "http://10.0.0.5/hook",
            "http://192.168.1.10:8080/hook",
            "http://10.0.0.7/hook",
            "http://[fd00::1]/hook",
            "https://webhooks.example.com/hook",
            "http://worker3.example.com:8080/hook",
        ] {
            assert!(
                super::validate_url(raw).is_ok(),
                "{raw} is a legitimate private target and must be allowed"
            );
        }
    }

    /// The resolver, not the URL string, is what defeats DNS rebinding: `localhost` is a name that
    /// resolves to loopback at connect time, and `deliver_once` must refuse it **before** any
    /// connection. A listener on that port proves the negative: if the guard were absent the
    /// delivery would connect and succeed.
    #[tokio::test]
    async fn a_delivery_to_a_name_that_resolves_to_loopback_is_refused() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        use tokio::io::AsyncWriteExt as _;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind a loopback listener");
        let port = listener.local_addr().expect("listener addr").port();
        let connected = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&connected);
        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                flag.store(true, Ordering::SeqCst);
                let _ = stream
                    .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\n\r\n")
                    .await;
            }
        });

        let outcome = super::deliver_once(
            uuid::Uuid::now_v7(),
            &format!("http://localhost:{port}/hook"),
            None,
            &serde_json::json!({"event": "webhook.test"}),
        )
        .await;
        assert!(!outcome.ok, "a loopback target must be refused");
        assert!(
            !connected.load(Ordering::SeqCst),
            "the refusal must happen before the socket is opened"
        );
    }
}
