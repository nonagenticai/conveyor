//! T5 (#77) — the kata loop's store half (SPEC_V2 §6.3, §9.3–§9.4, §12 T5;
//! `docs/plans/inception/T5/02-architecture-design.md`).
//!
//! The acceptance the store can prove: an unmeasurable kata is refused with no row; one open
//! kata per signature; a candidate cannot be measured without a prediction; only the held-out
//! split on the pinned instrument version and sha measures; adoption is another person's human
//! decision (a machine-only decision and the kata's own opener are refused); and a revert
//! without its restore is refused.
//!
//! `cargo test -p conveyor-store --test kata`.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_store::begin_scoped;
use conveyor_store::kata::{
    self, ADOPT_GATE, CandidateRecord, KataError, KataRecord, KataStoreError, MeasurementRecord,
    PROPAGATE_GATE, PropagationRecord,
};
use serde_json::{Value, json};
use uuid::Uuid;

struct Fixture {
    tenant: TenantId,
    project: Uuid,
    opener: Uuid,
    approver: Uuid,
}

async fn user(db: &support::TestDb, name: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO app_user (display_name) VALUES ($1) RETURNING id")
        .bind(format!("{name}-{}", Uuid::now_v7().simple()))
        .fetch_one(&db.super_pool)
        .await
        .expect("insert an app_user")
}

async fn fixture(db: &support::TestDb) -> Fixture {
    let tenant_id: Uuid =
        sqlx::query_scalar("INSERT INTO tenant (slug, name) VALUES ($1, 'Kata') RETURNING id")
            .bind(format!("kata-{}", Uuid::now_v7().simple()))
            .fetch_one(&db.super_pool)
            .await
            .expect("insert the tenant");
    let tenant = TenantId::from_uuid(tenant_id).expect("tenant id");
    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name) VALUES ($1, $2, 'Kata project') RETURNING id",
    )
    .bind(tenant_id)
    .bind(format!("kata-{}", Uuid::now_v7().simple()))
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the kata project");
    Fixture {
        tenant,
        project,
        opener: user(db, "opener").await,
        approver: user(db, "approver").await,
    }
}

fn prediction() -> Value {
    json!({ "metric": "pass_rate", "at_least": 0.8 })
}

fn base_ref() -> Value {
    json!({
        "kind": "procedure",
        "id": Uuid::now_v7().to_string(),
        "base_version": "v1",
    })
}

/// The standard measurable open arguments.
struct OpenArgs<'a> {
    signature: &'a str,
    instrument_kind: &'a str,
    instrument_id: Option<Uuid>,
    instrument_version: &'a str,
    split: &'a str,
    split_sha256: &'a str,
    prediction: Value,
    base_ref: Value,
}

fn args(signature: &str) -> OpenArgs<'_> {
    OpenArgs {
        signature,
        instrument_kind: "eval",
        instrument_id: Some(Uuid::now_v7()),
        instrument_version: "eval-v3",
        split: "test",
        split_sha256: "sha-locked-v3",
        prediction: prediction(),
        base_ref: base_ref(),
    }
}

async fn open_with(
    db: &support::TestDb,
    f: &Fixture,
    args: OpenArgs<'_>,
) -> Result<KataRecord, KataStoreError> {
    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let outcome = kata::kata_open_tx(
        &mut tx,
        f.tenant,
        f.opener,
        f.project,
        args.signature,
        args.instrument_kind,
        args.instrument_id,
        args.instrument_version,
        args.split,
        args.split_sha256,
        &args.prediction,
        &args.base_ref,
    )
    .await;
    match &outcome {
        Ok(_) => tx.commit().await.expect("commit"),
        Err(_) => tx.rollback().await.expect("rollback"),
    }
    outcome
}

async fn open(db: &support::TestDb, f: &Fixture, signature: &str) -> KataRecord {
    open_with(db, f, args(signature))
        .await
        .expect("open the kata")
}

async fn propose(
    db: &support::TestDb,
    f: &Fixture,
    kata_id: Uuid,
    prediction: &Value,
) -> Result<CandidateRecord, KataStoreError> {
    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let outcome = kata::kata_propose_tx(
        &mut tx,
        f.tenant,
        f.opener,
        kata_id,
        "Change the procedure",
        "The procedure loses a step on retries",
        &json!({ "kind": "procedure", "base_version": "v1" }),
        prediction,
    )
    .await;
    match &outcome {
        Ok(_) => tx.commit().await.expect("commit"),
        Err(_) => tx.rollback().await.expect("rollback"),
    }
    outcome
}

async fn measure(
    db: &support::TestDb,
    f: &Fixture,
    kata_id: Uuid,
    candidate_id: Uuid,
    instrument_version: &str,
    split: &str,
    split_sha256: &str,
) -> Result<MeasurementRecord, KataStoreError> {
    measure_as(
        db,
        f,
        f.opener,
        kata_id,
        candidate_id,
        instrument_version,
        split,
        split_sha256,
    )
    .await
}

async fn measure_as(
    db: &support::TestDb,
    f: &Fixture,
    actor: Uuid,
    kata_id: Uuid,
    candidate_id: Uuid,
    instrument_version: &str,
    split: &str,
    split_sha256: &str,
) -> Result<MeasurementRecord, KataStoreError> {
    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let metric = json!({ "pass_rate": 0.81 });
    let noise_band = json!({ "stddev": 0.02 });
    let outcome = kata::kata_measure_tx(
        &mut tx,
        f.tenant,
        actor,
        kata_id,
        candidate_id,
        instrument_version,
        split,
        split_sha256,
        Uuid::now_v7(),
        &metric,
        &noise_band,
    )
    .await;
    match &outcome {
        Ok(_) => tx.commit().await.expect("commit"),
        Err(_) => tx.rollback().await.expect("rollback"),
    }
    outcome
}

/// Insert one approval with a gate, its target and one decision.
async fn decided(
    db: &support::TestDb,
    f: &Fixture,
    gate: &str,
    target: Uuid,
    decision: &str,
    decided_by_kind: &str,
    approver: Uuid,
) -> Uuid {
    let approval: Uuid = sqlx::query_scalar(
        "INSERT INTO approval (tenant_id, gate, kind, status, meta)
         VALUES ($1, $2, 'gate', 'approved', jsonb_build_object('target', $3))
         RETURNING id",
    )
    .bind(f.tenant.as_uuid())
    .bind(gate)
    .bind(target.to_string())
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the approval");
    sqlx::query(
        "INSERT INTO approval_decision
             (tenant_id, approval_id, approver_id, decision, decided_by_kind)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(f.tenant.as_uuid())
    .bind(approval)
    .bind(approver)
    .bind(decision)
    .bind(decided_by_kind)
    .execute(&db.super_pool)
    .await
    .expect("insert the approval decision");
    approval
}

async fn adopt(
    db: &support::TestDb,
    f: &Fixture,
    kata_id: Uuid,
    candidate_id: Uuid,
    approval_id: Uuid,
) -> Result<CandidateRecord, KataStoreError> {
    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let outcome = kata::kata_adopt_tx(&mut tx, f.tenant, kata_id, candidate_id, approval_id).await;
    match &outcome {
        Ok(_) => tx.commit().await.expect("commit"),
        Err(_) => tx.rollback().await.expect("rollback"),
    }
    outcome
}

async fn close(
    db: &support::TestDb,
    f: &Fixture,
    kata_id: Uuid,
    outcome: &str,
    restored_ref: Option<&Value>,
) -> Result<KataRecord, KataStoreError> {
    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let result = kata::kata_close_tx(
        &mut tx,
        f.tenant,
        kata_id,
        outcome,
        "recorded reason",
        restored_ref,
    )
    .await;
    match &result {
        Ok(_) => tx.commit().await.expect("commit"),
        Err(_) => tx.rollback().await.expect("rollback"),
    }
    result
}

async fn candidate_status(db: &support::TestDb, candidate_id: Uuid) -> String {
    sqlx::query_scalar("SELECT status FROM kata_candidate WHERE id = $1")
        .bind(candidate_id)
        .fetch_one(&db.super_pool)
        .await
        .expect("read the candidate status")
}

#[tokio::test]
async fn an_unmeasurable_kata_is_not_opened() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db).await;

    let mut missing_instrument = args("kata/no-instrument");
    missing_instrument.instrument_id = None;
    let refused = open_with(&db, &f, missing_instrument).await;
    assert!(
        matches!(
            refused,
            Err(KataStoreError::Illegal(KataError::Unmeasurable { missing }))
                if missing == "instrument_id"
        ),
        "an open without an instrument id is refused: {refused:?}"
    );

    let mut missing_prediction = args("kata/no-prediction");
    missing_prediction.prediction = Value::Null;
    let refused = open_with(&db, &f, missing_prediction).await;
    assert!(
        matches!(
            refused,
            Err(KataStoreError::Illegal(KataError::Unmeasurable { missing }))
                if missing == "prediction"
        ),
        "an open without a prediction is refused: {refused:?}"
    );

    let mut empty_prediction = args("kata/empty-prediction");
    empty_prediction.prediction = json!({});
    let refused = open_with(&db, &f, empty_prediction).await;
    assert!(
        matches!(
            refused,
            Err(KataStoreError::Illegal(KataError::Unmeasurable { missing }))
                if missing == "prediction"
        ),
        "an empty prediction object is no prediction: {refused:?}"
    );

    // Acceptance 4: the refusal writes no kata row.
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM kata")
        .fetch_one(&db.super_pool)
        .await
        .expect("count the kata rows");
    assert_eq!(rows, 0, "an unmeasurable kata must not be opened");
}

#[tokio::test]
async fn a_second_open_kata_on_one_signature_is_refused() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db).await;

    let first = open(&db, &f, "kata/signature").await;
    assert_eq!(first.status, "open");
    assert_eq!(first.instrument_version, "eval-v3");
    assert_eq!(first.split_sha256, "sha-locked-v3");

    let refused = open_with(&db, &f, args("kata/signature")).await;
    assert!(
        matches!(
            refused,
            Err(KataStoreError::Illegal(KataError::SignatureOpen))
        ),
        "a second open kata for one signature is refused: {refused:?}"
    );
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM kata")
        .fetch_one(&db.super_pool)
        .await
        .expect("count the kata rows");
    assert_eq!(rows, 1, "the refused second open writes no row");

    // A different signature is a different kata.
    let second = open(&db, &f, "kata/other-signature").await;
    assert_ne!(second.id, first.id);
}

#[tokio::test]
async fn a_candidate_needs_its_prediction_and_ordinals_increment() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db).await;
    let kata = open(&db, &f, "kata/propose").await;

    let refused = propose(&db, &f, kata.id, &Value::Null).await;
    assert!(
        matches!(
            refused,
            Err(KataStoreError::Illegal(KataError::PredictionMissing))
        ),
        "a candidate without a prediction is refused: {refused:?}"
    );

    let first = propose(&db, &f, kata.id, &prediction())
        .await
        .expect("propose the first candidate");
    assert_eq!(first.ordinal, 1);
    assert_eq!(first.status, "proposed");
    assert_eq!(first.prediction.as_ref(), Some(&prediction()));

    let second = propose(&db, &f, kata.id, &prediction())
        .await
        .expect("propose the second candidate");
    assert_eq!(second.ordinal, 2, "ordinals increment per kata");
}

#[tokio::test]
async fn only_the_held_out_split_on_the_pinned_version_measures() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db).await;
    let kata = open(&db, &f, "kata/measure").await;
    let candidate = propose(&db, &f, kata.id, &prediction())
        .await
        .expect("propose the candidate");

    let refused = measure(
        &db,
        &f,
        kata.id,
        candidate.id,
        "eval-v3",
        "train",
        "sha-locked-v3",
    )
    .await;
    assert!(
        matches!(
            refused,
            Err(KataStoreError::Illegal(KataError::MeasurementNotHeldOut { ref split }))
                if split == "train"
        ),
        "a train split measures nothing: {refused:?}"
    );

    let refused = measure(
        &db,
        &f,
        kata.id,
        candidate.id,
        "eval-v3",
        "test",
        "sha-redrawn",
    )
    .await;
    assert!(
        matches!(
            refused,
            Err(KataStoreError::Illegal(
                KataError::MeasurementNotHeldOut { .. }
            ))
        ),
        "a re-drawn split sha is not the pinned one: {refused:?}"
    );

    let refused = measure(
        &db,
        &f,
        kata.id,
        candidate.id,
        "eval-v4",
        "test",
        "sha-locked-v3",
    )
    .await;
    assert!(
        matches!(
            refused,
            Err(KataStoreError::Illegal(
                KataError::MeasurementNotHeldOut { .. }
            ))
        ),
        "a changed instrument version is not the pinned one: {refused:?}"
    );

    let measurement = measure(
        &db,
        &f,
        kata.id,
        candidate.id,
        "eval-v3",
        "test",
        "sha-locked-v3",
    )
    .await
    .expect("the held-out reading measures");
    assert_eq!(measurement.instrument_kind, "eval");
    assert_eq!(measurement.instrument_version, "eval-v3");
    assert_eq!(measurement.split, "test");
    assert_eq!(measurement.split_sha256, "sha-locked-v3");
    assert_eq!(measurement.measured_by, f.opener);
    assert_eq!(candidate_status(&db, candidate.id).await, "measured");

    // A hand-written candidate with no prediction cannot be measured at all.
    let bare: Uuid = sqlx::query_scalar(
        "INSERT INTO kata_candidate (tenant_id, kata_id, ordinal, title, target_ref)
         VALUES ($1, $2, 99, 'No prediction', $3) RETURNING id",
    )
    .bind(f.tenant.as_uuid())
    .bind(kata.id)
    .bind(json!({ "kind": "procedure", "base_version": "v1" }))
    .fetch_one(&db.super_pool)
    .await
    .expect("insert a candidate without a prediction");
    let refused = measure(&db, &f, kata.id, bare, "eval-v3", "test", "sha-locked-v3").await;
    assert!(
        matches!(
            refused,
            Err(KataStoreError::Illegal(KataError::PredictionMissing))
        ),
        "a candidate with no prediction is refused before the run: {refused:?}"
    );
}

#[tokio::test]
async fn adoption_needs_another_person_and_a_measured_candidate() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db).await;
    let kata = open(&db, &f, "kata/adopt").await;
    let candidate = propose(&db, &f, kata.id, &prediction())
        .await
        .expect("propose the candidate");

    let unmeasured = propose(&db, &f, kata.id, &prediction())
        .await
        .expect("propose a second candidate");
    let gate = decided(&db, &f, ADOPT_GATE, kata.id, "approve", "human", f.approver).await;
    let refused = adopt(&db, &f, kata.id, unmeasured.id, gate).await;
    assert!(
        matches!(
            refused,
            Err(KataStoreError::Illegal(KataError::CandidateNotMeasured))
        ),
        "an unmeasured candidate cannot be adopted: {refused:?}"
    );

    measure(
        &db,
        &f,
        kata.id,
        candidate.id,
        "eval-v3",
        "test",
        "sha-locked-v3",
    )
    .await
    .expect("measure the candidate");

    // A machine-only decision is no adoption.
    let machine = decided(&db, &f, ADOPT_GATE, kata.id, "approve", "agent", f.approver).await;
    let refused = adopt(&db, &f, kata.id, candidate.id, machine).await;
    assert!(
        matches!(
            refused,
            Err(KataStoreError::Illegal(KataError::SelfApproval))
        ),
        "a machine-only decision adopts nothing: {refused:?}"
    );

    // The kata's own opener cannot decide its adoption.
    let self_decision = decided(&db, &f, ADOPT_GATE, kata.id, "approve", "human", f.opener).await;
    let refused = adopt(&db, &f, kata.id, candidate.id, self_decision).await;
    assert!(
        matches!(
            refused,
            Err(KataStoreError::Illegal(KataError::SelfApproval))
        ),
        "the opener's own approval is refused: {refused:?}"
    );

    // An approval for another kata does not adopt this candidate.
    let other_kata = open(&db, &f, "kata/adopt-other").await;
    let wrong_target = decided(
        &db,
        &f,
        ADOPT_GATE,
        other_kata.id,
        "approve",
        "human",
        f.approver,
    )
    .await;
    let refused = adopt(&db, &f, kata.id, candidate.id, wrong_target).await;
    assert!(
        matches!(
            refused,
            Err(KataStoreError::Illegal(KataError::SelfApproval))
        ),
        "an approval naming another kata adopts nothing: {refused:?}"
    );

    // A distinct human approves; the candidate is adopted and the kata records it, still open.
    let human = decided(&db, &f, ADOPT_GATE, kata.id, "approve", "human", f.approver).await;
    let adopted = adopt(&db, &f, kata.id, candidate.id, human)
        .await
        .expect("the distinct human adoption succeeds");
    assert_eq!(adopted.status, "adopted");
    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let read = kata::kata_get_tx(&mut tx, f.tenant, kata.id)
        .await
        .expect("read the kata")
        .expect("the kata exists");
    tx.commit().await.expect("commit");
    assert_eq!(read.status, "open", "adoption does not close the kata");
    assert_eq!(read.outcome.as_deref(), Some("kept"));
}

/// The MEASURER is excluded too: running the instrument and deciding the adoption are one
/// principal's work, so the guard must refuse the measurer as well as the opener. Deleting the
/// measurer limb would leave every other test green; this one pins it.
#[tokio::test]
async fn the_kata_measurer_cannot_decide_its_adoption() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db).await;
    let kata = open(&db, &f, "kata/measurer-adopt").await;
    let candidate = propose(&db, &f, kata.id, &prediction())
        .await
        .expect("propose");
    measure_as(
        &db,
        &f,
        f.approver,
        kata.id,
        candidate.id,
        "eval-v3",
        "test",
        "sha-locked-v3",
    )
    .await
    .expect("the approver runs the held-out measurement");
    let approval = decided(&db, &f, ADOPT_GATE, kata.id, "approve", "human", f.approver).await;
    let refused = adopt(&db, &f, kata.id, candidate.id, approval).await;
    assert!(
        matches!(
            refused,
            Err(KataStoreError::Illegal(KataError::SelfApproval))
        ),
        "the measurer's own approval is refused: {refused:?}"
    );
}

#[tokio::test]
async fn a_revert_must_name_its_restore_and_close_records_the_outcome() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db).await;
    let open_args = args("kata/revert");
    let base = open_args.base_ref.clone();
    let kata = open_with(&db, &f, open_args)
        .await
        .expect("open the kata to revert");

    let refused = close(&db, &f, kata.id, "reverted", None).await;
    assert!(
        matches!(
            refused,
            Err(KataStoreError::Illegal(KataError::RevertMissing))
        ),
        "a revert without a restore is refused: {refused:?}"
    );
    let null = Value::Null;
    let refused = close(&db, &f, kata.id, "reverted", Some(&null)).await;
    assert!(
        matches!(
            refused,
            Err(KataStoreError::Illegal(KataError::RevertMissing))
        ),
        "a JSON-null restore is no restore: {refused:?}"
    );

    let reverted = close(&db, &f, kata.id, "reverted", Some(&base))
        .await
        .expect("the revert names the version it restored");
    assert_eq!(reverted.status, "closed");
    assert_eq!(reverted.outcome.as_deref(), Some("reverted"));
    assert_eq!(
        reverted.restored_ref.as_ref(),
        Some(&base),
        "the restored version is the recorded base"
    );
    assert_eq!(reverted.outcome_reason, "recorded reason");

    let refused = close(&db, &f, kata.id, "kept", None).await;
    assert!(
        matches!(
            refused,
            Err(KataStoreError::Illegal(KataError::NotOpen { ref status }))
                if status == "closed"
        ),
        "a closed kata does not close twice: {refused:?}"
    );

    let kept = open(&db, &f, "kata/kept").await;
    let closed = close(&db, &f, kept.id, "kept", None)
        .await
        .expect("a kept close needs no restore");
    assert_eq!(closed.status, "closed");
    assert_eq!(closed.outcome.as_deref(), Some("kept"));
    assert!(closed.restored_ref.is_none());

    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let listed = kata::kata_list_tx(&mut tx, f.tenant, Some("closed"))
        .await
        .expect("list the closed katas");
    tx.commit().await.expect("commit");
    assert_eq!(listed.len(), 2, "both closed katas are listed");
}

#[tokio::test]
async fn observations_need_evidence_and_the_problem_needs_data() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db).await;
    let kata = open(&db, &f, "kata/observe").await;

    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let refused = kata::kata_observe_tx(&mut tx, f.tenant, kata.id, &json!([])).await;
    tx.rollback().await.expect("rollback");
    assert!(
        matches!(
            refused,
            Err(KataStoreError::Illegal(
                KataError::ObservationWithoutEvidence
            ))
        ),
        "an observation with no ref is refused: {refused:?}"
    );

    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    kata::kata_observe_tx(
        &mut tx,
        f.tenant,
        kata.id,
        &json!([{ "kind": "card", "id": Uuid::now_v7().to_string(), "get": "gemba" }]),
    )
    .await
    .expect("record the evidence-backed observation");
    let observed = kata::kata_observe_tx(
        &mut tx,
        f.tenant,
        kata.id,
        &json!({ "refs": [{ "kind": "run", "id": Uuid::now_v7().to_string() }] }),
    )
    .await
    .expect("record a second observation");
    tx.commit().await.expect("commit");
    let observations = observed.observations.as_array().expect("an array");
    assert_eq!(observations.len(), 2, "observations append in order");

    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let refused =
        kata::kata_analyse_tx(&mut tx, f.tenant, kata.id, &json!({ "statement": "x" })).await;
    tx.rollback().await.expect("rollback");
    assert!(
        matches!(
            refused,
            Err(KataStoreError::Illegal(KataError::ProblemWithoutData))
        ),
        "a problem without data is refused: {refused:?}"
    );

    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let analysed = kata::kata_analyse_tx(
        &mut tx,
        f.tenant,
        kata.id,
        &json!({ "statement": "retries lose a step", "data": [{ "metric": "pass_rate" }] }),
    )
    .await
    .expect("the data-backed problem is recorded");
    tx.commit().await.expect("commit");
    assert!(analysed.problem.is_some());
}

#[tokio::test]
async fn propagation_targets_need_their_own_human_gate() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db).await;
    let kata = open(&db, &f, "kata/propagate").await;

    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let propagation: PropagationRecord = kata::kata_propagate_tx(
        &mut tx,
        f.tenant,
        f.opener,
        kata.id,
        &json!({ "tenant_id": f.tenant.as_uuid(), "kind": "procedure", "base_version": "v1" }),
    )
    .await
    .expect("register the propagation target");
    tx.commit().await.expect("commit");
    assert_eq!(propagation.status, "pending");

    let machine = decided(
        &db,
        &f,
        PROPAGATE_GATE,
        propagation.id,
        "approve",
        "agent",
        f.approver,
    )
    .await;
    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let refused =
        kata::kata_propagation_approve_tx(&mut tx, f.tenant, propagation.id, machine).await;
    tx.rollback().await.expect("rollback");
    assert!(
        matches!(
            refused,
            Err(KataStoreError::Illegal(
                KataError::PropagationApprovalMissing { .. }
            ))
        ),
        "a machine decision propagates nothing: {refused:?}"
    );

    let human = decided(
        &db,
        &f,
        PROPAGATE_GATE,
        propagation.id,
        "approve",
        "human",
        f.approver,
    )
    .await;
    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let published = kata::kata_propagation_approve_tx(&mut tx, f.tenant, propagation.id, human)
        .await
        .expect("the human gate publishes the target");
    tx.commit().await.expect("commit");
    assert_eq!(published.status, "published");
    assert_eq!(published.approval_id, Some(human));
}
