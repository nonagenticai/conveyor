//! The live sandbox proof — S17 P1.1 and P1.2 — deployment-gated and `#[ignore]` by default.
//!
//! It runs the real lifecycle against the cluster this process is configured for, using the
//! driver's own [`Sandbox`] client:
//!
//! 1. **P1.1** start a Job and read back the object the API stored: a digest image,
//!    `automountServiceAccountToken: false`, `enableServiceLinks: false`, `backoffLimit: 0`,
//!    `activeDeadlineSeconds`, `ttlSecondsAfterFinished`, and the deployed node
//!    selector/toleration when `SANDBOX_NODE_SELECTOR`/`SANDBOX_TOLERATION` are set.
//! 2. **P1.2** wait for the terminal state, read the log and prove the runner's apply/eval
//!    markers are in it, then delete and prove the Job and its pod are gone.
//!
//! The driver-side half of P1.2 (the log is stored and the case recorded *before* the delete,
//! and a terminal Job is reaped each tick) is proved without a cluster in
//! `apps/server/tests/benchmark_jobs.rs`.
//!
//! Requires `SANDBOX_NAMESPACE` plus the API credential the driver uses (`SANDBOX_API_URL` /
//! `SANDBOX_TOKEN`, or the mounted service account) and `SANDBOX_LIVE_IMAGE`: any
//! swebench-verified50 instance image pinned by digest, with `/testbed` and `git`. Pull it once
//! first; the Job's own deadline bounds a cold pull.
//!
//! ```text
//! SANDBOX_NAMESPACE=swe-bench \
//! SANDBOX_API_URL=https://kubernetes.default.svc:443 \
//! SANDBOX_TOKEN="$(cat .../token)" SANDBOX_CA_PATH=.../ca.crt \
//! SANDBOX_NODE_SELECTOR=kubernetes.io/hostname=hp1 \
//! SANDBOX_TOLERATION=ci-runner=true:NoSchedule \
//! SANDBOX_LIVE_IMAGE=swebench/django__django-16493@sha256:<64 hex> \
//!   cargo test -p conveyor-server --test sandbox_live -- --ignored --nocapture
//! ```

use std::time::{Duration, Instant};

use conveyor_domain::benchmark::{APPLY_PATCH_PASS, EVAL_MARKER};
use conveyor_server::sandbox::{JobState, POD_DEADLINE_S, Sandbox, SandboxError};
use uuid::Uuid;

/// Printed by the eval script; its presence proves the patch landed and the eval ran.
const PROOF: &str = "conveyor sandbox live proof";

fn env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn wait_seconds() -> u64 {
    env("SANDBOX_LIVE_TIMEOUT_S")
        .and_then(|value| value.parse().ok())
        .unwrap_or(POD_DEADLINE_S + 120)
}

#[ignore = "needs a live cluster, SANDBOX_* config and SANDBOX_LIVE_IMAGE"]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_live_sandbox_lifecycle_creates_runs_reads_logs_and_deletes() {
    let sandbox = Sandbox::from_env().expect(
        "SANDBOX_NAMESPACE and the API credential (SANDBOX_API_URL/SANDBOX_TOKEN or the \
         mounted service account) must be set",
    );
    let image = env("SANDBOX_LIVE_IMAGE").expect(
        "SANDBOX_LIVE_IMAGE must name a swebench instance image pinned by @sha256:<64 hex>",
    );
    let digest = image
        .rsplit_once("@sha256:")
        .expect("SANDBOX_LIVE_IMAGE must be pinned by @sha256");
    assert_eq!(digest.1.len(), 64, "the digest must be 64 hex digits");
    assert!(
        digest.1.chars().all(|c| c.is_ascii_hexdigit()),
        "the digest must be hex"
    );

    let run = Uuid::now_v7();
    let instance = "conveyor-live-proof";
    // A new file is the patch every verified50 tree can apply; the eval prints it back, so the
    // log proves the whole apply → eval path, not just that a pod ran.
    let patch = format!(
        "diff --git a/conveyor-live-proof.txt b/conveyor-live-proof.txt\n\
         new file mode 100644\n\
         --- /dev/null\n\
         +++ b/conveyor-live-proof.txt\n\
         @@ -0,0 +1 @@\n\
         +{PROOF}\n"
    );
    let eval =
        format!("#!/bin/bash\ncat /testbed/conveyor-live-proof.txt\necho \"{EVAL_MARKER}\"\n");

    let started_at = Instant::now();
    let name = sandbox
        .start(run, instance, &image, &eval, &patch)
        .await
        .expect("the Job is created");

    // ── P1.1 — the object the API stored, not the manifest the test meant to send ──────────
    let job = sandbox
        .job(&name)
        .await
        .expect("the created Job is readable");
    let spec = &job["spec"];
    let pod = &spec["template"]["spec"];
    assert_eq!(spec["backoffLimit"], 0, "no retry of an untrusted run");
    assert!(
        spec["activeDeadlineSeconds"].as_u64().unwrap_or(0) > 0,
        "the Job carries a deadline: {spec}"
    );
    assert_eq!(spec["ttlSecondsAfterFinished"], 3600);
    assert_eq!(pod["automountServiceAccountToken"], false);
    assert_eq!(pod["enableServiceLinks"], false);
    assert_eq!(pod["restartPolicy"], "Never");
    let created_image = pod["containers"][0]["image"]
        .as_str()
        .expect("the container names an image");
    assert_eq!(
        created_image,
        image.as_str(),
        "the Job runs the digest image"
    );
    if let Some(selector) = env("SANDBOX_NODE_SELECTOR") {
        let (key, value) = selector.split_once('=').expect("key=value");
        assert_eq!(
            pod["nodeSelector"][key].as_str(),
            Some(value),
            "the Job is placed on the deployed grading node: {pod}"
        );
    }
    if let Some(raw) = env("SANDBOX_TOLERATION") {
        let (pair, effect) = raw.split_once(':').expect("key=value:Effect");
        let (key, value) = pair.split_once('=').expect("key=value:Effect");
        let toleration = &pod["tolerations"][0];
        assert_eq!(toleration["key"].as_str(), Some(key));
        assert_eq!(toleration["value"].as_str(), Some(value));
        assert_eq!(toleration["effect"].as_str(), Some(effect));
        assert_eq!(toleration["operator"], "Equal");
    }

    // ── P1.2 — the terminal state and the log ──────────────────────────────────────────────
    let bound = Duration::from_secs(wait_seconds());
    let state = loop {
        match sandbox
            .state(&name)
            .await
            .expect("the Job state is readable")
        {
            JobState::Running if started_at.elapsed() < bound => {
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
            JobState::Running => panic!("Job {name:?} did not finish within {bound:?}"),
            terminal => break terminal,
        }
    };
    assert_eq!(
        state,
        JobState::Succeeded,
        "the live Job finished {state:?}"
    );

    let log = sandbox.logs(&name).await.expect("the finished Job's log");
    assert!(
        log.contains(APPLY_PATCH_PASS),
        "the runner applied the patch; log was: {log}"
    );
    assert!(
        log.contains(PROOF),
        "the eval script ran against the patched tree; log was: {log}"
    );

    // ── P1.2 — the Job and its pod are deleted once the log is read ────────────────────────
    sandbox.delete(&name).await.expect("the Job is deleted");
    let reaped_at = Instant::now();
    loop {
        let job_gone = matches!(sandbox.job(&name).await, Err(SandboxError::NotFound { .. }));
        let pod_gone = matches!(
            sandbox.logs(&name).await,
            Err(SandboxError::NoPod(_)) | Err(SandboxError::NotFound { .. })
        );
        if job_gone && pod_gone {
            break;
        }
        assert!(
            reaped_at.elapsed() < Duration::from_secs(60),
            "the Job {name:?} and its pod were not deleted"
        );
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}
