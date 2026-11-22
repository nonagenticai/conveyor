# 0009 — Prompt selection and recipe authoring

Status: Accepted · Date: 2026-11-22 · Issue: #15 · Resolves: S21 §4 §8 §12, SPEC_V2 §4.5 and §7.7

## Decision

1. **A recipe version is immutable.** `(tenant, key, version)` is created once and never edited. A
   change is a new version. The lifecycle is `draft → active → retired`: at most one draft and one
   active version per `(tenant, key)`, both partial unique indexes. `prompt_recipes.activate`
   retires the current active version and activates the target in one transaction; the unique
   active index, not an application check, refuses a race. A retired or draft version stays
   readable by exact pin so an old `inference_call` remains explicable, but a new run may not pin
   it. The default status stays `active`, so seeds and existing rows are unchanged.

2. **`prompt_section.selector` is the one selection config.** It is a JSON object, not a second
   template language. An empty object means the source's default selection. The recipe names
   section *kinds* and a selector; it never carries source text for a rendered kind. `system` is
   still the only literal kind.

3. **Memory is selected from the card, hot first.** The selector may name `kinds`, `limit` (default
   8, maximum 32) and `include_digest`. Selection is the card's newest un-consolidated hot rows
   (`created_at DESC, id DESC`), then only the newest `kind='digest'` row if room remains; hot is
   never displaced by a digest. Rows render oldest-first as `[kind/source#id] body`. The ids are in
   the text, so the selection is part of the hash. An optional section with no rows is absent; a
   required one refuses by name.

4. **Knowledge retrieval happens before the compile, outside any transaction.**
   `compile_for_run` resolves the recipe, facts, memory and the capability's knowledge targets in a
   transaction, commits, then calls the one retrieval path that `knowledge.query` also uses, and
   only then runs the pure compiler. No network call runs inside a compile transaction and no
   second retrieval implementation exists. The query is `selector.query`, else the card name +
   description, capped at 2,000 characters; with no query the section renders pack identities only.
   A missing or failed required pack keeps `409 knowledge_degraded`.

5. **The hash is SHA-256 of the rendered text.** Source identities — recipe version, pack
   `key@version`, memory row ids, knowledge hit ids — are rendered into the text, so they are in
   the pre-image. The compiled text is never stored: `inference_call.prompt_ref`,
   `prompt_version` and `prompt_sha256` are the record.

6. **Laya is outside the compiler.** The judge's `{model, state, questions}` document is built from
   the step output and the SOP's acceptance criteria in `crates/actions/src/process/judge.rs`; the
   decision ladder reads typed facts. Neither consumes the compiled LLM prompt, and the compiler
   never calls laya. One compiler, one judge, two different questions.

7. **The runtime tool list is not prompt text.** The executor passes the grant's closed schemas as
   the model's `tools` array; the capability section names the allowed actions. The prompt
   compiler does not serialize tool schemas a second time.

## Context

S21's v1 seam shipped the recipe table, the pure compiler, budgets and the role section. The
2026-11-10 plan audit (§W15) recorded the three remaining gaps: tenant recipe authoring, memory
selection → `prompt.memory`, and knowledge sections that carry retrieved content rather than pack
ids. SPEC_V2 §4.5 names knowledge, memory, the PDDL request and the curated tool list as the four
sources of an internal agent's prompt, and §7.7 requires one compiler with a recorded hash.

Alternatives rejected: reusing `prompt_section.template` as a query string (it would become a
second, undocumented template language); calling the `knowledge.query` action from inside the
compile transaction (a nested action and a network call inside a transaction); a separate memory
store for prompts (the card ledger is already the record); and storing the compiled text (it
drifts from the sources it was built from).
