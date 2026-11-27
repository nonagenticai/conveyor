# U6 — the benefit inventory and the instruments

Measured 2026-11-27. This is slice 1 of #853: the per-profile inventory, the instruments for every
gap, and the first-transition design. It claims no live write.

## Method

- The live snapshot: `git_sha 54e1ffcaa`, generated 2026-11-27T14:00Z, tenant `nonagentic`
  (`manifest.json`, `msp.json`).
- `seed/programme-definition.json` at main@746f5a90f (blob `0f3756ea`) — the 7 seeded benefits and
  8 map links.
- The staged record on `issue/526-u5a`: 20 `benefits.update` journal entries, the baseline inputs
  for B1/B2/B4/B5/B6, and the profile updates. They land with #526; nothing below duplicates them.
- `docs/evidence/walk-coverage.json` at main@746f5a90f (blob `3db1357a`) — B8's instrument record.
- The role standard (`.claude/skills/benefits-manager`): **six mandatory groups** — identification
  and purpose, performance (KPI, baseline, target, method), cost, conditions (capabilities,
  preconditions, operating-practice change), uncertainty and dependencies, responsibility and
  attribution (BCM owner, operational area) — plus the map path. The BCM owns measurement; the
  SRO sets the target and signs the realization.
- The audit's narrower rule (`scripts/audit-programme-conformance.py`, `benefits()`): `target`,
  `measurement_method`, `operational_area`, an owner job, a benefits-map edge; the baseline must be
  a record of `value`, `unit`, `measured_at` and `method`.
- **Zeros below are counted, never assumed.** A measured zero carries the method and the date that
  prove the absence; where no instrument can observe a value, it is reported `unmeasured` and no
  transition or realization is recorded (the role's refusal). The audit has no `baseline_status`
  escape, so a counted zero is the honest representation of an observed absence.

## The profiles (live)

| Ref | Description | Target | Method | Area | Owner | Map edge | Six groups | Baseline (live) |
|---|---|---|---|---|---|---|---|---|
| B1 | Auditable decisions | present | present | **missing** (staged on #526) | present | T2, T18 | **empty live; staged on #526** (objectives, KPIs, capabilities, cost, risks, preconditions) | placeholder string; staged 0.15861 decisions/run (105/662 at 2026-11-22) |
| B2 | Less bespoke tooling | present | present | present | present | T13 | complete (63 capabilities, 51 risks) | placeholder; staged 18 tool_call rows (2026-11-22) |
| B3 | Self-development | present | present | present | present | T6 | complete | placeholder; no instrument recorded |
| B4 | Public collaboration | present | present | present | present | DESIGN-PUBLISH | complete | placeholder; staged 0 outside contributions (2026-11-22) |
| B5 | One model | present | present | present | present | T0 | complete | placeholder; staged 1 model store (2026-11-22) |
| B6 | Predictable delivery | present | present | present | present | P0 | complete | placeholder; staged 28 open register findings (2026-11-22) |
| B7 | Deliberated cross-department decisions | present | present | BCM | present | T19 | **empty** (objectives, KPIs, capabilities, cost, preconditions, risks, observable outcome) | measured 0 meetings (2026-11-24) |
| B8 | UI-independent action contract | present (dict, value 52) | present | **missing** | present | **missing** | **empty** | dict with `value`/`measured_at` null; measurable today (0 of 52) |

`financial_value` is empty on every profile; the role standard says it is recorded "where
expressible", so an empty value is not a gap.

## The gaps, and what closes each

| Gap | Closes by | Where |
|---|---|---|
| B1 six groups and `operational_area` | The staged #526 update | #852 promotion; verify after |
| B1/B2/B4/B5/B6 placeholder baselines | The staged #526 baseline records | #852 promotion; verify after |
| B3 baseline | New count: **0** self-development runs reached live verification, method naming the unbuilt `dev.*` family | this unit, after #852 |
| B7 six groups | Author objectives, KPIs, capabilities, cost, preconditions, risks and the observable outcome around the existing target | this unit |
| B7 status `draft` | Activate at the transition | this unit, with the transition |
| B8 six groups | Author the same six groups around the existing target | this unit |
| B8 `operational_area` | Set to `the conveyor programme` through `benefits.update` | this unit, after #852 |
| B8 map edge | A `benefits.link` to the Gemba Walk project (the second client) | this unit, after #852 |
| B8 baseline | `value 0`, `unit destinations with a passing spec`, measured 2026-11-27 | this unit, after #852 |

## The instruments

Each instrument reads rows or committed records; none writes.

- **B1 — decisions per run.** `count(approval_decision) / count(run)` at one snapshot. Staged value:
  105/662 = **0.15861** at 2026-11-22. Re-read the numerator and denominator after the promotion.
- **B2 — tool calls per toolkit.** `select toolkit_id, count(*) from tool_call group by toolkit_id`;
  zero-use is a toolkit with 0 rows in the window (`tools.usage` already computes `zero_use`).
  Staged value: **18 tool_call rows** at 2026-11-22.
- **B3 — self-development runs reaching live verification.** Count `run` rows of the development
  procedure that carry a live-verification receipt. Today the `dev.*` action family is unbuilt
  (`v1-p91-1`) and no deploy/live-verify gate exists (`v1-p93-2`), so the count is a **measured 0**
  at 2026-11-27; the method names both findings, which is what makes the zero observed rather than
  assumed. The instrument becomes the run row plus the deploy/live-verify evidence once the dev
  family lands.
- **B4 — outside contributions.** Merged PR authors from the GitHub issue registry; outside = an
  author other than the owner or the automation. Staged value: **0** at 2026-11-22.
- **B5 — one model store.** The T0 ownership-matrix check; a second home is a defect. Staged value:
  **1** model store at 2026-11-22.
- **B6 — open register findings.** `check-registers.py` at one snapshot. Staged value: **28** at
  2026-11-22.
- **B7 — meetings per cross-department decision.** Read-time aggregates over `meeting`,
  `meeting_agenda_item`, `meeting_attendance`, `meeting_position`, `approval_decision` and
  `project_dependency`; a metric reads rows and writes nothing. Measured value: **0** at
  2026-11-24, with the method recording that the pre-T19 record holds approval rows only.
- **B8 — walk destinations with a passing spec.** `scripts/build-walk-coverage.py --check` and the
  committed record's summary: at main@746f5a90f, `routes_total 52`, `registered 3`, `verified 0`,
  `absent 49`. Measured value: **0** of a target 52 at 2026-11-27.

## The first transition

Two candidates, both real; the programme owner (SRO) picks one before the live slice. This unit
prepares the evidence and the recommendation; the BCM/SRO signs, never the builder.

**Recommended — B7 with T19.** The baseline (measured 0 meetings, 2026-11-24) predates the T19
forum; the enactment is T19 going live (the T19F live gate, 13/13, 2026-11-27); the post-measurement
is the first real cross-department meeting with a position per required department, read from the
typed rows. Evidence: the meeting id, the positions, the decision. This is the only benefit whose
baseline predates its capability and whose post-measurement is a governed act.

**Alternative — B2 with T13's tool retirement.** Baseline 18 tool_call rows (2026-11-22); the
enactment is `tools.retire` unbinding a zero-use tool under an owner decision (`v1-p119-1`, landing
in the tranche-3 unit); the post-measurement is the tool_call count and the retired row. It waits on
`v1-p119-1`.

In both cases the transition row carries `baseline_measured_at` < `enacted_at` < `post_measured_at`,
both values, and non-empty evidence; `benefits.get` and `benefits.transition_list` are the
read-backs. `realized` is written only with the BCM/SRO approval, and the engine refuses it while
either measurement is missing.

## What lands with #526 and is not duplicated here

The journal's 20 `benefits.update` entries carry: B1's six groups and `operational_area`, the
B1/B2/B4/B5/B6 baseline records, their preconditions, and `status: active` for B1, B2, B4, B5 and
B6. After #852, this unit re-reads every field and writes only what is missing.

## Acceptance mapping

| Check | After this unit |
|---|---|
| `benefit_profile_incomplete` (audit) | zero: B1's area lands with #526; B8's area and edge are set here |
| `benefit_baseline_placeholder` (audit) | zero: B1/B2/B4/B5/B6 land with #526; B3 and B8 are measured here; B7 is already measured |
| Six groups (role standard) | every profile's six groups non-empty by `benefits.get`; B1 staged, B7 and B8 authored here |
| `realized_without_transition` | zero: only the transition benefit is `realized`, with the three measurements and evidence |
| `transition_order` | zero: the baseline precedes the enactment and the post-measurement |
