//! #744 — the proof fixture: a stub prover, a sealed `lean4.token`, and a certificate.
//!
//! A certificate takes two steps, and the tests drive both of them for real:
//!
//! 1. the declared `lean4.check` external action, which the engine sends to this stub;
//! 2. `proofs.record`, which names that invocation and reads the response the engine persisted.
//!
//! The stub answers from a per-source plan, so one binary's tests can run concurrently without
//! fighting over a global: a source is `valid` unless the test has asked for it to be refuted
//! (which is how a "the prover changed its mind" re-verification is staged).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use conveyor_actions::{InvokeRequest, invoke, registry};
use conveyor_auth::Role;
use conveyor_domain::TenantId;
use serde_json::{Value, json};
use uuid::Uuid;

use super::{Pg, Seeded, set_env};

/// The theorem every fixture certificate closes.
pub const THEOREM: &str = "delivery_ok";

/// A spec digest shaped like the column's CHECK wants it.
pub fn spec_sha256() -> String {
    "7c3a5b19".repeat(8)
}

/// Render a facts document the way the engine does. The source is never hand-written: a caller
/// cannot supply one that disagrees with the facts, so neither may a fixture.
pub fn source(facts: &Value) -> String {
    conveyor_actions::proof::render::render(facts, THEOREM).expect("the fixture facts render")
}

/// One stub prover per test binary, with the process-global base URL set exactly once.
pub async fn prover() -> &'static Prover {
    static PROVER: tokio::sync::OnceCell<Prover> = tokio::sync::OnceCell::const_new();
    PROVER.get_or_init(Prover::start).await
}

/// The stub's state and the calls the fixture makes against it.
pub struct Prover {
    plan: Arc<Mutex<HashMap<String, bool>>>,
}

impl Prover {
    async fn start() -> Prover {
        let plan: Arc<Mutex<HashMap<String, bool>>> = Arc::new(Mutex::new(HashMap::new()));
        let handler = {
            let plan = plan.clone();
            move |axum::Json(body): axum::Json<Value>| {
                let plan = plan.clone();
                async move {
                    let source = body
                        .get("source")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned();
                    let valid = plan
                        .lock()
                        .expect("the stub plan")
                        .get(&source)
                        .copied()
                        .unwrap_or(true);
                    axum::Json(json!({
                        "valid": valid,
                        "diagnostics": [],
                        "computation_time_ms": 7.5,
                    }))
                }
            }
        };
        let app = axum::Router::new().route("/api/check", axum::routing::post(handler));
        // The stub must outlive any one test: `#[tokio::test]` builds a runtime per test and
        // tears it down when that test returns, so a `tokio::spawn` here would die with the
        // first test and every later test would see an unreachable prover. Its own thread with
        // its own runtime lives until the process exits, which is exactly one test binary.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind the prover stub");
        listener
            .set_nonblocking(true)
            .expect("the stub listener is non-blocking");
        let base = format!("http://{}", listener.local_addr().expect("addr"));
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
                .expect("the stub runtime");
            runtime.block_on(async move {
                let listener =
                    tokio::net::TcpListener::from_std(listener).expect("the stub listener");
                let _ = axum::serve(listener, app).await;
            });
        });
        // One base URL for the whole binary: every test talks to this stub.
        set_env("CONVEYOR_EXTERNAL_LEAN4_URL", &base);
        Prover { plan }
    }

    /// Answer `valid: false` for this source from now on.
    pub fn refute(&self, source: &str) {
        self.plan
            .lock()
            .expect("the stub plan")
            .insert(source.to_owned(), false);
    }

    /// Answer `valid: true` for this source again.
    pub fn accept(&self, source: &str) {
        self.plan
            .lock()
            .expect("the stub plan")
            .insert(source.to_owned(), true);
    }
}

/// Seal the tenant's prover credential, as `conveyor admin upstream-token` does.
pub async fn seal_lean4(pg: &Pg, tenant: TenantId) {
    seal_lean4_token(pg, tenant, b"test-lean4-token").await;
}

/// Seal a specific prover credential — the live corpus test seals a real licence token.
pub async fn seal_lean4_token(pg: &Pg, tenant: TenantId, token: &[u8]) {
    let mut tx = conveyor_store::begin_scoped(&pg.runtime, tenant)
        .await
        .expect("scoped tx");
    conveyor_store::secret::create_secret_tx(&mut tx, tenant, "lean4.token", token, "tenant")
        .await
        .expect("seal the lean4 token");
    tx.commit().await.expect("commit the lean4 token");
}

/// A well-formed engine request, with the fields a certificate path cares about.
pub fn request(action: &str, user_id: Uuid, input: Value, key: Option<&str>) -> InvokeRequest {
    InvokeRequest {
        action: action.to_owned(),
        input,
        precondition: None,
        idempotency_key: key.map(str::to_owned),
        approval_id: None,
        actor_id: user_id,
        actor_kind: "user".to_owned(),
        role: Role::Admin,
        policy: Default::default(),
        authority: conveyor_actions::Authority::Role,
        request_id: Some("rid-proofs".to_owned()),
        trace_id: Some("trace-proofs".to_owned()),
        depth: 0,
        app_key: "conveyor-test".to_owned(),
        usage: None,
    }
}

/// Run the real `lean4.check` external action and return the key and the body it sent.
///
/// The body is returned rather than rebuilt: `proofs.record` re-computes the engine's request
/// hash from it, so a fixture that retyped it could accidentally pass a test a caller would fail.
pub async fn check(pg: &Pg, seeded: &Seeded, source: &str) -> (String, Value) {
    let key = format!("lean4-{}", Uuid::now_v7());
    let input = json!({ "source": source, "timeout": 60 });
    let outcome = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request("lean4.check", seeded.user_id, input.clone(), Some(&key)),
    )
    .await
    .expect("the prover call");
    assert_eq!(outcome.status, 200, "the prover call: {}", outcome.body);
    (key, input)
}

/// A `proofs.record` body for the invocation a check produced.
pub fn record_input(
    key: &str,
    invocation_input: &Value,
    subject_id: Uuid,
    facts: &Value,
    theorem: &str,
    trust: &str,
) -> Value {
    json!({
        "action": "lean4.check",
        "idempotency_key": key,
        "invocation_input": invocation_input,
        "subject_type": "card",
        "subject_id": subject_id,
        "spec_name": "SPEC_V2",
        "spec_sha256": spec_sha256(),
        "facts_canonical": facts.to_string(),
        "theorem": theorem,
        "trust": trust,
    })
}

/// The facts a test certificate is about: a slice that is closed and carries its evidence.
///
/// `marker` makes each test's facts distinct, so a facts digest is never accidentally shared.
pub fn facts(marker: &str) -> Value {
    json!({
        "kind": "slice",
        "spec_sha256": spec_sha256(),
        "model_sha256": conveyor_actions::proof::render::model_sha256(),
        "state": "closed",
        "required": ["gate", marker],
        "present": ["gate", marker],
    })
}

/// Record a certificate end to end and return its id and `certificate_sha256`.
pub async fn record(pg: &Pg, seeded: &Seeded, marker: &str) -> (String, String) {
    let facts = facts(marker);
    let source = source(&facts);
    let (key, input) = check(pg, seeded, &source).await;
    let subject = Uuid::now_v7();
    let outcome = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request(
            "proofs.record",
            seeded.user_id,
            record_input(&key, &input, subject, &facts, THEOREM, "kernel"),
            None,
        ),
    )
    .await
    .expect("proofs.record");
    assert_eq!(outcome.status, 201, "{}", outcome.body);
    let body = &outcome.body["output"];
    (
        body["id"].as_str().expect("the certificate id").to_owned(),
        body["certificate_sha256"]
            .as_str()
            .expect("the certificate digest")
            .to_owned(),
    )
}

// ── the engine's own unit (#837) ─────────────────────────────────────────────────────────

/// The theorem `proofs.attest` returns.
pub const UNIT_THEOREM: &str = "unit_record_ok";
/// The specification a unit fixture pins.
pub const UNIT_SPEC: &str = "docs/SPEC_V2.md";

/// A chartered development project and the facts its unit certificate derives.
pub struct Unit {
    pub development_project_id: String,
    pub issue_ref: String,
    pub lane: String,
    pub head_sha: String,
    pub digest: String,
}

async fn invoke_ok(pg: &Pg, seeded: &Seeded, actor: Uuid, action: &str, input: Value) -> Value {
    let outcome = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request(action, actor, input, None),
    )
    .await
    .unwrap_or_else(|error| panic!("{action} refused: {} — {}", error.code, error.detail));
    outcome.body["output"].clone()
}

/// Charter a unit with a pinned proof policy. The implementer is the seeded admin; the gates
/// are decided by the distinct reviewer `seed` provisions.
pub async fn charter_unit(pg: &Pg, seeded: &Seeded, issue_number: u64, required: bool) -> Unit {
    let lane = format!("issue/837-{}", Uuid::now_v7().simple());
    let issue_ref = format!("nonagenticai/conveyor#{issue_number}");
    let project = invoke_ok(
        pg,
        seeded,
        seeded.user_id,
        "development_projects.charter",
        json!({
            "title": format!("proof unit {issue_number} {}", Uuid::now_v7().simple()),
            "repo": "nonagenticai/conveyor",
            "lane": lane.as_str(),
            "issue_ref": issue_ref.as_str(),
            "implementer_id": seeded.user_id,
            "acceptance_command": "cargo test -p conveyor-actions --test pm_gate_unit_proof",
            "proof_policy": {
                "spec_name": UNIT_SPEC,
                "spec_sha256": spec_sha256(),
                "required": required,
            },
        }),
    )
    .await;
    Unit {
        development_project_id: project["id"]
            .as_str()
            .expect("a development project id")
            .to_owned(),
        issue_ref,
        lane,
        head_sha: format!("{:040x}", Uuid::now_v7().as_u128()),
        digest: format!("sha256:{}", Uuid::now_v7().simple()),
    }
}

/// Record one development step as the implementer.
pub async fn record_step(pg: &Pg, seeded: &Seeded, unit: &Unit, step: &str, evidence: Value) {
    invoke_ok(
        pg,
        seeded,
        seeded.user_id,
        "development_projects.record_step",
        json!({
            "development_project_id": unit.development_project_id.as_str(),
            "step": step,
            "evidence": evidence,
        }),
    )
    .await;
}

/// Open a development gate and approve it as the distinct reviewer.
pub async fn approve_gate(pg: &Pg, seeded: &Seeded, unit: &Unit, action: &str) {
    let gate = invoke_ok(
        pg,
        seeded,
        seeded.user_id,
        action,
        json!({"development_project_id": unit.development_project_id.as_str()}),
    )
    .await;
    let approval_id = gate["approval_id"].as_str().expect("an approval id");
    let reviewer = super::reviewer_id(pg).await;
    invoke_ok(
        pg,
        seeded,
        reviewer,
        "approvals.decide",
        json!({"approval_id": approval_id, "decision": "approve"}),
    )
    .await;
}

/// Drive the unit from lane to live check: branch → PR → CI → review → merge → rollback plan →
/// deploy → live. The three gates are approved by a principal other than the implementer.
pub async fn drive_unit_to_live(pg: &Pg, seeded: &Seeded, unit: &Unit) {
    record_step(
        pg,
        seeded,
        unit,
        "branch",
        json!({"name": unit.lane.as_str(), "base": "main"}),
    )
    .await;
    record_step(
        pg,
        seeded,
        unit,
        "pull_request",
        json!({
            "number": 837,
            "url": "https://github.com/nonagenticai/conveyor/pull/837",
            "head_sha": unit.head_sha.as_str(),
            "paths": ["crates/actions/src/proof/facts.rs"],
        }),
    )
    .await;
    record_step(
        pg,
        seeded,
        unit,
        "ci",
        json!({"run_id": "run-837", "sha": unit.head_sha.as_str(), "verdict": "green"}),
    )
    .await;
    approve_gate(pg, seeded, unit, "development_projects.review_gate").await;
    approve_gate(pg, seeded, unit, "development_projects.merge_gate").await;
    record_step(
        pg,
        seeded,
        unit,
        "merge",
        json!({"merge_sha": format!("{:040x}", Uuid::now_v7().as_u128())}),
    )
    .await;
    record_step(
        pg,
        seeded,
        unit,
        "rollback_plan",
        json!({
            "prior_digest": "sha256:prior",
            "method": "redeploy the prior image digest",
            "tested": true,
            "evidence": ["the prior digest served /health in the rehearsal"],
            "backward_compatible": true,
        }),
    )
    .await;
    approve_gate(pg, seeded, unit, "development_projects.deploy_gate").await;
    record_step(
        pg,
        seeded,
        unit,
        "deploy",
        json!({"digest": unit.digest.as_str(), "migration_generation": 80}),
    )
    .await;
    record_step(
        pg,
        seeded,
        unit,
        "live_verify",
        json!({
            "health_status": 200,
            "serving_digest": unit.digest.as_str(),
            "acceptance_command": "curl -fsS \"$CONVEYOR_BASE_URL/health\"",
            "acceptance_exit": 0,
        }),
    )
    .await;
}

/// `proofs.attest`'s answer for the unit.
pub async fn attest(pg: &Pg, seeded: &Seeded, unit: &Unit) -> Value {
    invoke_ok(
        pg,
        seeded,
        seeded.user_id,
        "proofs.attest",
        json!({"development_project_id": unit.development_project_id.as_str()}),
    )
    .await
}

/// Attest, check and record the unit's certificate. Returns its id and certificate digest.
pub async fn record_unit(pg: &Pg, seeded: &Seeded, unit: &Unit) -> (String, String) {
    let attested = attest(pg, seeded, unit).await;
    let source = attested["lean_source"]
        .as_str()
        .expect("the attestation source");
    let (key, input) = check(pg, seeded, source).await;
    let outcome = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request(
            "proofs.record",
            seeded.user_id,
            json!({
                "action": "lean4.check",
                "idempotency_key": key,
                "invocation_input": input,
                "subject_type": "unit",
                "subject_id": unit.development_project_id,
                "spec_name": attested["spec_name"],
                "spec_sha256": attested["spec_sha256"],
                "facts_canonical": attested["facts_canonical"],
                "theorem": attested["theorem"],
                "trust": "kernel",
            }),
            None,
        ),
    )
    .await
    .expect("proofs.record");
    assert_eq!(outcome.status, 201, "{}", outcome.body);
    let body = &outcome.body["output"];
    (
        body["id"].as_str().expect("the certificate id").to_owned(),
        body["certificate_sha256"]
            .as_str()
            .expect("the certificate digest")
            .to_owned(),
    )
}
