//! P2.2 — a browser job runs under the in-cluster contract, is time-capped and has no egress.
//!
//! Two halves, because "time capped" and "no egress" are enforced at two different layers and a
//! test of only one would be a green light over the other:
//!
//! 1. **Dispatch** (`crates/runner`, the same code that creates the cluster objects) turns the
//!    instruction's limits into the Job and never grants the pod a credential.
//! 2. **The executor** (`apps/media`) refuses a caller-supplied URL, so the only document it ever
//!    loads is one the job already carries.

use conveyor_domain::runner::{Instruction, Limits};
use conveyor_media::browser::render::render;
use conveyor_media::common::Config;
use conveyor_runner::{Dispatch, manifests};
use serde_json::json;
use uuid::Uuid;

fn instruction(kind: &str, wall_secs: i64) -> Instruction {
    Instruction {
        schema: "runner/v1".to_string(),
        job_id: Uuid::from_u128(7),
        tenant: Uuid::from_u128(11),
        kind: kind.to_string(),
        attempt: 1,
        correlation_id: Uuid::from_u128(13),
        deadline_unix_ms: 1_900_000_000_000,
        params: json!({"html": "<p>synthetic</p>"}),
        inputs: Vec::new(),
        outputs: Vec::new(),
        limits: Limits {
            cpu_millis: 500,
            memory_bytes: 512 * 1024 * 1024,
            wall_secs,
            max_output_bytes: 4 * 1024 * 1024,
        },
        conveyor_base_url: "http://conveyor.conveyor.svc.cluster.local:8080".to_string(),
        trace_id: "trace-p22".to_string(),
    }
}

fn dispatch() -> Dispatch {
    Dispatch {
        namespace: "conveyor-jobs".to_string(),
        image: "registry.internal:5000/conveyor-media-browser@sha256:00".to_string(),
        pull_secret: conveyor_runner::DEFAULT_PULL_SECRET.to_string(),
        node_selector: None,
        toleration: None,
    }
}

/// The wall clock is the instruction's, and the pod cannot acquire a cluster credential.
#[test]
fn browser_job_is_time_capped_and_credential_free() {
    let instruction = instruction("media.render_html", 120);
    let manifests = manifests(&instruction, &dispatch(), "job-token").expect("the spec builds");

    let job = &manifests.job;
    assert_eq!(
        job["spec"]["activeDeadlineSeconds"], 180,
        "the pod clock is the instruction's wall clock plus the result grace window"
    );

    let pod = &job["spec"]["template"]["spec"];
    assert_eq!(pod["automountServiceAccountToken"], false);
    assert!(
        pod.get("serviceAccountName").is_none(),
        "a media pod must never run under a service account"
    );
    assert!(
        pod.get("hostNetwork").is_none(),
        "host networking would be egress with no policy in between"
    );

    // The only address the pod is given is conveyor's own API. Any second URL in the environment
    // would be a channel the job was not declared to have.
    let urls: Vec<String> = pod["containers"][0]["env"]
        .as_array()
        .expect("the executor container has env")
        .iter()
        .filter_map(|entry| entry["value"].as_str())
        .filter(|value| value.starts_with("http://") || value.starts_with("https://"))
        .map(str::to_string)
        .collect();
    assert_eq!(
        urls,
        vec![instruction.conveyor_base_url.clone()],
        "the pod may address conveyor and nothing else"
    );
}

/// A caller-supplied URL is refused by name: the executor loads only documents the job carries.
#[tokio::test]
async fn a_url_document_is_refused_so_a_render_needs_no_egress() {
    let dir = std::env::temp_dir().join(format!("conveyor-p22-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let config = Config {
        service: "conveyor-media-browser".to_string(),
        work_dir: dir.clone(),
        out_dir: dir,
    };
    let error = render(&json!({"url": "http://example.invalid/"}), &config)
        .await
        .expect_err("a URL must be refused");
    assert!(
        error.to_string().contains("url_not_supported"),
        "expected a named refusal, got {error}"
    );
}
