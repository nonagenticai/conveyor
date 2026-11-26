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

1. a job step invokes the declared `lean4.check` external action — the engine owns the timeout,
   breaker, idempotency claim and audit row;
2. `proofs.record` names that call's idempotency key, reads the engine's persisted response, and
   writes the certificate in its own transaction.

A caller cannot supply a verdict; it can only name an invocation the engine already made.

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

## Out of scope

Rust-level verification (Verus/Kani); refinement of the model to the implementation; prover-side
attestation; and widening the requirement from the programme release gate to every card's
Definition of Done before one real cycle of refusals exists.
