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
    for card in scan.candidates {
        match caller
            .call("cards.archive", json!({"id": card.id}), Some(card.version))
            .await
        {
            Ok(_) => report.archived += 1,
            Err(error) => report.failed.push((card.id, error.to_string())),
        }
    }
    Ok(report)
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
