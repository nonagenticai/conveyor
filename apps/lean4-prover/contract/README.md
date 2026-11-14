# The captured lean4-prover contract

Two artefacts, one rule: **the capture comes from the deployment that serves traffic, never from
this crate.** A lock regenerated from the rewrite cannot fail for the reason `--check-lock` exists.

| file | what it is |
|---|---|
| `openapi.lock.json` | the OpenAPI 3.1 document the live deployment serves, normalised with `jq -S` |
| `manifest.json` | the provenance: capture time, source commit, image digest, operation and schema counts, and `dropped` (empty — nothing is dropped) |
| `golden/` | request/response pairs captured from the same deployment |

Regenerate with the committed scripts, never by hand:

```sh
contract/capture.sh --live http://HOST:PORT \
  --image-digest sha256:… --source-commit 3492708
contract/capture-goldens.sh --live http://HOST:PORT
```

No address is stored in the tree; the URL is supplied at capture time (the public repository is
never a fixture target, so no capture names a host).

## What the goldens cover, and what they cannot

The captured deployment provisions **no licence token**, so the goldens are the *unlicensed
caller's* view: the health probes, `/metrics`, and the three compute routes' `401` refusal. A
successful `200` from `/api/check`, `/api/elaborate` or `/api/prove` cannot be captured without
minting a licence; fabricating one would be a hand-written fixture wearing a capture's name. Those
three operations are named in `DECLARED_UNCAPTURABLE` in `tests/parity.rs`, with the reason, and
their success shape is proved by `tests/http_contract.rs` against a stub toolchain.

`tests/parity.rs::every_route_is_replayed_or_declared` asserts the union of the goldens and the
declarations covers `ROUTES` exactly — a new route with no golden, or a declaration whose route no
longer exists, fails the run.

## Tolerance list

| field | rule | why |
|---|---|---|
| `lean_version` | presence only, value not compared | it is whatever elan reports where the process runs; the deployment reports `""`, a workstation reports a version string |
| every `/metrics` value | family names only (plus status and content type) | counters move between any two requests, including the capture's own |

No other field is tolerated. If a capture starts differing somewhere else, that is a contract
change and it belongs in a re-capture, not in this table.

## The deployable is not here

`apps/lean4-prover` carries the **crate**. The image, the 10.6 GB Lean/Mathlib toolchain layer and the
node pin stay with the deployment until #83 decides otherwise (ADR 0006 §3), so no `Dockerfile` is
committed here: the old repository is private and names a private registry base, and a second build
definition in this workspace would be a second truth about the same image.
