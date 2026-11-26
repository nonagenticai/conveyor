# Project brief — proof of delivery

- **Programme**: conveyor buildout · tranche 1 (authorised)
- **Plan**: `docs/plans/proof-of-delivery.md`
- **Units**: #744 (capability), #746 (corpus, certificates, live gate)
- **ADM**: phase G, capability plateau

## Outcome

A delivered slice or work package can be handed to a third party with a certificate that says what
was claimed, which facts it was checked against, and the exact Lean source that closed the claim —
and that party can re-run the check without trusting the conveyor.

## Why now

The Lean prover is already a workspace crate and a declared set of external actions
(`crates/actions/src/integrations/lean4.rs`). Gate evidence is already a closed, typed set
(`resolve_evidence`). The only missing pieces are the durable certificate and one evidence kind, so
the marginal cost is small and the evidence model stops being prose.

## Scope

In: the `proof` store, `proofs.*` actions, the `lean_proof` evidence kind, the Lean model and facts
renderer, programme certificates, package-manifest proofs, and the live gate on the programme
release.

Out: Rust-level verification, model-to-implementation refinement, prover-side attestation, and
widening the requirement to every card's Definition of Done before one real cycle of refusals.

## Benefits

Enabling for **B1 — Auditable decisions** (the certificate is the evidence a decision row cites) and
for **B6 — Predictable delivery** (the release gate reads a bundle rather than a checklist). No new
benefit instrument is introduced.

## Gates

| Gate | Condition |
|---|---|
| Capability (#744) | every scenario green on a stub prover at the PR head |
| Live (#746) | the deployed engine carries migration `0076`, the `conveyor` namespace reaches `lean4-prover:8812`, and the tamper drill refuses |

## Risks

- The prover allows two active lanes shared with other tenants; proof work is capped at one
  concurrent call and honours `Retry-After`.
- Kernel reduction over large literals can exceed the 120-second cap; the measurement step records
  the real numbers before anything depends on them, and `native_decide` is recorded as a different
  trust claim when used.
