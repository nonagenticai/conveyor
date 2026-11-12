# Conveyor workspace UI study

**Follow-up:** [studio.html](studio.html) and [STUDIO-SPEC.md](STUDIO-SPEC.md) extend this study
with the tenant-wide project portfolio, tenant configuration hierarchy, real coding catalogue,
governed-asset CRUD and model/view creation requested in the subsequent review. This file
continues to describe the preserved `web-ui.html` study.

**Status:** interactive HTML design study, 2026-11-10. The binding product contract remains
[`../SPEC.md`](../SPEC.md) and [`../plans/`](../plans/). This document proposes an interaction
model; it does not claim that a pictured feature is implemented in the Rust application.

Open [`web-ui.html`](web-ui.html) directly in a browser. Its CSS, JavaScript and fonts are local.
There is no build step, backend connection or runtime network dependency. The earlier design
essay and wireframes are preserved in [`web-ui-proposal.html`](web-ui-proposal.html).

## Direction

Conveyor should feel like a place to operate an execution system: start with a piece of work,
see how it is being carried out, inspect the evidence, and step in at a decision. The default
project view is an execution canvas with a persistent inspector and a compact execution feed.
The work board is another view of the same work, rather than the whole product.

The mockup deliberately uses a realistic work package across its screens: a customer portal’s
workspace invitation feature, linked from approved requirements through implementation,
verification and a human review. The user can follow that record across the workspace.

### Inputs and their contribution

| Input | Applied pattern |
| --- | --- |
| `../fr/frontend/src/lib/components/cockpit/` | Distinct project lifecycle and card state; next human gate; evidence reached from the cockpit |
| `../w2v_frontend/src/lib/components/runs/StageIOInspector.svelte` | Trace each stage’s input to its output; make provenance inspectable |
| `../w2v_frontend/src/lib/styles/tokens.css` and `src/app.css` | Shared Non-Agentic teal/midnight identity, IBM Plex Sans/Mono and Fraunces headings |
| `../care-bus-agents/design/console/index.html` and `design/admin/` | Operator-first activity, semantic execution colors and contextual inspection |
| `../agent/web/src/routes/console/` | Lead with requests needing a person; inspect a selected execution without losing context |
| [ChatDev](https://github.com/OpenBMB/ChatDev) | Visible workflow topology, role specialization and explicit handoffs |
| [DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness) | An inspectable workbench with conversation, execution and artifacts close together |
| `../plans/slices/S12-ui-buildout.md` | Screen requirements and the existing action/read-model boundaries |
| `../plans/slices/S15-capability-knowledge-memory.md` and `S21-message-construction.md` | Pinned knowledge, readiness, work-package memory and compiled-context provenance |
| `../plans/slices/S16-inference-audit.md` through `S20-eval-hillclimb.md` | Cache classes, proposal-only audits, versioned evaluation instruments and held-out measurement |

These are interaction references, not imported runtimes or an assertion that the platforms
share the same execution architecture.

## Information architecture

| Area | Destination | Primary question |
| --- | --- | --- |
| Operate | Workspace | Where is this work, and what happens next? |
| Operate | Work board | What is ready, executing, inspecting or done? |
| Operate | Runs | What is executing, waiting or failing? |
| Operate | Decisions | What requires my input? |
| Build | Capabilities | What can execute, with which bindings and limits? |
| Build | Procedures | What is the published sequence, and how does a draft change it? |
| Build | Knowledge & memory | What does the executor know, and where did that context come from? |
| Build | Requirements | Which approved intent does this work implement? |
| Improve | Evaluations | Is the system improving against a fixed instrument? |
| Improve | Usage & audit | Which model calls cost money, and which prompt changes are proposed? |
| Connect | Channels | How do the same work and decisions reach Web, Telegram and MCP? |
| Connect | Settings | Which people and agents have access? |

Artifacts and Activity are project views. Inspector tabs expose Overview, Context and Tools.
The command palette searches destinations and a small set of relevant work/actions.

## Main workspace

1. **Project header:** objective and approved brief, followed by a project lifecycle rail.
2. **Project views:** Execution, Work board, Artifacts and Activity.
3. **Attention strip:** one concrete ask, linked directly to the evidence review.
4. **Execution canvas:** selectable procedure, internal-agent, external-executor and gate steps.
5. **Inspector:** selected step, executor kind, output, provenance and observed actions.
6. **Feed:** handoffs and evidence recorded in the sample execution.
7. **Copilot:** contextual interactive help. It does not replace the executor or approval gate.

The graph is a proposed project/work-package execution projection. Its implementation will
need an explicit read model joining the existing run, step, procedure and approval records;
this mockup does not invent a backend endpoint or a new agent runtime.

Project lifecycle, card list and run state remain separate. A card can move without answering
its approval. A completed implementation can belong to a run that is still waiting for review.

## Interactions provided

- Select any execution node and switch inspector tabs. Context exposes pinned source classes;
  Tools distinguishes Conveyor-visible actions from an external platform’s private execution.
- Replay the recorded sample feed; zoom the desktop canvas. The narrow canvas scrolls horizontally.
- Open the implementation review, inspect the patch/report, then approve or return with a note.
  Returning requires a note. The shared inbox, node, feed and run reflect the decision.
- Create a work item and a queued sample run. Filter the board; drag cards between lists or open
  a card and use its list selector with the keyboard.
- Filter runs, inspect their steps/readiness error, and cancel a sample live run.
- Inspect capabilities and create a blocked capability draft; inspect packs and memory facts.
- Inspect requirements and revision/trace links; add a draft requirement.
- Edit and reorder a procedure draft; preview validation/publication. The sample ordering rule
  flags verification after review and permits an explicit audit acknowledgment.
- Refuse an evaluation start while grading sign-off is missing. Review the pilot sample, record
  a person’s sign-off for its digest, then preview a queued evaluation. Filter the case report
  and inspect sample train/held-out traces as the person with approval access.
- Inspect prompt findings and preview accepting/rejecting proposals; no procedure is auto-edited.
- Export small JSON samples for artifacts, evaluation and usage.
- Use scripted copilot responses, command palette, light/dark themes, advanced details and mobile navigation.

Work data lasts for the page session and resets on reload. Only theme preference uses local
storage. A small number of operations are explicitly previews: publishing a procedure does
not create a real published version, starting an eval does not run workers, and prompt-finding
decisions update only the local proposal state, without applying a procedure change. The copilot is scripted.

## Domain rules retained

- **D19/D21:** external agents use Conveyor’s MCP surface and durable project-task/input semantics.
- **D22:** procedure, bounded internal loop and external executor are distinct. The copilot is
  interactive help; it is not the work executor. External model usage is not counted as a
  Conveyor model call.
- **D20/D27/S21:** context comes from versioned procedural, capability/role, knowledge and
  project/card sources. Missing required sources block readiness. The source recipe and hash
  are inspectable; the ledger does not store compiled prompt bodies.
- **D23:** provider cache hits, writes, misses and unknowns remain distinct. The displayed
  75% ratio is `108 / (108 + 36)`; 8 writes and 4 unknown calls are excluded. Audit findings
  propose changes and never automatically apply them.
- **D25/D26:** a versioned eval instrument needs a person’s sign-offs. Held-out results are
  the headline; agents are not given held-out transcripts. Scores and trend are illustrative,
  not measured claims or proof of statistical significance.
- **Identity:** a person and an agent have distinct badges; decisions expose their channel.
  Tenant membership uses admin, not an invented owner role.

## Visual and accessibility rules

Use a midnight green navigation rail, neutral paper surfaces, a restrained teal accent,
Fraunces display headings and IBM Plex for operational reading and identifiers. Amber signals
waiting for a person; purple identifies external execution; status always has text as well as color.
The canvas is code-native SVG and HTML, with no decorative generated imagery.

The inspector sits beside the canvas on wide screens and below it on smaller screens. Tables
and the board scroll inside their own regions. Mobile navigation is an explicit drawer. Native
modal dialogs trap focus; commands work with the keyboard; the inspector tabs support arrow,
Home and End keys; card list selection provides a keyboard alternative to dragging. Reduced
motion preferences are respected.

The font assets are local Google Fonts distributions under their included SIL Open Font Licenses.

## Scope of this study

This is a proposal for the operational center and its adjacent authoring/measurement screens.
It is not an exhaustive rendering of every S12/S13 screen. Full ArchiMate/UML authoring, the
workflow editor, benchmark control comparisons, complete instrument authoring, bulk imports,
platform provisioning, billing integration, invitations and role administration need subsequent
interaction design. The member and channel views here explain the intended boundaries.

No Rust application code, database schema, action contract, deployment or existing application
route is changed by this study.

## Verification

A local headless Chrome smoke pass completed **68 checks** at 1440 px and 390 px, including
navigation, node inspection, keyboard commands, work creation/list moves, approval and return,
run filtering, grading sign-off refusal/acceptance, replay, and responsive overflow checks.
Light/dark screenshots were inspected, and the HTML was also opened successfully with a
`file:` URL. No page runtime errors, failed asset loads or external page requests were observed
(navigation-cancelled requests excluded). JavaScript syntax and `git diff --check` passed.
This validates the prototype interactions, not the production application or its backend.
