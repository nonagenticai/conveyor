# The walk's slice-gate audit (W6)

Measured 2026-11-28 at `main@8430123a782d02907332acd5fc964561a2a4abc9` for S4 (#817), against the
56-route census (`docs/evidence/walk-census.json`) and `apps/web` at the same sha.

## 1. The rule

`docs/plans/GEMBA-WALK-SPEC.md` §14: a destination may not claim to be implemented from its route
alone, and a capability the server lacks is the honest gate — never a fabricated surface. The plan's
W6 asks a narrower question: **does every gate the walk renders match what `apps/web` renders at the
same sha, and is every difference owned by a slice or a recorded decision?**

Two kinds of non-live state are possible, and they are not the same thing:

- a **role gate** — the action is absent from *this role's* manifest (`ctx.hasAction(name)` is
  false). It is derived at runtime and cannot drift from the app, which gates the same way.
- a **slice gate** — the capability does not exist yet for *anyone*, so the screen says so in prose.

## 2. The measured census

40 destination modules were read (38 top-level plus `v1/boards.js` and `v1/cockpit.js`):

| Kind | Count | Meaning |
|---|---|---|
| `ctx.hasAction` only | 25 | role-gated; no hardcoded promise |
| non-manifest condition | 3 | `measure.js`, `copilot.js`, `sources.js` — examined in §4 |
| no non-live state at all | 12 | the screen renders its read directly |

The 12 with no non-live state: `agents`, `decisions`, `execution`, `jobs`, `meetings`, `organisation`,
`plan`, `project-types`, `runs`, `validation`, `v1/boards`, `v1/cockpit`.

Several of them substitute an empty list when an action is absent (`decisions.js:69,72`,
`meetings.js:65,68`, `plan.js:110-116`, `execution.js:66`, `project-types.js:38`, `jobs.js:196`,
`organisation.js:214-216`, `runs.js:182`). §5 shows that this is `apps/web`'s own convention, not a
walk defect.

## 3. The role gates (25)

Each row is the condition that renders the non-live panel; every one is a manifest check.

| Module | Condition | Panel |
|---|---|---|
| `adm` | `!hasAction("architecture.profile_get")` | "ADM is planned" |
| `architecture` | `!hasAction("models.list")` | "Architecture is planned" |
| `approvals` | `!hasAction("approvals.list")` | "Approvals is planned" |
| `authoring` | `!hasAction("authoring.intakes")`; `hasAction("authoring.intake")` for the row control | "Authoring is planned" |
| `benchmarks` | `!hasAction("benchmark_suites.list")`, `benchmark_runs.list`, `benchmark_runs.get` | "Benchmarks is planned" |
| `evals` | `!hasAction("evals.list")`, `evals.get`; `evals.create` for the control | "Evals is planned" |
| `feedback` | `!hasAction("feedback.list")` | "Feedback is planned" |
| `improve` | `!hasAction("kata.open")` | "Improve is planned" |
| `knowledge` | `!hasAction("knowledge.query")`; `capabilities.list` for the select | "Knowledge is planned" |
| `memory` | `!hasAction("memory.recall")` | "Memory is planned" |
| `metering` | `!hasAction("billing.summary")`; `metrics.usage`, `inference_usage.report` per section | "Metering is planned" |
| `models` | `!hasAction("models.list")`; `models.create` for the control | "Models is planned" |
| `operations` | `!hasAction("ops.pods")`; each section's own action | "Operations is planned" |
| `platform` | `!hasAction("ops.flux_status")`; each section's own action | "Platform is planned" |
| `portfolio` | `hasAction("programmes.create")` for the control | role hint, no planned panel |
| `procedures` | `!hasAction("procedures.list")`; `procedures.create` for the control | "Procedures is planned" |
| `process-map` | `!hasAction("process_maps.list")`; `process_maps.create` | "Process map is planned" |
| `processes` | `!hasAction("processes.list")`; `processes.create` | "Processes is planned" |
| `programme` | `!hasAction("programmes.list")` | "Not yet live — T10" |
| `prompt-audit` | `!hasAction("prompt_audits.list")` | "Prompt audit is planned" |
| `requirements` | `!hasAction("requirements.list")`; `requirements.create` | "Requirements is planned" |
| `search` | `!hasAction("search.query")` | "Search is planned" |
| `settings` | `!hasAction("tenants.get")` | "Settings is planned" |
| `sources` | `!hasAction("knowledge.query")`; `capabilities.list`; `authoring.assets` | "Sources & retrieval is planned" |
| `tools` | `!hasAction("tools.list")` | "Tools is planned" (T13 named in prose) |
| `usage` | `!hasAction("metrics.usage")`; `billing.summary`, `inference_usage.report` per section | "Usage is planned" |

## 4. The three non-manifest conditions

**`measure.js:57`** — the instruments list is filtered by `hasAction`, and an empty result renders
"Measure is planned". The app does the same: `apps/web/src/screens/measure.rs:51-53` returns an
empty vector when `metrics.usage` is absent. **Match.**

**`copilot.js:56-62`** — the screen reads `/api/copilot/actions`; when the route is absent it names
the S10 slice and says it will not invent a conversation. The app has a live chat transport
(`apps/web/src/screens/copilot.rs:3`, `POST /api/copilot/conversations`). **Deliberate divergence,
sanctioned by `GEMBA-WALK-SPEC.md` §15**, which lists chat as a non-goal for the walk. It is recorded
here so it is a decision, not an omission; if the owner ever lifts that non-goal, this is the file and
the line that changes.

**`sources.js:162-164`** — an empty asset read names T16/T17 in prose. The read itself is gated on
`authoring.assets` and the app's `sources.rs:41-45` performs the same empty read. **Match**; the slice
names are prose, not a promise.

## 5. The empty-read convention

`apps/web` substitutes an empty list when an action is absent or unauthorised:
`measure.rs:51-53` (`metrics.usage`), `sources.rs:32-34` (`capabilities.list`) and `:41-45`
(`authoring.assets`), `plan.rs:41-52` (`wbs.list`, `baselines.list`, `registers.list`). The walk does
the same in nine destinations (listed in §2). This is **parity, not a defect**: the walk's rule is
"the same read contract as `apps/web`", and a divergence toward a stricter gate would be a visible
difference between the two clients rather than an improvement. It is recorded so the next session
does not re-litigate it.

## 6. Findings

| # | Finding | Class | Owner |
|---|---|---|---|
| F1 | The walk's copilot lists actions and does not carry the app's chat transport | deliberate, SPEC §15 non-goal | recorded here |
| F2 | S1's board screen lacks seven write verbs the app carries: `cards.unarchive`, `cards.add_label`, `cards.remove_label`, `lists.update`, `lists.move`, `custom_fields.set_value`, `custom_fields.clear_value` | measured gap → follow-on | **#912** |
| F3 | `/knowledge/sources` renders a fabricated title, submits an unbound query when `capabilities.list` is absent, and sends a nested `page` the actions ignore | measured defect → defect unit | **#910** |

F2 and F3 are follow-ons on the owning wings; S4 edits no sibling module. Both are linked above and
carry their own Definition of Ready.

## 7. Compliance statement

Every non-live state the walk renders is either (a) a runtime manifest check, which cannot drift from
the app because both read the same manifest, or (b) one of the three conditions in §4, each compared
with `apps/web` at the sha and either matching or recorded as a spec-sanctioned divergence. No slice
gate is unowned, and no destination claims a surface the server does not have. The two S4 routes
(`/boards`, `/card/:card_id`) have no non-live state; their writes are `hasAction`-gated.

## 8. Re-running the audit

```bash
git grep -n 'is planned\|not in this role' -- apps/walk/src/destinations
git grep -n 'has_action' -- apps/web/src/screens
python3 scripts/build-walk-coverage.py --check --gate=P6 --report apps/walk/test-results/walk-report.json
```
