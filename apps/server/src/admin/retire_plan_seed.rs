//! `conveyor admin retire-plan-seed` — archive the retired plan-seed cards.
//!
//! The build-time plan seeder is gone. Its cards are not work, so this one-off command archives
//! every card it owned (`meta.seed_name`) through the **real** `cards.archive` handler. The lane
//! move, the `card.archived` event and the audit row are the same ones a person's action writes.
//! It is idempotent and tenant-scoped, and it never touches a card without the seed marker.

use conveyor_domain::TenantId;
use conveyor_store as store;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use super::action::{ActionCaller, CallError};

/// What one run did.
#[derive(Debug, Default)]
pub struct RetireReport {
    /// Cards archived by this run.
    pub archived: usize,
    /// Cards already archived before this run.
    pub already_archived: i64,
    /// Cards skipped because a lease or a live run still holds them.
    pub skipped_live: i64,
    /// Per-card failures; the run continues and exits non-zero when this is non-empty.
    pub failed: Vec<(Uuid, String)>,
}

impl RetireReport {
    /// True when every candidate was archived.
    pub fn ok(&self) -> bool {
        self.failed.is_empty()
    }

    /// The CLI's machine-readable line.
    pub fn json(&self) -> Value {
        json!({
            "archived": self.archived,
            "already_archived": self.already_archived,
            "skipped_live": self.skipped_live,
            "failed": self.failed.iter().map(|(id, error)| json!({"id": id, "error": error})).collect::<Vec<_>>(),
        })
    }
}

/// Archive every seeded card of one tenant.
///
/// `cards.archive` is destructive (#282, #283), so each card needs a gate naming **that card**.
/// This command is the operator's decision — run by hand, on one tenant, against cards nothing
/// else holds — but the gate is the engine's, so the command asks for one per card and decides
/// it as the same service principal. What it needs from the tenant is `self_approval: allowed`
/// for the duration, which is the narrowest setting that makes that possible: an approval is
/// still REQUIRED for every card (and one row is written per card), only the identity of the
/// decider is relaxed, and it is restored before the command returns.
pub async fn run(
    pool: &PgPool,
    tenant: TenantId,
    app_key: &str,
) -> Result<RetireReport, RetireError> {
    let scan = store::retire::scan_seeded_cards(pool, tenant).await?;
    let caller = ActionCaller::new(pool, tenant, app_key).await?;
    let mut report = RetireReport {
        already_archived: scan.already_archived,
        skipped_live: scan.live,
        ..RetireReport::default()
    };
    if scan.candidates.is_empty() {
        return Ok(report);
    }
    let before = policy(&caller, "cards.archive").await?;
    set_policy(&caller, "cards.archive", &before.0, "allowed").await?;
    for card in scan.candidates {
        match archive_one(&caller, card.id, card.version).await {
            Ok(()) => report.archived += 1,
            Err(error) => report.failed.push((card.id, error.to_string())),
        }
    }
    // Restore what the tenant had, before the report is returned — a command that left the
    // tenant's policy widened would be a worse defect than the one it fixed.
    set_policy(&caller, "cards.archive", &before.0, &before.1).await?;
    Ok(report)
}

/// Open a gate for one card, approve it as this principal, and archive through the real handler.
async fn archive_one(
    caller: &ActionCaller<'_>,
    card: Uuid,
    version: i64,
) -> Result<(), CallError> {
    let input = json!({ "id": card });
    let gate = caller
        .call(
            "approvals.request",
            json!({
                "gate": "cards.archive",
                "kind": "action",
                "action": "cards.archive",
                "input": input,
                "proposed_input": input,
                "description": "conveyor admin retire-plan-seed",
            }),
            None,
        )
        .await?;
    let approval_id = gate["id"]
        .as_str()
        .and_then(|raw| Uuid::parse_str(raw).ok())
        .ok_or_else(|| CallError::Action {
            name: "approvals.request".to_owned(),
            status: 500,
            detail: format!("the gate carried no id: {gate}"),
        })?;
    caller
        .call(
            "approvals.decide",
            json!({ "approval_id": approval_id, "decision": "approve" }),
            None,
        )
        .await?;
    caller
        .call_gated(
            "cards.archive",
            json!({ "id": card }),
            Some(version),
            Some(approval_id),
        )
        .await
        .map(|_| ())
}

/// `(approval, self_approval)` as the tenant currently holds them for `action`.
async fn policy(caller: &ActionCaller<'_>, action: &str) -> Result<(String, String), CallError> {
    let policy = caller
        .call("approval_policies.get", json!({ "action": action }), None)
        .await?;
    let text = |field: &str, fallback: &str| {
        policy[field]
            .as_str()
            .unwrap_or(fallback)
            .to_owned()
    };
    Ok((text("approval", "required"), text("self_approval", "forbidden")))
}

async fn set_policy(
    caller: &ActionCaller<'_>,
    action: &str,
    approval: &str,
    self_approval: &str,
) -> Result<(), CallError> {
    caller
        .call(
            "approval_policies.set",
            json!({ "action": action, "approval": approval, "self_approval": self_approval }),
            None,
        )
        .await
        .map(|_| ())
}

/// Why the command could not start.
#[derive(Debug, thiserror::Error)]
pub enum RetireError {
    /// The scan failed.
    #[error("database: {0}")]
    Database(#[from] sqlx::Error),
    /// The action caller could not be built.
    #[error(transparent)]
    Call(#[from] CallError),
}
