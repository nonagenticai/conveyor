#![forbid(unsafe_code)]
//! S16 store: the prompt audit's state machine, the auto-audit race guard, the finding decision
//! gate and the cadence counter.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_domain::inference::CacheStatus;
use conveyor_domain::prompt_audit::{
    AuditStatus, Confidence, FindingAction, FindingDraft, FindingStatus, Pattern,
};
use conveyor_store::begin_scoped;
use conveyor_store::inference::{NewInferenceCall, insert_call_tx};
use conveyor_store::prompt_audit::{
    ListFindings, NewPromptAudit, PROMPT_REF_V1, PromptAuditStoreError, calls_since_last_audit_tx,
    decide_finding_tx, insert_audit_tx, insert_auto_audit_tx, insert_finding_tx, list_findings_tx,
    transition_audit_tx,
};
use uuid::Uuid;

async fn tenant(db: &support::TestDb, slug: &str) -> TenantId {
    let id: Uuid =
        sqlx::query_scalar("INSERT INTO tenant (slug, name) VALUES ($1, $1) RETURNING id")
            .bind(slug)
            .fetch_one(&db.super_pool)
            .await
            .expect("seed tenant");
    TenantId::from_uuid(id).expect("non-nil")
}

fn audit(requested_by: Option<Uuid>) -> NewPromptAudit {
    NewPromptAudit {
        scope: serde_json::json!({"surfaces": ["processes"], "process_ids": []}),
        target_model: "audit-model".to_owned(),
        prompt_ref: PROMPT_REF_V1.to_owned(),
        requested_by,
    }
}

fn finding(location: &str) -> FindingDraft {
    FindingDraft {
        location: location.to_owned(),
        evidence: "OLD TEXT".to_owned(),
        pattern: Pattern::DatedInstruction,
        why_obsolete: "the target model follows the plain instruction".to_owned(),
        confidence: Confidence::High,
        action: FindingAction::Rewrite,
        replacement: Some("NEW TEXT".to_owned()),
    }
}

#[tokio::test]
async fn audits_follow_the_state_machine_and_findings_are_decided_once() {
    let db = support::TestDb::fresh().await;
    let tenant = tenant(&db, "s16-audit").await;
    let decider = Uuid::now_v7();

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let id = insert_audit_tx(&mut tx, tenant, &audit(Some(decider)))
        .await
        .expect("insert audit");
    // An illegal jump is refused before any write.
    assert!(matches!(
        transition_audit_tx(&mut tx, tenant, id, AuditStatus::Succeeded, None, None).await,
        Err(PromptAuditStoreError::Illegal(_))
    ));
    let running = transition_audit_tx(&mut tx, tenant, id, AuditStatus::Running, None, None)
        .await
        .expect("queued → running");
    assert_eq!(running.status, "running");
    // A same-status replay is a no-op, not an error (the worker retries).
    let again = transition_audit_tx(&mut tx, tenant, id, AuditStatus::Running, None, None)
        .await
        .expect("replay");
    assert_eq!(again.status, "running");
    let done = transition_audit_tx(&mut tx, tenant, id, AuditStatus::Succeeded, Some(1), None)
        .await
        .expect("running → succeeded");
    assert_eq!((done.status.as_str(), done.finding_count), ("succeeded", 1));

    let finding_id = insert_finding_tx(&mut tx, tenant, id, &finding("process_step:abc"))
        .await
        .expect("insert finding");
    let found = list_findings_tx(
        &mut tx,
        tenant,
        &ListFindings {
            audit_id: Some(id),
            limit: 10,
            ..ListFindings::default()
        },
    )
    .await
    .expect("list findings");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].status, "open");

    let accepted = decide_finding_tx(
        &mut tx,
        tenant,
        finding_id,
        FindingStatus::Accepted,
        decider,
    )
    .await
    .expect("accept");
    assert_eq!(accepted.status, "accepted");
    assert_eq!(accepted.decided_by, Some(decider));
    // The same decision is idempotent; a different one is refused.
    decide_finding_tx(
        &mut tx,
        tenant,
        finding_id,
        FindingStatus::Accepted,
        decider,
    )
    .await
    .expect("idempotent accept");
    assert!(matches!(
        decide_finding_tx(
            &mut tx,
            tenant,
            finding_id,
            FindingStatus::Rejected,
            decider
        )
        .await,
        Err(PromptAuditStoreError::AlreadyDecided(_))
    ));
    tx.commit().await.expect("commit");
}

#[tokio::test]
async fn the_auto_audit_guard_allows_exactly_one_live_audit() {
    let db = support::TestDb::fresh().await;
    let tenant = tenant(&db, "s16-auto").await;
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let first = insert_auto_audit_tx(&mut tx, tenant, &audit(None))
        .await
        .expect("first auto audit");
    assert!(first.is_some(), "the first event-created audit inserts");
    let second = insert_auto_audit_tx(&mut tx, tenant, &audit(None))
        .await
        .expect("second auto audit");
    assert!(second.is_none(), "a live auto audit refuses a second");
    // A manual audit is not restricted by the auto guard (the partial index only covers
    // `requested_by IS NULL`).
    let manual = insert_audit_tx(&mut tx, tenant, &audit(Some(Uuid::now_v7())))
        .await
        .expect("manual audit");
    assert!(!manual.is_nil());
    // Once the first is terminal, the next event can create one.
    transition_audit_tx(
        &mut tx,
        tenant,
        first.expect("first id"),
        AuditStatus::Failed,
        None,
        Some("boom"),
    )
    .await
    .expect("fail the first");
    let third = insert_auto_audit_tx(&mut tx, tenant, &audit(None))
        .await
        .expect("third auto audit");
    assert!(third.is_some(), "a terminal audit frees the guard");
    tx.commit().await.expect("commit");
}

#[tokio::test]
async fn the_cadence_counts_the_ledger_and_excludes_the_audit_itself() {
    let db = support::TestDb::fresh().await;
    let tenant = tenant(&db, "s16-cadence").await;

    let id = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let id = insert_audit_tx(&mut tx, tenant, &audit(None))
            .await
            .expect("insert audit");
        tx.commit().await.expect("commit");
        id
    };
    // Put the audit's creation a second in the past so the calls below are unambiguously after it.
    sqlx::query("UPDATE prompt_audit SET created_at = now() - interval '1 second' WHERE id = $1")
        .bind(id)
        .execute(&db.super_pool)
        .await
        .expect("age the audit");

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    for prompt_ref in [None, Some("copilot"), Some(PROMPT_REF_V1)] {
        insert_call_tx(
            &mut tx,
            tenant,
            &NewInferenceCall {
                action_invocation_id: None,
                trace_id: None,
                correlation_id: None,
                caller_kind: "other",
                run_id: None,
                run_step_id: None,
                conversation_turn_id: None,
                eval_run_id: None,
                prompt_ref,
                prompt_version: None,
                prompt_sha256: None,
                model: "m",
                served_model: None,
                usage: None,
                cache_status: CacheStatus::Unknown.as_str(),
                latency_ms: 1,
                outcome: "ok",
            },
        )
        .await
        .expect("insert call");
    }
    let count = calls_since_last_audit_tx(&mut tx, tenant)
        .await
        .expect("cadence count");
    assert_eq!(
        count, 2,
        "two ordinary calls count; the audit's own prompt_audit@1 call is excluded"
    );
    tx.commit().await.expect("commit");
}
