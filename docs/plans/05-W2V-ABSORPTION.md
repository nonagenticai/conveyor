# 05 — The w2v absorption: disposition and the production layer

This plan complements `04-TOOL-ESTATE-INVENTORY.md`; it does not replace it. Plan 04 holds the
grouping decision, the sixteen-blueprint vocabulary and the destination count. This document holds
what has **actually shipped**, one disposition per absorbed component, and the measurement that
fixes the shape of the one family nobody has started.

Convention, inherited from plan 04 §5: components are named by **public role**. No private
repository name, host name or address appears here — the repository is public and
`scripts/ci/no-private-refs.sh` is a merge gate.

## 1. What has shipped, measured 2026-11-15

Plan 04 §1 states `apps/media` as "decided, unclaimed (#139)". That is no longer true. The row is
corrected here and in `04` itself.

| Group | Plan 04's state word | Measured state | Evidence |
|---|---|---|---|
| the executor group (`apps/media`) | decided, unclaimed | **absorbed** (#139 merged 2026-11-15) | `apps/media/src/{browser,ffmpeg}/`; twelve `media.*` actions in `crates/actions/src/media.rs` — eleven executor kinds and the capabilities probe |
| the analysis library (`crates/analysis`) | absorbed, not deployed | **in flight** (#154) | absent on main; P1–P4 on `issue/154-analysis` |
| deck authoring (`crates/deck`) | not listed | **in flight** (#161) | absent on main; `issue/161-deck` |
| the production family (`produce.*`) | "one family" (§3) | **not started** | zero `produce.*` actions registered |
| the model-server boundary | external HTTP | **holds**, and is already Rust (§5) | both serving hosts run Rust binaries |

The distinction plan 04 §1 draws — "merged" is not "live" — still applies to the two groups that
are absorbed but not deployed. It does **not** apply to the executor group's *code*: the rewrite and
the captured contract are on main and tested.

## 2. Disposition, one row per component

Each row is a component of the legacy media estate, named by role, with where it went.

| Component, by role | Disposition | Destination |
|---|---|---|
| the site-analysis service (critique, review, brand, SEO, CRO, marketing scorecards) | absorbed in flight | `crates/analysis`, `analyse.*` (#154) |
| the browser-rendered producers (HTML to image, recording, Gherkin) | absorbed | `apps/media`, browser executor: `media.render_html`, `media.record_browser`, `media.run_gherkin` |
| the ffmpeg producers (compile, concat, thumbnail, probe) | absorbed | `apps/media`, ffmpeg executor: `media.compile_video`, `media.concat_audio`, `media.thumbnail`, `media.probe_video` |
| the video measurement service (frame extraction, OCR, audio measurement) | absorbed | `apps/media`: `media.analyze_video`, `media.analyze_audio`, `media.ocr` |
| the video assembly producer | absorbed | `apps/media`: `media.produce_video` |
| the slide/deck service | in flight | `crates/deck` (#161), `produce.deck` |
| the content service (storyline, shot list, narration text, script generation) | **not started** | `produce.script` (#163) |
| the course assembler | not started | a blueprint over `produce.*`, not a service (#163) |
| the video-publish operation | out | no measured consumer; ADR 0006 defers it |
| the thin speech fan-out | out | model-server boundary (§5) |
| the app-inspection service | out | owner ruling 2026-11-15 (plan 04 §7.1): deferred outright, never worked live |
| the website-redesign service | out | subsumed by the site-analysis family's redesign output |
| the image/video generation fan-out | out | model-server boundary, and no measured consumer |
| the shared Python library | dissolved | its functions land in existing crates; no destination of its own |
| the orchestrator | out | plan 04 §6: its role becomes procedures and project types, not a second scheduler |
| the SPA | out of this programme | absorbed by the product's own web surface (Leptos), on its own schedule |
| the six predecessor microservices (analyzer, brand, critique, review, narrative, script) | archived | subsumed by the site-analysis and content services before this programme started |

## 3. Why the production family is data, not services

Plan 04 §2 tabulates the sixteen blueprints. What it does not record is the measurement that makes
the production family's shape non-negotiable.

Across the sixteen blueprint directories, the same stage primitives are re-implemented per blueprint:

| Primitive | Copies | What it is |
|---|---|---|
| `narrate.py` | **9** | narration text to per-shot audio, via the speech model server |
| `compile.py` | **8** | shots to one encoded video, via ffmpeg |
| `slides.py` | **7** | a slide spec to rendered stills |
| `script.py` | **7** | the deliverable's narration script |
| `record.py` | **7** | a browser stepped through virtual time into a video |
| `scan.py` | **3** | a crawl/fetch pass feeding an audit |
| `critique.py` | **2** | the scored read over a scan |
| `stitch.py` | **2** | segments joined into one artefact |
| `thumbnail.py` | **1** | a poster frame |

**46 stage files are 9 distinct operations.** Every blueprint's real content is the *ordering* and
the *parameters*, not the stage logic. Two consequences:

1. The production family needs **one action per primitive**, not one service per blueprint or per
   legacy service. `produce.script`, `produce.speech`, `produce.still`, `produce.deck`,
   `produce.render`, `produce.qc` cover all nine primitives (`SPEC_V2` §10.4 already declares this
   set).
2. A blueprint is **data**: an ordered phase list whose phases name registered actions. It is not a
   deployable, and porting 46 files as 46 modules would reproduce the sprawl inside one repository.

The two families divide exactly as plan 04 §2 says: nine blueprints produce **evidence** (the
`analyse.*` side, mostly landed), seven produce **an artefact** (the `produce.*` side, not started).

## 4. The production layer — the only unstarted family

This is the gap that remains after the executor group merged and the analysis and deck lanes finish.

| What exists after #154 and #161 | What does not |
|---|---|
| 12 executor actions (`media.*`) | any `produce.*` action |
| the `analyse.*` library and its action | the blueprint manifest type |
| the runner substrate and job ledger | the sixteen blueprints as data |
| `crates/deck` for deck assembly | the script/storyline/storyboard capability |

Issue #163 carries the plan: capture the content service's contract, build the manifest type, ship
`produce.script`, then express the sixteen blueprints over the actions that exist and declare the
phases that have no action yet rather than silently omitting them.

## 5. The model-server boundary, verified

ADR 0007 keeps the model servers outside the repository as remote HTTP hosts bound as toolkits. That
is no longer only a decision — it is the estate's actual condition, measured 2026-11-15:

- The speech, transcription and vision servers on both serving hosts run **Rust** binaries
  (Mach-O arm64, Tokio symbols), alongside a vendored embedding/rerank server.
- The one remaining Python model server is the image-generation host. It has no Rust equivalent and
  is explicitly outside this programme.
- The legacy speech backend, the voice generator and the speech router have **no running process and
  no service definition** on either host.

So the boundary costs nothing to maintain: the Python the conversion would have had to remove is
already gone, and what remains is one service with no rewrite path.

## 6. How to re-measure

Every number above is reproducible without cluster access:

- **Action families**: `git grep -hoE 'name = "[a-z0-9_.]+"' origin/main -- crates/actions/src`,
  then count by namespace prefix. `media.*` must be 12 and `produce.*` must be 0 while this
  document's §1 is current.
- **Blueprint duplication**: count `basename` matches under the orchestrator's stage directory,
  excluding its two library directories. 16 blueprint directories, 9 primitives, 46 files.
- **Group state**: `gh issue view <n> --json state` for the executor, analysis and deck units.

If a count disagrees with this document, the document is stale — correct it in the same PR that
changes the count, not later.
