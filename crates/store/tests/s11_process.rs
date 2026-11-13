//! S11's store layer: the process version family and the run_step trace.
//!
//! The properties, each a way this could go wrong silently:
//!
//! 1. **One draft and one published per key** — enforced by partial unique indexes, not by a SELECT two
//!    transactions can both pass.
//! 2. **Publish retires the previous version** — the order (retire, then publish) is forced by the immediate
//!    unique index, and getting it wrong is a unique violation, not a silent two-published state.
//! 3. **A retry APPENDS** — `run_step` is unique on `(run_id, position, attempt)`; an overwrite would destroy
//!    the trace S13's diff gate reads.
//! 4. **Tenant isolation** — every table is RLS+FORCE; a second tenant sees none of it
//!
//! `cargo test -p conveyor-store --test s11_process -- --nocapture`.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_domain::process::StepInput;
use conveyor_store::process::{self, NewProcess, ProcessStoreError};
use serde_json::json;
use uuid::Uuid;

fn tenant(seed: &support::Seeded) -> TenantId {
    TenantId::from_uuid(seed.tenant).expect("non-nil")
}

fn steps(raw: serde_json::Value) -> Vec<StepInput> {
    serde_json::from_value(raw).expect("steps deserialize")
}

fn two_steps() -> Vec<StepInput> {
    steps(json!([
        {"action": "cards.claim", "input_template": {"id": "{{card.id}}"}},
        {"input_template": {"id": "{{card.id}}"}, "gate_kind": "human_approval"},
    ]))
}

#[tokio::test(flavor = "multi_thread")]
async fn declared_step_outputs_round_trip() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "s11-outputs").await;

    let mut tx = conveyor_store::begin_scoped(&db.runtime, tenant(&seed))
        .await
        .expect("scope");
    let created = process::create_draft_tx(
        &mut tx,
        tenant(&seed),
        &NewProcess {
            key: "outputs".to_owned(),
            title: "Declared outputs".to_owned(),
            description: String::new(),
            card_type_id: None,
            list_kind: Some("doing".to_owned()),
            project_type_id: None,
            outputs: json!(["delivery_report"]),
            capability_id: None,
            owner_user_id: None,
        },
    )
    .await
    .expect("create a draft");
    assert_eq!(created.list_kind.as_deref(), Some("doing"));
    assert_eq!(created.outputs, json!(["delivery_report"]));
    let declared = steps(json!([
        {"action": "cards.claim", "outputs": ["version", "claim_fence"]},
        {"action": "cards.comment", "input_template": {"id": "{{steps.1.output.version}}"}}
    ]));
    process::insert_steps_tx(&mut tx, tenant(&seed), created.id, &declared)
        .await
        .expect("insert steps");

    let read = process::list_steps_tx(&mut tx, tenant(&seed), created.id)
        .await
        .expect("list steps");
    assert_eq!(
        read[0].outputs,
        vec!["version".to_owned(), "claim_fence".to_owned()]
    );
    assert!(
        read[1].outputs.is_empty(),
        "an undeclared step exposes nothing"
    );
    assert_eq!(read[0].kind, "computed", "an action infers a computed step");
    assert_eq!(read[0].actor, "tool_agent");
    assert_eq!(read[0].action.as_deref(), Some("cards.claim"));
    tx.rollback().await.expect("rollback");
}

/// 0033's step columns: the kind/actor are inferred when absent, an empty action is stored as
/// NULL (the column is nullable now), and the capability/agent/SOP/call-target ids round-trip.
#[tokio::test(flavor = "multi_thread")]
async fn step_kinds_actors_and_targets_round_trip() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "s11-step-fields").await;

    let capability: Uuid = sqlx::query_scalar(
        "INSERT INTO capability (tenant_id, code, label) VALUES ($1, 'project_management', 'PM') RETURNING id",
    )
    .bind(seed.tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("capability");
    let process_agent: Uuid = sqlx::query_scalar(
        "INSERT INTO capability_agent (tenant_id, capability_id, key, model_ref, role, toolset)
         VALUES ($1, $2, 'tools-a', 'model:a', 'tool_agent', ARRAY['cards.move']) RETURNING id",
    )
    .bind(seed.tenant)
    .bind(capability)
    .fetch_one(&db.super_pool)
    .await
    .expect("capability agent");
    let sop: Uuid = sqlx::query_scalar(
        "INSERT INTO procedure (tenant_id, key, title) VALUES ($1, 'deliver', 'Deliver') RETURNING id",
    )
    .bind(seed.tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("procedure");

    let mut tx = conveyor_store::begin_scoped(&db.runtime, tenant(&seed))
        .await
        .expect("scope");
    let created = process::create_draft_tx(
        &mut tx,
        tenant(&seed),
        &NewProcess {
            key: "step-fields".to_owned(),
            title: "Step fields".to_owned(),
            description: String::new(),
            card_type_id: None,
            list_kind: None,
            project_type_id: None,
            outputs: json!([]),
            capability_id: None,
            owner_user_id: None,
        },
    )
    .await
    .expect("create a draft");
    let declared = steps(json!([
        {"kind": "agent", "process_agent_id": process_agent, "procedure_id": sop,
         "capability_id": capability},
        {"kind": "call", "child_process_key": "child.process"},
        {
            "kind": "gate",
            "gate_kind": "human_approval",
            "action": "",
            "actor": "human"
        }
    ]));
    process::insert_steps_tx(&mut tx, tenant(&seed), created.id, &declared)
        .await
        .expect("insert steps");
    process::replace_steps_tx(&mut tx, tenant(&seed), created.id, &declared)
        .await
        .expect("replace steps");

    let read = process::list_steps_tx(&mut tx, tenant(&seed), created.id)
        .await
        .expect("list steps");
    assert_eq!(read.len(), 3);
    assert_eq!(read[0].kind, "agent");
    assert_eq!(
        read[0].actor, "process_agent",
        "an agent step defaults to the process agent"
    );
    assert_eq!(read[0].action, None, "an agent step names no action");
    assert_eq!(read[0].capability_id, Some(capability));
    assert_eq!(read[0].process_agent_id, Some(process_agent));
    assert_eq!(read[0].procedure_id, Some(sop));

    assert_eq!(read[1].kind, "call");
    assert_eq!(read[1].actor, "system");
    assert_eq!(read[1].child_process_key.as_deref(), Some("child.process"));
    assert_eq!(read[1].action, None);

    assert_eq!(read[2].kind, "gate");
    assert_eq!(read[2].actor, "human", "an explicit actor wins");
    assert_eq!(
        read[2].action, None,
        "a blank action is stored as NULL, not an empty string"
    );
    tx.rollback().await.expect("rollback");
}

async fn new_draft(db: &support::TestDb, seed: &support::Seeded, key: &str) -> Uuid {
    let mut tx = conveyor_store::begin_scoped(&db.runtime, tenant(seed))
        .await
        .expect("scope");
    let created = process::create_draft_tx(
        &mut tx,
        tenant(seed),
        &NewProcess {
            key: key.to_owned(),
            title: "Delivery phase".to_owned(),
            description: String::new(),
            card_type_id: Some(seed.card_type),
            list_kind: None,
            project_type_id: None,
            outputs: json!([]),
            capability_id: None,
            owner_user_id: Some(seed.user),
        },
    )
    .await
    .expect("create a draft");
    process::insert_steps_tx(&mut tx, tenant(seed), created.id, &two_steps())
        .await
        .expect("insert steps");
    tx.commit().await.expect("commit");
    created.id
}

#[tokio::test(flavor = "multi_thread")]
async fn one_draft_and_one_published_per_key() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "s11-a").await;
    let id = new_draft(&db, &seed, "dev-phase").await;

    // A second draft of the same key is refused by `uq_process_draft`.
    let mut tx = conveyor_store::begin_scoped(&db.runtime, tenant(&seed))
        .await
        .expect("scope");
    let error = process::create_draft_tx(
        &mut tx,
        tenant(&seed),
        &NewProcess {
            key: "dev-phase".to_owned(),
            title: "Second draft".to_owned(),
            description: String::new(),
            card_type_id: None,
            list_kind: None,
            project_type_id: None,
            outputs: json!([]),
            capability_id: None,
            owner_user_id: None,
        },
    )
    .await
    .expect_err("a second draft");
    assert!(matches!(error, ProcessStoreError::DraftExists), "{error}");
    tx.rollback().await.expect("rollback");

    // Publishing moves it to `published`, and publishing a fork retires it.
    let mut tx = conveyor_store::begin_scoped(&db.runtime, tenant(&seed))
        .await
        .expect("scope");
    let published = process::publish_tx(&mut tx, tenant(&seed), id, None)
        .await
        .expect("publish");
    assert_eq!(published.status, "published");
    tx.commit().await.expect("commit");

    let mut tx = conveyor_store::begin_scoped(&db.runtime, tenant(&seed))
        .await
        .expect("scope");
    let fork = process::fork_version_tx(&mut tx, tenant(&seed), id, None)
        .await
        .expect("fork");
    assert_eq!(fork.version, 2, "the fork is the next version");
    assert_eq!(fork.status, "draft");
    let copied = process::list_steps_tx(&mut tx, tenant(&seed), fork.id)
        .await
        .expect("steps");
    assert_eq!(copied.len(), 2, "the fork copies the version it came from");
    assert_eq!(copied[0].action.as_deref(), Some("cards.claim"));

    process::publish_tx(&mut tx, tenant(&seed), fork.id, None)
        .await
        .expect("publish the fork");
    let previous = process::get_process_tx(&mut tx, tenant(&seed), id)
        .await
        .expect("read")
        .expect("the old version");
    assert_eq!(
        previous.status, "retired",
        "publishing v2 retires v1 in the same transaction"
    );
    let current = process::published_by_key_tx(&mut tx, tenant(&seed), "dev-phase")
        .await
        .expect("read")
        .expect("a published version");
    assert_eq!(current.id, fork.id);
    tx.commit().await.expect("commit");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_published_version_is_frozen() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "s11-b").await;
    let id = new_draft(&db, &seed, "frozen").await;

    // A draft edit replaces the whole step list.
    let mut tx = conveyor_store::begin_scoped(&db.runtime, tenant(&seed))
        .await
        .expect("scope");
    let edited = process::update_draft_tx(
        &mut tx,
        tenant(&seed),
        id,
        &process::DraftPatch {
            title: Some("Delivery phase (edited)"),
            steps: Some(&steps(json!([{"action": "cards.claim"}]))),
            ..process::DraftPatch::default()
        },
    )
    .await
    .expect("edit the draft");
    assert_eq!(edited.title, "Delivery phase (edited)");
    assert_eq!(
        edited.version, 1,
        "`version` is the family version (uq tenant+key+version), not an edit counter"
    );
    assert_eq!(
        process::list_steps_tx(&mut tx, tenant(&seed), id)
            .await
            .expect("steps")
            .len(),
        1
    );

    // `If-Match` naming a stale version is a conflict, not a silent overwrite.
    let error = process::update_draft_tx(
        &mut tx,
        tenant(&seed),
        id,
        &process::DraftPatch {
            expected: Some(Version::INITIAL),
            ..process::DraftPatch::default()
        },
    )
    .await
    .expect_err("stale If-Match");
    assert!(
        matches!(error, ProcessStoreError::VersionMismatch { .. }),
        "{error}"
    );

    process::publish_tx(&mut tx, tenant(&seed), id, None)
        .await
        .expect("publish");
    let error = process::update_draft_tx(
        &mut tx,
        tenant(&seed),
        id,
        &process::DraftPatch {
            title: Some("nope"),
            ..process::DraftPatch::default()
        },
    )
    .await
    .expect_err("published rows are frozen");
    assert!(matches!(error, ProcessStoreError::NotDraft(_)), "{error}");
    tx.rollback().await.expect("rollback");
}

use conveyor_domain::Version;

#[tokio::test(flavor = "multi_thread")]
async fn a_run_materialises_claims_and_appends_attempts() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "s11-c").await;
    let id = new_draft(&db, &seed, "trace").await;

    let mut tx = conveyor_store::begin_scoped(&db.runtime, tenant(&seed))
        .await
        .expect("scope");
    let published = process::publish_tx(&mut tx, tenant(&seed), id, None)
        .await
        .expect("publish");
    let steps = process::list_steps_tx(&mut tx, tenant(&seed), published.id)
        .await
        .expect("steps");
    let materialised = process::materialise_run_steps_tx(&mut tx, tenant(&seed), seed.run, &steps)
        .await
        .expect("materialise");
    assert_eq!(materialised.len(), 2);
    assert!(materialised.iter().all(|step| step.status == "queued"));

    // Claiming is a compare-and-set: the second claim of the same row sees nothing. The resolved
    // input is persisted by the claim so a crash replay never re-resolves against changed state.
    let resolved = json!({"card_id": "{{card.id}}", "body": "resolved once"});
    let claimed = process::claim_run_step_tx(&mut tx, tenant(&seed), seed.run, 1, 1, &resolved)
        .await
        .expect("claim")
        .expect("the step was queued");
    assert_eq!(claimed.status, "running");
    assert_eq!(claimed.input, resolved, "the claim persists the input");
    assert!(
        process::claim_run_step_tx(&mut tx, tenant(&seed), seed.run, 1, 1, &json!({}))
            .await
            .expect("claim twice")
            .is_none(),
        "a replay must not re-run the step"
    );
    let stored = process::get_run_step_by_triple_tx(&mut tx, tenant(&seed), seed.run, 1, 1)
        .await
        .expect("read the triple")
        .expect("the row exists");
    assert_eq!(stored.input, resolved, "a replay reads the stored input");

    // Recording writes the outcome; a second record on a finished row is a no-op.
    let recorded = process::record_run_step_tx(
        &mut tx,
        tenant(&seed),
        claimed.id,
        "failed",
        &json!({"id": "card"}),
        &json!({}),
        Some("boom"),
    )
    .await
    .expect("record")
    .expect("the step was running");
    assert_eq!(recorded.status, "failed");
    assert_eq!(recorded.error.as_deref(), Some("boom"));
    assert!(
        process::record_run_step_tx(
            &mut tx,
            tenant(&seed),
            claimed.id,
            "succeeded",
            &json!({}),
            &json!({}),
            None
        )
        .await
        .expect("record twice")
        .is_none()
    );

    // A retry appends attempt 2 at the same position; attempt 1 is still there.
    let retried = process::append_attempt_tx(&mut tx, tenant(&seed), claimed.id)
        .await
        .expect("append an attempt");
    assert_eq!(retried.position, 1);
    assert_eq!(retried.attempt, 2);
    assert_eq!(retried.status, "queued");
    let all = process::list_run_steps_tx(&mut tx, tenant(&seed), seed.run)
        .await
        .expect("list");
    assert_eq!(all.len(), 3, "two steps plus the appended attempt");
    tx.commit().await.expect("commit");
}

#[tokio::test(flavor = "multi_thread")]
async fn another_tenant_sees_nothing() {
    let db = support::TestDb::fresh().await;
    let mine = support::seed_tenant(&db.super_pool, "s11-d").await;
    let theirs = support::seed_tenant(&db.super_pool, "s11-e").await;
    let id = new_draft(&db, &mine, "private").await;

    let mut tx = conveyor_store::begin_scoped(&db.runtime, tenant(&theirs))
        .await
        .expect("scope");
    assert!(
        process::get_process_tx(&mut tx, tenant(&theirs), id)
            .await
            .expect("read")
            .is_none(),
        "another tenant must not see the row, even by id"
    );
    assert!(
        process::list_processes_tx(&mut tx, tenant(&theirs), None, None, None, 50)
            .await
            .expect("list")
            .is_empty()
    );
    assert!(
        process::list_run_steps_tx(&mut tx, tenant(&theirs), mine.run)
            .await
            .expect("list run steps")
            .is_empty()
    );
    tx.rollback().await.expect("rollback");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_key_lookup_reads_the_latest_version_or_a_named_one() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "s11-f").await;
    let v1 = new_draft(&db, &seed, "lookup").await;

    let mut tx = conveyor_store::begin_scoped(&db.runtime, tenant(&seed))
        .await
        .expect("scope");
    process::publish_tx(&mut tx, tenant(&seed), v1, None)
        .await
        .expect("publish v1");
    let fork = process::fork_version_tx(&mut tx, tenant(&seed), v1, None)
        .await
        .expect("fork");
    assert_eq!(fork.version, 2);

    // `None` is the LATEST version of any status: here the draft fork, not the published v1.
    let latest = process::get_process_by_key_tx(&mut tx, tenant(&seed), "lookup", None)
        .await
        .expect("read")
        .expect("the key exists");
    assert_eq!(latest.id, fork.id);
    assert_eq!(latest.version, 2);
    assert_eq!(latest.status, "draft");

    // A named version is that exact version, whatever its status.
    let first = process::get_process_by_key_tx(&mut tx, tenant(&seed), "lookup", Some(1))
        .await
        .expect("read")
        .expect("v1 exists");
    assert_eq!(first.id, v1);
    assert_eq!(first.status, "published");

    // An unknown version is absent, not the latest; so is an unknown key.
    assert!(
        process::get_process_by_key_tx(&mut tx, tenant(&seed), "lookup", Some(9))
            .await
            .expect("read")
            .is_none()
    );
    assert!(
        process::get_process_by_key_tx(&mut tx, tenant(&seed), "no-such-key", None)
            .await
            .expect("read")
            .is_none()
    );
    tx.commit().await.expect("commit");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_list_filters_by_card_type() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "s11-g").await;
    let typed = new_draft(&db, &seed, "typed").await;

    // A second process of the same tenant that names no card type.
    let untyped = {
        let mut tx = conveyor_store::begin_scoped(&db.runtime, tenant(&seed))
            .await
            .expect("scope");
        let created = process::create_draft_tx(
            &mut tx,
            tenant(&seed),
            &NewProcess {
                key: "untyped".to_owned(),
                title: "No card type".to_owned(),
                description: String::new(),
                card_type_id: None,
                list_kind: None,
                project_type_id: None,
                outputs: json!([]),
                capability_id: None,
                owner_user_id: None,
            },
        )
        .await
        .expect("create a draft");
        tx.commit().await.expect("commit");
        created.id
    };

    let mut tx = conveyor_store::begin_scoped(&db.runtime, tenant(&seed))
        .await
        .expect("scope");
    let filtered =
        process::list_processes_tx(&mut tx, tenant(&seed), None, Some(seed.card_type), None, 50)
            .await
            .expect("list");
    assert_eq!(filtered.len(), 1, "only the process naming that card type");
    assert_eq!(filtered[0].id, typed);

    let all = process::list_processes_tx(&mut tx, tenant(&seed), None, None, None, 50)
        .await
        .expect("list");
    assert_eq!(all.len(), 2, "no filter returns both");
    let ids: Vec<Uuid> = all.iter().map(|row| row.id).collect();
    assert!(ids.contains(&typed), "the typed process is listed");
    assert!(ids.contains(&untyped), "the untyped process is listed");
    tx.commit().await.expect("commit");
}

#[tokio::test(flavor = "multi_thread")]
async fn steps_for_a_page_are_grouped_by_process_then_position() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "s11-h").await;
    let first = new_draft(&db, &seed, "page-a").await;
    let second = new_draft(&db, &seed, "page-b").await;

    let mut tx = conveyor_store::begin_scoped(&db.runtime, tenant(&seed))
        .await
        .expect("scope");
    let rows = process::list_steps_for_processes_tx(&mut tx, tenant(&seed), &[first, second])
        .await
        .expect("steps for a page");
    assert_eq!(rows.len(), 4, "two steps for each of the two processes");

    // Each process is ONE contiguous run — `process_id` is arbitrary (uuidv7), so assert the
    // grouping, not which process comes first.
    let mut runs: Vec<Uuid> = Vec::new();
    for row in &rows {
        if runs.last() != Some(&row.process_id) {
            assert!(
                !runs.contains(&row.process_id),
                "a process's steps must not be split across the page"
            );
            runs.push(row.process_id);
        }
    }
    assert_eq!(runs.len(), 2);
    for id in [first, second] {
        let positions: Vec<i32> = rows
            .iter()
            .filter(|row| row.process_id == id)
            .map(|row| row.position)
            .collect();
        assert_eq!(positions, vec![1, 2], "positions ascend within a process");
    }

    // An empty page is an empty list, without a query to the database.
    assert!(
        process::list_steps_for_processes_tx(&mut tx, tenant(&seed), &[])
            .await
            .expect("empty page")
            .is_empty()
    );
    tx.commit().await.expect("commit");
}

/// The page size is the CALLER's to clamp, so a caller can ask for one row more than a page.
///
/// The action decides "is there another page" by reading `limit + 1` rows and popping the extra. Any clamp
/// applied here would eat that row, and a request for a full page would answer `next_cursor: None` while more
/// rows existed — a silent truncation, which is the one thing a paginated read must never do. This test needs
/// MORE rows than `MAX_LIMIT` (200) or it cannot tell a clamp from a pass-through.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_page_size_is_not_clamped_under_the_caller() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "s11-limit").await;
    const ROWS: i64 = 201;

    let mut tx = conveyor_store::begin_scoped(&db.runtime, tenant(&seed))
        .await
        .expect("scope");
    sqlx::query(
        "INSERT INTO process (tenant_id, key, title)
         SELECT $1, 'bulk-' || g, 'Bulk ' || g FROM generate_series(1, $2) g",
    )
    .bind(seed.tenant)
    .bind(ROWS as i32)
    .execute(&mut *tx)
    .await
    .expect("bulk drafts");

    let page = process::list_processes_tx(&mut tx, tenant(&seed), None, None, None, ROWS + 1)
        .await
        .expect("list a full page plus the probe row");
    assert_eq!(
        page.len() as i64,
        ROWS,
        "the store must return every row the caller asked for"
    );
    tx.commit().await.expect("commit");
}

/// A process run persists its version and inputs, and its first step's job is deduped.
///
/// `0015` added `run.process_id`/`run.inputs` with defaults; `insert_run_tx` bound neither, so a
/// process run inserted through it would look like an ordinary run and the executor could never
/// resolve the steps it must claim. The job's dedupe key is the `run_step` identity
/// (`<run_id>:<position>:<attempt>`), so a replay must collapse onto the live job rather than enqueue
/// a second driver for the same attempt.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_process_run_keeps_its_version_inputs_and_one_job_per_attempt() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "s11-run").await;
    let process_id = new_draft(&db, &seed, "dev-run").await;

    let mut tx = conveyor_store::begin_scoped(&db.runtime, tenant(&seed))
        .await
        .expect("scope");
    let run = conveyor_store::run::insert_run_tx(
        &mut tx,
        tenant(&seed),
        &conveyor_store::run::NewRun {
            card_id: None,
            project_id: seed.project,
            attempt: 1,
            correlation_id: None,
            process_id: Some(process_id),
            inputs: json!({"ticket": "T-1"}),
            capability_id: None,
            capability_agent_id: None,
            parent_run_id: None,
            step_id: None,
            workflow_step_id: None,
            depth: 0,
            process_version: 1,
            job_binding_id: None,
            release_hash: None,
        },
    )
    .await
    .expect("insert a process run");

    let (stored_process, stored_inputs, status): (Option<Uuid>, serde_json::Value, String) =
        sqlx::query_as(
            "SELECT process_id, inputs, status FROM run WHERE tenant_id = $1 AND id = $2",
        )
        .bind(seed.tenant)
        .bind(run.id)
        .fetch_one(&mut *tx)
        .await
        .expect("the run row");
    assert_eq!(stored_process, Some(process_id));
    assert_eq!(stored_inputs, json!({"ticket": "T-1"}));
    assert_eq!(status, "queued");

    let first = process::enqueue_process_step_tx(&mut tx, tenant(&seed), run.id, 1, 1)
        .await
        .expect("enqueue the first attempt")
        .expect("a new job");
    let replay = process::enqueue_process_step_tx(&mut tx, tenant(&seed), run.id, 1, 1)
        .await
        .expect("replay the enqueue");
    assert_eq!(
        replay, None,
        "the same (run, position, attempt) is one live job"
    );

    let (kind, dedupe_key, payload): (String, String, serde_json::Value) = sqlx::query_as(
        "SELECT kind, dedupe_key, payload FROM job WHERE tenant_id = $1 AND id = $2",
    )
    .bind(seed.tenant)
    .bind(first)
    .fetch_one(&mut *tx)
    .await
    .expect("the job row");
    assert_eq!(kind, process::PROCESS_STEP_KIND);
    assert_eq!(dedupe_key, process::process_step_dedupe_key(run.id, 1, 1));
    assert_eq!(dedupe_key, format!("{}:1:1", run.id));
    assert_eq!(payload["position"].as_i64(), Some(1));
    assert_eq!(payload["attempt"].as_i64(), Some(1));
    tx.commit().await.expect("commit");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_dead_step_sweep_marks_the_step_and_skips_the_rest() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "s11-reap").await;
    let id = new_draft(&db, &seed, "reap").await;

    // A running process run whose first step job has died (its lease budget is exhausted and
    // `reap_exhausted` has dead-lettered it).
    let mut tx = conveyor_store::begin_scoped(&db.runtime, tenant(&seed))
        .await
        .expect("scope");
    let published = process::publish_tx(&mut tx, tenant(&seed), id, None)
        .await
        .expect("publish");
    sqlx::query(
        "UPDATE run SET process_id = $3, status = 'running'
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant(&seed).as_uuid())
    .bind(seed.run)
    .bind(published.id)
    .execute(&mut *tx)
    .await
    .expect("bind the run to the process");
    let steps = process::list_steps_tx(&mut tx, tenant(&seed), published.id)
        .await
        .expect("steps");
    process::materialise_run_steps_tx(&mut tx, tenant(&seed), seed.run, &steps)
        .await
        .expect("materialise");
    process::enqueue_process_step_tx(&mut tx, tenant(&seed), seed.run, 1, 1)
        .await
        .expect("enqueue");
    sqlx::query(
        "UPDATE job SET status = 'dead', attempts = max_attempts
          WHERE tenant_id = $1 AND kind = 'process.step' AND dedupe_key = $2",
    )
    .bind(tenant(&seed).as_uuid())
    .bind(process::process_step_dedupe_key(seed.run, 1, 1))
    .execute(&mut *tx)
    .await
    .expect("dead-letter the job");
    tx.commit().await.expect("commit");

    let mut tx = conveyor_store::begin_scoped(&db.runtime, tenant(&seed))
        .await
        .expect("scope");
    let reaped = process::reap_dead_process_runs_tx(&mut tx, tenant(&seed))
        .await
        .expect("reap");
    tx.commit().await.expect("commit the reap");

    assert_eq!(reaped.len(), 1, "one run was closed");
    assert_eq!(reaped[0].run_id, seed.run);
    assert_eq!(
        reaped[0].steps.len(),
        2,
        "the failed step and the skipped one"
    );
    let statuses: Vec<(i32, String)> = sqlx::query_as(
        "SELECT position, status FROM run_step
          WHERE tenant_id = $1 AND run_id = $2 ORDER BY position",
    )
    .bind(tenant(&seed).as_uuid())
    .bind(seed.run)
    .fetch_all(&db.super_pool)
    .await
    .expect("the trace");
    assert_eq!(
        statuses,
        vec![(1, "failed".to_owned()), (2, "skipped".to_owned())]
    );
    let (status, error_class): (String, Option<String>) =
        sqlx::query_as("SELECT status, error_class FROM run WHERE id = $1")
            .bind(seed.run)
            .fetch_one(&db.super_pool)
            .await
            .expect("the run");
    assert_eq!(status, "failed");
    assert_eq!(error_class.as_deref(), Some("crashed"));

    // The sweep is idempotent: a second pass finds no live run to close.
    let mut tx = conveyor_store::begin_scoped(&db.runtime, tenant(&seed))
        .await
        .expect("scope");
    let again = process::reap_dead_process_runs_tx(&mut tx, tenant(&seed))
        .await
        .expect("reap twice");
    tx.commit().await.expect("commit");
    assert!(again.is_empty());
}

/// A dead job carries a per-transition suffix: `…:wait:<n>` for a judge park, `…:gate` for a
/// gate resume, `…:call:<n>` for a call poll. The sweep matched the dedupe key by EQUALITY, so a
/// dead park/gate/call job named no step and left it `running` forever while the run was failed
/// around it (audit 2026-11-10). The match is a prefix now.
#[tokio::test(flavor = "multi_thread")]
async fn the_dead_sweep_matches_a_park_or_gate_suffixed_job_key() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "s11-reap-suffix").await;
    let id = new_draft(&db, &seed, "reap-suffix").await;

    let mut tx = conveyor_store::begin_scoped(&db.runtime, tenant(&seed))
        .await
        .expect("scope");
    let published = process::publish_tx(&mut tx, tenant(&seed), id, None)
        .await
        .expect("publish");
    sqlx::query(
        "UPDATE run SET process_id = $3, status = 'running'
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant(&seed).as_uuid())
    .bind(seed.run)
    .bind(published.id)
    .execute(&mut *tx)
    .await
    .expect("bind the run to the process");
    let steps = process::list_steps_tx(&mut tx, tenant(&seed), published.id)
        .await
        .expect("steps");
    process::materialise_run_steps_tx(&mut tx, tenant(&seed), seed.run, &steps)
        .await
        .expect("materialise");
    process::enqueue_process_step_tx(&mut tx, tenant(&seed), seed.run, 1, 1)
        .await
        .expect("enqueue");
    // The step is mid-park (a judge outage): `running`, with the job's key carrying the suffix.
    sqlx::query(
        "UPDATE run_step SET status = 'running'
          WHERE tenant_id = $1 AND run_id = $2 AND position = 1 AND attempt = 1",
    )
    .bind(tenant(&seed).as_uuid())
    .bind(seed.run)
    .execute(&mut *tx)
    .await
    .expect("the parked step");
    sqlx::query(
        "UPDATE job SET status = 'dead', attempts = max_attempts, dedupe_key = $2 || ':wait:3'
          WHERE tenant_id = $1 AND kind = 'process.step' AND dedupe_key = $2",
    )
    .bind(tenant(&seed).as_uuid())
    .bind(process::process_step_dedupe_key(seed.run, 1, 1))
    .execute(&mut *tx)
    .await
    .expect("dead-letter the park job");
    tx.commit().await.expect("commit");

    let mut tx = conveyor_store::begin_scoped(&db.runtime, tenant(&seed))
        .await
        .expect("scope");
    let reaped = process::reap_dead_process_runs_tx(&mut tx, tenant(&seed))
        .await
        .expect("reap");
    tx.commit().await.expect("commit the reap");

    assert_eq!(reaped.len(), 1, "one run was closed");
    let statuses: Vec<(i32, String)> = sqlx::query_as(
        "SELECT position, status FROM run_step
          WHERE tenant_id = $1 AND run_id = $2 ORDER BY position",
    )
    .bind(tenant(&seed).as_uuid())
    .bind(seed.run)
    .fetch_all(&db.super_pool)
    .await
    .expect("the trace");
    assert_eq!(
        statuses,
        vec![(1, "failed".to_owned()), (2, "skipped".to_owned())],
        "the parked step is terminal, never left running"
    );
}

/// S13 §5.4: a step that timed out fails the run with `error_class = 'timeout'`, not the generic
/// `crashed`. A timeout is retryable, so the step row never records it; the job's `last_error`
/// is where the 504 landed, and the sweep maps it.
#[tokio::test(flavor = "multi_thread")]
async fn the_dead_sweep_names_a_timeout_as_its_error_class() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "s11-reap-timeout").await;
    let id = new_draft(&db, &seed, "reap-timeout").await;

    let mut tx = conveyor_store::begin_scoped(&db.runtime, tenant(&seed))
        .await
        .expect("scope");
    let published = process::publish_tx(&mut tx, tenant(&seed), id, None)
        .await
        .expect("publish");
    sqlx::query(
        "UPDATE run SET process_id = $3, status = 'running'
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant(&seed).as_uuid())
    .bind(seed.run)
    .bind(published.id)
    .execute(&mut *tx)
    .await
    .expect("bind the run to the process");
    process::enqueue_process_step_tx(&mut tx, tenant(&seed), seed.run, 1, 1)
        .await
        .expect("enqueue");
    sqlx::query(
        "UPDATE job SET status = 'dead', attempts = max_attempts,
                        last_error = 'action-timeout: cards.get did not finish within 15000 ms'
          WHERE tenant_id = $1 AND kind = 'process.step' AND dedupe_key = $2",
    )
    .bind(tenant(&seed).as_uuid())
    .bind(process::process_step_dedupe_key(seed.run, 1, 1))
    .execute(&mut *tx)
    .await
    .expect("dead-letter a timed-out job");
    tx.commit().await.expect("commit");

    let mut tx = conveyor_store::begin_scoped(&db.runtime, tenant(&seed))
        .await
        .expect("scope");
    process::reap_dead_process_runs_tx(&mut tx, tenant(&seed))
        .await
        .expect("reap");
    tx.commit().await.expect("commit the reap");

    let (status, error_class): (String, Option<String>) =
        sqlx::query_as("SELECT status, error_class FROM run WHERE id = $1")
            .bind(seed.run)
            .fetch_one(&db.super_pool)
            .await
            .expect("the run");
    assert_eq!(status, "failed");
    assert_eq!(error_class.as_deref(), Some("timeout"));
}
