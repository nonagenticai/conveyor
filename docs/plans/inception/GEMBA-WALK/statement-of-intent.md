---
document: GEMBA-WALK/statement-of-intent
category: statement-of-intent
project: GEMBA-WALK
status: draft
---

# GEMBA-WALK — Statement of intent

## Intended use

The Gemba Walk is a secondary, game-style client of the conveyor, served at
https://game.nonagentic.ai/ with its own `conveyor-game` Keycloak client. It reads and writes the
same records as the operator console; it is not a second backend or a decision store.

## Ownership

The GEMBA-WALK programme project owns the walk. `apps/web` remains normative.

## Measures of effectiveness

- 56/56 census routes reachable.
- The P6 gate green.
- The soak within 1.5× the warm baseline (measured 1.059× over 702 visits).
- The accessibility budgets met.
- Sign-in through `conveyor-game`, proving the live path.

## Links

- Issue: #817
- Plan: `docs/plans/GEMBA-WALK.md`
- Register: GW-A8 and GW-A9 in `03-artifact-register.md`
- Evidence: `docs/evidence/walk-gate-audit.md`, `docs/evidence/walk-a11y.md`,
  `docs/evidence/walk-perf.md`
