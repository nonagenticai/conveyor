# The captured contract

`openapi.lock.json` is what the old Python service served, filtered by `drop.jq`, normalised with
`jq -S`. It is the **target**: `--check-lock` builds the document from this implementation and
compares it to the capture, so the check can fail for the reason it exists.

Nothing here is regenerated from the rewrite.

| File | What it is |
|---|---|
| `openapi.lock.json` | the contract the service must serve (25 paths, 26 operations) |
| `manifest.json` | when it was captured, from which commit and image digest, and the lock's sha256 |
| `drop.jq` | the two graph surfaces this rewrite does not carry, with the reason |
| `capture.sh` | captures the lock from a running old service |
| `run-local.sh` | boots the old service with the dead graph/discovery clients stubbed (`ENVIRONMENT=development`) |
| `capture-goldens.sh` | captures synthetic request/response pairs into `golden/` |
| `tolerances.toml` | every declared float tolerance and every ignored field |

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
