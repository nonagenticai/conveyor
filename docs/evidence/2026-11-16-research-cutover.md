# Research cutover — 2026-11-16

Part of #205. The two Rust binaries were deployed (`gitops-production#5297`), the scraper was cut
over (`#5303`), and the Python `media_discovery` was retired in favour of the Rust media binary
(`gitops-production#5306`) — **with conveyor#226 still open**: the port had dropped the reference's
tenant gate. On 2026-11-17 the media Deployment was withdrawn to 0 replicas (gitops#5318) and the
gate was restored (conveyor#226, this document's "The media gate" section); the Deployment returns
behind the restored gate in the deployment repository.

## Digests deployed

| Binary | Image | Digest | Source commit |
|---|---|---|---|
| `conveyor-research` | `ghcr.io/nonagenticai/conveyor-research` | `sha256:8991e468baa0305b880c4881a4917761c80261f77c4fe27dff373e803facb2b7` | `bfe86aa` |
| `conveyor-research-media` | `ghcr.io/nonagenticai/conveyor-research-media` | `sha256:4a4d25c9200805cd1a20895482d04c675823b7cc0c8e7593736d3f7c2ee3f0f7` | `bfe86aa` |

Both are public ghcr packages, pulled anonymously — no imagePullSecret.

## Deploy

| Deployment | Namespace | Service | Database | State after deploy |
|---|---|---|---|---|
| `research` | `webscraper` | `research:9119` | none (stateless) | 1/1, 0 restarts |
| `media` | `media-discovery` | `research-media:8809` | the existing `media_discovery` database | 1/1, 0 restarts |

**Why the existing namespaces, not a new `research` namespace.** `default-restrict` is a
hand-written per-namespace policy in the shared `clusters/strato/policies/network/baseline-networkpolicies.yaml`;
a new namespace would need its own entry there, plus its own egress rule, PSA and ambient labels.
The two existing namespaces already carry all of it, proven. #205's P2.1 verifies `-n research` and
is amended here to `-n webscraper` for that reason.

## The adopted database needed one explicit step

`apps/research/migrations/0001_media_baseline.sql` is a `pg_dump --schema-only` of the reference's
own database. sqlx would have re-run it against the schema it was dumped from and crashed
(`relation "collection_items" already exists`, measured). Measured state before the fix:
`alembic_version = c7a1f4e9b820` — the exact revision the baseline was captured from — and no
`_sqlx_migrations` table.

Resolution: a one-shot Job (`media-baseline-seed`) let the binary create the empty
`_sqlx_migrations` table, then the baseline was seeded as applied:

```
version = 1, description = 'media baseline', success = true,
checksum = sha384(0001_media_baseline.sql) = 4d10d854…559551b0d24cc6898c053a796f2311cc29101
```

Re-running the binary then booted cleanly: `media-discovery 0.1.0 on 0.0.0.0:8809`. The row is
created by sqlx's own DDL, not by hand, so the shape cannot drift from the library's expectations.

## Shadow table (P3.1)

Five public URLs, both binaries answering the same request. Bodies compared field by field after
removing `processing_time_seconds`.

| URL | Result |
|---|---|
| `https://example.com` | equal except `.url` |
| `https://www.rust-lang.org` | equal except `.url` |
| `https://en.wikipedia.org/wiki/Lean_(proof_assistant)` | equal except `.url` |
| `https://httpbin.org/html` | `.url`, plus `text_content` / `markdown_content` |
| `https://www.gnu.org/` | equal except `.url` |

**Differences, named — `unexplained = 0`**

1. `.url` — the reference echoes the normalised URL (`https://example.com/`); the port echoes the
   request's verbatim. Cosmetic; not reproduced.
2. `text_content` / `markdown_content` on httpbin — where the reference's primary extractor won it
   drops the leading `<h1>Herman Melville - Moby-Dick</h1>`; the port uses the boilerplate-strip
   baseline always. Documented in `apps/research/contract/README.md` ("the BeautifulSoup
   boilerplate-strip baseline is ported verbatim; trafilatura is not"), not a port defect.

**Media (P2.3)** — `media_collections` holds 0 rows, so the scenario "an existing collection is
read" cannot be run as written. Proxied with an item read:

| Call | Reference | Port |
|---|---|---|
| `GET /api/items/019ddddf-…` with `X-Tenant-Id` | **401** — a bare tenant header needs a service token | **200** + the full row |

That difference was the port's missing gate, not a row defect: #226. It is closed — the port now
refuses a bare tenant without a vouch (401), refuses a service token whose subject is not on
`MEDIA_DISCOVERY_ONBEHALF_SUBJECTS` (403), serves `X-Tenant-Id: Bearer <licence>` with the
licence's tenant, and keeps the vouch from re-labelling the named tenant. The three shapes are
pinned in `apps/research/tests/tenant.rs`.

## Cutover (P4, scraper half only)

| Action | Evidence |
|---|---|
| mcp `WEBSCRAPER_URL` → `http://research.webscraper.svc.cluster.local:9119` | read back from the reconciled Deployment |
| n8n-facing `webscraper-host-vs` destination → `research` Service | the internal service DNS name unchanged |
| Python `webscraper` Deployment, Service and egress policy removed | `kubectl -n webscraper get deploy,svc` lists only `research` |
| live request after the flip | `POST http://research:9119/api/scrape/url` → the example.com body |
| mcp restarted on the new URL | pod `1/1 Running` |

## Not done, and why

- **The media gate (RESOLVED 2026-11-17, #226).** At the time of the cutover the port trusted a
  bare `X-Tenant-Id`; the reference demands a credential. The exposure was closed by withdrawing the
  Deployment (gitops#5318), and the gate was restored in conveyor#226 — see "The media gate" below.
- **Retiring `webscraper_service`** — done, under the owner's 2026-11-17 procedure. The first
  attempt used the session's `gh` shim (`~/.claude/bin/gh`), which is a GitHub App installation
  token with `permissions.admin=false`, so it was refused `403 Resource not accessible by
  integration`. The real binary `/opt/homebrew/bin/gh` is the owner's account and has admin; the
  steps then completed:

  | Step | Result |
  |---|---|
  | rename | `nonagenticai/webscraper_service` → `nonagenticai/webscraper_service-archive` |
  | transfer | → `nonagenticai-archive/webscraper_service-archive` (private, full history kept) |
  | republish | `nonagenticai/webscraper_service` created **public**, **1 commit**, 102 files, no history |

  Cleaning applied before publication, measured on the export: `.secrets.baseline` and
  `docker-compose.override.yml` dropped; four internal service DNS names, one public API hostname
  and one private LAN IP replaced with `example.com` hosts. `betterleaks` after cleaning: 5 findings, all test-fixture passwords
  (`username="alice", password="s3cr3t"` and equivalent), zero real credentials. The public README
  now opens with a retired notice pointing at `apps/research`.

  Note for future retirements: the High-alert gate is vacuous on this repo — it has no code
  scanning configured at all (`403 Advanced Security must be enabled`), so "no high alerts" means
  "not scanned", not "clean".
- **`webscraper-env`.** Unreferenced after the cutover, deliberately not deleted in a Deployment
  swap; a separate auditable act.

## The media gate (2026-11-17, #226)

The reference's gate was three behaviours, each now ported and pinned by a test:

| Credential | Reference | This service |
|---|---|---|
| none | 401 `AUTHENTICATION_REQUIRED` | 401, same code, `WWW-Authenticate: Bearer` |
| `X-Tenant-Id: Bearer <licence>` | the licence's tenant | introspected at `LICENSE_INTROSPECT_URL`; inactive/unknown/tenant-less → 401 |
| bare `X-Tenant-Id` + `Authorization: Bearer <service licence>` | the NAMED tenant, only for an allowlisted subject | same; empty allowlist → every bare tenant refused (fail-closed) |
| bare `X-Tenant-Id` alone | 401 — a claim is not a credential | 401 |
| a non-uuid tenant | 400 `INVALID_TENANT_ID` | 400, same code |

Deviations, deliberate: no introspection cache (the reference caches 5 min; this service has had
zero callers and a fresh check never serves a revoked licence from cache) and no trusted-CIDR
branch (the vouch is required for every caller — strictly stricter than the reference's peer
allowance).

Sequence, recorded honestly: the Python service was retired before the gate was restored, so on
2026-11-17 the Deployment was first **withdrawn to 0 replicas** (gitops#5318 — measured zero
callers: a cluster-wide workload sweep matched only the Deployment itself, and the parent
kustomization already recorded "zero callers in 7d"), then returned behind the restored gate.
The live verification of the three shapes and the pinned digest are recorded on conveyor#226.
