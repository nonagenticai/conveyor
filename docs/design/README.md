# Web UI design

The latest design is **[studio.html](studio.html)** — the **SPEC_V2 organisational operating model**.
Open it directly in a browser; no build or server is required. When `data/snapshot.js` is present
(written by `conveyor snapshot`), the pages it covers render the conveyor's own rows — Portfolio,
Organisation, Jobs, Metering and the Plan **Boards** tab — and carry a `LIVE` ribbon with the
snapshot hash and timestamp; an unwired destination or tab keeps the `V2 · PROPOSED` ribbon.
The Plan **Boards** tab is a join, not a seed: it renders each project's GitHub issues from the
issue registry fetched at publish, joined to the conveyor's own cards and runs. A unit with no real
card is shown as **not intaken**. V1 surfaces implemented in the Rust app are unlabelled.

**Two surfaces, one pack.** Published at **https://nonagentic.ai/design/** the pack is the
read-only public snapshot: no accounts, no backend call, the privacy contract unchanged. Served by
the app itself at **https://conveyor.nonagentic.ai/design/** the same pack is the **operator
console**: `console.js` finds the same-origin `/config.json`, signs in against the realm the server
already trusts (auth-code + PKCE S256, token in memory), and the Plan **Boards** tab reads and
writes the platform's own rows — intake an issue as a card, move it, comment, claim and release.
Signed out, or on the homepage, nothing changes: no API request and no write control.

The **Projects** destination is the programme workspace: the tenant's projects, a project's board,
plan link and linked issues, its cards with their process binding and the card type's expected
outputs, its text **outputs** (compose, edit, publish flag) and the public feedback inbox. Intake
binds the card type's READY process (`meta.process_key`), so a card can **Start run**
(`process_runs.start`) and open in the Execution room. Signed in it can edit a project
(`projects.update`, If-Match), compose an output and **Publish** — which enqueues a job that builds
the pack and opens a pull request on the homepage repository; it never merges. On the homepage the
same page is read-only from the snapshot.

The **Execution room** lists the tenant's runs (`runs.list`), renders one run's steps, tool calls,
gates and outputs (`runs.get`), cancels a live run (`runs.cancel`) and decides a pending gate
(`approvals.decide`). **Portfolio** signed in reads the real programme from the API — projects,
linked units, loaded boards and `meta.depends_on` edges — never the design fixtures. **Contribute**
is the public entry point: it lists the `status:ready` units from the issue registry, the four-check
claim protocol and the feedback/Discussions links; it writes nothing.

- **Portfolio & plan:** the computed in-flight set, programme shared configuration, typed
  project-dependency edges, the WSJF backlog, PI cadence, capacity and per-project metering; then
  WBS, frozen baseline vs computed forecast, one six-kind register and controlled change.
- **Organisation & jobs:** industry → ordered departments → jobs, the full job anatomy, immutable
  releases, tenant-scoped bindings and the stale-binding refusal; the authority matrix, approval
  policies, the rule → laya → generation → human decision ladder, tenant refinements and laya config.
- **Procedures & authoring:** capability → procedure → step, SOP intake (single/batch), RDF/OWL and
  BPMN/Visio/draw.io conversion, the PDDL action space, the relational PKO, knowledge-pack pairing
  and publish; chat/document intake drafts every tenant asset behind a review gate.
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

The pack is published at **https://nonagentic.ai/design/** by the `nonagentic_homepage`
repository, which copies this directory verbatim into `dist/design/` and serves `studio.html` as
`index.html`. Opening `studio.html` directly is still the offline entry point. The app image also
serves the pack at `/design`: the web stage copies the generated documents (`docs/`) and the data
directory (the placeholder snapshot and the public issue registry) beside the runtime files, so
the console's Documents destination and data links resolve. `node
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

**Platform outputs** are the second document source: a publishable text output is rendered at
publish time to `docs/output-<id>.html` and indexed by `docs/outputs.js`
(`window.CONVEYOR_OUTPUTS = {schema:"design-outputs/v1", …}`); the Documents destination adds a
**Platform outputs** group. A clean checkout carries the null placeholder and shows no group; a
body is rendered with raw HTML neutralised, so the public page has no executable markup from an
author.

## Feedback

The public surface is GitHub, not a service. `studio.html` links the repository's **Discussions**
and opens a prefilled issue with the `feedback` label from every page; the snapshot's
`manifest.feedback_url` carries the target, and `data/feedback.json` publishes the feedback issues
that exist. The issue template is `.github/ISSUE_TEMPLATE/feedback.yml`.

The console lists the `feedback`-labelled issues on the Projects destination and can **intake** one
into a project's intake lane (`cards.create` carrying `meta.issue`); the issue stays the GitHub
unit and is not modified.

## Retired studies

The earlier v1 studies are preserved under [`legacy/`](legacy/) and are **not** published by
this pack or linked from the site; the SPEC_V2 studio above replaces them.
