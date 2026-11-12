//! S19 Part B — `eval_runs.*` over the wire: the run is refused, started, resumed, read and
//! cancelled.
//!
//! The refusal is the load-bearing half. `eval_runs.start` is the only action that answers "may
//! this instrument measure anything yet", and it must answer with **every** missing gate in one
//! reply — a caller told one reason at a time edits, retries, and learns the next reason, which
//! is how a sign-off gets built against a moving digest.
//!
//! Nothing here drives a case; the driver's own proof is `apps/server/tests/eval_runner.rs`.

mod support;

use conveyor_domain::TenantId;
use conveyor_store::begin_scoped;
use serde_json::{Value, json};
use uuid::Uuid;

const TOKEN: &str = "dev:alice";

/// Request a gate and approve it as the owner (a human, which is what `evals:approve` admits).
async fn sign_off(running: &support::Running, eval_id: &str, gate: &str) -> String {
    let (status, body) = running
        .invoke(
            "eval_signoffs.request",
            TOKEN,
            json!({ "eval_id": eval_id, "gate": gate, "ask": "review the instrument" }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let approval_id = body["output"]["id"]
        .as_str()
        .expect("approval id")
        .to_owned();
    let (status, body) = running
        .invoke(
            "eval_signoffs.decide",
            TOKEN,
            json!({ "approval_id": approval_id, "decision": "approve" }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["status"], "approved");
    approval_id
}

/// An eval with a version and nothing else: no cases, no metric, no split, no signatures.
async fn empty_draft(running: &support::Running) -> (String, String) {
    let (status, body) = running
        .invoke(
            "evals.create",
            TOKEN,
            json!({
                "name": "run contracts",
                "purpose": "does start refuse, resume and cancel correctly",
                "subject_kind": "completions",
                "entry_action": "completions.create",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let eval_id = body["output"]["id"].as_str().expect("eval id").to_owned();
    let (status, body) = running
        .invoke(
            "eval_versions.create",
            TOKEN,
            json!({ "eval_id": eval_id }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let version_id = body["output"]["id"]
        .as_str()
        .expect("version id")
        .to_owned();
    (eval_id, version_id)
}

/// A **ready** instrument: `cases` cases with an answer key, one programmatic metric, both
/// sign-offs approved and the split locked.
///
/// The order is the contract, not a convenience: the version digest hashes the cases and the
/// metrics, so signing before they exist voids the signature the moment the next row lands.
async fn ready_instrument(running: &support::Running, cases: usize) -> (String, String) {
    let (eval_id, version_id) = empty_draft(running).await;
    for n in 0..cases {
        let (status, body) = running
            .invoke(
                "eval_cases.create",
                TOKEN,
                json!({
                    "version_id": version_id,
                    "input": format!("classify: case {n}"),
                    "tags": ["triage"],
                    "expected": { "label": "billing" },
                    "gold_provenance": "human_written",
                }),
                None,
                None,
                None,
            )
            .await;
        assert_eq!(status, 201, "{body}");
    }
    let (status, body) = running
        .invoke(
            "eval_metrics.create",
            TOKEN,
            json!({
                "version_id": version_id,
                "key": "label_accuracy",
                "role": "quality",
                "kind": "binary",
                "label": "label acc",
                "grader_kind": "programmatic",
                "grader": { "kind": "exact", "field": "label" },
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");

    sign_off(running, &eval_id, "eval_inputs").await;
    sign_off(running, &eval_id, "eval_grading").await;

    let (status, body) = running
        .invoke(
            "evals.draw_split",
            TOKEN,
            json!({ "eval_id": eval_id, "train_ratio": 0.5 }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    (eval_id, version_id)
}

/// The `eval.case.run` jobs a run owns, as `(dedupe_key, max_attempts)`.
async fn case_jobs(pg: &support::Pg, tenant: TenantId, run_id: &str) -> Vec<(String, i32)> {
    let prefix = format!("{run_id}:%");
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scoped tx");
    let rows: Vec<(String, i32)> = sqlx::query_as(
        "SELECT dedupe_key, max_attempts FROM job
          WHERE tenant_id = $1 AND kind = 'eval.case.run' AND dedupe_key LIKE $2
          ORDER BY dedupe_key",
    )
    .bind(tenant.as_uuid())
    .bind(prefix)
    .fetch_all(&mut *tx)
    .await
    .expect("read the run's jobs");
    tx.commit().await.expect("commit");
    rows
}

#[tokio::test]
async fn start_names_every_missing_gate_in_one_answer() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let (eval_id, _version) = empty_draft(&running).await;

    let (status, body) = running
        .invoke(
            "eval_runs.start",
            TOKEN,
            json!({ "eval_id": eval_id }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "eval_not_ready", "{body}");
    let detail = body["detail"].as_str().expect("a detail");
    for gate in ["eval_inputs", "eval_grading", "metrics", "split_missing"] {
        assert!(
            detail.contains(gate),
            "the refusal must name {gate}: {detail}"
        );
    }
    assert!(
        !detail.contains("split_incomplete"),
        "an eval with no cases has no unsplit case, and a reason that cannot be fixed is noise: {detail}"
    );

    // Nothing was written: a refused start leaves no run behind.
    let mut tx = begin_scoped(&pg.owner, seeded.tenant)
        .await
        .expect("scoped tx");
    let runs: i64 = sqlx::query_scalar("SELECT count(*) FROM eval_run WHERE tenant_id = $1")
        .bind(seeded.tenant.as_uuid())
        .fetch_one(&mut *tx)
        .await
        .expect("count runs");
    tx.commit().await.expect("commit");
    assert_eq!(runs, 0, "a refused start must not create a run");
}

#[tokio::test]
async fn start_enqueues_one_job_per_case_and_rep_and_a_second_start_resumes() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let (eval_id, version_id) = ready_instrument(&running, 3).await;

    let (status, body) = running
        .invoke(
            "eval_runs.start",
            TOKEN,
            json!({ "eval_id": eval_id, "version_id": version_id, "reps": 2 }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let run_id = body["output"]["run"]["id"]
        .as_str()
        .expect("run id")
        .to_owned();
    assert_eq!(body["output"]["queued_jobs"], 6, "3 cases × 2 reps: {body}");
    assert_eq!(body["output"]["skipped_slots"], 0, "{body}");
    assert_eq!(body["output"]["run"]["status"], "queued", "{body}");
    assert_eq!(body["output"]["run"]["case_count"], 3, "{body}");
    assert_eq!(body["output"]["run"]["reps"], 2, "{body}");

    let jobs = case_jobs(&pg, seeded.tenant, &run_id).await;
    assert_eq!(jobs.len(), 6, "{jobs:?}");
    let mut keys: Vec<&str> = jobs.iter().map(|(key, _)| key.as_str()).collect();
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(keys.len(), 6, "every slot owns exactly one job: {jobs:?}");
    assert!(
        jobs.iter().all(|(_, attempts)| *attempts == 5),
        "the run's jobs carry the eval's own retry policy: {jobs:?}"
    );

    // ── a second start is a RESUME, not a second run ──
    let (status, body) = running
        .invoke(
            "eval_runs.start",
            TOKEN,
            json!({ "eval_id": eval_id, "version_id": version_id, "reps": 2 }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(
        body["output"]["run"]["id"].as_str(),
        Some(run_id.as_str()),
        "`eval_run_variant_unique` means one baseline row per eval: {body}"
    );
    assert_eq!(
        body["output"]["queued_jobs"], 0,
        "`job`'s dedupe key makes a re-enqueue a no-op, so the second start queues nothing: {body}"
    );
    assert_eq!(
        body["output"]["skipped_slots"], 0,
        "`skipped_slots` counts slots that already hold a RESULT, not slots already queued — \
         nothing has run yet: {body}"
    );
    assert_eq!(
        case_jobs(&pg, seeded.tenant, &run_id).await.len(),
        6,
        "the second start must not double-queue a slot"
    );
}

#[tokio::test]
async fn a_run_is_readable_and_cancel_is_terminal() {
    let pg = support::Pg::fresh().await;
    let _seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let (eval_id, version_id) = ready_instrument(&running, 2).await;

    let (status, body) = running
        .invoke(
            "eval_runs.start",
            TOKEN,
            json!({ "eval_id": eval_id, "version_id": version_id }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let run_id = body["output"]["run"]["id"]
        .as_str()
        .expect("run id")
        .to_owned();

    let (status, body) = running
        .invoke(
            "eval_runs.get",
            TOKEN,
            json!({ "id": run_id }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["id"], run_id);

    let (status, body) = running
        .invoke(
            "eval_runs.list",
            TOKEN,
            json!({ "eval_id": eval_id }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let items = body["output"]["items"].as_array().expect("a page");
    assert_eq!(items.len(), 1, "{body}");
    assert_eq!(items[0]["id"], run_id);

    let (status, body) = running
        .invoke(
            "eval_runs.cancel",
            TOKEN,
            json!({ "id": run_id }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["status"], "cancelled");

    // Cancelled is terminal: a second cancel is a 409, not a silent no-op that reads as success.
    let (status, body) = running
        .invoke(
            "eval_runs.cancel",
            TOKEN,
            json!({ "id": run_id }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "conflict", "{body}");

    // A cancelled run cannot be resumed either — the same rule, from the other side.
    let (status, body) = running
        .invoke(
            "eval_runs.start",
            TOKEN,
            json!({ "eval_id": eval_id, "version_id": version_id }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "run_terminal", "{body}");

    let (status, body) = running
        .invoke(
            "eval_runs.list",
            TOKEN,
            json!({ "eval_id": eval_id, "status": "cancelled" }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let items = body["output"]["items"].as_array().expect("a page");
    assert_eq!(items.len(), 1, "{body}");
    let _: Value = body;
}

/// `eval_runs.start` is `evals:approve`: building is not signing, so the machine role is refused.
///
/// This is the same predicate the driver relies on — a KataExecutor that could start a run could
/// also start one whose entry action is a sign-off action, which is the self-sign trap.
#[tokio::test]
async fn start_is_refused_to_the_machine_role() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    // Two cases, not one: `evals.draw_split` refuses a version with fewer than two
    // (`422 split_too_small`), because a split of one case measures nothing.
    let (eval_id, version_id) = ready_instrument(&running, 2).await;

    let user_id: Uuid = sqlx::query_scalar(
        "INSERT INTO app_user (oidc_subject, email, display_name, kind)
         VALUES ('dev:robot', 'robot@dev.invalid', 'Robot', 'service') RETURNING id",
    )
    .fetch_one(&pg.owner)
    .await
    .expect("insert the machine user");
    let mut tx = begin_scoped(&pg.owner, seeded.tenant).await.expect("scope");
    sqlx::query(
        "INSERT INTO membership (tenant_id, user_id, role) VALUES ($1, $2, 'kata_executor')",
    )
    .bind(seeded.tenant.as_uuid())
    .bind(user_id)
    .execute(&mut *tx)
    .await
    .expect("membership");
    tx.commit().await.expect("commit");

    let (status, body) = running
        .invoke(
            "eval_runs.start",
            "dev:robot",
            json!({ "eval_id": eval_id, "version_id": version_id }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 403, "{body}");
    assert_eq!(body["type"], "action-forbidden", "{body}");
}
