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

use sqlx::PgPool;
use sqlx::migrate::{MigrateError, Migrator};

/// The embedded migrations, from the workspace root `migrations/`.
pub static MIGRATOR: Migrator = sqlx::migrate!("../../migrations");

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

/// The one migration entry point: assert the runtime role, apply the embedded migrations,
/// re-apply the grants. Never called implicitly by `serve` (CONTRACT §10).
pub async fn migrate(pool: &PgPool) -> Result<(), StoreMigrateError> {
    assert_runtime_role(pool, RUNTIME_ROLE).await?;
    MIGRATOR.run(pool).await?;
    apply_runtime_grants(pool).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
                (39, "document outputs".to_owned())
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
