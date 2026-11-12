//! S13's default process catalogue (`seed/processes/**`): parse, validate, apply.
//!
//! The properties, each a way this could go wrong silently:
//!
//! 1. **The exemplar parses to its documented shape** — four processes and five SOPs.
//! 2. **Apply is idempotent** — a second run inserts nothing, creates no version 2 and leaves
//!    every id where it was. A loader that re-published would leave `version = 2` behind.
//! 3. **A malformed file is refused with the typed error** naming the file, the process and the
//!    step, before any write.
//!
//! `rbuild -- test -p conveyor-store --test process_seed -- --nocapture`.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_store::process_seed::{self, ProcessSeedError, SeedCounts};
use uuid::Uuid;

fn tenant(seed: &support::Seeded) -> TenantId {
    TenantId::from_uuid(seed.tenant).expect("non-nil")
}

/// `(position, kind, actor, action, procedure_id, process_agent_id, capability_id)`.
type StepColumns = (
    i32,
    String,
    String,
    Option<String>,
    Option<Uuid>,
    Option<Uuid>,
    Option<Uuid>,
);

/// `(role, process_key, status, prompt_ref, model_ref, max_iterations, timeout_secs)`.
type AgentColumns = (
    String,
    Option<String>,
    String,
    Option<String>,
    String,
    i32,
    i32,
);

#[test]
fn the_exemplar_type_file_yields_four_processes_and_five_sops() {
    let catalogue = process_seed::catalogue().expect("the embedded catalogue validates");

    let exemplar = catalogue
        .types
        .iter()
        .find(|file| file.card_type == "development_bug_fix")
        .expect("the exemplar type file is embedded");

    assert_eq!(exemplar.processes.len(), 4, "one process per list stage");
    assert_eq!(
        exemplar.sops.len(),
        5,
        "four stage SOPs plus the computed-move SOP"
    );
    assert_eq!(exemplar.agents.len(), 1, "the type's one process agent");
    assert_eq!(exemplar.capability, "development");

    let mut stages: Vec<&str> = exemplar
        .processes
        .iter()
        .map(|process| {
            process
                .list
                .as_deref()
                .expect("a type process names its list stage")
        })
        .collect();
    stages.sort_unstable();
    assert_eq!(stages, ["doing", "done", "qa", "ready"]);

    // Every step resolves inside the file — the validation the loader enforces.
    let agent = &exemplar.agents[0];
    assert_eq!(agent.key, "development_bug_fix");
    assert_eq!(agent.prompt_ref().as_deref(), Some("agent@1"));
    for process in &exemplar.processes {
        for step in &process.steps {
            assert!(
                exemplar.sops.iter().any(|sop| sop.key == step.sop),
                "{} step {} names an undefined SOP {}",
                process.key,
                step.position,
                step.sop
            );
            if let Some(key) = step.process_agent.as_deref() {
                assert!(
                    exemplar.agents.iter().any(|agent| agent.key == key),
                    "{} step {} names an undefined agent {key}",
                    process.key,
                    step.position
                );
            }
        }
    }
}

/// Seed every capability the embedded catalogue names. `migrate --seed` imports the real 17-row
/// vocabulary; the fixture reads the codes out of the catalogue itself so it survives the
/// catalogue growing file by file.
async fn seed_capabilities(db: &support::TestDb, seed: &support::Seeded) {
    let catalogue = process_seed::catalogue().expect("the embedded catalogue validates");
    let mut codes: Vec<String> = catalogue
        .types
        .iter()
        .map(|file| file.capability.clone())
        .collect();
    for file in &catalogue.types {
        codes.extend(
            file.agents
                .iter()
                .filter_map(|agent| agent.capability.clone()),
        );
        codes.extend(
            file.processes
                .iter()
                .filter_map(|process| process.capability.clone()),
        );
    }
    if let Some(named) = &catalogue.named {
        codes.extend(named.capability.iter().cloned());
        codes.extend(
            named
                .agents
                .iter()
                .filter_map(|agent| agent.capability.clone()),
        );
        codes.extend(
            named
                .processes
                .iter()
                .filter_map(|process| process.capability.clone()),
        );
    }
    codes.sort();
    codes.dedup();
    // ⛔ The fixture must not seed codes the REAL import does not provide. Deriving them from the
    // catalogue alone made this test pass while `seed/capability.json` said `bdd_tdd` and the
    // files said `bddtdd`, and while `kata` existed nowhere — a production `--seed` would refuse
    // the catalogue. Assert against the shipped vocabulary, then insert.
    let shipped: Vec<String> = conveyor_store::seed::capability_export()
        .expect("the embedded capability seed parses")
        .capabilities
        .into_iter()
        .map(|capability| capability.code)
        .collect();
    for code in &codes {
        assert!(
            shipped.contains(code),
            "the process catalogue names capability {code:?}, which seed/capability.json does \
             not import"
        );
    }
    for code in codes {
        sqlx::query(
            "INSERT INTO capability (tenant_id, code, label) VALUES ($1, $2, $2)
             ON CONFLICT (tenant_id, code) DO NOTHING",
        )
        .bind(seed.tenant)
        .bind(&code)
        .execute(&db.super_pool)
        .await
        .expect("seed a catalogue capability");
    }
}

/// The project types that declare phases, read from the SHIPPED catalogue.
///
/// ⛔ Not a hardcoded copy: the map validator resolves a phase against `meta.phases`, so a
/// `seed/catalogue.json` edit that changes a type's phases must move this test (and the loader)
/// with it, never pass against a stale fixture.
fn catalogue_project_types() -> Vec<(String, String, Vec<String>)> {
    let raw = include_str!("../../../seed/catalogue.json");
    let document: serde_json::Value =
        serde_json::from_str(raw).expect("seed/catalogue.json parses");
    document["entries"]
        .as_array()
        .expect("catalogue entries")
        .iter()
        .filter(|entry| entry["kind"] == "project_type")
        .filter_map(|entry| {
            let code = entry["code"].as_str()?.to_owned();
            let label = entry["label"].as_str().unwrap_or(&code).to_owned();
            let phases: Vec<String> = entry["meta"]["phases"]
                .as_array()
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|phase| phase.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default();
            (!phases.is_empty()).then_some((code, label, phases))
        })
        .collect()
}

/// Seed the project types `maps.yaml` binds to, with the phases `catalogue.json` declares.
async fn seed_project_types(db: &support::TestDb, seed: &support::Seeded) {
    for (code, label, phases) in catalogue_project_types() {
        sqlx::query(
            "INSERT INTO catalogue_entry (tenant_id, kind, code, label, meta, sort)
             VALUES ($1, 'project_type', $2, $3, $4, 0)",
        )
        .bind(seed.tenant)
        .bind(&code)
        .bind(&label)
        .bind(serde_json::json!({ "phases": phases }))
        .execute(&db.super_pool)
        .await
        .expect("seed a project type");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn applying_the_catalogue_twice_inserts_nothing_and_creates_no_version_two() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "s13-process-seed").await;
    let tenant = tenant(&seed);

    // The capabilities the catalogue's agents hang off. `migrate --seed` imports the real 17-row
    // vocabulary; the loader refuses a file whose capability is absent.
    seed_capabilities(&db, &seed).await;
    seed_project_types(&db, &seed).await;

    let first = process_seed::apply_all(&db.runtime, tenant)
        .await
        .expect("the first apply writes the catalogue");
    assert!(first.processes >= 4, "created: {first:?}");
    assert!(first.sops >= 5, "created: {first:?}");
    assert!(first.agents >= 1, "created: {first:?}");
    assert!(first.card_types >= 1, "created: {first:?}");
    assert_eq!(first.maps, 3, "maps.yaml ships three maps: {first:?}");

    let process_ids: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM process WHERE tenant_id = $1 ORDER BY id")
            .bind(seed.tenant)
            .fetch_all(&db.super_pool)
            .await
            .expect("read the process ids");
    assert_eq!(
        process_ids.len(),
        first.processes,
        "every created process is one row"
    );
    let sop_rows: i64 = sqlx::query_scalar("SELECT count(*) FROM procedure WHERE tenant_id = $1")
        .bind(seed.tenant)
        .fetch_one(&db.super_pool)
        .await
        .expect("count the SOPs");
    assert_eq!(sop_rows, first.sops as i64, "every created SOP is one row");

    // The created card type took its label from `seed/catalogue.json`, not from the code.
    let label: String =
        sqlx::query_scalar("SELECT label FROM card_type WHERE tenant_id = $1 AND code = $2")
            .bind(seed.tenant)
            .bind("development_bug_fix")
            .fetch_one(&db.super_pool)
            .await
            .expect("the created card type");
    assert_eq!(label, "Development bug fix");

    // The agent row carries the seed's fields, not `create_agent_tx`'s defaults.
    let agent: AgentColumns = sqlx::query_as(
        "SELECT role, process_key, status, prompt_ref, model_ref, max_iterations, timeout_secs
           FROM capability_agent WHERE tenant_id = $1 AND key = 'development_bug_fix'",
    )
    .bind(seed.tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("the created agent");
    assert_eq!(agent.0, "process_agent");
    assert_eq!(agent.1.as_deref(), Some("development_bug_fix"));
    assert_eq!(agent.2, "active");
    assert_eq!(agent.3.as_deref(), Some("agent@1"));
    assert_eq!(agent.4, "claude-sonnet-5-5");
    assert_eq!(agent.5, 12);
    assert_eq!(agent.6, 120);

    // The READY process resolved its step's SOP, agent and capability to uuids.
    let ready: Uuid = sqlx::query_scalar(
        "SELECT id FROM process
          WHERE tenant_id = $1 AND key = 'development_bug_fix.READY' AND status = 'published'",
    )
    .bind(seed.tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("the published READY process");
    let steps: Vec<StepColumns> = sqlx::query_as(
        "SELECT position, kind, actor, action, procedure_id, process_agent_id, capability_id
               FROM process_step WHERE tenant_id = $1 AND process_id = $2 ORDER BY position",
    )
    .bind(seed.tenant)
    .bind(ready)
    .fetch_all(&db.super_pool)
    .await
    .expect("the READY steps");
    assert_eq!(steps.len(), 2, "readying + the computed move");
    let (position, kind, actor, action, procedure_id, process_agent_id, capability_id) =
        steps[0].clone();
    assert_eq!(position, 1);
    assert_eq!(kind, "agent");
    assert_eq!(actor, "process_agent");
    assert_eq!(action.as_deref(), Some("agent.run"));
    assert!(process_agent_id.is_some(), "the agent key is unresolved");
    assert!(capability_id.is_some(), "the capability code is unresolved");
    let procedure_id = procedure_id.expect("the step names a published SOP");
    let procedure_key: String =
        sqlx::query_scalar("SELECT key FROM procedure WHERE tenant_id = $1 AND id = $2")
            .bind(seed.tenant)
            .bind(procedure_id)
            .fetch_one(&db.super_pool)
            .await
            .expect("the step's SOP");
    assert_eq!(procedure_key, "development_bug_fix.ready");

    // ── the process maps ───────────────────────────────────────────────────────────────────
    let maps: Vec<(Uuid, String, Uuid, i64, serde_json::Value)> = sqlx::query_as(
        "SELECT id, key, project_type_id, version, definition FROM process_map
          WHERE tenant_id = $1 AND status = 'published' ORDER BY key",
    )
    .bind(seed.tenant)
    .fetch_all(&db.super_pool)
    .await
    .expect("the published process maps");
    assert_eq!(maps.len(), 3, "maps.yaml ships exactly three maps");

    // The key, its project type and the exact phase sequence, in order — the loader resolved
    // every node against the project type's `meta.phases`, so a silent drift shows here.
    let mut expected: Vec<(String, Vec<String>)> = catalogue_project_types()
        .into_iter()
        .map(|(code, _, phases)| (code, phases))
        .collect();
    expected.sort();
    let mut map_ids: Vec<Uuid> = Vec::new();
    for ((id, key, project_type_id, version, definition), (expected_key, phases)) in
        maps.iter().zip(expected)
    {
        map_ids.push(*id);
        assert_eq!(key.as_str(), expected_key);
        assert_eq!(*version, 1, "{key} is published as version 1");

        let catalogue_id: Uuid = sqlx::query_scalar(
            "SELECT id FROM catalogue_entry
              WHERE tenant_id = $1 AND kind = 'project_type' AND code = $2",
        )
        .bind(seed.tenant)
        .bind(key)
        .fetch_one(&db.super_pool)
        .await
        .expect("the seeded project type");
        assert_eq!(
            *project_type_id, catalogue_id,
            "{key} is bound to the uuid of its project type"
        );

        let document: conveyor_domain::process_map::MapDoc =
            serde_json::from_value(definition.clone()).expect("the stored definition is a MapDoc");
        assert_eq!(document.schema, "1");
        assert!(
            document
                .nodes
                .iter()
                .all(|node| node.kind == conveyor_domain::process_map::NodeKind::Phase),
            "{key} draws phases only"
        );
        let refs: Vec<&str> = document
            .nodes
            .iter()
            .map(|node| node.reference.as_str())
            .collect();
        assert_eq!(
            refs.as_slice(),
            phases,
            "{key} stores its project type's phases, in order"
        );
        assert_eq!(
            document.edges.len(),
            phases.len() - 1,
            "{key} links consecutive phases"
        );
        for (index, edge) in document.edges.iter().enumerate() {
            assert_eq!(edge.from, phases[index]);
            assert_eq!(edge.to, phases[index + 1]);
        }
    }

    // ── the second run ──────────────────────────────────────────────────────────────────────
    let second = process_seed::apply_all(&db.runtime, tenant)
        .await
        .expect("the second apply is a no-op");
    assert_eq!(
        second,
        SeedCounts::default(),
        "the second run creates nothing"
    );
    assert_eq!(second.maps, 0, "no map is re-created");

    let after: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM process WHERE tenant_id = $1 ORDER BY id")
            .bind(seed.tenant)
            .fetch_all(&db.super_pool)
            .await
            .expect("re-read the process ids");
    assert_eq!(after, process_ids, "the second run changes no id");

    let maps_after: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM process_map WHERE tenant_id = $1 ORDER BY id")
            .bind(seed.tenant)
            .fetch_all(&db.super_pool)
            .await
            .expect("re-read the map ids");
    assert_eq!(maps_after.len(), 3, "the second run creates no map");
    map_ids.sort_unstable();
    assert_eq!(maps_after, map_ids, "the second run changes no map id");

    // A re-publish is the one write that would not change a count of published rows; the family
    // version is what catches it.
    let versioned: i64 = sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM process WHERE tenant_id = $1 AND version > 1)
              + (SELECT count(*) FROM procedure WHERE tenant_id = $1 AND version > 1)
              + (SELECT count(*) FROM process_map WHERE tenant_id = $1 AND version > 1)",
    )
    .bind(seed.tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("count version 2+ rows");
    assert_eq!(
        versioned, 0,
        "a second apply must publish no new version (process, procedure or map)"
    );
}

// ── the malformed-file refusal ───────────────────────────────────────────────────────────────

fn process_yaml(key: &str, stage: &str, sop: &str) -> String {
    format!(
        "  - key: {key}\n    list: {stage}\n    title: {stage}\n    steps:\n      \
         - {{position: 1, kind: computed, actor: system, sop: {sop}}}\n"
    )
}

fn type_file_yaml(processes: &[(String, String, String)]) -> String {
    let mut yaml = String::from(
        "card_type: broken_type\n\
         capability: development\n\
         agent: {key: broken_type, model: claude-sonnet-5-5}\n\
         sops:\n  - {key: broken_type.system, title: System}\n\
         processes:\n",
    );
    for (key, stage, sop) in processes {
        yaml.push_str(&process_yaml(key, stage, sop));
    }
    yaml
}

fn four_processes(bad_sop: &str, duplicate_stage: bool) -> String {
    let stages = if duplicate_stage {
        ["ready", "ready", "qa", "done"]
    } else {
        ["ready", "doing", "qa", "done"]
    };
    let processes: Vec<(String, String, String)> = stages
        .iter()
        .enumerate()
        .map(|(index, stage)| {
            let key = format!("broken_type.{stage}{index}");
            (key, (*stage).to_owned(), bad_sop.to_owned())
        })
        .collect();
    type_file_yaml(&processes)
}

#[test]
fn a_missing_sop_reference_is_refused_with_the_file_process_and_step() {
    let yaml = four_processes("broken_type.missing", false);
    let error = process_seed::parse_type_file("types/broken_type.yaml", &yaml)
        .expect_err("a step naming an undefined SOP is refused");

    match &error {
        ProcessSeedError::UnknownSop {
            file,
            process,
            step,
            sop,
        } => {
            assert_eq!(file, "types/broken_type.yaml");
            assert_eq!(process, "broken_type.ready0");
            assert_eq!(*step, 1);
            assert_eq!(sop, "broken_type.missing");
        }
        other => panic!("expected UnknownSop, got {other:?}"),
    }
    assert!(
        error.to_string().contains("broken_type.missing"),
        "the message names the missing SOP: {error}"
    );
}

#[test]
fn a_duplicate_list_stage_is_refused() {
    let yaml = four_processes("broken_type.system", true);
    let error = process_seed::parse_type_file("types/broken_type.yaml", &yaml)
        .expect_err("two processes for one list stage are refused");

    match &error {
        ProcessSeedError::DuplicateList {
            file,
            process,
            list,
        } => {
            assert_eq!(file, "types/broken_type.yaml");
            assert_eq!(process, "broken_type.ready1", "the second process is named");
            assert_eq!(list, "ready");
        }
        other => panic!("expected DuplicateList, got {other:?}"),
    }
}

/// The loader applies the same step-shape rule `validate_publish` applies, so the catalogue can
/// never carry a shape the executor would mis-dispatch (2026-11-10: the seed wrote an action on
/// `agent` steps that publish refused, which made every catalogue process un-republishable).
#[test]
fn a_step_whose_kind_and_fields_disagree_is_refused_at_parse_time() {
    let cases = [
        // A computed step with no action: the executor would invoke the empty string.
        (
            "    steps:\n      - {position: 1, kind: computed, actor: system, \
             sop: broken_type.system}\n",
            "a computed step must name its action",
        ),
        // The agent primitive on a computed step is a second spelling for `agent`.
        (
            "    steps:\n      - {position: 1, kind: computed, actor: tool_agent, \
             action: agent.run, sop: broken_type.system}\n",
            "is the agent step's primitive",
        ),
        // An agent step naming some other action.
        (
            "    steps:\n      - {position: 1, kind: agent, actor: process_agent, \
             action: cards.move, process_agent: broken_type, sop: broken_type.system}\n",
            "an agent step runs `agent.run`",
        ),
        // A gate step with no gate code.
        (
            "    steps:\n      - {position: 1, kind: gate, actor: system, \
             sop: broken_type.system}\n",
            "a gate step must name its gate_kind",
        ),
        // A call step with no child.
        (
            "    steps:\n      - {position: 1, kind: call, actor: system, \
             sop: broken_type.system}\n",
            "a call step must name its child_process_key",
        ),
    ];
    // A type file needs all four list stages, so the malformed one is the first stage and the
    // other three carry a valid computed step — otherwise the stage check fires first.
    fn stage(key: &str, list: &str, steps: &str) -> String {
        format!("  - key: {key}\n    list: {list}\n    title: {list}\n{steps}")
    }
    const VALID: &str = "    steps:\n      - {position: 1, kind: computed, actor: system, \
                         action: cards.move, sop: broken_type.system}\n";
    for (steps, expected) in cases {
        let yaml = format!(
            "card_type: broken_type\n\
             capability: development\n\
             agent: {{key: broken_type, model: claude-sonnet-5-5}}\n\
             sops:\n  - {{key: broken_type.system, title: System}}\n\
             processes:\n{}{}{}{}",
            stage("broken_type.READY", "ready", steps),
            stage("broken_type.DOING", "doing", VALID),
            stage("broken_type.QA", "qa", VALID),
            stage("broken_type.DONE", "done", VALID),
        );
        let error = match process_seed::parse_type_file("types/broken_type.yaml", &yaml) {
            Ok(_) => panic!("the case must be refused: {steps:?}"),
            Err(error) => error,
        };
        match &error {
            ProcessSeedError::BadStepShape { detail, step, .. } => {
                assert_eq!(*step, 1);
                assert!(
                    detail.contains(expected),
                    "{detail:?} should mention {expected:?}"
                );
            }
            other => panic!("expected BadStepShape, got {other:?}"),
        }
    }
}

/// The generated `manifest.yaml` is the reviewed index of the 280 — and nothing else asserts it.
/// A hand-added or hand-deleted entry, or a file renamed without regenerating, would leave the
/// index lying about what the loader will apply.
#[test]
fn the_manifest_matches_the_type_files_exactly() {
    #[derive(serde::Deserialize)]
    struct Manifest {
        version: i32,
        processes: Vec<Entry>,
    }
    #[derive(serde::Deserialize, PartialEq, Eq, Debug)]
    struct Entry {
        key: String,
        file: String,
        list: String,
    }

    let raw = include_str!("../../../seed/processes/manifest.yaml");
    let manifest: Manifest = serde_yaml::from_str(raw).expect("the manifest parses");
    assert_eq!(manifest.version, 1);

    let catalogue = process_seed::catalogue().expect("the embedded catalogue validates");
    let mut expected: Vec<Entry> = catalogue
        .types
        .iter()
        .flat_map(|file| {
            file.processes.iter().map(|process| Entry {
                key: process.key.clone(),
                file: format!("types/{}.yaml", file.card_type),
                list: process.list.clone().unwrap_or_default(),
            })
        })
        .collect();
    expected.sort_by(|left, right| left.key.cmp(&right.key));

    assert_eq!(
        expected.len(),
        280,
        "four processes for each of the 70 types"
    );
    assert_eq!(manifest.processes, expected, "the manifest is stale");
}
