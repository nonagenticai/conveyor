#![allow(dead_code)]
//! T8 (#79) fixtures: a tenant with two distinct cards, two failed runs, a programme and a
//! capability knowledge pin.
//!
//! Evidence independence is a property of the referenced objects, so the fixture offers two of
//! each: a proposal can name two independent refs, or two refs to ONE object, and the test tells
//! the difference. The capability pin is the thing a promotion must never touch.

use conveyor_domain::TenantId;
use conveyor_store::begin_scoped;
use serde_json::json;
use uuid::Uuid;

use super::{Pg, Seeded, seed};

/// The seeded tenant plus its reviewable objects.
pub struct Anchor {
    /// `dev-a`'s tenant, alice and the shared reviewer.
    pub seeded: Seeded,
    /// A programme the proposal may belong to.
    pub programme_id: Uuid,
    /// A cadence the proposal may name.
    pub cadence_id: Uuid,
    /// The first card.
    pub card_a: Uuid,
    /// A second, distinct card.
    pub card_b: Uuid,
    /// A failed run on [`Self::card_a`].
    pub run_a: Uuid,
    /// A failed run on [`Self::card_b`].
    pub run_b: Uuid,
    /// The capability whose knowledge pin must not move.
    pub capability_id: Uuid,
}

/// One evidence ref object in the action's wire shape.
pub fn reference(kind: &str, id: Uuid) -> serde_json::Value {
    json!({"kind": kind, "id": id})
}

/// The knowledge pins a promotion must leave untouched, in a comparable shape.
pub async fn pins(pg: &Pg, tenant: TenantId) -> Vec<(String, i32, bool)> {
    sqlx::query_as(
        "SELECT pack_key, pack_version, required FROM capability_knowledge
          WHERE tenant_id = $1 ORDER BY pack_key, pack_version",
    )
    .bind(tenant.as_uuid())
    .fetch_all(&pg.super_pool)
    .await
    .expect("read the knowledge pins")
}

/// Seed the tenant and its two-card/two-run world.
pub async fn anchor(pg: &Pg) -> Anchor {
    let seeded = seed(pg, "dev-a", "alice", "Alice").await;
    let tenant: TenantId = seeded.tenant;
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scope");

    let programme_id: Uuid = sqlx::query_scalar(
        "INSERT INTO programme (tenant_id, name, owner_user_id) VALUES ($1, 'T8', $2) RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(seeded.user_id)
    .fetch_one(&mut *tx)
    .await
    .expect("programme");
    let cadence_id: Uuid = sqlx::query_scalar(
        "INSERT INTO programme_cadence (tenant_id, programme_id, kind, period, meta)
         VALUES ($1, $2, 'retro', 'PI 1 retro', '{}'::jsonb) RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(programme_id)
    .fetch_one(&mut *tx)
    .await
    .expect("cadence");
    let project_id: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name, programme_id, project_type_code)
         VALUES ($1, $2, 'Delivery', $3, 'delivery') RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(format!("T8-{}", Uuid::now_v7().simple()))
    .bind(programme_id)
    .fetch_one(&mut *tx)
    .await
    .expect("project");
    let board_id: Uuid = sqlx::query_scalar(
        "INSERT INTO board (tenant_id, project_id, name, position)
         VALUES ($1, $2, 'Board', 0) RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(project_id)
    .fetch_one(&mut *tx)
    .await
    .expect("board");
    let list_id: Uuid = sqlx::query_scalar(
        "INSERT INTO list (tenant_id, board_id, name, position, kind)
         VALUES ($1, $2, 'Active', 1024, 'active') RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(board_id)
    .fetch_one(&mut *tx)
    .await
    .expect("list");
    let card_type_id: Uuid = sqlx::query_scalar(
        "INSERT INTO card_type (tenant_id, code, label) VALUES ($1, 'lesson', 'Lesson')
         RETURNING id",
    )
    .bind(seeded.tenant_id)
    .fetch_one(&mut *tx)
    .await
    .expect("card type");

    let card_a: Uuid = sqlx::query_scalar(
        "INSERT INTO card
             (tenant_id, board_id, list_id, card_type_id, name, position, created_by)
         VALUES ($1, $2, $3, $4, 'First failing work', 1024, $5) RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(board_id)
    .bind(list_id)
    .bind(card_type_id)
    .bind(seeded.user_id)
    .fetch_one(&mut *tx)
    .await
    .expect("card a");
    let card_b: Uuid = sqlx::query_scalar(
        "INSERT INTO card
             (tenant_id, board_id, list_id, card_type_id, name, position, created_by)
         VALUES ($1, $2, $3, $4, 'Second failing work', 1024, $5) RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(board_id)
    .bind(list_id)
    .bind(card_type_id)
    .bind(seeded.user_id)
    .fetch_one(&mut *tx)
    .await
    .expect("card b");

    let run_a: Uuid = sqlx::query_scalar(
        "INSERT INTO run (tenant_id, card_id, project_id, status, error_class)
         VALUES ($1, $2, $3, 'failed', 'tool_error') RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(card_a)
    .bind(project_id)
    .fetch_one(&mut *tx)
    .await
    .expect("run a");
    let run_b: Uuid = sqlx::query_scalar(
        "INSERT INTO run (tenant_id, card_id, project_id, status, error_class)
         VALUES ($1, $2, $3, 'failed', 'tool_error') RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(card_b)
    .bind(project_id)
    .fetch_one(&mut *tx)
    .await
    .expect("run b");

    let capability_id: Uuid = sqlx::query_scalar(
        "INSERT INTO capability (tenant_id, code, label) VALUES ($1, 'lesson', 'Lesson')
         RETURNING id",
    )
    .bind(seeded.tenant_id)
    .fetch_one(&mut *tx)
    .await
    .expect("capability");
    sqlx::query(
        "INSERT INTO capability_knowledge
             (tenant_id, capability_id, pack_key, pack_version, required)
         VALUES ($1, $2, 'delivery-pack', 3, true)",
    )
    .bind(seeded.tenant_id)
    .bind(capability_id)
    .execute(&mut *tx)
    .await
    .expect("knowledge pin");
    tx.commit().await.expect("commit the T8 fixture");

    Anchor {
        seeded,
        programme_id,
        cadence_id,
        card_a,
        card_b,
        run_a,
        run_b,
        capability_id,
    }
}

/// Insert a machine principal holding `role`, and return its user id.
pub async fn principal(pg: &Pg, tenant: TenantId, subject: &str, role: &str) -> Uuid {
    let user_id: Uuid = sqlx::query_scalar(
        "INSERT INTO app_user (oidc_subject, email, display_name, kind)
         VALUES ($1, $2, $3, 'service') RETURNING id",
    )
    .bind(subject)
    .bind(format!("{}@dev.invalid", subject.replace(':', "-")))
    .bind(subject)
    .fetch_one(&pg.owner)
    .await
    .expect("insert the machine principal");
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scope");
    sqlx::query("INSERT INTO membership (tenant_id, user_id, role) VALUES ($1, $2, $3)")
        .bind(tenant.as_uuid())
        .bind(user_id)
        .bind(role)
        .execute(&mut *tx)
        .await
        .expect("machine membership");
    tx.commit().await.expect("commit the machine membership");
    user_id
}
