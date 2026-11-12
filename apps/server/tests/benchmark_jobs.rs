//! S17 step 10 — the sandbox is fail-closed and its manifests carry the isolation contract.
//!
//! No cluster here: a live run needs a designated grading node (SPEC R12), so these tests are
//! the proof the fail-closed path and the pod shape are right before one exists. The driver's
//! tick is proved against a stub Kubernetes API at the bottom of the file.

use std::time::Duration;

use conveyor_domain::benchmark::{APPLY_PATCH_FAIL, APPLY_PATCH_PASS, EVAL_MARKER, TIMEOUT_MARKER};
use conveyor_server::sandbox::{
    CONFIGMAP_LIMIT, DEFAULT_PULL_SECRET, EVAL_TIMEOUT_S, GIT_APPLY_CMDS, NodeSelector,
    POD_DEADLINE_S, Sandbox, SandboxConfig, SandboxError, Toleration, runner_script,
};
use uuid::Uuid;

fn config() -> SandboxConfig {
    SandboxConfig {
        namespace: "swe-bench".to_owned(),
        api_url: "https://kubernetes.default.svc:443".to_owned(),
        token: "test-token".to_owned(),
        ca_pem: None,
        node_selector: None,
        toleration: None,
        pull_secret: DEFAULT_PULL_SECRET.to_owned(),
        timeout: Duration::from_millis(200),
    }
}

#[test]
fn an_unset_namespace_is_not_configured() {
    let err = SandboxConfig::from_lookup(|_| None, |_| None).unwrap_err();
    assert!(
        matches!(err, SandboxError::NotConfigured),
        "expected NotConfigured, got {err:?}"
    );
}

#[test]
fn a_namespace_without_a_credential_fails_closed() {
    let err = SandboxConfig::from_lookup(
        |key| (key == "SANDBOX_NAMESPACE").then(|| "swe-bench".to_owned()),
        |_| None,
    )
    .unwrap_err();
    assert!(
        matches!(err, SandboxError::Credential(_)),
        "expected Credential, got {err:?}"
    );
}

#[test]
fn the_node_selector_and_toleration_are_deployment_config() {
    let cfg = SandboxConfig::from_lookup(
        |key| match key {
            "SANDBOX_NAMESPACE" => Some("swe-bench".to_owned()),
            "SANDBOX_API_URL" => Some("https://kubernetes.default.svc:443".to_owned()),
            "SANDBOX_TOKEN" => Some("token".to_owned()),
            "SANDBOX_NODE_SELECTOR" => Some("kubernetes.io/hostname=hp3".to_owned()),
            "SANDBOX_TOLERATION" => Some("ci-runner=true:NoSchedule".to_owned()),
            _ => None,
        },
        |_| None,
    )
    .unwrap();
    assert_eq!(
        cfg.node_selector,
        Some(NodeSelector {
            key: "kubernetes.io/hostname".to_owned(),
            value: "hp3".to_owned(),
        })
    );
    assert_eq!(
        cfg.toleration,
        Some(Toleration {
            key: "ci-runner".to_owned(),
            value: "true".to_owned(),
            effect: "NoSchedule".to_owned(),
        })
    );
}

#[test]
fn the_manifest_carries_the_isolation_contract() {
    let sandbox = Sandbox::new(config()).unwrap();
    let run = Uuid::now_v7();
    let (configmap, job) = sandbox
        .manifests(
            run,
            "django__django-16493",
            "registry.example.com:5000/swebench/x@sha256:abc",
            "#!/bin/bash\necho eval\n",
            "diff --git a/x b/x\n",
        )
        .unwrap();

    let name = Sandbox::job_name(run, "django__django-16493");
    assert_eq!(configmap["metadata"]["name"], name.as_str());
    assert_eq!(
        configmap["data"]["patch.diff"],
        "diff --git a/x b/x\n".to_string()
    );
    assert!(
        configmap["data"]["run.sh"]
            .as_str()
            .unwrap()
            .contains("cd /testbed")
    );

    let pod = &job["spec"]["template"]["spec"];
    assert_eq!(pod["automountServiceAccountToken"], false);
    assert_eq!(pod["enableServiceLinks"], false);
    assert_eq!(pod["imagePullSecrets"][0]["name"], DEFAULT_PULL_SECRET);
    assert_eq!(
        pod["containers"][0]["image"],
        "registry.example.com:5000/swebench/x@sha256:abc"
    );
    assert_eq!(
        pod["containers"][0]["securityContext"]["allowPrivilegeEscalation"],
        false
    );
    assert_eq!(job["spec"]["backoffLimit"], 0);
    assert_eq!(job["spec"]["activeDeadlineSeconds"], POD_DEADLINE_S);
    assert_eq!(pod["containers"][0]["resources"]["limits"]["cpu"], "2");
    assert_eq!(pod["volumes"][0]["configMap"]["defaultMode"], 0o555);
    assert!(
        pod.get("nodeSelector").is_none(),
        "a node selector must not be invented: the grading node is deployment config (SPEC R12)"
    );
}

#[test]
fn an_oversized_input_is_refused_before_create() {
    let sandbox = Sandbox::new(config()).unwrap();
    let err = sandbox
        .manifests(
            Uuid::now_v7(),
            "django__django-16493",
            "registry.example.com:5000/swebench/x@sha256:abc",
            &"x".repeat(CONFIGMAP_LIMIT),
            "",
        )
        .unwrap_err();
    assert!(
        matches!(err, SandboxError::ConfigMapTooLarge { .. }),
        "expected ConfigMapTooLarge, got {err:?}"
    );
}

#[test]
fn a_long_instance_id_keeps_the_job_name_under_63() {
    let run = Uuid::now_v7();
    let long = "a".repeat(120);
    let name = Sandbox::job_name(run, &long);
    assert!(name.len() <= 63, "{name:?} is {} chars", name.len());
    assert_eq!(
        name,
        Sandbox::job_name(run, &long),
        "the name is deterministic"
    );
    assert_ne!(name, Sandbox::job_name(Uuid::now_v7(), &long));
}

#[test]
fn the_runner_script_keeps_swebench_order_and_markers() {
    let script = runner_script();
    let mut previous = 0usize;
    for command in GIT_APPLY_CMDS {
        let needle = format!("\"{command}\"");
        let at = script
            .find(&needle)
            .unwrap_or_else(|| panic!("{needle:?} missing from the runner script"));
        assert!(at > previous, "{needle:?} is out of order");
        previous = at;
    }
    assert!(script.contains(APPLY_PATCH_FAIL));
    assert!(script.contains(APPLY_PATCH_PASS));
    assert!(script.contains(EVAL_MARKER));
    assert!(script.contains(TIMEOUT_MARKER));
    assert!(
        script.contains(&format!(
            "timeout {EVAL_TIMEOUT_S} /bin/bash /bench/eval.sh"
        )),
        "the eval timeout must be swebench's own default"
    );
    assert!(script.contains("git apply --check --reverse /tmp/patch.diff"));
}

// ── the driver tick against a stub Kubernetes API ───────────────────────────────────────────

mod common;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::Json;
use axum::Router;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use conveyor_domain::TenantId;
use conveyor_server::benchmark_worker;
use conveyor_store as store;
use conveyor_store::benchmark::{NewRun, SeedDocument, SeedInstance};
use serde_json::json;

#[derive(Clone, Default)]
struct Stub {
    jobs: Arc<Mutex<Vec<String>>>,
    log: Arc<Mutex<String>>,
}

async fn create_object(Json(body): Json<serde_json::Value>) -> impl IntoResponse {
    (StatusCode::CREATED, Json(body))
}

async fn create_job(
    State(stub): State<Stub>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    stub.jobs.lock().expect("jobs").push(
        body["metadata"]["name"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
    );
    (StatusCode::CREATED, Json(body))
}

async fn get_job() -> impl IntoResponse {
    // The stub finishes every Job immediately, so the second tick reads it Succeeded.
    Json(json!({"status": {"succeeded": 1}}))
}

async fn list_pods(Query(query): Query<HashMap<String, String>>) -> impl IntoResponse {
    let pod = query
        .get("labelSelector")
        .and_then(|selector| selector.strip_prefix("job-name="))
        .map(|name| format!("{name}-pod"))
        .unwrap_or_else(|| "pod".to_owned());
    Json(json!({"items": [{"metadata": {"name": pod}}]}))
}

async fn read_log(State(stub): State<Stub>) -> impl IntoResponse {
    stub.log.lock().expect("log").clone()
}

async fn deleted() -> impl IntoResponse {
    StatusCode::OK
}

fn instance(id: &str) -> SeedInstance {
    SeedInstance {
        instance_id: id.to_owned(),
        repo: "django/django".to_owned(),
        base_commit: "0".repeat(40),
        image: format!("registry.example.com:5000/swebench/{id}@sha256:abc"),
        problem_statement: "fix it".to_owned(),
        oracle_files: vec!["django/core.py".to_owned()],
        gold_patch: "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1 +1 @@\n-a\n+b\n".to_owned(),
        test_patch: "diff --git a/t b/t\n".to_owned(),
        fail_to_pass: vec!["tests/test_core.py::test_fix".to_owned()],
        pass_to_pass: vec!["tests/test_core.py::test_keep".to_owned()],
        eval_script: "#!/bin/bash\necho eval\n".to_owned(),
        parser: "parse_log_django".to_owned(),
    }
}

fn seed_doc() -> SeedDocument {
    SeedDocument {
        schema: 1,
        slug: "driver-v1".to_owned(),
        dataset: "SWE-bench/SWE-bench_Verified".to_owned(),
        dataset_revision: "rev".to_owned(),
        swebench_version: "5.0.2".to_owned(),
        parser_version: "swebench-5.0.2".to_owned(),
        image_lock_sha256: "lock".to_owned(),
        seed_sha256: "seed".to_owned(),
        instances: vec![instance("django__django-1"), instance("django__django-2")],
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_driver_starts_grades_and_finishes_a_control_run() {
    let stub = Stub::default();
    *stub.log.lock().expect("log") =
        ">>>>> Applied Patch\n>>>>> conveyor-bench eval\n\n>>>>> End Test Output\n".to_owned();
    let app = Router::new()
        .route("/api/v1/namespaces/{ns}/configmaps", post(create_object))
        .route("/api/v1/namespaces/{ns}/configmaps/{name}", delete(deleted))
        .route("/apis/batch/v1/namespaces/{ns}/jobs", post(create_job))
        .route(
            "/apis/batch/v1/namespaces/{ns}/jobs/{name}",
            get(get_job).delete(deleted),
        )
        .route("/api/v1/namespaces/{ns}/pods", get(list_pods))
        .route("/api/v1/namespaces/{ns}/pods/{pod}/log", get(read_log))
        .with_state(stub.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind the stub");
    let address = listener.local_addr().expect("stub address");
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    // The in-memory backend is the object store's own implementation (S09's test path); the
    // driver keeps every log as an S09 document, so the test needs a real sink.
    // SAFETY: `benchmark_jobs` is the only test binary that reads these vars, and the value is
    // set before any object store is built. `set_var` is unsafe in the 2024 edition.
    unsafe {
        std::env::set_var("CONVEYOR_S3_BACKEND", "memory");
    }
    let objects =
        conveyor_actions::integrations::objects::Objects::from_env().expect("memory object store");

    let db = common::TestDb::fresh().await;
    let tenant_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ('bench-driver', 'bench-driver') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("tenant");
    let tenant = TenantId::from_uuid(tenant_id).expect("tenant");

    let mut tx = store::begin_scoped(&db.runtime, tenant)
        .await
        .expect("scoped");
    let (suite, created) = store::benchmark::seed_suite_tx(&mut tx, tenant, &seed_doc())
        .await
        .expect("seed");
    assert!(created);
    let ids = vec!["django__django-1".to_owned(), "django__django-2".to_owned()];
    let run = store::benchmark::create_run_tx(
        &mut tx,
        tenant,
        &NewRun {
            suite_id: suite.id,
            arm: "gold",
            subject_kind: "suite",
            capability_agent_id: None,
            process_id: None,
            subject_version: None,
            label: "control-gold",
            options: &json!({"concurrency": 2}),
            instance_ids: &ids,
        },
    )
    .await
    .expect("run");
    tx.commit().await.expect("commit");

    let config = SandboxConfig {
        namespace: "swe-bench".to_owned(),
        api_url: format!("http://{address}"),
        token: "test".to_owned(),
        ca_pem: None,
        node_selector: None,
        toleration: None,
        pull_secret: DEFAULT_PULL_SECRET.to_owned(),
        timeout: Duration::from_secs(5),
    };
    let sandbox = Sandbox::new(config).expect("sandbox client");
    let payload = json!({ "run_id": run.id });

    benchmark_worker::poll_with(&db.runtime, tenant, &payload, &sandbox, &objects)
        .await
        .expect("tick one: start");
    assert_eq!(
        stub.jobs.lock().expect("jobs").len(),
        2,
        "one deterministic Job per case"
    );

    benchmark_worker::poll_with(&db.runtime, tenant, &payload, &sandbox, &objects)
        .await
        .expect("tick two: poll and grade");

    let mut tx = store::begin_scoped(&db.runtime, tenant)
        .await
        .expect("scoped");
    let finished = store::benchmark::get_run_tx(&mut tx, tenant, run.id)
        .await
        .expect("run");
    assert_eq!(finished.status, "succeeded");
    assert_eq!(
        (finished.n, finished.n_graded, finished.n_resolved),
        (2, 2, 0)
    );
    assert_eq!(
        finished.control_ok,
        Some(false),
        "a gold control that missed is recorded false, never hidden"
    );
    let cases = store::benchmark::list_cases_tx(&mut tx, tenant, run.id, None, None, 10)
        .await
        .expect("cases");
    assert_eq!(cases.len(), 2);
    assert!(
        cases
            .iter()
            .all(|case| case.verdict == "unresolved" && case.log_document_id.is_some()),
        "every case carries the grader's verdict and its stored log"
    );
    tx.rollback().await.ok();
}
