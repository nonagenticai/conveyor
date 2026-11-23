# 06 — Tool provenance and the retirement gate

**Status:** the method and per-source summary behind `seed/tool_provenance.json` · 2026-11-22 ·
unit #558. **Authority above this file:** `docs/SPEC_V2.md` §11.2 and §11.9,
`docs/decisions/0006-tool-absorption-grouping.md`,
`docs/decisions/0007-tool-estate-conveyor-and-coding.md`, and the inventory in
`docs/plans/04-TOOL-ESTATE-INVENTORY.md` and `docs/plans/05-W2V-ABSORPTION.md`.

This file does not restate 04/05. It records **how the capability diff was measured**, where each
source went, and what the repository now enforces. The machine-readable artifact is the seed; the
gate is `crates/actions/tests/tool_provenance.rs`.

## Method

1. **Inventory the sources the repository records**, not the repositories themselves. The evidence
   is `docs/plans/02-source-map/` (the retired work-management, board, gateway and flow surfaces),
   the analysis and evidence-review capture notes under `docs/evidence/`, and the in-repo OpenAPI
   locks for the four survivor services. No private repository, host, address or credential is
   named.
2. **One row per recorded capability.** Where a source map records an operation family
   (`/requirements*`, `/pm/projects`, the board application's board/card surface), the row is the
   family. Where the contract is an OpenAPI lock, the row is the operation. A row is reviewable; a
   transcription of 548 work-management paths or 367 flow-engine operations is not.
3. **Disposition is one of three.** `ALREADY-PRESENT` names the conveyor action or manifest tool
   that carries the capability. `PORT` names a live consumer and a usage sample, and either a
   manifest tool or the open unit that will provide it. `DROP` records the measurement date and the
   query or decision that justified dropping it.
4. **A retired source is never a destination.** Only a `live` source carries service aliases.
   Every manifest `service` must be owned by a live source, and a retired, archived or migration
   source carries none — so adding a manifest cannot turn one into a destination. No retired
   product name is used as a source label.
5. **The gate runs with the suite.** `cargo nextest run -p conveyor-actions --test tool_provenance`
   is part of `scripts/ci/gate.sh`; the seed is reviewed in the same PR as any manifest it targets.

## Per-source summary

| Source (public role) | Status | What it was | Disposition |
|---|---|---|---|
| coding fleet | live | repository provisioning, verified writes, tests, PRs | two tools (`coding.jobs_create`, `coding.jobs_get`); the fleet stays external (D14/ADR 0007) |
| model hosts | live | speech, transcription, image generation | three tools; the hosts stay external HTTP |
| strata retrieval | live | pinned-leaf retrieval | one tool (`retrieval.query`) |
| requirements / work management | retired | requirements, principles, decisions, projects, programmes, benefits, registers, architecture, modelling, UML, catalogue, search, approvals, cards, schedules, secrets, channels | **carried as conveyor actions**; the control-plane and product surfaces are dropped (§11.2) |
| board application | retired | boards, lists, cards, comments, labels, custom fields, memberships, tasks, webhooks | **carried as `boards.*`, `cards.*`, `tasks.*`, `webhooks.*`**; attachments and the admin-only surfaces are dropped |
| flow engine | retired | 348 workflows, agent steps, prompt recipes | **flows become procedures** (`procedures.*`); the runtime and its node packages are dropped |
| legacy media fleet | migration | 16 stage blueprints, orchestrator, analysis, deck, content, browser/ffmpeg producers, media measurement | **rewritten as `media.*`, `analyse.*`, `decks.render`, `produce.script`, and the model-server tools**; the orchestrator, SPA, app inspector, video-publish and video-generation fan-out are dropped |
| archived graph platform | archived | graph service, hirag, karma, neo4j, orchestrator, clients, frontend | **not a destination** (R19); the graph service and the client surface are dropped, while K3–K5 (neo4j, hirag, karma) remain **undecided** and are deliberately not dispositioned in the seed |
| research and Lean services | live | webscraper, media discovery, Lean prover, scientific processor | **porting**; the manifests land in #561 (webscraper, media discovery, Lean) and #560 (scientific) |

## What the seed proves today

- Every one of the six manifest tools has an `ALREADY-PRESENT` row.
- Every `ALREADY-PRESENT` target is a registered action or a published tool; the test resolves
  them against the live registry and manifests, not against a hand-maintained list.
- Every manifest `service` is owned by a `live` source; no non-live source carries a service
  alias, so no retired application can become a destination.
- Every `DROP` carries a date and a query or an owner decision.
- Every `PORT` carries a live consumer and a usage sample; the eleven survivor rows name the open
  unit (#561 or #560) until their manifest tools exist.

## Gaps this inventory exposes (not claimed here)

- **`evidence_review`** was absorbed into `apps/scientific`; it has no manifest row yet. It needs a
  unit of its own once the scientific toolkit's read surface is settled.
- **`strata_ingest`** is live and has no conveyor action or toolkit. It is a follow-on unit, not a
  row that can resolve today.
- **`ops.*`** exists as an action family (18 actions, read verbs plus gated writes) but has no
  toolkit; host control is T7's mechanism decision, not a provenance row.
- **The four survivor services** are pending rows, not omissions; #561 and #560 replace the
  `pending` with the published tool targets and keep this gate green.

## Corrections measured on 2026-11-22

`docs/plans/05-W2V-ABSORPTION.md` was measured on 2026-11-15. Three of its statements are now stale
and are corrected here, in the same PR that measures them:

- `produce.script` is registered; the production family is not "zero actions".
- The deck action is `decks.render` (`crates/deck`), not the planned `produce.deck`.
- `analyse.site` and `analyse.video` are registered; the analysis library is on `main`, not "in
  flight".

## Re-measuring

```bash
cargo nextest run -p conveyor-actions --test tool_provenance
bash scripts/ci/no-private-refs.sh
```

The first fails naming the row, tool or source that broke a rule. The second fails if a private
repository path, host or credential reached the seed.
