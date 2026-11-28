//! The coach substrate a worker-level flow test needs.
//!
//! `coach.interrupt` creates a `coach` card and starts the coach process by key in the same
//! transaction (SPEC_V2 §6.2), so a test that asserts a producer opened an interruption must
//! seed both. The process is the ordinary published row the job runner resolves by key; one
//! step is mandatory (`process_runs.start`), and `cards.get` is enough for the substrate —
//! these tests assert the interruption, not the coach's triage.

use uuid::Uuid;

use super::TestDb;

/// The process `coach.interrupt` starts (`crate::board::pull::COACH_ERROR_PROCESS`).
pub const COACH_ERROR_PROCESS: &str = "cap.kanban_coach.handle_error.v2";

/// Seed the `coach` card type and a published one-step coach process for `tenant_id`.
pub async fn seed_coach_handle_error(db: &TestDb, tenant_id: Uuid, owner: Uuid) {
    sqlx::query(
        "INSERT INTO card_type (tenant_id, code, label) VALUES ($1, 'coach', 'Coach')
         ON CONFLICT (tenant_id, code) DO NOTHING",
    )
    .bind(tenant_id)
    .execute(&db.super_pool)
    .await
    .expect("seed the coach card type");

    let process: Uuid = sqlx::query_scalar(
        "INSERT INTO process (tenant_id, key, title, status, version, owner_user_id)
         VALUES ($1, $2, 'Coach error', 'published', 1, $3) RETURNING id",
    )
    .bind(tenant_id)
    .bind(COACH_ERROR_PROCESS)
    .bind(owner)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed the coach process");

    sqlx::query(
        "INSERT INTO process_step (tenant_id, process_id, position, action, input_template)
         VALUES ($1, $2, 1, 'cards.get', '{\"id\": \"{{card.id}}\"}'::jsonb)",
    )
    .bind(tenant_id)
    .bind(process)
    .execute(&db.super_pool)
    .await
    .expect("seed the coach process step");
}
