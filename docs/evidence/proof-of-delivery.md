# Proof of delivery — the Lean certificate corpus

Part of #746. This document records what was actually measured, on which artefact, and what was
**not** run. It is a delivery record, not a claim of completeness: § Not proven lists the gap.

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

## Not proven — the deployment gap

Scenarios P6.1 and P6.2 of #746 are live and cannot be run yet, so this unit stops short of closing
them:

1. The deployed engine does not yet carry migration `0076` — it lands with the store unit's merge.
2. The deployed engine has no `CONVEYOR_EXTERNAL_LEAN4_URL`, so the `lean4.*` externals answer
   `503 action-service-unconfigured`; the deployment correction is a private infrastructure change.
3. No sealed `lean4.token` exists for the live tenant. Minting one is `conveyor admin
   upstream-token --service lean4 --mint`, which requires the deployment above.

Network reachability from the conveyor namespace to the prover is already in place, and the
release gate's required-evidence mechanism is proven by test. What remains is a deployment, not
code.
