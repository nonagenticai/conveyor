# Evidence — tranche-8 PI plan body correction (version 3)

The published `pi-plan` document `01a2229e-663b-7081-816f-da0c5557549a` carried two stale facts
after the first correction: its "Backlog refinement" paragraph still cited the 2026-11-27 cut of
26 open packages (S06d 9 / S13 10 / S19 7), and its Evidence paragraph said the pinned snapshot
held 931 files. The baseline register carries 38 open packages (S06d 13, S13 14, S19 11) and the
snapshot manifest holds 933 entries.

This act rewrites the body in place with `documents.update` (If-Match against version 2, the
admin path for a published text output): the re-estimates now read 13/14/11 with their P0/P1/P2
breakdown. A further edit (version 4) removes the file count from the Evidence paragraph
entirely: the replay record is shared with the tranche-7 lane, so the snapshot's file count
drifts as that lane writes, and the manifest sha256 — not a count — is the snapshot's identity.
The document id is unchanged, so the pi cadence's `meta.documents` link stays valid;
`published_at` and `publishable` are preserved.

The corrected body is the one in `docs/evidence/record/inputs/tr8/document_pi-plan_tranche-8.json`
— the input the journal's `documents.compose` entry names, so the promotion replay composes the
corrected body rather than this local repair. Read back with `documents.get` on the same record.
