# inference-router — captured contract

`openapi.lock.json` is the OpenAPI 3.1 document the live service served when the
absorption started (26 paths, 35 schemas). `manifest.json` records when it was
captured, the deployed image digest and the lock's own sha256.

The lock is the parity baseline:

- the Rust service serves the same document and proves it with `--check-lock`;
- the golden replay and the shadow diff use this document's paths and schemas.

Re-capture only when the live service changes on purpose, and explain the diff in
the same commit:

```sh
./capture.sh --live http://<host>:<port> --image-digest sha256:<digest>
```

`capture.sh` needs `curl`, `jq` and `shasum` (or `sha256sum`). The service address
is supplied at capture time and is never stored in this tree.

## The oracle — reproducing the capture

`capture.sh` talks to a *running* service. The service the goldens come from is the
**pinned old image**, booted as a throwaway stack by `oracle.sh`:

```sh
export ORACLE_REGISTRY=<private-registry-host:port>   # the pinned image is not public
./oracle.sh                      # boots postgres:18 + redis + the stub + the pinned image
./oracle.sh --capture            # ... and runs capture-goldens.sh against it
./oracle.sh --down               # remove everything (a non---keep run does this itself)
```

The image is pulled **by digest** from `manifest.json`, from the registry the caller names
in `ORACLE_REGISTRY`; no registry host is recorded in this tree. Contributors without
access to that image can still run the parity harness against a candidate — the committed
goldens are the oracle's recorded answers. The database starts empty and
the image migrates it to alembic `0016` in its own lifespan, exactly as a fresh
production replica does. `--policy <doc>` seeds a tenant inference policy before the
capture; `--ledger-out` / `--cost-out` record the oracle's own `inference_call` row
and `billing:costs` stream entry for one request.

`golden/ledger_*.json` and `golden/cost_*.txt` are those artifacts with only the
genuinely volatile fields removed (clock, duration, per-request uuid). They are the
parity targets for the Rust write path; `request_sha256`/`response_sha256` are kept
because they are deterministic for a fixed request. Re-running the launcher from a
clean state reproduces every one of them byte-for-byte.

⚠️ `stubs/oracle-providers.yaml` carries `local-llm: stub-model`. A tenant policy's
platform hop is required to name the reserved model `local-llm`, and the router
matches a **concrete** requested model against the hop's own model, so without that
alias no policy-routed request can be captured at all (it is refused `400`
`outside_policy`). The alias adds one row to the `models` and `provider_models`
goldens, which is the only difference from the S9a capture.

## The shadow — old vs candidate on a recorded request window

`shadow.sh` replays a window of completion requests into the pinned old image and the
Rust candidate, both behind the deterministic stub, and diffs the answers. The two
replay targets are separate stacks built by `oracle.sh` and `candidate.sh`; the stub is
stdout-deterministic, so two instances of it return the same body for the same request.

```sh
# a corpus is the captured window, one JSON object per line, or built from the goldens:
./shadow.sh --from-golden golden --candidate "$CANDIDATE" --self-test
# the cutover form (a real window; see the capture requirements below):
./shadow.sh --live "$INFERENCE_LIVE" --candidate "$CANDIDATE" --since 2026-11-13T00:00:00Z
```

- `--live` fetches `/api/internal/inference-calls` and each `/{id}/capture`. The live
  service stores a body **only** when the caller sends `X-Capture: full` **and** its
  `X-App-Id` is on `INFERENCE_CAPTURE_ALLOWED_CALLERS`; a streamed row never captures.
  The corpus is written to a temporary directory and is never committed — it can hold
  live prompts. Set `INFERENCE_ROUTER_INTERNAL_SECRET` and `CAPTURE_APP_ID`.
- The requested model is rewritten to `stub-model` unless it is already a stub alias:
  the live catalog does not exist beside the stub. `model_rewrites` in the report says
  how many cases this touched.
- `shadow-report.json` carries `cases / equal / tolerated / unexplained` and one row per
  case: `sha256(request)` and the differing JSON pointers. It never carries a body, a
  prompt, a tenant or a call id.
- Exit codes: `0` unexplained=0, `1` unexplained>0, `2` usage/transport, `3` no cases.
  A vacuous zero is not a pass.
- `--self-test` re-diffs the first case with one synthetic field added and fails unless
  the comparator flags it.

⚠️ The hermetic part of this file is proven; a **live** window needs the capture gate
armed and callers that opt in. The first `#44` attempt measured zero captured bodies
across 37,938 live rows, which is why the live run is an acceptance-blocked step of the
cutover issue rather than a merge blocker here.

