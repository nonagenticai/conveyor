//! Shared seeding for the S11 step-8 server tests (`process_step`, `process_reap`).
//!
//! The graph is inserted with superuser SQL on purpose: these tests are about the WORKER's
//! dispatch and the reaper's predicate, not about the HTTP surface that creates a run. Every row
//! a `process.step` job needs to be real is here — a project/board/list/card_type/card, a
//! published process with its frozen steps, a running run with materialised `queued` steps, and
//! the job row itself.

use conveyor_domain::TenantId;
use serde_json::{Value, json};
use uuid::Uuid;

use super::TestDb;

/// The rows one seeded process run is made of.
pub struct ProcessGraph {
    /// The tenant everything belongs to.
    pub tenant: TenantId,
    /// The card the run executes.
    pub card: Uuid,
    /// The running run.
    pub run: Uuid,
    /// The frozen `process_step` rows, in position order.
    pub steps: Vec<Uuid>,
}

/// Seed one running process run whose first step has a `process.step` job in `job_status`.
///
/// `steps` is `(action, input_template)` in position order. Every step is materialised `queued`
/// at attempt 1, and exactly one job exists for position 1 — the shape `process_runs.start`
/// commits.
pub async fn seed(
    db: &TestDb,
    slug: &str,
    steps: &[(&str, Value)],
    job_status: &str,
) -> ProcessGraph {
    let tenant_id: Uuid =
        sqlx::query_scalar("INSERT INTO tenant (slug, name) VALUES ($1, $1) RETURNING id")
            .bind(slug)
            .fetch_one(&db.super_pool)
            .await
            .expect("seed a tenant");
    let tenant = TenantId::from_uuid(tenant_id).expect("non-nil tenant");
    let user: Uuid = sqlx::query_scalar(
        "INSERT INTO app_user (oidc_subject, email, display_name)
         VALUES ($1, $2, $1) RETURNING id",
    )
    .bind(format!("dev:{slug}"))
    .bind(format!("{slug}@dev.invalid"))
    .fetch_one(&db.super_pool)
    .await
    .expect("seed a user");
    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name) VALUES ($1, 'p', 'Project') RETURNING id",
    )
    .bind(tenant_id)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed a project");
    let board: Uuid = sqlx::query_scalar(
        "INSERT INTO board (tenant_id, project_id, name) VALUES ($1, $2, 'Board') RETURNING id",
    )
    .bind(tenant_id)
    .bind(project)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed a board");
    let lane: Uuid = sqlx::query_scalar(
        "INSERT INTO list (tenant_id, board_id, name, position, kind)
         VALUES ($1, $2, 'Active', 1, 'active') RETURNING id",
    )
    .bind(tenant_id)
    .bind(board)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed a list");
    let card_type: Uuid = sqlx::query_scalar(
        "INSERT INTO card_type (tenant_id, code, label) VALUES ($1, 'feature', 'Feature') RETURNING id",
    )
    .bind(tenant_id)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed a card type");
    let card: Uuid = sqlx::query_scalar(
        "INSERT INTO card (tenant_id, board_id, list_id, card_type_id, name, position)
         VALUES ($1, $2, $3, $4, 'Card', 1) RETURNING id",
    )
    .bind(tenant_id)
    .bind(board)
    .bind(lane)
    .bind(card_type)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed a card");
    let process: Uuid = sqlx::query_scalar(
        "INSERT INTO process (tenant_id, key, title, status, version, owner_user_id)
         VALUES ($1, 'proc', 'Process', 'published', 1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind(user)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed a published process");

    let mut step_ids = Vec::with_capacity(steps.len());
    for (index, (action, template)) in steps.iter().enumerate() {
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO process_step (tenant_id, process_id, position, action, input_template)
             VALUES ($1, $2, $3, $4, $5) RETURNING id",
        )
        .bind(tenant_id)
        .bind(process)
        .bind(i32::try_from(index).expect("small list") + 1)
        .bind(*action)
        .bind(template)
        .fetch_one(&db.super_pool)
        .await
        .expect("seed a process step");
        step_ids.push(id);
    }

    let run: Uuid = sqlx::query_scalar(
        "INSERT INTO run (tenant_id, card_id, project_id, status, attempt, process_id, inputs)
         VALUES ($1, $2, $3, 'running', 1, $4, '{}'::jsonb) RETURNING id",
    )
    .bind(tenant_id)
    .bind(card)
    .bind(project)
    .bind(process)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed a running run");
    for (index, (action, _)) in steps.iter().enumerate() {
        sqlx::query(
            "INSERT INTO run_step
                 (tenant_id, run_id, process_step_id, position, attempt, action, status)
             VALUES ($1, $2, $3, $4, 1, $5, 'queued')",
        )
        .bind(tenant_id)
        .bind(run)
        .bind(step_ids[index])
        .bind(i32::try_from(index).expect("small list") + 1)
        .bind(*action)
        .execute(&db.super_pool)
        .await
        .expect("materialise a run step");
    }
    sqlx::query(
        "INSERT INTO job (tenant_id, kind, dedupe_key, status, payload, max_attempts)
         VALUES ($1, 'process.step', $2, $3, $4, 5)",
    )
    .bind(tenant_id)
    .bind(format!("{run}:1:1"))
    .bind(job_status)
    .bind(json!({ "run_id": run, "position": 1, "attempt": 1 }))
    .execute(&db.super_pool)
    .await
    .expect("seed the step job");

    ProcessGraph {
        tenant,
        card,
        run,
        steps: step_ids,
    }
}
