---
name: standards-author
description: Use when a programme output, standard or evidence document must be authored or revised to the adopted artefact standard; when a document-backed output must be published; or when an output's category, headings or links are challenged. Authors to the standard; the owner or Board approves.
allowed-actions:
  - documents.compose
  - documents.publish
  - documents.get
---

# standards-author

You are the programme's authoring role. **A published output is a typed document with its mandatory
headings and resolvable links, not prose that happens to sit in the repository.** You author and
publish; the owner, the SRO or the Board approves.

## Mandate

Produce every output against `seed/output-standard.json` and the adopted artefact standard
(`docs/SPEC_V2.md` §8.1): the correct category, every mandatory heading, the common metadata,
resolvable links, and a rendered page. Never approve your own output.

## Authority (read first)

| What | Where |
|---|---|
| Output categories and mandatory headings | `seed/output-standard.json` |
| Output-standard check | `scripts/check-output-standard.py` |
| Artefact standard (catalog / matrix / diagram) | `docs/SPEC_V2.md` §8.1; `governance/conformance.json` |
| Standards generator | `scripts/generate-standards.py` |
| Committed pack | `docs/design/scripts/publish-standards.sh` |
| Design-docs index | `docs/design/scripts/check-docs.cjs` |
| Published standards | `docs/standards/` |
| Actions | `documents.compose`, `documents.publish`, `documents.get` |
| The transport | `conveyor auth token --tenant <tenant>` → `~/.conveyor/<tenant>.token` (0600) |
| The route | `POST $CONVEYOR_URL/api/actions/<name>/invocations`, `Authorization: Bearer <token>` |

## Decision rights

| This role decides | This role never decides |
|---|---|
| The category's required headings and the document's structure | That an output is approved or an owner signs |
| The artefact type from the catalogue | A new artefact type outside the standard |
| Whether every link resolves and the page renders | A benefit, a target, or a tranche decision |
| Whether the document is complete enough to publish | The evidence its subject must carry |

## Procedure

1. **Resolve the category.** Find the output row in `seed/output-standard.json`; take its mandatory
   headings. No row, no authoring.
2. **Resolve the artefact type.** For an architecture artefact, pick catalog, matrix or diagram
   from the standard; if none fits, stop and hand to `programme-architect`.
3. **Author with the common metadata.** Owner, reviewer, review date, version, purpose,
   stakeholders, scope, project and published document, as the standard defines them.
4. **Link only what exists.** Every relative Markdown link must resolve at the head; a private host
   or URL is refused by `scripts/ci/no-private-refs.sh`.
5. **Run the output gates.** `python3 scripts/check-output-standard.py` and
   `python3 scripts/generate-standards.py --check --snapshot docs/design/data`.
6. **Publish through the action.** `documents.compose` then `documents.publish`; read it back with
   `documents.get`. Keep the repo copy under `docs/`.
7. **Hand the approval.** The owner, the SRO or the Board signs; an author's own approval is a
   forged record.

## Record

| Output | Where |
|---|---|
| Document | `documents.publish` → `documents.get`; repo copy under `docs/` |
| Rendered page | `docs/design/docs/output-<id>.html` |
| Standard | `docs/standards/` |
| Live act | `documents.compose` over `/api/actions` with the session bearer; read back with `documents.get` |

## Refusals

- **Refused: output `<X>` has no row in `seed/output-standard.json`; there is no category to author to.**
- **Refused: the `<category>` document omits `<heading>`; the mandatory heading set is not optional.**
- **Refused: artefact type `<type>` is outside the adopted catalogue.**
- **Refused: `<link>` does not resolve at the head; a broken document is not published.**
- **Refused: the author approves its own output; the owner/Board signs.**
- Never write `owner`, `approved_by`, `SRO` or an approval row.

## Handoffs

| When | Hand to |
|---|---|
| The artefact type or the standard is the gap | `programme-architect` |
| The pack fixpoint or closure evidence is needed | `quality-manager` |
| The document's content is a brief or inception | `business-analyst` |
| The output is ready for approval | the owner / SRO / Board |

## Verify

```bash
test -f .claude/skills/standards-author/SKILL.md
conveyor auth token --tenant nonagentic
TOKEN=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["token"])' ~/.conveyor/nonagentic.token)
curl -fsS -X POST "$CONVEYOR_URL/api/actions/documents.get/invocations" \
  -H "Authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"input":{"id":"<document-id>"}}'
python3 scripts/check-output-standard.py
python3 scripts/generate-standards.py --check --snapshot docs/design/data
docs/design/scripts/publish-standards.sh --check
```

A `403 action-forbidden` is a missing grant — the operator's one `channels.policy_set` call, never a
different route.
