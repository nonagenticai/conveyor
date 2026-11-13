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

