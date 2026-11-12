# 0002 — The workflow editor: a Leptos/SVG canvas, not `egui_xyflow`

Status: Accepted · Date: 2026-11-09 · Slice: S13 §5.6 (step 16)

## Decision

The admin graph editor is **fallback A**: a small Leptos/SVG canvas in the existing web app. The
`egui_xyflow` spike (`spikes/editor/`) is **not** adopted and is not built.

The rule the slice set stands: **the engine never depends on the editor.** All validation is
server-side (`workflows.publish` refuses an invalid graph); a failed or absent editor leaves every
workflow publishable and runnable.

## Context — what was measured

`docs/plans/slices/S13-n8n-engine.md` §5.6, measured 2026-11-07 against crates.io and GitHub, not
locally:

| fact | value |
|---|---|
| latest published | `egui_xyflow` **0.4.3**, 2026-05-06 |
| repository | `avinkrisv/egui-xyflow`, 21 stars, one maintainer |
| repo head (unpublished) | 0.5.0 on egui 0.37, last commit 2026-10-26 |
| MSRV | 1.95 |
| 0.4.3's egui pin | **egui 0.31 — UNVERIFIED** (the 0.5.0 commit message says 0.1–0.4 stayed on 0.31) |

The slice's adoption gate was four-fold: builds against the app's egui/eframe pin without patching;
60 fps drag at 20 nodes / 40 edges; a lossless layout round-trip; and a published version or a
vendored commit with a recorded SHA. **None of the four was measured**, because the first already
decides it: there is no eframe/egui in this app — ADR 0001 chose **Leptos CSR** — so "builds against
the app's egui pin" would mean adding a second UI toolkit and a second rendering stack to an admin
screen.

## Why fallback A

1. **No second UI stack.** The app is Leptos CSR (ADR 0001). `egui_xyflow` would pull egui/eframe
   in as a parallel renderer for one admin screen, including its own event loop and wasm bundle.
2. **The pin is a bet, not a dependency.** A published 0.4.3 whose egui pin is unverified, against
   an 0.5.0 that is not on crates.io, maintained by one person. Vendoring a commit is possible but
   inherits the same unverified pin.
3. **The graph surface is small.** The measured estate is 13 graph workflows after the three-way
   split, of which one is live (`kata.open`); the gate fixtures are 5–7 nodes. 60 fps at 20
   nodes/40 edges is not a requirement this screen has ever had to meet.
4. **Nothing in the engine can depend on it.** Publish-time validation is server-side and already
   proved (`definition_validate`, `workflows.publish`). The editor is an admin convenience.

## Consequences

- `spikes/editor/` is not created; no `egui`/`eframe` dependency enters the workspace.
- The editor work is a Leptos/SVG screen: load a definition via `workflows.get`, lay out nodes,
  drag, save via `workflows.update` (draft only) — with the same publish validation as any client.
- If the canvas proves inadequate (measured, not assumed), fallback B is a read-only SVG importer
  plus form/JSON edits; the engine requires neither.
- This record closes S13 step 16d. The spike (16a–16c) is deliberately not run.

## Superseded 2026-11-09

The graph editor this decision selected **is not built**: the process model replaces the DAG (SPEC D6, amended 2026-11-09), so the editor edits an ordered list of stages, not a graph. Fallback A's principle stands — no second UI stack; the stage editor is a form/list in the existing Leptos app. The `egui_xyflow` spike remains unrun and unnecessary.
