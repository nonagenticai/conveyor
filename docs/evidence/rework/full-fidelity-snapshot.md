# The full-fidelity snapshot loop

**Why this exists.** Twice on 2026-11-21 the programme's acceptance evidence was taken against a
snapshot the *deployed* engine built, and that engine's projection is older than the branch's: it
omits `architecture_contract.requested_by`, `document.published_at` (the `outputs.js` index) and
the demo-credit rows. Three checks per manifest therefore reported failures that were artefacts of
the snapshot, not defects in the record — and one closure was paused on them.

The branch binary carries the newer projection, but it cannot read the live primary directly: the
branch is at migration `0065`, the live database at `0061`, and (measured 2026-11-22) dev Macs
cannot reach `postgres.internal:5432` at all — the tailnet ACL permits `:22` only. Restoring a copy
and migrating the copy closes both gaps without touching production and without merging anything.

**What it is not.** Not a second source of truth and not a write path. Records still go through the
product's actions (`programme-record.py`, or the deployed engine). The copy is read-only input to
`verify-project.py` and the strict audit.

## The loop

1. **Dump the primary.** On the control-plane host, as the `postgres` superuser (no password on
   disk, no credential in a shell history):

   ```sh
   ssh -i ~/.ssh/strato-vps admin@100.81.177.99 \
     "sudo -u postgres pg_dump -Fc -Z6 -d conveyor -f /tmp/conveyor-lane.dump"
   scp -i ~/.ssh/strato-vps admin@100.81.177.99:/tmp/conveyor-lane.dump /tmp/conveyor-lane.dump
   ```

   The database was 74 MB on 2026-11-22; the compressed dump is 7.4 MB.

2. **Restore into a throwaway `postgres:18`.** The dump carries RLS policies that name
   `conveyor_migrate`, `conveyor_runtime` and `process`, so the roles must exist **before**
   `pg_restore` or it stops on `CREATE POLICY`:

   ```sh
   docker run -d --name conveyor-restore -e POSTGRES_USER=postgres \
     -e POSTGRES_PASSWORD="$(openssl rand -hex 16)" -e POSTGRES_DB=conveyor \
     -p 127.0.0.1:55432:5432 postgres:18
   docker exec conveyor-restore psql -U postgres -q \
     -c "CREATE ROLE conveyor_migrate; CREATE ROLE conveyor_runtime; CREATE ROLE process;"
   docker exec -i conveyor-restore pg_restore -U postgres -d conveyor \
     --no-owner --no-privileges --exit-on-error < /tmp/conveyor-lane.dump
   ```

   `--no-owner` is required: the tables are owned by `conveyor_migrate` on the primary and that
   role is not a superuser here. `--exit-on-error` is required: `pg_restore` otherwise reports
   success after skipping the failed policies.

3. **Migrate the copy with the branch binary.** `conveyor migrate` takes the *session* advisory
   lock, so it must not go through PgBouncer:

   ```sh
   APP_ENV=test \
   DATABASE_MIGRATE_URL="postgresql://postgres:$PW@127.0.0.1:55432/conveyor" \
     target/claude/debug/conveyor migrate
   ```

   Verified 2026-11-22: `migrations applied … migrations 63` (0062–0064 applied over the restored
   `_sqlx_migrations` max 61). `0064`'s `programme_cadence_state_check` validated against the
   restored rows (`materialised` 3, `planned` 9) — if any row had violated it, the migration would
   have failed and that row would be the finding, never edited to fit.

4. **Build the snapshot from the copy.** `scripts/snapshot-live.sh` already opens a read-only
   transaction; only its URL changes:

   ```sh
   PROGRAMME_RECORD_BIN=target/claude/debug/conveyor \
   SNAPSHOT_DATABASE_URL="postgresql://postgres:$PW@127.0.0.1:55432/conveyor" \
   TENANT=nonagentic scripts/snapshot-live.sh /tmp/snap-full
   ```

   Verified 2026-11-22: 155 files, 140 platform outputs, `docs/outputs.js` carries 140 rows **with**
   `published_at`, `architecture.json` carries 22/23 contracts with `requested_by`, `msp.json`
   carries 29 demo credits and 10 lessons.

5. **Verify with the command clauses pointed at the copy.** Manifest command clauses call
   `programme-record.py`, which uses the *local binary* when `PROGRAMME_RECORD_LOCAL_BIN` is set —
   and the binary's admin path connects with `DATABASE_MIGRATE_URL`, not `DATABASE_URL` (the
   default `DATABASE_URL` points at the compose stack and times out). `admin action invoke` also
   takes `--input` as a **file path**, not inline JSON:

   ```sh
   export DATABASE_URL="$URL" DATABASE_MIGRATE_URL="$URL" APP_ENV=test
   export PROGRAMME_RECORD_LOCAL_BIN=target/claude/debug/conveyor
   python3 scripts/verify-project.py --all --snapshot /tmp/snap-full/data \
     --evidence-dir /tmp/evidence-full
   ```

   Keep `--evidence-dir` outside the repo until the verdict is accepted; a run against a stale copy
   must never overwrite committed evidence.

## What the first run proved

| Reading | Deployed-engine snapshot (23:21Z) | Restored + migrated copy (00:00Z) |
|---|---|---|
| S03 manifest | 5 failures (contract, review, closure gate, lessons, demo credit) | **1** (`lessons`) |
| T2 manifest | 4 failures (contract, review, lessons, demo credit) | **1** (`lessons`) |
| Contracts with `requested_by` | 0 | 22 of 23 |
| Published outputs with `published_at` | 0 | 140 |
| Demo credits | 0 | 29 |
| Programme-wide | — | 8 green / 39 red (47 manifests) |

The only tranche-2 defect the fidelity run leaves is the missing retro lesson — plus T2's
`non_conformant` review level, which is a record decision rather than a manifest check.

## The trap this loop exposed

`artefacts.json`'s `documents` array is **not** the publication projection: it selects only
`published_at IS NOT NULL AND status='active'` rows but does not expose `published_at` itself, so
counting `published_at` there reads as "0 published" on a healthy record. The publication evidence
is `docs/outputs.js` from the pack, which the verifier already reads. A measurement that looks at
the wrong file invents an outage.

## Refresh

Re-dump before each batch: the live record moves as the tranches close. The copy is disposable; the
restore SHA and the copy's `_sqlx_migrations` head belong in every evidence file that cites it.
