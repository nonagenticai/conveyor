//! T2 — the decision ladder's acceptance scenarios (issue #73) over real HTTP and real Postgres.
//!
//! Each test builds its own tenant through [`support::seed`]; the fixture is the S05/S08 shape:
//! an ephemeral Postgres 18, the two estate roles, the embedded migrations and a real server.
//! The assertions are the issue's:
//!
//! * P1.1 — an L0 rule, an L1 laya and an L3 human decision project the same columns;
//! * P2.1 — a tenant refinement changes a threshold with no code change and no redeploy;
//! * P3.1 — every decision pins the policy, refinement and laya-config versions it read, and a
//!   later change does not rewrite the earlier row.
//!
//! The L1 path is the three calls the finding v1-w1c-3 settled: `ladder.prepare` builds the
//! laya document, `laya.classify` runs the engine-owned call, and `ladder.evaluate` records the
//! verified answer. Every laya test takes [`LAYA_ENV`] because the external base URL is
//! process-wide.
//!
//! The superuser pool (`pg.super_pool`) is used only to prove a row's stored pins directly;
//! every behaviour under test goes through an action.

mod support;

use serde_json::{Value, json};

const LAYA_URL_ENV: &str = "CONVEYOR_EXTERNAL_LAYA_URL";

/// `CONVEYOR_EXTERNAL_LAYA_URL` is process-wide and each test needs its own answer document,
/// so the laya tests hold this lock for their whole body. The refusals that never reach the
/// external call (a direct confidence, a missing config) do not take it.
static LAYA_ENV: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

const KIND: &str = "gate.precheck";
const GATE: &str = "capability_ready";
const QUESTION: &str = "Is the capability ready for the gate?";

fn facts() -> Value {
    json!({"card": "card-1", "pin": "knowledge@7"})
}

async fn fixture() -> (support::Pg, support::Running, support::Seeded) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    support::seed(&pg, "dev-b", "bob", "Bob").await;
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");
    let running = support::Running::start(&pg).await;
    (pg, running, seeded)
}

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

fn id_of(value: &Value, field: &str) -> String {
    value[field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} is not a string in {value}"))
        .to_owned()
}

async fn create_policy(
    running: &support::Running,
    kind: &str,
    gate: Option<&str>,
    threshold: f64,
    fail_policy: &str,
    action: &str,
) -> (String, String) {
    let (status, headers, body) = running
        .invoke_full(
            "decision_policies.create",
            "dev:alice",
            json!({
                "kind": kind,
                "gate": gate,
                "threshold": threshold,
                "fail_policy": fail_policy,
                "action": action
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let etag = headers.get("etag").cloned().expect("an ETag on create");
    (id_of(output(&body), "id"), etag)
}

async fn create_laya_config(running: &support::Running) -> String {
    let (status, body) = running
        .invoke(
            "laya_configs.create",
            "dev:alice",
            json!({"model_ref": "jev-latest", "timeout_ms": 2500}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    id_of(output(&body), "id")
}

/// Seal the tenant's `laya.token`, so `laya.classify` reaches its service.
async fn seal_laya_token(pg: &support::Pg, seeded: &support::Seeded) {
    let mut tx = conveyor_store::begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope the laya token");
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

/// Point the process at a stub answering `answer` for the ladder's one question.
async fn laya_stub(answer: Value) -> support::LayaStub {
    let stub = support::LayaStub::start(json!({"answers": {"decision": answer}})).await;
    support::set_env(LAYA_URL_ENV, &stub.base);
    stub
}

/// Author and publish the tenant's option set for a case; returns the published view.
async fn author_options(running: &support::Running, kind: &str, options: Value) -> Value {
    let (status, created) = running
        .invoke(
            "decision_options.create",
            "dev:alice",
            json!({"kind": kind, "gate": GATE, "options": options}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    let id = id_of(output(&created), "id");
    let (status, published) = running
        .invoke(
            "decision_options.publish",
            "dev:alice",
            json!({"id": id}),
            Some("1"),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{published}");
    output(&published).clone()
}

/// Seed and publish a platform-library option set (the tenant's base), returning the set id
/// and its entry ids. The platform seed writes library rows; a tenant action cannot.
async fn seed_library_options(
    pg: &support::Pg,
    tenant: conveyor_domain::TenantId,
    kind: &str,
    options: Value,
) -> (uuid::Uuid, Vec<uuid::Uuid>) {
    let entries: Vec<conveyor_store::ladder::NewOptionEntry> = options
        .as_array()
        .expect("the option list")
        .iter()
        .enumerate()
        .map(|(index, option)| conveyor_store::ladder::NewOptionEntry {
            key: option["key"].as_str().unwrap_or_default().to_owned(),
            label: option["label"].as_str().unwrap_or_default().to_owned(),
            criteria: option["criteria"].as_str().unwrap_or_default().to_owned(),
            position: i32::try_from(index + 1).unwrap_or(1),
        })
        .collect();
    let mut tx = conveyor_store::begin_scoped(&pg.owner, tenant)
        .await
        .expect("scope the library set");
    let (set, stored) = conveyor_store::ladder::insert_option_set_tx(
        &mut tx,
        tenant,
        &conveyor_store::ladder::NewOptionSet {
            kind: kind.to_owned(),
            gate: Some(GATE.to_owned()),
            origin: "library".to_owned(),
            entries,
        },
    )
    .await
    .expect("insert the library set");
    let _ = conveyor_store::ladder::publish_option_set_tx(&mut tx, tenant, set.id, None)
        .await
        .expect("publish the library set");
    tx.commit().await.expect("commit the library set");
    (set.id, stored.into_iter().map(|entry| entry.id).collect())
}

/// `ladder.prepare`: the wire document and its idempotency hash.
async fn prepare(
    running: &support::Running,
    kind: &str,
    laya_config_id: Option<&str>,
) -> (u16, Value) {
    let mut input = json!({
        "kind": kind,
        "gate": GATE,
        "question": QUESTION,
        "input": facts(),
    });
    if let Some(id) = laya_config_id {
        input["laya_config_id"] = json!(id);
    }
    running
        .invoke("ladder.prepare", "dev:alice", input, None, None, None)
        .await
}

/// `laya.classify` with the cited idempotency key.
async fn classify(running: &support::Running, request: Value, key: &str) -> (u16, Value) {
    running
        .invoke("laya.classify", "dev:alice", request, None, Some(key), None)
        .await
}

/// `ladder.evaluate` citing the key its `laya.classify` call used.
async fn evaluate(
    running: &support::Running,
    kind: &str,
    laya_config_id: Option<&str>,
    laya_key: &str,
) -> (u16, Value) {
    let mut input = json!({
        "kind": kind,
        "gate": GATE,
        "question": QUESTION,
        "input": facts(),
        "laya_key": laya_key,
    });
    if let Some(id) = laya_config_id {
        input["laya_config_id"] = json!(id);
    }
    running
        .invoke("ladder.evaluate", "dev:alice", input, None, None, None)
        .await
}

/// The settled three-call L1 path for one case.
async fn laya_decision(
    running: &support::Running,
    kind: &str,
    laya_config_id: &str,
    key: &str,
) -> (u16, Value) {
    let (status, prepared) = prepare(running, kind, Some(laya_config_id)).await;
    if status != 200 {
        return (status, prepared);
    }
    let request = output(&prepared)["request"].clone();
    let (status, classified) = classify(running, request, key).await;
    if status != 200 {
        return (status, classified);
    }
    evaluate(running, kind, Some(laya_config_id), key).await
}

/// The same settled path, with the decision bound to a card and carrying row metadata.
///
/// `card_id`/`meta` are row fields, not part of the request the laya call verified, so the
/// replay still hashes the same document; the binding is what `ladder.apply_draft` reads.
async fn laya_decision_bound(
    running: &support::Running,
    kind: &str,
    laya_config_id: &str,
    key: &str,
    card_id: &str,
    meta: Option<Value>,
) -> (u16, Value) {
    let (status, prepared) = prepare(running, kind, Some(laya_config_id)).await;
    if status != 200 {
        return (status, prepared);
    }
    let request = output(&prepared)["request"].clone();
    let (status, classified) = classify(running, request, key).await;
    if status != 200 {
        return (status, classified);
    }
    let mut input = json!({
        "kind": kind,
        "gate": GATE,
        "question": QUESTION,
        "input": facts(),
        "laya_key": key,
        "laya_config_id": laya_config_id,
        "card_id": card_id,
    });
    if let Some(meta) = meta {
        input["meta"] = meta;
    }
    running
        .invoke("ladder.evaluate", "dev:alice", input, None, None, None)
        .await
}

async fn laya_rows(pg: &support::Pg, seeded: &support::Seeded) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM decision WHERE tenant_id = $1 AND rung = 'laya'")
        .bind(seeded.tenant.as_uuid())
        .fetch_one(&pg.super_pool)
        .await
        .expect("count the laya decisions")
}

/// An approved `refinement.publish` gate naming `policy_id` (§7.2).
///
/// Requested through the real action as `dev:alice`, whose request carries the policy id in
/// `meta.target` (a parentless gate is legitimate exactly then), and decided by the distinct
/// reviewer: the requester cannot decide the gate that judges their own change.
async fn approved_refinement_gate(running: &support::Running, policy_id: &str) -> String {
    let (status, approval) = running
        .invoke(
            "approvals.request",
            "dev:alice",
            json!({
                "gate": "refinement.publish",
                "kind": "decision",
                "description": "Change the refinement policy",
                "target": policy_id
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{approval}");
    let approval_id = id_of(output(&approval), "id");
    let (status, decided) = running
        .invoke(
            "approvals.decide",
            support::REVIEWER,
            json!({"approval_id": approval_id, "decision": "approve"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{decided}");
    approval_id
}

/// P1.1 — a rule, a laya and a human decision share one audit shape.
#[tokio::test(flavor = "multi_thread")]
async fn a_rule_a_laya_and_a_human_decision_share_one_audit_shape() {
    let _env = LAYA_ENV.lock().await;
    let (pg, running, seeded) = fixture().await;
    create_policy(&running, KIND, Some(GATE), 0.8, "closed", "handover").await;
    let laya_config_id = create_laya_config(&running).await;
    seal_laya_token(&pg, &seeded).await;
    let _stub = laya_stub(json!({"score": 0.9})).await;

    // L0 — a deterministic rule outcome.
    let (status, rule) = running
        .invoke(
            "ladder.record",
            "dev:alice",
            json!({
                "rung": "rule",
                "kind": KIND,
                "gate": GATE,
                "answer": "pass",
                "outcome": "accepted",
                "confidence": 1.0,
                "latency_ms": 0
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{rule}");

    // L1 — the verified laya choice.
    let (status, laya) = laya_decision(&running, KIND, &laya_config_id, "key-shape").await;
    assert_eq!(status, 201, "{laya}");

    // L3 — a human decision, recorded against its approval.
    let (status, project) = running
        .invoke(
            "projects.create",
            "dev:alice",
            json!({"title": "Human decision project", "project_type_code": "delivery"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{project}");
    let project_id = id_of(output(&project), "id");
    let (status, approval) = running
        .invoke(
            "approvals.request",
            "dev:alice",
            json!({
                "project_id": project_id,
                "gate": GATE,
                "kind": "decision",
                "description": "Confirm the gate"
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{approval}");
    let approval_id = id_of(output(&approval), "id");
    // A human decision must cite an approval that actually resolved in its favour; the
    // requester cannot decide their own gate, so the fixture's second admin approves it.
    let (status, decided) = running
        .invoke(
            "approvals.decide",
            support::REVIEWER,
            json!({"approval_id": approval_id, "decision": "approve"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{decided}");
    let (status, human) = running
        .invoke(
            "ladder.record",
            "dev:alice",
            json!({
                "rung": "human",
                "kind": KIND,
                "gate": GATE,
                "project_id": project_id,
                "answer": "approve",
                "outcome": "accepted",
                "confidence": 1.0,
                "latency_ms": 0,
                "approval_id": approval_id
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{human}");

    // The one shape: the three rows carry the same columns, and only their values differ.
    let (status, list) = running
        .invoke(
            "ladder.list",
            "dev:alice",
            json!({"kind": KIND}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{list}");
    let items = output(&list)["items"].as_array().expect("items");
    assert_eq!(items.len(), 3, "{list}");

    let rungs: Vec<&str> = items
        .iter()
        .map(|row| row["rung"].as_str().expect("rung"))
        .collect();
    assert!(rungs.contains(&"rule"), "{rungs:?}");
    assert!(rungs.contains(&"laya"), "{rungs:?}");
    assert!(rungs.contains(&"human"), "{rungs:?}");

    let shape: Vec<Vec<&str>> = items
        .iter()
        .map(|row| {
            let mut keys: Vec<&str> = row
                .as_object()
                .expect("a decision object")
                .keys()
                .map(String::as_str)
                .collect();
            keys.sort_unstable();
            keys
        })
        .collect();
    assert_eq!(
        shape[0], shape[1],
        "the rule and laya rows disagree on columns"
    );
    assert_eq!(
        shape[1], shape[2],
        "the laya and human rows disagree on columns"
    );

    for row in items {
        assert_eq!(row["kind"], KIND, "{row}");
        assert!(
            row["policy_id"].is_string(),
            "every rung names its policy: {row}"
        );
        assert!(row["policy_version"].is_number(), "{row}");
        assert!(row["answer"].is_string(), "{row}");
        assert!(row["confidence"].is_number(), "{row}");
        assert!(row["actor_kind"].is_string(), "{row}");
        assert!(row["latency_ms"].is_number(), "{row}");
    }

    let human_row = items
        .iter()
        .find(|row| row["rung"] == "human")
        .expect("the human row");
    assert_eq!(human_row["approval_id"], approval_id, "{human_row}");
    let laya_row = items
        .iter()
        .find(|row| row["rung"] == "laya")
        .expect("the laya row");
    assert_eq!(laya_row["outcome"], "accepted", "{laya_row}");
    assert!(
        laya_row["laya_invocation_id"].is_string(),
        "the laya row names its verified invocation: {laya_row}"
    );
}

/// P2.1 — a tenant refinement changes the threshold with no code change.
#[tokio::test(flavor = "multi_thread")]
async fn a_refinement_changes_a_threshold_without_a_code_change() {
    let _env = LAYA_ENV.lock().await;
    let (pg, running, seeded) = fixture().await;
    let (policy_id, _etag) =
        create_policy(&running, KIND, Some(GATE), 0.8, "closed", "block").await;

    // An L1 decision pins the laya config it used, so one must exist.
    let laya_config_id = create_laya_config(&running).await;
    seal_laya_token(&pg, &seeded).await;
    let _stub = laya_stub(json!({"score": 0.7})).await;

    // The platform policy at 0.8 refuses a 0.7 answer.
    let (status, refused) = laya_decision(&running, KIND, &laya_config_id, "key-before").await;
    assert_eq!(status, 201, "{refused}");
    assert_eq!(output(&refused)["outcome"], "refused", "{refused}");
    assert_eq!(output(&refused)["policy_version"], 1, "{refused}");

    // The tenant publishes a 0.5 refinement — configuration, not a build, but a gated policy
    // change (§7.2): it cites the approved refinement.publish gate naming this policy.
    let approval_id = approved_refinement_gate(&running, &policy_id).await;
    let (status, published) = running
        .invoke(
            "refinements.publish",
            "dev:alice",
            json!({
                "policy_id": policy_id,
                "threshold_override": 0.5,
                "approval_id": approval_id
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{published}");
    assert_eq!(output(&published)["version"], 1, "{published}");

    // The identical request is now accepted, and the row pins the refinement it applied.
    let (status, accepted) = laya_decision(&running, KIND, &laya_config_id, "key-after").await;
    assert_eq!(status, 201, "{accepted}");
    assert_eq!(output(&accepted)["outcome"], "accepted", "{accepted}");
    assert_eq!(output(&accepted)["refinement_version"], 1, "{accepted}");
    assert_eq!(output(&accepted)["policy_version"], 1, "{accepted}");
    assert_eq!(
        output(&accepted)["policy_refinement_id"],
        published["output"]["id"],
        "{accepted}"
    );
}

/// §7.2 — changing a refinement is a gated policy change: a principal holding
/// `governance:write` cannot publish an override alone, and the refusal writes no row.
#[tokio::test(flavor = "multi_thread")]
async fn a_refinement_cannot_publish_without_its_gate() {
    let (_pg, running, _seeded) = fixture().await;
    let (policy_id, _etag) =
        create_policy(&running, KIND, Some(GATE), 0.8, "closed", "block").await;

    let (status, refused) = running
        .invoke(
            "refinements.publish",
            "dev:alice",
            json!({"policy_id": policy_id, "threshold_override": 0.5}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{refused}");
    assert_eq!(
        output(&refused)["errors"][0]["field"],
        "approval_id",
        "{refused}"
    );
    assert_eq!(
        output(&refused)["errors"][0]["code"],
        "approval_missing",
        "{refused}"
    );

    // The refusal runs before the write: the policy has no refinement row and no version.
    let (status, list) = running
        .invoke(
            "refinements.list",
            "dev:alice",
            json!({"policy_id": policy_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{list}");
    assert_eq!(
        output(&list)["items"].as_array().expect("items").len(),
        0,
        "a refused publish wrote a refinement row: {list}"
    );
}

/// §7.2 — the gate names the policy it changes: an approved gate for another policy cannot
/// authorise this one.
#[tokio::test(flavor = "multi_thread")]
async fn a_refinement_gate_for_another_policy_cannot_authorise_this_one() {
    let (_pg, running, _seeded) = fixture().await;
    let (policy_id, _etag) =
        create_policy(&running, KIND, Some(GATE), 0.8, "closed", "block").await;
    let (other_policy_id, _other_etag) = create_policy(
        &running,
        "gate.precheck.other",
        Some(GATE),
        0.8,
        "closed",
        "block",
    )
    .await;

    // A real approved human gate — but its target is the other policy.
    let other_gate = approved_refinement_gate(&running, &other_policy_id).await;
    let (status, refused) = running
        .invoke(
            "refinements.publish",
            "dev:alice",
            json!({
                "policy_id": policy_id,
                "threshold_override": 0.5,
                "approval_id": other_gate
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{refused}");
    assert_eq!(
        output(&refused)["errors"][0]["field"],
        "approval_id",
        "{refused}"
    );
    assert_eq!(
        output(&refused)["errors"][0]["code"],
        "approval_missing",
        "{refused}"
    );

    let (status, list) = running
        .invoke(
            "refinements.list",
            "dev:alice",
            json!({"policy_id": policy_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{list}");
    assert_eq!(
        output(&list)["items"].as_array().expect("items").len(),
        0,
        "a gate for another policy authorised a refinement: {list}"
    );
}

/// §7.2 — an approved human gate naming this policy publishes, and each gated publish advances
/// the refinement's version.
#[tokio::test(flavor = "multi_thread")]
async fn a_human_approved_refinement_gate_publishes() {
    let (_pg, running, _seeded) = fixture().await;
    let (policy_id, _etag) =
        create_policy(&running, KIND, Some(GATE), 0.8, "closed", "block").await;

    let first_gate = approved_refinement_gate(&running, &policy_id).await;
    let (status, published) = running
        .invoke(
            "refinements.publish",
            "dev:alice",
            json!({
                "policy_id": policy_id,
                "threshold_override": 0.5,
                "approval_id": first_gate
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{published}");
    assert_eq!(output(&published)["version"], 1, "{published}");

    // A second gated change republishes the same refinement at version 2.
    let second_gate = approved_refinement_gate(&running, &policy_id).await;
    let (status, republished) = running
        .invoke(
            "refinements.publish",
            "dev:alice",
            json!({
                "policy_id": policy_id,
                "threshold_override": 0.4,
                "approval_id": second_gate
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{republished}");
    assert_eq!(output(&republished)["version"], 2, "{republished}");
    assert_eq!(
        output(&republished)["id"],
        published["output"]["id"],
        "a republish must advance the one refinement, not add a second: {republished}"
    );
}

/// P3.1 — every decision pins the versions it read; a later change does not rewrite the row.
#[tokio::test(flavor = "multi_thread")]
async fn every_decision_pins_the_refinement_and_laya_config_versions() {
    let _env = LAYA_ENV.lock().await;
    let (pg, running, seeded) = fixture().await;
    let (policy_id, etag) = create_policy(&running, KIND, Some(GATE), 0.8, "closed", "block").await;
    let approval_id = approved_refinement_gate(&running, &policy_id).await;
    let (status, published) = running
        .invoke(
            "refinements.publish",
            "dev:alice",
            json!({
                "policy_id": policy_id,
                "threshold_override": 0.6,
                "approval_id": approval_id
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{published}");

    // A laya decision with no laya config to pin is refused before any external call: the pin
    // is mandatory, not a default.
    let (status, unpinned) = prepare(&running, KIND, None).await;
    assert_eq!(status, 422, "{unpinned}");
    assert_eq!(
        output(&unpinned)["errors"][0]["code"],
        "pins_incomplete",
        "{unpinned}"
    );

    let laya_config_id = create_laya_config(&running).await;
    seal_laya_token(&pg, &seeded).await;
    let _stub = laya_stub(json!({"score": 0.9})).await;

    let (status, first) = laya_decision(&running, KIND, &laya_config_id, "key-v1").await;
    assert_eq!(status, 201, "{first}");
    assert_eq!(output(&first)["policy_version"], 1, "{first}");
    assert_eq!(output(&first)["refinement_version"], 1, "{first}");
    assert_eq!(output(&first)["laya_config_version"], 1, "{first}");

    // The policy moves to version 2. The refinement pinned version 1's content, so it is now
    // stale: the ladder refuses it instead of applying the old override to new thresholds.
    let (status, updated) = running
        .invoke(
            "decision_policies.update",
            "dev:alice",
            json!({"id": policy_id, "threshold": 0.7}),
            Some(&etag),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{updated}");
    assert_eq!(output(&updated)["version"], 2, "{updated}");
    let (status, stale) = prepare(&running, KIND, Some(&laya_config_id)).await;
    assert_eq!(status, 409, "{stale}");
    assert_eq!(output(&stale)["type"], "base_changed", "{stale}");

    // Republishing the refinement against the new base makes the case usable again; the
    // decision already recorded still pins version 1.
    let second_gate = approved_refinement_gate(&running, &policy_id).await;
    let (status, republished) = running
        .invoke(
            "refinements.publish",
            "dev:alice",
            json!({
                "policy_id": policy_id,
                "threshold_override": 0.7,
                "approval_id": second_gate
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{republished}");
    assert_eq!(output(&republished)["version"], 2, "{republished}");

    let (status, second) = laya_decision(&running, KIND, &laya_config_id, "key-v2").await;
    assert_eq!(status, 201, "{second}");
    assert_eq!(output(&second)["policy_version"], 2, "{second}");
    assert_eq!(output(&second)["refinement_version"], 2, "{second}");

    // The first row's pins are unchanged — the row is reproducible, not rewritten.
    let first_id = id_of(output(&first), "id");
    let pinned: i64 = sqlx::query_scalar("SELECT policy_version FROM decision WHERE id = $1::uuid")
        .bind(&first_id)
        .fetch_one(&pg.super_pool)
        .await
        .expect("read the stored pin");
    assert_eq!(
        pinned, 1,
        "the recorded decision's pin moved with the policy"
    );
}

/// P4.1/P4.2 — a generated draft is applied only through the gated action.
///
/// The compile-time half is the `compile_fail` doctest in `conveyor_domain::ladder`; this test
/// is the runtime half on the same rule: an accepted L1 decision is the only way a generated
/// draft reaches a card, and the ordinary edit path refuses the generated class outright.
#[tokio::test(flavor = "multi_thread")]
async fn a_generated_draft_cannot_transition_a_card() {
    let _env = LAYA_ENV.lock().await;
    let (pg, running, seeded) = fixture().await;
    seal_laya_token(&pg, &seeded).await;
    // The merged contract reads the answer from a verified laya call, not from the caller.
    let _stub = laya_stub(json!({"score": 0.9})).await;
    create_policy(
        &running,
        "gate.precheck",
        Some("capability_ready"),
        0.8,
        "closed",
        "block",
    )
    .await;
    let config = create_laya_config(&running).await;

    // A project, its board, an active lane and a card to apply to.
    let (status, project) = running
        .invoke(
            "projects.create",
            "dev:alice",
            json!({"title": "Draft project", "project_type_code": "delivery"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{project}");
    let project_id = id_of(output(&project), "id");
    let (status, view) = running
        .invoke(
            "projects.get",
            "dev:alice",
            json!({"id": project_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{view}");
    let board_id = id_of(&output(&view)["boards"][0], "id");
    let (status, board) = running
        .invoke(
            "boards.get",
            "dev:alice",
            json!({"id": board_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{board}");
    let lane = output(&board)["lists"]
        .as_array()
        .expect("lists")
        .iter()
        .find(|row| row["list"]["kind"] == "active")
        .unwrap_or_else(|| panic!("no active lane in {board}"));
    let lane_id = id_of(&lane["list"], "id");
    let (status, card) = running
        .invoke(
            "cards.create",
            "dev:alice",
            json!({
                "board_id": board_id,
                "list_id": lane_id,
                "name": "The card the draft wants to move",
                "card_type_code": "feature"
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{card}");
    let card_id = id_of(output(&card), "id");

    // A generated draft artefact.
    let (status, draft) = running
        .invoke(
            "documents.compose",
            "dev:alice",
            json!({
                "title": "Generated patch",
                "project_id": project_id,
                "body": "the model's proposal",
                "meta": {"generated": true}
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{draft}");
    let document_id = id_of(output(&draft), "id");

    // A decision that does not name this draft cannot attach it, whatever its outcome.
    let (status, unrelated) = laya_decision_bound(
        &running,
        "gate.precheck",
        &config,
        "draft-unrelated",
        &card_id,
        None,
    )
    .await;
    assert_eq!(status, 201, "{unrelated}");
    let (status, refused_apply) = running
        .invoke(
            "ladder.apply_draft",
            "dev:alice",
            json!({
                "document_id": document_id,
                "card_id": card_id,
                "decision_id": id_of(output(&unrelated), "id")
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{refused_apply}");

    // The settled L1 path, bound to the card and naming the draft, is the gate.
    let (status, accepted) = laya_decision_bound(
        &running,
        "gate.precheck",
        &config,
        "draft-accepted",
        &card_id,
        Some(json!({"draft_document_id": document_id})),
    )
    .await;
    assert_eq!(status, 201, "{accepted}");
    assert_eq!(output(&accepted)["outcome"], "accepted", "{accepted}");
    let decision_id = id_of(output(&accepted), "id");

    let (status, applied) = running
        .invoke(
            "ladder.apply_draft",
            "dev:alice",
            json!({"document_id": document_id, "card_id": card_id, "decision_id": decision_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{applied}");
    assert_eq!(output(&applied)["card_id"], card_id, "{applied}");

    let (status, read_back) = running
        .invoke(
            "documents.get",
            "dev:alice",
            json!({"id": document_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{read_back}");
    assert_eq!(output(&read_back)["card_id"], card_id, "{read_back}");

    // The ordinary edit path refuses the generated class, so the gate has no bypass.
    let (status, headers, second) = running
        .invoke_full(
            "documents.compose",
            "dev:alice",
            json!({
                "title": "Another generated patch",
                "project_id": project_id,
                "body": "another proposal",
                "meta": {"generated": true}
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{second}");
    let second_id = id_of(output(&second), "id");
    let etag = headers.get("etag").cloned().expect("an ETag on compose");
    let (status, bypass) = running
        .invoke(
            "documents.update",
            "dev:alice",
            json!({"id": second_id, "card_id": card_id}),
            Some(&etag),
            None,
            None,
        )
        .await;
    assert_eq!(
        status, 409,
        "the generated draft was attached by an edit: {bypass}"
    );
    assert_eq!(output(&bypass)["type"], "draft_requires_gate", "{bypass}");
}

// ── v1-w1c-3: the L1 answer has provenance ──────────────────────────────────────────────

/// The acceptance's first half: the old request shape — a confidence the caller supplies — is
/// no longer a valid `ladder.evaluate` input, and it writes nothing.
#[tokio::test(flavor = "multi_thread")]
async fn a_confidence_posted_directly_cannot_produce_a_laya_decision() {
    let (pg, running, seeded) = fixture().await;
    create_policy(&running, KIND, Some(GATE), 0.8, "closed", "block").await;
    let _config = create_laya_config(&running).await;

    let (status, refused) = running
        .invoke(
            "ladder.evaluate",
            "dev:alice",
            json!({
                "kind": KIND,
                "gate": GATE,
                "input": facts(),
                "answer": "pass",
                "confidence": 0.99,
                "probabilities": {"pass": 0.99},
                "latency_ms": 4,
                "laya_key": "never-called"
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{refused}");
    assert_eq!(laya_rows(&pg, &seeded).await, 0);

    let invoked: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM action_invocation
          WHERE tenant_id = $1 AND action = 'ladder.evaluate' AND outcome = 'ok'",
    )
    .bind(seeded.tenant.as_uuid())
    .fetch_one(&pg.super_pool)
    .await
    .expect("count the evaluate invocations");
    assert_eq!(invoked, 0, "a refused evaluate recorded a successful call");
}

/// The Jev bridge refuses a choice with fewer than two options (and more than 255), so the
/// authored set is refused at authoring time: a one-option "choice" is not a choice, and a
/// caller cannot bypass the set by posting its own list.
#[tokio::test(flavor = "multi_thread")]
async fn a_choice_needs_between_two_and_255_options() {
    let (_pg, running, _seeded) = fixture().await;

    let single = json!([{"key": "only", "label": "The only option"}]);
    let (status, refused) = running
        .invoke(
            "decision_options.create",
            "dev:alice",
            json!({"kind": KIND, "gate": GATE, "options": single}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{refused}");
    assert_eq!(
        output(&refused)["errors"][0]["code"],
        "option_set_size",
        "{refused}"
    );

    let many: Vec<Value> = (0..256)
        .map(|n| json!({"key": format!("option_{n}")}))
        .collect();
    let (status, refused) = running
        .invoke(
            "decision_options.create",
            "dev:alice",
            json!({"kind": KIND, "gate": GATE, "options": many}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{refused}");
    assert_eq!(
        output(&refused)["errors"][0]["code"],
        "option_set_size",
        "{refused}"
    );
}

/// Library-first resolution: a published tenant set shadows the platform library for the same
/// `(kind, gate)`, and the decision pins the shadow it read.
#[tokio::test(flavor = "multi_thread")]
async fn a_tenant_option_set_shadows_the_library_set() {
    let _env = LAYA_ENV.lock().await;
    let (pg, running, seeded) = fixture().await;
    create_policy(&running, KIND, Some(GATE), 0.8, "closed", "block").await;
    let laya_config_id = create_laya_config(&running).await;
    seal_laya_token(&pg, &seeded).await;

    let (library_id, _) = seed_library_options(
        &pg,
        seeded.tenant,
        KIND,
        json!([
            {"key": "library_allow", "label": "Library allow"},
            {"key": "library_refuse", "label": "Library refuse"}
        ]),
    )
    .await;
    let tenant = author_options(
        &running,
        KIND,
        json!([
            {"key": "bug", "label": "A bug"},
            {"key": "feature", "label": "A feature"}
        ]),
    )
    .await;
    let tenant_id = id_of(&tenant, "id");
    assert_ne!(library_id.to_string(), tenant_id);

    let _stub = laya_stub(json!({
        "choice": "bug",
        "confidence": 0.91,
        "probabilities": {"bug": 0.91, "feature": 0.09}
    }))
    .await;
    let (status, prepared) = prepare(&running, KIND, Some(&laya_config_id)).await;
    assert_eq!(status, 200, "{prepared}");
    assert_eq!(
        output(&prepared)["request"]["questions"]["decision"]["criteria"],
        json!({"bug": "A bug", "feature": "A feature"}),
        "the tenant set shadows the library: {}",
        output(&prepared)
    );
    let request = output(&prepared)["request"].clone();
    let (status, classified) = classify(&running, request, "key-shadow").await;
    assert_eq!(status, 200, "{classified}");
    let (status, decided) = evaluate(&running, KIND, Some(&laya_config_id), "key-shadow").await;
    assert_eq!(status, 201, "{decided}");
    assert_eq!(output(&decided)["option_set_id"], tenant_id, "{decided}");

    let (status, got) = running
        .invoke(
            "decision_options.get",
            "dev:alice",
            json!({"kind": KIND, "gate": GATE}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{got}");
    assert_eq!(output(&got)["origin"], "tenant", "{got}");
    assert_eq!(output(&got)["id"], tenant_id, "{got}");
}

/// A hidden library option never reaches the laya wire; the refinement is still pinned.
#[tokio::test(flavor = "multi_thread")]
async fn a_hidden_option_is_absent_from_the_wire() {
    let (pg, running, seeded) = fixture().await;
    create_policy(&running, KIND, Some(GATE), 0.8, "closed", "block").await;
    let laya_config_id = create_laya_config(&running).await;
    let (_, entries) = seed_library_options(
        &pg,
        seeded.tenant,
        KIND,
        json!([
            {"key": "allow", "label": "Allow"},
            {"key": "review", "label": "Review"},
            {"key": "refuse", "label": "Refuse"}
        ]),
    )
    .await;

    let approval_id = approved_refinement_gate(&running, &entries[2].to_string()).await;
    let (status, refined) = running
        .invoke(
            "decision_options.refine",
            "dev:alice",
            json!({
                "entry_id": entries[2].to_string(),
                "hidden": true,
                "approval_id": approval_id
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{refined}");
    let options = output(&refined)["options"].as_array().expect("options");
    let hidden = options
        .iter()
        .find(|option| option["key"] == "refuse")
        .expect("the hidden option");
    assert_eq!(hidden["hidden"], true, "{refined}");
    assert!(
        !output(&refined)["refinement_hashes"]
            .as_array()
            .expect("refinement hashes")
            .is_empty(),
        "the applied refinement is pinned: {refined}"
    );

    let (status, prepared) = prepare(&running, KIND, Some(&laya_config_id)).await;
    assert_eq!(status, 200, "{prepared}");
    let criteria = &output(&prepared)["request"]["questions"]["decision"]["criteria"];
    assert_eq!(
        criteria,
        &json!({"allow": "Allow", "review": "Review"}),
        "a hidden option never reaches the wire: {}",
        output(&prepared)
    );
    assert!(criteria.get("refuse").is_none(), "{criteria}");
}

/// A refinement is valid only against the base entry it pinned; a changed base is refused
/// instead of silently re-applied to different content.
#[tokio::test(flavor = "multi_thread")]
async fn a_refinement_whose_base_changed_is_refused() {
    let (pg, running, seeded) = fixture().await;
    create_policy(&running, KIND, Some(GATE), 0.8, "closed", "block").await;
    let laya_config_id = create_laya_config(&running).await;
    let (_, entries) = seed_library_options(
        &pg,
        seeded.tenant,
        KIND,
        json!([
            {"key": "allow", "label": "Allow"},
            {"key": "review", "label": "Review"}
        ]),
    )
    .await;

    let approval_id = approved_refinement_gate(&running, &entries[0].to_string()).await;
    let (status, refined) = running
        .invoke(
            "decision_options.refine",
            "dev:alice",
            json!({
                "entry_id": entries[0].to_string(),
                "label": "Allow with a note",
                "approval_id": approval_id
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{refined}");

    // The platform changes the base entry underneath the tenant's refinement.
    let mut tx = conveyor_store::begin_scoped(&pg.owner, seeded.tenant)
        .await
        .expect("scope the base change");
    sqlx::query("UPDATE decision_option_entry SET label = 'Changed by the platform' WHERE id = $1")
        .bind(entries[0])
        .execute(&mut *tx)
        .await
        .expect("change the base entry");
    tx.commit().await.expect("commit the base change");

    let (status, refused) = prepare(&running, KIND, Some(&laya_config_id)).await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(output(&refused)["type"], "base_changed", "{refused}");
}

/// The decision pins the set version it read; a later edit advances the set without rewriting
/// the earlier decision.
#[tokio::test(flavor = "multi_thread")]
async fn a_decision_pins_the_option_set_version_it_read() {
    let _env = LAYA_ENV.lock().await;
    let (pg, running, seeded) = fixture().await;
    create_policy(&running, KIND, Some(GATE), 0.8, "closed", "block").await;
    let laya_config_id = create_laya_config(&running).await;
    seal_laya_token(&pg, &seeded).await;
    let set = author_options(
        &running,
        KIND,
        json!([
            {"key": "bug", "label": "A bug"},
            {"key": "feature", "label": "A feature"}
        ]),
    )
    .await;
    let set_id = id_of(&set, "id");
    let _stub = laya_stub(json!({
        "choice": "bug",
        "confidence": 0.9,
        "probabilities": {"bug": 0.9, "feature": 0.1}
    }))
    .await;

    let (status, decided) = laya_decision(&running, KIND, &laya_config_id, "key-versioned").await;
    assert_eq!(status, 201, "{decided}");
    assert_eq!(output(&decided)["option_set_version"], 2, "{decided}");
    assert_eq!(output(&decided)["option_set_id"], set_id, "{decided}");

    // A later edit advances the set; the earlier decision keeps the version it read.
    let (status, updated) = running
        .invoke(
            "decision_options.update",
            "dev:alice",
            json!({
                "id": set_id,
                "options": [
                    {"key": "bug", "label": "A bug"},
                    {"key": "feature", "label": "A feature"},
                    {"key": "question", "label": "A question"}
                ]
            }),
            Some("2"),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{updated}");
    assert_eq!(output(&updated)["version"], 3, "{updated}");

    let decision_id = id_of(output(&decided), "id");
    let (status, listed) = running
        .invoke(
            "ladder.list",
            "dev:alice",
            json!({"kind": KIND}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{listed}");
    let row = output(&listed)["items"]
        .as_array()
        .expect("items")
        .iter()
        .find(|row| row["id"] == decision_id.as_str())
        .expect("the decision row");
    assert_eq!(row["option_set_version"], 2, "{row}");
    assert_eq!(row["option_set_id"], set_id, "{row}");
}

/// Options are never a caller's list: both ladder inputs refuse one, and a laya answer outside
/// the authored set is a 422 that writes no decision.
#[tokio::test(flavor = "multi_thread")]
async fn a_caller_cannot_supply_its_own_options() {
    let _env = LAYA_ENV.lock().await;
    let (pg, running, seeded) = fixture().await;
    create_policy(&running, KIND, Some(GATE), 0.8, "closed", "block").await;
    let laya_config_id = create_laya_config(&running).await;
    seal_laya_token(&pg, &seeded).await;
    let _set = author_options(
        &running,
        KIND,
        json!([
            {"key": "bug", "label": "A bug"},
            {"key": "feature", "label": "A feature"}
        ]),
    )
    .await;

    let (status, refused) = running
        .invoke(
            "ladder.prepare",
            "dev:alice",
            json!({
                "kind": KIND,
                "gate": GATE,
                "question": QUESTION,
                "input": facts(),
                "options": [{"key": "bug"}, {"key": "feature"}]
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{refused}");

    let (status, refused) = running
        .invoke(
            "ladder.evaluate",
            "dev:alice",
            json!({
                "kind": KIND,
                "gate": GATE,
                "laya_key": "unused",
                "options": [{"key": "bug"}, {"key": "feature"}]
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{refused}");

    // laya answers outside the authored set: refused, not recorded.
    let _stub = laya_stub(json!({
        "choice": "outside",
        "confidence": 0.99,
        "probabilities": {"outside": 0.99}
    }))
    .await;
    let (status, prepared) = prepare(&running, KIND, Some(&laya_config_id)).await;
    assert_eq!(status, 200, "{prepared}");
    let request = output(&prepared)["request"].clone();
    let (status, classified) = classify(&running, request, "key-outside").await;
    assert_eq!(status, 200, "{classified}");
    let (status, refused) = evaluate(&running, KIND, Some(&laya_config_id), "key-outside").await;
    assert_eq!(status, 422, "{refused}");
    assert_eq!(
        output(&refused)["errors"][0]["code"],
        "answer_not_in_set",
        "{refused}"
    );
    assert_eq!(laya_rows(&pg, &seeded).await, 0);
}

/// A policy refinement pins the policy content it was authored against; a later policy edit
/// refuses the stale refinement instead of applying it to different thresholds.
#[tokio::test(flavor = "multi_thread")]
async fn a_policy_refinement_whose_base_changed_is_refused() {
    let (_pg, running, _seeded) = fixture().await;
    let (policy_id, etag) = create_policy(&running, KIND, Some(GATE), 0.8, "closed", "block").await;
    let laya_config_id = create_laya_config(&running).await;

    let approval_id = approved_refinement_gate(&running, &policy_id).await;
    let (status, published) = running
        .invoke(
            "refinements.publish",
            "dev:alice",
            json!({
                "policy_id": policy_id,
                "threshold_override": 0.5,
                "approval_id": approval_id
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{published}");
    assert!(
        output(&published)["base_hash"]
            .as_str()
            .is_some_and(|hash| hash.len() == 64),
        "the refinement pins the policy content it read: {published}"
    );

    // The policy changes after the refinement was published.
    let (status, headers, updated) = running
        .invoke_full(
            "decision_policies.update",
            "dev:alice",
            json!({"id": policy_id, "threshold": 0.7}),
            Some(etag.as_str()),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{updated}");
    assert!(headers.contains_key("etag"), "an ETag on update");

    let (status, refused) = prepare(&running, KIND, Some(&laya_config_id)).await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(output(&refused)["type"], "base_changed", "{refused}");
}

/// A key no successful `laya.classify` call carries — forged, or the key of a call that was
/// made with a different document — cannot be cited.
#[tokio::test(flavor = "multi_thread")]
async fn a_forged_laya_key_is_refused() {
    let _env = LAYA_ENV.lock().await;
    let (pg, running, seeded) = fixture().await;
    create_policy(&running, KIND, Some(GATE), 0.8, "closed", "block").await;
    let laya_config_id = create_laya_config(&running).await;
    seal_laya_token(&pg, &seeded).await;
    let stub = laya_stub(json!({"score": 0.9})).await;

    // A key nothing ever ran under.
    let (status, forged) = evaluate(&running, KIND, Some(&laya_config_id), "forged-key").await;
    assert_eq!(status, 503, "{forged}");
    assert_eq!(output(&forged)["type"], "gate_unavailable", "{forged}");

    // A real, successful call — cited by a key that names a different one.
    let (status, prepared) = prepare(&running, KIND, Some(&laya_config_id)).await;
    assert_eq!(status, 200, "{prepared}");
    let request = output(&prepared)["request"].clone();
    let (status, classified) = classify(&running, request, "used-key").await;
    assert_eq!(status, 200, "{classified}");
    let (status, mismatched) = evaluate(&running, KIND, Some(&laya_config_id), "other-key").await;
    assert_eq!(status, 503, "{mismatched}");
    assert_eq!(
        output(&mismatched)["type"],
        "gate_unavailable",
        "{mismatched}"
    );
    assert_eq!(laya_rows(&pg, &seeded).await, 0);
    assert_eq!(stub.hits.get(), 1, "only the cited call reached laya");
}

/// A wire body that differs from the one `ladder.evaluate` rebuilds is a different request: the
/// idempotency hash stops matching, so the tampered answer cannot be recorded.
#[tokio::test(flavor = "multi_thread")]
async fn a_tampered_laya_request_does_not_verify() {
    let _env = LAYA_ENV.lock().await;
    let (pg, running, seeded) = fixture().await;
    create_policy(&running, KIND, Some(GATE), 0.8, "closed", "block").await;
    let laya_config_id = create_laya_config(&running).await;
    seal_laya_token(&pg, &seeded).await;
    let stub = laya_stub(json!({"score": 0.99})).await;

    let (status, prepared) = prepare(&running, KIND, Some(&laya_config_id)).await;
    assert_eq!(status, 200, "{prepared}");
    let mut request = output(&prepared)["request"].clone();
    request["state"] = json!("a different question with a friendlier answer");
    let (status, classified) = classify(&running, request, "tampered-key").await;
    assert_eq!(status, 200, "{classified}");

    let (status, refused) = evaluate(&running, KIND, Some(&laya_config_id), "tampered-key").await;
    assert_eq!(status, 503, "{refused}");
    assert_eq!(output(&refused)["type"], "gate_unavailable", "{refused}");
    assert!(
        output(&refused)["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("not this decision's request"),
        "{refused}"
    );
    assert_eq!(laya_rows(&pg, &seeded).await, 0);
    assert_eq!(stub.hits.get(), 1);
}

/// The acceptance's success half: the answer, confidence, probabilities, latency and every
/// provenance pin come from the verified response and its invocation.
#[tokio::test(flavor = "multi_thread")]
async fn a_laya_decision_derives_its_fields_latency_and_provenance_from_the_response() {
    let _env = LAYA_ENV.lock().await;
    let (pg, running, seeded) = fixture().await;
    create_policy(&running, KIND, Some(GATE), 0.8, "closed", "block").await;
    let laya_config_id = create_laya_config(&running).await;
    seal_laya_token(&pg, &seeded).await;
    let answer = json!({
        "choice": "bug",
        "confidence": 0.93,
        "probabilities": {"bug": 0.93, "feature": 0.07}
    });
    let stub = laya_stub(answer.clone()).await;
    // The choice is authored, published and resolved by the case — never posted with the
    // answer.
    let set = author_options(
        &running,
        KIND,
        json!([
            {"key": "bug", "label": "A bug"},
            {"key": "feature", "label": "A feature"}
        ]),
    )
    .await;
    let set_id = id_of(&set, "id");
    assert_eq!(set["version"], 2, "create then publish: {set}");

    let (status, prepared) = prepare(&running, KIND, Some(&laya_config_id)).await;
    assert_eq!(status, 200, "{prepared}");
    let request = output(&prepared)["request"].clone();
    assert_eq!(request["model"], "jev-latest", "{request}");
    assert_eq!(
        request["questions"]["decision"]["type"], "choice",
        "{request}"
    );
    assert_eq!(
        request["questions"]["decision"]["criteria"],
        json!({"bug": "A bug", "feature": "A feature"}),
        "the bridge reads a choice's criteria as a map of option -> description: {request}"
    );
    let state = request["state"].as_str().expect("state is a string");
    assert!(state.contains(QUESTION), "{state}");
    assert!(state.contains("knowledge@7"), "{state}");
    let request_hash = output(&prepared)["request_hash"]
        .as_str()
        .expect("a request hash")
        .to_owned();
    assert!(request_hash.starts_with("fnv1a64:"), "{request_hash}");

    let (status, classified) = classify(&running, request, "key-provenance").await;
    assert_eq!(status, 200, "{classified}");

    let (status, decided) = evaluate(&running, KIND, Some(&laya_config_id), "key-provenance").await;
    assert_eq!(status, 201, "{decided}");
    let decision = output(&decided);
    assert_eq!(decision["rung"], "laya", "{decision}");
    assert_eq!(decision["answer"], "bug", "{decision}");
    // The decision pins the authored set, its published version and the effective options'
    // canonical hash; no refinement applied.
    assert_eq!(decision["option_set_id"], set_id, "{decision}");
    assert_eq!(decision["option_set_version"], 2, "{decision}");
    assert!(
        decision["option_set_hash"]
            .as_str()
            .is_some_and(|hash| hash.len() == 64),
        "the decision pins the effective option-set hash: {decision}"
    );
    assert_eq!(
        decision["option_refinement_hashes"],
        json!([]),
        "{decision}"
    );
    assert_eq!(decision["confidence"], 0.93, "{decision}");
    assert_eq!(
        decision["probabilities"],
        json!({"bug": 0.93, "feature": 0.07}),
        "{decision}"
    );
    assert_eq!(decision["outcome"], "accepted", "{decision}");
    assert_eq!(decision["laya_request_hash"], request_hash, "{decision}");
    // The decision hashes the whole answer document the parser consumes (`{"answers": …}`),
    // not the inner choice; the stub's body is that document.
    let expected_response_hash = conveyor_actions::spec::sha256_hex(
        &serde_json::to_vec(&json!({"answers": {"decision": answer.clone()}}))
            .expect("the answer document"),
    );
    assert_eq!(
        decision["laya_response_hash"], expected_response_hash,
        "{decision}"
    );

    // The latency is the engine's own measurement of the external call, and the invocation id
    // names the row the call recorded — not anything the caller posted.
    let invocation_id = decision["laya_invocation_id"]
        .as_str()
        .expect("the invocation id");
    let (action, duration_ms, key): (String, i32, Option<String>) = sqlx::query_as(
        "SELECT action, duration_ms, idempotency_key
           FROM action_invocation WHERE id = $1::uuid",
    )
    .bind(invocation_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("read the cited invocation");
    assert_eq!(action, "laya.classify");
    assert_eq!(key.as_deref(), Some("key-provenance"));
    assert_eq!(
        decision["latency_ms"].as_i64(),
        Some(i64::from(duration_ms)),
        "{decision}"
    );
    assert_eq!(stub.hits.get(), 1);
    assert_eq!(laya_rows(&pg, &seeded).await, 1);
}

/// A closed safety gate's laya outage applies `refused` and records the outage as an
/// `unavailable` decision. The refusal is 503 `gate_unavailable`; the outcome survives the
/// handler transaction's rollback in a decision row, never as a silent pass (§7.2,
/// finding v1-w1c-5).
#[tokio::test(flavor = "multi_thread")]
async fn a_closed_laya_outage_records_a_refused_unavailable_decision() {
    let (pg, running, seeded) = fixture().await;
    create_policy(&running, KIND, Some(GATE), 0.8, "closed", "block").await;
    let laya_config_id = create_laya_config(&running).await;
    // Deliberately no `laya.token`: the classify call answers 503.

    let (status, prepared) = prepare(&running, KIND, Some(&laya_config_id)).await;
    assert_eq!(status, 200, "{prepared}");
    let request = output(&prepared)["request"].clone();
    let (status, classified) = classify(&running, request, "key-outage").await;
    assert_eq!(status, 503, "{classified}");
    let (status, refused) = evaluate(&running, KIND, Some(&laya_config_id), "key-outage").await;
    assert_eq!(status, 503, "{refused}");
    assert_eq!(output(&refused)["type"], "gate_unavailable", "{refused}");
    assert!(
        output(&refused)["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("fail_policy=closed outcome=refused"),
        "{refused}"
    );

    let (rung, outcome, answer, policy_version, laya_config_version, meta): (
        String,
        String,
        String,
        Option<i64>,
        Option<i64>,
        Value,
    ) = sqlx::query_as(
        "SELECT rung, outcome, answer, policy_version, laya_config_version, meta
           FROM decision WHERE tenant_id = $1 AND kind = $2",
    )
    .bind(seeded.tenant.as_uuid())
    .bind(KIND)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the outage decision");
    assert_eq!(rung, "rule", "the fail policy is the rule that decided");
    assert_eq!(outcome, "refused", "a closed policy refuses");
    assert_eq!(answer, "unavailable");
    assert_eq!(meta["decision"], "unavailable", "{meta}");
    assert_eq!(meta["fail_policy"], "closed", "{meta}");
    assert_eq!(meta["action"], "block", "{meta}");
    assert!(
        meta.get("note").is_none(),
        "a closed outage records no note"
    );
    assert!(policy_version.is_some(), "the row pins the policy version");
    assert!(
        laya_config_version.is_some(),
        "the row pins the laya config version"
    );
    assert_eq!(
        laya_rows(&pg, &seeded).await,
        0,
        "the outage faked an L1 row"
    );
}

/// An open routing gate's laya outage applies its configured action: with `annotate` the
/// outcome is `accepted` and the decision records the note an operator audits. The refusal is
/// still 503 `gate_unavailable`, never a silent pass (§7.2, finding v1-w1c-5).
#[tokio::test(flavor = "multi_thread")]
async fn an_open_laya_outage_records_its_action_and_annotate_note() {
    let (pg, running, seeded) = fixture().await;
    create_policy(&running, KIND, Some(GATE), 0.8, "open", "annotate").await;
    let laya_config_id = create_laya_config(&running).await;
    // Deliberately no `laya.token`: the classify call answers 503.

    let (status, prepared) = prepare(&running, KIND, Some(&laya_config_id)).await;
    assert_eq!(status, 200, "{prepared}");
    let request = output(&prepared)["request"].clone();
    let (status, classified) = classify(&running, request, "key-open-outage").await;
    assert_eq!(status, 503, "{classified}");
    let (status, open) = evaluate(&running, KIND, Some(&laya_config_id), "key-open-outage").await;
    assert_eq!(status, 503, "{open}");
    assert_eq!(output(&open)["type"], "gate_unavailable", "{open}");
    assert!(
        output(&open)["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("fail_policy=open outcome=accepted"),
        "{open}"
    );

    let (rung, outcome, answer, meta): (String, String, String, Value) = sqlx::query_as(
        "SELECT rung, outcome, answer, meta
           FROM decision WHERE tenant_id = $1 AND kind = $2",
    )
    .bind(seeded.tenant.as_uuid())
    .bind(KIND)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the outage decision");
    assert_eq!(rung, "rule", "the fail policy is the rule that decided");
    assert_eq!(outcome, "accepted", "an open annotate policy proceeds");
    assert_eq!(answer, "unavailable");
    assert_eq!(meta["decision"], "unavailable", "{meta}");
    assert_eq!(meta["fail_policy"], "open", "{meta}");
    assert_eq!(meta["action"], "annotate", "{meta}");
    let note = meta["note"].as_str().expect("the annotate note");
    assert!(note.contains("laya unavailable"), "{note}");
    assert_eq!(
        laya_rows(&pg, &seeded).await,
        0,
        "the outage faked an L1 row"
    );
}

// ── the seam's budget (SPEC_V2 §10.2, finding v1-p102-1) ────────────────────────────────

/// §10.2 fixes the conveyor→laya seam at a 1500 ms default: a config that names no timeout
/// stores that default, never the old 30 s.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_laya_config_defaults_to_fifteen_hundred_ms() {
    let (_pg, running, _seeded) = fixture().await;
    let (status, created) = running
        .invoke(
            "laya_configs.create",
            "dev:alice",
            json!({"model_ref": "jev-latest"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    assert_eq!(
        output(&created)["timeout_ms"],
        1500,
        "the seam's measured default is 1500 ms (SPEC_V2 §10.2)"
    );
}

/// A configured budget above the seam's hard budget is refused as a field failure — never
/// silently clamped — and the hard budget itself is accepted.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_laya_config_timeout_is_capped_at_the_hard_budget() {
    let (_pg, running, _seeded) = fixture().await;
    let (status, refused) = running
        .invoke(
            "laya_configs.create",
            "dev:alice",
            json!({"model_ref": "jev-latest", "timeout_ms": 3001}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{refused}");
    assert_eq!(
        output(&refused)["type"],
        "timeout_above_budget",
        "{refused}"
    );
    assert_eq!(refused["errors"][0]["field"], "timeout_ms", "{refused}");
    assert_eq!(
        refused["errors"][0]["code"], "timeout_above_budget",
        "{refused}"
    );

    let (status, accepted) = running
        .invoke(
            "laya_configs.create",
            "dev:alice",
            json!({"model_ref": "jev-latest", "timeout_ms": 3000}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{accepted}");
    assert_eq!(output(&accepted)["timeout_ms"], 3000, "{accepted}");
    // The dispatch bound itself is exercised by the stalled-judge test; this test is the config
    // boundary: 3001 refused, 3000 accepted.
}
