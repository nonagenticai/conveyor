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
   | `apps/media` | the w2v stage fleet (analysis, content, ai-content-generation, slide, html-render, video-production, video-analyzer, app-inspector, bdd-playwright, youtube-upload, frontend-test-runner, tts-gateway) and the video-compiler service; their shared library becomes `crates/media` | one ffmpeg/render chain; the orchestrator's sequencing becomes procedures, not a service |
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

4. **`scientific-processor` drops its graph surface.** The `/api/topology` and `/api/explain`
   routes, the graph services behind them (centrality, community, subgraph, subgraphx,
   contradiction), and the graph-adapter and discovery-engine clients are **not ported**. Their only
   upstream's data was destroyed on 2026-11-01, nothing serves them, and no consumer exists. A
   capability with no live consumer is retired, not rewritten (P10). If that capability is ever
   wanted back it is a new one with a live upstream, never a restoration of the capture.

## Context

ADR 0004 named the tool estate but listed only the three services it was written about. ADR 0005
fixed the absorption order for those three and left `docs/SPEC_LEGACY_KAG.md` K7 (first rewrite
target) open. Meanwhile the kag-era survivors (`webscraper`, `media_discovery`,
`scientific_processor`, `lean4_prover`) and the w2v stage fleet had no owner, no grouping and no
order — which is how a programme of twenty Python services becomes twenty separate hand-built
lanes.

Measured 2026-11-14: `apps/` held `inference-router`, `server`, `web`; the four kag-era services and
every w2v stage are separate Python repositories; `nonagenticai/coding` is already Rust and under
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
