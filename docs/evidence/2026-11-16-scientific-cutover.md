# Scientific cutover — 2026-11-16, completed 2026-11-18

Part of #212. `apps/scientific` — absorbed in #105, merged and never deployed — is now the live
`scientific-processor` service. The publish row landed in #217; this document records the
deployment half, the tenant gate the service needed before it could serve, and the live checks.

## Digests deployed

| Binary | Image | Digest | Source |
|---|---|---|---|
| `conveyor-scientific` | `ghcr.io/nonagenticai/conveyor-scientific` | `sha256:d6d58c20c9aa947edbec40fa2b51eb90e3d03257597d8d3aee93d59f1bc9e6b3` | the merge-queue commit `cb0ca62` of conveyor PR #318 |

An earlier image (`sha256:7c541031…`, from `94edb9b`) existed only as the swap's first pin; it
predates the gate and is superseded. The package is public on ghcr (anonymous pull).

## Sequence

| Step | Change | Evidence |
|---|---|---|
| P1.1–P1.2 | Publish row `package: conveyor-scientific` / `bin: conveyor-scientific` / `image: conveyor-scientific`, never the legacy `scientific-processor` package that Flux tracks | conveyor PR #217 |
| P2 (swap) | Python image and the `alembic` init container removed; the Rust image digest-pinned; still 0 replicas because the port trusted a bare `X-Tenant-Id` and the namespace admits the whole pod CIDR to 8808 | deployment repo #5320 |
| P2 (gate) | The reference's tenant gate ported at its **enforce** semantics and shipped in the image | conveyor PR #318; #299 |
| P2 (activation) | Image `d6d58c20…` pinned; replicas 0 → 1 | deployment repo #5326 (`dfa4d6db`) |
| P2 (port) | `BIND_ADDR: 0.0.0.0:8808` pinned: the service image's generic ENV default is 8080 while every probe targets 8808 | deployment repo #5327 (`43770cd2`) |
| P2 (backend) | The retired embedding backend (modality-gateway, 0/0; folded into mlx-api by D57) removed from the env; the verification routes answer the reference's degraded 200 | deployment repo #5328 (`6ede450d`) |

Final: `scientific-processor` 1/1 Ready, 0 restarts; `/health` 200 and `/health/ready`
`{"ready":true,"reason":null}`.

## The tenant gate (conveyor#299)

The reference resolved the caller through licence introspection, the `Authorization` licence
carrier (enforce mode) with an on-behalf allowlist, and refused a plain header in enforce. The
reference shipped in **observe**; this port starts at the closure target, which its zero-caller
measurement allowed (a cluster-wide sweep for the service's name, its URL variable and its port
matched only its own Deployment). The port also carries no introspection cache. Both deviations
are recorded in `apps/scientific/src/auth.rs`.

Live probes (one-shot client pod in the namespace; the licence is the Deployment's own, read from
its Secret and never printed):

| Probe | Answer |
|---|---|
| no credential | `401 AUTHENTICATION_REQUIRED` — "Missing tenant context: name a tenant in X-Tenant-Id or present a license token" |
| plain `X-Tenant-Id` | `401 AUTHENTICATION_REQUIRED` — the enforce message naming `AUTH_TOKENLESS_IDENTITY_MODE=enforce` |
| `X-Tenant-Id: Bearer <licence>` | **200** — the licence's tenant, served |
| `Authorization: Bearer <licence>` | **200** — the second carrier, served |
| a foreign tenant with an unvouched licence | `403 TENANT_ASSERTION_NOT_PERMITTED` |
| credential-less introspection outage on the Authorization carrier | `503 LICENSE_INTROSPECTION_UNAVAILABLE` + `Retry-After` (pinned by the suite, not forced live) |

## P2.3 — one stats call, recorded

Request (the capture's own fixture):

```json
{"values":[1.0,2.0,3.0,4.0,5.0],"confidence_level":0.95,"method":"normal"}
```

Response, live (`POST /api/stats/confidence-interval`, licensed):

```json
{"mean":3.0,"ci_lower":1.036757,"ci_upper":4.963243,"confidence_level":0.95,"method":"normal","computation_time_ms":0.0}
```

The interval is the captured golden's (`ci_lower` `1.036757`, `ci_upper` `4.963243`); only
`computation_time_ms` differs, which the capture's tolerances already declare volatile.

## P3 — retired and archived

- **P3.1** No Deployment in the namespace runs the Python image: the container image is
  `ghcr.io/nonagenticai/conveyor-scientific@sha256:d6d58c20…` and `initContainers` is empty (the
  `alembic upgrade head` container went with the Python image).
- **P3.2** `nonagenticai/scientific-processor` is **archived** (`isArchived: true`, 2026-11-17).
  The alert gate is recorded as **unmeasurable rather than zero**: Advanced Security (code
  scanning) is not enabled on that repository (`403`) and Dependabot is disabled — the same
  vacuity the research retirement documented, so "no High alerts" means "not scanned".

## Unchanged by this cutover

The surviving 18 operations and their captured contract; the drops asserted by name (`topology`,
sandbox, `explain`) in `apps/scientific/src/contract.rs`; the existing ConfigMap/Secret names
(`SCIENTIFIC_PROCESSOR_*`, so the configuration is reused); the namespace, Service, RBAC and
NetworkPolicy; and the rule that the service has no database — nothing surviving writes a row.
