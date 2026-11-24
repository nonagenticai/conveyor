//! S21 step 10 (#656) — the L1 laya gate's acceptance, over real HTTP, real Postgres and a stub
//! laya.
//!
//! Every verified path is three calls: `ladder.case` builds the body, `ladder.precheck` posts it
//! (the engine records the exchange under the case's claim and key), and `ladder.evaluate` records
//! the decision only from that record. A verdict in a request is never evidence; an outage is
//! never a pass; an authored menu is the only menu.
//!
//! `CONVEYOR_EXTERNAL_LAYA_URL` is process-global, so every test here holds [`support::LAYA_ENV`]
//! for its stub's lifetime.

mod support;

use serde_json::{Value, json};
use uuid::Uuid;

const TOKEN: &str = "dev:alice";
const KIND: &str = "gate.precheck";
const GATE: &str = "capability_ready";

fn out(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

fn id_of(value: &Value, field: &str) -> String {
    value[field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} is not a string in {value}"))
        .to_owned()
}

/// The RFC 9457 problem `type` a refusal carries.
fn problem(body: &Value) -> &str {
    body["type"].as_str().unwrap_or_default()
}

async fn fixture() -> (support::Pg, support::Running, support::Seeded) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");
    let running = support::Running::start(&pg).await;
    seal_laya_token(&pg, &seeded).await;
    let (status, body) = running
        .invoke(
            "laya_configs.create",
            TOKEN,
            json!({"model_ref": "jev-latest", "timeout_ms": 2500}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    (pg, running, seeded)
}

async fn seal_laya_token(pg: &support::Pg, seeded: &support::Seeded) {
    let mut tx = conveyor_store::begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scoped tx");
    conveyor_store::secret::create_secret_tx(
        &mut tx,
        seeded.tenant,
        "laya.token",
        b"test-laya-token",
        "tenant",
    )
    .await
    .expect("seal the laya token");
    tx.commit().await.expect("commit the laya token");
}

/// Point the process at a fresh stub for this test and return it. The caller holds
/// [`support::LAYA_ENV`] for the stub's lifetime.
async fn stub(answer: Value) -> support::LayaStub {
    let stub = support::LayaStub::start(answer).await;
    support::set_env("CONVEYOR_EXTERNAL_LAYA_URL", &stub.base);
    stub
}

/// The score answer a `score` case reads.
fn score_answer(score: f64) -> Value {
    json!({"answers": {"accept": {"score": score, "reason": "the stub's verdict"}}})
}

/// Create the gate policy; `meta` selects a `choice` case, `fire_when` when the case fires.
async fn create_policy(
    running: &support::Running,
    gate: &str,
    meta: Value,
    fire_when: Option<Value>,
    fail_policy: &str,
    action: &str,
) -> String {
    let mut input = json!({
        "kind": KIND,
        "gate": gate,
        "threshold": 0.8,
        "fail_policy": fail_policy,
        "action": action,
        "meta": meta,
    });
    if let Some(fire_when) = fire_when {
        input["fire_when"] = fire_when;
    }
    let (status, body) = running
        .invoke("decision_policies.create", TOKEN, input, None, None, None)
        .await;
    assert_eq!(status, 201, "{body}");
    id_of(out(&body), "id")
}

/// Author a two-option menu: one accepted key, one refused key.
async fn author_set(running: &support::Running, gate: &str) -> Value {
    let (status, body) = running
        .invoke(
            "decision_option_sets.set",
            TOKEN,
            json!({
                "kind": KIND,
                "gate": gate,
                "options": [
                    {"key": "accept", "label": "Accept the draft", "payload": {"outcome": "accepted"}},
                    {"key": "refuse", "label": "Refuse the draft", "payload": {"outcome": "refused"}}
                ]
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    out(&body).clone()
}

/// `ladder.case`, asserted to fire; returns the `CaseOut`.
async fn build_case(
    running: &support::Running,
    gate: &str,
    output: &Value,
    criteria: &Value,
) -> Value {
    let (status, body) = running
        .invoke(
            "ladder.case",
            TOKEN,
            json!({
                "kind": KIND,
                "gate": gate,
                "output": output,
                "criteria": criteria,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let case = out(&body).clone();
    assert_eq!(case["fires"], json!(true), "{case}");
    case
}

/// Post the built case to laya through the engine, under the case's own claim.
async fn precheck(running: &support::Running, case: &Value) -> (u16, Value) {
    running
        .invoke(
            "ladder.precheck",
            TOKEN,
            case["case"].clone(),
            None,
            case["call_key"].as_str(),
            None,
        )
        .await
}

/// `ladder.evaluate` with the facts the case was built from and its claim.
async fn evaluate(
    running: &support::Running,
    gate: &str,
    case: &Value,
    output: &Value,
    criteria: &Value,
) -> (u16, Value) {
    running
        .invoke(
            "ladder.evaluate",
            TOKEN,
            json!({
                "kind": KIND,
                "gate": gate,
                "output": output,
                "criteria": criteria,
                "call_key": case["call_key"],
            }),
            None,
            None,
            None,
        )
        .await
}

async fn decision_count(pg: &support::Pg, seeded: &support::Seeded) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM decision WHERE tenant_id = $1 AND kind = $2")
        .bind(seeded.tenant.as_uuid())
        .bind(KIND)
        .fetch_one(&pg.super_pool)
        .await
        .expect("count decisions")
}

fn draft(text: &str) -> Value {
    json!({"draft": text})
}

fn criteria() -> Value {
    json!([{"criterion": "names the change"}])
}

// ── P1 — the case is the ladder's, and the decision cites the call ──────────────────────────

/// P1.1 — the engine records the call under the case's key and input hash.
#[tokio::test(flavor = "multi_thread")]
async fn the_precheck_call_records_the_case() {
    let _guard = support::LAYA_ENV.lock().await;
    let (_pg, running, _seeded) = fixture().await;
    create_policy(&running, GATE, json!({}), None, "closed", "handover").await;
    let stub = stub(score_answer(0.9)).await;

    let case = build_case(&running, GATE, &draft("the case"), &criteria()).await;
    assert_eq!(case["case"]["model"], json!("jev-latest"), "{case}");
    assert!(
        case["case"]["state"]
            .as_str()
            .is_some_and(|s| !s.is_empty())
    );
    assert_eq!(
        case["case"]["questions"]["accept"]["type"],
        json!("score"),
        "{case}"
    );

    let (status, body) = precheck(&running, &case).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(stub.calls(), 1, "the engine made exactly one laya call");
    assert_eq!(
        out(&body)["answers"]["accept"]["score"],
        json!(0.9),
        "{body}"
    );
}

/// P1.2 — the decision cites the recorded exchange and takes its measured latency.
#[tokio::test(flavor = "multi_thread")]
async fn evaluate_derives_the_verdict_from_the_recording() {
    let _guard = support::LAYA_ENV.lock().await;
    let (pg, running, seeded) = fixture().await;
    create_policy(&running, GATE, json!({}), None, "closed", "handover").await;
    let _stub = stub(score_answer(0.9)).await;

    let case = build_case(&running, GATE, &draft("the case"), &criteria()).await;
    let (status, body) = precheck(&running, &case).await;
    assert_eq!(status, 200, "{body}");
    let (status, body) = evaluate(&running, GATE, &case, &draft("the case"), &criteria()).await;
    assert_eq!(status, 201, "{body}");
    let decision = out(&body);
    assert_eq!(decision["rung"], json!("laya"), "{decision}");
    assert_eq!(decision["answer"], json!("accept"), "{decision}");
    assert_eq!(decision["outcome"], json!("accepted"), "{decision}");
    assert_eq!(decision["confidence"], json!(0.9), "{decision}");
    assert!(
        decision["latency_ms"].as_i64().is_some(),
        "the decision records the invocation's measured duration: {decision}"
    );
    let meta = &decision["meta"];
    assert!(meta["laya_call_key"].is_string(), "{meta}");
    assert!(meta["laya_invocation_id"].is_string(), "{meta}");
    assert_eq!(meta["case_sha256"], case["case_hash"], "{meta}");
    assert_eq!(decision_count(&pg, &seeded).await, 1);
}

/// P1.3 — a verdict in the request is refused and writes nothing.
#[tokio::test(flavor = "multi_thread")]
async fn a_caller_supplied_confidence_is_refused() {
    let _guard = support::LAYA_ENV.lock().await;
    let (pg, running, seeded) = fixture().await;
    create_policy(&running, GATE, json!({}), None, "closed", "handover").await;
    let _stub = stub(score_answer(0.9)).await;

    let (status, body) = running
        .invoke(
            "ladder.evaluate",
            TOKEN,
            json!({
                "kind": KIND,
                "gate": GATE,
                "output": draft("the case"),
                "criteria": criteria(),
                "call_key": "ladder.precheck:forged",
                "confidence": 0.99
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(problem(&body), "caller_supplied_verdict", "{body}");
    assert_eq!(decision_count(&pg, &seeded).await, 0);
}

/// P1.4 — a claim recorded for another case is refused.
#[tokio::test(flavor = "multi_thread")]
async fn a_call_for_another_case_is_refused() {
    let _guard = support::LAYA_ENV.lock().await;
    let (pg, running, seeded) = fixture().await;
    create_policy(&running, GATE, json!({}), None, "closed", "handover").await;
    let _stub = stub(score_answer(0.9)).await;

    let asked = build_case(&running, GATE, &draft("case A"), &criteria()).await;
    let (status, body) = precheck(&running, &asked).await;
    assert_eq!(status, 200, "{body}");

    // A different case: its facts are the ones evaluated, the old claim is the one presented.
    let (status, body) = evaluate(&running, GATE, &asked, &draft("case B"), &criteria()).await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(problem(&body), "laya_call_unverified", "{body}");
    assert_eq!(decision_count(&pg, &seeded).await, 0);
}

// ── P2 — options are authored, versioned rows ──────────────────────────────────────────────

/// P2.1 — a declared choice is built from the authored keys, and inline options are refused.
#[tokio::test(flavor = "multi_thread")]
async fn the_question_comes_from_the_authored_set() {
    let _guard = support::LAYA_ENV.lock().await;
    let (_pg, running, _seeded) = fixture().await;
    create_policy(
        &running,
        GATE,
        json!({"question_type": "choice"}),
        None,
        "closed",
        "handover",
    )
    .await;
    let set = author_set(&running, GATE).await;
    let _stub =
        stub(json!({"answers": {"accept": {"choice": "accept", "confidence": 0.95}}})).await;

    let case = build_case(&running, GATE, &draft("the case"), &criteria()).await;
    assert_eq!(set["version"], json!(1), "{set}");
    assert_eq!(case["option_set_version"], json!(1), "{case}");
    assert_eq!(
        case["case"]["questions"]["accept"]["type"],
        json!("choice"),
        "{case}"
    );
    assert_eq!(
        case["case"]["questions"]["accept"]["criteria"],
        json!(["accept", "refuse"]),
        "{case}"
    );

    let (status, body) = running
        .invoke(
            "ladder.evaluate",
            TOKEN,
            json!({
                "kind": KIND,
                "gate": GATE,
                "output": draft("the case"),
                "criteria": criteria(),
                "call_key": case["call_key"],
                "options": [{"key": "accept", "label": "Accept"}]
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(problem(&body), "action-invalid-input", "{body}");
    assert_eq!(body["errors"][0]["code"], json!("inline_options"), "{body}");
}

/// P2.2 — an answer outside the authored set is refused.
#[tokio::test(flavor = "multi_thread")]
async fn an_answer_outside_the_set_is_refused() {
    let _guard = support::LAYA_ENV.lock().await;
    let (pg, running, seeded) = fixture().await;
    create_policy(
        &running,
        GATE,
        json!({"question_type": "choice"}),
        None,
        "closed",
        "handover",
    )
    .await;
    author_set(&running, GATE).await;
    let _stub = stub(json!({"answers": {"accept": {"choice": "maybe", "confidence": 0.9}}})).await;

    let case = build_case(&running, GATE, &draft("the case"), &criteria()).await;
    let (status, body) = precheck(&running, &case).await;
    assert_eq!(status, 200, "{body}");
    let (status, body) = evaluate(&running, GATE, &case, &draft("the case"), &criteria()).await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(problem(&body), "laya_answer_not_in_options", "{body}");
    assert_eq!(decision_count(&pg, &seeded).await, 0);
}

/// P2.3 — a declared choice with no authored set is refused, never answered from free text.
#[tokio::test(flavor = "multi_thread")]
async fn a_case_without_an_authored_set_is_refused() {
    let _guard = support::LAYA_ENV.lock().await;
    let (_pg, running, _seeded) = fixture().await;
    create_policy(
        &running,
        GATE,
        json!({"question_type": "choice"}),
        None,
        "closed",
        "handover",
    )
    .await;
    let _stub = stub(score_answer(0.9)).await;

    let (status, body) = running
        .invoke(
            "ladder.case",
            TOKEN,
            json!({"kind": KIND, "gate": GATE, "output": draft("x"), "criteria": criteria()}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(problem(&body), "option_set_missing", "{body}");
}

/// P2.4 — authoring twice increments the version and the next read sees it.
#[tokio::test(flavor = "multi_thread")]
async fn the_set_version_increments() {
    let _guard = support::LAYA_ENV.lock().await;
    let (_pg, running, _seeded) = fixture().await;
    let first = author_set(&running, GATE).await;
    let second = author_set(&running, GATE).await;
    assert_eq!(first["version"], json!(1), "{first}");
    assert_eq!(second["version"], json!(2), "{second}");
    let (status, body) = running
        .invoke(
            "decision_option_sets.get",
            TOKEN,
            json!({"kind": KIND, "gate": GATE}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(out(&body)["version"], json!(2), "{body}");
    assert_eq!(
        out(&body)["options"].as_array().map(Vec::len),
        Some(2),
        "{body}"
    );
}

/// P2.5 — a choice decision records the authored version and the authored rows.
#[tokio::test(flavor = "multi_thread")]
async fn a_choice_decision_records_the_authored_set() {
    let _guard = support::LAYA_ENV.lock().await;
    let (pg, running, seeded) = fixture().await;
    create_policy(
        &running,
        GATE,
        json!({"question_type": "choice"}),
        None,
        "closed",
        "handover",
    )
    .await;
    author_set(&running, GATE).await;
    let _stub =
        stub(json!({"answers": {"accept": {"choice": "accept", "confidence": 0.95}}})).await;

    let case = build_case(&running, GATE, &draft("the case"), &criteria()).await;
    let (status, body) = precheck(&running, &case).await;
    assert_eq!(status, 200, "{body}");
    let (status, body) = evaluate(&running, GATE, &case, &draft("the case"), &criteria()).await;
    assert_eq!(status, 201, "{body}");
    let decision = out(&body);
    assert_eq!(decision["answer"], json!("accept"), "{decision}");
    assert_eq!(decision["outcome"], json!("accepted"), "{decision}");
    assert_eq!(decision["option_set_version"], json!(1), "{decision}");
    let options: Vec<(String, String)> = sqlx::query_as(
        "SELECT key, label FROM decision_option WHERE decision_id = $1 ORDER BY key",
    )
    .bind(Uuid::parse_str(&id_of(decision, "id")).expect("a decision id"))
    .fetch_all(&pg.super_pool)
    .await
    .expect("read the recorded options");
    assert_eq!(
        options,
        vec![
            ("accept".to_owned(), "Accept the draft".to_owned()),
            ("refuse".to_owned(), "Refuse the draft".to_owned()),
        ],
        "the decision records the authored rows, not the request"
    );
    assert_eq!(decision_count(&pg, &seeded).await, 1);
}

// ── P3 — the fail policy applies, and `fire_when` decides ──────────────────────────────────

/// P3.1 — a recorded call that did not answer follows the policy; it is never a pass.
#[tokio::test(flavor = "multi_thread")]
async fn an_outage_follows_the_fail_policy() {
    let _guard = support::LAYA_ENV.lock().await;
    let (pg, running, seeded) = fixture().await;
    create_policy(&running, GATE, json!({}), None, "closed", "handover").await;
    create_policy(
        &running,
        "capability_open",
        json!({}),
        None,
        "open",
        "handover",
    )
    .await;
    let stub = stub(json!({"answers": {"accept": {"error": "unavailable"}}})).await;

    // closed: the outage is a refusal, and the decision says the gate was unavailable.
    let case = build_case(&running, GATE, &draft("closed case"), &criteria()).await;
    let (status, body) = precheck(&running, &case).await;
    assert_eq!(status, 200, "{body}");
    let (status, body) = evaluate(&running, GATE, &case, &draft("closed case"), &criteria()).await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(out(&body)["outcome"], json!("refused"), "{body}");
    assert_eq!(
        out(&body)["meta"]["gate_unavailable"],
        json!(true),
        "{body}"
    );

    // open + handover: the same outage escalates instead, and nothing is accepted.
    let case = build_case(
        &running,
        "capability_open",
        &draft("open case"),
        &criteria(),
    )
    .await;
    let (status, body) = precheck(&running, &case).await;
    assert_eq!(status, 200, "{body}");
    let (status, body) = evaluate(
        &running,
        "capability_open",
        &case,
        &draft("open case"),
        &criteria(),
    )
    .await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(out(&body)["outcome"], json!("escalated"), "{body}");
    assert_eq!(stub.calls(), 2, "one call per case");
    assert_eq!(decision_count(&pg, &seeded).await, 2);
}

/// P3.1b — no recorded call at all is a caller error, never a decision.
#[tokio::test(flavor = "multi_thread")]
async fn an_unrecorded_outage_writes_no_decision() {
    let _guard = support::LAYA_ENV.lock().await;
    let (pg, running, seeded) = fixture().await;
    create_policy(&running, GATE, json!({}), None, "closed", "handover").await;
    let _stub = stub(score_answer(0.9)).await;

    let (status, body) = running
        .invoke(
            "ladder.evaluate",
            TOKEN,
            json!({
                "kind": KIND,
                "gate": GATE,
                "output": draft("never asked"),
                "criteria": criteria(),
                "call_key": "ladder.precheck:never-made"
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(problem(&body), "laya_call_unverified", "{body}");
    assert_eq!(decision_count(&pg, &seeded).await, 0);
}

/// P3.2 — a case whose `fire_when` does not hold is not asked and records nothing.
#[tokio::test(flavor = "multi_thread")]
async fn a_case_that_does_not_fire_is_not_asked() {
    let _guard = support::LAYA_ENV.lock().await;
    let (pg, running, seeded) = fixture().await;
    let policy_id = create_policy(
        &running,
        GATE,
        json!({}),
        Some(json!({"all_of": [{"gate": "another_gate"}]})),
        "closed",
        "handover",
    )
    .await;
    let stub = stub(score_answer(0.9)).await;

    let (status, body) = running
        .invoke(
            "ladder.case",
            TOKEN,
            json!({"kind": KIND, "gate": GATE, "output": draft("x"), "criteria": criteria()}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(out(&body)["fires"], json!(false), "{body}");
    assert!(out(&body)["case"].is_null(), "{body}");

    let (status, body) = running
        .invoke(
            "ladder.evaluate",
            TOKEN,
            json!({
                "kind": KIND,
                "gate": GATE,
                "output": draft("x"),
                "criteria": criteria(),
                "call_key": "ladder.precheck:not-asked"
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(problem(&body), "case_not_fired", "{body}");
    assert_eq!(stub.calls(), 0, "the case was never asked");
    assert_eq!(decision_count(&pg, &seeded).await, 0);

    // Clearing the expression (an explicit `null` on update) makes the policy fire again.
    let (status, body) = running
        .invoke(
            "decision_policies.update",
            TOKEN,
            json!({"id": policy_id, "fire_when": null}),
            Some("1"),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert!(out(&body)["fire_when"].is_null(), "{body}");
    let (status, body) = running
        .invoke(
            "ladder.case",
            TOKEN,
            json!({"kind": KIND, "gate": GATE, "output": draft("x"), "criteria": criteria()}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(out(&body)["fires"], json!(true), "{body}");
}
