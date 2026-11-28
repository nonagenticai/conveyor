# The walk's accessibility record (W7)

Measured 2026-11-28 for S4 (#817) against `apps/walk` at its head, using the documented local
harness (the published server image + Postgres 18 + Redis, `DEV_AUTH_TOKEN=dev:alice`). The browser
is Playwright's chromium; every number below is printed by the specs, not estimated.

## Protocol

```bash
cd apps/walk
PLAYWRIGHT_CHANNEL=chromium DEV_AUTH_TOKEN=dev:alice npx playwright test e2e/a11y
```

- `e2e/a11y/keyboard.spec.ts` — the skip link, a room sheet, Escape, focus return.
- `e2e/a11y/layout.spec.ts` — the canvas as an image and out of the tab order; the palette as a
  keyboard path; 390 px and 200 % zoom with no horizontal scroll; the 44 px control target; the
  contrast ratio of ink on paper.

## Results

| Budget (`GEMBA-WALK-SPEC.md` §8) | Measured | Verdict |
|---|---|---|
| Every destination reachable by keyboard alone; the skip link precedes the HUD | `#screen` receives focus from the skip link; the palette opens from its button by Enter; Escape returns focus to the opener | met |
| The canvas is `role="img"`, named, and not focus-trapped | `role="img"`, `aria-label="Walk world"`, `tabIndex = -1` | met |
| Sheets manage focus (capture, escape, return) | focus is contained on open, Escape closes, focus returns to the opener | met |
| 390 px width: no horizontal page scroll | 0 px overflow | met |
| 200 % zoom: no horizontal page scroll | 0 px overflow (measured with the root at `zoom: 2`) | met |
| Controls at least 44 px | every visible control measured 44 px | met **after a fix** (below) |
| WCAG 2.2 AA contrast | heading **12.9:1**, body **6.1:1**, primary control **12.9:1** | met (AA needs 4.5:1 body, 3:1 large text) |

Raw annotations from the run:

```
a11y.contrast={"heading":12.889,"body":6.146,"button":12.889}
a11y.controls= every control 44 px (the full list is in the run's JSON report)
```

## The fix this record required

The first measurement found every control under the budget: `.button` **37 px**, `.map-room`
**34 px`, `.palette-row` **36 px** — no minimum height in `apps/walk/style.css`. The SPEC's 44 px is
not optional, so S4 adds `min-height: 44px` to those three control classes and nothing else. This is
a design-system change with a global effect, declared here and in the issue so it is reviewable: it
changes no layout logic, only the minimum size of a control. The same spec asserts it from now on,
so a regression fails CI rather than a later audit.

## The one promise not met

The Palette button's `title` reads "Search routes and actions (Ctrl/Cmd + K)", and **no handler for
that shortcut exists anywhere in the client** (`grep -rn 'KeyK\|metaKey\|ctrlKey' apps/walk/src`
returns nothing). The palette is still a keyboard path — Tab and Enter reach it, proved above — but
the advertised shortcut is a false promise. The shell is a parent-owned seam, so this is defect unit
**#914**, not an S4 edit; its spec case lands with that unit.

## Re-running

```bash
cd apps/walk && PLAYWRIGHT_CHANNEL=chromium DEV_AUTH_TOKEN=dev:alice npx playwright test e2e/a11y
```

The `walk` workflow runs the same specs on every PR and on every push to `main`.
