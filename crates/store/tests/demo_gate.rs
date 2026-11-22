//! v1-w2c-2 — a demo credit needs an accepted deliverable with typed evidence.
//!
//! The demo cadence is the exhibition record: a deliverable is credited only when it is
//! `accepted` and its evidence array carries typed refs. The live rework refs are path strings;
//! a blank string, null or an empty object is not something a demo can exhibit. An unaccepted
//! deliverable, an empty array and an untyped ref are each a distinct typed refusal.
//!
//! `cargo test -p conveyor-store --test demo_gate`.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_domain::spine::PlanError;
use conveyor_store::begin_scoped;
use conveyor_store::spine::{self, PlanStoreError};
use serde_json::{Value, json};
use uuid::Uuid;

struct Fixture {
    tenant: TenantId,
    cadence: Uuid,
    deliverable: Uuid,
}

/// One tenant, programme, project, demo cadence and a WBS deliverable with the given state.
async fn fixture(db: &support::TestDb, status: &str, evidence: Value) -> Fixture {
    let slug = format!("demo-{}", Uuid::now_v7().simple());
    let tenant_id: Uuid =
        sqlx::query_scalar("INSERT INTO tenant (slug, name) VALUES ($1, 'Demo') RETURNING id")
            .bind(&slug)
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
        "INSERT INTO project (tenant_id, key, name, programme_id)
         VALUES ($1, $2, 'Demo', $3) RETURNING id",
    )
    .bind(tenant_id)
    .bind(format!("DEMO-{}", Uuid::now_v7().simple()))
    .bind(programme)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the project");
    let cadence: Uuid = sqlx::query_scalar(
        "INSERT INTO programme_cadence (tenant_id, programme_id, kind, period)
         VALUES ($1, $2, 'demo', 'D1') RETURNING id",
    )
    .bind(tenant_id)
    .bind(programme)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the cadence");
    let deliverable: Uuid = sqlx::query_scalar(
        "INSERT INTO wbs_node (tenant_id, project_id, kind, title, status, evidence)
         VALUES ($1, $2, 'deliverable', 'Deliverable', $3, $4) RETURNING id",
    )
    .bind(tenant_id)
    .bind(project)
    .bind(status)
    .bind(&evidence)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the deliverable");
    Fixture {
        tenant,
        cadence,
        deliverable,
    }
}

async fn credit(db: &support::TestDb, f: &Fixture) -> Result<Uuid, PlanStoreError> {
    let mut tx = begin_scoped(&db.runtime, f.tenant).await.expect("scope");
    let outcome = spine::credit_demo_tx(&mut tx, f.tenant, f.cadence, f.deliverable).await;
    match outcome {
        Ok(_) => {
            tx.commit().await.expect("commit");
        }
        Err(_) => {
            tx.rollback().await.expect("rollback");
        }
    }
    outcome
}

#[tokio::test]
async fn a_credit_needs_typed_evidence() {
    let db = support::TestDb::fresh().await;

    // An unaccepted deliverable is refused before the evidence is considered.
    let open = fixture(&db, "open", json!(["acceptance/T.toml"])).await;
    assert!(
        matches!(
            credit(&db, &open).await,
            Err(PlanStoreError::Illegal(
                PlanError::DemoDeliverableNotComplete
            ))
        ),
        "an open deliverable cannot be credited"
    );

    // An empty array is no evidence.
    let empty = fixture(&db, "accepted", json!([])).await;
    assert!(
        matches!(
            credit(&db, &empty).await,
            Err(PlanStoreError::Illegal(PlanError::DemoEvidenceMissing))
        ),
        "an empty evidence array is refused"
    );

    // A blank string is not a typed ref.
    let untyped = fixture(&db, "accepted", json!([""])).await;
    assert!(
        matches!(
            credit(&db, &untyped).await,
            Err(PlanStoreError::Illegal(PlanError::DemoEvidenceUntyped))
        ),
        "a blank string is not evidence"
    );

    // A live rework path is a typed ref.
    let path = fixture(&db, "accepted", json!(["acceptance/programme/T.toml"])).await;
    let id = credit(&db, &path)
        .await
        .expect("a typed path string credits");
    let evidence: Value =
        sqlx::query_scalar("SELECT evidence FROM programme_demo_credit WHERE id = $1")
            .bind(id)
            .fetch_one(&db.super_pool)
            .await
            .expect("read the credit");
    assert_eq!(evidence, json!(["acceptance/programme/T.toml"]));

    // An object with a value is typed too.
    let object = fixture(
        &db,
        "accepted",
        json!([{"kind": "document", "id": "doc-1"}]),
    )
    .await;
    assert!(
        credit(&db, &object).await.is_ok(),
        "an object with a value credits"
    );
}
