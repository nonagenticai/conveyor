# Live rollback across a migration boundary (2026-11-27, #669)

Issue #669. The rollback fix (#672, `a8c22f8e`) is proven live: a binary that embeds migrations
through 0080 ran against a database that carries 0081, became Ready, and served — both digests
and `/health` 200 at each step.

## The revisions

| step | image digest | `CONVEYOR_GIT_SHA` | migrations embedded | schema applied |
|---|---|---|---|---|
| before | `sha256:f13dea55…0171c` | `0096cc0da…` | 0081 | 81 |
| rolled back | `sha256:cbee6af8…4654` | `949e95c8c…` | 0080 | 81 |
| after | `sha256:f13dea55…0171c` | `0096cc0da…` | 0081 | 81 |

The rolled-back image is a post-fix build (`949e95c8c` descends from `a8c22f8e`); a pre-fix image
still fails closed by design — the tolerance is what the test exercises.

## The steps (UTC, 2026-11-27)

| time | step |
|---|---|
| 22:29:59 | image-update automation suspended; `spec.suspend` read back `true` |
| 22:30:31 | pin pushed (`5b39d1580b`): the five image lines — both Deployments — rewritten to the bare digest, the `$imagepolicy` setter removed so the automation cannot rewrite them |
| 22:30:44 | source and `services` kustomization reconciled to that revision |
| 22:31:24 | converged: two API pods and the worker Running and Ready on `cbee6af8…` |
| 22:31:27 | deployments recorded; both binaries report `949e95c8c…`; `/health` 200, `/health/ready` 200 |
| 22:32:0x | a database-backed action (`benchmark_suites.list`) invoked through the deployed CLI on the old binary returned its rows; `select max(version) from _sqlx_migrations` = 81 |
| 22:32:15 | revert pushed (`4733e66e39`); the image-update automation resumed (`spec.suspend` unset) |
| 22:32:44 | restored: all three pods on `f13dea55…`; both binaries report `0096cc0da…`; `/health` 200 |

## What the run proves

- **The runtime migrator tolerates a newer applied schema.** A 0080-max binary's `migrate` init
  container ran against schema 81, the pods became Ready, `/health` and `/health/ready` answered
  200, and a database-backed action served. Before #672 the same state failed in the init
  container (`VersionMissing`), so the prior digest never served at all.
- **The rollback is durable against the image automation** when the setter comment is removed, and
  the restore is a plain revert once the setter is back (the automation then owns the line again).
- **The service stayed available throughout**: replicas 2 with `maxUnavailable: 0`, worker 1, a
  PDB; the window from suspend to restored-and-ready was 2 m 45 s, and no step served fewer API
  pods than the rolling update allows.

## Two defects in the sanctioned rollback script (found en route)

`scripts/rollback-image.sh` (the deployment repository) could not be used as written:

- it matches the setter as `"$imagepolicy": "flux-system:<name>"` — with a space after the colon —
  while the conveyor manifest carries `{"$imagepolicy":"flux-system:conveyor"}` without one, so
  the script finds no target. The spaced form is the tree's majority (60 lines); conveyor's five
  lines are the unspaced form written by the image automation.
- its registry probe uses HTTP Basic auth; GHCR answers 401 (it requires the Bearer token
  exchange), so the script refuses any `ghcr.io` image before it reaches the rewrite.

This run followed the script's procedure by hand — suspend, bare-digest pin with the setter
removed, reconcile, verify, re-enrol — and the same pin and revert are the record above. The two
defects are reported here, not fixed in this unit.

## Scope

No application code changed. The only shared-state changes were the temporary pin and its revert
in the deployment repository (both named above), plus the suspension and resumption of the
image-update automation.
