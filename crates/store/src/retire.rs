//! The one-off retirement scan for the plan-seed cards.
//!
//! The build-time plan seeder stamped every card it owned with `meta.seed_name`. The seeder is
//! gone; those cards are not work, so an operator archives them through the real
//! `cards.archive` handler. This module only answers **which** cards are candidates and which
//! must be skipped, so the command never writes SQL of its own (CONTRACT §1).

use conveyor_domain::TenantId;
use sqlx::PgPool;
use uuid::Uuid;

use crate::scope::begin_scoped;

/// One seeded card that is safe to archive.
#[derive(Clone, Copy, Debug)]
pub struct SeededCard {
    /// The card id.
    pub id: Uuid,
    /// The row version `cards.archive` must name as its precondition.
    pub version: i64,
}

/// One tenant's seeded cards, partitioned by what the command may do.
#[derive(Clone, Debug, Default)]
pub struct SeededCards {
    /// Live, unleased and with no live run: archive these.
    pub candidates: Vec<SeededCard>,
    /// Already archived — a previous run or a person.
    pub already_archived: i64,
    /// Leased or with a queued/running/waiting_approval run: never touch.
    pub live: i64,
}

/// Read one tenant's plan-seed cards.
///
/// The explicit `tenant_id` predicate is deliberate: this scan is also called with the owner
/// pool, where RLS does not narrow the statement.
pub async fn scan_seeded_cards(
    pool: &PgPool,
    tenant: TenantId,
) -> Result<SeededCards, sqlx::Error> {
    let mut tx = begin_scoped(pool, tenant).await?;
    let rows: Vec<(Uuid, i64, bool, bool)> = sqlx::query_as(
        "SELECT c.id, c.version,
                c.archived_at IS NOT NULL,
                c.lease_token IS NOT NULL
                  OR EXISTS (SELECT 1 FROM run r
                              WHERE r.card_id = c.id
                                AND r.status IN ('queued', 'running', 'waiting_approval'))
           FROM card c
          WHERE c.tenant_id = $1 AND c.meta ? 'seed_name'
          ORDER BY c.id",
    )
    .bind(tenant.as_uuid())
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;

    let mut out = SeededCards::default();
    for (id, version, archived, live) in rows {
        if archived {
            out.already_archived += 1;
        } else if live {
            out.live += 1;
        } else {
            out.candidates.push(SeededCard { id, version });
        }
    }
    Ok(out)
}
