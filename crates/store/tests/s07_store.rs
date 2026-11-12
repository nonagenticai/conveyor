//! S07 store coverage: trace links, the project-request intake, the quality history and
//! the catalogue importer.
//!
//! One file, four modules, one fresh migrated database per test. Everything that writes runs
//! as `conveyor_runtime` in a [`begin_scoped`] transaction; the fixtures are superuser SQL,
//! because the superuser is the positive control that bypasses FORCE RLS.
//!
//! `cargo test -p conveyor-store --test s07_store -- --nocapture`.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::{TenantId, Version};
use conveyor_store::actions::{decode_time_cursor, encode_time_cursor};
use conveyor_store::begin_scoped;
use conveyor_store::catalogue::{
    CardTypeImport, CatalogueStoreError, ImportCounts, ImportEntry, import_card_types_tx,
    import_entries_tx, list_entries_tx,
};
use conveyor_store::project::{NewProject, get_project_tx};
use conveyor_store::project_request::{
    NewProjectRequest, ProjectRequestStoreError, ProjectRequestUpdate, approve_project_request_tx,
    create_project_request_tx, list_project_requests_tx, promote_project_request_tx,
    reject_project_request_tx, update_project_request_tx,
};
use conveyor_store::quality::{
    NewQualityEvaluation, QualityStoreError, canonical_pair, dismiss_conflict_tx,
    insert_evaluation_tx, insert_principle_conflict_tx, list_conflicts_tx, list_evaluations_tx,
    max_similarity_tx,
};
use conveyor_store::trace_link::{
    NewTraceLink, TraceFilter, TraceLinkStoreError, delete_trace_link_tx, insert_trace_link_tx,
    list_trace_links_tx,
};
use rust_decimal::Decimal;
use serde_json::json;
use uuid::Uuid;

/// S07 §5.7: a link is readable from both ends, both ends are resolved in the tenant, and a
/// duplicate edge or an unknown end type is a classified error.
#[tokio::test]
async fn trace_links_read_both_directions_and_resolve_both_ends_in_tenant() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "s07-trace").await;
    let tenant = TenantId::from_uuid(seed.tenant).expect("non-nil");

    // The fixture's own trace row uses from_type='requirement' (S03 legacy); the new writer
    // admits only governed_asset/card, so this edge is distinct from it.
    let link = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let link = insert_trace_link_tx(
            &mut tx,
            tenant,
            &NewTraceLink::new("governed_asset", seed.asset, "card", seed.card),
        )
        .await
        .expect("insert the trace link");
        tx.commit().await.expect("commit");
        link
    };
    assert_eq!(link.from_type, "governed_asset");
    assert_eq!(link.to_type, "card");
    assert_eq!(link.kind, "traces");

    // Direction 1: the edges the requirement emits.
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let from_links = list_trace_links_tx(
        &mut tx,
        tenant,
        &TraceFilter {
            from_type: Some("governed_asset".to_owned()),
            from_id: Some(seed.asset),
            ..TraceFilter::default()
        },
        50,
    )
    .await
    .expect("list by the from end");
    // Direction 2: the edges that point at the card. The fixture row is in this set too.
    let to_links = list_trace_links_tx(
        &mut tx,
        tenant,
        &TraceFilter {
            to_type: Some("card".to_owned()),
            to_id: Some(seed.card),
            ..TraceFilter::default()
        },
        50,
    )
    .await
    .expect("list by the to end");
    tx.rollback().await.expect("roll back the read");

    assert_eq!(
        from_links.len(),
        1,
        "the requirement emits our one new edge"
    );
    assert_eq!(from_links[0].id, link.id);
    println!(
        "from-direction returned {} row(s); to-direction returned {}",
        from_links.len(),
        to_links.len()
    );
    assert!(
        to_links.iter().any(|row| row.id == link.id),
        "the same row is readable from the other end: {to_links:?}"
    );

    // A duplicate (tenant, from, to, kind) is a conflict, not a 500.
    let duplicate = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let duplicate = insert_trace_link_tx(
            &mut tx,
            tenant,
            &NewTraceLink::new("governed_asset", seed.asset, "card", seed.card),
        )
        .await;
        tx.rollback().await.expect("roll back the refusal");
        duplicate
    };
    assert!(
        matches!(duplicate, Err(TraceLinkStoreError::Conflict)),
        "duplicate edge: {duplicate:?}"
    );

    // The whitelist is the store's, not the client's.
    let unknown = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let unknown = insert_trace_link_tx(
            &mut tx,
            tenant,
            &NewTraceLink::new("requirement", seed.asset, "card", seed.card),
        )
        .await;
        tx.rollback().await.expect("roll back the refusal");
        unknown
    };
    assert!(
        matches!(unknown, Err(TraceLinkStoreError::UnknownType(ref t)) if t == "requirement"),
        "unknown end type: {unknown:?}"
    );

    // A second tenant proves the end resolution is tenant-scoped, both directions.
    let other = support::seed_tenant(&db.super_pool, "s07-trace-b").await;
    let cross_to = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let cross = insert_trace_link_tx(
            &mut tx,
            tenant,
            &NewTraceLink::new("governed_asset", seed.asset, "card", other.card),
        )
        .await;
        tx.rollback().await.expect("roll back the refusal");
        cross
    };
    assert!(
        matches!(
            cross_to,
            Err(TraceLinkStoreError::EndNotFound { side: "to", id }) if id == other.card
        ),
        "a card in another tenant must not resolve: {cross_to:?}"
    );
    let cross_from = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let cross = insert_trace_link_tx(
            &mut tx,
            tenant,
            &NewTraceLink::new("governed_asset", other.asset, "card", seed.card),
        )
        .await;
        tx.rollback().await.expect("roll back the refusal");
        cross
    };
    assert!(
        matches!(
            cross_from,
            Err(TraceLinkStoreError::EndNotFound { side: "from", id }) if id == other.asset
        ),
        "an asset in another tenant must not resolve: {cross_from:?}"
    );

    // Delete is idempotent in the sense the caller needs: true once, false afterwards.
    let deleted = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let deleted = delete_trace_link_tx(&mut tx, tenant, link.id)
            .await
            .expect("delete the link");
        tx.commit().await.expect("commit");
        deleted
    };
    let deleted_again = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let deleted = delete_trace_link_tx(&mut tx, tenant, link.id)
            .await
            .expect("delete the absent link");
        tx.commit().await.expect("commit");
        deleted
    };
    assert!(deleted, "the first delete removes the row");
    assert!(!deleted_again, "the second delete reports absence");
}

/// S07 §5.4/§5.7: idempotent create, a promoted row is a 409 carrying its project, and
/// approve → promote writes the project **in the same transaction**.
#[tokio::test]
async fn project_requests_are_idempotent_then_promote_in_one_transaction() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "s07-pr").await;
    let tenant = TenantId::from_uuid(seed.tenant).expect("non-nil");

    let mut intake = NewProjectRequest::new("cp-1", "control-plane");
    intake.payload = json!({"title": "First intake", "scope": "v1"});
    intake.repository_url = Some("https://example.test/first.git".to_owned());

    let first = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let first = create_project_request_tx(&mut tx, tenant, &intake)
            .await
            .expect("create");
        tx.commit().await.expect("commit");
        first
    };
    assert_eq!(first.status, "draft");
    assert_eq!(first.version.get(), 1);

    // Re-posting the same external_ref updates in place and bumps the version.
    let mut revision = NewProjectRequest::new("cp-1", "control-plane-v2");
    revision.payload = json!({"title": "First intake, revised"});
    let second = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let second = create_project_request_tx(&mut tx, tenant, &revision)
            .await
            .expect("re-post");
        tx.commit().await.expect("commit");
        second
    };
    assert_eq!(second.id, first.id, "idempotent on (tenant, external_ref)");
    assert_eq!(second.version.get(), 2, "a re-post bumps the version");
    assert_eq!(second.payload["title"], "First intake, revised");
    assert_eq!(second.external_origin, "control-plane-v2");
    assert_eq!(second.repository_url, None, "the re-post replaced the row");

    // A version-guarded edit: fresh version applies, the stale one returns None.
    let patch = ProjectRequestUpdate {
        payload: Some(json!({"title": "Edited intake"})),
        repository_url: Some(Some("https://example.test/second.git".to_owned())),
        ..Default::default()
    };
    let edited = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let edited = update_project_request_tx(&mut tx, tenant, first.id, &patch, second.version)
            .await
            .expect("update runs");
        tx.commit().await.expect("commit");
        edited
    }
    .expect("the fresh version applies");
    assert_eq!(edited.version.get(), 3);
    assert_eq!(edited.payload["title"], "Edited intake");
    assert_eq!(
        edited.repository_url.as_deref(),
        Some("https://example.test/second.git")
    );
    let stale = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let stale = update_project_request_tx(&mut tx, tenant, first.id, &patch, Version::INITIAL)
            .await
            .expect("stale update runs");
        tx.rollback().await.expect("roll back");
        stale
    };
    assert!(stale.is_none(), "a stale version writes nothing");

    // Approve once; a second approve is an illegal transition.
    let approved = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let approved = approve_project_request_tx(&mut tx, tenant, first.id)
            .await
            .expect("approve");
        tx.commit().await.expect("commit");
        approved
    };
    assert_eq!(approved.status, "approved");
    assert_eq!(approved.version.get(), 4);
    let re_approved = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let re_approved = approve_project_request_tx(&mut tx, tenant, first.id).await;
        tx.rollback().await.expect("roll back");
        re_approved
    };
    assert!(
        matches!(
            re_approved,
            Err(ProjectRequestStoreError::Illegal { ref from, to: "approved" })
                if from == "approved"
        ),
        "approve from approved: {re_approved:?}"
    );

    // Promote: the project is created through the store inside this transaction, the actor
    // owns it because the caller left the owner empty, and the request carries the repository.
    let mut project_in = NewProject::new("s07-promoted", "Promoted from intake");
    project_in.description = "Created by promote".to_owned();
    let (promoted, project) = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let promoted =
            promote_project_request_tx(&mut tx, tenant, first.id, &project_in, Some(seed.user))
                .await
                .expect("promote");
        let project = get_project_tx(&mut tx, tenant, promoted.project_id.expect("project id"))
            .await
            .expect("read the new project")
            .expect("the project is visible in the same transaction");
        tx.commit().await.expect("commit the promotion");
        (promoted, project)
    };
    assert_eq!(promoted.status, "promoted");
    assert_eq!(promoted.version.get(), 5);
    assert_eq!(project.key, "s07-promoted");
    assert_eq!(project.name, "Promoted from intake");
    assert_eq!(
        project.owner_user_id,
        Some(seed.user),
        "the promoting actor owns a project the caller left unowned"
    );
    assert_eq!(
        project.repo_url.as_deref(),
        Some("https://example.test/second.git"),
        "the request's repository_url is carried into the project"
    );

    // Re-posting a promoted request is a conflict carrying the project id.
    let after_promotion = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let after_promotion = create_project_request_tx(&mut tx, tenant, &intake).await;
        tx.rollback().await.expect("roll back");
        after_promotion
    };
    match after_promotion {
        Err(ProjectRequestStoreError::Promoted { project_id }) => {
            assert_eq!(project_id, Some(project.id));
        }
        other => panic!(
            "expected Promoted carrying {project_id:?}, got {other:?}",
            project_id = project.id
        ),
    }

    // Promote is status-guarded: a draft cannot be promoted, and a rejected intake re-opens
    // as a draft when the bridge re-posts it.
    let draft = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let draft = create_project_request_tx(
            &mut tx,
            tenant,
            &NewProjectRequest::new("cp-2", "control-plane"),
        )
        .await
        .expect("create the second intake");
        tx.commit().await.expect("commit");
        draft
    };
    let refused = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let refused = promote_project_request_tx(
            &mut tx,
            tenant,
            draft.id,
            &NewProject::new("s07-refused", "Never created"),
            None,
        )
        .await;
        tx.rollback().await.expect("roll back the refusal");
        refused
    };
    assert!(
        matches!(
            refused,
            Err(ProjectRequestStoreError::Illegal { ref from, to: "promoted" })
                if from == "draft"
        ),
        "promote from draft: {refused:?}"
    );

    let rejected = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let rejected = reject_project_request_tx(&mut tx, tenant, draft.id)
            .await
            .expect("reject");
        tx.commit().await.expect("commit");
        rejected
    };
    assert_eq!(rejected.status, "rejected");
    let reopened = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let reopened = create_project_request_tx(
            &mut tx,
            tenant,
            &NewProjectRequest::new("cp-2", "control-plane"),
        )
        .await
        .expect("re-post re-opens the rejected intake");
        tx.commit().await.expect("commit");
        reopened
    };
    assert_eq!(reopened.id, draft.id);
    assert_eq!(
        reopened.status, "draft",
        "a re-post re-opens a rejected row"
    );
    assert!(reopened.version > rejected.version);

    // The list filter, its cursor codec, and the status vocabulary guard.
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let page_one = list_project_requests_tx(&mut tx, tenant, None, None, 1)
        .await
        .expect("page one");
    assert_eq!(page_one.len(), 1);
    let cursor = decode_time_cursor(&encode_time_cursor(page_one[0].created_at, page_one[0].id))
        .expect("the cursor round-trips through the shared codec");
    let page_two = list_project_requests_tx(&mut tx, tenant, None, Some(cursor), 1)
        .await
        .expect("page two");
    assert_eq!(page_two.len(), 1);
    assert_ne!(page_one[0].id, page_two[0].id, "the cursor moved the page");
    let promoted_only = list_project_requests_tx(&mut tx, tenant, Some("promoted"), None, 50)
        .await
        .expect("filter by status");
    assert!(promoted_only.iter().all(|row| row.status == "promoted"));
    assert!(promoted_only.iter().any(|row| row.id == first.id));
    let unknown = list_project_requests_tx(&mut tx, tenant, Some("nope"), None, 50).await;
    assert!(
        matches!(unknown, Err(ProjectRequestStoreError::UnknownStatus(ref s)) if s == "nope"),
        "unknown status filter: {unknown:?}"
    );
    tx.rollback().await.expect("roll back the reads");
}

/// S07 §5.5: append-only evaluations, the trigram read, and the direction-insensitive pair.
#[tokio::test]
async fn quality_evaluations_append_and_similarity_is_read_in_tenant() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "s07-q").await;
    let tenant = TenantId::from_uuid(seed.tenant).expect("non-nil");

    // Two scores for one revision: both rows are readable, newest first.
    let (first, second) = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let mut first = NewQualityEvaluation::new("revision", seed.asset);
        first.score = Some(Decimal::new(875, 1));
        first.criteria = json!({"rubric": "v1", "clarity": {"raw": 4, "weighted": 0.8}});
        let first = insert_evaluation_tx(&mut tx, tenant, &first)
            .await
            .expect("insert the first score");
        let mut second = NewQualityEvaluation::new("revision", seed.asset);
        second.score = Some(Decimal::new(750, 1));
        second.evaluator_kind = "model".to_owned();
        let second = insert_evaluation_tx(&mut tx, tenant, &second)
            .await
            .expect("insert the second score");
        tx.commit().await.expect("commit");
        (first, second)
    };
    assert_eq!(first.score, Some(Decimal::new(875, 1)));
    assert_eq!(first.evaluator_kind, "heuristic");
    assert_eq!(second.evaluator_kind, "model");

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let history = list_evaluations_tx(&mut tx, tenant, "revision", seed.asset, 50)
        .await
        .expect("read the history");
    tx.rollback().await.expect("roll back the read");
    assert_eq!(history.len(), 2, "append-only: both scores are kept");
    assert_eq!(history[0].id, second.id, "newest first");
    assert_eq!(history[1].id, first.id);
    assert_eq!(history[0].score, Some(Decimal::new(750, 1)));
    assert_eq!(history[1].criteria["rubric"], "v1");

    // The trigram read: two near-identical titles in a clean tenant, then the exclusion.
    let sim_tenant_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ('s07-sim', 'Similarity') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("seed the similarity tenant");
    let sim_tenant = TenantId::from_uuid(sim_tenant_id).expect("non-nil");
    let one: Uuid = sqlx::query_scalar(
        "INSERT INTO governed_asset (tenant_id, kind, key, title)
         VALUES ($1, 'requirement', 'sim-1', $2) RETURNING id",
    )
    .bind(sim_tenant_id)
    .bind("The system must never lose a committed write")
    .fetch_one(&db.super_pool)
    .await
    .expect("seed the first asset");
    let two: Uuid = sqlx::query_scalar(
        "INSERT INTO governed_asset (tenant_id, kind, key, title)
         VALUES ($1, 'requirement', 'sim-2', $2) RETURNING id",
    )
    .bind(sim_tenant_id)
    .bind("The system must never lose a committed write.")
    .fetch_one(&db.super_pool)
    .await
    .expect("seed the second asset");

    let mut tx = begin_scoped(&db.runtime, sim_tenant).await.expect("scope");
    let similarity = max_similarity_tx(
        &mut tx,
        sim_tenant,
        "requirement",
        "The system must never lose a committed write",
        two,
    )
    .await
    .expect("similarity");
    let other_similarity = max_similarity_tx(
        &mut tx,
        sim_tenant,
        "requirement",
        "The system must never lose a committed write",
        one,
    )
    .await
    .expect("similarity, other side");
    let empty_kind = max_similarity_tx(
        &mut tx,
        sim_tenant,
        "principle",
        "The system must never lose a committed write",
        Uuid::now_v7(),
    )
    .await
    .expect("similarity over an empty kind");
    tx.rollback().await.expect("roll back the reads");
    println!(
        "similarity: ex-one={similarity:.4} ex-two={other_similarity:.4} empty-kind={empty_kind}"
    );
    assert!(
        similarity > 0.5,
        "a near-identical title must score above 0.5, got {similarity}"
    );
    assert!(
        other_similarity > 0.5,
        "the exclusion is symmetric, got {other_similarity}"
    );
    assert_eq!(empty_kind, 0.0, "no rows coalesce to zero");

    // principle_conflict: canonical pairs, one verdict per pair in either direction.
    let principle_a: Uuid = sqlx::query_scalar(
        "INSERT INTO governed_asset (tenant_id, kind, key, title)
         VALUES ($1, 'principle', 'pr-a', 'Principle A') RETURNING id",
    )
    .bind(seed.tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed principle A");
    let principle_b: Uuid = sqlx::query_scalar(
        "INSERT INTO governed_asset (tenant_id, kind, key, title)
         VALUES ($1, 'principle', 'pr-b', 'Principle B') RETURNING id",
    )
    .bind(seed.tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed principle B");
    let (low, high) = canonical_pair(principle_a, principle_b);
    assert_eq!(canonical_pair(principle_b, principle_a), (low, high));
    assert!(low < high);

    let conflict = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let conflict =
            insert_principle_conflict_tx(&mut tx, tenant, principle_a, principle_b, "contradicts")
                .await
                .expect("insert the verdict");
        tx.commit().await.expect("commit");
        conflict
    };
    assert_eq!(conflict.principle_a_id, low, "lower uuid first");
    assert_eq!(conflict.principle_b_id, high);

    let duplicate = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let duplicate =
            insert_principle_conflict_tx(&mut tx, tenant, principle_b, principle_a, "contradicts")
                .await;
        tx.rollback().await.expect("roll back the refusal");
        duplicate
    };
    assert!(
        matches!(duplicate, Err(QualityStoreError::Conflict)),
        "the reversed pair is the same conflict: {duplicate:?}"
    );

    let missing = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let missing = insert_principle_conflict_tx(
            &mut tx,
            tenant,
            Uuid::now_v7(),
            principle_a,
            "contradicts",
        )
        .await;
        tx.rollback().await.expect("roll back the refusal");
        missing
    };
    assert!(
        matches!(missing, Err(QualityStoreError::PrincipleNotFound { .. })),
        "an unknown principle: {missing:?}"
    );

    let dismissed = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let dismissed = dismiss_conflict_tx(&mut tx, tenant, conflict.id, Some(seed.user))
            .await
            .expect("dismiss");
        tx.commit().await.expect("commit");
        dismissed
    }
    .expect("a live verdict is dismissed");
    assert_eq!(dismissed.dismissed_by, Some(seed.user));
    assert!(dismissed.dismissed_at.is_some());
    let re_dismissed = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let re_dismissed = dismiss_conflict_tx(&mut tx, tenant, conflict.id, Some(seed.user))
            .await
            .expect("dismiss again");
        tx.rollback().await.expect("roll back");
        re_dismissed
    };
    assert!(
        re_dismissed.is_none(),
        "a verdict is dismissed exactly once"
    );

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let listed = list_conflicts_tx(&mut tx, tenant, Some("contradicts"), None, 50)
        .await
        .expect("list the verdicts");
    let empty = list_conflicts_tx(&mut tx, tenant, Some("no-such-verdict"), None, 50)
        .await
        .expect("list an empty verdict filter");
    tx.rollback().await.expect("roll back the reads");
    assert!(listed.iter().any(|row| row.id == conflict.id));
    assert_eq!(empty.len(), 0);
}

/// S07 §5.4: the importer counts created/updated/skipped honestly, and a second identical
/// run writes nothing.
#[tokio::test]
async fn catalogue_import_counts_created_updated_and_skipped_honestly() {
    let db = support::TestDb::fresh().await;
    let tenant_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ('s07-cat', 'Catalogue') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("seed the catalogue tenant");
    let tenant = TenantId::from_uuid(tenant_id).expect("non-nil");

    let mut phase = ImportEntry::new("phase", "do", "Doing");
    phase.meta = json!({"order": 2});
    let mut project_type = ImportEntry::new("project_type", "bug_fix", "Bug fix");
    project_type.meta = json!({
        "phase_order": ["intake", "active", "done"],
        "matrix": {"bug_fix": {"intake": ["active"], "active": ["done"]}}
    });
    let entries = vec![
        ImportEntry::new("gate", "review", "Review"),
        phase,
        project_type,
    ];

    let first = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let first = import_entries_tx(&mut tx, tenant, &entries)
            .await
            .expect("first import");
        tx.commit().await.expect("commit");
        first
    };
    assert_eq!(
        first,
        ImportCounts {
            created: 3,
            updated: 0,
            skipped: 0
        }
    );

    let second = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let second = import_entries_tx(&mut tx, tenant, &entries)
            .await
            .expect("identical import");
        tx.commit().await.expect("commit");
        second
    };
    assert_eq!(
        second,
        ImportCounts {
            created: 0,
            updated: 0,
            skipped: 3
        },
        "an identical import writes nothing"
    );

    let mut renamed = entries.clone();
    renamed[1].label = "Doing (renamed)".to_owned();
    let third = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let third = import_entries_tx(&mut tx, tenant, &renamed)
            .await
            .expect("one changed label");
        tx.commit().await.expect("commit");
        third
    };
    assert_eq!(
        third,
        ImportCounts {
            created: 0,
            updated: 1,
            skipped: 2
        }
    );
    let (stored_label, stored_version): (String, i64) = sqlx::query_as(
        "SELECT label, version FROM catalogue_entry
          WHERE tenant_id = $1 AND kind = 'phase' AND code = 'do'",
    )
    .bind(tenant_id)
    .fetch_one(&db.super_pool)
    .await
    .expect("read the renamed row");
    assert_eq!(stored_label, "Doing (renamed)");
    assert_eq!(stored_version, 2, "an update bumps the version once");

    // Retiring a row is a change; the list honours include_inactive.
    let mut deactivated = renamed.clone();
    deactivated[0].is_active = false;
    let fourth = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let fourth = import_entries_tx(&mut tx, tenant, &deactivated)
            .await
            .expect("one retired row");
        tx.commit().await.expect("commit");
        fourth
    };
    assert_eq!(
        fourth,
        ImportCounts {
            created: 0,
            updated: 1,
            skipped: 2
        }
    );

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let active_gates = list_entries_tx(&mut tx, tenant, Some("gate"), false, 50)
        .await
        .expect("list the active gates");
    let all_gates = list_entries_tx(&mut tx, tenant, Some("gate"), true, 50)
        .await
        .expect("list all gates");
    let everything = list_entries_tx(&mut tx, tenant, None, true, 50)
        .await
        .expect("list the catalogue");
    let unknown = list_entries_tx(&mut tx, tenant, Some("not_a_kind"), true, 50).await;
    tx.rollback().await.expect("roll back the reads");
    assert!(active_gates.is_empty(), "the retired gate is hidden");
    assert_eq!(all_gates.len(), 1);
    assert_eq!(all_gates[0].code.as_str(), "review");
    assert_eq!(all_gates[0].version.get(), 2, "the retirement bumped it");
    assert_eq!(everything.len(), 3);
    assert!(
        matches!(unknown, Err(CatalogueStoreError::UnknownKind(ref k)) if k == "not_a_kind"),
        "unknown kind filter: {unknown:?}"
    );

    // Validation runs before the first statement: a bad line cannot half-import.
    let mixed = vec![
        ImportEntry::new("gate", "half_written", "Never inserted"),
        ImportEntry::new("gate", "BAD CODE", "Invalid"),
    ];
    let invalid = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let invalid = import_entries_tx(&mut tx, tenant, &mixed).await;
        tx.rollback().await.expect("roll back the refusal");
        invalid
    };
    assert!(
        matches!(invalid, Err(CatalogueStoreError::InvalidCode(_))),
        "invalid code: {invalid:?}"
    );
    let half_written: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM catalogue_entry
          WHERE tenant_id = $1 AND code = 'half_written'",
    )
    .bind(tenant_id)
    .fetch_one(&db.super_pool)
    .await
    .expect("count the half-written row");
    assert_eq!(half_written, 0, "validation precedes every statement");

    // card_type is its own table with its own unique key; same honest counting.
    let types = vec![
        CardTypeImport::new("feature", "Feature"),
        CardTypeImport::new("bug_fix", "Bug fix"),
    ];
    let types_created = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let counts = import_card_types_tx(&mut tx, tenant, &types)
            .await
            .expect("import card types");
        tx.commit().await.expect("commit");
        counts
    };
    assert_eq!(
        types_created,
        ImportCounts {
            created: 2,
            updated: 0,
            skipped: 0
        }
    );
    let types_again = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let counts = import_card_types_tx(&mut tx, tenant, &types)
            .await
            .expect("re-import card types");
        tx.commit().await.expect("commit");
        counts
    };
    assert_eq!(
        types_again,
        ImportCounts {
            created: 0,
            updated: 0,
            skipped: 2
        }
    );
    let mut changed_types = types.clone();
    changed_types[1].label = "Bug fix (renamed)".to_owned();
    changed_types[1].definition = json!({"inputs": ["repo"], "gates": ["human_approval"]});
    let types_changed = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let counts = import_card_types_tx(&mut tx, tenant, &changed_types)
            .await
            .expect("change one card type");
        tx.commit().await.expect("commit");
        counts
    };
    assert_eq!(
        types_changed,
        ImportCounts {
            created: 0,
            updated: 1,
            skipped: 1
        }
    );
    let (type_label, type_version): (String, i64) = sqlx::query_as(
        "SELECT label, version FROM card_type WHERE tenant_id = $1 AND code = 'bug_fix'",
    )
    .bind(tenant_id)
    .fetch_one(&db.super_pool)
    .await
    .expect("read the changed card type");
    assert_eq!(type_label, "Bug fix (renamed)");
    assert_eq!(type_version, 2);
}
