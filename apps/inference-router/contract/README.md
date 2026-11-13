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
