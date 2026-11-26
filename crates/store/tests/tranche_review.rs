//! T10 (#82) P4 — the end-of-tranche review is a programme decision.
//!
//! A review records continue, pause or stop with the baselines, the benefit status and the risk
//! register as evidence; `continue` authorises the next tranche in the same write, and a
//! project in an unauthorised tranche cannot start. A re-baseline is a change request, never a
//! side effect of the review.
//!
//! `cargo test -p conveyor-store --test tranche_review`.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_domain::spine::PlanError;
use conveyor_store::begin_scoped;
use conveyor_store::spine::{self, PlanStoreError, TranchePatch};
use serde_json::json;
use uuid::Uuid;

struct Fixture {
    tenant: TenantId,
    programme: Uuid,
    tranche0: Uuid,
    tranche1: Uuid,
    project: Uuid,
    reviewer: Uuid,
    baseline: Uuid,
    benefit: Uuid,
    risk: Uuid,
    change: Uuid,
}

fn evidence(f: &Fixture) -> serde_json::Value {
    json!({
        "baselines": [f.baseline],
        "benefit_status": [f.benefit],
        "risk_register": [f.risk],
    })
}

async fn fixture(db: &support::TestDb) -> Fixture {
    let tenant_id: Uuid =
        sqlx::query_scalar("INSERT INTO tenant (slug, name) VALUES ($1, 'Tranches') RETURNING id")
            .bind(format!("tranche-{}", Uuid::now_v7().simple()))
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
    let reviewer: Uuid = sqlx::query_scalar(
        "INSERT INTO app_user (oidc_subject, email, display_name)
         VALUES ($1, 'reviewer@dev.invalid', 'Reviewer') RETURNING id",
    )
    .bind(format!("reviewer-{}", Uuid::now_v7().simple()))
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the reviewer");
    let mut tranches = Vec::new();
    for number in 0..2 {
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO programme_tranche (tenant_id, programme_id, number)
             VALUES ($1, $2, $3) RETURNING id",
        )
        .bind(tenant_id)
        .bind(programme)
        .bind(number)
        .fetch_one(&db.super_pool)
        .await
        .expect("insert a tranche");
        tranches.push(id);
    }
    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name, programme_id, meta)
         VALUES ($1, $2, 'Delivery', $3, '{\"tranche\": 1}'::jsonb) RETURNING id",
    )
    .bind(tenant_id)
    .bind(format!("PRJ-{}", Uuid::now_v7().simple()))
    .bind(programme)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the project");
    let baseline: Uuid = sqlx::query_scalar(
        "INSERT INTO programme_baseline
             (tenant_id, programme_id, kind, snapshot, taken_by, approved_by)
         VALUES ($1, $2, 'management', '{}'::jsonb, $3, $3) RETURNING id",
    )
    .bind(tenant_id)
    .bind(programme)
    .bind(reviewer)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the approved baseline");
    let project_baseline: Uuid = sqlx::query_scalar(
        "INSERT INTO project_baseline (tenant_id, project_id, kind, snapshot, taken_by)
         VALUES ($1, $2, 'scope', '{}'::jsonb, $3) RETURNING id",
    )
    .bind(tenant_id)
    .bind(project)
    .bind(reviewer)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the project baseline");
    let change: Uuid = sqlx::query_scalar(
        "INSERT INTO project_register_item
             (tenant_id, project_id, kind, title, status, impact, baseline_ref, meta)
         VALUES ($1, $2, 'change', 'Re-baseline', 'approved', 2, $3,
                 '{\"impact\": {\"schedule\": \"a slip\"}}'::jsonb)
         RETURNING id",
    )
    .bind(tenant_id)
    .bind(project)
    .bind(project_baseline)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the approved change");
    let benefit: Uuid = sqlx::query_scalar(
        "INSERT INTO benefit_profile (tenant_id, programme_id, reference, description, status)
         VALUES ($1, $2, $3, 'A measured benefit', 'active') RETURNING id",
    )
    .bind(tenant_id)
    .bind(programme)
    .bind(format!("B-{}", Uuid::now_v7().simple()))
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the benefit profile");
    let risk: Uuid = sqlx::query_scalar(
        "INSERT INTO project_register_item (tenant_id, project_id, kind, title, status)
         VALUES ($1, $2, 'risk', 'A delivery risk', 'open') RETURNING id",
    )
    .bind(tenant_id)
    .bind(project)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the risk register item");
    Fixture {
        tenant,
        programme,
        tranche0: tranches[0],
        tranche1: tranches[1],
        project,
        reviewer,
        baseline,
        benefit,
        risk,
        change,
    }
}

async fn review(
    db: &support::TestDb,
    f: &Fixture,
    tranche: Uuid,
    decision: &str,
) -> Result<(), PlanStoreError> {
    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let outcome = spine::review_tranche_tx(
        &mut tx,
        f.tenant,
        tranche,
        decision,
        &evidence(f),
        f.reviewer,
    )
    .await
    .map(|_| ());
    match &outcome {
        Ok(()) => tx.commit().await.expect("commit"),
        Err(_) => tx.rollback().await.expect("rollback"),
    }
    outcome
}

async fn tranche_state(
    db: &support::TestDb,
    _tenant: TenantId,
    tranche: Uuid,
) -> (String, Option<Uuid>) {
    sqlx::query_as("SELECT state, authorisation_review_id FROM programme_tranche WHERE id = $1")
        .bind(tranche)
        .fetch_one(&db.super_pool)
        .await
        .expect("read the tranche")
}

#[tokio::test]
async fn a_review_records_continue_pause_or_stop() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db).await;

    review(&db, &f, f.tranche0, "continue")
        .await
        .expect("the review continues");
    let (state, review_id) = tranche_state(&db, f.tenant, f.tranche0).await;
    assert_eq!(state, "closed", "the reviewed tranche closes");
    assert!(
        review_id.is_none(),
        "the reviewed tranche is not 'authorised'"
    );
    let (next_state, review_id) = tranche_state(&db, f.tenant, f.tranche1).await;
    assert_eq!(
        next_state, "authorised",
        "continue authorises the next tranche"
    );
    assert!(review_id.is_some(), "the authorisation names its review");

    // The other decisions are the other outcomes, each on its own tranche: a tranche close has
    // one review, so a second decision over the same row is refused by `review_refuses_a_second_review`.
    let paused = fixture(&db).await;
    review(&db, &paused, paused.tranche0, "pause")
        .await
        .expect("the review pauses");
    assert_eq!(
        tranche_state(&db, paused.tenant, paused.tranche0).await.0,
        "paused"
    );
    let stopped = fixture(&db).await;
    review(&db, &stopped, stopped.tranche0, "stop")
        .await
        .expect("the review stops");
    assert_eq!(
        tranche_state(&db, stopped.tenant, stopped.tranche0).await.0,
        "stopped"
    );
}

#[tokio::test]
async fn a_review_without_evidence_is_refused() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db).await;
    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let refused = spine::review_tranche_tx(
        &mut tx,
        f.tenant,
        f.tranche0,
        "continue",
        &json!({ "baselines": {} }),
        f.reviewer,
    )
    .await;
    tx.rollback().await.expect("rollback");
    assert!(
        matches!(
            refused,
            Err(PlanStoreError::Illegal(
                PlanError::TrancheReviewEvidenceMissing
            ))
        ),
        "a review with no benefit status or risk register is refused: {refused:?}"
    );
    let reviews: i64 =
        sqlx::query_scalar("SELECT count(*) FROM programme_tranche_review WHERE tenant_id = $1")
            .bind(f.tenant.as_uuid())
            .fetch_one(&db.super_pool)
            .await
            .expect("count reviews");
    assert_eq!(reviews, 0, "the refusal writes no decision row");
}

#[tokio::test]
async fn a_rebaseline_goes_through_change_control() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db).await;

    // The review never touches the baseline.
    review(&db, &f, f.tranche0, "continue")
        .await
        .expect("the review continues");
    let (baseline,): (Option<Uuid>,) =
        sqlx::query_as("SELECT baseline_ref FROM programme_tranche WHERE id = $1")
            .bind(f.tranche0)
            .fetch_one(&db.super_pool)
            .await
            .expect("read the tranche");
    assert!(baseline.is_none(), "the review leaves the baseline alone");

    // An edit that sets a baseline cites the approved change. The review bumped the row's
    // version, so the edit names the version it actually read.
    let (current,): (i64,) = sqlx::query_as("SELECT version FROM programme_tranche WHERE id = $1")
        .bind(f.tranche0)
        .fetch_one(&db.super_pool)
        .await
        .expect("read the tranche version");
    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let refused = spine::update_tranche_tx(
        &mut tx,
        f.tenant,
        f.tranche0,
        current,
        &TranchePatch {
            baseline_ref: Some(f.baseline),
            ..TranchePatch::default()
        },
    )
    .await;
    tx.rollback().await.expect("rollback");
    assert!(
        matches!(
            refused,
            Err(PlanStoreError::Illegal(PlanError::RebaselineRequiresChange))
        ),
        "a re-baseline without a change request is refused: {refused:?}"
    );

    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let updated = spine::update_tranche_tx(
        &mut tx,
        f.tenant,
        f.tranche0,
        current,
        &TranchePatch {
            baseline_ref: Some(f.baseline),
            change_ref: Some(f.change),
            ..TranchePatch::default()
        },
    )
    .await
    .expect("the approved change authorises the new baseline");
    tx.commit().await.expect("commit");
    assert_eq!(updated.baseline_ref, Some(f.baseline));
}

#[tokio::test]
async fn a_project_cannot_start_before_its_tranche_is_authorised() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db).await;

    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let refused = spine::assert_project_tranche_authorised_tx(&mut tx, f.tenant, f.project).await;
    tx.rollback().await.expect("rollback");
    assert!(
        matches!(
            refused,
            Err(PlanStoreError::Illegal(PlanError::TrancheNotAuthorised))
        ),
        "a project in a planned tranche cannot start: {refused:?}"
    );

    // The sponsoring group's review is the authorisation.
    review(&db, &f, f.tranche0, "continue")
        .await
        .expect("the review continues");
    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let allowed = spine::assert_project_tranche_authorised_tx(&mut tx, f.tenant, f.project).await;
    tx.commit().await.expect("commit");
    assert!(allowed.is_ok(), "the identical start proceeds: {allowed:?}");
    let _ = f.programme;
}

async fn review_count(db: &support::TestDb, tenant: TenantId, tranche: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM programme_tranche_review WHERE tenant_id = $1 AND tranche_id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(tranche)
    .fetch_one(&db.super_pool)
    .await
    .expect("count reviews")
}

/// A tranche close has one review: a second decision over the same row is refused and writes
/// nothing.
#[tokio::test]
async fn review_refuses_a_second_review() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db).await;
    review(&db, &f, f.tranche0, "continue")
        .await
        .expect("the first review");

    let refused = review(&db, &f, f.tranche0, "pause").await;
    assert!(
        matches!(
            refused,
            Err(PlanStoreError::Illegal(PlanError::TrancheAlreadyReviewed))
        ),
        "a tranche close has one review: {refused:?}"
    );
    assert_eq!(
        review_count(&db, f.tenant, f.tranche0).await,
        1,
        "the refusal writes no second review row"
    );
}

/// A closed or retired tranche is not open to a review.
#[tokio::test]
async fn review_refuses_a_closed_tranche() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db).await;
    sqlx::query("UPDATE programme_tranche SET state = 'closed' WHERE id = $1")
        .bind(f.tranche0)
        .execute(&db.super_pool)
        .await
        .expect("close the tranche");

    let refused = review(&db, &f, f.tranche0, "pause").await;
    assert!(
        matches!(
            refused,
            Err(PlanStoreError::Illegal(
                PlanError::TrancheNotReviewable { .. }
            ))
        ),
        "a closed tranche cannot be reviewed: {refused:?}"
    );
    assert_eq!(
        review_count(&db, f.tenant, f.tranche0).await,
        0,
        "the refusal writes no review row"
    );
}

/// A `continue` with no next tranche authorises nothing, so it refuses before writing.
#[tokio::test]
async fn continue_refuses_a_missing_next_tranche() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db).await;

    // The fixture's tranche 1 is the last one.
    let refused = review(&db, &f, f.tranche1, "continue").await;
    assert!(
        matches!(
            refused,
            Err(PlanStoreError::Illegal(PlanError::NextTrancheMissing))
        ),
        "a continue with nothing to authorise is refused: {refused:?}"
    );
    assert_eq!(
        review_count(&db, f.tenant, f.tranche1).await,
        0,
        "the refusal writes no review row"
    );
}

/// A `continue` whose next tranche is not `planned` is refused before the review is written.
#[tokio::test]
async fn continue_refuses_a_next_tranche_not_planned() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db).await;
    sqlx::query("UPDATE programme_tranche SET state = 'authorised' WHERE id = $1")
        .bind(f.tranche1)
        .execute(&db.super_pool)
        .await
        .expect("authorise the next tranche out of band");

    let refused = review(&db, &f, f.tranche0, "continue").await;
    match &refused {
        Err(PlanStoreError::Illegal(PlanError::NextTrancheNotPlanned { state })) => {
            assert_eq!(state, "authorised")
        }
        other => panic!("a continue needs a planned next tranche: {other:?}"),
    }
    assert_eq!(
        review_count(&db, f.tenant, f.tranche0).await,
        0,
        "the refusal writes no review row"
    );
}

/// `pause` and `stop` end the tranche; they do not need a next tranche to authorise.
#[tokio::test]
async fn pause_and_stop_do_not_need_a_next_tranche() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db).await;
    review(&db, &f, f.tranche1, "pause")
        .await
        .expect("pause needs no next tranche");
    assert_eq!(tranche_state(&db, f.tenant, f.tranche1).await.0, "paused");

    let stopped = fixture(&db).await;
    review(&db, &stopped, stopped.tranche1, "stop")
        .await
        .expect("stop needs no next tranche");
    assert_eq!(
        tranche_state(&db, stopped.tenant, stopped.tranche1).await.0,
        "stopped"
    );
}

/// A well-formed UUID is a citation only when it names a row of the reviewed programme. The
/// seed's old trick — a state with no row behind it — is exactly what this refuses.
#[tokio::test]
async fn a_review_refuses_evidence_ids_that_do_not_resolve() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db).await;

    for class in ["baselines", "benefit_status", "risk_register"] {
        let mut cited = evidence(&f);
        cited[class] = json!([Uuid::now_v7()]);
        let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
        let refused = spine::review_tranche_tx(
            &mut tx, f.tenant, f.tranche0, "continue", &cited, f.reviewer,
        )
        .await;
        tx.rollback().await.expect("rollback");
        match refused {
            Err(PlanStoreError::Illegal(PlanError::TrancheReviewEvidenceUnresolved {
                class: named,
            })) => assert_eq!(named, class),
            other => panic!("an unresolvable {class} id is refused: {other:?}"),
        }
    }
    assert_eq!(
        review_count(&db, f.tenant, f.tranche0).await,
        0,
        "the refusal writes no review row"
    );
}

/// A baseline cited by a review is the approval, not just a row: it must carry `approved_by`
/// and be one of the three managed kinds. A baseline of another programme is not this
/// tranche's evidence at all.
#[tokio::test]
async fn a_review_refuses_an_unapproved_or_wrong_kind_baseline() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db).await;

    let unapproved: Uuid = sqlx::query_scalar(
        "INSERT INTO programme_baseline (tenant_id, programme_id, kind, snapshot, taken_by)
         VALUES ($1, $2, 'management', '{}'::jsonb, $3) RETURNING id",
    )
    .bind(f.tenant.as_uuid())
    .bind(f.programme)
    .bind(f.reviewer)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert an unapproved baseline");
    let mut cited = evidence(&f);
    cited["baselines"] = json!([unapproved]);
    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let refused = spine::review_tranche_tx(
        &mut tx, f.tenant, f.tranche0, "continue", &cited, f.reviewer,
    )
    .await;
    tx.rollback().await.expect("rollback");
    match refused {
        Err(PlanStoreError::Illegal(PlanError::TrancheBaselineNotApproved { kind })) => {
            assert_eq!(kind, "management")
        }
        other => panic!("an unapproved baseline is refused: {other:?}"),
    }

    // The schema admits only the three managed kinds, so the reachable "wrong baseline" is one
    // approved in a different programme.
    let other: Uuid = sqlx::query_scalar(
        "INSERT INTO programme (tenant_id, name) VALUES ($1, 'Other') RETURNING id",
    )
    .bind(f.tenant.as_uuid())
    .fetch_one(&db.super_pool)
    .await
    .expect("insert a second programme");
    let foreign: Uuid = sqlx::query_scalar(
        "INSERT INTO programme_baseline
             (tenant_id, programme_id, kind, snapshot, taken_by, approved_by)
         VALUES ($1, $2, 'governance', '{}'::jsonb, $3, $3) RETURNING id",
    )
    .bind(f.tenant.as_uuid())
    .bind(other)
    .bind(f.reviewer)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert a foreign-programme baseline");
    let mut cited = evidence(&f);
    cited["baselines"] = json!([foreign]);
    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let refused = spine::review_tranche_tx(
        &mut tx, f.tenant, f.tranche0, "continue", &cited, f.reviewer,
    )
    .await;
    tx.rollback().await.expect("rollback");
    assert!(
        matches!(
            refused,
            Err(PlanStoreError::Illegal(
                PlanError::TrancheReviewEvidenceUnresolved { class: "baselines" }
            ))
        ),
        "a baseline of another programme is not evidence: {refused:?}"
    );
}

/// A closed tranche is terminal: the engine reaches it only through a `continue` review of the
/// tranche itself, and the programme's genesis tranche is seeded closed with no review at all.
/// A project in it must therefore close and start — the retrospective rework re-closes tranche
/// 0's projects, and requiring a link refused every one of those closes (2026-11-26).
#[tokio::test]
async fn a_closed_tranche_admits_its_projects_without_an_authorisation_review() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db).await;
    sqlx::query("UPDATE programme_tranche SET state = 'closed' WHERE id = $1")
        .bind(f.tranche1)
        .execute(&db.super_pool)
        .await
        .expect("close the tranche with no review");

    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let allowed = spine::assert_project_tranche_authorised_tx(&mut tx, f.tenant, f.project).await;
    tx.commit().await.expect("commit");
    assert!(
        allowed.is_ok(),
        "a closed tranche admits its projects: {allowed:?}"
    );
}

/// The state alone is not an authorisation: an `authorised` row with no review, or a review of
/// the previous tranche that did not continue, cannot start a project. The previous tranche's
/// `continue` review is the authorisation the engine writes.
#[tokio::test]
async fn a_project_cannot_start_on_a_tranche_without_an_authorisation_review() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db).await;
    sqlx::query("UPDATE programme_tranche SET state = 'authorised' WHERE id = $1")
        .bind(f.tranche1)
        .execute(&db.super_pool)
        .await
        .expect("authorise the tranche with no review");

    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let refused = spine::assert_project_tranche_authorised_tx(&mut tx, f.tenant, f.project).await;
    tx.rollback().await.expect("rollback");
    assert!(
        matches!(
            refused,
            Err(PlanStoreError::Illegal(PlanError::TrancheNotAuthorised))
        ),
        "an authorisation with no review is refused: {refused:?}"
    );

    // A review of the previous tranche that did NOT continue authorises nothing.
    let paused: Uuid = sqlx::query_scalar(
        "INSERT INTO programme_tranche_review
             (tenant_id, tranche_id, decision, evidence, reviewed_by)
         VALUES ($1, $2, 'pause', $3, $4) RETURNING id",
    )
    .bind(f.tenant.as_uuid())
    .bind(f.tranche0)
    .bind(evidence(&f))
    .bind(f.reviewer)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert a pause review of the previous tranche");
    sqlx::query("UPDATE programme_tranche SET authorisation_review_id = $2 WHERE id = $1")
        .bind(f.tranche1)
        .bind(paused)
        .execute(&db.super_pool)
        .await
        .expect("point the tranche at the pause review");
    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let refused = spine::assert_project_tranche_authorised_tx(&mut tx, f.tenant, f.project).await;
    tx.rollback().await.expect("rollback");
    assert!(
        matches!(
            refused,
            Err(PlanStoreError::Illegal(PlanError::TrancheNotAuthorised))
        ),
        "a pause review is not an authorisation: {refused:?}"
    );

    // The continue review of the previous tranche is the authorisation the engine writes.
    let previous: Uuid = sqlx::query_scalar(
        "INSERT INTO programme_tranche_review
             (tenant_id, tranche_id, decision, evidence, reviewed_by)
         VALUES ($1, $2, 'continue', $3, $4) RETURNING id",
    )
    .bind(f.tenant.as_uuid())
    .bind(f.tranche0)
    .bind(evidence(&f))
    .bind(f.reviewer)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the previous tranche's continue review");
    sqlx::query("UPDATE programme_tranche SET authorisation_review_id = $2 WHERE id = $1")
        .bind(f.tranche1)
        .bind(previous)
        .execute(&db.super_pool)
        .await
        .expect("link the authorisation to the previous review");
    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let allowed = spine::assert_project_tranche_authorised_tx(&mut tx, f.tenant, f.project).await;
    tx.commit().await.expect("commit");
    assert!(
        allowed.is_ok(),
        "the previous tranche's continue review authorises the start: {allowed:?}"
    );
}

/// The seed asserts no authorisation of its own: every seeded tranche starts `planned`, and a
/// non-planned row would have to name a review of itself.
#[tokio::test]
async fn seeded_non_planned_tranches_carry_a_review() {
    let db = support::TestDb::fresh().await;
    let seeded = support::seed_tenant(&db.super_pool, "msp-review").await;
    conveyor_store::seed_programme::seed_programme(&db.super_pool, Some("msp-review"))
        .await
        .expect("seed the definition");

    let total: i64 =
        sqlx::query_scalar("SELECT count(*) FROM programme_tranche WHERE tenant_id = $1")
            .bind(seeded.tenant)
            .fetch_one(&db.super_pool)
            .await
            .expect("count the seeded tranches");
    assert_eq!(total, 14, "the seed inserts the fourteen tranches");
    let unbacked: i64 = sqlx::query_scalar(
        "SELECT count(*)
           FROM programme_tranche t
          WHERE t.tenant_id = $1 AND t.state <> 'planned'
            AND (t.authorisation_review_id IS NULL
                 OR NOT EXISTS (
                     SELECT 1 FROM programme_tranche_review r
                      WHERE r.tenant_id = t.tenant_id
                        AND r.id = t.authorisation_review_id
                        AND r.tranche_id = t.id))",
    )
    .bind(seeded.tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("count tranches whose state has no review");
    assert_eq!(
        unbacked, 0,
        "a seeded non-planned tranche carries a review of itself"
    );
    let non_planned: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM programme_tranche
          WHERE tenant_id = $1 AND state <> 'planned'",
    )
    .bind(seeded.tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("count the non-planned tranches");
    assert_eq!(
        non_planned, 0,
        "the seed does not assert an authorisation without a review"
    );
}

/// SPEC_V2 §8.3 / T10 — the tranche's own baseline pointer is a citation only when the
/// baseline is this programme's, approved, and one of the three MSP kinds. An unapproved row
/// and another programme's row are both refused; an approved management baseline is accepted.
#[tokio::test]
async fn a_tranche_baseline_must_be_approved_and_in_its_programme() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db).await;
    let unapproved: Uuid = sqlx::query_scalar(
        "INSERT INTO programme_baseline
             (tenant_id, programme_id, kind, snapshot, taken_by)
         VALUES ($1, $2, 'governance', '{}'::jsonb, $3) RETURNING id",
    )
    .bind(f.tenant.as_uuid())
    .bind(f.programme)
    .bind(f.reviewer)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert an unapproved baseline");
    let foreign_programme: Uuid = sqlx::query_scalar(
        "INSERT INTO programme (tenant_id, name) VALUES ($1, 'Other') RETURNING id",
    )
    .bind(f.tenant.as_uuid())
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the other programme");
    let foreign: Uuid = sqlx::query_scalar(
        "INSERT INTO programme_baseline
             (tenant_id, programme_id, kind, snapshot, taken_by, approved_by)
         VALUES ($1, $2, 'management', '{}'::jsonb, $3, $3) RETURNING id",
    )
    .bind(f.tenant.as_uuid())
    .bind(foreign_programme)
    .bind(f.reviewer)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert a foreign approved baseline");

    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let tranche = spine::get_tranche_tx(&mut tx, f.tenant, f.tranche0)
        .await
        .expect("read the tranche")
        .expect("tranche exists");
    let patch = |baseline: Uuid| TranchePatch {
        entry_criteria: None,
        exit_criteria: None,
        baseline_ref: Some(baseline),
        review_at: None,
        change_ref: Some(f.change),
    };

    let refused = spine::update_tranche_tx(
        &mut tx,
        f.tenant,
        f.tranche0,
        tranche.version,
        &patch(unapproved),
    )
    .await;
    assert!(
        matches!(
            refused,
            Err(PlanStoreError::Illegal(
                PlanError::TrancheBaselineNotApproved { .. }
            ))
        ),
        "an unapproved baseline must be refused"
    );

    let refused = spine::update_tranche_tx(
        &mut tx,
        f.tenant,
        f.tranche0,
        tranche.version,
        &patch(foreign),
    )
    .await;
    assert!(
        matches!(refused, Err(PlanStoreError::NotFound { .. })),
        "another programme's baseline must be refused"
    );

    let accepted = spine::update_tranche_tx(
        &mut tx,
        f.tenant,
        f.tranche0,
        tranche.version,
        &patch(f.baseline),
    )
    .await
    .expect("an approved management baseline of this programme is accepted");
    assert_eq!(accepted.baseline_ref, Some(f.baseline));
    tx.commit().await.expect("commit");
}
