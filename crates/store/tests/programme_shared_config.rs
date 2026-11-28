//! v1-p82-3 — `programme.shared_config` with inheritance and lock semantics.
//!
//! A programme carries shared values beside its dependency graph, cadence and standing backlog
//! (SPEC_V2 §8.2). A key the programme locks keeps the programme's value when a project override
//! is resolved; every other key takes the override. The stored shape is
//! `{ "values": { key: value }, "locked": [key, ...] }`; a flat object is read as values with no
//! locks, so an older caller still resolves.
//!
//! `cargo test -p conveyor-store --test programme_shared_config`.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_store::begin_scoped;
use conveyor_store::portfolio::{
    NewProgrammeRow, ProgrammePatch, create_programme_tx, get_programme_tx, resolve_shared_config,
    update_programme_tx,
};
use serde_json::json;
use uuid::Uuid;

#[test]
fn a_locked_programme_config_cannot_be_overridden_per_project() {
    let programme =
        json!({"values": {"pi_length_weeks": 8, "wip_limit": 5}, "locked": ["pi_length_weeks"]});
    let overrides = json!({"pi_length_weeks": 2, "wip_limit": 9, "new_key": true});

    let resolved = resolve_shared_config(&programme, &overrides);

    assert_eq!(
        resolved["values"]["pi_length_weeks"],
        json!(8),
        "the programme's locked value wins"
    );
    assert_eq!(
        resolved["values"]["wip_limit"],
        json!(9),
        "an unlocked value takes the project override"
    );
    assert_eq!(
        resolved["values"]["new_key"],
        json!(true),
        "a new key is inherited"
    );
    assert_eq!(resolved["locked"], json!(["pi_length_weeks"]));
}

#[test]
fn a_flat_config_is_values_with_no_locks() {
    let resolved = resolve_shared_config(&json!({"pi": 8}), &json!({"pi": 2, "wip": 3}));
    assert_eq!(resolved["values"]["pi"], json!(2));
    assert_eq!(resolved["values"]["wip"], json!(3));
    assert_eq!(resolved["locked"], json!([]));
}

#[tokio::test]
async fn shared_config_round_trips_through_the_programme_row() {
    let db = support::TestDb::fresh().await;
    let slug = format!("shared-config-{}", Uuid::now_v7().simple());
    let tenant_id: Uuid =
        sqlx::query_scalar("INSERT INTO tenant (slug, name) VALUES ($1, 'Shared') RETURNING id")
            .bind(&slug)
            .fetch_one(&db.super_pool)
            .await
            .expect("insert the tenant");
    let tenant = TenantId::from_uuid(tenant_id).expect("tenant id");
    let initial = json!({"values": {"pi_length_weeks": 8}, "locked": ["pi_length_weeks"]});

    let created = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let record = create_programme_tx(
            &mut tx,
            tenant,
            &NewProgrammeRow {
                name: "Shared".to_owned(),
                status: None,
                owner_user_id: None,
                shared_config: initial.clone(),
            },
        )
        .await
        .expect("create the programme");
        tx.commit().await.expect("commit");
        record
    };
    assert_eq!(created.shared_config, initial);

    {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let read = get_programme_tx(&mut tx, tenant, created.id)
            .await
            .expect("get the programme")
            .expect("the programme exists");
        tx.commit().await.expect("commit");
        assert_eq!(read.shared_config, initial, "the column round-trips");
    }

    let next = json!({"values": {"pi_length_weeks": 6, "wip_limit": 5}, "locked": []});
    {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let updated = update_programme_tx(
            &mut tx,
            tenant,
            created.id,
            created.version,
            &ProgrammePatch {
                shared_config: Some(next.clone()),
                ..ProgrammePatch::default()
            },
        )
        .await
        .expect("update the programme");
        tx.commit().await.expect("commit");
        assert_eq!(updated.shared_config, next, "an update replaces the config");
    }
}
