# inference-router absorption — evidence (2026-11-13)

Issue #44 · head `7c139b8` · branch `issue/44-inference-router`. The reference service is pinned
by digest; no private hostname, slug, address or path appears in this file.

## The contract

| Fact | Value |
|---|---|
| Contract capture | `contract/manifest.json` — `captured_at` 2026-11-13T10:20:46Z |
| Pinned image | `sha256:723671a96af98111f16430ef4934370900962c04b1b6793f178d7a65ff70c58f` |
| Lock sha256 | `99953f1d681e53b7fadd6a5c9afa84251f7914fdd53185316efa9ccbe430fa78` |
| Served document | `--check-lock` → `lock OK: 27 operations` (26 paths in the captured document; the service registers 27 operations across them) |

The lock was captured from the running service and re-captured from the pinned image by
`contract/oracle.sh`; the S9a capture recorded on #44 measured the oracle's normalised
`/openapi.json` hashing to the lock's sha256.

## Parity — the three layers

### 1. Golden replay (hermetic, every case)

`contract/verify.sh --candidate <local>` replays the 30 recorded cases through the stub:

```
cases=30 unexplained=0
```

Every response matches after the shared normaliser; the one tolerance is named with its reason
(`contract/tolerances.json`), and `verify.sh` refuses a tolerance with a blank reason.
`jq -e '.tolerances[] | select(.reason == null or .reason == "")'` returns no rows.

### 2. Shadow replay (hermetic, the completion corpus)

`contract/shadow.sh` replays a recorded request window into the pinned old image and the Rust
candidate, both behind the same deterministic stub, and diffs the normalised answers.
`contract/shadow-report.json` is the committed report — hashed case ids only, no bodies:

```
cases=9 equal=9 tolerated=0 unexplained=0
```

Those 9 are every `POST /api/chat/completions` case in the golden set, replayed across the wire
rather than parser-to-parser. `--self-test` re-diffs the first case with one synthetic field added
and fails unless the comparator flags it; it flagged it.

The `--live` form (`--live URL --candidate URL`) was exercised end to end against a local
capture-enabled instance: one recorded call was fetched through
`/api/internal/inference-calls/{id}/capture` and replayed — `cases=1 equal=1 unexplained=0`. The
live **production** window is not obtainable today; see below.

### 3. The live window — measured, deferred to #71

Measured 2026-11-13 against the running service and its database:

| Measurement | Value |
|---|---|
| `inference_call` rows | 37,938 |
| Rows with a captured body (`request_json`/`request_text`) | **0** |
| Most recent row | 2026-11-08 17:21 +01 |
| Pod logs in the last 24 h | none |

The capture gate is not the allowlist alone: a body is stored only when the **caller** sends
`X-Capture: full` **and** its `X-App-Id` is on `INFERENCE_CAPTURE_ALLOWED_CALLERS`, and a streamed
row never captures. No live caller sends the header, and there has been no completion traffic since
2026-11-08. Arming the allowlist therefore records nothing.

Owner decision, 2026-11-13: #44 closes on the hermetic evidence above. The live window becomes an
acceptance-blocked scenario of the deployment sibling **#71**, where capture can be armed and the
traffic is actually being switched. This is recorded on #44 P3.2; no synthetic substitute is
claimed as a live window.

## The defect this work found

The port declared `X-Capture` on the completions route and served the capture read, but **nothing
ever wrote the capture columns**. After cutover the capture route and the replay surfaces it feeds
would have been dead — silently, because the read answers `404 not_captured`.

It was found by building the shadow harness and recording a call through it: the call answered
`captured=false` on a live process while the source looked complete.

Fixed at `7c139b8`:

- `apps/inference-router/src/ledger.rs` — the row now carries `request_json` (the fingerprint plus
  `model`), `response_json`, `request_text` and `response_text`; the insert writes them. The
  stored request minus `model` re-hashes to `request_sha256`, which is what the read's
  `request_sha256_verified` checks.
- `apps/inference-router/src/router/completions.rs` — `capture_allowed`: `X-Capture: full` from an
  allowlisted `X-App-Id`. A refused capture serves the request normally and stores hashes only.
  A streamed row never captures (the reference forces it false on that path).
- `apps/inference-router/tests/capture_write.rs` — 3 tests: the allowlisted store→read round trip
  (both integrity flags true), no header stores nothing and the read answers `404 not captured`,
  and an unlisted `X-App-Id` stores nothing.

The success path is not in the golden set: the captured `internal_capture` case is the 403 refusal,
and the oracle harness cannot arm its allowlist yet. The behaviour is pinned by the tests against
the reference's source, and the limitation is stated rather than hidden.

## What each #44 scenario shows

| Scenario | Result | Where |
|---|---|---|
| P1.1 capture the document | lock served by the pinned image; `--check-lock` green | `contract/manifest.json`, `--check-lock` |
| P1.2 synthetic/scrubbed goldens | `no-private-refs.sh` OK over 1,382 tracked files | this head |
| P2.1 served document equals the lock | `lock OK: 27 operations` | `7c139b8` |
| P2.2 migrations equal the live schema | `schema OK: the migrations produce the live schema` | live dump, this head |
| P2.3 package tests | 103 lib + 13 admin_auth + 10 admin_policy + 3 capture + 4 catalogue + 4 internal_guard + 7 internal_reads + 6 migrations + 13 replay_canary + 12 write_path; 0 failed | `7c139b8` |
| P3.1 golden replay | `cases=30 unexplained=0` | `7c139b8` |
| P3.2 live window | harness proven (9/9, self-test fails on a mutation); live leg acceptance-blocked by #71 | `7c139b8`, #71 |
| P3.3 tolerances carry reasons | no rows | `7c139b8` |
| P4.1/P4.2 cutover + rollback | moved to #71 | #71 |

## Consumers and their state

| Consumer | Use today | Cutover state |
|---|---|---|
| `fr` | replay invocation-log read over `INFERENCE_ROUTER_URL` | still the live Python service |
| the conveyor server | `CONVEYOR_EXTERNAL_INFERENCE_URL` | not deployed in the cluster yet |
| the agent platform | completions (named in #44) | still the live Python service |

No consumer is switched by this issue. The cutover, its per-consumer order and the rollback are
#71's.

## Cutover and rollback

Recorded in #71, not here: this issue ships no deployment and no production call. The one-flag
rollback is `CONVEYOR_EXTERNAL_INFERENCE_URL` back to the old service, with no code change.
