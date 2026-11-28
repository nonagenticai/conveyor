//! T9 / #526 — a kata outcome is a typed candidate, not a label.
//!
//! Finding v1-u5a-t9-2: an H review must name a `kata.open` run of this project, or an approved
//! deferral of this project; a bare `{"kind": "kata"}` is refused.
//!
//! `cargo test -p conveyor-actions --test togaf_kata_outcome`.

mod support;

#[path = "../../api/tests/architecture_support/mod.rs"]
mod arch;

use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

async fn process_id(pool: &PgPool, tenant: Uuid, key: &str) -> Uuid {
    sqlx::query(
        "INSERT INTO process (tenant_id, key, title, status, version)
         SELECT $1, $2, 'Kata process', 'published',
                (SELECT COALESCE(MAX(version), 0) + 1
                   FROM process WHERE tenant_id = $1 AND key = $2)
         ON CONFLICT DO NOTHING",
    )
    .bind(tenant)
    .bind(key)
    .execute(pool)
    .await
    .expect("insert the published process");
    sqlx::query_scalar(
        "SELECT id FROM process
          WHERE tenant_id = $1 AND key = $2 AND status = 'published'",
    )
    .bind(tenant)
    .bind(key)
    .fetch_one(pool)
    .await
    .expect("read the published process")
}

async fn run_of(pool: &PgPool, tenant: Uuid, project: Uuid, process: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO run (tenant_id, project_id, process_id, status)
         VALUES ($1, $2, $3, 'succeeded')
         RETURNING id",
    )
    .bind(tenant)
    .bind(project)
    .bind(process)
    .fetch_one(pool)
    .await
    .expect("insert the run")
}

/// Record one review with `outcome` on a fresh contract of the project.
async fn review_with_outcome(f: &arch::Fixture, project: &str, outcome: Value) -> (u16, Value) {
    let contract =
        arch::request_contract(&f.running, project, "request_for_architecture", "h").await;
    arch::call_as(
        &f.running,
        support::REVIEWER,
        "architecture.reviews_record",
        json!({
            "project_id": project,
            "contract_id": arch::id_of(&contract),
            "level": "compliant",
            "delegation_ref": f.delegation,
            "findings": [{ "finding": "the architecture meets its purpose" }],
            "evidence": { "ref": "phase H" },
            "outcome": outcome,
        }),
        None,
    )
    .await
}

#[tokio::test]
async fn a_bare_kata_outcome_is_refused() {
    let f = arch::fixture().await;
    let project = arch::raw_project(&f.pg, f.seeded.tenant_id, "Phase H").await;
    let (status, refused) =
        review_with_outcome(&f, &project.to_string(), json!({ "kind": "kata" })).await;
    assert_eq!(status, 409, "a bare kata outcome was accepted: {refused}");
    assert_eq!(
        refused["type"], "architecture_kata_candidate_required",
        "{refused}"
    );
}

#[tokio::test]
async fn a_kata_outcome_names_a_kata_candidate_of_the_project() {
    let f = arch::fixture().await;
    let project = arch::raw_project(&f.pg, f.seeded.tenant_id, "Phase H").await;
    let kata = process_id(&f.pg.super_pool, f.seeded.tenant_id, "kata.open").await;
    let candidate = run_of(&f.pg.super_pool, f.seeded.tenant_id, project, kata).await;

    let (status, review) = review_with_outcome(
        &f,
        &project.to_string(),
        json!({ "kind": "kata", "kata_candidate_id": candidate }),
    )
    .await;
    assert_eq!(status, 201, "{review}");
    assert_eq!(
        review["outcome"]["kata_candidate_id"],
        candidate.to_string(),
        "{review}"
    );

    // A run of another process is not a kata candidate.
    let other_process = process_id(&f.pg.super_pool, f.seeded.tenant_id, "other.process").await;
    let not_kata = run_of(&f.pg.super_pool, f.seeded.tenant_id, project, other_process).await;
    let (status, refused) = review_with_outcome(
        &f,
        &project.to_string(),
        json!({ "kind": "kata", "kata_candidate_id": not_kata }),
    )
    .await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(
        refused["type"], "architecture_kata_candidate_not_found",
        "{refused}"
    );

    // A kata run of another project is not THIS project's candidate.
    let elsewhere = arch::raw_project(&f.pg, f.seeded.tenant_id, "Elsewhere").await;
    let foreign = run_of(&f.pg.super_pool, f.seeded.tenant_id, elsewhere, kata).await;
    let (status, refused) = review_with_outcome(
        &f,
        &project.to_string(),
        json!({ "kind": "kata", "kata_candidate_id": foreign }),
    )
    .await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(
        refused["type"], "architecture_kata_candidate_not_found",
        "{refused}"
    );
}

#[tokio::test]
async fn a_kata_deferral_requires_an_approved_project_decision() {
    let f = arch::fixture().await;
    let project = arch::raw_project(&f.pg, f.seeded.tenant_id, "Phase H").await;
    let decision: Uuid = sqlx::query_scalar(
        "INSERT INTO approval (tenant_id, project_id, gate, kind, status)
         VALUES ($1, $2, 'kata_deferral', 'kata_deferral', 'approved')
         RETURNING id",
    )
    .bind(f.seeded.tenant_id)
    .bind(project)
    .fetch_one(&f.pg.super_pool)
    .await
    .expect("insert the deferral decision");

    let (status, review) = review_with_outcome(
        &f,
        &project.to_string(),
        json!({
            "kind": "kata",
            "deferral": {
                "approval_id": decision,
                "reason": "the kata starts after Phase H closes",
            },
        }),
    )
    .await;
    assert_eq!(status, 201, "{review}");

    // A decision id that names no approved decision of this project is refused.
    let (status, refused) = review_with_outcome(
        &f,
        &project.to_string(),
        json!({
            "kind": "kata",
            "deferral": {
                "approval_id": Uuid::now_v7().to_string(),
                "reason": "the kata starts after Phase H closes",
            },
        }),
    )
    .await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(
        refused["type"], "architecture_kata_deferral_not_approved",
        "{refused}"
    );

    // A pending decision is not an approved deferral.
    let pending: Uuid = sqlx::query_scalar(
        "INSERT INTO approval (tenant_id, project_id, gate, kind, status)
         VALUES ($1, $2, 'kata_deferral', 'kata_deferral', 'pending')
         RETURNING id",
    )
    .bind(f.seeded.tenant_id)
    .bind(project)
    .fetch_one(&f.pg.super_pool)
    .await
    .expect("insert the pending decision");
    let (status, refused) = review_with_outcome(
        &f,
        &project.to_string(),
        json!({
            "kind": "kata",
            "deferral": {
                "approval_id": pending,
                "reason": "the kata starts after Phase H closes",
            },
        }),
    )
    .await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(
        refused["type"], "architecture_kata_deferral_not_approved",
        "{refused}"
    );

    // A deferral without its reason is refused, not accepted on the approval alone.
    let (status, refused) = review_with_outcome(
        &f,
        &project.to_string(),
        json!({
            "kind": "kata",
            "deferral": { "approval_id": decision },
        }),
    )
    .await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(refused["type"], "conflict", "{refused}");
}
