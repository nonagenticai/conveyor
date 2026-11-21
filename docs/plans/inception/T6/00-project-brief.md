# T6 — Development project

- **Status:** published
- **Version:** 1.0
- **Author / date:** conveyor programme session / 2026-11-21
- **Reviewer / review date:** #347 independent review / 2026-11-18; owner publication / 2026-11-21
- **Related project:** T6 — Development project · issue #80 · live document `01a1e008-8640-73e4-a90a-6bf8c250a0af`
- **Rationale:** state the inception basis for T6: a real feature lands on the conveyor and one dependency repo through the lane, PR, CI, review, merge, deploy and live-verify path, without the project changing its own gates.
- **Published document:** `01a1e008-8640-73e4-a90a-6bf8c250a0af` — published 2026-11-21
- **Contract:** pending
- **ADM phase / plateau:** A / PSA for the artefact (project row: G / capability)
- **Entries:** outcome, boundary, reuse sources, inception units, dependencies, acceptance, capabilities, plan, risks, links.
- **Derivation / realization / reuse / dependency:** derived from SPEC_V2 §6.4, §9.1 and §12 T6, and P0 §4; realized by #80; reuse sources below; depends on T5, T13 and P0.
- **Links:** issue #80; `docs/SPEC_V2.md` §12; `01-inception-analysis.md`; `02-architecture-design.md`; `seed/programme.json`.

## Outcome

The conveyor develops a real feature on itself and one dependency repo: lane, PR, CI, review, merge, deploy and live verify, with rollback tested.

## Boundary

In scope: the development-project lifecycle — a lane branch and PR, the local gate, CI, an independent human review, the merge, the deploy read-back and a tested rollback — implemented with the existing GitHub client, PM and document surfaces.

Out of scope: the V1 built slices (not re-opened); a second GitHub client or token path; the coding fleet's execution (T13 owns it); cluster writes (Flux and a human own the deploy); the CI workflow itself (reused unchanged).

## Reuse sources

- `crates/actions/src/integrations/github.rs` — the one token boundary and HTTP stack; extend its verbs, never add a second client.
- `apps/server/src/publish.rs` — the PR-only, sealed-secret, one-branch, one-PR, never-merge discipline.
- `migrations/0013_documents.sql` + `migrations/0040_organisation.sql` — the per-repository credential and the release binding.
- `crates/actions/src/pm/mod.rs` — the PM actions that gate a development project, including the independent-approver rule.
- `.github/workflows/ci-public.yml` — the repo-specific gate; the estate archetype owns the shared part.
- Estate capability, by role: the coding fleet — lane, commit and PR operations, at its effect ceiling.
- Upstream: the GitHub REST API and its merge queue — required checks and merge semantics.

## Inception units

1. `inception-analysis` — `01-inception-analysis.md`; live `01a1ee4e-ac03-76aa-8f54-81018d8bcd57` (published 2026-11-21).
2. `architecture-design` — `02-architecture-design.md`; live `01a1ee4e-b842-70a4-81ee-aacce62c31a9` (published 2026-11-21).

Both are published (owner publication, 2026-11-21); the inception gate is met.

## Dependencies

Typed, from `seed/programme.json`: blocked-by T5, T13 and P0; related #80 (delivery). T13's broker must exist before a coding job is bound; T5's kata lifecycle is the measurement path. The delivery unit is `status:blocked` because tranche 11 is not authorised.

## Acceptance

The project gate: a real feature lands through lane → PR → CI → review → merge → deploy → live verify; the project cannot change its own gates; rollback tested.

Verify: exercised by #80's own scenarios (`crates/actions/tests/development_project.rs`, to create there); at inception the acceptance is the published analysis+design pair, not yet satisfied.

## Capabilities

`development` — owns the delivery path; no other capability may fork the merge gate.

## Plan

`docs/SPEC_V2.md` §6.4, §9.1 and §12 row T6. Delivery unit #80; no separate `S##` slice file. Target tranche **11 — not authorised**.

## Risks

- A PR weakens its own gate → check: the merge refuses a protected-path change without a gate-change approval.
- Green CI is read as approval → check: merge refuses without an independent human approval naming the run evidence.
- The token leaks → check: secret scan and private-ref scan on the PR, plus the redaction test.
- A second coding path forms before T13 → check: the served-set lock test in `crates/actions/tests/registry.rs`.
- Deploy or rollback is asserted, not evidenced → check: closure refuses a release with an empty live check or an untested rollback.
- A skipped private-repo run counts as green → check: the gate requires a run URL on a public repository.

## Links

- Issue #80 (delivery; blocked).
- `docs/SPEC_V2.md` §6.4, §9.1, §12 T6.
- `seed/programme.json` row T6.
- `01-inception-analysis.md`, `02-architecture-design.md`.
