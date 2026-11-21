# Tranche 2 / PI 2 — the rework record (2026-11-21)

Delegation: `01a1ff1f-e4ba-72dc-9c1a-13049933cf14` (the U5 extension of the standing delegation).
Tranche: `01a1e9e0-e2ac-786b-ac48-3b8bcdf12188` (state `authorised` at 2026-11-21T19:51:16Z by the
tranche-1 `continue` review `01a1ffa9-c06f-7921-9d6b-611d871ce6cc`).
PI period: `tranche-2`. Programme: `01a1d039-0d24-7d16-ae52-4841e96ccfac`.

## Why this tranche is retrospective

Both of its projects are already recorded as delivered: S03 closed its V1 slice and T2 closed the
decision ladder. What was never recorded is the programme's own evidence for them — charter, scope
baseline, delivery contract, the A–D artefacts, a Phase-G review and a closure review. This batch
writes that record from each project's plan and its code at HEAD, and re-closes each project through
the one gate.

T2 is additionally one of the four live-register `closed_without_closure_review` projects: its
closure has no review at all, so it is reopened before it is re-closed.

## The acts

| act | action | row |
|---|---|---|
| PI cadence | `cadences.create` | `01a2004a-51f4-7482-b5f8-455a4bd371b5` |
| backlog refinement | `cadences.create` | `01a2004a-739d-717e-81dd-15ddf37f4c2f` |
| system demo | `cadences.create` | `01a2004a-87d8-77e9-b06a-171361548e38` |
| inspect and adapt | `cadences.create` | `01a2004a-9c9a-7999-bbbd-9976427009b6` |

Each write was read back through `cadences.list` and its id matched in the returned rows; the
live list shows 12 cadence rows, four of them for `tranche-2`.

## The PI artefacts

| artefact | row | published |
|---|---|---|
| PI plan (published) | `01a20051-62d5-76b5-9c22-a298c4c31c46` | 2026-11-21T22:54:58Z |
| PI objectives (published) | `01a20052-1d3e-71e4-9490-f531e1592cc0` | 2026-11-21T22:55:19Z |

Both were composed through the branch binary (`--local-bin`) because the deployed image's
compiled-in output standard does not carry the `pi-plan` and `pi-objectives` categories: the
same payload through the deployed engine is refused 422 with `unknown_category` in `errors[]`.
Publication used `documents.publish` with the composed version as `If-Match`.

## Residue that must be cleaned up

Two probe documents were created while diagnosing the category refusal and could not be deleted
from this session: `documents.delete` is `effect = destructive` with
`approval_target = target_documents_delete`, so removing them needs a destructive approval.

| probe | id | state |
|---|---|---|
| `probe-minimal` (no category) | `01a20050-8a6e-7597-b91f-9dbefe610535` | unpublished, body `probe` |
| `probe-evidence` (category `evidence`) | `01a20051-1199-754d-aafc-4c797ccc75d3` | unpublished, body `probe` |

Neither is publishable, so neither can reach the public pack; both should be deleted at the next
destructive-approval window.

## The lifecycle is blocked on the lane's own migrations

`scripts/rework-project.py` builds its review snapshot with `$PROGRAMME_RECORD_BIN` against the
read-only primary. That build now fails:

    error: snapshot: database: error returned from database: column d.archetype does not exist at line 3854

The branch is at migration `0063_tool_verify`; the live database is at `0061_contract_requester`.
`0062_release_layers` adds the column the branch's snapshot query reads. So until the lane merges
and its migrations are applied — or the batch runs against a snapshot built by the deployed
engine — no project can be taken through the runner. The two content records are authored and pass
`rework-content-check.py` offline; they are stored beside this file.
