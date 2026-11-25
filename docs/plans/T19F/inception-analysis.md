---
document: T19F/inception-analysis
version: 1
status: draft
project: T19F
category: inception-analysis
owner: business-analyst
reviewer: programme-architect
review_date: 2026-12-01
artifact_kind: inception-analysis
phase: Preliminary
plateau: definition
---

# T19F — Inception analysis

## Question

T19's closure review names two open items: the `meeting_type` authoring route and the T9 board /
T10 cadence binding. What is the smallest implementation that closes them through the platform's
existing routes, and what does it cost?

## Measured starting point

`main@2debb226`, 2026-11-24:

| Seam | Measurement |
|---|---|
| The policy has versions but no writer | `meeting_type` is versioned and immutable (`migrations/0065_meetings.sql`); `crates/store/src/meetings.rs:schedule_tx` creates v1 only and refuses an existing key with "a policy change is a new version through the governance route" |
| The authoring route exists | `authoring.intake/draft/review/publish/assets`; `migrations/0064_authoring.sql` admits `kind='policy'`; `authoring.review` requires an approved `approval_id`; one published row per `(tenant, kind, key)` |
| The board has no type | `governance_body` (`migrations/0054`) has no meeting-type column; `architecture.board_open` and `board_decide` (`crates/actions/src/architecture.rs`) call `decide_board_tx`, which writes one approval row directly |
| The cadence has no type | `programme_cadence` (`migrations/0044`) has `meta` only; `cadences.create/update` (`crates/actions/src/spine/mod.rs`) do not name a meeting type; `meetings.schedule` accepts a `cadence_slot` but nothing materialises it |
| The arithmetic exists | `crates/store/src/meetings.rs:close_item_tx`; T19's D4 evidence has 26 scenarios |
| The surfaces have no policy view | `docs/design/v2.js:meetingsPage`; `apps/web/src/screens/meetings.rs` |

## Options

| Option | Shape | Assessment |
|---|---|---|
| A. Dedicated `meetings.types_*` CRUD | new actions with their own approval | duplicates T17's review/publication pipeline; a second authoring route to maintain |
| B. T17 authoring route | `authoring.draft(kind=policy)` → human review approval → `authoring.publish` writes the next `meeting_type` version | reuses the platform's prescribed tenant-asset route; one publish hook; the asset key and the type key stay distinct |
| C. Seed/config only | edit the seed and redeploy for a policy change | no runtime route; a tenant cannot change its own policy |

**Decision: B.** The policy is a tenant policy; T17 is the tenant's authoring route. The hook is a
single additive call inside `asset_publish_tx`, with the existing review approval as the governance
gate. Option A would create a second policy store; option C leaves the clause in the schedule's
refusal with no way out.

## Binding options

| Option | Shape | Assessment |
|---|---|---|
| A. Link only | store the type key on the body and cadence row; decisions/materialisation stay manual | the keys are visible but the board still writes one direct approval; the cadence still needs a hand-made schedule |
| B. Sittings and slots | a board sitting opens the approval + meeting + agenda item and decides through `close_item_tx`; a cadence row materialises its meeting | the board's decision carries the deliberation T19 built; one class of arithmetic |
| C. Rebuild T9/T10 | replace the board/cadence aggregates with meeting calls | a rebuild the spec explicitly forbids |

**Decision: B**, with the direct `architecture.board_decide` path unchanged. The sitting is an
additional entry, not a replacement; a body without a type keeps the direct path.

## Affected units

- `crates/store/src/meetings.rs` — the version writer, the sitting helper, the cadence materialiser.
- `crates/store/src/authoring.rs` — the publish hook.
- `crates/store/src/architecture.rs` — `insert_board_approval_tx` extracted from `decide_board_tx`.
- `crates/store/src/spine.rs` — the cadence key and the derived meeting.
- `crates/actions/src/{meetings,authoring,architecture,spine}` — the five actions and the changed input.
- `crates/api/tests/meetings_authoring.rs`, `meetings_binding.rs` — the new scenarios.
- `docs/design/v2.js`, `apps/web/src/screens/meetings.rs`, `apps/web/e2e/meetings.spec.ts` — the surfaces.
- The programme record — the T19F plan, artefacts, seed, coverage and design pack.

## Assumptions

- T17's authoring route is stable while #655 lands its read action; the hook is additive.
- A governance body has at most one standing type at a time; the explicit key replaces the
  key-coincidence convention.
- A cadence row is one forum instance per slot; a cancelled meeting may be replaced.
- T19F is delivered at tranche 6 by the coverage graph (T19 is tranche 4, T17 tranche 5).

## Risks

| Risk | Likelihood | Impact | Response |
|---|---|---|---|
| T17 file collision | medium | merge conflict | rebase before the PR; the hook does not change signatures |
| A policy version is published without evidence | low | governance defect | `authoring.review` requires an approved approval row; the plan tests the refusal |
| A board sitting bypasses the direct policy | low | dual control lowered | `close_item_tx` is the only writer; the stricter approval policy wins |
| A cadence slot gets a second meeting | medium | duplicate deliberation | the partial unique index; the action is idempotent |
| T19F closure manufactures a P0 | high if closed | strict gate red | T19F stays active; the closure review defers to the tranche-6 review |

## Recommendation

Deliver T19F in the split the delivery contract writes: the plan and artefacts (#682), the
implementation (#683) and the deployment-gated proof (#684). The live proof waits for the rollout
that carries migration 0073; it is recorded as acceptance-blocked, never waived.
