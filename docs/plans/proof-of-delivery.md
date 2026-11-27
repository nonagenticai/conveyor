# Proof of delivery — Lean-checked certificates for the conveyor

Status: building · Issue: [#744](https://github.com/nonagenticai/conveyor/issues/744) (capability) ·
[#746](https://github.com/nonagenticai/conveyor/issues/746) (corpus, programme certificates, live
gate)

## Why

`docs/SPEC_V2.md` §5.3 already says a deliverable is accepted when an artefact or run meets its
criteria, and §5.4 makes evidence a closed set that a gate resolves before it opens. Neither says
what "the evidence is sufficient" means in a form a third party can re-check without trusting the
conveyor's own code.

The Lean prover (`apps/lean4-prover`) is a workspace crate with a synchronous API: `POST /api/check`
takes Lean source and answers `valid` with diagnostics. This plan turns that into a **certificate**:
a canonical statement of what was claimed, the facts it was checked against, the complete Lean
source that proves it, and the prover identity — stored so anyone can replay the check.

## What is proven, and what is not

Three grades, in increasing cost:

1. **Instance** — one delivery record's facts, serialised as a Lean literal, closed by `by decide`.
   Cheap, and it fails when a required item is missing.
2. **Universal** — theorems over a hand-written model of §5's rules, quantified over every state
   (`closed` is terminal, no self-transition, an accepting path carries evidence). This is the part
   a test cannot express.
3. **Bundle** — one theorem over the set of leaf certificate hashes, so a release is proven without
   re-proving every record in one source document.

**Not proven:** that the Rust implementation refines the model. Lean checks the model and the
record, not the binary. Every certificate therefore carries the assumption, the spec hash and the
prover identity, and the identity is **asserted by the conveyor from configuration**, not attested
by the prover. A certificate is *re-checkable*, not *signed*. Closing that gap needs prover-side
changes and is deliberately a later unit.

## The certificate

One row per certificate (`proof`), append-only:

| Field | Meaning |
|---|---|
| `subject_type`, `subject_id` | the card, run, WBS node, release, programme or package it is about |
| `spec_name`, `spec_sha256` | which specification the statement answers, and its digest |
| `facts_canonical`, `facts_sha256` | the canonical JSON of the facts, and its digest |
| `lean_source`, `theorem`, `trust` | the exact source checked, the theorem name, and whether reduction was kernel (`decide`) or native (`native_decide`) |
| `verdict`, `computation_time_ms` | what the prover answered and how long it took |
| `prover_identity` | toolchain, Mathlib revision and the prover build the check ran on |
| `idempotency_key`, `invocation_id` | the engine invocation whose persisted response this certificate records |
| `certificate_sha256` | the digest of the tuple above |

The binding between Rust and Lean is checkable without hashing inside Lean:

- `lean_source == render(facts_canonical)`, and
- `sha256(facts_canonical) == facts_sha256`.

A verifier re-renders from the stored facts and compares bytes; a mismatch is a tampered row, not a
different proof.

## How a certificate is produced

An action handler runs inside one transaction and may not speak HTTP
(`crates/api/tests/externals_declared.rs`), so the work is two steps, not one action:

0. a caller that is not this Rust process renders the facts through `proofs.render` first, so the
   source it sends next is the engine's own output (a caller that already runs in-process re-renders
   at record time, which is the same function);
1. a job step invokes the declared `lean4.check` external action — the engine owns the timeout,
   breaker, idempotency claim and audit row;
2. `proofs.record` names that call's idempotency key, reads the engine's persisted response, and
   writes the certificate in its own transaction.

A caller cannot supply a verdict; it can only name an invocation the engine already made.

## The actions

Five internal actions, all tenant-scoped; the prover calls stay the declared `lean4.*` externals
from T14.

| Action | Scope | What it refuses |
|---|---|---|
| `proofs.record` | `approvals:write` | an unknown, in-flight, expired or non-200 invocation; a body whose request hash differs from the stored one (the source is taken from that body, so a caller cannot substitute it); a theorem the source does not declare; `trust: kernel` on a source that reduces with `native_decide`; facts that are not JSON; a second certificate for one invocation |
| `proofs.reverify` | `approvals:read` | a row whose digest or facts no longer describe its columns; naming the certificate's own invocation (a replay is not a re-check); a fresh check whose source, verdict or recomputed digest differs from the stored ones |
| `proofs.get` | `approvals:read` | a missing id in this tenant |
| `proofs.list` | `approvals:read` | an unknown subject kind |
| `proofs.render` | `approvals:read` | a facts document that is not JSON; a model the binary does not embed; a kind the renderer does not know; facts the model refuses for that kind; a theorem name that is not a Lean identifier. It writes nothing and derives nothing: it is the renderer's public face, so a collector that is not this Rust process obtains the exact source `proofs.record` will require instead of re-implementing the renderer. |

`proofs.reverify` cannot call the prover — an action handler runs in one transaction and may not
speak HTTP — so re-verification is two steps like record: the caller runs a **fresh**
`lean4.check` on the stored source under a new key, then names both the certificate and that
invocation. The action is the judgement, and it writes nothing: the table is append-only, so a
certificate that re-verifies is the same certificate.

## Gate evidence

`lean_proof` joins `governed_asset` and `baseline` in `resolve_evidence`
(`crates/actions/src/pm/mod.rs`). A gate opened with `pinned_spec_sha256` accepts a proof only when
it is `proven`, on that spec hash, with a `certificate_sha256` that recomputes. Every refusal is a
typed `422`, so the gate can fail — which §5.4 requires, and a gate that never refuses is a defect.

## Units

| Unit | Scope | Acceptance |
|---|---|---|
| #744 | the store, `proofs.*` actions, the `lean_proof` evidence kind, the credential | provable against a stub prover |
| #746 | the model, the facts renderer, programme certificates, package manifests, the live gate | needs the deployed engine and a reachable prover |
| #836 | the unit model (`checkObs` / `checkLedger`, per-rule results), the `unit_ledger` renderer arm and subject, the collector and the evaluation of the real ledger | one chunk certificate over every unit in the window, with the failing units named by rule |
| #855 | the live certification evidence — the ledger's chunk certificates and the passing-subset control committed, the collector's public-edge signature, the 60-minute prover-token trap | two ledger chunks refused by the kernel, a proven control, one re-verification, every id in the evidence doc |
| #837 | engine-derived facts (`proofs.attest`, `checkRecord`), `proofs.record` refusing caller facts, the close gate | a real unit refused by the model, then the gate required |

## Out of scope

Rust-level verification (Verus/Kani); refinement of the model to the implementation; prover-side
attestation; and widening the requirement from the programme release gate to every card's
Definition of Done before one real cycle of refusals exists.
