# SPEC — legacy kag disposition (Rust rewrite)

**Repo:** `nonagenticai/conveyor` · **Location:** `docs/SPEC_LEGACY_KAG.md`
**Date:** 2026-11-11 · **Status:** draft for decision · owner direction 2026-11-11; amended
2026-11-13 (ADR 0005)
**Relationship to `SPEC_V2.md`:** D54 records the direction — legacy kag is rewritten in Rust, not
revived, and the home of each capability is decided **here**. This spec is not a V2 slice; V2 binds
the surviving services as tools and does not depend on this spec's outcome.

---

## 1. Purpose

Decide, per legacy kag capability, whether it **survives** and **where its Rust rewrite lives**. The
owner left three items explicitly undecided — **neo4j, hirag and karma** — and gave the surrounding
rule: most other services either support those three (so their fate follows) or are already needed by
w2v, the agent platform or the conveyor. Since **w2v migrates into the conveyor**, the w2v services
are rewritten in Rust as they migrate.

The question is not "how do we revive kag". `kag` is **off** (owner ruling 2026-10-29, reference
only), every kag repository is archived, and nothing serves it. The question is which capabilities
are still needed and where they belong.

## 2. Constraints (owner rulings)

1. **Rust, not a Python revival.** Archived code is reference; a surviving capability is
   reimplemented.
2. **Shared where both platforms need it.** `strata_ingest` and `strata_rag` are the shared
   ingest/retrieval services; the conveyor uses them at least to the extent the agent platform
   already does (SPEC_V2 §11.2, D54).
3. **`strata_ingest` and `strata_rag` stay separate repositories** (ADR 0005, 2026-11-13). They are
   already Rust, they are shared with the agent platform, and their release cadence and
   open-sourcing are decided separately. The conveyor binds them over HTTP; they are not workspace
   members.
4. **The conveyor is the work system.** A proposal/decision capability belongs to its decision
   ladder (rule → laya → generation → human), not to a separate service, unless a measured consumer
   outside the conveyor needs it.
5. **w2v migrates into the conveyor** and its services are rewritten in Rust, performed by the
   conveyor's development project (T6), not by a hand-built absorption lane (ADR 0005).
6. **External data sources stay behind services** (SPEC_V2 D42) until their own migration.
7. **A capability with no live consumer is retired**, not rewritten (P10). "It exists" is not a
   reason.

## 3. The three undecided capabilities

### 3.1 neo4j — the graph store

**What it is.** The graph database behind the archived graph capability. Nothing serves it.

**Question.** Does any surviving capability need a property graph, and if so where does it live?

| Option | Meaning | Cost |
|---|---|---|
| **A. Drop** | no graph store; relational rows + `strata_rag` cover the need | lowest ops cost; must prove no graph query is needed |
| **B. Graph capability inside `strata_rag`** | a graph index beside the existing retrieval, one service | Rust work in strata; one service to operate |
| **C. Standalone Rust graph service** | its own service and store | highest build/ops cost; only with a measured query load |
| **D. Keep an external graph database** | unmanaged/self-run | a non-Rust dependency on the critical path; needs an explicit exception |

**Default recommendation:** **A** until a measured need exists; **B** is the first choice if a need
appears. A graph database is not a default.

### 3.2 hirag_service — retrieval

**What it is.** The archived retrieval service (hybrid retrieval by name; exact modes to be confirmed
from the archive). Nothing serves it.

**Question.** Which of its retrieval modes does `strata_rag` not already provide, and where do the
missing ones go?

| Option | Meaning |
|---|---|
| **A. Fold into `strata_rag`** | reimplement the missing modes in the shared retrieval service; one retrieval path |
| **B. Separate Rust service** | a second retrieval service beside `strata_rag` |
| **C. Drop the covered modes, keep measured gaps** | port only what `strata_rag` cannot do today |

**Default recommendation:** **A/C** — `strata_rag` is the shared retrieval system, so folding the
missing modes into it is the default; a second service is justified only by a measured gap
`strata_rag` cannot close (K4). The comparison is measured (same queries, same corpus), not
asserted.

### 3.3 karma_service — proposals

**What it is.** Per the AHTAPol record, KARMA produces proposals that human or deterministic
domain-policy actions in the owning services consume. Archived; nothing serves it. The exact proposal
types are to be confirmed from the archive.

**Question.** Is a proposal capability still needed, and is it a service or a use of the conveyor's
decision ladder?

| Option | Meaning |
|---|---|
| **A. Fold into the conveyor** | a proposal is an L2 generation plus an L1/L3 decision; the conveyor already records both |
| **B. Rust service in strata** | knowledge-side proposals used by more than the conveyor |
| **C. Drop** | no live consumer |

**Default recommendation:** **A** if a consumer exists (it is exactly the conveyor's decision
ladder); **C** otherwise.

## 4. Dependents — what follows the three

- A service that **supports** neo4j/hirag/karma stays until its consumer migrates; its disposition
  follows the decision above, and it is listed here once the dependency edges are read from the
  archives.
- A service **already needed** by w2v, the agent platform or the conveyor stays; its Rust rewrite is
  sequenced with the w2v migration (§5), not with this spec.
- No capability is rewritten merely because it exists. Each surviving row needs a named consumer and
  a named replacement.

## 5. w2v migration into the conveyor

- The w2v stage fleet is live and is a **conveyor migration target** (D54). The rewrite is performed
  by the conveyor itself as development-project work (T6), not by hand-built absorption issues
  (ADR 0005).
- Each stage is **rewritten in Rust** as it moves; until then the live service is bound through the
  conveyor broker as a tool (SPEC_V2 §11).
- The orchestrator's role — sequencing a deliverable run — becomes a conveyor procedure over
  cards/runs; it is not ported as a second scheduler.
- A stage moves only when its Rust replacement passes the same acceptance checks as the live stage;
  the two coexist behind the broker during the transition.
- Migration order is decided by measured use and coupling, not by code size.

## 6. Decision log

| # | Decision | State |
|---|---|---|
| **K1** | Legacy kag is rewritten in Rust; no Python revival | **Decided** (owner, 2026-11-11) |
| **K2** | A separate spec decides the home of each capability | **Decided** (owner, 2026-11-11) |
| **K3** | neo4j | **Open** — options §3.1 |
| **K4** | hirag_service | **Open** — options §3.2 |
| **K5** | karma_service | **Open** — options §3.3 |
| **K6** | w2v stage migration order | **Open** — §5 |
| **K7** | First rewrite target | **Open** — after K3–K5 |
| **K8** | `strata_ingest` and `strata_rag` stay external repositories; their open-sourcing is a separate decision | **Decided** (owner, 2026-11-13; ADR 0005) |

## 7. Non-goals

- Reviving kag, its Python services or its n8n integration.
- A second retrieval path beside `strata_rag` unless K4 measures a gap it cannot close.
- A runtime dependency on any archived repository.
- Migrating a capability with no named consumer.
