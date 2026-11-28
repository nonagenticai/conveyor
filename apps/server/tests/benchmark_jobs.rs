//! S17 step 10 — the sandbox is fail-closed and its manifests carry the isolation contract.
//!
//! No cluster here: a live run needs a designated grading node (SPEC R12), so these tests are
//! the proof the fail-closed path and the pod shape are right before one exists. The driver's
//! tick is proved against a stub Kubernetes API at the bottom of the file.

use std::time::Duration;

use conveyor_domain::benchmark::{APPLY_PATCH_FAIL, APPLY_PATCH_PASS, EVAL_MARKER, TIMEOUT_MARKER};
use conveyor_server::sandbox::{
    CONFIGMAP_LIMIT, DEFAULT_PULL_SECRET, EVAL_TIMEOUT_S, GIT_APPLY_CMDS, GS_RELAY, GS_WRAPPER,
    GS_WRAPPER_DIR, NodeSelector, POD_DEADLINE_S, Sandbox, SandboxConfig, SandboxError, Toleration,
    runner_script,
};
use uuid::Uuid;

/// A real OCI digest: the sandbox refuses any image not pinned this way (P1.1).
const DIGEST: &str = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";

fn pinned_image(id: &str) -> String {
    format!("registry.example.com:5000/swebench/{id}@sha256:{DIGEST}")
}

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
fn a_missing_namespace_fails_closed_even_when_every_other_key_is_set() {
    // P1.3: the sandbox client cannot exist without its namespace, so the action layer's 503
    // (`benchmark_sandbox_not_configured`) fires before a Job, a ConfigMap or a case exists.
    let err = SandboxConfig::from_lookup(
        |key| match key {
            "SANDBOX_API_URL" => Some("https://kubernetes.default.svc:443".to_owned()),
            "SANDBOX_TOKEN" => Some("token".to_owned()),
            "SANDBOX_NODE_SELECTOR" => Some("kubernetes.io/hostname=compute-1".to_owned()),
            "SANDBOX_TOLERATION" => Some("ci-runner=true:NoSchedule".to_owned()),
            "SANDBOX_PULL_SECRET" => Some("registry-pull-secret".to_owned()),
            _ => None,
        },
        |_| Some(b"ca".to_vec()),
    )
    .unwrap_err();
    assert!(
        matches!(err, SandboxError::NotConfigured),
        "expected NotConfigured, got {err:?}"
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
    let (configmap, bin_configmap, job) = sandbox
        .manifests(
            run,
            "django__django-16493",
            &pinned_image("x"),
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
        pinned_image("x"),
        "the pod runs the digest-pinned instance image"
    );
    assert_eq!(
        pod["containers"][0]["securityContext"]["allowPrivilegeEscalation"],
        false
    );
    assert_eq!(job["spec"]["backoffLimit"], 0);
    assert_eq!(job["spec"]["activeDeadlineSeconds"], POD_DEADLINE_S);
    assert_eq!(pod["containers"][0]["resources"]["limits"]["cpu"], "2");
    assert_eq!(pod["volumes"][0]["configMap"]["defaultMode"], 0o555);

    // The `gs` relay: the image's Ghostscript 9.55 refuses every `/testbed` read and write
    // (the `--permit-file-*` flags are ineffective in that build and `-dNOSAFER` is ignored),
    // while `/tmp` is allowed. The shim runs the relay, which mirrors the paths it is handed
    // under `/tmp` and copies the results back.
    let bin_name = Sandbox::wrapper_configmap_name(&name);
    assert_eq!(bin_configmap["metadata"]["name"], bin_name.as_str());
    let wrapper = bin_configmap["data"]["gs"]
        .as_str()
        .expect("the gs wrapper is a string");
    assert_eq!(wrapper, GS_WRAPPER, "the ConfigMap ships the shim verbatim");
    assert!(
        wrapper.starts_with("#!/bin/sh\n")
            && wrapper.contains("exec \"$PY\" /bench-bin/gs-relay.py \"$@\""),
        "the shim must exec the relay and pass its arguments through: {wrapper:?}"
    );
    let relay = bin_configmap["data"]["gs-relay.py"]
        .as_str()
        .expect("the gs relay is a string");
    assert_eq!(relay, GS_RELAY, "the ConfigMap ships the relay verbatim");
    assert!(
        relay.contains("GS = \"/usr/bin/gs\"")
            && relay.contains("ROOT = \"/testbed\"")
            && relay.contains("tempfile.mkdtemp")
            && relay.contains("setpagedevice"),
        "the relay must stage /testbed under /tmp around the real Ghostscript: {relay:?}"
    );

    let volumes = pod["volumes"].as_array().expect("volumes");
    let bin_volume = volumes
        .iter()
        .find(|volume| volume["name"] == "bench-bin")
        .expect("the wrapper ConfigMap is a volume");
    assert_eq!(bin_volume["configMap"]["name"], bin_name.as_str());
    assert_eq!(
        bin_volume["configMap"]["defaultMode"], 0o755,
        "the wrapper must be executable inside the pod"
    );

    let mounts = pod["containers"][0]["volumeMounts"]
        .as_array()
        .expect("volume mounts");
    let bin_mount = mounts
        .iter()
        .find(|mount| mount["name"] == "bench-bin")
        .expect("the wrapper volume is mounted");
    assert_eq!(bin_mount["mountPath"], GS_WRAPPER_DIR);
    assert_eq!(bin_mount["readOnly"], true);

    // `$(PATH)` in a pod env value would not expand from the image (kubelet only expands
    // variables it defines), so the runner script applies the ordering at runtime instead.
    let run_script = configmap["data"]["run.sh"]
        .as_str()
        .expect("the runner script is a string");
    let path_line = format!("export PATH=\"{GS_WRAPPER_DIR}:$PATH\"");
    let path_at = run_script
        .find(&path_line)
        .unwrap_or_else(|| panic!("{path_line:?} missing from the runner script"));
    let first_command = run_script
        .find("cp /bench/patch.diff")
        .expect("the runner script copies the patch");
    assert!(
        path_at < first_command,
        "the wrapper directory must be first on PATH before the runner runs any command"
    );

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
            &pinned_image("x"),
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
fn an_unpinned_image_is_refused_before_create() {
    let sandbox = Sandbox::new(config()).unwrap();
    let err = sandbox
        .manifests(
            Uuid::now_v7(),
            "django__django-16493",
            "registry.example.com:5000/swebench/x:latest",
            "#!/bin/bash\necho eval\n",
            "",
        )
        .unwrap_err();
    assert!(
        matches!(err, SandboxError::ImageNotPinned(_)),
        "expected ImageNotPinned, got {err:?}"
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
use axum::extract::{Path, Query, State};
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
    events: Arc<Mutex<Vec<String>>>,
    log: Arc<Mutex<String>>,
}

impl Stub {
    fn record(&self, event: String) {
        self.events.lock().expect("events").push(event);
    }

    fn events(&self) -> Vec<String> {
        self.events.lock().expect("events").clone()
    }
}

async fn create_configmap(
    State(stub): State<Stub>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let name = body["metadata"]["name"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    stub.record(format!("configmap:create:{name}"));
    (StatusCode::CREATED, Json(body))
}

async fn create_job(
    State(stub): State<Stub>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let name = body["metadata"]["name"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    stub.jobs.lock().expect("jobs").push(name.clone());
    stub.record(format!("job:create:{name}"));
    (StatusCode::CREATED, Json(body))
}

async fn get_job(
    State(stub): State<Stub>,
    Path((_ns, name)): Path<(String, String)>,
) -> impl IntoResponse {
    // The stub finishes every Job immediately, so the second tick reads it Succeeded.
    stub.record(format!("job:get:{name}"));
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

async fn read_log(
    State(stub): State<Stub>,
    Path((_ns, pod)): Path<(String, String)>,
) -> impl IntoResponse {
    stub.record(format!("pod:log:{pod}"));
    stub.log.lock().expect("log").clone()
}

async fn delete_job(
    State(stub): State<Stub>,
    Path((_ns, name)): Path<(String, String)>,
) -> impl IntoResponse {
    stub.record(format!("job:delete:{name}"));
    StatusCode::OK
}

async fn delete_configmap(
    State(stub): State<Stub>,
    Path((_ns, name)): Path<(String, String)>,
) -> impl IntoResponse {
    stub.record(format!("configmap:delete:{name}"));
    StatusCode::OK
}

fn instance(id: &str) -> SeedInstance {
    SeedInstance {
        instance_id: id.to_owned(),
        repo: "django/django".to_owned(),
        base_commit: "0".repeat(40),
        image: pinned_image(id),
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
        .route("/api/v1/namespaces/{ns}/configmaps", post(create_configmap))
        .route(
            "/api/v1/namespaces/{ns}/configmaps/{name}",
            delete(delete_configmap),
        )
        .route("/apis/batch/v1/namespaces/{ns}/jobs", post(create_job))
        .route(
            "/apis/batch/v1/namespaces/{ns}/jobs/{name}",
            get(get_job).delete(delete_job),
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

    // P1.2: each finished Job's log was read and stored before the Job (and, by background
    // propagation, its pod) was deleted. A delete first would lose the evidence the verdict is
    // graded from.
    let events = stub.events();
    let started = stub.jobs.lock().expect("jobs").clone();
    assert_eq!(started.len(), 2);
    for job in &started {
        let read_at = events
            .iter()
            .position(|event| event == &format!("pod:log:{job}-pod"))
            .unwrap_or_else(|| panic!("the log of {job:?} was never read: {events:?}"));
        let deleted_at = events
            .iter()
            .position(|event| event == &format!("job:delete:{job}"))
            .unwrap_or_else(|| panic!("the finished Job {job:?} was never deleted: {events:?}"));
        assert!(
            read_at < deleted_at,
            "the log of {job:?} must be read before its pod is deleted"
        );
        let bin_created_at = events
            .iter()
            .position(|event| event == &format!("configmap:create:{job}-bin"))
            .unwrap_or_else(|| {
                panic!("the wrapper ConfigMap of {job:?} was never created: {events:?}")
            });
        let job_created_at = events
            .iter()
            .position(|event| event == &format!("job:create:{job}"))
            .expect("the Job create was recorded");
        assert!(
            bin_created_at < job_created_at,
            "the wrapper ConfigMap of {job:?} must exist before its Job starts"
        );
        assert!(
            events.contains(&format!("configmap:delete:{job}")),
            "the ConfigMap of {job:?} must be deleted too: {events:?}"
        );
        assert!(
            events.contains(&format!("configmap:delete:{job}-bin")),
            "the wrapper ConfigMap of {job:?} must be deleted too: {events:?}"
        );
    }
    assert_eq!(
        events
            .iter()
            .filter(|event| event.starts_with("job:delete:"))
            .count(),
        2,
        "one delete per finished Job, no more"
    );

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
