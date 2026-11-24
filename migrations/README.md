# Migrations

`sqlx` embeds every `*.sql` file here at compile time (`crates/store/src/migrate.rs`). The
server's `migrate` init container applies them before the application starts; the file name's
leading number is the version, and an applied migration is **never edited** — its checksum is
part of the ledger and `sqlx` refuses a changed checksum (`VersionMismatch`). Add a new file
instead.

## Expand/contract is mandatory

Every migration must be **expand/contract**: backward-compatible with the immediately prior
release. The runtime migrator (`crates/store/src/migrate.rs::runtime_migrator`) tolerates
applied versions the running binary does not embed, so an application-only rollback can start
the prior release against the newer schema (issue #669). That tolerance is only sound under
this obligation:

- **Allowed (expand):** new tables, new nullable columns or columns with defaults, new
  indexes, new constraints that old rows satisfy.
- **Forbidden in one step (contract):** renames, drops, narrowing type changes, and any change
  that makes the prior release's reads or writes fail. Do those in a later release, after the
  previous release no longer runs — split into an expand migration now and a contract migration
  once the old code is gone.

## Rollback across a migration

A deploy's rollback plan records `backward_compatible` (`crates/actions/src/development.rs`):

- `true` — the deploy's migrations are additive; an application-only rollback may serve the
  prior digest, and its evidence names **both digests and the applied migration generation**
  (`migration_generation`, the max applied version at deploy time).
- `false` — the deploy contains a non-additive migration; an application-only rollback is
  **refused** (`schema_restore_required`). The plan must name the `restore_path`: restoring the
  database to the matching schema generation through the gated ops path, with the restore
  recorded as the rollback evidence.

The `migrate` init container also re-applies `scripts/grants-runtime.sql` after every run; a
new table is invisible to `conveyor_runtime` until that happens, so never grant from inside a
migration.
