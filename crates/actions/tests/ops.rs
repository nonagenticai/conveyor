//! T7 — the incident lifecycle and the write family's acceptance (issue #78).
//!
//! * P1.1 an incident is triaged with no cluster write;
//! * P1.2 an approved mitigation sets `mitigated_at` with the approver, the verb version and the
//!   read-back;
//! * P1.3 a resolution writes one linked postmortem (and a kata candidate only when systemic);
//! * P2.2 the tenant's quorum decides: one of two decisions stays refused, one of one proceeds;
//! * P3.1 restart, scale, rollback and flag-flip each run, return a read-back and are recorded;
//! * P3.2 a verb outside the four is refused before any request is made.
//!
//! `rbuild -- test -p conveyor-actions --test ops`.

mod support;

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::routing::get;
use conveyor_domain::ListKind;
use conveyor_store as store;
use rust_decimal::Decimal;
use serde_json::{Value, json};
use uuid::Uuid;

const TOKEN: &str = "dev:alice";
const REVIEWER: &str = "dev:reviewer";
const KATA_KEY: &str = "kata.open";
const DEPLOYMENT_PATH: &str = "/apis/apps/v1/namespaces/conveyor/deployments/conveyor";

/// The ops client reads its address and token from the process environment, and these tests
/// each run their own stub. Serialise them so one test never writes through another's stub.
/// A `tokio` mutex, because the guard is held across the awaits the test performs.
static ENV: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn env_lock() -> tokio::sync::MutexGuard<'static, ()> {
    ENV.lock().await
}

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

fn text(value: &Value, field: &str) -> String {
    value[field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} is not a string in {value}"))
        .to_owned()
}

/// A project with a board and one active lane.
async fn fixture() -> (
    support::Pg,
    support::Running,
    support::Seeded,
    String,
    String,
) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");
    // The full export carries the `incident` card type; the light seed above carries only the
    // three types a project needs.
    let export = store::seed::catalogue_export().expect("the catalogue export");
    let incident: Vec<_> = export
        .card_types
        .into_iter()
        .filter(|card_type| card_type.code == "incident")
        .collect();
    assert_eq!(incident.len(), 1, "the catalogue seed carries `incident`");
    let mut tx = store::begin_scoped(&pg.owner, seeded.tenant)
        .await
        .expect("scoped tx");
    store::catalogue::import_card_types_tx(&mut tx, seeded.tenant, &incident)
        .await
        .expect("import the incident card type");
    tx.commit().await.expect("commit the card type");

    let mut tx = store::begin_scoped(&pg.owner, seeded.tenant)
        .await
        .expect("scoped tx");
    let project = store::project::insert_project_tx(
        &mut tx,
        seeded.tenant,
        &store::project::NewProject::new(format!("O{}", Uuid::now_v7().simple()), "Ops fixture"),
    )
    .await
    .expect("the project");
    let board = store::board::insert_board_tx(
        &mut tx,
        seeded.tenant,
        &store::board::NewBoard::new(project.id, "Board"),
    )
    .await
    .expect("the board");
    let active = store::list::insert_list_tx(
        &mut tx,
        seeded.tenant,
        &store::list::NewList::new(board.id, "Active", Decimal::ZERO, ListKind::Active),
    )
    .await
    .expect("the active lane");
    tx.commit().await.expect("commit the fixture board");

    let running = support::Running::start(&pg).await;
    (
        pg,
        running,
        seeded,
        board.id.to_string(),
        active.id.to_string(),
    )
}

async fn open_incident(
    running: &support::Running,
    board: &str,
    lane: &str,
    severity: &str,
) -> String {
    let (status, body) = running
        .invoke(
            "ops.incident",
            TOKEN,
            json!({
                "board_id": board,
                "list_id": lane,
                "service": "conveyor",
                "severity": severity,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    text(output(&body), "id")
}

async fn triage(running: &support::Running, incident: &str, runbook: &str) -> (u16, Value) {
    running
        .invoke(
            "ops.triage",
            TOKEN,
            json!({
                "incident_id": incident,
                "severity": "sev2",
                "runbook_procedure_id": runbook,
                "narrative": "the pod crash-loops after the image roll",
            }),
            None,
            None,
            None,
        )
        .await
}

async fn publish_runbook(running: &support::Running, key: &str) -> String {
    let (status, body) = running
        .invoke(
            "procedures.create",
            TOKEN,
            json!({
                "key": key,
                "title": format!("SOP {key}"),
                "acceptance_criteria": ["the step's output is complete"],
                "inspection": {"kind": "checker", "rule": "always"},
                "outputs": [],
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let id = text(output(&body), "id");
    let (status, body) = running
        .invoke(
            "procedures.publish",
            TOKEN,
            json!({"procedure_id": id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    id
}

async fn publish_kata(running: &support::Running) {
    let sop = publish_runbook(running, "ops.test.kata").await;
    let (status, body) = running
        .invoke(
            "processes.create",
            TOKEN,
            json!({
                "key": KATA_KEY,
                "title": "Kata",
                "steps": [{"action": "cards.get", "procedure_id": sop,
                           "input_template": {"id": "{{card.id}}"}}],
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let id = text(output(&body), "id");
    let (status, body) = running
        .invoke(
            "processes.publish",
            TOKEN,
            json!({"process_id": id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
}

// ── the stub Kubernetes API ────────────────────────────────────────────────────────────────

fn initial_deployment() -> Value {
    json!({
        "kind": "Deployment",
        "apiVersion": "apps/v1",
        "metadata": {
            "name": "conveyor",
            "namespace": "conveyor",
            "annotations": {"deployment.kubernetes.io/revision": "2"}
        },
        "spec": {
            "replicas": 3,
            "template": {
                "metadata": {"annotations": {"deployment.kubernetes.io/revision": "2"}},
                "spec": {"containers": [{"name": "serve", "image": "conveyor:test",
                                         "env": [{"name": "FEATURE_X", "value": "off"}]}]}
            }
        }
    })
}

/// A JSON merge-patch, exactly the subset the ops verbs send: objects merge, everything else
/// replaces.
fn merge(target: &mut Value, patch: &Value) {
    match (target, patch) {
        (Value::Object(target), Value::Object(patch)) => {
            for (key, value) in patch {
                match target.get_mut(key) {
                    Some(existing) => merge(existing, value),
                    None => {
                        target.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (target, patch) => *target = patch.clone(),
    }
}

async fn stub_api() -> (String, Arc<AtomicUsize>) {
    let patches = Arc::new(AtomicUsize::new(0));
    let deployment = Arc::new(Mutex::new(initial_deployment()));
    let app = axum::Router::new()
        .route(
            DEPLOYMENT_PATH,
            get({
                let deployment = deployment.clone();
                move || {
                    let deployment = deployment.clone();
                    async move { axum::Json(deployment.lock().expect("stub lock").clone()) }
                }
            })
            .patch({
                let deployment = deployment.clone();
                let patches = patches.clone();
                move |body: axum::Json<Value>| {
                    let deployment = deployment.clone();
                    let patches = patches.clone();
                    async move {
                        patches.fetch_add(1, Ordering::SeqCst);
                        let mut current = deployment.lock().expect("stub lock");
                        let patch = body.0;
                        merge(&mut current, &patch);
                        axum::Json(current.clone())
                    }
                }
            }),
        )
        .route(
            "/apis/apps/v1/namespaces/conveyor/replicasets",
            get(move || async move {
                axum::Json(json!({
                    "kind": "ReplicaSetList",
                    "items": [{
                        "kind": "ReplicaSet",
                        "metadata": {
                            "name": "conveyor-1",
                            "annotations": {"deployment.kubernetes.io/revision": "1"},
                            "ownerReferences": [{"kind": "Deployment", "name": "conveyor"}]
                        },
                        "spec": {"template": {
                            "metadata": {"annotations": {"deployment.kubernetes.io/revision": "1"}},
                            "spec": {"containers": [{"name": "serve", "image": "conveyor:old"}]}
                        }}
                    }]
                }))
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind the API stub");
    let base = format!("http://{}", listener.local_addr().expect("addr"));
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (base, patches)
}

fn scale_input() -> Value {
    json!({"namespace": "conveyor", "deployment": "conveyor", "replicas": 5, "dry_run": false})
}

// ── P1.1 — an incident is triaged with no cluster write ───────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_incident_is_triaged_with_no_write() {
    let _env = env_lock().await;
    let (pg, running, seeded, board, lane) = fixture().await;
    let (base, patches) = stub_api().await;
    support::set_env("OPS_API_URL", &base);
    support::set_env("OPS_TOKEN", "test-token");
    let runbook = publish_runbook(&running, "ops.test.crash_loop").await;
    let incident = open_incident(&running, &board, &lane, "sev3").await;

    let (status, body) = triage(&running, &incident, &runbook).await;
    assert_eq!(status, 200, "{body}");
    let incident_out = output(&body);
    assert_eq!(incident_out["state"], "triaged");
    assert_eq!(incident_out["severity"], "sev2");
    assert_eq!(incident_out["runbook_procedure_id"], runbook);
    assert!(incident_out["triaged_at"].is_string());
    assert!(incident_out["mitigated_at"].is_null());
    assert_eq!(
        patches.load(Ordering::SeqCst),
        0,
        "a triage makes no cluster request"
    );
    let _ = (&pg, &seeded);
}

// ── P1.2 — an approved mitigation sets mitigated_at with the read-back ─────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_approved_mitigation_sets_mitigated_at_with_a_read_back() {
    let _env = env_lock().await;
    let (pg, running, seeded, board, lane) = fixture().await;
    let (base, patches) = stub_api().await;
    support::set_env("OPS_API_URL", &base);
    support::set_env("OPS_TOKEN", "test-token");
    let runbook = publish_runbook(&running, "ops.test.crash_loop").await;
    let incident = open_incident(&running, &board, &lane, "sev2").await;
    let (status, body) = triage(&running, &incident, &runbook).await;
    assert_eq!(status, 200, "{body}");

    // A REAL approval: requested by alice, decided by the reviewer — so the record can name the
    // approver from the immutable decision row (a status flipped in SQL has no decider).
    let input = json!({"incident_id": incident, "action": "ops.scale", "input": scale_input()});
    let (status, body) = running
        .invoke(
            "approvals.request",
            TOKEN,
            json!({"gate": "ops.mitigate", "kind": "action", "action": "ops.mitigate",
                   "input": input.clone()}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let gate = text(output(&body), "id");
    let (status, body) = running
        .invoke(
            "approvals.decide",
            REVIEWER,
            json!({"approval_id": gate, "decision": "approve"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let (status, body) = running
        .invoke_approved(
            "ops.mitigate",
            TOKEN,
            input,
            None,
            Uuid::parse_str(&gate).expect("gate id"),
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let incident_out = output(&body);
    assert_eq!(incident_out["state"], "mitigated");
    assert!(incident_out["mitigated_at"].is_string());
    assert_eq!(
        patches.load(Ordering::SeqCst),
        1,
        "exactly one cluster write"
    );

    let mut tx = store::begin_scoped(&pg.owner, seeded.tenant)
        .await
        .expect("scoped tx");
    let card = store::card::get_card_record_tx(
        &mut tx,
        seeded.tenant,
        Uuid::parse_str(&incident).unwrap(),
    )
    .await
    .expect("read the incident")
    .expect("the incident exists");
    let mitigation = &card.meta["mitigation"];
    assert_eq!(mitigation["action"], "ops.scale");
    assert_eq!(mitigation["action_version"], 1);
    assert!(
        mitigation["approval_id"].is_string(),
        "the record names the approval: {mitigation}"
    );
    assert!(
        mitigation["approvers"]
            .as_array()
            .is_some_and(|a| !a.is_empty()),
        "the record names the approver: {mitigation}"
    );
    assert_eq!(
        mitigation["read_back"]["checks"][0]["name"],
        "spec_replicas"
    );
    assert!(
        mitigation["read_back"]["checks"][0]["ok"]
            .as_bool()
            .unwrap_or(false),
        "the read-back check passed: {mitigation}"
    );
}

// ── P1.3 — one linked postmortem, a kata candidate only when systemic ───────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_incident_resolves_with_a_linked_postmortem() {
    let _env = env_lock().await;
    let (pg, running, seeded, board, lane) = fixture().await;
    let (base, _patches) = stub_api().await;
    support::set_env("OPS_API_URL", &base);
    support::set_env("OPS_TOKEN", "test-token");
    let runbook = publish_runbook(&running, "ops.test.crash_loop").await;
    let incident = open_incident(&running, &board, &lane, "sev2").await;
    let (status, body) = triage(&running, &incident, &runbook).await;
    assert_eq!(status, 200, "{body}");
    let input = json!({"incident_id": incident, "action": "ops.scale", "input": scale_input()});
    let (status, body) = running
        .invoke_gated(&pg, seeded.tenant, "ops.mitigate", TOKEN, input, None)
        .await;
    assert_eq!(status, 200, "{body}");

    // A non-systemic finding writes the postmortem and opens no kata.
    let (status, body) = running
        .invoke(
            "ops.postmortem",
            TOKEN,
            json!({
                "incident_id": incident,
                "evidence": {"pod": "conveyor-abc", "log_excerpt": "OOMKilled"},
                "postmortem": {"title": "OOM after the roll", "body": "Blameless body.",
                               "systemic": false},
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let incident_out = output(&body);
    assert_eq!(incident_out["state"], "mitigated");
    let asset = incident_out["postmortem_asset_id"]
        .as_str()
        .expect("asset")
        .to_owned();
    assert!(incident_out["kata_candidate_id"].is_null());

    // The resolution follows its postmortem.
    let (status, body) = running
        .invoke(
            "ops.resolve",
            TOKEN,
            json!({"incident_id": incident}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["state"], "resolved");
    assert!(output(&body)["resolved_at"].is_string());

    let mut tx = store::begin_scoped(&pg.owner, seeded.tenant)
        .await
        .expect("scoped tx");
    let found =
        store::governance::get_asset_tx(&mut tx, seeded.tenant, Uuid::parse_str(&asset).unwrap())
            .await
            .expect("read the postmortem");
    assert!(found.is_some(), "the postmortem governed asset exists");
    assert!(
        store::card::get_card_record_tx(
            &mut tx,
            seeded.tenant,
            Uuid::parse_str(&incident).unwrap()
        )
        .await
        .expect("read the incident")
        .expect("the incident")
        .meta["resolved_at"]
            .is_string(),
        "the incident carries resolved_at"
    );

    // A second postmortem and a second resolution are both refused.
    let (status, body) = running
        .invoke(
            "ops.postmortem",
            TOKEN,
            json!({
                "incident_id": incident,
                "evidence": {},
                "postmortem": {"title": "Again", "body": "No."},
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    let (status, body) = running
        .invoke(
            "ops.resolve",
            TOKEN,
            json!({"incident_id": incident}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_systemic_finding_opens_exactly_one_kata() {
    let _env = env_lock().await;
    let (pg, running, seeded, board, lane) = fixture().await;
    let (base, _patches) = stub_api().await;
    support::set_env("OPS_API_URL", &base);
    support::set_env("OPS_TOKEN", "test-token");
    publish_kata(&running).await;
    let runbook = publish_runbook(&running, "ops.test.crash_loop").await;
    let incident = open_incident(&running, &board, &lane, "sev1").await;
    let (status, body) = triage(&running, &incident, &runbook).await;
    assert_eq!(status, 200, "{body}");
    let input = json!({"incident_id": incident, "action": "ops.restart",
                       "input": {"namespace": "conveyor", "deployment": "conveyor"}});
    let (status, body) = running
        .invoke_gated(&pg, seeded.tenant, "ops.mitigate", TOKEN, input, None)
        .await;
    assert_eq!(status, 200, "{body}");
    let (status, body) = running
        .invoke(
            "ops.postmortem",
            TOKEN,
            json!({
                "incident_id": incident,
                "evidence": {"finding": "the image roll removed the readiness probe"},
                "postmortem": {"title": "Systemic", "body": "The roll pattern is unsafe.",
                               "systemic": true},
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert!(
        output(&body)["kata_candidate_id"].is_string(),
        "a systemic finding opens exactly one kata: {body}"
    );
    let (status, body) = running
        .invoke(
            "ops.resolve",
            TOKEN,
            json!({"incident_id": incident}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
}

// ── P2.2 — the tenant's quorum decides ───────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_tenant_policy_sets_the_quorum() {
    let _env = env_lock().await;
    let (pg, running, seeded, board, lane) = fixture().await;
    let (base, patches) = stub_api().await;
    support::set_env("OPS_API_URL", &base);
    support::set_env("OPS_TOKEN", "test-token");
    let runbook = publish_runbook(&running, "ops.test.crash_loop").await;
    let incident = open_incident(&running, &board, &lane, "sev2").await;
    let (status, body) = triage(&running, &incident, &runbook).await;
    assert_eq!(status, 200, "{body}");
    let input = json!({"incident_id": incident, "action": "ops.scale", "input": scale_input()});

    // A two-decision gate with one decision stays pending, so the mitigation is refused.
    let (status, body) = running
        .invoke(
            "approvals.request",
            TOKEN,
            json!({
                "gate": "ops.mitigate",
                "kind": "action",
                "action": "ops.mitigate",
                "input": input,
                "required_decisions": 2,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let gate = text(output(&body), "id");
    let (status, body) = running
        .invoke(
            "approvals.decide",
            REVIEWER,
            json!({"approval_id": gate, "decision": "approve"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let (status, body) = running
        .invoke_approved(
            "ops.mitigate",
            TOKEN,
            input.clone(),
            None,
            Uuid::parse_str(&gate).expect("gate id"),
        )
        .await;
    assert_eq!(status, 403, "one of two decisions must not admit: {body}");
    assert_eq!(
        patches.load(Ordering::SeqCst),
        0,
        "a refused gate reaches no cluster"
    );

    // A one-decision gate with its one decision proceeds.
    let (status, body) = running
        .invoke_gated(&pg, seeded.tenant, "ops.mitigate", TOKEN, input, None)
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(patches.load(Ordering::SeqCst), 1);
}

// ── P3.1 — the four write verbs run and read back ─────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_four_write_verbs_are_callable_and_read_back() {
    let _env = env_lock().await;
    let (pg, running, seeded, _board, _lane) = fixture().await;
    let (base, patches) = stub_api().await;
    support::set_env("OPS_API_URL", &base);
    support::set_env("OPS_TOKEN", "test-token");

    let cases = [
        (
            "ops.restart",
            json!({"namespace": "conveyor", "deployment": "conveyor", "dry_run": false}),
            "restarted_at_annotation",
        ),
        ("ops.scale", scale_input(), "spec_replicas"),
        (
            "ops.rollback",
            json!({"namespace": "conveyor", "deployment": "conveyor",
                   "revision": 1, "dry_run": false}),
            "pod_template_restored",
        ),
        (
            "ops.flag_flip",
            json!({"namespace": "conveyor", "deployment": "conveyor", "container": "serve",
                   "flag": "FEATURE_X", "value": "on", "dry_run": false}),
            "env_value",
        ),
    ];
    for (action, input, check) in cases {
        let (status, body) = running
            .invoke_gated(&pg, seeded.tenant, action, TOKEN, input, None)
            .await;
        assert_eq!(status, 200, "{action}: {body}");
        let checks = output(&body)["checks"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        assert!(
            checks.iter().any(|c| c["name"] == check && c["ok"] == true),
            "{action}: the read-back {check} did not pass: {body}"
        );
    }
    assert_eq!(
        patches.load(Ordering::SeqCst),
        4,
        "each verb made exactly one cluster write"
    );
}

// ── P3.2 — a verb outside the four is refused ─────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_undeclared_verb_is_refused() {
    let _env = env_lock().await;
    let (_pg, running, _seeded, board, lane) = fixture().await;
    let (base, patches) = stub_api().await;
    support::set_env("OPS_API_URL", &base);
    support::set_env("OPS_TOKEN", "test-token");
    let runbook = publish_runbook(&running, "ops.test.crash_loop").await;
    let incident = open_incident(&running, &board, &lane, "sev2").await;
    let (status, body) = triage(&running, &incident, &runbook).await;
    assert_eq!(status, 200, "{body}");
    let input = json!({
        "incident_id": incident,
        "action": "ops.secret_read",
        "input": {"namespace": "conveyor"},
    });
    // The gate's own target cannot be composed for an undeclared verb, so no approval can even
    // be requested for it — the closed set is enforced before the handler is reachable.
    let (status, body) = running
        .invoke(
            "approvals.request",
            TOKEN,
            json!({"gate": "ops.mitigate", "kind": "action", "action": "ops.mitigate",
                   "input": input}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert!(body.to_string().contains("not one of"), "{body}");
    // The same refusal happens before any approval check: the target cannot be composed, so the
    // call never reaches the handler and never touches the cluster.
    let (status, body) = running
        .invoke("ops.mitigate", TOKEN, input, None, None, None)
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(patches.load(Ordering::SeqCst), 0);
}
