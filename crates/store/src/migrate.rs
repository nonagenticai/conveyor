//! Migrations and the role assertions around them.
//!
//! ⚠️ The path is `sqlx::migrate!("../../migrations")` **on purpose**: the macro resolves
//! against the *defining crate's* manifest directory (`crates/store`), so a bare
//! `"./migrations"` would silently mean `crates/store/migrations` — an empty embedded set
//! and a server that starts against a schema it never applied. The unit test below pins the
//! embedded set to the migrations that exist:
//! `[(1, "platform"), (2, "domain"), (3, "action registry"), (4, "board"),
//!   (5, "control plane"), (7, "requirements"), (8, "s08 portfolio"), (9, "s08 models"),
//!   (10, "s08 retention")]`. Version 6 is reserved for S06a, which ships no migration of
//! its own.
//!
//! `migrate` is the only entry point: it asserts the runtime role exists and is
//! unprivileged (the agent pattern's `DO` block swallowed `insufficient_privilege`, so a
//! migration could pass with no runtime role and every GRANT was a no-op), runs the
//! embedded migrations, then re-applies the runtime grants.
//!
//! ⚠️ **Every migration must be expand/contract** — backward-compatible with the immediately
//! prior release. The runtime migrator ([`runtime_migrator`]) tolerates applied versions the
//! binary does not embed, so an application-only rollback can bring the prior release up
//! against the newer schema (issue #669). That tolerance is only sound under the
//! expand/contract obligation: a non-additive migration (a rename, a drop, a narrowing type
//! change) forbids an application-only rollback and requires a restore-based rollback through
//! the gated ops path. See `migrations/README.md`.

use sqlx::PgPool;
use sqlx::migrate::{MigrateError, Migrator};

/// The embedded migrations, from the workspace root `migrations/`.
pub static MIGRATOR: Migrator = sqlx::migrate!("../../migrations");

/// The migrator [`migrate`] runs: the same embedded set, with sqlx's tolerance for applied
/// versions this binary does not embed switched on.
///
/// Without it, `sqlx` fails closed with `VersionMissing(version)` when the database carries a
/// migration a newer release applied (sqlx-core 0.9's `validate_applied_migrations`). An
/// application-only rollback — the prior image started against the newer schema — would then
/// never reach Ready (issue #669). With it, the older release starts and ignores *newer*
/// applied versions.
///
/// This does **not** weaken checksum validation: a version this binary does embed whose
/// checksum changed is still refused with `VersionMismatch` (`sqlx` checks that in its apply
/// loop, which `ignore_missing` does not skip). It is only safe because every migration is
/// expand/contract (module docs and `migrations/README.md`).
pub fn runtime_migrator() -> Migrator {
    let mut migrator = Migrator::with_migrations(MIGRATOR.iter().cloned().collect());
    migrator.set_ignore_missing(true);
    migrator
}

/// The role every request runs as. Created by `scripts/db-roles.sql`, never by a migration.
pub const RUNTIME_ROLE: &str = "conveyor_runtime";

/// `scripts/grants-runtime.sql`, re-applied after every migration run.
pub const RUNTIME_GRANTS_SQL: &str = include_str!("../../../scripts/grants-runtime.sql");

/// Failures of the `migrate` path.
#[derive(Debug, thiserror::Error)]
pub enum StoreMigrateError {
    /// The runtime role does not exist, so every grant would be a no-op.
    #[error(
        "role {0} does not exist: run scripts/db-roles.sql as a superuser before migrating \
         (a migration must not create it — that is how a swallowed error hides)"
    )]
    RuntimeRoleMissing(String),
    /// The runtime role is more powerful than the isolation model allows.
    #[error(
        "role {0} must be NOSUPERUSER NOBYPASSRLS: a superuser or BYPASSRLS role reads every \
         tenant and FORCE ROW LEVEL SECURITY cannot stop it"
    )]
    RuntimeRolePrivileged(String),
    /// `pg_roles` could not be read.
    #[error("reading pg_roles: {0}")]
    Database(#[from] sqlx::Error),
    /// A migration failed.
    #[error("applying migrations: {0}")]
    Migrate(#[from] MigrateError),
}

/// Assert `role` exists with `rolsuper = false AND rolbypassrls = false`.
pub async fn assert_runtime_role(pool: &PgPool, role: &str) -> Result<(), StoreMigrateError> {
    let attrs: Option<(bool, bool)> =
        sqlx::query_as("SELECT rolsuper, rolbypassrls FROM pg_roles WHERE rolname = $1")
            .bind(role)
            .fetch_optional(pool)
            .await?;
    match attrs {
        None => Err(StoreMigrateError::RuntimeRoleMissing(role.to_owned())),
        Some((false, false)) => Ok(()),
        Some(_) => Err(StoreMigrateError::RuntimeRolePrivileged(role.to_owned())),
    }
}

/// Re-apply the runtime grants. A table added by a later migration is otherwise invisible
/// to `conveyor_runtime`. Idempotent, and the GRANT-before-REVOKE order inside the script is
/// what keeps `audit_event`, `event` and `action_invocation` append-only and `tenant` /
/// `app_user` undeletable.
pub async fn apply_runtime_grants(pool: &PgPool) -> Result<(), StoreMigrateError> {
    sqlx::raw_sql(RUNTIME_GRANTS_SQL).execute(pool).await?;
    Ok(())
}

/// The one migration entry point: assert the runtime role, apply the embedded migrations
/// (tolerating newer applied versions — see [`runtime_migrator`]), re-apply the grants. Never
/// called implicitly by `serve` (CONTRACT §10).
pub async fn migrate(pool: &PgPool) -> Result<(), StoreMigrateError> {
    assert_runtime_role(pool, RUNTIME_ROLE).await?;
    runtime_migrator().run(pool).await?;
    apply_runtime_grants(pool).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_runtime_migrator_embeds_the_same_set_and_tolerates_newer_applied_versions() {
        let embedded: Vec<(i64, String)> = MIGRATOR
            .iter()
            .map(|m| (m.version, m.description.to_string()))
            .collect();
        let runtime = runtime_migrator();
        let runtime_set: Vec<(i64, String)> = runtime
            .iter()
            .map(|m| (m.version, m.description.to_string()))
            .collect();
        assert_eq!(
            runtime_set, embedded,
            "the runtime migrator must carry the same embedded set as MIGRATOR; a second \
             sqlx::migrate! path is how the two drift"
        );
        assert!(
            runtime.ignore_missing,
            "the runtime migrator must tolerate applied versions a newer release added, or an \
             application-only rollback dies on VersionMissing (issue #669)"
        );
        assert!(
            !MIGRATOR.ignore_missing,
            "the control used by the store integration test keeps sqlx's fail-closed default"
        );
    }

    #[test]
    fn embedded_migrations_are_exactly_the_s0_set() {
        let embedded: Vec<(i64, String)> = MIGRATOR
            .iter()
            .map(|m| (m.version, m.description.to_string()))
            .collect();
        assert_eq!(
            embedded,
            vec![
                (1, "platform".to_owned()),
                (2, "domain".to_owned()),
                // sqlx renders the file name's underscore as a space in the description.
                (3, "action registry".to_owned()),
                (4, "board".to_owned()),
                (5, "control plane".to_owned()),
                // The gap at 6 is deliberate: 0006 is reserved for S06a, which ships no
                // migration of its own (the signed channel reuses `approval.callback_url`).
                (7, "requirements".to_owned()),
                (8, "s08 portfolio".to_owned()),
                (9, "s08 models".to_owned()),
                (10, "s08 retention".to_owned()),
                // S00 day 2's table verdicts, executed: seven tables the inventory measured as unused.
                (11, "drop unused tables".to_owned()),
                // The FKs and indexes the three orphan retention rules needed.
                (12, "retention chain".to_owned()),
                // S09: document, secret, repository, project_repository.
                (13, "documents".to_owned()),
                // S09 Lane A: the webhook signing secret's key id (sealed under the tenant KEK).
                (14, "webhook secret seal".to_owned()),
                // S11: procedure, procedure_step, run_step, and `run.procedure_id`/`run.inputs`.
                (15, "s11 procedure".to_owned()),
                // One draft per (tenant, key) — the draft twin of 0015's published index.
                (16, "procedure draft".to_owned()),
                // `procedure.lock_version`: If-Match needs a counter that changes on edit.
                (17, "procedure lock version".to_owned()),
                // S10: conversation, conversation_turn, and one pending proposal per turn.
                (18, "copilot".to_owned()),
                // S13: `workflow`, `run.workflow_id` and the four `run_step` trace columns.
                (19, "s13 workflow".to_owned()),
                // S15: the capability aggregate, its executor config, knowledge binding, relations
                // and hot work-package memory; the 17 catalogue capabilities move here.
                (20, "capability".to_owned()),
                // S11's D22 revisit: the two forward-reference `run` columns; 0020 adds their FKs.
                (21, "s11 run capability".to_owned()),
                // S11: `kata_executor`, the synthetic tenant-admin membership role (CCR 5).
                (22, "s11 kata executor role".to_owned()),
                // S06c: the card-less live project-execution index and the approval project index.
                (23, "s06c mcp tasks".to_owned()),
                // S16: the inference ledger, the prompt audit and its findings.
                (24, "s16 inference audit".to_owned()),
                // S17: the four benchmark tables (SPEC D24). 0024 stays S16's.
                (25, "s17 benchmark".to_owned()),
                // S19: the eval instrument (SPEC D25) and S20's climb state (D26), plus the
                // `approval.eval_id` sign-off column and its partial unique index (CCR 5,
                // approved by the owner 2026-11-09).
                (26, "s19 eval".to_owned()),
                // S15: the FKs 0021 could not declare (0021 runs after 0020 on a fresh database).
                (27, "run capability fks".to_owned()),
                // RBAC: the tenant role policy, invites, platform admins, and the retirement of
                // the `owner` membership role (George 2026-11-09).
                (28, "rbac provisioning".to_owned()),
                // S20: the climb's final report. `eval_climb` itself shipped in 0026; this is the
                // one column the loop state cannot derive (CONTRACT-CHANGE REQUEST, 2026-11-09).
                (29, "s20 eval climb report".to_owned()),
                // S13 (D27): the prompt catalogue — `prompt` + `prompt_section`, one active
                // version per (tenant, key). Was missing from this list while the file was
                // already embedded; the same lane updated `s07_schema`, not this assertion.
                (30, "prompt catalogue".to_owned()),
                // S21 step 1: the per-section/per-prompt character budget and the `role` source
                // kind (slice S21 §4.4, §3.2).
                (31, "s21 prompt budget".to_owned()),
                // S21 step 2: the declared step outputs the plan projection validates (slice §6).
                (32, "s21 plan outputs".to_owned()),
                // D23's amendment (S19 §9): the `eval` caller kind and `inference_call.eval_run_id`,
                // so an eval's own model calls are attributable to the eval run.
                (33, "d23 eval caller".to_owned()),
                // S18 steps 1/3/4: the subject version a run measured, the produced timestamp on a
                // case, and the improvement verdict on the candidate run.
                (34, "s18 kata arm".to_owned()),
                // S13 (process model): procedure→process, the SOP `procedure` table, `process_map`,
                // the run tree, step kinds/actors, capability kinds/agent roles, `run_step.judge`
                // and the bus cursor (0035_process_tree.sql).
                (35, "process tree".to_owned()),
                // S19 (owner ruling 2026-11-09): the one-shot per-output grade the SOP's
                // `inspection.kind='eval'` uses (0036_s19_output_grade.sql).
                (36, "s19 output grade".to_owned()),
                // S13 (W5): the run tree's workflow step — a workflow parent's procedure node
                // starts its child from a `run_step`, not a `process_step`
                // (0037_run_tree_workflow_step.sql).
                (37, "run tree workflow step".to_owned()),
                // S06d: open connector kinds, per-channel action grants, the multicast dispatch
                // ledger, and the `service` decider kind (0038_s06d_connector_standard.sql).
                (38, "s06d connector standard".to_owned()),
                // #70: the text-output columns on `document` — `body`, `card_id`, `publishable`
                // and their two indexes (0039_document_outputs.sql).
                (39, "document outputs".to_owned()),
                // T0: the organisation spine — industry and the department placement, job
                // catalogue, immutable release, binding, the card/run pins, and the
                // decision-ladder tables (0040_organisation.sql, SPEC_V2 §4, §10.3, §12 T0).
                (40, "organisation".to_owned()),
                // #144 (ADR 0007 §Decision 3–4): `runner_run` — one row per dispatched attempt —
                // and `runner_artefact`, the manifest a polled result is checked against.
                (41, "runner".to_owned()),
                // #156: the work store — provenance, the publish request, promoted
                // `category`/`unit`, and the attachment kind on `document`
                // (0042_document_workflow.sql).
                (42, "document workflow".to_owned()),
                // #160: the tenant's registered inference destination
                // (0043_tenant_inference_endpoint.sql).
                (43, "tenant inference endpoint".to_owned()),
                // #72 (T1): the plan spine rows and their actions (0044_plan_spine.sql).
                (44, "plan spine".to_owned()),
                // #46: the commercial-equivalent price per model, the price side of the
                // read-time cost derivation (0045_commercial_price.sql).
                (45, "commercial price".to_owned()),
                // #73 (T2): the decision ladder's rungs, answer, option-set version, the three
                // version pins, the actor and the approval link (0046_decision_ladder.sql).
                (46, "decision ladder".to_owned()),
                // #265: the non-token surfaces' unit and per-unit rate on the catalogue, and the
                // call's own usage_unit/usage_units on the ledger (0047_media_price.sql).
                (47, "media price".to_owned()),
                // #172 (T7): the hosts conveyor may reach, and what each may be asked for
                // (0048_host_registry.sql, SPEC_V2 §10.4.1). A platform fact — no tenant column.
                (48, "host registry".to_owned()),
                (49, "pm job".to_owned()),
                (50, "approval policies".to_owned()),
                // #305: the durable tranche-review record — the decision and the evidence a
                // review was taken on (0051_tranche_review.sql).
                (51, "tranche review".to_owned()),
                // #303: the governed retirement path for benefit profiles and tranches
                // (0052_benefit_retire.sql).
                (52, "benefit retire".to_owned()),
                // #295 (ADR 0008): the per-card append-only claim/fact ledger
                // (0053_card_ledger.sql).
                (53, "card ledger".to_owned()),
                // #81 (T9): the eleven TOGAF ADM tables (0054_architecture.sql).
                (54, "architecture".to_owned()),
                // #76 (T13): the tool platform — toolkit, tool, tool_binding and the append-only
                // tool_call ledger (0055_tool_platform.sql, SPEC_V2 §11.8).
                (55, "tool platform".to_owned()),
                // #82 (T10): the lesson, the demo credit and the tranche authorisation record
                // (0056_cadence_tranche.sql).
                (56, "cadence tranche".to_owned()),
                // #75 (T4): the kanban coach — the interruption record with its per-signature
                // open index, and the append-only remedy (0057_coach.sql, SPEC_V2 §6.2).
                (57, "coach".to_owned()),
                // #413: SPEC §10.3's PDDL action text on procedure and process_step (the table
                // 0035 renamed from procedure_step), and the card's WBS node pin.
                (58, "data model gaps".to_owned()),
                // T11 (#84): the versioned C0–C5 threshold configuration and the billing
                // reconciliation record (0059_metrics.sql, SPEC_V2 §7.5, §15.1 D37/D50/D52).
                (59, "metrics".to_owned()),
                // T16 (#85): the derived SOP/process provenance, the executable PDDL state and
                // the publication approval (0060_procedure_ingestion.sql, SPEC_V2 §5.6; the PKO
                // tables are deliberately absent — owner decision 2026-11-19, knowledge in strata).
                (60, "procedure ingestion".to_owned()),
                // #508: the contract's requester, recorded at request time and never rewritten
                // (0061_contract_requester.sql, SPEC_V2 §8.1 — the approver must differ).
                (61, "contract requester".to_owned()),
                // S21 step 3 (#15): the draft recipe lifecycle, the section selector and the
                // memory source kind (0062_s21_prompt_selection.sql, ADR 0009).
                (62, "s21 prompt selection".to_owned()),
                // #579: the async invocation record, so a caller behind the CDN's ~100 s cap
                // can submit and poll instead of holding one response open
                // (0063_async_invocation.sql). Adopted from main unchanged: production has
                // applied this checksum, so its bytes never change.
                (63, "async invocation".to_owned()),
                // T17 (#87): the tenant asset authoring intake, drafts, review gate and the
                // one-published-row-per-(kind, key) catalogue (0064_authoring.sql, SPEC_V2 §5.7).
                (64, "authoring".to_owned()),
                // T19 (#606): the decision forum — meeting types, meetings, agenda items,
                // attendance, positions and the participant extension (0065_meetings.sql,
                // SPEC_V2 §6.8, D66).
                (65, "meetings".to_owned()),
                // T5 (#77): the kata's record, its six Gets and its held-out measurement
                // (0066_kata.sql, SPEC_V2 §6.3, §9.3–§9.4).
                (66, "kata".to_owned()),
                // T5 P2–P4 (#77): the open-time measurability gate, the pinned pre-kata
                // procedure version, and the append-only revert and adoption evidence
                // (0067_kata_gate.sql).
                (67, "kata gate".to_owned()),
                // T6: the development charter, its immutable step evidence, the closure record,
                // the one-pending development gate index and the approval-policy auto-merge terms.
                (68, "development project".to_owned()),
                // T8 (#79): the lesson lifecycle — the independent-evidence CHECK, the proposer/
                // reviewer record and the governed-asset source draft promotion
                // (0069_lesson_lifecycle.sql, SPEC_V2 §7.6, §10.3, §10.4, §12 T8, D36).
                (69, "lesson lifecycle".to_owned()),
                // T15 (#86): the tool_draft governed-asset kind, the registered tool sources, the
                // binding-scoped subset, the ledger's binding pin, the promotion record and the
                // publication quorum (0070_tool_authoring.sql, SPEC_V2 §11.3–§11.6, D42/D43/D50).
                (70, "tool authoring".to_owned()),
                // ── The lane's reserved block (0080+), after main's 0066–0070. Versions
                // 0071–0079 stay free for main; the block's SQL is unchanged, only its version
                // moved. The kata union (the lane's candidate/propagation records and
                // pre-registration fields on main's 0066/0067) sits at 94, after the block,
                // because main took 70.
                // v1-w1a-2 (#526): the release's layered kind store — base → industry →
                // department archetype → department → job — and `department.archetype`
                // (0080_release_layers.sql, SPEC_V2 §4.1).
                (80, "release layers".to_owned()),
                // v1-w4a-2 (#526): the write tool's declared verification path on the version
                // it was published with (0081_tool_verify.sql, SPEC_V2 §11.3).
                (81, "tool verify".to_owned()),
                // v1-p82-2 (#526): `programme_cadence.state` becomes a closed set
                // (0082_cadence_state.sql, SPEC_V2 §8.2).
                (82, "cadence state".to_owned()),
                // v1-p5-3 (#526): a change's impact block is an object carrying at least one of
                // scope/schedule/cost/risk (0083_change_impact_keys.sql, SPEC_V2 §5.5).
                (83, "change impact keys".to_owned()),
                // v1-p82-3 (#526): `programme.shared_config` — the programme's shared values and
                // the keys it locks against per-project override
                // (0085_programme_shared_config.sql, SPEC_V2 §8.2).
                (85, "programme shared config".to_owned()),
                // v1-w1c-3 (#526): an L1 decision names the verified `laya.classify`
                // invocation behind its answer and the request/response hashes it was
                // verified against (0086_ladder_laya_provenance.sql, SPEC_V2 §7.2).
                (86, "ladder laya provenance".to_owned()),
                // v1-w1c-4 (#526): the authored option set, its per-option tenant
                // refinement and the decision's set pins — options are rows, never a
                // caller's list (0087_ladder_option_sets.sql, SPEC_V2 §7.2).
                (87, "ladder option sets".to_owned()),
                // (0088): a register row belongs to a project OR a programme, and carries its
                // next review date and response (0088_programme_risk_register.sql, SPEC_V2
                // §8.3–§8.4).
                (88, "programme risk register".to_owned()),
                // (0089): the baseline/interim/post readings a benefit's evaluation reads, and
                // the seven programme strategy/plan asset kinds
                // (0089_benefit_measurement.sql, SPEC_V2 §8.3).
                (89, "benefit measurement".to_owned()),
                // SPEC_V2 §11.3 (T13, v1-w4a-5): a call that exceeds the tool's own
                // `timeout_secs` is `pending`; the resume token travels with the pending row
                // (0090_tool_call_resume.sql).
                (90, "tool call resume".to_owned()),
                // SPEC_V2 §6.2 (T4, v1-p6-5): an interruption records which input opened it —
                // a failed run, a WIP breach, a stale lease, a gate timeout or a board event
                // (0091_coach_interruption_trigger.sql).
                (91, "coach interruption trigger".to_owned()),
                // (0092_benefit_measurement_typed.sql): the typed baseline/interim/post readings
                // a benefit's evaluation reads.
                (92, "benefit measurement typed".to_owned()),
                // (0093_approval_policy_approver_kind.sql): who may decide a gate, as tenant data.
                (93, "approval policy approver kind".to_owned()),
                // #526 (the kata union): the lane's candidate and propagation records on main's
                // kata loop — the columns the coach/ops/architecture `kata_candidate_id`s need —
                // and the pre-registration fields (0094_kata_union.sql).
                (94, "kata union".to_owned()),
                // #526 (v1-w1b-5): the tenant's meta-role × action authority matrix; an absent
                // row is the compiled §6.7 default (0095_job_authority.sql).
                (95, "job authority".to_owned()),
                // #526 (v1-w1a-5): the run kind, and the CHECK that a job run carries its binding
                // and release pin (0096_run_kind.sql).
                (96, "run kind".to_owned()),
                // #526 (v1-p5-5): the work-package card CHECK and the job-run card CHECK
                // (0097_work_package_card.sql).
                (97, "work package card".to_owned()),
                // #526 (v1-p55-1): the declared register links shape and its default
                // (0098_register_links_shape.sql).
                (98, "register links shape".to_owned()),
                // #526 (v1-p101-1): the decision row's run/step links (0099_step_decisions.sql).
                (99, "step decisions".to_owned()),
                // #526 (v1-p102-1): the laya config default and its hard-budget check
                // (0100_laya_timeout_default.sql).
                (100, "laya timeout default".to_owned())
            ],
            "the embedded migration set is {embedded:?}; a wrong sqlx::migrate! path or a \
             file outside the set changes it"
        );
    }

    #[test]
    fn runtime_grants_grant_before_revoking_the_immutable_and_anchor_rows() {
        let sql = RUNTIME_GRANTS_SQL;
        let grant = sql
            .find("GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES")
            .expect("the schema-wide GRANT is what makes a later table visible");
        let revoke = sql
            .find("REVOKE UPDATE, DELETE ON audit_event")
            .expect("audit_event must be revoked after the grant");
        assert!(
            grant < revoke,
            "the schema-wide GRANT must come BEFORE the audit_event revoke, or the grant \
             re-grants UPDATE/DELETE and the append-only guarantee silently disappears"
        );
        for narrow in [
            "REVOKE UPDATE ON event FROM conveyor_runtime",
            "REVOKE UPDATE ON action_invocation FROM conveyor_runtime",
            "REVOKE DELETE ON tenant, app_user FROM conveyor_runtime",
        ] {
            let at = sql
                .find(narrow)
                .unwrap_or_else(|| panic!("{narrow:?} is missing from the runtime grants"));
            assert!(
                grant < at,
                "{narrow:?} must come AFTER the schema-wide GRANT, or the grant re-grants it"
            );
        }
        assert!(
            !sql.contains("REVOKE DELETE ON event"),
            "event.prune needs DELETE on event; only UPDATE is revoked"
        );
        assert!(
            sql.contains("REVOKE ALL ON _sqlx_migrations"),
            "the migration ledger is not application data"
        );
    }
}
