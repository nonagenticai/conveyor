#![forbid(unsafe_code)]
//! The `ops.host_*` family — bare-metal host control through the per-host agent (T7,
//! SPEC_V2 §10.4.1).
//!
//! | action | effect | what it does |
//! |---|---|---|
//! | `ops.host_status` | read | the unit's state on a registered host |
//! | `ops.host_logs` | read | a bounded tail of the unit's log |
//! | `ops.host_restart_unit` | **destructive** | restart a unit behind an approved approval |
//!
//! **The destination is the registry's.** The `host` input names a row in `host_registry`; the
//! row supplies the address, the account and the pinned host key. No input carries a URL, an
//! address or a command, and the `unit` must be on both the row's allowlist (checked here) and
//! the host's own agent config (checked there) — two refusals from two authorities.
//!
//! **The credential boundary is the agent's.** The conveyor holds no host credential; it holds a
//! key whose `authorized_keys` line pins it to the agent binary, and the agent answers three
//! verbs. The restart is gated exactly as the cluster writes are: `require_approval_tx` runs
//! before any session is opened, and a `dry_run` changes nothing.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgConnection, Row};
use uuid::Uuid;

use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;

use super::{CheckOut, SERVICE, name_for_target, name_of, tail_of};

/// `ops.host_status` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HostIn {
    /// The registered host's name.
    pub host: String,
    /// The unit, as the host's agent config names it.
    pub unit: String,
}

/// `ops.host_logs` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HostLogsIn {
    /// The registered host's name.
    pub host: String,
    /// The unit, as the host's agent config names it.
    pub unit: String,
    /// How many trailing lines (default 200, max 2000).
    #[serde(default)]
    pub lines: Option<u32>,
}

/// `ops.host_restart_unit` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HostRestartIn {
    /// The registered host's name.
    pub host: String,
    /// The unit, as the host's agent config names it.
    pub unit: String,
    /// Return the request that WOULD be made and open no session.
    #[serde(default)]
    pub dry_run: bool,
}

/// `ops.host_status` output.
#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct HostStatusOut {
    /// The host read.
    pub host: String,
    /// The unit read.
    pub unit: String,
    /// `launchctl` or `systemctl`.
    pub mechanism: String,
    /// The state word the host reported (`running`, `active`, …).
    pub state: String,
    /// One line of evidence (a pid, a timestamp).
    pub detail: String,
}

/// `ops.host_logs` output.
#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct HostLogsOut {
    /// The host read.
    pub host: String,
    /// The unit read.
    pub unit: String,
    /// The mechanism that answered.
    pub mechanism: String,
    /// The tail that was requested.
    pub lines: u32,
    /// The log text, as the host returned it.
    pub log: String,
}

/// `ops.host_restart_unit` output.
#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct HostRestartOut {
    /// The host restarted.
    pub host: String,
    /// The unit restarted.
    pub unit: String,
    /// `launchctl` or `systemctl`.
    pub mechanism: String,
    /// The exact thing the approval authorised.
    pub target: String,
    /// The approval that authorised the call.
    pub approval_id: Option<Uuid>,
    /// Whether anything was restarted.
    pub dry_run: bool,
    /// The remote command the agent was (or would be) asked to answer.
    pub remote_command: Vec<String>,
    /// The read-back observations.
    pub checks: Vec<CheckOut>,
}

/// One registered, enabled host.
struct RegisteredHost {
    address: String,
    agent_user: String,
    host_key: String,
    mechanism: String,
    units: Vec<String>,
}

impl RegisteredHost {
    fn target(&self, name: &str) -> conveyor_host::HostTarget {
        conveyor_host::HostTarget {
            name: name.to_owned(),
            address: self.address.clone(),
            agent_user: self.agent_user.clone(),
            host_key: self.host_key.clone(),
        }
    }
}

/// The registry lookup: one enabled row by name, or a typed refusal.
async fn registered(tx: &mut PgConnection, host: &str) -> Result<RegisteredHost, ActionError> {
    let row = sqlx::query(
        "SELECT address, agent_user, host_key, mechanism, units \
         FROM host_registry WHERE name = $1 AND enabled",
    )
    .bind(host)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| {
        tracing::error!(%error, "read the host registry");
        ActionError::internal()
    })?;
    let row = row.ok_or_else(|| {
        ActionError::new(
            404,
            "host_not_registered",
            format!("{host:?} is not a registered host"),
        )
    })?;
    Ok(RegisteredHost {
        address: row.get("address"),
        agent_user: row.get("agent_user"),
        host_key: row.get("host_key"),
        mechanism: row.get("mechanism"),
        units: row.get("units"),
    })
}

/// The unit must be on the row's allowlist as well as the agent's — the registry is the action
/// layer's authority, and the agent's config is the host's.
fn allowlisted<'a>(
    host: &str,
    row: &RegisteredHost,
    unit: &'a str,
) -> Result<&'a str, ActionError> {
    if row.units.iter().any(|allowed| allowed == unit) {
        return Ok(unit);
    }
    Err(ActionError::new(
        404,
        "unit_not_allowlisted",
        format!("{unit:?} is not on {host:?}'s allowlist"),
    ))
}

/// Build the client, refusing visibly when the process holds no key.
fn client() -> Result<conveyor_host::Client, ActionError> {
    conveyor_host::Client::from_env().map_err(refusal)
}

/// Map a client failure onto the action surface: `503` unconfigured, the agent's own refusal as
/// a `403` when it is an authorisation refusal and `409` for any other refusal, `502` for a
/// transport or backend failure.
fn refusal(error: conveyor_host::HostError) -> ActionError {
    match error {
        conveyor_host::HostError::Unconfigured => ActionError::unconfigured(SERVICE),
        conveyor_host::HostError::Refused { code, message } => {
            let status = if code.contains("not_allowed") || code == "shell_refused" {
                403
            } else {
                409
            };
            // The code is the agent's, so it is carried in the detail rather than invented as a
            // second slug the contract would have to keep in sync.
            ActionError::new(status, "host-agent-refused", format!("{code}: {message}"))
        }
        other => ActionError::new(502, "host-unavailable", other.to_string()),
    }
}

/// The unit's state on a registered host.
#[crate::action(
    name = "ops.host_status",
    version = 1,
    kind = "internal",
    scopes = ["integrations:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant",
    agent_tier = "searchable"
)]
async fn ops_host_status(
    tx: &mut PgConnection,
    _ctx: HandlerCtx,
    input: HostIn,
) -> Result<HandlerOutput<HostStatusOut>, ActionError> {
    let host = name_of("host", &input.host)?;
    let unit = name_of("unit", &input.unit)?;
    let row = registered(tx, &host).await?;
    let unit = allowlisted(&host, &row, &unit)?;
    let client = client()?;
    let body: Value = client
        .status(&row.target(&host), unit)
        .await
        .map_err(refusal)?;
    Ok(HandlerOutput::ok(HostStatusOut {
        host,
        unit: unit.to_owned(),
        mechanism: row.mechanism,
        state: text(&body, "state"),
        detail: text(&body, "detail"),
    }))
}

/// A bounded tail of the unit's log.
#[crate::action(
    name = "ops.host_logs",
    version = 1,
    kind = "internal",
    scopes = ["integrations:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant",
    agent_tier = "searchable"
)]
async fn ops_host_logs(
    tx: &mut PgConnection,
    _ctx: HandlerCtx,
    input: HostLogsIn,
) -> Result<HandlerOutput<HostLogsOut>, ActionError> {
    let host = name_of("host", &input.host)?;
    let unit = name_of("unit", &input.unit)?;
    let lines = tail_of(input.lines);
    let row = registered(tx, &host).await?;
    let unit = allowlisted(&host, &row, &unit)?;
    let client = client()?;
    let body: Value = client
        .logs(&row.target(&host), unit, lines)
        .await
        .map_err(refusal)?;
    Ok(HandlerOutput::ok(HostLogsOut {
        host,
        unit: unit.to_owned(),
        mechanism: row.mechanism,
        lines,
        log: text(&body, "log"),
    }))
}

/// `ops.host_restart_unit`'s approval target: this unit on this host, restarting (#282).
///
/// Built from the same two names the handler uses, so the approval the engine demands and the
/// host the handler addresses cannot disagree.
fn target_host_restart(input: &HostRestartIn) -> Result<String, String> {
    let host = name_for_target("host", &input.host)?;
    let unit = name_for_target("unit", &input.unit)?;
    Ok(format!("host/{host}/unit/{unit}/restart"))
}

/// Restart a unit on a registered host — destructive, and gated exactly as the cluster writes
/// are. The approval names the exact unit: `host/<host>/unit/<unit>/restart`.
#[crate::action(
    name = "ops.host_restart_unit",
    version = 1,
    kind = "internal",
    scopes = ["ops:write"],
    idempotent = false,
    timeout_ms = 60000,
    effect = "destructive",
    confirm = "none",
    tenancy = "tenant",
    approval_target = "target_host_restart"
)]
async fn ops_host_restart_unit(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: HostRestartIn,
) -> Result<HandlerOutput<HostRestartOut>, ActionError> {
    let host = name_of("host", &input.host)?;
    let unit = name_of("unit", &input.unit)?;
    // #282: the engine gated this call with `target_host_restart` before this handler ran — on a
    // DETERMINISTIC target, before the registry is read and long before any session exists. A dry
    // run is gated too, and the refusal is independent of whether the host exists.
    let target = format!("host/{host}/unit/{unit}/restart");
    let row = registered(tx, &host).await?;
    let unit = allowlisted(&host, &row, &unit)?;
    let remote_command = vec!["restart".to_owned(), unit.to_owned()];
    let mut checks = Vec::new();
    if input.dry_run {
        checks.push(CheckOut {
            name: "dry_run".to_owned(),
            ok: true,
            detail: format!(
                "no session was opened; the agent would be asked `{}`",
                remote_command.join(" ")
            ),
        });
    } else {
        let client = client()?;
        let body: Value = client
            .restart(&row.target(&host), unit)
            .await
            .map_err(refusal)?;
        let state = text(&body, "state");
        checks.push(CheckOut {
            name: "unit_state_after_restart".to_owned(),
            ok: running(&state),
            detail: format!("{unit} reports state {state:?}"),
        });
    }
    Ok(HandlerOutput::ok(HostRestartOut {
        host,
        unit: unit.to_owned(),
        mechanism: row.mechanism,
        target,
        approval_id: ctx.approval_id,
        dry_run: input.dry_run,
        remote_command,
        checks,
    }))
}

/// `launchctl` says `running`; `systemctl` says `active` or `active (running)`.
fn running(state: &str) -> bool {
    state == "running" || state.starts_with("active")
}

/// One string field of the agent's answer, or the empty string when it did not send one.
fn text(body: &Value, key: &str) -> String {
    body.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}
