//! S15 steps 1–4: the capability aggregate's store.
//!
//! The properties, each a way this could go wrong silently:
//!
//! 1. **The seed is idempotent** — a second import creates nothing and rewrites nothing; a changed
//!    row bumps `version`, so an If-Match caller sees the change instead of a silent refresh.
//! 2. **Conflicts are typed** — a duplicate code/key is `DuplicateCode`/`DuplicateKey`, never a 500.
//! 3. **The executor spec is version-guarded** — a stale If-Match is `VersionMismatch{current}`.
//! 4. **The bindings resolve** — packs come back pinned and ordered; a card type resolves its
//!    capability through the join.
//! 5. **Five tables isolate four ways** — runtime, owner-without-scope, superuser, forged write.
//!
//! `rbuild -- test -p conveyor-store --test capability -- --nocapture`.

#[path = "support/mod.rs"]
mod support;

use std::str::FromStr;

use conveyor_domain::TenantId;
use conveyor_domain::capability::RelationKind;
use conveyor_store::begin_scoped;
use conveyor_store::capability::{
    self, AgentPatch, CapabilityImport, CapabilityStoreError, NewAgent, NewCapability,
    RelationImport,
};
use rust_decimal::Decimal;
use serde_json::json;
use uuid::Uuid;

fn tenant(seed: &support::Seeded) -> TenantId {
    TenantId::from_uuid(seed.tenant).expect("non-nil")
}

fn analysis() -> CapabilityImport {
    CapabilityImport {
        code: "analysis".to_owned(),
        label: "Analysis".to_owned(),
        description: "Card types for analysis.".to_owned(),
        status: "active".to_owned(),
        meta: json!({}),
    }
}

fn architecture() -> CapabilityImport {
    CapabilityImport {
        code: "architecture".to_owned(),
        label: "Architecture".to_owned(),
        description: "Card types for architecture.".to_owned(),
        status: "active".to_owned(),
        meta: json!({}),
    }
}

async fn new_capability(db: &support::TestDb, seed: &support::Seeded, code: &str) -> Uuid {
    let mut tx = begin_scoped(&db.runtime, tenant(seed))
        .await
        .expect("scope");
    let created = capability::create_capability_tx(
        &mut tx,
        tenant(seed),
        &NewCapability {
            code: code.to_owned(),
            label: code.to_owned(),
            description: String::new(),
            status: "active".to_owned(),
            meta: json!({}),
        },
    )
    .await
    .expect("create a capability");
    tx.commit().await.expect("commit");
    created.id
}

#[tokio::test]
async fn the_seed_import_is_idempotent_and_a_change_bumps_the_version() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "s15-import").await;
    let rows = vec![analysis(), architecture()];

    let mut tx = begin_scoped(&db.runtime, tenant(&seed))
        .await
        .expect("scope");
    let first = capability::import_capabilities_tx(&mut tx, tenant(&seed), &rows)
        .await
        .expect("first import");
    assert_eq!(
        (first.created, first.updated, first.skipped),
        (2, 0, 0),
        "the first run creates both rows"
    );

    let second = capability::import_capabilities_tx(&mut tx, tenant(&seed), &rows)
        .await
        .expect("second import");
    assert_eq!(
        (second.created, second.updated, second.skipped),
        (0, 0, 2),
        "the second run rewrites nothing"
    );

    let mut changed = rows.clone();
    changed[0].label = "Analysis (renamed)".to_owned();
    let third = capability::import_capabilities_tx(&mut tx, tenant(&seed), &changed)
        .await
        .expect("changed import");
    assert_eq!(
        (third.created, third.updated, third.skipped),
        (0, 1, 1),
        "exactly the changed row is rewritten"
    );

    let record = capability::get_capability_by_code_tx(&mut tx, tenant(&seed), "analysis")
        .await
        .expect("read back")
        .expect("the row exists");
    assert_eq!(record.label, "Analysis (renamed)");
    assert_eq!(record.version, 2, "a seed change is not silent");

    let relations = vec![RelationImport {
        from_code: "analysis".to_owned(),
        to_code: "architecture".to_owned(),
        kind: "requires".to_owned(),
    }];
    let first_relations = capability::import_relations_tx(&mut tx, tenant(&seed), &relations)
        .await
        .expect("import the relation");
    assert_eq!((first_relations.created, first_relations.skipped), (1, 0));
    let again_relations = capability::import_relations_tx(&mut tx, tenant(&seed), &relations)
        .await
        .expect("import the relation twice");
    assert_eq!((again_relations.created, again_relations.skipped), (0, 1));
    tx.commit().await.expect("commit");
}

#[tokio::test]
async fn a_duplicate_code_and_a_duplicate_agent_key_are_typed_conflicts() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "s15-conflict").await;

    // A 23505 aborts the transaction it was raised in, so every expected conflict gets its own
    // transaction: that is also how the action layer will run (one action, one transaction).
    let created = {
        let mut tx = begin_scoped(&db.runtime, tenant(&seed))
            .await
            .expect("scope");
        let created = capability::create_capability_tx(
            &mut tx,
            tenant(&seed),
            &NewCapability {
                code: "analysis".to_owned(),
                label: "Analysis".to_owned(),
                description: String::new(),
                status: "active".to_owned(),
                meta: json!({}),
            },
        )
        .await
        .expect("create");
        tx.commit().await.expect("commit");
        created
    };

    {
        let mut tx = begin_scoped(&db.runtime, tenant(&seed))
            .await
            .expect("scope");
        let duplicate = capability::create_capability_tx(
            &mut tx,
            tenant(&seed),
            &NewCapability {
                code: "analysis".to_owned(),
                label: "Analysis again".to_owned(),
                description: String::new(),
                status: "active".to_owned(),
                meta: json!({}),
            },
        )
        .await;
        assert!(
            matches!(duplicate, Err(CapabilityStoreError::DuplicateCode)),
            "a duplicate code must be a typed conflict, got {duplicate:?}"
        );
        tx.rollback().await.expect("roll back the conflict");
    }

    {
        let mut tx = begin_scoped(&db.runtime, tenant(&seed))
            .await
            .expect("scope");
        let agent = capability::create_agent_tx(
            &mut tx,
            tenant(&seed),
            &NewAgent {
                capability_id: created.id,
                key: "default".to_owned(),
                status: "active".to_owned(),
                model_ref: "model:primary".to_owned(),
                prompt_ref: Some("prompt_audit@1".to_owned()),
                params: json!({"temperature": 0}),
                allowed_actions: vec!["documents.list".to_owned()],
                max_iterations: 20,
                timeout_secs: 300,
            },
        )
        .await
        .expect("create the agent");
        assert_eq!(agent.version, 1);
        tx.commit().await.expect("commit");
    }

    {
        let mut tx = begin_scoped(&db.runtime, tenant(&seed))
            .await
            .expect("scope");
        let duplicate = capability::create_agent_tx(
            &mut tx,
            tenant(&seed),
            &NewAgent {
                capability_id: created.id,
                key: "default".to_owned(),
                status: "active".to_owned(),
                model_ref: "model:secondary".to_owned(),
                prompt_ref: None,
                params: json!({}),
                allowed_actions: vec![],
                max_iterations: 1,
                timeout_secs: 1,
            },
        )
        .await;
        assert!(
            matches!(duplicate, Err(CapabilityStoreError::DuplicateKey)),
            "a duplicate agent key must be a typed conflict, got {duplicate:?}"
        );
        tx.rollback().await.expect("roll back the conflict");
    }
}

#[tokio::test]
async fn an_agent_update_is_version_guarded() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "s15-agent").await;
    let capability_id = new_capability(&db, &seed, "analysis").await;

    let mut tx = begin_scoped(&db.runtime, tenant(&seed))
        .await
        .expect("scope");
    let agent = capability::create_agent_tx(
        &mut tx,
        tenant(&seed),
        &NewAgent {
            capability_id,
            key: "default".to_owned(),
            status: "draft".to_owned(),
            model_ref: "model:primary".to_owned(),
            prompt_ref: None,
            params: json!({}),
            allowed_actions: vec![],
            max_iterations: 1,
            timeout_secs: 1,
        },
    )
    .await
    .expect("create");

    let patch = AgentPatch {
        status: "active".to_owned(),
        model_ref: "model:secondary".to_owned(),
        prompt_ref: Some("prompt_audit@1".to_owned()),
        params: json!({"temperature": 0}),
        allowed_actions: vec!["documents.list".to_owned()],
        max_iterations: 10,
        timeout_secs: 120,
    };
    let updated = capability::update_agent_tx(&mut tx, tenant(&seed), agent.id, 1, &patch)
        .await
        .expect("update at version 1");
    assert_eq!(updated.version, 2);
    assert_eq!(updated.model_ref, "model:secondary");

    let stale = capability::update_agent_tx(&mut tx, tenant(&seed), agent.id, 1, &patch).await;
    assert!(
        matches!(
            stale,
            Err(CapabilityStoreError::VersionMismatch { current: 2 })
        ),
        "a stale If-Match must name the current version, got {stale:?}"
    );

    let missing =
        capability::update_agent_tx(&mut tx, tenant(&seed), Uuid::now_v7(), 1, &patch).await;
    assert!(
        matches!(missing, Err(CapabilityStoreError::NotFound)),
        "a missing row is NotFound, not a version conflict"
    );
    tx.commit().await.expect("commit");
}

#[tokio::test]
async fn the_packs_and_the_card_type_binding_resolve() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "s15-bind").await;
    let capability_id = new_capability(&db, &seed, "analysis").await;

    // The card type's binding is an admin act; set it as the superuser, then read it through the
    // store's join as the runtime role.
    sqlx::query("UPDATE card_type SET capability_id = $1 WHERE id = $2")
        .bind(capability_id)
        .bind(seed.card_type)
        .execute(&db.super_pool)
        .await
        .expect("bind the card type");

    let mut tx = begin_scoped(&db.runtime, tenant(&seed))
        .await
        .expect("scope");
    let resolved = capability::capability_for_card_type_tx(&mut tx, tenant(&seed), seed.card_type)
        .await
        .expect("resolve the card type")
        .expect("the card type is bound");
    assert_eq!(resolved.id, capability_id);

    let unscoped = capability::capability_for_card_type_tx(&mut tx, tenant(&seed), Uuid::now_v7())
        .await
        .expect("an unbound card type is None");
    assert!(unscoped.is_none());

    let first = capability::bind_knowledge_tx(
        &mut tx,
        tenant(&seed),
        capability_id,
        "conveyor",
        1,
        Some(Decimal::from_str("0.75").expect("decimal")),
        true,
    )
    .await
    .expect("bind v1");
    capability::bind_knowledge_tx(
        &mut tx,
        tenant(&seed),
        capability_id,
        "conveyor",
        2,
        None,
        false,
    )
    .await
    .expect("bind v2");

    // Re-binding a pinned version updates it in place; it never adds a second row.
    let rebound = capability::bind_knowledge_tx(
        &mut tx,
        tenant(&seed),
        capability_id,
        "conveyor",
        1,
        None,
        false,
    )
    .await
    .expect("re-bind v1");
    assert_eq!(rebound.id, first.id);
    assert!(!rebound.required);
    assert!(rebound.min_score.is_none());

    let packs = capability::packs_for_capability_tx(&mut tx, tenant(&seed), capability_id)
        .await
        .expect("the packs");
    assert_eq!(packs.len(), 2, "two pinned versions, not three");
    assert_eq!(packs[0].pack_version, 1);
    assert_eq!(packs[1].pack_version, 2);

    let other_id = new_capability(&db, &seed, "architecture").await;
    let relation = capability::upsert_relation_tx(
        &mut tx,
        tenant(&seed),
        capability_id,
        other_id,
        RelationKind::Requires,
    )
    .await
    .expect("upsert the relation");
    assert_eq!(relation.kind, "requires");
    let edges = capability::relations_from_tx(&mut tx, tenant(&seed), capability_id)
        .await
        .expect("the edges");
    assert_eq!(edges.len(), 1);
    tx.commit().await.expect("commit");
}

/// Bad input must be a typed refusal, never a DB CHECK surfacing as a 500 (the S11 lesson).
#[tokio::test]
async fn blank_fields_and_a_bad_pack_version_are_typed_refusals() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "s15-refuse").await;
    let capability_id = new_capability(&db, &seed, "analysis").await;
    let mut tx = begin_scoped(&db.runtime, tenant(&seed))
        .await
        .expect("scope");

    let blank_code = capability::create_capability_tx(
        &mut tx,
        tenant(&seed),
        &NewCapability {
            code: "   ".to_owned(),
            label: "Label".to_owned(),
            description: String::new(),
            status: "active".to_owned(),
            meta: json!({}),
        },
    )
    .await;
    assert!(
        matches!(blank_code, Err(CapabilityStoreError::Blank("code"))),
        "a blank code must be a typed refusal, got {blank_code:?}"
    );

    let bad_status = capability::create_agent_tx(
        &mut tx,
        tenant(&seed),
        &NewAgent {
            capability_id,
            key: "default".to_owned(),
            status: "published".to_owned(),
            model_ref: "model:primary".to_owned(),
            prompt_ref: None,
            params: json!({}),
            allowed_actions: vec![],
            max_iterations: 1,
            timeout_secs: 1,
        },
    )
    .await;
    assert!(
        matches!(bad_status, Err(CapabilityStoreError::UnknownStatus(_))),
        "an agent status outside the vocabulary must be typed, got {bad_status:?}"
    );

    let blank_key = capability::create_agent_tx(
        &mut tx,
        tenant(&seed),
        &NewAgent {
            capability_id,
            key: "  ".to_owned(),
            status: "draft".to_owned(),
            model_ref: "model:primary".to_owned(),
            prompt_ref: None,
            params: json!({}),
            allowed_actions: vec![],
            max_iterations: 1,
            timeout_secs: 1,
        },
    )
    .await;
    assert!(
        matches!(blank_key, Err(CapabilityStoreError::Blank("key"))),
        "a blank agent key must be a typed refusal, got {blank_key:?}"
    );

    let bad_pack = capability::bind_knowledge_tx(
        &mut tx,
        tenant(&seed),
        capability_id,
        "conveyor",
        0,
        None,
        true,
    )
    .await;
    assert!(
        matches!(bad_pack, Err(CapabilityStoreError::BadPackVersion(0))),
        "pack_version 0 must be a typed refusal, got {bad_pack:?}"
    );
    tx.rollback().await.expect("roll back");
}

/// The four RLS vantages on the five new tables, as `rls_domain.rs` does for S03's fourteen.
#[tokio::test]
async fn the_five_capability_tables_isolate_runtime_owner_and_superuser() {
    let db = support::TestDb::fresh().await;
    let a = support::seed_tenant(&db.super_pool, "s15-rls-a").await;
    let b = support::seed_tenant(&db.super_pool, "s15-rls-b").await;
    let tables = [
        "capability",
        "capability_agent",
        "capability_knowledge",
        "capability_relation",
        "work_package_memory",
    ];

    for seeded in [&a, &b] {
        let first: Uuid = sqlx::query_scalar(
            "INSERT INTO capability (tenant_id, code, label, status)
             VALUES ($1, 'analysis', 'Analysis', 'active') RETURNING id",
        )
        .bind(seeded.tenant)
        .fetch_one(&db.super_pool)
        .await
        .expect("seed capability 1");
        let second: Uuid = sqlx::query_scalar(
            "INSERT INTO capability (tenant_id, code, label, status)
             VALUES ($1, 'architecture', 'Architecture', 'active') RETURNING id",
        )
        .bind(seeded.tenant)
        .fetch_one(&db.super_pool)
        .await
        .expect("seed capability 2");
        sqlx::query(
            "INSERT INTO capability_agent
                 (tenant_id, capability_id, key, role, status, model_ref, max_iterations,
                  timeout_secs)
             VALUES ($1, $2, 'default', 'tool_agent', 'active', 'model:primary', 1, 1)",
        )
        .bind(seeded.tenant)
        .bind(first)
        .execute(&db.super_pool)
        .await
        .expect("seed agent");
        sqlx::query(
            "INSERT INTO capability_knowledge
                 (tenant_id, capability_id, pack_key, pack_version, required)
             VALUES ($1, $2, 'conveyor', 1, true)",
        )
        .bind(seeded.tenant)
        .bind(first)
        .execute(&db.super_pool)
        .await
        .expect("seed knowledge");
        sqlx::query(
            "INSERT INTO capability_relation
                 (tenant_id, from_capability_id, to_capability_id, kind)
             VALUES ($1, $2, $3, 'requires')",
        )
        .bind(seeded.tenant)
        .bind(first)
        .bind(second)
        .execute(&db.super_pool)
        .await
        .expect("seed relation");
        sqlx::query(
            "INSERT INTO work_package_memory
                 (tenant_id, project_id, card_id, run_id, kind, source, body)
             VALUES ($1, $2, $3, $4, 'fact', 'agent', 'seeded')",
        )
        .bind(seeded.tenant)
        .bind(seeded.project)
        .bind(seeded.card)
        .bind(seeded.run)
        .execute(&db.super_pool)
        .await
        .expect("seed memory");
    }

    // 1. superuser: both tenants' rows exist (1 memory, 2 capabilities, 1 each of the rest).
    for table in tables {
        let count: i64 =
            sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
                .fetch_one(&db.super_pool)
                .await
                .expect("superuser count");
        let expected = if table == "capability" { 4 } else { 2 };
        assert_eq!(count, expected, "superuser sees every {table} row");
    }

    // 2. the schema owner with no scope sees nothing — that is what FORCE buys.
    for table in tables {
        let count: i64 =
            sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
                .fetch_one(&db.owner)
                .await
                .expect("owner count");
        assert_eq!(
            count, 0,
            "the owner must see 0 rows of {table} without a scope"
        );
    }

    // 3. runtime scoped to A: A's rows only, never B's.
    let tenant_a = TenantId::from_uuid(a.tenant).expect("non-nil");
    let mut tx = begin_scoped(&db.runtime, tenant_a)
        .await
        .expect("scope to A");
    for table in tables {
        let total: i64 =
            sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
                .fetch_one(&mut *tx)
                .await
                .expect("runtime count");
        let expected = if table == "capability" { 2 } else { 1 };
        assert_eq!(total, expected, "runtime scoped to A sees A's {table} rows");
        let foreign: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT count(*) FROM {table} WHERE tenant_id = $1"
        )))
        .bind(b.tenant)
        .fetch_one(&mut *tx)
        .await
        .expect("foreign count");
        assert_eq!(foreign, 0, "B's {table} rows must be invisible to A");
    }

    // 4. a forged tenant_id = B INSERT under A's scope fails the policy's WITH CHECK.
    let forged = sqlx::query(
        "INSERT INTO capability (tenant_id, code, label, status)
         VALUES ($1, 'forged', 'Forged', 'draft')",
    )
    .bind(b.tenant)
    .execute(&mut *tx)
    .await;
    assert!(
        forged.is_err(),
        "a forged tenant_id must fail the RLS WITH CHECK"
    );
    tx.rollback().await.expect("roll back");
}
