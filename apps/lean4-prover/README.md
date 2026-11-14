# lean4-prover

Lean 4 formal verification as a REST API: typecheck, elaborate, and prove Lean 4
code. It shells out to `lake env lean` against a prebuilt Mathlib project — there
is no model, no GPU and no Apple-specific code.

**This crate lives in the conveyor workspace since 2026-11-14** (`docs/decisions/0006-tool-absorption-grouping.md`).
The crate moved; the deployable did not — the image, its 10.6 GB Lean/Mathlib layer and the node pin
stay with the deployment until #83 decides otherwise. The contract this crate must keep serving is
captured in `contract/` (`--check-lock` compares the served document against it; `tests/parity.rs`
replays the goldens).

**Rust since 2026-11-13.** The service ran in-cluster on hp1 before moving to a
launchd unit on mini1, and it has now left Apple hardware entirely: it is a
linux/amd64 container in the strato k3s cluster.

## API

Every compute call requires a licence token, introspected against the licences
service. There is no trusted-network bypass.

```
X-Tenant-Id: Bearer <licence-token>
```

(`Authorization: Bearer <token>` is also accepted. A bare `X-Tenant-Id` is never
identity.)

| Method | Path | Purpose |
|---|---|---|
| GET | `/health` | Lean binary reachable? 503 when not |
| GET | `/health/live` | Liveness, no dependencies |
| GET | `/health/ready` | Readiness, checks the Lean binary |
| GET | `/metrics` | Prometheus (`gpu_admission_*`, `lean4_prover_*`) |
| GET | `/openapi.json`, `/docs` | Schema, when `ENABLE_DOCS=true` |
| POST | `/api/check` | Typecheck Lean source |
| POST | `/api/elaborate` | Elaborate a term, return its type |
| POST | `/api/prove` | Try tactics in order until one closes the goal |

```bash
curl -s -X POST http://lean4-prover.lean4-prover.svc.cluster.local:8812/api/prove \
  -H 'Content-Type: application/json' \
  -H 'X-Tenant-Id: Bearer <licence-token>' \
  -d '{"theorem":"example : 2 + 2 = 4","imports":["Mathlib.Tactic"]}'
# → {"proven":true,"successful_tactic":"norm_num", ...}
```

Requests over a documented bound answer 422. A saturated `prove` lane answers
503 with `Retry-After`. An invalid or missing licence answers 401. An
unreachable licences service answers 503 — never an allow.

## Configuration

Environment variables only; production values live in
`gitops-production/clusters/strato/services/lean4-prover/configmap.yaml`.
`.env.example` documents every knob.

| Variable | Default | Notes |
|---|---|---|
| `ENABLE_AUTH` | `true` | Boot REFUSES to start when true and the URL below is empty |
| `LICENSE_INTROSPECT_URL` | — | `POST {"token": ...}`; 200 + `active` decides |
| `LEAN4_PROVER_BIND_PORT` | `8812` | |
| `LEAN4_PROVER_PROJECT_DIR` | `/lean-project` | The lake project |
| `LEAN4_PROVER_LEAN_BINARY` / `_LAKE_BINARY` | `/opt/elan/bin/lean`, `/opt/elan/bin/lake` | Absolute on purpose — the child env is replaced |
| `LEAN4_PROVER_TIMEOUT` | `120` | Seconds; clamped to 120 per request |
| `LEAN4_PROVER_MAX_MEMORY_MB` | `4096` | `RLIMIT_AS` on Linux |
| `GPU_LANE_PROVE_MAX_ACTIVE` / `_MAX_WAITING` / `_MAX_WAIT_S` | `2` / `8` / `30` | Shared by check/elaborate/prove |

## Local development

```bash
~/.claude/scripts/cargo-guard.py fmt
~/.claude/scripts/cargo-guard.py clippy --all-targets -- -D warnings
~/.claude/scripts/cargo-guard.py test
```

`scripts/pre-push.sh` runs those three and is wired into the pre-push hook. CI
is the fleet `rust-service.yml` archetype (`.github/workflows/ci.yaml`), which
builds and pushes the image — the image is the deployed artifact.

The unit tests need no Lean toolchain. `tests/http_contract.rs` drives the real
router and the real subprocess runner against a stand-in `lake`/`lean`, so the
timeout and process-group-kill paths are exercised without Mathlib.

## Image

Three stages: the Rust binary, a Lean stage that installs elan and pulls
Mathlib's prebuilt oleans for the revision pinned in `lean/lake-manifest.json`,
and a slim runtime carrying both.

`lake-manifest.json` is copied in deliberately — `lake update` would resolve
Mathlib HEAD at build time, making an unchanged repo produce a different image
tomorrow.
