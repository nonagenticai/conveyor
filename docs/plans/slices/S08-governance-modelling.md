---
slice: S08
title: Governance, modelling, render and export
status: planned
depends_on: [S03, S04, S07]
spec_sections: ["§2", "§4", "§8 S8", "§11 R1"]
adds_tables: [project_stakeholder, model, model_element, model_relationship, model_view, view_object, view_connection, model_validation_run]
adds_actions: [programmes.create, programmes.get, programmes.update, programmes.list, stakeholders.create, stakeholders.get, stakeholders.update, stakeholders.delete, stakeholders.list, models.create, models.get, models.update, models.delete, models.list, models.publish, models.compare, models.validate, models.render, models.export_xmi, models.import_xmi, models.traceability, models.gaps, models.impact, models.completeness, model_elements.create, model_elements.update, model_elements.delete, model_elements.list, model_relationships.create, model_relationships.update, model_relationships.delete, model_relationships.list, model_views.create, model_views.get, model_views.update, model_views.delete, model_views.list, model_views.generate, view_objects.create, view_objects.update, view_objects.delete, view_connections.create, view_connections.update, view_connections.delete]
adds_events: [portfolio.changed, model.created, model.published, model.validated, model.exported]
adds_modules: []
parity_ids: PENDING-S0
---

# S08 — Governance, modelling, render and export

Programme project: S08 · issues none yet

<!-- 2026-11-08: `project_capability`, `deliverable` and `register_item` were built by 0008 and DROPPED by
migration 0011 — S00 day 2 measured zero usage (project_deliverables/project_milestones 0, pm_project_capabilities
0, pm_resources/pm_risks/pm_benefits 0, and no matching operation in the 348 flows). Their 14 actions
(`project_capabilities.*`, `deliverables.*`, `registers.*`) die with them. `project_stakeholder` STAYS (21 live
rows). `tocs.*` (6), `toc_nodes.*` (4) and the `toc.changed` event also die — they are the API over S07's
`outline`/`outline_node`. `programmes.*`, `stakeholders.*`, `portfolio.changed` and the whole modelling surface
are untouched. -->

**Gate (SPEC §8 S8):** golden SVG/PNG/PDF/XMI fixtures; Part A's round-trip is the governance floor.

## 1. Objective

RM's governance surface lands on S03's spine — portfolio registers and TOC trees as actions over existing tables; its architecture surface on one flattened model shape for ArchiMate and UML. R1 closes by **scoping**: a curated profile validated against live RM data, fr's render map and n8n operations.

## 2. Starting state

- S03: `governed_asset`, `revision`, `catalogue_entry`, `trace_link`, `programme`, `project`; RLS + outbox.
- S07: `outline`/`outline_node` and the `principles.*`/`decisions.*` actions.
- S04: registry, invocation route, OpenAPI + MCP projection. S02: `job`, `event`, `audit_event`.

## 3. Scope

**IN** — the 11 tables of `02-DATA-ARCHITECTURE §2.8/§2.9`; Part A actions over them plus TOC and programmes; Part B profile, validation, render, XMI and four read-only computations.

**OUT** — UI (**S12**); model search (**S07**; model arm only if S0 proves use); project CRUD (**S05**); principle-conflict verdicts (**S07**); `architecture_containers` (**dropped** — it feeds fr's queue selector, replaced by S06's card-type×list); RM's change-set/review/publication split (**dropped**, §8.1); graph layout and OCL/Profile/SysML (**out of v1**).

**UI surface declared for S12**: `trace/architecture` — `model_views.get` + the rendered object,
`models.traceability`/`trace_links.list`, `model_validation_run` findings, `tocs.graph`; SSE `model:{id}` on
publish.

## 4. Source material

| path | read this | carry | do not carry |
|---|---|---|---|
| `02-source-map/30-rm-planka-mcp-n8n.md` §1 (RM local HEAD `9ee72ab4e178`, 5 behind `origin/main 191086942ce5` on 2026-11-07; re-check moved paths) | `architecture_*`, `modeling_*`, `uml_*`, `pm_*` routers | element/relationship/view concepts, `registry.json` | five-table sprawl, containers, change sets |
| `requirements_management/src/data/archimate/3.2/registry.json` | kind codes, labels, edge hints | labels/icons only | as validation authority |
| fr `library` screens — `fr@6f7c509 frontend/src/routes/(project)/projects/[id]/library/` (`architecture`, `pm`, `principles`, `decisions`, `requirements`, `workflows`) | rendered element kinds | render map as profile input | fr's visual design |
| `40-live-state.md` | catalogue counts real; modelling counts unmeasured | measure | assumed model data |

## 5. Deliverables

### 5.1 Part A — ships vs reuses

| table | owner | S08 use |
|---|---|---|
| `project_stakeholder` | **S08 ships** | stakeholders per project |
| `project_capability` | **S08 ships** | capability coverage |
| `deliverable` | **S08 ships** | deliverables + milestones, `kind_code` from `catalogue_entry` |
| `register_item` | **S08 ships** | one shape, `kind ∈ resource\|risk\|benefit` |
| `programme` | S03 | `programmes.*` |
| `governed_asset` (`principle`,`adr`) | S03 | **reused**; S07 ships those actions |
| `outline` / `outline_node` | **S07** | **reused**; S08 ships TOC actions only |
| `catalogue_entry` | S03 | validates codes |
| `revision`, `trace_link` | S03 | model revisions; asset↔element traces |

The source map assigns `principles.py`/`decisions.py` to S08; S07 defines them, so S08 consumes (§9.4).

### 5.2 Part A actions

| group | actions |
|---|---|
| programmes | `programmes.create/get/update/list` over S03's `programme` |
| stakeholders | `stakeholders.create/get/update/delete/list` |
| capabilities | `project_capabilities.create/update/delete/list` |
| deliverables | `deliverables.create/get/update/delete/list`; `kind_code` must resolve |
| principles, ADRs | **reused** from S07: `principles.*`/`decisions.*` over `governed_asset`, kind fixed by declaration |
| registers | `registers.create/get/update/delete/list`; `kind` required, kind fields in `meta` |
| TOC | `tocs.create/get/update/delete/list`, `tocs.graph`, `toc_nodes.create/update/move/delete` |

Inputs are writable columns, never `tenant_id`; outputs the row + `etag`. `list` takes `{cursor,limit}`, `get` `{id}`, `move` `{id,parent_id,position}`.

TOC: `outline.kind ∈ ('ltp','toc')`; `outline_node.edge_kind ∈ ('necessity','sufficiency','and','or', null)`. `toc_nodes.move` re-parents, re-positions and rejects cycles via a recursive ancestor walk in one transaction; `tocs.graph` returns the tree and a necessity/sufficiency roll-up. No new table or column.

### 5.3 Part B — seven tables, §2.8 verbatim

| table | key columns | constraint |
|---|---|---|
| `model` | `language`, `language_version`, `title`, `head_revision`, `published_revision_id` | `ix(tenant_id,project_id)` |
| `model_element` | `model_id`, `kind`, `name`, `documentation`, `parent_element_id`, `meta` | `ix(tenant_id,model_id,kind)` |
| `model_relationship` | `model_id`, `kind`, `source_id`, `target_id`, `meta` | `check(source_id<>target_id)` |
| `model_view` | `model_id`, `kind ∈ (diagram,archimate_view,uml_diagram)`, `name`, `meta` | `ix(tenant_id,model_id)` |
| `view_object` | `view_id`, `element_id`, `parent_object_id`, `kind`, `bounds`, `style`, `label`, `position` | `ix(tenant_id,view_id,position,id)` |
| `view_connection` | `view_id`, `relationship_id`, `source_object_id`, `target_object_id`, `bendpoints`, `style` | `ix(tenant_id,view_id)` |
| `model_validation_run` | `model_id`, `revision_id`, `ruleset`, `ruleset_version`, `status`, `findings`, `content_hash` | retention 90 days |

All carry `tenant_id NOT NULL`, `FORCE` RLS, `created_at`/`updated_at`.

### 5.4 ArchiMate profile — `conveyor-archimate-3.2-v1` (30 element kinds, 11 relationship kinds)

- **Business (6):** `business_actor`, `business_role`, `business_process`, `business_function`, `business_service`, `business_object`
- **Application (4):** `application_component`, `application_service`, `application_interface`, `data_object`
- **Technology (4):** `node`, `system_software`, `technology_service`, `artifact`
- **Strategy (4):** `resource`, `capability`, `course_of_action`, `value_stream`
- **Motivation (7):** `stakeholder`, `driver`, `assessment`, `goal`, `outcome`, `requirement`, `principle`
- **Implementation & Migration (3):** `work_package`, `deliverable`, `gap`
- **Composite (2):** `grouping`, `location`
- **Relationships (11):** `composition`, `aggregation`, `assignment`, `realization`, `serving`, `access`, `influence`, `triggering`, `flow`, `specialization`, `association`

**OUT:** physical layer (`equipment`, `facility`, `distribution_network`, `material`, `path`), collaborations, unused behaviour/event kinds (`business_interaction`, `application_process`, `technology_function`, `technology_interaction`, `communication_network`, …), `product`, `contract`, `representation`, `meaning`, `value`, `implementation_event`, `plateau`, junctions, and every 3.2 kind not listed.

**Validated against three consumers** — RM's live `architecture_elements`/`uml_elements` kinds, fr's
kind→icon switch, n8n's `requirements` operations (actions, never elements).
Every non-zero kind is in-profile or deferred; step 9 extracts, §7.2 tabulates.

### 5.5 UML 2.5.1 subset — `conveyor-uml-2.5.1-v1` (10 element kinds, 9 relationship kinds)

- **Elements:** `class`, `interface`, `enumeration`, `datatype`, `package`, `component`, `object`, `actor`, `use_case`, `lifeline`
- **Relationships:** `association`, `aggregation`, `composition`, `generalization`, `realization`, `dependency`, `include`, `extend`, `message`
- **Diagrams (5):** class, package, component, use-case, sequence

**OUT:** activity/state-machine/deployment diagrams, combined fragments, multiplicity expressions, OCL, profiles/stereotypes beyond `meta.stereotype`.

### 5.6 Rendering — as built (2026-11-08)

Revision + view → `crates/render` → **SVG**, then PNG and PDF from *that* SVG. `resvg 0.45.1` rasterises the
`usvg` tree and `svg2pdf 0.13.0` converts the same tree, so all three formats are encodings of one parse and
cannot disagree about the page. ⚠️ **This replaces `typst`**, which was in this plan until B2: typst re-lays the
page from different input (a second layout, and a class of bug no byte test can catch when the two paths never
meet) and would have added ~130 crates to a workspace whose `target/` was already 24 GiB. XMI via `quick-xml`.

**Determinism contract:** fixed `(revision.content_hash, view.id, format, RENDERER_VERSION)` ⇒ byte-identical
output, proved two ways — checked-in golden bytes, and rendering in **two separate processes** and comparing.
SVG ids are `e-<uuid>` / `r-<uuid>` / `v-<uuid>` / `o-<uuid>` / `c-<uuid>` and markers `m-<kind>`, never
counters or map order; collections sorted; no timestamp, hostname, path or hash iteration. The PDF carries no
date and no file id (measured: `svg2pdf` writes exactly one document-info field, a constant `producer`).

**Page:** one view is one A4 landscape page — canvas `1122.52 × 793.70` px at 96 dpi. Those two decimals are
load-bearing: it is `297 × 210 mm ÷ 25.4 × 96`, so the PDF page is exactly `841.89 × 595.28` pt (measured
`MediaBox [0 0 841.89 595.275]`); rounding to 1123×794 makes the page 0.13 % oversize and a printer rescales.
The diagram is fitted into a 24 px margin, centred, and never magnified above 1:1.

**Fonts:** the four DejaVu Sans 2.37 faces vendored at `crates/render/fonts/` (from RM's own vendored set) with
`LICENSE` and `SHA256SUMS`, registered from `include_bytes!` into `usvg`'s `fontdb`. ⚠️ **No mono face** — the
renderer draws labels and kind captions, never code, so `DejaVuSansMono.ttf` had no consumer. ⚠️ **The original
claim here — "`load_system_fonts()` does not exist" — is FALSE and is corrected:** `fontdb`'s `fs` feature is
enabled transitively by `svg2pdf`, so the function exists whatever this crate declares. Host fonts are kept out
by two guards instead: `usvg`/`resvg` are pinned `default-features = false` (so usvg's `system-fonts` feature is
off and the parser never scans the system itself), and a **source-scan test** in `crates/render/src/font.rs`
fails if any production line calls `load_system_fonts` — the "CI grep" this section asked for, running in the
gate. A second test asserts the database holds exactly the four vendored faces. A character outside the face's
cmap is a refused render (`MissingGlyph`), never tofu.

**Layout:** authored geometry wins — a view's stored `view_object.bounds` and `view_connection.bendpoints` are
drawn as stored, and a connector takes the *kind* of the relationship it names (so a composition gets its
diamond). With no view (`view_id` absent) or a view with no objects, `crates/domain::model::layout` supplies
kind-banded rows ordered by `(kind, id)` — not `(position, id)`, which this plan said and the model never had:
`model_element` has no `position` column.

### 5.7 XMI

ArchiMate: Open Group 3.2 Model Exchange File Format subset over XMI 2.1. UML: XMI 2.5.1. Out-of-profile constructs increment `unsupported[]` and are skipped — never silently dropped, never fatal.

**"Full conformance is infeasible" means:** no MOF/EMOF completeness; no profile/stereotype definitions (UML Profiles, SysML, Archi specializations); no vendor extensions beyond `meta.archi`; no `Difference`/`Copy`/`Document` envelopes; no UML diagram interchange; unknown namespaces preserved as text.

**Guarantees instead:** (a) every in-profile element/relationship appears exactly once, with an XML id derived
from its UUID — ⛔ **prefixed, `e-<uuid>` / `r-<uuid>`, not the bare UUID this plan originally promised**: an
`xs:ID`/NCName may not start with a digit and a UUID usually does, so a bare one is not a legal XML id at all.
RM's own exporters hit this and prefix (`_` in `tools/uml/xmi.py`, `id-` in `tools/architecture/exchange.py`);
(b) re-import into a fresh model yields an identical canonical multiset of `(kind,name,documentation,parent-path)`
and `(kind,source-path,target-path)` — ids are *not* part of the multiset, which is why a foreign id can be
mapped without breaking the guarantee; (c) **re-exporting what was imported is canonically equal to the first
export**, not byte-identical — ⚠️ this plan said "byte-identical" and that is unachievable: the store mints a
fresh `uuidv7()` for every imported row (`NewElement` carries no id), so the `e-<uuid>` in the second export
necessarily differs. Byte identity holds where it can: two exports of the *same* snapshot are identical;
(d) `unsupported[]` is returned by the action and carried on the import's `model.created` event.

**Ids on import.** An id that strips to a UUID is preserved verbatim. Anything else — an Archi export writes
`id-1a2b3c`, RM writes `_<uuid>` — is mapped through `Uuid::new_v5(CONVEYOR_XMI_NS, raw)`, so importing the
same file twice yields the same ids **within the document** and a foreign document is importable at all. A
duplicate id is refused (`422`); so is a `<!DOCTYPE>` (the parser never resolves an entity, and the document is
untrusted input), a document over 1 MiB, and nesting deeper than 256.

**What the codec deliberately does not carry** (each is *reported*, never dropped in silence — measured against
the pinned `archimate3_Model.xsd` and RM's own writers):

| construct | decision | why |
|---|---|---|
| ArchiMate concept containment (`parent_element_id`) | reported as `Unsupported{element}` | `ElementType` extends `ConceptType` only — the 3.x exchange format has no nesting slot. RM reports the same loss as `CONCEPT-PARENT`. UML nests natively and round-trips. |
| ArchiMate view kind other than `archimate_view` | exported as a diagram, kind change reported | the format has one `Diagram` type |
| UML diagrams | carried in one `xmi:Extension extender="conveyor"` | no diagram interchange in this scope; bounds/label/position/bendpoints survive |
| `aggregation` / `composition` in UML | both `uml:Association`, distinguished by `aggregation="shared"\|"composite"` on the part end | that is what UML means by them |
| `include` / `extend` / `message` in UML | directed `packagedElement` with `client`/`supplier` | the model has no interaction layer; the kind stays in `xmi:type` |
| foreign constructs (`ownedAttribute`, `organizations`, …) | ignored, not reported | `Unsupported.construct` is defined as element/relationship/view/object/connection only — a deliberate scope line |

⚠️ Two XML facts the implementation had to learn, recorded because both produced silent corruption first:
**`quick-xml` 0.42 escapes attribute values itself** (`Attribute::from` calls `escape_attribute`), so escaping
by hand double-escapes (`&amp;amp;`); and it emits `Event::GeneralRef` for entity references rather than folding
them into `Text`, so an undefined entity must be refused explicitly. Also: matching attributes by local name is
unsafe for UML — `xmi:type` and `type` both reduce to `type` on an `ownedEnd`; match the exact name first.

### 5.8 Validation

`models.validate` enqueues `job(kind='model.validate', dedupe_key='model:<id>:<content_hash>')` **after commit**; the job writes one `model_validation_run`. Checks: kind in-profile; relationship endpoints legal; parent/child legality; view connection's relationship exists; a `principle`/`requirement` element's `meta.governed_asset_id` resolves in-tenant with the same title. Findings: `[{code,severity,element_id?,relationship_id?,pointer,message}]`. Write-time: an out-of-profile `kind` is `422` problem+json, `type=/problems/model-profile-violation`, `errors[]` naming kind and profile version.

### 5.9 Part B actions — 35, all on §2.8's seven tables (none new)

| group | actions |
|---|---|
| models | `models.create/get/update/delete/list/publish/compare/validate/render/export_xmi/import_xmi` |
| elements | `model_elements.create/update/delete/list` |
| relationships | `model_relationships.create/update/delete/list` |
| views | `model_views.create/get/update/delete/list/generate` |
| view objects | `view_objects.create/update/delete` |
| view connections | `view_connections.create/update/delete` |
| read-only tools | `models.traceability`, `models.gaps`, `models.impact`, `models.completeness` — computations over `trace_link` + the model graph; no writes, no tables |

Part A is 33 + Part B 35 = the front-matter's 68; `import_xmi` creates a revision in one transaction.

**As built (B2).** The three exchange actions live in `crates/actions/src/model/exchange.rs` and reuse
`model:read` / `model:write` — no new scope. Their payloads follow CONTRACT §6: `models.render` returns
`content_b64` + `media_type` + `content_sha256` + `byte_len` (cap 8 MiB), `models.export_xmi` returns the `xml`
text itself (cap 4 MiB), and `import_xmi` caps its input at **1 MiB** — the action request body is axum's 2 MiB
default and JSON inflates, so a larger document would fail at the transport before reaching the codec.
`import_xmi` writes a **draft** revision (`write_head_revision_tx`), never a published one: an import is not a
publication, and publishing here would point `model.published_revision_id` at a graph nobody reviewed.
(`models.compare` itself takes any two revision ids — it does not filter on status, so comparing the imported
draft against the published revision is exactly how you see what the document would change.)
`models.render` and `models.export_xmi` emit `model.exported`; `models.import_xmi` emits `model.created`.

**Byte routes, not actions.** `GET /api/models/{id}/render?view_id=&format=` and `GET /api/models/{id}/xmi`
serve the same bytes with the real media type and an `ETag` equal to their SHA-256. They exist because a browser
`<img>`/download cannot POST an action or send a bearer header (the dev-auth cookie covers them) and they have
no MCP projection. They call the same `conveyor_render` entry point, so a route and an action cannot disagree.

### 5.10 File tree

```
migrations/0008_s08_portfolio.sql · 0009_s08_models.sql · 0010_s08_retention.sql
crates/domain/src/{portfolio,model,profile/{archimate,uml},validate}.rs
crates/store/src/{portfolio,toc,model,view,validation,prune}.rs
crates/actions/src/{portfolio,toc}/…, crates/actions/src/model/{models,elements,relationships,model_views,objects,connections,tools,exchange}.rs
crates/render/src/{lib,ids,font,svg,png,pdf,xmi}.rs · crates/render/fonts/{4 faces,LICENSE,SHA256SUMS}
crates/render/tests/{golden,xmi_roundtrip}.rs · crates/render/tests/fixtures/mod.rs
crates/render/tests/golden/{a,b,c}/diagram.{svg,png,pdf} · crates/render/tests/golden/manifest.json
crates/api/src/routes/models.rs (the two byte routes)
crates/api/tests/{portfolio_roundtrip,toc_tree,model_profile,validation_job,architecture_tools,model_render}.rs
```

⚠️ Two deviations from the plan as written: the three exchange actions sit in `crates/actions/src/model/`
(the crate's model family module, where the other 32 already are) rather than a top-level `exchange.rs`; and
`crates/render` has **no `layout.rs`** — `layout` lives in `crates/domain/src/model.rs` because
`model_views.generate` needed it in B1, and the renderer draws what that function placed.

### 5.11 Events

Outbox rows, one per mutation, in its transaction (CONTRACT §7).

| type | emitter | payload keys |
|---|---|---|
| `portfolio.changed` | `programmes.*`, `stakeholders.*`, `project_capabilities.*`, `deliverables.*`, `registers.*` | entity, id, op |
| `toc.changed` | `tocs.*`, `toc_nodes.*` | outline_id, node_id, op |
| `model.created` | `models.create`, `models.import_xmi` | model_id, language |
| `model.published` | `models.publish` | model_id, revision_id |
| `model.validated` | `model.validate` job | model_id, run_id, status |
| `model.exported` | `models.render`, `models.export_xmi` | model_id, format |

### 5.12 Retention producers (added in B2)

§2.4's "model revisions keep the last 50" and §5.3's "`model_validation_run` kept 90 days" were rules with no
producer — both tables grew without bound. B2 ships them as job kinds beside `event.prune`/`job.prune`:
`model.prune` (`25 3 * * *`) and `validation.prune` (`35 3 * * *`), in `crates/store/src/prune.rs`, plus
migration `0010` which re-creates `model_validation_run_revision_fk` as `ON DELETE SET NULL` — without it a
revision prune fails the moment a validation run names a revision older than the 50th. `0009` is not edited:
sqlx records a checksum per migration, so editing a shipped file breaks every migrated database.

Three §6 rules elsewhere still have **no producer** (`action_invocation` 30 d, `run` + `run_step` 180 d,
`delivery` 30 d) and are not S08's tables. They are named in `02-DATA-ARCHITECTURE §6` as the next maintenance
lane — deliberately not absorbed here.

## 6. Build order

1. Preflight: `cargo test --workspace`; `docker compose up -d db`; `lsof -nP -iTCP:8930 -sTCP:LISTEN`; measure RM (`psql "$RM_READONLY_URL" -c "BEGIN READ ONLY; SELECT kind,count(*) FROM architecture_elements UNION ALL SELECT kind,count(*) FROM uml_elements GROUP BY 1; COMMIT;"`).
2. `0008_s08_portfolio.sql`: four tables, FKs, indexes, `FORCE` RLS; `cargo run -p conveyor-server -- migrate`
   twice (the binary re-applies the runtime grants; `sqlx migrate run` would not — CONTRACT §16).
3. Domain + store: register `kind`, `kind_code` lookup; `cargo test -p conveyor-store portfolio`.
4. Actions `programmes.*`, `stakeholders.*`, `project_capabilities.*`, `deliverables.*`, `registers.*`; `curl -s localhost:8930/api/actions | jq`.
5. TOC actions on S07's tables: tree CRUD, move with cycle check, graph; `cargo test -p conveyor-api --test toc_tree`.
6. Part A gate: scripted round-trip (S07 principle → ADR → TOC tree → register → deliverable → capability → stakeholder); assert `event` rows. `cargo test -p conveyor-api --test portfolio_roundtrip`.
7. `0009_s08_models.sql`: seven tables, indexes, RLS; migrate twice. `grep -c 'CREATE TABLE' migrations/0009_s08_models.sql` → 7.
8. `crates/domain/src/profile/`: const profiles + `validate::check`. `cargo test -p conveyor-domain profile`.
9. Run the three consumer extracts; write §7.2; freeze `conveyor-archimate-3.2-v1`. `psql "$RM_READONLY_URL" -c "BEGIN READ ONLY; SELECT kind,count(*) FROM architecture_elements GROUP BY 1; COMMIT;" > /tmp/rm-arch-kinds.tsv` (repeat for `uml_elements`; the fr library switch is the second extract).
10. Model/element/relationship actions + write-time `422`; `cargo test -p conveyor-api --test model_profile`.
11. `models.publish` sets `revision.status='published'` + `published_revision_id` in one transaction; `models.compare`. `cargo test -p conveyor-api --test model_profile publish`.
12. Views: `model_views.*`, `view_objects.*`, `view_connections.*`, `model_views.generate`; bounds parsed, never queried. `cargo test -p conveyor-api --test model_profile views`.
13. SVG renderer, deterministic ids, embedded faces; model A renders. `cargo test -p conveyor-render --lib`. ⚠️ **No `cargo insta`** — the goldens are checked-in bytes plus a SHA-256 manifest (see 14), because an `.snap` holding a binary document is unreadable in a diff and `insta accept` blesses changes nobody looked at.
14. PNG and PDF from the same SVG; models A/B/C. First `GOLDEN_UPDATE=1 cargo test -p conveyor-render --test golden` to write `tests/golden/{a,b,c}/diagram.{svg,png,pdf}` + `manifest.json`, then the plain run asserts them.
15. Byte-stability across **processes**: `GOLDEN_UPDATE=1 GOLDEN_DIR=/tmp/g1 …` and `…/tmp/g2 …` in two invocations, then `diff -r /tmp/g1 /tmp/g2` must be empty. Measured 2026-11-08: identical for all three formats, PDF included (`MediaBox [0 0 841.89 595.275]`, no `/CreationDate`).
16. XMI export → import; `crates/render/tests/xmi_roundtrip.rs` asserts the canonical tuples, a byte-identical re-export, `unsupported[]`, and that DOCTYPE/duplicate ids are refused.
17. Validation job post-commit; assert no run row inside a mutation transaction. `cargo test -p conveyor-api --test validation_job`. *(B1)*
18. Read-only tools; `cargo test -p conveyor-api --test architecture_tools`. *(B1)*
19. Retention: migration `0010`; `model_revision_prune`/`validation_run_prune`; worker arms; `DEFAULT_SCHEDULES` 8 → 10. `cargo test -p conveyor-server --test prune`; `cargo run -q -p conveyor-server -- migrate` twice.
20. The three exchange actions + the two byte routes; regenerate `served-spec.lock.json` (`--print-openapi --write-lock`) — **+5** operations (3 POST action paths, 2 GET routes), 187 → 192. `cargo test -p conveyor-api --test model_render`.
21. Gate, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`; fill §7.2.

## 7. Gate

```bash
set -euo pipefail
source ~/.cargo/env          # CONTRACT §16 — cargo is not on PATH
cargo test -p conveyor-api --test portfolio_roundtrip --test model_profile --test model_render
cargo test -p conveyor-store --test s08_models --test s08_portfolio
cargo test -p conveyor-render --test golden --test xmi_roundtrip
cargo test -p conveyor-server --test prune
# ⚠️ Corrected 2026-11-09: this block named three targets that do not exist — `toc_tree`, `validation_job`,
# `architecture_tools`. The TOC round trip is inside `portfolio_roundtrip`; the model/architecture tools and the
# validation job are `model_profile` (api) and `s08_models` (store); the portfolio aggregates are `s08_portfolio`.
```

| # | property | proof |
|---|---|---|
| a | Part A round-trip | principle (S07) + ADR approved; TOC 5 nodes, subtree moved; register/deliverable/capability/stakeholder create→get→update→list; cross-tenant read 0 |
| b | TOC legality | cyclic move → `422`; `tocs.graph` edges equal stored `edge_kind` |
| c | no double definition | `GET /api/actions` has no S08-owned `principles.*`/`decisions.*` |
| d | golden fixtures | 3 models × SVG/PNG/PDF **checked-in bytes + `manifest.json`** (not `insta`); rendering in two separate processes and `diff -r`-ing the trees is empty |
| e | profile rejection | POST `model_elements{kind:"business_interaction"}` → `422`, problem+json names kind and profile version |
| f | XMI round-trip | export A → import fresh → canonical multiset equal; re-export byte-identical; `unsupported[]=0` for A, non-empty for a document with an out-of-profile kind |
| g | validation post-commit | dangling relationship → `status='findings'` naming its id; no run row inside the mutation tx |
| h | id determinism | same view rendered after shuffled insertion order is byte-identical |
| i | retention (B2) | 51 revisions → 1 pruned, the published one survives; a 91-day-old validation run pruned, a 1-hour-old one not; the run survives with `revision_id IS NULL` when its revision is pruned |
| j | artefact parity (B2) | `models.render`'s base64 decodes to the bytes `GET …/render` returns, and their sha256 is the ETag |

**7.2 Completion record** — filled 2026-11-08 (S08 Part B1). Every number below was measured; none is a
planning estimate.

| source | kinds seen | in profile | deferred/dropped | date |
|---|---|---|---|---|
| RM `architecture_elements`, `architecture_relationships`, `architecture_views`, `architecture_view_objects`, `architecture_view_connections` | **0 rows** (`parity/evidence/rm-counts.tsv`) | n/a | n/a — nothing to defer | 2026-11-08 |
| RM `uml_elements`, `uml_relationships`, `modeling_*` | **0 rows** | n/a | n/a | 2026-11-08 |
| RM `archimate/3.2/registry.json` @ `9ee72ab` | **61 concepts / 11 relationships** | **30 / 11** | **31 concepts**: physical 4, business 7, application 5, technology 9, motivation 3, implementation 2, other 1 | 2026-11-08 |
| fr `lib/modeling/archimate.ts` | 8 layers, **11 relationship kinds** (byte-identical to the registry's list) | 7 layers / 11 relationships | the physical layer — 0 kinds in profile | 2026-11-08 |
| fr `lib/modeling/uml.ts` (`UML_KIND_LABEL`) | **25 UML kinds** | **10** | **15** — activity, state-machine and deployment constructs | 2026-11-08 |
| fr `lib/components/conformance/palette.ts` | 2 element kinds (`application_component`, `artifact`) | 2 | none | 2026-11-08 |
| n8n `requirements` node operations (`parity/surfaces/n8n.txt`) | 66 operation lines | n/a — actions, never elements | the modelling operations land as **actions**, not model kinds | 2026-11-08 |

**Open question 1 closes on its own stated default.** Every RM modelling table holds zero rows, so the profile
was derived from the registry, fr's render map and the standard — not from live data. No live RM query was
needed and none was run.

**Open question 2 closes.** fr renders architecture elements **dynamically** from
`/api/assets/architecture/metamodel/concepts`; there is no kind→icon switch to extract (verified at both the
current HEAD and the pinned SHA). The two static consumers are `archimate.ts` and `uml.ts`, both measured above.
No fr kind falls outside the profile that is load-bearing there.

**Frozen:** `conveyor-archimate-3.2-v1` (30 element kinds + 11 relationship kinds) and
`conveyor-uml-2.5.1-v1` (10 + 9, plus the five diagram kinds carried in `model_view.meta.diagram_kind`).
Adding a kind later is a profile data change with a `ruleset_version` bump only when a rule's *meaning* changes.

## 8. Traps

1. **RM's table sprawl.** `modeling_models`, `_model_revisions`, `_publications`, `_reviews`, `_change_sets`, `_validation_runs`, `_validation_findings`, `_trace_links` collapse to `model` + S03 `revision` + `model_validation_run(findings jsonb)` + S03 `trace_link`. Change set = one transaction; publication = `revision.status='published'`; review = S03 `approval`.
2. **`entity_version_history` has no `tenant_id`.** Do not carry the table or its RLS carve-out; `revision` is tenant-scoped `NOT NULL`.
3. **JSONB geometry, not hit-testing.** `view_object.bounds` is allowed (§5: read whole with the view), but nothing may `WHERE bounds->>…`; hit-testing, culling and overlap checks parse the rect in Rust. A measured hot predicate turns `bounds` into four generated numeric columns.
4. **Fonts are a dependency.** An empty container renders tofu and a base-image change silently changes every golden PNG. Vendor the faces, never call `load_system_fonts()`, fail on a missing glyph.
5. **SVG ids must be deterministic.** Map iteration or a counter renumbers per run and churns snapshots; ids derive from uuids, collections are ordered.
6. **Validation inside the mutation transaction** holds the model lock for a full graph check and could roll back a legal edit. Post-commit job keyed on `(model_id, content_hash)`.
7. **Do not trust an XMI file.** Disable DTD/external entities in `quick-xml`, cap size and depth; a duplicate `xmi:id` is `422`.
8. **The registry is not the profile.** `registry.json` says what ArchiMate permits; the profile says what this app accepts. Using the registry as validator makes the gate vacuous.
9. ⛔ **An XML id may not start with a digit, and a UUID usually does.** `xmi:id="<uuid>"` is not valid XMI — RM prefixes `_` and `id-` for exactly this reason. Every id is `e-`/`r-`/`v-`/`o-`/`c-` prefixed and stripped on import; a foreign id is mapped through UUID v5 so the file still imports deterministically.
10. ⛔ **Deterministic bytes are not the same as a right picture.** The first golden render was byte-stable and drew a view connection as a plain arrow because the renderer never resolved `relationship_id` to its kind. A golden test cannot catch that class of defect; only looking at the image (or asserting the marker) can. Assert the *semantics* you can — the composition marker, the two-layer colour, the connector's kind — not only the hash.
11. ⛔ **`svg2pdf` writes one document-info field and no date, but prove it.** The determinism claim rests on `pdf.document_info(id).producer(TextStr("svg2pdf"))` being the only thing it sets — verified in 0.13.0's source, and measured by rendering in two processes. If it ever gains a timestamp, the PDF must become *normalised* in the contract; hand-editing PDF bytes is not an option, because changing them shifts every xref offset.
12. **`view_object.position` is `numeric → f64`.** Sort by `(position, id)` and never print it into the document: a float's printed form would become part of the byte contract.

## 9. Decisions closed here

1. **One model shape, `language` discriminates**; no per-language tables.
2. **Profiles are curated and versioned** — ArchiMate 30 + 11, UML 10 + 9 with five diagram kinds; `registry.json` supplies labels/icons/edge hints only.
3. **Profile membership is decided by consumers**: live RM data > fr render map > the standard.
4. **Principles/ADRs stay S07's**; S08 adds no action or table. A `principle`/`requirement` element carries `meta.governed_asset_id` — the asset owns the lifecycle, the element the drawing.
5. **TOC tables stay S07's**; S08 ships actions only.
6. **Publication is a revision status**; change sets are transactions; reviews reuse `approval`.
7. **Rendering determinism is a contract** keyed on `(content_hash, view, format, renderer_version)`.
8. **XMI is profile-scoped with stated guarantees**; unsupported constructs are reported, never a reason to widen the profile.
9. **Portfolio registers are one table with `kind`**; deliverables and milestones one table with `kind_code`.
10. **`architecture_containers` is dropped**; queue selection is S06's card-type×list.

## 10. Open questions

| # | Question | Default |
|---|---|---|
| 1 | Are RM's `architecture_*`/`uml_*` tables non-empty? | Measure at step 1; if empty, models are hand-authored; profile from fr's render map + registry |
| 2 | Do fr's library screens render kinds beyond the profile? `UNVERIFIED` | Extract the switch at step 9; add the kind rather than render it nowhere |
| 3 | Is a real graph layout needed? | No — kind-banded deterministic rows |
| 4 | Sequence-diagram fidelity | Lifelines + ordered messages; combined fragments out |
| 5 | ArchiMate XMI flavour | Open Group 3.2 Exchange File Format; Archi 5.10 round-trip is a manual check |
