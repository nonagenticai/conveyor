# The console's live engine proof (A5) — 2026-11-19 (#97)

Measured against the deployed revision through the operator CLI — the same
`POST /api/actions/<name>/invocations` contract the console calls. There is no headless browser
token, and browser-only flows are out of scope per the issue.

## Prerequisites (measured)

- Pod `CONVEYOR_GIT_SHA=f14b7d36…`, image `sha256:3695374…` (2026-11-19).
- The 2026-11-15 credential blocker **predated #160**: the tenant's inference endpoint is
  registered (`https://api.nonagentic.ai/inference-router`) and the calls below succeed. No
  entitlement change was needed.

## A5.1 — a real inference call

The engine called `inference.chat` on every agent turn: 12 invocations on run
`01a1f2ac-1e68-7687-bb8d-b17383e6644b`, all `outcome=ok` (`model=served_model=local-llm`), and one
on the proof run below. Each ledger row carries its run and step.

## A5.2 — a throwaway project and its root run

Project `A5-ENGINE-PROOF-2026-11-19D` (`project_type_code: it`), run
`01a1f359-e318-747f-8324-b82b49db43bc`. The run has three `run_step` rows and `runs.list` lists it.

## A5.3 — the first agent step

Step 1 `agent.run` **succeeded**: judge accept (score 0.8361) against
`[brief_unchanged, scope_is_stated, answer_is_the_analysis]`. The resolved tool surface is **empty
by design** — #383 (agent v4) makes the model answer in one turn; the process composes the
document as step 2. The composed document is the analysis artefact; step 3 moved the card.

## The blocking failure that was recorded, then fixed

Before #383, step 1 failed `agent.turn_limit` after 12 turns of refused tool calls (run
`01a1f2ac-1e68-7687-bb8d-b17383e6644b`). #383 fixed it in database content; the run above is after
that fix, and its evidence is on #414.

## Result

A5.1 PASS · A5.2 PASS · A5.3 PASS. The throwaway projects (A/B/C/D) are closed.
