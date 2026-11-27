# W1 — the Gemba Walk client and renderer spike

**Run:** [45953201576](https://github.com/nonagenticai/conveyor/actions/runs/45953201576) ·
`issue/813-walk-g0` at `ce63d718f` · 2026-11-27 · **conclusion: success** (5/5 tests, 6.1 min)

**Environment:** GitHub Actions `ubuntu-latest`, Chromium `158.0.8127.18`, 4× CPU throttling,
Slow 4G emulation (1.5 Mbps down / 750 kbps up / 150 ms), the real `conveyor` server with
Postgres 18 and Redis, the `--dev-seed` tenant `dev-a` as `dev:alice` (admin). The reference
scene is SPEC §12's: six rooms, twelve figures, labels, one moving figure.

**Artifacts:** the run's `walk-spike` artifact (`results.json` + logs), copied here as
[`walk-spike-results.json`](walk-spike-results.json). The spike code and its workflow were
deleted after this run; this record and the results are what survive.

## What the spike proved

| # | Contract | Observation | Evidence |
|---|---|---|---|
| 1 | Dev session | `/api/me` returned Alice, active tenant `dev-a`, role `admin` | `proofs.me` |
| 2 | Auth-code + PKCE S256 | A 43-char verifier, a 43-char S256 challenge, `state`, and a `redirect_uri` of `http://127.0.0.1:8932/walk/`; the authorize URL is `<issuer>/protocol/openid-connect/auth` with `response_type=code` | `proofs.pkce` |
| 3 | The identity limit | The CI `/config.json` has `issuer: ""` and `dev_auth: true`, so the **real** issuer redirect was not exercised; the `/walk/` redirect URI remains the external prerequisite | `proofs.config`; ADR 0021 consequences |
| 4 | Schema sheet | `actions.describe` rendered forms for `programmes.list` (read; `cursor, limit`), `programmes.update` (write; `id, name, owner_user_id, status`) and `cards.archive` (destructive; `id`), with the effect/confirm/tenancy shown | `proofs.forms`; the form DOM |
| 5 | Read | `programmes.list {limit: 200}` answered `{items: []}` — the empty state is a valid read; the dev seed has no programmes | `proofs.readItems` |
| 6 | Write + `Idempotency-Key` | `cards.create` carried a UUID `Idempotency-Key` and created the card seen in the refusal target | `proofs.createHeaders`, `proofs.createdCard` |
| 7 | Write + `If-Match` | `cards.move` carried `If-Match: 1` from the card's version and moved it from intake to active | `proofs.moveHeaders` |
| 8 | The destructive gate | `cards.archive` with no approval was refused `403 approval_required`, naming the target `card/<id>/archive` in `approval_id`; no optimistic mutation | `proofs.refusal` |
| 9 | SSE over fetch | A `fetch` + `ReadableStream` subscription on `board:<id>` received four frames: three `card.created` replays and the `card.moved` caused by the write | `proofs.sseFrames`, `proofs.sseTypes` |
| 10 | The scoped deep link | `/walk/portfolio/deep-link` returned the spike's own entry, not another app; the dev server's `GET /walk` 308, asset 200 and missing-asset 404 were also exercised locally | `proofs.deepLink`; `spike/walk/dev-server.mjs` (deleted) |
| 11 | Static copy | The suite ran from the static `spike/walk` tree with no server-side rendering and no build step | the workflow's static dev server |

## The renderer measurement (ADR 0023)

Five cold runs per renderer; the median with min–max. Both renderers draw the same scene, so the
comparison is the delivery, not the picture.

| Metric | canvas-2D | WebGL2 |
|---|---|---|
| Available | yes | yes (SwiftShader on the runner) |
| Frame p50 | **16.7 ms** (16.7–16.7) | **16.7 ms** (16.7–16.7) |
| Frame p95 | **16.8 ms** (16.7–16.8) | **16.7–16.8 ms** (16.7–16.8) |
| Long tasks | 0 | 0 |
| Cold load (median) | **1 269 ms** (1 261–1 282) | **1 267 ms** (1 264–1 281) |
| JS transfer | **24 388 B**, 9 requests | the same (no extra asset) |
| Heap after 5 s (median) | **6.0 MB** (3.1–8.9) | **8.9 MB** (4.2–13.8) |
| 5-minute soak (canvas-2D) | start 2.54 MB → after 20 room changes 2.71 MB → after the hold **1.93 MB** | — |

**Reading:** both renderers are vsync-capped at 60 fps on the reference scene, so the frame metric
is a tie; canvas-2D has the smaller heap, needs no WebGL2/GPU, and is the smaller implementation.
The soak shows no growth (the final heap is below the start). The wasm/ECS budgets — ≤ 600 KB gzip,
≤ 1 ms fixed tick, ≤ 5 000 entities — are **not** measured here; they belong to `crates/walk-world`
and its W1d world spec. The runner is a Linux GitHub runner, not the M-series reference laptop; the
protocol and machine class are recorded so a repeat is comparable.

## What this closes, and what it does not

- Closes the P0 client-contract gate: no unproven assumption (dev session, PKCE construction,
  schema forms, headers, refusal, SSE, deep link) enters P1.
- Closes the renderer question: **ADR 0023 selects canvas-2D**; WebGL2 remains a candidate behind
  the `WorldView` port, not a dependency.
- Does **not** prove production sign-in: the issuer is empty in the dev environment and the
  `/walk/` redirect URI is an external registration. That scenario stays `acceptance-blocked-by`.
- Does **not** prove the wasm/ECS budgets; W1a–W1d and the walk workflow will.
- Does **not** replace the W2 foundation's real Portfolio write: the dev seed's programmes are
  empty, so the write proof here is the card create/move. The foundation creates or picks a
  Portfolio row and records its action.

## Links

- ADR 0023 — [`docs/decisions/0023-gemba-walk-renderer.md`](../decisions/0023-gemba-walk-renderer.md)
- SPEC §6.3, §9, §12 · ECS §12–§13 · Plan §7 P0, §8 W1 · WBS W1
- Issue #813 · ADR 0022 (the simulation) · ADR 0021 (the client)
