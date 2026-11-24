#![forbid(unsafe_code)]
//! The runtime migrator's tolerance for applied versions it does not embed (issue #669).
//!
//! A database migrated by a newer release carries `_sqlx_migrations` versions the prior
//! release's embedded set does not contain. `sqlx`'s default fails closed with
//! `VersionMissing`; the runtime migrator switches `ignore_missing` on so an application-only
//! rollback can bring the prior release up. These tests pin both halves:
//!
//! * the embedded `MIGRATOR` (the control, `ignore_missing = false`) still fails with
//!   `VersionMissing` on a synthetic newer applied version;
//! * `migrate()` (the runtime migrator) succeeds on the same database;
//! * a *known* migration whose checksum changed is still `VersionMismatch` — the tolerance
//!   must not weaken integrity for the migrations the binary does embed.

#[path = "support/mod.rs"]
mod support;

use sqlx::Connection;
use sqlx::migrate::MigrateError;

/// The next version after the embedded set — a version a hypothetical newer release applied
/// and this binary does not carry.
fn synthetic_version() -> i64 {
    conveyor_store::MIGRATOR
        .iter()
        .map(|migration| migration.version)
        .max()
        .expect("the embedded migration set is not empty")
        + 1_000
}

#[tokio::test]
async fn the_runtime_migrator_tolerates_an_applied_version_the_binary_does_not_embed() {
    let db = support::TestDb::fresh().await;
    let synthetic = synthetic_version();
    sqlx::query(
        "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time)
         VALUES ($1, $2, true, $3, 0)",
    )
    .bind(synthetic)
    .bind("a newer release's migration, absent from this binary")
    .bind(vec![7_u8; 48])
    .execute(&db.super_pool)
    .await
    .expect("insert the synthetic applied version");

    // Control: the embedded static keeps sqlx's fail-closed default. A dedicated connection
    // avoids leaving the migration advisory lock on a pooled session when this fails.
    let mut control = sqlx::PgConnection::connect(&db.owner_url)
        .await
        .expect("connect the control session");
    let error = conveyor_store::MIGRATOR
        .run(&mut control)
        .await
        .expect_err("the default migrator must refuse an applied version it does not embed");
    control.close().await.expect("close the control session");
    match error {
        MigrateError::VersionMissing(version) => assert_eq!(
            version, synthetic,
            "the refusal names the newer applied version"
        ),
        other => panic!("expected VersionMissing({synthetic}), got {other:?}"),
    }

    // The runtime migrator — what `migrate` runs — tolerates it, so the prior release starts.
    conveyor_store::migrate(&db.owner)
        .await
        .expect("migrate() must tolerate an applied version newer than the embedded set");

    let still_applied: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM _sqlx_migrations WHERE version = $1 AND success)",
    )
    .bind(synthetic)
    .fetch_one(&db.super_pool)
    .await
    .expect("read the migration ledger");
    assert!(
        still_applied,
        "the tolerance ignores the newer version; it must not rewrite the ledger"
    );
}

#[tokio::test]
async fn a_known_migration_with_a_changed_checksum_is_still_refused() {
    let db = support::TestDb::fresh().await;
    // Version 1 is embedded; its checksum must match what this binary embedded. Corrupting it
    // simulates an edited applied migration — the tolerance must not mask that.
    sqlx::query("UPDATE _sqlx_migrations SET checksum = $1 WHERE version = 1")
        .bind(vec![9_u8; 48])
        .execute(&db.super_pool)
        .await
        .expect("corrupt the applied checksum");

    let error = conveyor_store::migrate(&db.owner)
        .await
        .expect_err("a changed checksum on a known migration must be refused");
    match error {
        conveyor_store::StoreMigrateError::Migrate(MigrateError::VersionMismatch(version)) => {
            assert_eq!(version, 1, "the refusal names the edited migration");
        }
        other => panic!("expected VersionMismatch(1), got {other:?}"),
    }
}
