//! The Job spec is a pure function of the instruction: no cluster is needed to prove the two
//! invariants that matter — a pod never acquires a cluster credential, and its caps are the
//! instruction's caps.

use conveyor_domain::runner::{CONTRACT, Input, Instruction, Limits, Output};
use conveyor_runner::{Dispatch, RunnerError, job_name, manifests};
use serde_json::json;

fn instruction() -> Instruction {
    Instruction {
        schema: CONTRACT.to_owned(),
        job_id: uuid::Uuid::now_v7(),
        tenant: uuid::Uuid::now_v7(),
        kind: "media.render_html".to_owned(),
        attempt: 1,
        correlation_id: uuid::Uuid::now_v7(),
        deadline_unix_ms: 1_763_000_000_000,
        params: json!({"html": "<h1>hi</h1>"}),
        inputs: vec![Input {
            name: "source".to_owned(),
            document_id: uuid::Uuid::now_v7(),
            sha256: "a".repeat(64),
            content_type: "text/html".to_owned(),
            bytes: 14,
        }],
        outputs: vec![Output {
            name: "page".to_owned(),
            content_type: "image/png".to_owned(),
            max_bytes: 8 * 1024 * 1024,
        }],
        limits: Limits {
            cpu_millis: 500,
            memory_bytes: 512 * 1024 * 1024,
            wall_secs: 90,
            max_output_bytes: 16 * 1024 * 1024,
        },
        conveyor_base_url: "http://conveyor.conveyor.svc.cluster.local:8080".to_owned(),
        trace_id: "trace-1".to_owned(),
    }
}

fn dispatch() -> Dispatch {
    Dispatch {
        namespace: "conveyor-runners".to_owned(),
        image: "ghcr.io/nonagenticai/media-browser@sha256:abc".to_owned(),
        pull_secret: "registry-pull-secret".to_owned(),
        node_selector: None,
        toleration: None,
    }
}

#[test]
fn job_has_no_service_account_token() {
    let manifests = manifests(&instruction(), &dispatch(), "token-1").expect("manifests");
    let pod = &manifests.job["spec"]["template"]["spec"];
    assert_eq!(pod["automountServiceAccountToken"], json!(false));
    assert!(
        pod.get("serviceAccountName").is_none(),
        "a runner pod must never name a service account: {pod}"
    );
    for volume in pod["volumes"].as_array().expect("volumes") {
        assert!(
            volume.get("projected").is_none(),
            "no projected service-account token volume"
        );
    }
}

#[test]
fn caps_come_from_the_instruction() {
    let instruction = instruction();
    let manifests = manifests(&instruction, &dispatch(), "token-1").expect("manifests");
    let container = &manifests.job["spec"]["template"]["spec"]["containers"][0];
    assert_eq!(container["resources"]["limits"]["cpu"], json!("500m"));
    // Kubernetes quantities are strings; the value is the instruction's byte count, unconverted.
    assert_eq!(
        container["resources"]["limits"]["memory"],
        json!((512 * 1024 * 1024).to_string())
    );
    // The wall clock plus the result-posting grace window.
    assert_eq!(
        manifests.job["spec"]["activeDeadlineSeconds"],
        json!(instruction.limits.wall_secs + 60)
    );
    // A capability with no instruction cannot silently get a bigger pod.
    assert_eq!(manifests.job["spec"]["backoffLimit"], json!(0));
}

#[test]
fn the_secret_carries_the_token_and_the_configmap_the_instruction() {
    let instruction = instruction();
    let manifests = manifests(&instruction, &dispatch(), "token-1").expect("manifests");
    assert_eq!(manifests.secret["stringData"]["token"], json!("token-1"));
    assert_eq!(
        manifests.secret["metadata"]["name"],
        manifests.job["metadata"]["name"]
    );
    let embedded: Instruction =
        serde_json::from_str(&manifests.instruction_json).expect("the instruction round-trips");
    assert_eq!(embedded, instruction);
    assert_eq!(
        manifests.config_map["data"]["instruction.json"],
        json!(manifests.instruction_json)
    );
}

#[test]
fn the_pod_has_no_store_credential() {
    let manifests = manifests(&instruction(), &dispatch(), "token-1").expect("manifests");
    let container = &manifests.job["spec"]["template"]["spec"]["containers"][0];
    let names: Vec<&str> = container["env"]
        .as_array()
        .expect("env")
        .iter()
        .map(|entry| entry["name"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(
        names,
        vec![
            "RUNNER_INSTRUCTION",
            "RUNNER_BASE_URL",
            "RUNNER_TOKEN",
            "HOME",
            "TMPDIR"
        ]
    );
    assert!(!names.iter().any(|name| name.starts_with("CONVEYOR_S3")));
    assert_eq!(
        container["securityContext"]["readOnlyRootFilesystem"],
        json!(true)
    );
}

#[test]
fn the_configmap_cap_is_enforced() {
    let mut instruction = instruction();
    instruction.params = json!({"blob": "x".repeat(1_100_000)});
    match manifests(&instruction, &dispatch(), "token-1") {
        Err(RunnerError::ConfigMapTooLarge { limit, .. }) => {
            assert_eq!(limit, conveyor_runner::CONFIGMAP_LIMIT);
        }
        other => panic!("expected ConfigMapTooLarge, got {other:?}"),
    }
}

#[test]
fn an_invalid_instruction_is_refused() {
    let mut instruction = instruction();
    instruction.kind = "render".to_owned();
    match manifests(&instruction, &dispatch(), "token-1") {
        Err(RunnerError::Manifest { detail }) => assert!(detail.contains("kind"), "{detail}"),
        other => panic!("expected Manifest, got {other:?}"),
    }
}

#[test]
fn an_empty_token_is_refused() {
    match manifests(&instruction(), &dispatch(), "  ") {
        Err(RunnerError::Manifest { detail }) => assert!(detail.contains("token"), "{detail}"),
        other => panic!("expected Manifest, got {other:?}"),
    }
}

#[test]
fn job_name_is_deterministic_and_bounded() {
    let job_id = uuid::Uuid::now_v7();
    let first = job_name(job_id, 1);
    let second = job_name(job_id, 1);
    assert_eq!(first, second);
    assert_ne!(first, job_name(job_id, 2));
    assert!(first.len() <= 63, "{first}");
    assert!(first.starts_with("run-"));
}
