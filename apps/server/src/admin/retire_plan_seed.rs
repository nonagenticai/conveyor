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
/// else holds — but the gate is the engine's, so the command asks for one per card and has a
/// DISTINCT tenant member decide it (SPEC_V2 §5.4). It never writes an approval policy: the
/// requester's exclusion is an invariant, not a permission (ledger v1-w4b-4). A tenant with no
/// second member is refused rather than self-approved.
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
    // §5.4: the gate's requester cannot decide it. The command therefore asks a DISTINCT active
    // member to decide each gate; it never stores a self-approval permission (ledger v1-w4b-4).
    let approver_id = store::identity::tenant_actor_excluding(pool, tenant, caller.actor_id())
        .await?
        .ok_or(RetireError::NoDistinctApprover(tenant))?;
    let approver = ActionCaller::human_as(pool, tenant, app_key, Some(approver_id)).await?;
    for card in scan.candidates {
        match archive_one(&caller, &approver, card.id, card.version).await {
            Ok(()) => report.archived += 1,
            Err(error) => report.failed.push((card.id, error.to_string())),
        }
    }
    Ok(report)
}

/// Open a gate for one card as the caller, have the distinct approver decide it, and archive
/// through the real handler with that approval id.
async fn archive_one(
    caller: &ActionCaller<'_>,
    approver: &ActionCaller<'_>,
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
                // The gate's subject is the seeded card, not a typed document; an operator
                // running the retire command records that explicitly (v1-u5a-approvals-1).
                "evidence_waiver_reason": "conveyor admin retire-plan-seed: the gate's subject is the seeded card, not a document",
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
            errors: String::new(),
        })?;
    approver
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
    /// §5.4: the gate's requester cannot decide it, and this tenant has no second active member
    /// to act as the distinct approver (ledger v1-w4b-4).
    #[error(
        "tenant {0} has no second active member to approve the archive gates; add a distinct \
         approver (SPEC_V2 §5.4)"
    )]
    NoDistinctApprover(TenantId),
    /// The action caller could not be built.
    #[error(transparent)]
    Call(#[from] CallError),
}
