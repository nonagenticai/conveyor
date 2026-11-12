//! Shared fixtures for the S03 store tests.
//!
//! The harness is `apps/server/tests/common/mod.rs` by `#[path]`: one ephemeral
//! `postgres:18` per test binary on an OS-assigned port, one fresh migrated database per
//! test, roles created exactly as `scripts/db-roles.sql` does. A second copy of that harness
//! is how two test suites drift apart.
//!
//! The seed below is deliberately raw SQL as the SUPERUSER: these are fixtures, and the
//! superuser bypasses RLS (it is the positive control). The behaviour under test — runtime
//! visibility, forged writes, claims, approvals, transitions — always runs as
//! `conveyor_runtime` in a scoped transaction.

#![allow(dead_code)]

#[path = "../../../../apps/server/tests/common/mod.rs"]
mod harness;

pub use harness::*;

use sqlx::PgPool;
use uuid::Uuid;

/// One complete fixture set for all fourteen S03 tables.
#[derive(Clone, Debug)]
pub struct Seeded {
    /// The tenant row's id.
    pub tenant: Uuid,
    /// The shared `app_user` used by owner/assignee/approver/author columns.
    pub user: Uuid,
    /// A second `app_user`, for a second approver.
    pub other_user: Uuid,
    /// `programme.id`.
    pub programme: Uuid,
    /// `project.id`.
    pub project: Uuid,
    /// `board.id`.
    pub board: Uuid,
    /// `list.id` (an `active` lane).
    pub list: Uuid,
    /// `card_type.id`.
    pub card_type: Uuid,
    /// `card.id`.
    pub card: Uuid,
    /// `run.id`.
    pub run: Uuid,
    /// `approval.id`.
    pub approval: Uuid,
    /// `governed_asset.id`.
    pub asset: Uuid,
}

/// The fourteen S03 tables, alphabetically. Every isolation assertion walks this list.
pub const DOMAIN_TABLES: [&str; 14] = [
    "andon_state",
    "approval",
    "approval_decision",
    "board",
    "card",
    "card_type",
    "catalogue_entry",
    "governed_asset",
    "list",
    "programme",
    "project",
    "revision",
    "run",
    "trace_link",
];

/// Insert one row into each of the fourteen S03 tables for a fresh tenant.
///
/// Runs as the superuser with explicit `tenant_id` values, so the rows exist independent of
/// any tenant scope. Returns the ids a test needs to build children.
pub async fn seed_tenant(pool: &PgPool, slug: &str) -> Seeded {
    let tenant: Uuid =
        sqlx::query_scalar("INSERT INTO tenant (slug, name) VALUES ($1, $2) RETURNING id")
            .bind(slug)
            .bind(format!("Tenant {slug}"))
            .fetch_one(pool)
            .await
            .expect("seed tenant");

    let user: Uuid =
        sqlx::query_scalar("INSERT INTO app_user (display_name) VALUES ($1) RETURNING id")
            .bind(format!("user-{slug}"))
            .fetch_one(pool)
            .await
            .expect("seed app_user");
    let other_user: Uuid =
        sqlx::query_scalar("INSERT INTO app_user (display_name) VALUES ($1) RETURNING id")
            .bind(format!("other-{slug}"))
            .fetch_one(pool)
            .await
            .expect("seed second app_user");

    let programme: Uuid = sqlx::query_scalar(
        "INSERT INTO programme (tenant_id, name, owner_user_id) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(tenant)
    .bind(format!("Programme {slug}"))
    .bind(user)
    .fetch_one(pool)
    .await
    .expect("seed programme");

    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name, programme_id, project_type_code, owner_user_id)
         VALUES ($1, $2, $3, $4, 'delivery', $5) RETURNING id",
    )
    .bind(tenant)
    .bind(format!("proj-{slug}"))
    .bind(format!("Project {slug}"))
    .bind(programme)
    .bind(user)
    .fetch_one(pool)
    .await
    .expect("seed project");

    let board: Uuid = sqlx::query_scalar(
        "INSERT INTO board (tenant_id, project_id, name, position) VALUES ($1, $2, $3, 0) RETURNING id",
    )
    .bind(tenant)
    .bind(project)
    .bind(format!("Board {slug}"))
    .fetch_one(pool)
    .await
    .expect("seed board");

    let list: Uuid = sqlx::query_scalar(
        "INSERT INTO list (tenant_id, board_id, name, position, kind)
         VALUES ($1, $2, 'Active', 1024, 'active') RETURNING id",
    )
    .bind(tenant)
    .bind(board)
    .fetch_one(pool)
    .await
    .expect("seed list");

    let card_type: Uuid = sqlx::query_scalar(
        "INSERT INTO card_type (tenant_id, code, label) VALUES ($1, 'feature', 'Feature') RETURNING id",
    )
    .bind(tenant)
    .fetch_one(pool)
    .await
    .expect("seed card_type");

    let card: Uuid = sqlx::query_scalar(
        "INSERT INTO card (tenant_id, board_id, list_id, card_type_id, name, position, created_by)
         VALUES ($1, $2, $3, $4, $5, 1024, $6) RETURNING id",
    )
    .bind(tenant)
    .bind(board)
    .bind(list)
    .bind(card_type)
    .bind(format!("Card {slug}"))
    .bind(user)
    .fetch_one(pool)
    .await
    .expect("seed card");

    let run: Uuid = sqlx::query_scalar(
        "INSERT INTO run (tenant_id, card_id, project_id) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(tenant)
    .bind(card)
    .bind(project)
    .fetch_one(pool)
    .await
    .expect("seed run");

    let approval: Uuid = sqlx::query_scalar(
        "INSERT INTO approval (tenant_id, run_id, project_id, gate, kind, status, requested_by, expires_at)
         VALUES ($1, $2, $3, 'review', 'gate', 'pending', $4, now() + interval '1 hour')
         RETURNING id",
    )
    .bind(tenant)
    .bind(run)
    .bind(project)
    .bind(user)
    .fetch_one(pool)
    .await
    .expect("seed approval");

    sqlx::query(
        "INSERT INTO approval_decision (tenant_id, approval_id, approver_id, decision)
         VALUES ($1, $2, $3, 'approve')",
    )
    .bind(tenant)
    .bind(approval)
    .bind(other_user)
    .execute(pool)
    .await
    .expect("seed approval_decision");

    let asset: Uuid = sqlx::query_scalar(
        "INSERT INTO governed_asset (tenant_id, kind, key, title, owner_user_id)
         VALUES ($1, 'requirement', $2, $3, $4) RETURNING id",
    )
    .bind(tenant)
    .bind(format!("req-{slug}"))
    .bind(format!("Requirement {slug}"))
    .bind(user)
    .fetch_one(pool)
    .await
    .expect("seed governed_asset");

    sqlx::query(
        "INSERT INTO trace_link (tenant_id, from_type, from_id, to_type, to_id)
         VALUES ($1, 'requirement', $2, 'card', $3)",
    )
    .bind(tenant)
    .bind(asset)
    .bind(card)
    .execute(pool)
    .await
    .expect("seed trace_link");

    sqlx::query(
        "INSERT INTO catalogue_entry (tenant_id, kind, code, label)
         VALUES ($1, 'gate', 'review', 'Review')",
    )
    .bind(tenant)
    .execute(pool)
    .await
    .expect("seed catalogue_entry");

    sqlx::query(
        "INSERT INTO revision (tenant_id, entity_type, entity_id, seq, label, content_hash,
                               author_id, status)
         VALUES ($1, 'requirement', $2, 1, 'R1', 'seed', $3, 'draft')",
    )
    .bind(tenant)
    .bind(asset)
    .bind(user)
    .execute(pool)
    .await
    .expect("seed revision");

    sqlx::query(
        "INSERT INTO andon_state (tenant_id, engaged, engaged_by, reason)
         VALUES ($1, false, $2, '')",
    )
    .bind(tenant)
    .bind(user)
    .execute(pool)
    .await
    .expect("seed andon_state");

    Seeded {
        tenant,
        user,
        other_user,
        programme,
        project,
        board,
        list,
        card_type,
        card,
        run,
        approval,
        asset,
    }
}
