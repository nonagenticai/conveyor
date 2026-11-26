# Proof of delivery — the Lean certificate corpus

Part of #746, extended by #762. This document records what was actually measured, on which artefact, and what was
**not** run. It is a delivery record, not a claim of completeness: § Live on the released engine holds the drills
against the deployed engine, and the assumption no certificate removes.

## Artefacts

| Thing | Value |
|---|---|
| Model | `docs/spec/lean/DeliveryModel.lean`, 3 179 bytes, `sha256 d19f8ad3…baca504` |
| Specification pin | `docs/SPEC_V2.md`, `sha256 fc0fe31a…a99b99e2` |
| Toolchain | `leanprover/lean4:v4.30.0-rc2`, the prover application's own lake project |
| Reduction | kernel only — `by decide` and `simp`, no compiler-native reduction, no Mathlib import |

## How the checks were run

Two lanes, and they are deliberately different:

- **The corpus and the renderer's exact output** were checked by running `lake env lean <file>`
  inside the prover's own pod, against its own lake project, with the file streamed in and removed
  afterwards. That is the same command the prover's runner issues, so "valid" here means the same
  thing the API would answer — without going through the HTTP layer.
- **The engine path** (the declared `lean4.check` external, the sealed credential, the persisted
  invocation, `proofs.record` and `proofs.reverify`) is proven by integration tests against a real
  PostgreSQL 18 and a stub prover: `crates/actions/tests/proofs_record.rs`,
  `proofs_reverify.rs`, `pm_gate_proof.rs`.
- **The live drills** (§ Live on the released engine) run that same engine path against the deployed
  engine and the real prover, through the public API, with a real tenant bearer.

## Measurement — kernel checks on the exact artefacts

| Source | Bytes | Exit | Wall clock |
|---|---|---|---|
| The model alone (definitions + three universal theorems) | 3 179 | 0 | 10 242 ms (first run after the pod restarted; every later run is ~1.4 s) |
| Slice certificate — a closed slice with its evidence | — | 0 | 1 462 ms |
| Slice with one required evidence item removed | — | 1 | 1 399 ms |
| Model with an escape transition out of `closed` | — | 1 | 1 413 ms |
| Programme bundle, 20 leaves, final model | 5 143 | 0 | 3 107 ms |
| Programme bundle, one leaf dropped | — | 1 | 1 925 ms |
| Bundle with two slices sharing one certificate digest | — | 1 | — |

The renderer's own emission format was checked the same way, once per kind: a rendered `slice`, a
rendered `programme` bundle and a rendered `package` manifest each exit 0, including the
`checks := [("name", true)]` pair list. The negative runs are what stop the corpus being vacuous:
a model that is not a model, a slice missing its evidence, a bundle missing a leaf, and a bundle
that reuses one certificate for two slices are all refused by the kernel, not by us.

Earlier on the same day, through the prover's HTTP API with a licence token, the same shapes
measured: model 1 466 ms, slice 1 462 ms, package 1 598 ms, 20-leaf bundle 6 149 ms, and the same
four negatives refused. The HTTP answer agreed with the kernel answer in every case.

## What the certificate says, and what it does not

A certificate is an instance proof over canonical facts plus a universal proof over a hand-written
model. It proves that the facts, as recorded, satisfy `checkSlice` / `checkProgramme` /
`checkPackage`, and that a closed item is terminal with an accepted path always carrying evidence.

It does **not** prove that the engine's facts describe what actually happened in the database. That
refinement is assumed, and every certificate names the assumption, the specification digest and the
model digest. `README.md` in `docs/spec/lean/` states this in the same words a reader of a
certificate would need.

## The engine's refusals, measured by test

| Scenario | Refusal |
|---|---|
| A caller writes the Lean source instead of the renderer | `422 proof_source_not_rendered` |
| Facts that name a different model than the binary embeds | `422 proof_model_mismatch` |
| Facts and certificate pinning different specifications | `422 proof_spec_mismatch` |
| Facts missing a required item | `422 proof_facts_incomplete`, naming the item |
| A certificate reused for a different engine invocation | `422 proof_invocation_already_recorded` |
| A stored row rewritten and its digest recomputed | `422 proof_source_not_rendered` on re-verification |
| A gate that names `lean_proof` required, opened without one | `422 gate_evidence_missing`, naming the kind |
| A gate requiring a kind that is not typed | `422 gate_evidence_unknown_kind` |

## Live on the released engine

Measured 2026-11-26 against the deployed engine — the image carrying `0076`, #746 and
`CONVEYOR_EXTERNAL_LEAN4_URL` from the private infrastructure repository — with a real tenant
bearer from the application's own OIDC login. Every body below is the deployed engine's answer;
none of it is a test double.

### The credential

The tenant's prover bearer is a licence token minted on the tenant's **own `conveyor` licence** and
sealed with `conveyor admin upstream-token --tenant … --service lean4 --token-stdin`. The engine
reads it as the `lean4.token` secret and sends `Authorization: Bearer …`; the prover introspects it
and takes the tenant from the licence, never from a header. A dedicated `lean4-prover` licence app
does not exist, so the self-service mint (`--mint`) is tracked as #776.

### The measurements

| Step | Request | Deployed engine's answer |
|---|---|---|
| Prover reachability | `lean4.check`, `{"source": "theorem smoke_ok : 2 + 2 = 4 := by decide"}` | `200`; `{"valid": true, "computation_time_ms": 835.01, "diagnostics": [], "stderr": "", "stdout": ""}` |
| The renderer's own output | `lean4.check`, the slice certificate `render` emits from the facts below (3 537 characters) | `200`; `{"valid": true, "computation_time_ms": 1509.88}` |
| Negative control | `lean4.check`, `theorem nope : 1 = 2 := by decide` | `200`; `{"valid": false, "computation_time_ms": 836.15}`, the tactic's message in `stdout` |
| The certificate | `proofs.record` naming that invocation, the facts and `theorem delivery_ok`, `trust: kernel` | `201`; `{"verdict": "proven", "certificate_sha256": "31feb1ba…33e4648", "facts_sha256": "7c7daf3a…ddc4f0f", "computation_time_ms": 1509.88}` |
| The gate, without the certificate | `approvals.request`, `required_kinds: ["lean_proof"]`, `require_evidence: true`, `pinned_spec_sha256`, `evidence: []` | `422`; `"type": "gate_evidence_missing"`, `"detail": "this gate requires a \"lean_proof\" evidence item to open; the request names none"` |
| The gate, with it | the same request plus `evidence: [{"kind": "lean_proof", "id": "<certificate>"}]` | `200`; the approval's `evidence[0]` is `{"kind": "lean_proof", "id": "01a219b7…cfea658", "certificate_sha256": "31feb1ba…33e4648", "spec_sha256": "fc0fe31a…a99b99e2"}` |
| The tamper drill | the stored `facts_canonical` edited (one evidence item renamed) **with** `facts_sha256` and `certificate_sha256` recomputed, so the row is internally consistent; a fresh `lean4.check` of the unchanged source answers `valid: true`, then `proofs.reverify` on the rewritten row | `422`; `"type": "proof_source_not_rendered"`, `"detail": "proof_id: the stored source is not this rendering of the stored facts"` |
| The refusal is recorded | the engine's own record of the call above | `action_invocation` row `proofs.reverify | refused`, and an `audit_event` `{"outcome": "refused"}` at the same instant |

The facts the certificate carries name the model digest `d19f8ad3…baca504` and the specification pin
`fc0fe31a…a99b99e2`; `proofs.record` re-rendered the facts and refused nothing, which is what makes
the row's `lean_source` the renderer's output rather than a caller's claim.

An approval must belong to a project, card or run, so the two live gate calls name a project; the
earlier attempts on the same morning that omit it are input validation, not the gate.

### A prover defect this exposed

A check that fails returns `valid: false` with `diagnostics: []` and Lean's message in **`stdout`**:
`apps/lean4-prover/src/runner.rs` parses only `stderr` (three call sites). Certificates are
unaffected — `valid` comes from the exit code — but the diagnostic channel is empty for every
failure. Worth its own fix; it is not #746's.

### What this still does not prove

- That the facts in a certificate describe what actually happened in the database. That refinement
  is the assumption every certificate names (see `docs/spec/lean/README.md`); proving it is
  Verus/Kani work, not part of the corpus.
- The self-service mint path: the credential above was minted and sealed by an operator, and
  `--mint` still needs the dedicated client and the licence-app decision (#776).
