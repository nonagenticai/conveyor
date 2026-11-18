# Card ledger — the old call sites and their new shape

Date: 2026-11-18 · Issue: #295 · Decision: ADR 0008 · Kind: P3 call-site catalogue

## Method

The available records are `parity/SUMMARY.md` §"The five A-hit contradictions" (including "The
ledger is the consequential one") and the two `parity/parity.yaml` rows, P-1224 and P-1101. Those
records give the surface, the invocation counts and the payload's named fields; they do **not**
contain a per-workflow or per-call-site inventory. The catalogue therefore enumerates the
*semantics the records name* and maps each to `cards.ledger_append` / `cards.ledger_list`. It does
not invent 145 individual rows from a count.

Recorded volume: 145 append call sites in 29 active workflows and 65 read call sites in 20 active
workflows — 210 in-cluster invocations across 49 workflows. Every one of those invocations is one
of the three semantics below; the counts collapse, the semantics do not.

## Catalogue

| # | old surface | recorded semantic | new shape |
|---|---|---|---|
| 1 | `POST /pm/cards/{planka_card_id}/ledger` | **append a claim**: `kind: claim`, `source{list,node,execution_id}`, laya `verdict{score,accepted,reason}` | `cards.ledger_append` with `kind: "claim"`, `source` stored as the `source` JSON object, `verdict` stored as the `verdict` JSON object (ADR 0008 row shape) |
| 2 | `POST /pm/cards/{planka_card_id}/ledger` | **append a fact**: `kind: fact`, no verdict recorded | `cards.ledger_append` with `kind: "fact"`, `verdict` omitted (`NULL`); `source` defaults to `{}` when the caller has none |
| 3 | `GET /pm/cards/{planka_card_id}/ledger` | **read one card's ledger** (the `pmCardLedger.listCardLedger` call in 20 workflows) | `cards.ledger_list` for one conveyor card id, oldest first, optional `kind` filter, bounded `limit` |

### Named drops

| dropped | basis |
|---|---|
| `planka_card_id` as the path key and card identity | SPEC §4 drops the identity, not the capability; the ledger is keyed by the conveyor `card.id` (ADR 0008 §2) |
| RM's `pm_card_ledger` storage and its rows | `02-DATA-ARCHITECTURE` §7.2 drops the table for the key; nothing is copied at cutover (D7), so there is no backfill and no compatibility column |
| Per-workflow / per-call-site rows (145 + 65) | the records name counts, not sites; manufacturing one row per invocation would be invention |

## Not determinable from the available records

- The body text of each workflow's claim or fact — the records describe the payload shape, not the
  145 bodies.
- Whether any of the 145 appends used a payload variant outside `claim`/`fact`. The records say the
  payload is `kind=claim|fact`; no third kind is recorded, so none is catalogued.
- Whether the read callers ever filtered by kind or by source. The records name only a per-card
  read; `cards.ledger_list`'s `kind` filter is an available capability, not a recorded call site.
- Whether any post-cutover consumer needs the pre-cutover RM rows. ADR 0008's no-copy rule means
  they are not in conveyor; if a consumer needs history, that is a separate evidence question.

## Count

**3 semantic sites catalogued, 3 mapped to the new actions, 0 mapped to a drop by capability
(only the identity key and the RM storage are dropped), 0 unmapped, 0 invented.** The recorded raw
invocations (145 append + 65 read across 49 workflows) collapse onto these three semantics.
