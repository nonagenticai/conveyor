//! S13 / audit W6 — one command installs a tenant that can actually run.
//!
//! `migrate --process-catalogue --tenant <slug>` is the production install. Before 2026-11-10 it
//! installed processes, SOPs, agents and maps but NOT the vocabulary they name: a gate step's
//! `human_approval` was "not a gate in this tenant's catalogue", a card-type process's type was
//! absent, and a map's project type did not exist. The install now runs the catalogue vocabulary
//! first, and this test is the proof that the sequence works from a fresh database — through the
//! SAME functions the CLI calls, with no test-only SQL.
//!
//! `crates/actions/tests/process_catalogue.rs` proves the installed catalogue then RUNS end to end.

mod common;

use uuid::Uuid;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn one_command_installs_a_runnable_tenant_from_a_fresh_database() {
    let db = common::TestDb::fresh().await;
    // The dev tenants, exactly as `migrate --dev-seed` creates them: the CLI runs this on
    // `DATABASE_MIGRATE_URL`, so the owner pool is the faithful one (the runtime role has no
    // grants on the seed's tables).
    conveyor_store::seed::dev(&db.owner)
        .await
        .expect("dev tenants");

    let counts = conveyor_server::seed::seed_process_catalogue(&db.owner, Some("dev-a"))
        .await
        .expect("the production install command");
    let dev_a = counts
        .into_iter()
        .find(|(slug, _)| slug == "dev-a")
        .map(|(_, counts)| counts)
        .expect("dev-a was installed");
    // 280 card-type processes + 53 named, and the SOPs/agents/maps they need.
    assert_eq!(dev_a.processes, 333, "{dev_a:?}");
    assert_eq!(dev_a.maps, 3, "{dev_a:?}");
    // 70 type agents + 53 named ones (an agent is shared by every process of its card type),
    // and one SOP per process plus the mechanical `.system` SOP a gate or call step needs.
    assert_eq!(dev_a.agents, 123, "{dev_a:?}");
    assert!(dev_a.sops >= 403, "{dev_a:?}");

    let tenant: Uuid = sqlx::query_scalar("SELECT id FROM tenant WHERE slug = 'dev-a'")
        .fetch_one(&db.super_pool)
        .await
        .expect("the tenant");

    // The vocabulary a process NAMES is present: the gate code, the card types, the capability.
    let gates: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM catalogue_entry
          WHERE tenant_id = $1 AND kind = 'gate' AND code = 'human_approval'",
    )
    .bind(tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("the gate code");
    assert_eq!(gates, 1, "a seeded gate step must resolve");

    let card_types: i64 = sqlx::query_scalar("SELECT count(*) FROM card_type WHERE tenant_id = $1")
        .bind(tenant)
        .fetch_one(&db.super_pool)
        .await
        .expect("the card types");
    assert!(card_types >= 70, "the catalogue's card types: {card_types}");

    let kata: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM capability WHERE tenant_id = $1 AND code = 'kata'",
    )
    .bind(tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("the kata capability");
    assert_eq!(kata, 1, "kata.open must resolve its capability");

    // A gate process and a call process are published, with the steps that make them what they
    // are — the two step kinds the seed could not express before 2026-11-10.
    let (status, steps): (String, i64) = sqlx::query_as(
        "SELECT p.status, count(s.id) FROM process p
           JOIN process_step s ON s.process_id = p.id AND s.tenant_id = p.tenant_id
          WHERE p.tenant_id = $1 AND p.key = 'ask_fr'
          GROUP BY p.status",
    )
    .bind(tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("the ask_fr process");
    assert_eq!(status, "published");
    assert_eq!(steps, 2, "an agent step and the human gate");

    let gate_step: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM process_step s JOIN process p ON p.id = s.process_id
          WHERE p.tenant_id = $1 AND p.key = 'ask_fr' AND s.kind = 'gate'
            AND s.gate_kind = 'human_approval'",
    )
    .bind(tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("the ask_fr gate step");
    assert_eq!(gate_step, 1);

    let call_step: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM process_step s JOIN process p ON p.id = s.process_id
          WHERE p.tenant_id = $1 AND p.key = 'cap.kanban_coach.handle_error'
            AND s.kind = 'call' AND s.child_process_key = 'kata.open'",
    )
    .bind(tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("the coach's kata call");
    assert_eq!(call_step, 1);

    // The process a project starts is published with a live executor and a published SOP: the
    // three facts `process_runs.start` would otherwise refuse on.
    let (agent_live, sop_published): (i64, i64) = sqlx::query_as(
        "SELECT
           (SELECT count(*) FROM capability_agent a
              JOIN process_step s ON s.process_agent_id = a.id
              JOIN process p ON p.id = s.process_id
             WHERE p.tenant_id = $1 AND p.key = 'bug_analysis.READY'
               AND a.status = 'active' AND a.role = 'process_agent'),
           (SELECT count(*) FROM procedure pr
             WHERE pr.tenant_id = $1 AND pr.status = 'published')",
    )
    .bind(tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("the start process's readiness facts");
    assert!(agent_live >= 1, "bug_analysis.READY binds a live agent");
    assert!(
        sop_published >= 350,
        "every SOP is published: {sop_published}"
    );

    // Idempotent: a second install creates nothing.
    let again = conveyor_server::seed::seed_process_catalogue(&db.owner, Some("dev-a"))
        .await
        .expect("a second install");
    let again = again
        .into_iter()
        .find(|(slug, _)| slug == "dev-a")
        .map(|(_, counts)| counts)
        .expect("dev-a again");
    assert_eq!(again.processes, 0, "a re-run creates no process: {again:?}");
    assert_eq!(again.maps, 0, "a re-run creates no map: {again:?}");
}
