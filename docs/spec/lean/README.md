# Lean delivery certificates

A delivered slice, a delivered work package and the programme release each carry a **certificate**:
a Lean source that a third party can re-check from the facts alone. The prover is a compute
backend, not an oracle — it answers "this source is valid", so the value is in *what the source
says*, and that is a small, hand-written model plus the facts rendered into it.

## The three grades

| Grade | What is proven | Where it lives |
|---|---|---|
| **Instance** | one record's facts satisfy the model (`checkSlice facts = true`, closed `by decide`) | rendered per record by `crates/actions/src/proof/render.rs` |
| **Universal** | the model's own properties, for every state (`closed_is_terminal`, `no_self_transition`, `accepted_path_carries_evidence`) | `DeliveryModel.lean`, proven once |
| **Bundle** | a release's leaf certificate digests are exactly the expected set, with no duplicate and none missing | rendered by `crates/actions/src/proof/programme.rs` |

An instance certificate is only worth what the model is worth, which is why the model travels
**inside every certificate's source**: a reader does not have to trust our copy of it.

## What is proven, and what is assumed

Proven: the facts, as written, satisfy `checkSlice` / `checkProgramme` / `checkPackage`, and the
model's universal theorems. The kernel checks this; nothing is trusted from the prover beyond an
answer of `valid`.

Assumed, and stated in every certificate:

- **Refinement.** That the engine builds the facts from what it actually recorded — an approved
  approval row, a card in its done list, a run's evidence. Lean does not see the database. Proving
  that the Rust implementation refines the model is a different tool's job (Verus, Kani, or a
  refinement proof); this corpus does not claim it.
- **The spec pin.** `spec_sha256` names the `docs/SPEC_V2.md` the facts were taken against. The
  renderer refuses a facts document whose recorded model digest is not the model it embeds, so a
  certificate cannot silently mix models.
- **The toolchain.** Sources are pure core Lean, no `Mathlib` import and no `native_decide`. The
  kernel reduces them; the prover's own identity is recorded on the row, and
  `prover_identity.attested` is `false` because the compute backend is not attested.

## The re-check protocol

A certificate is two engine invocations and one judgement, never a caller's claim:

1. **Render** the facts (pure function of the canonical JSON, no clock, no map iteration order).
2. **Check** the rendered source through the declared external action `lean4.check`, with an
   `Idempotency-Key`. The engine owns the HTTP call, the timeout, the breaker and the audit row,
   and persists the response.
3. **Record** (`proofs.record`) that invocation's key and input body. The handler re-reads the
   persisted response, refuses a caller-supplied verdict and refuses a source that is not the
   renderer's output for those facts.

Re-verification is the same shape with a **fresh** invocation: `lean4.check` again with a new key,
then `proofs.reverify`, which refuses unless the fresh source, verdict and recomputed certificate
digest all equal the stored ones. The proof table is append-only, so a successful re-verification
returns the same row — it is a replay, not a second claim.

## Model drift

The model file is embedded into every rendered source, so editing it changes every future source.
A stored certificate is re-checkable against the model it was proven on — its `facts_canonical`
carries that model's digest — and a certificate can never be re-rendered against a different model
and still pass. Re-proving after a model edit is a **new certificate**, never an edit of the old one.

## Limits

- The model is deliberately small: §5.3 (plan ≠ evidence), §5.4 (a gate carries evidence), and a
  terminal closed state. It is not a model of the whole engine.
- A certificate re-checks a *statement about recorded facts*, not the behaviour of the running
  system. What it buys is that the statement cannot be quietly changed later: the facts, the spec
  pin and the model digest are all in the digest the certificate carries.
- The prover has a source-size and timeout cap; a bundle is a set of leaf digests for that reason,
  not one enormous checked expression.
