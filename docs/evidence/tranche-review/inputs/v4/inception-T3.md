# Inception analysis — T3 PM job (retrospective, 2026-11-28)

## Current state

Before the PM job's own build, projects existed as rows and their lifecycle actions existed
separately: a charter here, a baseline there, a closure gate whose requester could also decide it.
There was no single end-to-end path a PM could run from a request to a closure, and no executable
refusal of self-approval. The receipt `docs/evidence/rework/T3.json` recorded the project as closed
without its live delivery contract and its Phase-G review, which is what the retrospective
completes.

## Reuse sources

- The approval and gate machinery (`approvals.request`, `approvals.decide`, the distinct-approver
  rule and the published-delegation rule) was reused as the PM job's decision surface; the job
  added no second approval store.
- The tenant-scoped project rows and the store were reused for the charter, the WBS, the baseline
  and the closure.
- The programme cadence surfaces were reused for the PI plan, the demo credits and the
  retrospective, rather than a PM-specific cadence.
- The register rows (risk, issue, assumption, dependency, change) were reused as the project's
  governance record; a baseline change is a change row, not a new mechanism.

## Build vs reuse

Built: the PM job's sequence — charter, WBS with its typed WSJF inputs, scope baseline, staffing,
gate routing, change control and closure — together with the refusals that make it executable: the
approver cannot be the requester, a baseline is changed through a change request and never edited
in place, and a closure reads the project's declared outputs, its closure review, its live delivery
contract and Phase-G review, its approved baseline, its artefacts and registers, and a demo credit
per accepted deliverable. Reused: the gate engine, the store, the cadence and the registers. The
line was drawn at "the PM job sequences what the engine already enforces", so the job adds no new
persistence and no second decision path.

## Risks

- Self-approval is the central risk; the gate refuses when the requester and the approver are the
  same person, and the test record carries a distinct-approver closure.
- A baseline edited in place would void the frozen scope; the change request is the only path, and
  the registers record it.
- A closure over missing declared outputs would close a project on an incomplete record; the close
  gate reads the declared outputs and refuses by name.
- The PM job is only as good as its record: a project can still be chartered with a vague mandate,
  which is why the charter gate and the reviewed artefacts exist rather than a single approval.
