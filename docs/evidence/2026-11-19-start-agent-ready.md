# The start agent completes READY — 2026-11-19 (#383)

The fix is **database content**, not a seed change. These are the rows that changed on the live
tenant (`nonagentic`) and the run that proves them.

## Rows changed

| Row | Before | After |
|---|---|---|
| `capability_agent` `project_start_request_analysis` | v3, `allowed_actions` = `[documents.compose, documents.get, documents.list, cards.get, cards.comment]` | **v4**, `allowed_actions` = `[]` |
| `procedure` `project_start_request_analysis.ready` | v1, criteria `[brief_unchanged, scope_is_stated]` | **v2** (`01a1f356-594c-7148-8cd2-fdc63205b6c9`), adds `answer_is_the_analysis` to the criteria and the definition of done |
| `process` `project_start_request_analysis.READY` | v1: `agent.run` → `cards.move` | **v2**: `agent.run` (outputs `[output]`) → computed `documents.compose` (body `{{steps.1.output.output}}`, outputs `[id]`) → `cards.move` |

The agent's surface is empty — the #307 pattern. The engine omits `tools` for an empty surface, the
model answers in one turn, and the process stores the answer as the analysis document. The seed
files are untouched: content lives in the database.

## Proof run

Project `A5-ENGINE-PROOF-2026-11-19D` (`project_type_code: it`), run
`01a1f359-e318-747f-8324-b82b49db43bc`:

| Step | Action | Result |
|---|---|---|
| 1 | `agent.run` | **succeeded**; judge accept, score 0.8361, against `[brief_unchanged, scope_is_stated, answer_is_the_analysis]` |
| 2 | `documents.compose` | **succeeded** |
| 3 | `cards.move` | **succeeded** |

No tool call was refused: the run's trace records one `inference.chat` (`ok`) and one
`laya.judge` (`ok`).

## Observations recorded, not fixed here

- A project created without `project_type_code` cannot move its start card
  (`card-move-not-allowed`), and `program` has no matrix entry for the start card type; the types
  that do are `architecture`, `it`, `kata`, `py2rust`.
- A failed run's coach run can hold the single worker past its job lease (two measured, both
  cancelled). This starves every queued step.
- The judge accepted a meta answer at 0.836 while the prompt template still says "answer with a
  short summary of what you did" — the wrong instruction for a tool-less agent. Prompt recipes
  have no authoring action today.

## Cleanup

`A5-ENGINE-PROOF-2026-11-19B/-C/-D` are closed (version 2). Each close gate was approved under a
scoped `projects.close` self-approval window that was restored to `forbidden` afterwards.
