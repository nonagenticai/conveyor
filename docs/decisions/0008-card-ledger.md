# 0008 — The card ledger is its own append-only table

Status: Accepted · Date: 2026-11-18 · Issue: #295 · Resolves: `parity/SUMMARY.md` §"The five A-hit
contradictions" — the open home for the P-1224/P-1101 ledger capability.

## Decision

1. **Home.** The per-card claim/fact record lives in a new table, `card_ledger`. It is **not**
   `work_package_memory`: that table is project-scoped (`project_id NOT NULL`), carries
   consolidation state (`consolidated_at`, `supersedes_id`) and holds derived digest rows, so it
   answers "what a run learned", not "what was claimed and accepted about this card". It is **not**
   `event`: the event table is the board's outbound stream — relayed once, pruned by retention, and
   keyed to a mutation's doorbell — so it is a transport record, not a record a caller reads back.

2. **Key.** The ledger is keyed by the conveyor card id:
   `card_id uuid NOT NULL REFERENCES card(id) ON DELETE RESTRICT`. The old `planka_card_id` is
   **not carried**: an identity is not content (D7), and nothing is copied across the cutover. The
   old rows stay in the RM database; the new table starts empty, with no backfill and no
   compatibility column.

3. **Row shape** (`migrations/0052_card_ledger.sql`):

   | column | type | meaning |
   |---|---|---|
   | `id` | `uuid PK DEFAULT gen_random_uuid()` | the entry |
   | `tenant_id` | `uuid NOT NULL` | RLS anchor; bound explicitly by the store |
   | `card_id` | `uuid NOT NULL` FK `card` | the conveyor card |
   | `kind` | `text CHECK (kind IN ('claim','fact'))` | the two kinds the callers use |
   | `body` | `text NOT NULL` | the claim or fact itself |
   | `source` | `jsonb NOT NULL DEFAULT '{}'` | where it came from (`list`, `node`, `execution_id`, …) |
   | `verdict` | `jsonb` | the laya verdict (`score`, `accepted`, `reason`), when one exists |
   | `created_by` | `uuid` | the writing user; `NULL` for a service principal |
   | `created_at` | `timestamptz NOT NULL DEFAULT now()` | when it was appended |

   Index: `(tenant_id, card_id, created_at, id)` — one card's ledger, oldest first. Tenant RLS is
   the house `tenant_isolation` policy (`ENABLE` + `FORCE`, `USING` and `WITH CHECK`).

4. **Actions.**

   | action | kind | effect | tenancy | behaviour |
   |---|---|---|---|---|
   | `cards.ledger_append` | internal | write | tenant | appends exactly one row and exactly one `event` (`card_ledger.appended`, topic `card:<card_id>`, aggregate `card_ledger`) in the same transaction (CONTRACT §7) |
   | `cards.ledger_list` | internal, idempotent | read | tenant | one card's entries, oldest first, optional `kind` filter, bounded `limit` |

   Both resolve the card in the caller's tenant first: another tenant's card is a `404`, never an
   empty page and never an FK acceptance.

5. **Append-only by construction.** There is no update action, no delete action and no mutating
   store function; `card_ledger` has no `version` and no `updated_at` because nothing edits a row.
   This is deliberately different from the grant-based append-only tables (`event`,
   `approval_decision`): the guarantee is the absence of a write path, recorded in the migration's
   header. Adding a mutating path is the change that breaks it.

## Context

`parity/parity.yaml` records two live surfaces: P-1224 (`POST /pm/cards/{planka_card_id}/ledger`,
29 active workflows / 145 call sites) and P-1101 (`GET /pm/cards/{planka_card_id}/ledger`, 20 / 65).
SPEC §4 drops `planka_card_id` as card *identity*, not the capability: the callers write claims and
facts with a `source` object and a laya verdict and read them back per card. `parity/SUMMARY.md`
named `work_package_memory` and `event` as candidates and left the home open; this record closes it.

The same resolution rejects the other A-hit contradictions' pattern for this row: there is no
"copy" step and no inherited ledger. A claim recorded before cutover is RM history; a claim
recorded after cutover is a `card_ledger` row.

## Consequences

- No data migration: the table is new and empty. A consumer that needs historical claims reads
  them from RM until RM is retired (D7's normal cutover rule).
- The event vocabulary gains `card_ledger.appended`; a webhook may subscribe to it.
- `docs/evidence/2026-11-18-card-ledger-call-sites.md` maps the recorded call-site semantics onto
  `cards.ledger_append`/`cards.ledger_list` and names the drops.
- A future search or digest feature must read `card_ledger`; it must not add a second claim store.
