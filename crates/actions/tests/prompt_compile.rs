//! SPEC D27 — the compile seam's refusals and the two non-run surfaces.
//!
//! A prompt that cannot be compiled must fail **by name**, before a model call: an unknown pin,
//! a malformed ref, and a retired version are three different answers, and a caller that cannot
//! tell them apart cannot fix a misconfiguration. The audit and the copilot are proved here
//! because they have no run facts — their recipes are `system`-only, and that must compile with
//! an empty fact set rather than refuse.

mod support;

use conveyor_store::begin_scoped;
use uuid::Uuid;

use conveyor_actions::prompt::{PromptError, compile_for_audit, compile_for_copilot};

async fn fixture() -> (support::Pg, support::Seeded) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    (pg, seeded)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pin_resolves_by_name_and_every_refusal_names_its_reason() {
    let (pg, seeded) = fixture().await;

    let audit = compile_for_audit(&pg.runtime, seeded.tenant, "prompt_audit@1")
        .await
        .expect("prompt_audit@1 resolves");
    assert_eq!(audit.key, "prompt_audit");
    assert_eq!(audit.version, "1");
    assert!(
        audit.text.contains("You audit prompts"),
        "the seeded text is what compiles: {}",
        audit.text
    );
    assert_eq!(audit.sha256.len(), 64);

    // Malformed: no '@'.
    match compile_for_audit(&pg.runtime, seeded.tenant, "prompt_audit")
        .await
        .expect_err("malformed ref")
    {
        PromptError::InvalidRef(reference) => assert_eq!(reference, "prompt_audit"),
        other => panic!("expected InvalidRef, got {other:?}"),
    }

    // Unknown pin: a version that does not exist.
    match compile_for_audit(&pg.runtime, seeded.tenant, "prompt_audit@2")
        .await
        .expect_err("unknown pin")
    {
        PromptError::Unresolved(reference) => assert_eq!(reference, "prompt_audit@2"),
        other => panic!("expected Unresolved, got {other:?}"),
    }

    // Retired: the row exists but a new run may not pin it.
    sqlx::query(
        "UPDATE prompt SET status = 'retired' WHERE tenant_id = $1 AND key = 'prompt_audit'",
    )
    .bind(seeded.tenant_id)
    .execute(&pg.super_pool)
    .await
    .expect("retire");
    match compile_for_audit(&pg.runtime, seeded.tenant, "prompt_audit@1")
        .await
        .expect_err("retired pin")
    {
        PromptError::Retired { reference, status } => {
            assert_eq!(reference, "prompt_audit@1");
            assert_eq!(status, "retired");
        }
        other => panic!("expected Retired, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_copilot_compiles_from_its_catalogue_row_with_no_run_facts() {
    let (pg, seeded) = fixture().await;
    let conversation: Uuid = {
        let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
            .await
            .expect("scope");
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO conversation (tenant_id, user_id, project_id)
             VALUES ($1, $2, NULL) RETURNING id",
        )
        .bind(seeded.tenant_id)
        .bind(seeded.user_id)
        .fetch_one(&mut *tx)
        .await
        .expect("conversation");
        tx.commit().await.expect("commit");
        id
    };

    let compiled =
        compile_for_copilot(&pg.runtime, seeded.tenant, conversation, "copilot_system@1")
            .await
            .expect("the copilot prompt resolves");
    assert_eq!(compiled.key, "copilot_system");
    assert_eq!(
        compiled.text,
        format!(
            "## Instructions\n{}",
            conveyor_store::prompts::COPILOT_SYSTEM_V1.trim_end()
        ),
        "the seeded row is the one source of the copilot's system message"
    );

    // A retired copilot prompt is refused, not silently replaced by the constant.
    sqlx::query(
        "UPDATE prompt SET status = 'retired' WHERE tenant_id = $1 AND key = 'copilot_system'",
    )
    .bind(seeded.tenant_id)
    .execute(&pg.super_pool)
    .await
    .expect("retire");
    assert!(matches!(
        compile_for_copilot(&pg.runtime, seeded.tenant, conversation, "copilot_system@1")
            .await
            .expect_err("retired"),
        PromptError::Retired { .. }
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_over_budget_recipe_refuses_with_the_measured_size_through_the_action_seam() {
    let (pg, seeded) = fixture().await;

    // A literal-only recipe whose whole-prompt budget is one character short of what it renders.
    // The rendered block includes its heading, so `measured` is larger than the text alone.
    let text = "You audit prompts.";
    let budget = i32::try_from(text.chars().count()).expect("fits") - 1;
    let prompt_id: Uuid = sqlx::query_scalar(
        "INSERT INTO prompt (tenant_id, key, version, status, title, budget_chars)
         VALUES ($1, 'tight', 1, 'active', 'Tight', $2) RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(budget)
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert prompt");
    sqlx::query(
        "INSERT INTO prompt_section (tenant_id, prompt_id, position, source_kind, required, template)
         VALUES ($1, $2, 1, 'system', true, $3)",
    )
    .bind(seeded.tenant_id)
    .bind(prompt_id)
    .bind(text)
    .execute(&pg.super_pool)
    .await
    .expect("insert section");

    let refused = compile_for_audit(&pg.runtime, seeded.tenant, "tight@1")
        .await
        .expect_err("over budget");
    let (measured, limit) = match &refused {
        PromptError::BudgetExceeded {
            section,
            measured,
            limit,
            ..
        } => {
            assert!(
                section.is_none(),
                "the whole prompt is over its budget, not one section"
            );
            assert_eq!(*limit, u32::try_from(budget).expect("positive"));
            assert!(
                *measured > *limit,
                "the refusal carries the real rendered size"
            );
            (*measured, *limit)
        }
        other => panic!("expected BudgetExceeded, got {other:?}"),
    };

    // The action seam turns it into a readiness refusal naming the field, not a 500 and not an
    // empty prompt.
    let error = refused.into_action_error("prompt_audit");
    assert_eq!(error.status, 409);
    assert_eq!(error.code, "capability_not_ready");
    let field = error.errors.first().expect("one field error");
    assert_eq!(field.field, "prompt_ref");
    assert_eq!(field.code, "prompt_over_budget");
    assert!(
        field.detail.contains(&measured.to_string()) && field.detail.contains(&limit.to_string()),
        "the detail names both numbers: {}",
        field.detail
    );

    // At the exact measurement it compiles: the refusal was not an off-by-one.
    sqlx::query("UPDATE prompt SET budget_chars = $3 WHERE tenant_id = $1 AND id = $2")
        .bind(seeded.tenant_id)
        .bind(prompt_id)
        .bind(i32::try_from(measured).expect("fits"))
        .execute(&pg.super_pool)
        .await
        .expect("raise the budget to the measurement");
    assert!(
        compile_for_audit(&pg.runtime, seeded.tenant, "tight@1")
            .await
            .is_ok(),
        "exactly at the measured size must compile"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_section_budget_names_the_section_through_the_action_seam() {
    let (pg, seeded) = fixture().await;

    // The same literal text, but the ceiling sits on the SECTION, not on the prompt.
    let prompt_id: Uuid = sqlx::query_scalar(
        "INSERT INTO prompt (tenant_id, key, version, status, title)
         VALUES ($1, 'tight_section', 1, 'active', 'Tight section') RETURNING id",
    )
    .bind(seeded.tenant_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert prompt");
    sqlx::query(
        "INSERT INTO prompt_section
             (tenant_id, prompt_id, position, source_kind, required, template, budget_chars)
         VALUES ($1, $2, 1, 'system', true, 'You audit prompts.', 1)",
    )
    .bind(seeded.tenant_id)
    .bind(prompt_id)
    .execute(&pg.super_pool)
    .await
    .expect("insert section");

    let refused = compile_for_audit(&pg.runtime, seeded.tenant, "tight_section@1")
        .await
        .expect_err("over budget");
    match &refused {
        PromptError::BudgetExceeded {
            section,
            measured,
            limit,
            ..
        } => {
            assert_eq!(
                *section,
                Some(conveyor_domain::prompt::PromptSource::System),
                "the refusal names the section, not the whole prompt"
            );
            assert_eq!(*limit, 1);
            assert!(*measured > *limit);
        }
        other => panic!("expected BudgetExceeded, got {other:?}"),
    }

    let error = refused.into_action_error("prompt_audit");
    assert_eq!(error.status, 409);
    let field = error.errors.first().expect("one field error");
    assert_eq!(field.field, "prompt_ref");
    assert_eq!(field.code, "prompt_over_budget");
    assert!(
        field.detail.contains("the system section"),
        "the detail names the section: {}",
        field.detail
    );
}
