# The delivery system that uses the conveyor

**Status:** U1 of the delivery-system rework (#756); U2 is #753; U3 (the GitHub channel) is created
when U1 merges.

## Why

Three weeks of buildout produced most of the V2 slices, but the programme that builds the conveyor
is not run on the conveyor. Progress lives in repo files and is reconstructed by Python; the roles
that perform the gated acts have no executable form; the public record carries no provenance.

Measured 2026-11-26 on `main@ba56759a`:

- `scripts/` — 21 files, 9,794 lines. Twelve are pure file transforms or validators; nine are state
  machines that reach GitHub, the cluster or the network.
- The engine already exposes every action twice — `POST /api/actions/<name>/invocations` and `/mcp`
  — both licence- and scope-gated. Sessions bypass both, because there is no non-interactive bearer
  (measured: the pinned client is public with direct grants off; the service client is
  service-account-only).
- The design site is row-driven: `msp.json` shows 6 benefit profiles with **0 transitions, 0
  baselines and 0 register items**; `architecture.json` shows 3 artefacts and 1 contract. Those
  zeros are the programme's real health.
- Output pages carry a generator meta tag only — no kind, hash, status, owner, gate or commit.

## The finding

The buildout authors progress as files and projects it **into** rows; SPEC_V2 §10.1 says the
conveyor owns those rows. That inversion is what the Python estate pays for, and every recurring
red is the same shape — two representations of one fact (seed vs rows, standards vs output pages,
registry PR vs live issue list).

## The target model — one direction per channel

| Channel | Carries | Never carries |
|---|---|---|
| Conveyor rows | every progress fact: artefact, register item, baseline, tranche review, benefit, gate | derived copies |
| Role skills | the act: read the role's card → one decision → one action → read-back → close | free-form work, verbal status |
| GitHub | intent: issue = claim, PR = code + CI evidence | programme state |
| Design site | proof: rendered rows with provenance and drill-down | hand-authored pages |

## Units

- **U1 (#756)** — render the record that already exists: a provenance block on every output page,
  and a per-project declared-vs-evidenced register. Deletes the dead half of
  `scripts/check-output-standard.py`.
- **U2 (#753, claimed elsewhere)** — six programme delivery-role playbooks and role routing.
- **U3 (created when U1 merges)** — the GitHub channel: issue form, contract check, sub-issues,
  tranche milestones, required checks, and one sync action replacing the registry commit-PR loop.

## Out of scope

The `apps/web` UI (T18 owns it; the site is the public record, the app is the working UI); Projects
V2 (a second board); any change to the Rust action layer beyond the two named additions; the V1
S-slices.

## Risks

- **Register's "declared" side** — the standard's categories are keyed by project class. If class →
  project type → tranche does not resolve from rows, the register ships per project type and the
  class rollup waits.
- **#526 overlap** — shared files in the programme actions; U3 sequences after it.
- **Committed-pack removal (U3)** — output pages stop being in git; the site's index becomes the
  provenance record. The homepage mirror must build from rows first.
