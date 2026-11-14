# The captured contract

`openapi.lock.json` is what the old Python service served, filtered by `drop.jq`, normalised with
`jq -S`. It is the **target**: `--check-lock` builds the document from this implementation and
compares it to the capture, so the check can fail for the reason it exists.

Nothing here is regenerated from the rewrite.

| File | What it is |
|---|---|
| `openapi.lock.json` | the contract the service must serve (17 paths, 18 operations) |
| `manifest.json` | when it was captured, from which commit and image digest, the lock's sha256, and every path the filter dropped |
| `drop.jq` | the surfaces this rewrite does not carry, with the reason for each |
| `capture.sh` | captures the lock from a running old service |
| `run-local.sh` | boots the old service with the dead graph/discovery clients stubbed (`ENVIRONMENT=development`) |
| `capture-goldens.sh` | captures synthetic request/response pairs into `golden/` |
| `tolerances.toml` | every declared float tolerance and every ignored field |

## What is not carried, and why

Two tests, one decision, applied the same way: an operation whose only data source no longer exists,
or whose runtime already exists as another conveyor tool, is not ported.

- **The graph surfaces** — `/api/topology/*`, `/api/explain/*`.
- **The graph-dependent operations** — `verify/cove` and `verify/integrity` (the discovery engine and
  the graph adapter), `pipeline/trigger|webhook|batch` (they ask the graph which entities to read).
  The store was destroyed on 2026-11-01 and both backends are gone.
- **The sandbox** — `sandbox/execute|jobs|results` would be a second code-execution runtime; the
  conveyor already dispatches execution through the coding service.

No live consumer exists for any of them: the Deployment has been at `replicas: 0` for 188 days.

## Goldens cannot cover everything

A golden is the old service's own answer to a fixed input, which is only meaningful when the answer
is a function of that input. The ten routes that call a model, a prover or an object store are
therefore proven by contract tests in their modules instead, and `tests/parity.rs` names every one of
them so the gap cannot widen silently.

## Reproducing the capture

The old service cannot start in production mode: its settings require a graph-adapter URL and a
discovery-engine URL, and both backends are gone. `run-local.sh` sets `ENVIRONMENT=development`,
which makes those two requirements non-fatal, and boots it far enough to serve its own document.

```sh
SCIENTIFIC_PROCESSOR_SRC=<checkout> ../contract/run-local.sh 8899 &
../contract/capture.sh --live http://127.0.0.1:8899 --source-commit <sha> --image-digest <digest>
../contract/capture-goldens.sh --live http://127.0.0.1:8899
```

The address is never stored in the lock or the manifest.
