# 0006 — The tool estate completes in this workspace, grouped by runtime shape

Status: Accepted · Date: 2026-11-14 · Issue: #105

## Decision

Owner direction, 2026-11-14: **every tool the conveyor calls is Rust in this workspace**, and thin
tools are combined where that makes sense. Orchestration and frontend work is out of scope — the
conveyor is both.

1. **The remaining tool estate joins `apps/`**, grouped as follows. A group is one deployable with
   one image, one database and one role; the toolkits it publishes stay separate catalogue entries.

   | App | Absorbs | Grouping reason |
   |---|---|---|
   | `apps/scientific` | the scientific research service (:8808), the evidence-review service (:8810) | one evidence family — meta-analysis, survival, PRISMA screening; CPU-bound, no browser, no GPU |
   | `apps/research` | the web-scraper service (:9119), the media-discovery service (:8809) | outbound web fetchers; IO-bound, no numerics stack |
   | `apps/media` | the separable media services — html-render, slide, video-production, frontend-test-runner (Chromium); video-compiler, video-analyzer (ffmpeg); tts-gateway, youtube-upload (thin) | grouped **by runtime**, because they do not share one. Corrected 2026-11-14: the "w2v stages" are not services — they live in `w2v_orchestrator/src/services/stages/` (18 directories), i.e. inside the orchestrator that is out of scope, and `w2v_common` is a library, not a service |
   | `apps/lean4-prover` | the Lean 4 prover (already Rust) | crate moves in; the deployable stays separate — 10.6 GB of toolchain state and a node pin |
   | `apps/coding` | the `coding-*` fleet | already being rewritten in its own Rust repository; **join that work, never a second rewrite** |
   | in flight | `apps/inference-router` (merged), `apps/licenses` (#45), `apps/billing-service` (#46) | ADR 0004/0005 |

2. **The combining rule.** Combine two services when they share runtime shape, job family and
   caller. Keep them apart when one needs a heavy runtime the other does not (the Lean toolchain, a
   browser) or when their scale shape differs. Three Python services becoming one small Rust
   deployable is the point; folding a model server into a statistics service is not.

3. **Sequence.** `apps/scientific` (the dead service, largest deletion first) → `apps/lean4-prover`
   (cheapest) → `apps/research` → `apps/media` → `apps/coding`. ADR 0005's rule that the w2v and
   coding rewrites are performed by the conveyor's own development project stands; this ADR fixes
   only their destination.

4. **`scientific-processor` carries only what has a live upstream.** Two drops, one test — an
   operation whose only data source no longer exists, or whose runtime already exists as another
   conveyor tool, is not ported. A capability with no live consumer is retired, not rewritten (P10).
   If any of it is ever wanted back it is a new capability with a live upstream, never a restoration
   of the capture.
   - **The graph surface.** `/api/topology` and `/api/explain`, the graph services behind them
     (centrality, community, subgraph, subgraphx, contradiction), and the graph-adapter and
     discovery-engine clients. Their store was destroyed on 2026-11-01.
   - **The graph-dependent operations** (`verify/cove`, `verify/integrity`,
     `pipeline/trigger|webhook|batch`): the same store, reached through the same two dead clients.
   - **The sandbox** (`sandbox/execute|jobs|results`): a second code-execution runtime when the
     conveyor already dispatches execution as `coding_jobs.create` / `coding_jobs.get`.
   - **Consequence — no database.** With the sandbox and the pipeline producers gone, nothing in the
     surviving service writes a row. It ships with no Postgres and no migration.
   - **`z3`/`sympy` verification is refused by name** (`422 language_not_supported`). Not because
     the sandbox owned it — it did not: the old service evaluated those expressions in-process with
     an AST allowlist interpreter over the `z3` and `sympy` libraries. The reason is that there is no
     faithful Rust equivalent in this workspace: `z3` would mean binding libz3, and `sympy` has no
     Rust counterpart. **Known deviation:** the captured default for `language` is `z3`, so a caller
     that omits the field is refused here where the old service answered. Translation to both
     languages is unaffected — it is a model call, not an evaluation.

   The lock therefore carries 18 of the captured 26 operations. No live consumer is lost: the
   Deployment has been at `replicas: 0` since 2026-11-01.

## Context

ADR 0004 named the tool estate but listed only the three services it was written about. ADR 0005
fixed the absorption order for those three and left `docs/SPEC_LEGACY_KAG.md` K7 (first rewrite
target) open. Meanwhile the kag-era survivors (`webscraper`, `media_discovery`,
`scientific_processor`, `lean4_prover`) and the w2v stage fleet had no owner, no grouping and no
order — which is how a programme of twenty Python services becomes twenty separate hand-built
lanes.

Measured 2026-11-14: `apps/` held `inference-router`, `server`, `web`; the four kag-era services and
every media service are separate Python repositories (the w2v *stages* are orchestrator code, not
repositories — see the corrected `apps/media` row); `nonagenticai/coding` is already Rust and under
active construction.

## What this supersedes

- `docs/SPEC_LEGACY_KAG.md` **K7** becomes decided by consequence: the first target is the
  scientific research service. K3–K5 (neo4j, hirag, karma) stay open and are unaffected — decision 4
  above does not decide them, it observes that the graph adapter has no data and no consumer.
- ADR 0004's in-scope list grows by the kag-era survivors, the w2v stage fleet and the video
  compiler. ADR 0004's "how a service joins" procedure and ADR 0005's exceptions (the two strata
  services stay external; coding and w2v are conveyor-executed) are unchanged.

## Consequences

- One issue per app; the first is #105 (`apps/scientific`). The toolkits a group publishes keep
  their §11.2 identities, so the broker's catalogue does not learn the deployment topology.
- `strata_ingest` and `strata_rag` remain external repositories (ADR 0005 §2). The mini model
  servers (TTS, ASR, image generation, VLM) stay remote HTTP hosts bound as toolkits: they are model
  servers, not tool code, and the already-converted ones are not touched again.
- A group's image is built by `docker/Dockerfile.service` and published by the per-service CI row,
  which lands only after the package is on `main` (#99's rule).

## Amendment — 2026-11-15 (#139): the device-inspection decision

#139's P4 asks for one explicit answer, and this is it.

**`app_inspector_service` is not absorbed.** It stays a binding to an external tool, for two
reasons that are independent:

1. **No runtime in common.** ADR 0006 §2 keeps services apart when one needs a heavy runtime the
   others do not. Device inspection needs a Selenium/Appium driver in front of a *device farm* — a
   fleet of physical or emulated devices. No such farm exists in this estate, and Chromium (which
   `apps/media`'s browser binary does carry) is not a substitute for one: the capability is
   "drive a phone", not "render a page".
2. **No conveyor consumer.** Its only caller is the w2v orchestrator, which is out of scope by the
   §Decision above. Decision 4's rule applies unchanged: a capability with no live consumer is
   retired, not rewritten, and if it is ever wanted back it is a new capability with a live
   upstream.

Two further browser services are named here so the record is complete rather than silent:
`bdd_playwright_service` and `redesign_service` are browser services the `apps/media` group row
does not list. Neither is absorbed by #139. If the conveyor ever calls either as a tool, that is a
new capability and its own issue — not a widening of this group by inference.

What remains true of the group is unchanged: the eight services the row names, three binaries by
runtime shape, one issue and one PR.
