# Research cutover — 2026-11-16

Part of #205. The two Rust binaries were deployed as shadows (`gitops-production#5297`), the scraper
was cut over (`#5303`); the media cutover is **blocked** by #226 and did not happen.

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

That is the whole reason the media cutover did not happen: see #226. The row content itself matched.

## Cutover (P4, scraper half only)

| Action | Evidence |
|---|---|
| mcp `WEBSCRAPER_URL` → `http://research.webscraper.svc.cluster.local:9119` | read back from the reconciled Deployment |
| n8n-facing `webscraper-host-vs` destination → `research` Service | `webscraper.internal` unchanged |
| Python `webscraper` Deployment, Service and egress policy removed | `kubectl -n webscraper get deploy,svc` lists only `research` |
| live request after the flip | `POST http://research:9119/api/scrape/url` → the example.com body |
| mcp restarted on the new URL | pod `1/1 Running` |

## Not done, and why

- **The media cutover.** #226: the port trusts a bare `X-Tenant-Id`; the reference demands a token.
  `media-discovery` keeps running, and its repo stays unarchived.
- **Archiving `webscraper_service`.** Attempted, refused: `403 Resource not accessible by
  integration` — the available credential has no repo-admin on the org repo. The repo has no code
  scanning configured at all (403 "Advanced Security must be enabled"), so the High-alert gate is
  vacuous rather than failed. To finish: an owner-run
  `gh api -X PATCH repos/nonagenticai/webscraper_service -f archived=true`.
- **`webscraper-env`.** Unreferenced after the cutover, deliberately not deleted in a Deployment
  swap; a separate auditable act.
