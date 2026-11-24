# 0020 — The release compiles the knowledge layers

Status: Accepted · Date: 2026-11-24 · Issue: #697 · Resolves: S21 §23 P4, rework `v1-w1a-2`;
implements SPEC_V2 §4.1, §4.2

## Decision

1. **One table holds every layer.** Migration `0072` adds `organisation_layer`: `tenant_id`,
   `level ∈ base | industry | department_archetype | department | job`, exactly one scope column per
   level (`industry_id`, `department_id` or `job_id`, enforced by a CHECK), `kind`, `value` (jsonb),
   `required` (bool), `version`, timestamps, RLS enabled and forced. One live row per
   `(tenant, level, scope, kind)`; a change is a version bump, never a second row. There is no
   second catalogue and no imported manifest — §4.1 makes the conveyor's own tables the catalogue.

2. **Resolution is the declared order, later wins.** `compile_release_tx` reads the placement and
   resolves each kind through base → industry → department archetype → each department in its
   declared `ordinal` → job. The last layer that sets a kind provides the value; the snapshot
   records that value's level and scope key alongside it.

3. **`required` tightens downward only.** A kind is required if any layer in the path declares it
   required; a lower layer may require what an ancestor left optional, never the reverse. The
   resolved `required` flag is part of the snapshot.

4. **Sibling departments must agree, or the job resolves it.** Two departments in the same
   placement setting one kind to different values is `422 department_conflict`, naming the kind and
   the departments — unless a job-layer row sets that kind, which resolves the conflict at the top
   of the path. The rule is evaluated during publish, before any row is written.

5. **A required kind with no resolvable value refuses the publish.** `409 release_not_ready` names
   the kind and the department whose path is short. It is a refusal to publish, never a published
   release with a hole; the release row is not written.

6. **The snapshot names every value's source, deterministically.** The compiled snapshot carries a
   `layers` member: one entry per resolved kind with `value`, `required`, `level` and `source_key`,
   sorted by kind. The release hash is the canonical hash of that snapshot, so two identical
   publishes are byte-for-byte identical and a value's origin is auditable from the release alone.

7. **The layers are authorable.** `organisation_layers.set` (`jobs:write`) upserts one row per
   `(tenant, level, scope, kind)` with its `value` and `required`; `organisation_layers.list`
   (`jobs:read`) reads them by level and scope. A table nobody can write is the `tool_binding`
   defect again (`v1-p111-1`).

8. **No other behaviour moves.** The release remains immutable and pinned; a binding still pins one
   release hash. Existing `job_release` rows are never rewritten — a publish after this unit writes
   a new row whose snapshot includes `layers`; old rows keep their bytes and their hashes.

## Why

SPEC_V2 §4.1: "Knowledge is layered base → industry → department archetype → each department in
declared order (with its region sub-layer) → job. Kinds are declared at every layer; a kind is
required or optional; `required` can only be tightened downward. Sibling departments that set one
key to different values are a compile error (`department_conflict`), resolved only by a job-layer
row. … no resolvable required knowledge → `409 release_not_ready` naming the kind and department."
Nothing in the code reads a layer: `compile_release_tx` assembles the snapshot from
`job_catalogue.definition`, the industry and the placement rows, and neither refusal class exists
anywhere under `crates/`. A job can therefore be released and run with a required kind unresolved,
and the release hash describes a configuration the spec says cannot exist.

The rejection of a second store is forced by §4.1's last paragraph: "The catalogue is the
conveyor's own data … No manifest is imported from another system, and no other system is asked to
validate a release." One table, one writer, one resolver.

## Alternatives rejected

- **Layers inside `job_catalogue.definition`.** One jsonb blob per job cannot express industry,
  department-archetype or per-department layers, cannot name a value's source, and cannot detect a
  sibling conflict — it would encode the same resolution logic in a place no reader can audit.
- **One table per level.** Five tables, five writers and five RLS policies for one concept invite
  drift; the SPEC treats the layers as one chain.
- **Resolving at run time instead of publish.** The release is immutable and pinned; a run-time read
  would make the pin a lie and let a knowledge change alter a running job's configuration.
- **A separate required-kind registry.** Two stores that can disagree about whether a kind is
  required; `required` is a property of the layer row that sets the value.
- **Failing the run instead of the publish.** §4.1 puts the refusal at release: a run that starts
  with an unresolved required kind has already spent a model call by the time it is noticed.

## Consequences

- Migration `0072` adds one table; no existing table changes shape.
- Two actions join the registry (`569 → 571` at the issue's baseline measure); the served-spec lock
  moves by their operations; `jobs:read|write` are reused, so no new scope.
- A publish after this unit carries a `layers` member in its snapshot, so a test asserting a
  literal hash for a new publish moves with the shape; existing release rows are untouched.
- `v1-w1a-2` clears when the unit's acceptance passes; the register records the PR.
- The region sub-layer named in §4.1 is folded into the department level's `source_key` when a
  region exists; no region rows are seeded, and a separate region level is not introduced until
  the spec gives it content.
