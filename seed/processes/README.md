# `seed/processes/` — the software-development process catalogue (tenant content)

The process model (SPEC D6/D20/D22/D28/D29/D30) makes a **process** the executor: an ordered list
of steps, each judged against its **SOP** (the `procedure` table). This directory is ONE catalogue:
the 280 per-type processes (70 card types × four list stages) and the 53 named processes (`cap.*`,
`ask_fr`, `kata.open`) that describe a software-development lifecycle.

⛔ **There is NO universal tenant seed.** A tenant may run accounting, marketing, software
development, or project management with human members; installing this catalogue into a new
tenant would presuppose its business. It is installed **only** by an explicit, per-tenant request:

```bash
conveyor migrate --process-catalogue --tenant <slug>
```

It never rides `migrate --seed`, and `projects.create` never requires it: with no published start
process the project, board and start card are still created, and no run is started.

The files are *content*: the loader imports them into `process`, `process_step`, `procedure` (SOP)
and `capability_agent`; nothing here is compiled into behaviour, and the database is the only
runtime source of truth. A tenant's later edits through `processes.*` are never overwritten (the
loader skips any published key).

## Files

| path | what |
|---|---|
| `types/<card_type>.yaml` | one card type: its SOPs, four list processes, and its process agent |
| `named.yaml` | the 51 `cap.<family>.<fn>` + `ask_fr` + `kata.open` processes |
| `manifest.yaml` | the generated index: 280 entries, `key → {type file, list}` |
| `maps.yaml` | the three `process_map` phase DAGs (`it`, `kata`, `py2rust`); applied by `apply_all` after `named.yaml` |
| `workflows/<name>.yaml` | **NOT SHIPPED — a deliberate scope call, not a blocker.** The reusable tool sequences would be published `workflow` rows a process `call` step resolves (process-first, workflow-second). The earlier note here claimed the repo had no action a sequence could name; that is FALSE — `coding_jobs.create\|get`, the `cards.*`, `requirements.*`, `documents.*` and `artifacts.*` families are all registered. What is missing is the AUTHORING: the old n8n tool sequences are not being ported (owner audit 2026-11-10, § Scope correction: re-authoring the 348 flows is not a launch requirement). The `call` step's workflow limb is proved by `crates/actions/tests/call_step.rs`; the catalogue's own calls target published PROCESSES (`kata.open`). |


## Coverage: which catalogue types have processes, and which do not

The 280 type processes are the **n8n matrix** — 70 card types × four list stages. `seed/catalogue.json`
declares 74 card-type codes; the five `wf_*` codes (`wf_andon_scope`, `wf_change`, `wf_observe`,
`wf_probe`, `wf_root_cause`, all placed in the `kata` project type) are **not in that matrix** and
therefore have no type file. That is a boundary, not a missing import: a card of such a type has no
published process, and `process_runs.start` by key is the only way one runs. `phase_coverage_check`
is the inverse — a type file authored for the flows the ledger targets, with no `catalogue.json`
entry — and `the_manifest_matches_the_type_files_exactly` asserts exactly the 70 × 4 = 280 the
matrix defines.

## `maps.yaml`

The three project maps shipped today:

| key | project type | phases, in order |
|---|---|---|
| `it` | `it` | setup → preliminary → analysis → design → execution → test |
| `kata` | `kata` | setup → preliminary → execution |
| `py2rust` | `py2rust` | setup → plan → port → parity → cutover |

Each entry is a `process_map` document (`schema`, `nodes`, `edges`). Its phase nodes repeat the
project type's `meta.phases` from `seed/catalogue.json` exactly — the loader fails closed if a
node does not resolve. At apply time the loader resolves the project type
(`catalogue_entry(kind='project_type')`), builds `MapFacts` from the published process keys, the
tenant's capability codes, that project type's `meta.phases` and the `milestone_kind` codes, runs
`conveyor_domain::process_map::validate`, then creates and publishes version 1. A map already
published for its `(key, project_type)` is skipped, so a second apply inserts nothing.

`kaizen`, `meta_procedure`, `meta_prompt` and `program` declare **zero phases and zero card
types** in `seed/catalogue.json`, so no map is derived for them. Giving one of them a phase means
adding its map here too.

## A type file

```yaml
card_type: development_bug_fix          # catalogue card_type code (seed/catalogue.json)
capability: development                 # capability code the type's agents hang off
outputs: [draft_artifact_id, reviewed_artifact_id, published_artifact_id]

agent:                                  # ONE capability_agent for the type
  key: development_bug_fix              # tenant-unique; equals process_key by convention
  role: process_agent
  prompt: agent                         # the compiled prompt recipe (0030's `agent`)
  model: claude-sonnet-5-5              # model registry ref
  max_iterations: 12
  timeout_secs: 120

sops:
  - key: development_bug_fix.ready      # SOP key; every step of the process references one
    title: Bug fix — ready
    definition_of_ready: {checks: [finding_is_confirmed, workspace_imports]}
    definition_of_done:  {checks: [draft_artifact_id]}
    acceptance_criteria:
      - {name: change_is_minimal, text: "The diff changes only what the finding's root cause needs."}
    inspection: {kind: laya, threshold: 0.7, max_rejects: 2}
    outputs: [draft_artifact_id]

processes:
  - key: development_bug_fix.READY
    list: ready                         # ready | doing | qa | done
    title: Bug fix — ready
    steps:
      - position: 1
        kind: agent                     # agent | computed | gate | call
        actor: process_agent            # process_agent | tool_agent | system | human
        action: agent.run
        process_agent: development_bug_fix
        sop: development_bug_fix.ready
        on_result: fail                 # fail | continue | retry
        max_attempts: 1
        input_template: {phase: readying}
      - position: 2
        kind: computed
        actor: system
        action: cards.move
        sop: development_bug_fix.ready
        input_template:
          id: "{{card.id}}"
          list_id: "{{board.lists.active}}"
```

### Rules the loader enforces

1. **Every step names an SOP** (`sop:`), and the SOP must be defined in the same file (or
   `named.yaml`). The publish validator refuses a step without one.
2. **References are symbolic.** `capability:` names a capability code; `process_agent:` names the
   agent **key**; `sop:` names an SOP key; `card_type:` names a catalogue code. The loader resolves
   each to a uuid and fails closed, naming the missing key.
3. **`list` is a catalogue stage**, not a board lane. Board lanes are `intake | active | waiting |
   done | archive`; a computed move addresses one as `{{board.lists.<kind>}}` (the step template
   context exposes them, D21).
4. **A move to the next stage is a computed `cards.move` step**, never a lane-kind shortcut.
5. **Prompts are not stored here.** An agent step names the `agent` prompt recipe; the compiler
   builds the text from the process/SOP/card at run time (0030).
6. **Each type file supplies exactly four processes** — `READY`, `DOING`, `QA`, `DONE`.
   `manifest.yaml` is generated from the files and the loader asserts both directions.

## The four-stage ladder (measured from the v4 design)

| stage | shape |
|---|---|
| READY | one agent (readying) + the computed move |
| DOING | four agents (readying → doing → inspecting → releasing) + the computed move |
| QA | four agents + the computed move |
| DONE | four agents + the computed move |

Per-type content (criteria, outputs, prompts' source material) comes from
`seed/catalogue.json` (`card_types[].definition`) and the v4 capability files under
`~/Movies/n8n-designs/design/choices/v4/capabilities/` (each type's `text`, `phase`, `proof` and
`inspect.criteria`). The per-type criteria ARE the SOP's `acceptance_criteria`.

## `named.yaml`

The 51 `cap.<family>.<fn>` processes (five functions — `consult`, `create`, `inspect`, `plan`,
`unblock` — for nine families, plus the four `project_management.*` and
`kanban_coach.handle_error`), `ask_fr`, and `kata.open`. Same schema as a type process, with
`card_type: null` and `list: null`.

## `manifest.yaml` (generated)

```yaml
version: 1
processes:
  - {key: development_bug_fix.READY, file: types/development_bug_fix.yaml, list: ready}
  # … 280 entries
```
A test in `crates/store/tests/process_seed.rs` asserts the manifest matches the type files
exactly (280 entries, four per file) and that every `process:` target in
`parity/n8n-dispositions.tsv` resolves to a seeded key (the ledger test in
`crates/actions/tests/n8n_migration.rs`).
