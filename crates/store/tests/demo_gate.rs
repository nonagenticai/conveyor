//! T10 (#82) P2 — a deliverable without evidence cannot demo.
//!
//! A demo credit is recorded only for an accepted `wbs_node(kind='deliverable')` that exhibits
//! at least one evidence ref; otherwise the recording is refused with §5.4's missing-evidence
//! code and no credit row exists. The evidence is copied onto the credit.
//!
//! `cargo test -p conveyor-store --test demo_gate`.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_domain::spine::PlanError;
use conveyor_store::begin_scoped;
use conveyor_store::spine::{self, PlanStoreError};
use serde_json::json;
use uuid::Uuid;

struct Fixture {
    tenant: TenantId,
    cadence: Uuid,
    project: Uuid,
}

async fn fixture(db: &support::TestDb) -> Fixture {
    let tenant_id: Uuid =
        sqlx::query_scalar("INSERT INTO tenant (slug, name) VALUES ($1, 'Demo') RETURNING id")
            .bind(format!("demo-{}", Uuid::now_v7().simple()))
            .fetch_one(&db.super_pool)
            .await
            .expect("insert the tenant");
    let tenant = TenantId::from_uuid(tenant_id).expect("tenant id");
    let programme: Uuid =
        sqlx::query_scalar("INSERT INTO programme (tenant_id, name) VALUES ($1, 'P') RETURNING id")
            .bind(tenant_id)
            .fetch_one(&db.super_pool)
            .await
            .expect("insert the programme");
    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name) VALUES ($1, $2, 'Delivery') RETURNING id",
    )
    .bind(tenant_id)
    .bind(format!("PRJ-{}", Uuid::now_v7().simple()))
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the project");
    let cadence: Uuid = sqlx::query_scalar(
        "INSERT INTO programme_cadence (tenant_id, programme_id, kind, period)
         VALUES ($1, $2, 'demo', 'Demo 1') RETURNING id",
    )
    .bind(tenant_id)
    .bind(programme)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the demo cadence");
    Fixture {
        tenant,
        cadence,
        project,
    }
}

async fn deliverable(
    db: &support::TestDb,
    f: &Fixture,
    status: &str,
    evidence: serde_json::Value,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO wbs_node (tenant_id, project_id, position, kind, title, status, evidence)
         VALUES ($1, $2, 0, 'deliverable', 'the thing', $3, $4) RETURNING id",
    )
    .bind(f.tenant.as_uuid())
    .bind(f.project)
    .bind(status)
    .bind(evidence)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the deliverable")
}

async fn credit(
    db: &support::TestDb,
    f: &Fixture,
    deliverable: Uuid,
) -> Result<Uuid, PlanStoreError> {
    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let outcome = spine::credit_demo_tx(&mut tx, f.tenant, f.cadence, deliverable).await;
    if outcome.is_ok() {
        tx.commit().await.expect("commit");
    } else {
        tx.rollback().await.expect("rollback");
    }
    outcome
}

async fn credits(db: &support::TestDb, tenant: TenantId) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM programme_demo_credit WHERE tenant_id = $1")
        .bind(tenant.as_uuid())
        .fetch_one(&db.super_pool)
        .await
        .expect("count credits")
}

#[tokio::test]
async fn a_delivered_item_with_evidence_earns_credit() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db).await;
    let item = deliverable(&db, &f, "accepted", json!([{ "run_id": "r-1" }])).await;

    let credit_id = credit(&db, &f, item)
        .await
        .expect("the demo earns a credit");
    let (cadence, copied): (Uuid, serde_json::Value) =
        sqlx::query_as("SELECT cadence_id, evidence FROM programme_demo_credit WHERE id = $1")
            .bind(credit_id)
            .fetch_one(&db.super_pool)
            .await
            .expect("read the credit");
    assert_eq!(cadence, f.cadence);
    assert_eq!(
        copied,
        json!([{ "run_id": "r-1" }]),
        "the evidence is copied onto the credit"
    );

    // Recording the same pair again returns the same credit, never a second row.
    let again = credit(&db, &f, item).await.expect("the second recording");
    assert_eq!(again, credit_id);
    assert_eq!(credits(&db, f.tenant).await, 1);
}

#[tokio::test]
async fn evidence_less_deliverable_cannot_demo() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db).await;
    let item = deliverable(&db, &f, "accepted", json!([])).await;

    let refused = credit(&db, &f, item).await;
    assert!(
        matches!(
            refused,
            Err(PlanStoreError::Illegal(PlanError::DemoEvidenceMissing))
        ),
        "§5.4's missing-evidence code: {refused:?}"
    );
    assert_eq!(
        credits(&db, f.tenant).await,
        0,
        "no credit row exists after the refusal"
    );

    // An unfinished deliverable is refused for its own reason.
    let draft = deliverable(&db, &f, "open", json!([{ "run_id": "r-2" }])).await;
    let refused = credit(&db, &f, draft).await;
    assert!(
        matches!(
            refused,
            Err(PlanStoreError::Illegal(
                PlanError::DemoDeliverableNotComplete
            ))
        ),
        "an unfinished deliverable is refused: {refused:?}"
    );
    assert_eq!(credits(&db, f.tenant).await, 0);
}
