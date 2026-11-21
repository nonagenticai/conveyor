# Inception record — the five proposed V2 projects

This folder holds the durable inception artefacts for the five proposed V2 projects: **T5 Kata**,
**T6 Development project**, **T12 One organisation loop**, **T15 Tool authoring and BYO** and
**T17 Tenant asset authoring**.

All fifteen are **published** (2026-11-21) under the owner's direction in #527. The inception gate
(`crates/actions/src/programme.rs::assert_inception_published`) is met for each project; this record
authorises no tranche and claims no completion.

## The standard

Each project carries the three categories its `v2-proposed` class declares in
`seed/output-standard.json`:

| File | Category | Required headings |
|---|---|---|
| `00-project-brief.md` | `project-brief` | Outcome, Boundary, Reuse sources, Inception units, Dependencies, Acceptance, Capabilities, Plan |
| `01-inception-analysis.md` | `inception-analysis` | Current state, Reuse sources, Build vs reuse, Risks |
| `02-architecture-design.md` | `architecture-design` | Context and boundary; Aggregates, tables and actions; Decisions; Acceptance; Architecture artefacts |

Each file opens with the SPEC_V2 §8.1 common metadata (status, version, author/date,
reviewer/review date, related project, rationale, published document/contract, phase/plateau,
entries, links). Current-state rows cite `path:symbol`; every future claim is phrased as plan.

`statement-of-architecture-work` is **not** part of this set: it is a T9-only contract kind.

## The live record

The bodies were written and independently reviewed under #347 (2026-11-18). That unit closed on
`publishable = true`, while the inception gate reads `published_at IS NOT NULL` — so the documents
were never actually published and the public index carried them as publishable drafts. This unit
makes the bodies durable here, corrects the briefs, updates the live rows to the exact file bytes,
and publishes all fifteen through `documents.publish` (2026-11-21). It also fixes the snapshot and
outputs-index filters that treated `publishable` as publication.

| Project | Category | Live document (published 2026-11-21) |
|---|---|---|
| T5 | project-brief | `01a1e008-7d05-75d4-ae9d-4e2d485271b4` |
| T5 | inception-analysis | `01a1ee4b-fbb1-7433-8ad7-2e75199c733f` |
| T5 | architecture-design | `01a1ee4c-04d8-757b-ad3d-1807a55e0ae1` |
| T6 | project-brief | `01a1e008-8640-73e4-a90a-6bf8c250a0af` |
| T6 | inception-analysis | `01a1ee4e-ac03-76aa-8f54-81018d8bcd57` |
| T6 | architecture-design | `01a1ee4e-b842-70a4-81ee-aacce62c31a9` |
| T12 | project-brief | `01a1e008-2e2f-71fa-8714-2c4802220b43` |
| T12 | inception-analysis | `01a1ee5f-b75b-76ee-93f3-62903c1661ba` |
| T12 | architecture-design | `01a1ee5f-c33a-70bd-8cb2-a8ca3b379634` |
| T15 | project-brief | `01a1e008-4866-73b3-aff3-4b81913d1e23` |
| T15 | inception-analysis | `01a1ee5f-51f8-7613-8d20-35d470d53ecc` |
| T15 | architecture-design | `01a1ee5f-5b42-70c5-9c35-86f977d14e85` |
| T17 | project-brief | `01a1e008-58e9-772a-87a3-ad2e9a1e8354` |
| T17 | inception-analysis | `01a1ee42-312b-770c-bdae-36a1b077d2e6` |
| T17 | architecture-design | `01a1ee42-3aca-7179-867c-000d986a865e` |

## Publication and what follows

1. The bodies were reviewed independently under #347; the review defects are recorded on that issue.
2. The owner directed publication in #527 (2026-11-21); `documents.publish` set `published_at` and
   `published_by` on all fifteen through the operator transport.
3. The project now satisfies `assert_inception_published`. The tranche's own authorisation remains a
   separate act: these documents do not authorise tranche 10, 11, 12, 13 or 5.

## Verification

- Headings and metadata: the check in issue #527 (P1.1–P5.1).
- Live bytes: `documents.download` must hash to the file's `shasum -a 256`.
- Publication state: `documents.get` must read `publishable = true`, `published_at` non-null.
- Public window: only `published_at IS NOT NULL` rows reach the snapshot and the outputs index
  (`crates/store/src/snapshot.rs`, `crates/store/src/document.rs::list_published_outputs_tx`).
- No private host, path, token or credential appears in any file.
