# Web UI design

The latest design is **[studio.html](studio.html)** — the **SPEC_V2 organisational operating model**.
Open it directly in a browser; no build or server is required. When `data/snapshot.js` is present
(written by `conveyor snapshot`), the pages it covers render the conveyor's own rows — Portfolio,
Organisation, Jobs, Metering and the Plan **Boards** tab — and carry a `LIVE` ribbon with the
snapshot hash and timestamp; an unwired destination or tab keeps the `V2 · PROPOSED` ribbon.
The Plan **Boards** tab is a join, not a seed: it renders each project's GitHub issues from the
issue registry fetched at publish, joined to the conveyor's own cards and runs. A unit with no real
card is shown as **not intaken**. V1 surfaces implemented in the Rust app are unlabelled.
The registry is refreshed by the `issue-registry` workflow
(`.github/workflows/issue-registry.yml`), which runs `scripts/fetch-issues.sh` on every push to
`main` and daily, and opens one generated-data PR when GitHub's issues have moved; merging that PR
rebuilds the image the API serves the file from.

**The real operator UI is the app, not this pack.** Every V2 destination is a route in `apps/web`
(Leptos CSR in the same binary); SPEC_V2 §10.5 is its normative specification and
`docs/plans/T18-real-v2-ui.md` is the delivery plan. The pack remains the public programme site and
the reviewed design reference: a page here is evidence of intent, never of implementation, and the
V2 coverage record (`docs/evidence/v2-coverage.json`) measures the app's routes and specs.

**A separate client, specified here.** [GEMBA-WALK-SPEC.md](GEMBA-WALK-SPEC.md) specifies the Gemba
Walk at `/walk/` — a candidate primary UI (ADR 0021, D67) that is **not** a page in this pack and is
never served at `/design/`. The pack remains the public site; the app remains normative until the
walk's measured parity gate. Its world layer is
[GEMBA-WALK-ECS.md](GEMBA-WALK-ECS.md) — a `bevy_ecs 0.20.1` simulation in `crates/walk-world`
behind the `WorldView` port (ADR 0022, decision 0023 for the renderer). Its MSP/TOGAF/SAFe inception
set is [`docs/plans/inception/GEMBA-WALK/`](../plans/inception/GEMBA-WALK/README.md).

**Two surfaces, one pack.** Served by the app at **https://conveyor.nonagentic.ai/design/** the pack
is the **operating site**: the unauthenticated routes `/design/data/snapshot.js`,
`/design/docs/outputs.js`, `/design/docs/output-<id>.html` and
`/design/docs/attachment-<id>` are read from Postgres for the tenant named by
`CONVEYOR_PUBLIC_TENANT`, so the public reads the real programme and every output marked
publishable. `console.js` finds the same-origin `/config.json`, signs in against the realm the server
already trusts (auth-code + PKCE S256, token in memory), and turns the same pack into the operator
console — intake an issue as a card, move it, comment, claim and release, compose an output. Signed
out there is no API request and no write control; a missing asset is served `no-store`, never
cached as a 404.

**Authors and publishers are different roles.** An invited tenant member (`member`) composes and
edits outputs and files a publish request (`documents.request_publish`); only an `admin` moves an
output into or out of the public window (`documents.publish`). A published output is locked to its
author: an edit to public content is an admin act, so the author asks for an unpublish first. Every
content mutation appends a revision (`documents.revision_list/get`); `documents.revision_restore`
copies a text version forward as a new draft and never republishes. A file attached to an output is
a child `document` (`kind='attachment'`): PNG and JPEG are served inline, every other type is a
download, and an attachment is public only while its parent is.

The public sidebar carries the programme only: **Portfolio, Projects, Outputs, Plan, Validation,
Execution, Documents, Contribute, System architecture**. The V2 product-design study (Organisation,
Jobs, Procedures, Authoring, Improve, Operations, Tools, Metering and the V1 surfaces) is the
operator console's; a signed-out direct hash is a sign-in gate, and a reviewer can open it
explicitly with `?design=1`. The pack still loads the fixture scripts, but no public page renders
them.

**Programme definition** (`programme`) is the MSP layer of SPEC_V2 §8.3: the definition set,
the benefits map and benefit profiles, tranches with end-of-tranche reviews, stakeholder profiles,
and the transition and measured realization of each benefit. Signed in, its five tabs read and
write the programme store — `programmes.list`/`update`, `benefits.list`/`create`/`update`/`delete`/
`link`/`link_list`/`unlink`/`transition_list`/`transition_upsert`, `tranches.list`/`create`/
`update`/`delete`/`review_list`/`review`, `stakeholders.profile_list`/`profile_create`/
`profile_update`/`profile_delete` and `cadences.list`/`create`/`update`/`delete`, every edit
carrying `If-Match`. Signed out, the tabs render the published `msp` snapshot read-only;
`?design=1` keeps the fixture study. The two distinctions hold throughout: a capability is not a
benefit, and a tranche review is a programme decision, not a demo. The narrative study of the
programme architecture and benefits realization is
[`Program_Architecture_and_Benefits_Realization.md`](Program_Architecture_and_Benefits_Realization.md).

**Architecture capability** (`adm`) is the TOGAF ADM layer of SPEC_V2 §8.1: the Preliminary
phase (principles, tailored profile, Architecture Board, maturity and skills), the phase rail A–H
with its typed artefacts and contracts, the artefact repository (catalog, matrix and diagram, with
entries and derivation/realization/reuse links), and the board/compliance register with the six
compliance levels and the waiver discipline. Signed in, the screen reads the live architecture rows
and the public snapshot; `?design=1` keeps the fixture study. Its controls demonstrate the refusals the model depends on — a phase
cannot close while a mandatory artefact is draft, a review records one compliance level, a waiver
needs an accepting owner and a review date, and Phase H records exactly one change or kata.

**Outputs** is the programme register: every output across all projects with its project, unit,
category, author, state and public URL, filtered by those fields and paged with `next_cursor`.
Signed in it composes, edits with `If-Match`, requests or decides publication, reads the history,
attaches a file, downloads and deletes; signed out it lists the published index only. A project's
workspace adds its tranche, `blocked_by`, measured status and evidence from `data/coverage.json`,
groups its outputs by unit and category, and shows **Expected outputs** — the project's own
checklist from `meta.expected_outputs`: P0's eight definition artefacts, or a delivery project's
brief and, for V2, its inception analysis and architecture design. Each row is absent,
present-private or published; the first missing row is marked **next** and carries its compose
template. P0's brief artefact is a computed `project briefs N/47` coverage row across the delivery
projects. A missing output is a visible gap; it is not a backend gate. An admin's **Review
requests** panel lists every draft whose author asked for publication
(`documents.list {publish_requested: true}`) and publishes it in place; publication is the same
`documents.publish` action as the register's, and there is no separate approve/deny action.

**The delivery view** renders the work model at four levels from the same published snapshot.
**Portfolio** draws a Gantt of every in-flight programme — rolled up from its projects, counting
the completed ones — plus every standalone change initiative; a programme row opens the
programme's own Gantt at `#portfolio/<programmeId>`. **Programme** groups its projects by their
MSP tranche (`project.meta.tranche`) under the tranche rail, the benefit rail (measured vs
unmeasured baseline, never "realised" without a transition) and the SAFe cadence / TOGAF
capability line. **Projects** draws the selected project's board as a kanban at
`#projects/<projectId>`, with the project's own record above it — TOGAF contract and compliance
review, PMP WBS/register/baseline counts when present, and the declared artefacts. A card with
children is a work-package tile, and opening any card (`#projects/<projectId>/wp/<cardId>`) shows
its execution — runs and steps, gate, Definition of Done, spend — followed by its child cards
across their lanes, archived children included as done. A childless card is still a work package:
its execution is the whole answer and the page says there are no child cards. A bar is
**actuals** — first observed card or run activity to completion, or to the snapshot time while in
flight — and a planned window is a dashed track only when `project.meta.plan_start`/`meta.plan_end`
are present; a row with no event says *no dated activity* rather than inventing one. Spend comes
from `measure.inference` (`by_project`, `by_card`): tokens are stored, cost is derived from the
pinned commercial price, and a row with no priced call renders `—*` with its unpriced count —
never `$0`. The public boundary is the tenant portfolio: a programme-bound project is public; a
standalone project is public only when it is `active`/`paused` and carries a board or a card.

**The standards record is generated, not asserted.** The snapshot computes
`msp.benefit_realisation` (baseline → transition → post measurement), `msp.dependencies` (the
declared graph, typed only where the declaration is typed), `msp.tranche_reconciliation`
(declared state vs the authorising review and the projects actually delivering) and
`architecture.phase_progress` (A–H per project with its contract and compliance review). An
artefact that needs a decision nobody made is published as absent, never invented. The L1
scorecard and the L2 rails read those sections plus the `rework` register, whose denominators
come from `governance/conformance.json`, not from the view.

**The MSP, TOGAF, SAFe and PMP artefacts are generated documents.** `seed/output-standard.json`
v3 names, for every artefact, its standard, level, subject kind (programme, project, tranche,
benefit, capability, phase or card), mandatory flag, SPEC clause and the source each required
section derives from. `scripts/generate-standards.py` resolves every applicable artefact against
the snapshot, its published documents and the gap register, then publishes `data/standards.json`,
one page per artefact under `docs/standards/` and an index. A section with a named row or document
renders that record and its provenance; a section without one is *absent*, with the fixed line
naming the record that would satisfy it. The four levels link to the artefacts they own, and an
absent or partial artefact names the rework package that must close. The generator never calls a
model: the artefact record is derived, not drafted.

**The record is committed, not only buildable.** `docs/design/scripts/publish-standards.sh` runs
the same pass-1/pass-2 pipeline against the committed snapshot with no database and installs
`data/standards.json`, `data/standards.js`, one page per artefact under `docs/standards/` and the
register global `data/rework.js`; the canonical register is `data/rework.json`, written only by
the live publish, and `docs/evidence/rework-register.json` is its evidence copy (the same object,
serialised by the audit), with `docs/evidence/standards-findings.json` as its findings ledger.
`--check` compares the committed pack with a fresh build, and `scripts/ci/tests/standards_pack.sh`
validates the committed record, register and findings ledger as a published pack (schema, summary
arithmetic, package linkage) — the record cannot silently disappear from the served site again.
A write is refused when the snapshot derives a different register than the published
one: that means the issue-driven seed has moved past the last live publish, and a record written
there would name packages the published register lacks.

**The rework loop.** `scripts/audit-programme-conformance.py --rework-out` turns the gap register,
the checked-in v1 audit (`docs/evidence/v1-conformance-findings.json`) and the artefact findings
(`docs/evidence/standards-findings.json`) into the register the live publish writes
(`data/rework.json`, evidence copy `docs/evidence/rework-register.json`): one package per
remediation unit, carrying its standard, SPEC clause,
artefact, severity, evidence and Definition of Done. A data gap and an artefact that describe the
same missing record are one package, never two: the generator suppresses the artefact finding when
the gap register already owns the defect. The live publish carries the register into `snapshot.js`
(`conveyor snapshot --conformance`); a pack published from the committed snapshot carries it as
`data/rework.js`, and the public view reads either (`live.rework || CONVEYOR_REWORK`).
`scripts/materialise-rework.py` turns an open package into a
card with `meta.rework_key` on the owning project's board. Materialisation is a separate,
explicit, dry-run-first command with its own writable URL — the publish transaction stays
read-only. A card already applied is never duplicated; a package no longer in the register is
reported cleared, never auto-archived.

**The output standard** is data: `seed/output-standard.json` owns the vocabulary and the
per-project-class due-now and at-closure sets. `scripts/build-programme-seed.py` writes
`meta.expected_outputs` / `meta.closure_outputs` into the seed and `data/output-standard.js` for
this console; `documents.compose` and `documents.update` canonicalise the legacy aliases (`brief`,
`analysis`, `architecture`) and refuse anything else with `422 unknown_category`. The project
checklist counts the due-now set and shows the at-closure set separately; the first missing due-now
output is **next**. The **Programme queue** on Portfolio is the programme-level work list — every
project's published vs expected outputs, its closure count and its next output, ordered by tranche.
The public project page shows published counts only; the public queue never counts a private draft.

**The authoring standard** (`seed/output-standard.json` version 3) carries a `templates` map: one
entry per category with its required headings, optional headings, a Markdown skeleton and the
standards metadata the generator reads. The category list is the shared vocabulary: the output
classes' due-now/closure sets use it, and the standards catalogue marks the artefact categories it
generates as `on_demand` so a new artefact category never widens a project's output checklist. The
console seeds the skeleton on compose and shows a live **completeness** panel; **Preview** renders a
saved draft with the same `conveyor_render::markdown` the public page uses. The server blocks the
transition to public — through `documents.publish` or a `documents.compose`/`documents.update` that
sets `publishable: true` — with `409 output_incomplete` naming every missing heading; an unpublish
and an already-published row are never checked, so the outputs that predate the templates stay
public. The admin's review queue can **Return** a requested draft with a note (`documents.return`:
the request clears, `meta.rework` records the note and the missing sections in one UPDATE, and
`document.returned` is the event) or **Open a kata** — a published `decision` on the kata project
carrying `{kata, system_issue, source_document_id, source_project_id, coach_note}` — which returns
the source with a note linking the kata. A returned draft shows the **returned** state in the
register and its project.

The **Projects** destination is the programme workspace: the tenant's projects, a project's board,
plan link and linked issues, its cards with their process binding and the card type's expected
outputs, its text **outputs** (compose, edit, request publish, admin publish, history, attachments)
and the public feedback inbox. Intake binds the card type's READY process (`meta.process_key`), so a
card can **Start run** (`process_runs.start`) and open in the Execution room. **The board is
optional**: a project without one (an archived row, or a row from before the seed's board-ensure)
still opens its header, checklist and outputs; cards, runs and intake need a board and say so.
Signed in, the New/Edit project dialogs take a **delivery class** and a **tranche**; the class is
resolved from `seed/output-standard.json`, and its due-now and at-closure sets are written to
`meta.expected_outputs`/`meta.closure_outputs` through `projects.update`, so a project created here
appears in the programme queue. The class picker is generated from the standard, so #221's added
classes appear without a code change; the seed owns its own 48 rows and a later `migrate --seed`
restores their class and sets. An admin's publish sets the output's public bit; the live route
serves it immediately. The old homepage mirror at `nonagentic.ai/design` was retired on
2026-11-18: that path 308-redirects to the app-served pack, and the vendored copy is gone.

The **Execution room** lists the tenant's runs (`runs.list`), renders one run's steps, tool calls,
gates and outputs (`runs.get`), cancels a live run (`runs.cancel`) and decides a pending gate
(`approvals.decide`). **Portfolio** signed in reads the real programme from the API — projects,
linked units, loaded boards and `meta.depends_on` edges — never the design fixtures. **Contribute**
is the public entry point: it lists the `status:ready` units from the issue registry, the four-check
claim protocol, the 5/15–30/30+ first-contribution paths, the small-fix path and the credit links.
Issue status is a published snapshot to verify on GitHub, and the page writes nothing.

The **Validation** page is the measured programme: every project's status with its evidence, its
tranche and dependency edges, its units and its gap list; every screen's
live/mixed/proposed/legacy state; and every action the pack invokes. The data is
`data/coverage.json`, which `scripts/build-coverage.py` derives from the slice frontmatter, the
migrations, the compiled action-registry dump and `v2.js` — never a hand-written status.
`docs/design/scripts/check-coverage.cjs` is the gate, and the `design-pack` CI job runs it with
the eight pack checks and `build-docs.py --check`.

- **Portfolio & plan:** the computed in-flight set, programme shared configuration, typed
  project-dependency edges, the WSJF backlog, PI cadence, capacity and per-project metering; the
  Plan **WBS**, **Baseline vs forecast**, **Registers** and **Cadence** tabs read and write the
  project's store rows signed in (`wbs.*`, `baselines.list`/`take`, `registers.*` and
  `project_dependencies.*`), and render the published `msp` snapshot read-only signed out. The
  earlier WSJF, capacity, metering and change-control fixtures stay the study.
- **Organisation & jobs:** industry → ordered departments → jobs, the full job anatomy, immutable
  releases, tenant-scoped bindings and the stale-binding refusal; the authority matrix, approval
  policies, the rule → laya → generation → human decision ladder, tenant refinements and laya config.
- **Procedures & authoring:** capability → procedure → step, SOP intake (single/batch), RDF/OWL and
  BPMN/Visio/draw.io conversion, the PDDL action space, the knowledge-pack pairing and publish;
  chat/document intake drafts every tenant asset behind a review gate.
- **Tools:** toolkit versions, bindings and effect ceilings, the `tool_call` ledger, probes, BYO
  OpenAPI/MCP tools, governed tool authoring, the direct MinIO storage adapter and credential
  profiles. V1's coding catalogue (367 API operations) remains as a banner-labelled reference.
- **Improve & operations:** coach triage with the closed sets, kata six Gets, lessons with
  independent evidence and human adoption; incidents, on-call, runbooks, dry-run + dual-control
  writes, postmortems and the andon cord.
- **Measure:** the retained S17–S21 evals, benchmarks, hillclimb and prompt audit, plus metering
  roll-ups and drill-down from tenant to tool call with billing reconciliation.
- **Knowledge & memory workbench:** effective job-release configuration over industry → ordered
  departments → job (with `department_conflict` resolution and dependents), the seven-state source
  transformation with source/passage inspection and retrieval tests, the memory record lifecycle
  (Current/History/Derivation, successor/exclude/digest actions), the review & release queue, the
  explain-context manifest and per-object evaluation with memory-policy comparison.
- **System architecture:** ArchiMate 3.2 high-level views (organisation and work, platform and
  integrations, deployment) and pre-rendered Mermaid low-level design (component context, run and
  broker sequences, run state, decision ladder, three ER diagrams, knowledge pipeline, memory
  lifecycle, improvement loop and slice DAG), plus a UI map for every sidebar destination. The
  Mermaid sources live in `architecture/diagrams/`; the light and dark SVGs are committed, local
  and pre-rendered — the page has no runtime Mermaid dependency.
- **Documents:** every tracked design/spec/plan/evidence document is rendered from Markdown to a
  standalone HTML page under `docs/` and listed by the Documents destination.
- **Retained V1 surfaces:** programme control, boards, execution, decisions, requirements,
  principles, ArchiMate/UML, assurance, baselines, TOC (optional reasoning study), knowledge design,
  eight memory levels, and the tenant-administration screens documented in `STUDIO-SPEC.md`. The
  **Projects** destination is the live/snapshot programme workspace, not the retired V1 fixture.

Without a snapshot, every service result is an offline fixture. With `data/snapshot.js`, the
platform's public rows replace those fixtures on the pages that have a live source;
`docs/design/scripts/check-data.cjs` verifies the manifest hashes, the secret-metadata-only rule
and the email/token redaction. See [STUDIO-SPEC.md](STUDIO-SPEC.md) for sources,
boundaries and verification.

The **Organisation & Jobs** pages are the first destination whose mock data became the real seed:
`seed/organisation.json` (transcribed from this pack's `v2.js` fixtures and SPEC_V2 §4.6) is an
image input, compiled by `migrate --seed` into industries, the five ordered software departments
and the catalogue positions, with one immutable release per described job. A publish never rewrites
a seed: a seed change is a repository PR, a new image and a `migrate --seed` run.

The pack is served at **https://conveyor.nonagentic.ai/design/** by the app image;
**https://nonagentic.ai/design/** 308-redirects there (the homepage repository's vendored copy was
retired on 2026-11-18). Opening `studio.html` directly is still the offline entry point. The
image's `/design` route also carries the generated documents (`docs/`) and the data directory (the
placeholder snapshot and the public issue registry) beside the runtime files, so the console's
Documents destination and data links resolve. `node
docs/design/scripts/check-console.cjs` is the console gate: a signed-out pack makes zero API
requests, a signed-in one sends the actions it renders, and the 401/412 paths are rendered.

## Documents

`scripts/build-docs.py` renders every `docs/**/*.md` and the root `README.md` to
`docs/<repo path>.html` (for example `docs/SPEC_V2.html`, `docs/plans/00-INDEX.html`) and writes
the index to `docs/documents.json` + `docs/documents.js`. The pages are standalone — local fonts,
local stylesheet, no fetch — so they work from `file://` and from the published site.

- Links between documents resolve to their generated `.html` page; links to any other tracked file
  resolve to the GitHub `main` URL, so a reader never lands on a 404.
- Regenerate with `python3 docs/design/scripts/build-docs.py` (requires pandoc) and commit the
  output. `node docs/design/scripts/check-docs.cjs` is the gate; it needs no pandoc.
- The Documents destination in `studio.html` reads `docs/documents.js`. Without that file (a clean
  checkout before generation) the destination says so instead of rendering a broken list.

**Platform outputs** are the second document source: a publishable text output is served live at
`/design/docs/output-<id>.html` and indexed by `/design/docs/outputs.js`, both read from Postgres
for the public tenant. The committed `data/outputs.js` placeholder is only the clean-checkout
fallback; the live route shadows it. `conveyor snapshot --outputs-dir` writes the same bytes for
an offline pack, and a parity test holds the two to one renderer. A body is rendered with raw HTML
neutralised, so the public page has no executable markup from an author.

**Standards artefacts** are the third generated document source: `scripts/generate-standards.py
--html-out docs/standards` writes one page per MSP/TOGAF/SAFe/PMP artefact plus an index, each
carrying its status, resolved sources, gate state and the package that must close. The publish
runs them in pass 2, after the strict conformance gate, so a failed publish never writes a
standards page; `check-standards.cjs` and `check-data.cjs` gate the published record.

## Feedback

The public surface is GitHub, not a service. Every public programme page renders **Feedback &
discussion** (the repository's Discussions) and **Report this screen** (a prefilled issue with the
`feedback` label, carrying the page, snapshot, git SHA and URL) in its live data panel; the
snapshot's `feedback_url` carries the repository target. The panel names only the files the public
window serves — `data/snapshot.js`, `docs/outputs.js` and `docs/documents.json` — never the old
404 data files. A standalone output page carries a **Discuss this output** link. The links are
anchors: the public page makes no GitHub call and no `/api/` request. The issue template is
`.github/ISSUE_TEMPLATE/feedback.yml`.

The console lists the `feedback`-labelled issues on the Projects destination and can **intake** one
into a project's intake lane (`cards.create` carrying `meta.issue`); the issue stays the GitHub
unit and is not modified.

## Retired studies

The earlier v1 studies are preserved under [`legacy/`](legacy/) and are **not** published by
this pack or linked from the site; the SPEC_V2 studio above replaces them.
