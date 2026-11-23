//! T5 (#77) — the kata loop through the action surface (SPEC_V2 §6.3, §9.3–§9.4, §12 T5;
//! `docs/plans/inception/T5/02-architecture-design.md`): an unmeasurable kata is not opened,
//! one open kata per signature, a procedure change is measured on the held-out split, the
//! prediction precedes the measurement, the kata cannot approve its own adoption, and a revert
//! records the version it restored.
//!
//! The harness is `support::Pg::fresh()` plus a direct `InvokeRequest` as a real seeded actor,
//! the same shape `authoring.rs` uses.
//!
//! `cargo test -p conveyor-actions --test kata_gate`.

mod support;

use conveyor_actions::{ActionError, Authority, InvokeOutcome, InvokeRequest, invoke, registry};
use conveyor_auth::Role;
use conveyor_store::kata::ADOPT_GATE;
use serde_json::{Value, json};
use uuid::Uuid;

/// The 64-hex sha that locks the held-out split.
const SPLIT_SHA: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn request(actor: Uuid, action: &str, input: Value) -> InvokeRequest {
    InvokeRequest {
        credential_alias: None,
        action: action.to_owned(),
        input,
        precondition: None,
        idempotency_key: None,
        approval_id: None,
        actor_id: actor,
        actor_kind: "user".to_owned(),
        role: Role::Admin,
        policy: Default::default(),
        authority: Authority::Role,
        request_id: Some("rid-kata".to_owned()),
        trace_id: Some("trace-kata".to_owned()),
        depth: 0,
        app_key: "conveyor-test".to_owned(),
        usage: None::<conveyor_actions::UsageContext>,
    }
}

async fn fixture() -> (support::Pg, support::Seeded) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    (pg, seeded)
}

async fn call(
    pg: &support::Pg,
    seeded: &support::Seeded,
    action: &str,
    input: Value,
) -> Result<InvokeOutcome, ActionError> {
    invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request(seeded.user_id, action, input),
    )
    .await
}

fn id_of(value: &Value, field: &str) -> Uuid {
    value[field]
        .as_str()
        .and_then(|raw| Uuid::parse_str(raw).ok())
        .unwrap_or_else(|| panic!("{field} is not a uuid in {value}"))
}

/// One kata project. `kata.open` only proves the project exists in the tenant, and the project
/// family is not this slice's surface, so the fixture inserts the row directly.
async fn project(pg: &support::Pg, seeded: &support::Seeded) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name) VALUES ($1, $2, 'Kata project') RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(format!("kata-{}", Uuid::now_v7().simple()))
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert the kata project")
}

/// A measurable open request: the instrument, the held-out split with its sha, a prediction.
fn open_input(project: Uuid, signature: &str) -> Value {
    json!({
        "project_id": project,
        "signature": signature,
        "instrument_kind": "eval",
        "instrument_id": Uuid::now_v7(),
        "instrument_version": "eval-v3",
        "split": "test",
        "split_sha256": SPLIT_SHA,
        "prediction": { "delta": "5%" },
        "base_ref": {
            "kind": "procedure",
            "id": Uuid::now_v7().to_string(),
            "base_version": 1,
        },
    })
}

async fn open_kata(
    pg: &support::Pg,
    seeded: &support::Seeded,
    project: Uuid,
    signature: &str,
) -> Value {
    let outcome = call(pg, seeded, "kata.open", open_input(project, signature))
        .await
        .expect("the measurable kata opens");
    assert_eq!(outcome.status, 201, "{}", outcome.body);
    let output = outcome.body["output"].clone();
    assert_eq!(output["status"], "open", "{}", outcome.body);
    output
}

async fn propose(pg: &support::Pg, seeded: &support::Seeded, kata: Uuid) -> Value {
    let outcome = call(
        pg,
        seeded,
        "kata.propose",
        json!({
            "kata_id": kata,
            "title": "Drop the lost retry step",
            "rationale": "the step loses input on retries",
            "target_ref": {"kind": "procedure", "base_version": 1},
            "prediction": {"delta": "5%"},
        }),
    )
    .await
    .expect("the candidate proposes");
    assert_eq!(outcome.status, 201, "{}", outcome.body);
    outcome.body["output"].clone()
}

fn measure_input(kata: Uuid, candidate: Uuid, split: &str, split_sha256: &str) -> Value {
    json!({
        "kata_id": kata,
        "candidate_id": candidate,
        "instrument_version": "eval-v3",
        "split": split,
        "split_sha256": split_sha256,
        "run_id": Uuid::now_v7(),
        "metric": {"pass_rate": 0.81},
        "noise_band": {"stddev": 0.02},
    })
}

/// Insert one approved `kata.adopt` gate naming the kata, decided by `decider` as a person.
/// `requested_by` stays NULL: `kata.adopt` reads a decision somebody else opened and took.
async fn adopt_approval(
    pg: &support::Pg,
    seeded: &support::Seeded,
    kata: Uuid,
    decider: Uuid,
) -> Uuid {
    let approval: Uuid = sqlx::query_scalar(
        "INSERT INTO approval (tenant_id, gate, kind, status, requested_by, meta)
         VALUES ($1, $2, 'gate', 'approved', NULL, jsonb_build_object('target', $3::text))
         RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(ADOPT_GATE)
    .bind(kata.to_string())
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert the kata.adopt approval");
    sqlx::query(
        "INSERT INTO approval_decision
             (tenant_id, approval_id, approver_id, decision, decided_by_kind)
         VALUES ($1, $2, $3, 'approve', 'human')",
    )
    .bind(seeded.tenant_id)
    .bind(approval)
    .bind(decider)
    .execute(&pg.super_pool)
    .await
    .expect("insert the human adoption decision");
    approval
}

/// Acceptance 4: an open request without an instrument id, version, split, split sha or
/// prediction refuses `422 kata_unmeasurable` naming the field, and writes no kata row.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_kata_without_an_instrument_is_not_opened() {
    let (pg, seeded) = fixture().await;
    let project = project(&pg, &seeded).await;

    for field in [
        "instrument_id",
        "instrument_version",
        "split",
        "split_sha256",
        "prediction",
    ] {
        let mut input = open_input(project, &format!("kata/unmeasurable/{field}"));
        input
            .as_object_mut()
            .unwrap_or_else(|| panic!("an open request is an object"))
            .remove(field);
        let error = call(&pg, &seeded, "kata.open", input)
            .await
            .expect_err("an unmeasurable kata cannot open");
        assert_eq!(error.status, 422, "{field}: {}", error.detail);
        assert_eq!(error.code, "kata_unmeasurable", "{field}: {}", error.detail);
        assert!(
            error.detail.contains(field),
            "the refusal names {field}: {}",
            error.detail
        );
    }

    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM kata")
        .fetch_one(&pg.super_pool)
        .await
        .expect("count the kata rows");
    assert_eq!(rows, 0, "an unmeasurable kata must not be opened");
}

/// D-T5-7: the partial unique index — a second open kata on one signature is `409`, not a
/// second row.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_second_open_kata_on_one_signature_is_refused() {
    let (pg, seeded) = fixture().await;
    let project = project(&pg, &seeded).await;

    let first = open_kata(&pg, &seeded, project, "kata/signature").await;
    assert_eq!(first["instrument_version"], "eval-v3", "{first}");

    let error = call(
        &pg,
        &seeded,
        "kata.open",
        open_input(project, "kata/signature"),
    )
    .await
    .expect_err("a second open kata for one signature is refused");
    assert_eq!(error.status, 409, "{}", error.detail);
    assert_eq!(error.code, "kata_signature_open", "{}", error.detail);

    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM kata")
        .fetch_one(&pg.super_pool)
        .await
        .expect("count the kata rows");
    assert_eq!(rows, 1, "the refused second open writes no row");
}

/// Acceptance 1: a procedure change measures on the held-out split only — a train split or a
/// re-drawn split sha refuses `422 kata_measurement_not_held_out`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_procedure_change_is_measured_on_the_held_out_split() {
    let (pg, seeded) = fixture().await;
    let project = project(&pg, &seeded).await;
    let kata = id_of(
        &open_kata(&pg, &seeded, project, "kata/measure").await,
        "id",
    );
    let candidate = id_of(&propose(&pg, &seeded, kata).await, "id");

    let error = call(
        &pg,
        &seeded,
        "kata.measure",
        measure_input(kata, candidate, "train", SPLIT_SHA),
    )
    .await
    .expect_err("a train split measures nothing");
    assert_eq!(error.status, 422, "{}", error.detail);
    assert_eq!(
        error.code, "kata_measurement_not_held_out",
        "{}",
        error.detail
    );

    let measured = call(
        &pg,
        &seeded,
        "kata.measure",
        measure_input(kata, candidate, "test", SPLIT_SHA),
    )
    .await
    .expect("the held-out reading measures");
    assert_eq!(measured.status, 201, "{}", measured.body);
    let output = &measured.body["output"];
    assert_eq!(output["split"], "test", "{}", measured.body);
    assert_eq!(output["split_sha256"], SPLIT_SHA, "{}", measured.body);
    assert_eq!(output["instrument_version"], "eval-v3", "{}", measured.body);
    assert_eq!(
        output["candidate_id"],
        json!(candidate.to_string()),
        "{}",
        measured.body
    );

    let error = call(
        &pg,
        &seeded,
        "kata.measure",
        measure_input(kata, candidate, "test", &"b".repeat(64)),
    )
    .await
    .expect_err("a re-drawn split measures nothing");
    assert_eq!(error.status, 422, "{}", error.detail);
    assert_eq!(
        error.code, "kata_measurement_not_held_out",
        "{}",
        error.detail
    );
}

/// Acceptance 5: the prediction is pre-registered with the candidate — a proposal without one
/// refuses `422 kata_prediction_missing`, and with no candidate the measurement has nothing to
/// read.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_prediction_is_recorded_before_the_measurement() {
    let (pg, seeded) = fixture().await;
    let project = project(&pg, &seeded).await;
    let kata = id_of(
        &open_kata(&pg, &seeded, project, "kata/prediction").await,
        "id",
    );

    let error = call(
        &pg,
        &seeded,
        "kata.propose",
        json!({
            "kata_id": kata,
            "title": "Unpredicted candidate",
            "rationale": "it might win",
            "target_ref": {"kind": "procedure", "base_version": 1},
        }),
    )
    .await
    .expect_err("a candidate without a prediction is refused");
    assert_eq!(error.status, 422, "{}", error.detail);
    assert_eq!(error.code, "kata_prediction_missing", "{}", error.detail);

    let error = call(
        &pg,
        &seeded,
        "kata.measure",
        measure_input(kata, Uuid::now_v7(), "test", SPLIT_SHA),
    )
    .await
    .expect_err("a candidate that was never proposed cannot be measured");
    assert_eq!(error.status, 404, "{}", error.detail);
}

/// Acceptance 3: the kata cannot approve its own adoption — the opener's human decision is
/// `403 kata_self_approval`; a distinct person's decision adopts the measured candidate.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_kata_cannot_approve_its_own_adoption() {
    let (pg, seeded) = fixture().await;
    let project = project(&pg, &seeded).await;
    let kata = id_of(&open_kata(&pg, &seeded, project, "kata/adopt").await, "id");
    let candidate = id_of(&propose(&pg, &seeded, kata).await, "id");

    let measured = call(
        &pg,
        &seeded,
        "kata.measure",
        measure_input(kata, candidate, "test", SPLIT_SHA),
    )
    .await
    .expect("the held-out reading measures");
    assert_eq!(measured.status, 201, "{}", measured.body);

    // P4: the executor never approves its own work — the opener may not be the decider.
    let self_decided = adopt_approval(&pg, &seeded, kata, seeded.user_id).await;
    let error = call(
        &pg,
        &seeded,
        "kata.adopt",
        json!({"kata_id": kata, "candidate_id": candidate, "approval_id": self_decided}),
    )
    .await
    .expect_err("the kata's opener cannot adopt it");
    assert_eq!(error.status, 403, "{}", error.detail);
    assert_eq!(error.code, "kata_self_approval", "{}", error.detail);

    // A distinct person's human decision adopts the measured candidate.
    let reviewer = support::reviewer_id(&pg).await;
    let human = adopt_approval(&pg, &seeded, kata, reviewer).await;
    let adopted = call(
        &pg,
        &seeded,
        "kata.adopt",
        json!({"kata_id": kata, "candidate_id": candidate, "approval_id": human}),
    )
    .await
    .expect("a distinct person adopts the measured candidate");
    assert_eq!(adopted.status, 200, "{}", adopted.body);
    assert_eq!(
        adopted.body["output"]["status"], "adopted",
        "{}",
        adopted.body
    );
}

/// Acceptance 2: a reverted close without its restore refuses `422 kata_revert_missing`; with
/// the restore it records `closed` / `reverted` and the restored reference.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_reverted_kata_records_the_restore() {
    let (pg, seeded) = fixture().await;
    let project = project(&pg, &seeded).await;
    let kata = id_of(&open_kata(&pg, &seeded, project, "kata/revert").await, "id");

    let error = call(
        &pg,
        &seeded,
        "kata.close",
        json!({"kata_id": kata, "outcome": "reverted", "reason": "the metric regressed"}),
    )
    .await
    .expect_err("a revert without its restore is refused");
    assert_eq!(error.status, 422, "{}", error.detail);
    assert_eq!(error.code, "kata_revert_missing", "{}", error.detail);

    let restored = json!({"kind": "procedure", "version": 1});
    let closed = call(
        &pg,
        &seeded,
        "kata.close",
        json!({
            "kata_id": kata,
            "outcome": "reverted",
            "reason": "the metric regressed",
            "restored_ref": restored,
        }),
    )
    .await
    .expect("the revert names the version it restored");
    assert_eq!(closed.status, 200, "{}", closed.body);
    let output = &closed.body["output"];
    assert_eq!(output["status"], "closed", "{}", closed.body);
    assert_eq!(output["outcome"], "reverted", "{}", closed.body);
    assert_eq!(output["restored_ref"], restored, "{}", closed.body);
}
