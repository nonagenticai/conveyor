# GEMBA-WALK — the epic publish (deployment-gated)

`documents.compose` of `docs/plans/inception/GEMBA-WALK/05-safe-epic.md` as category `epic`.
`epic` is added to `seed/output-standard.json` on this branch; the standard is `include_str!` into
conveyor-domain, so the deployed engine refuses the category until the image built from the merged
main is rolled out. The refusal below is the gate, not a transport failure.


## The refusal (live, 2026-11-27)

    error: documents.compose refused (422): the input does not match the action schema
    command terminated with exit code 1

The deployed engine's embedded standard predates this branch's `epic` category. The composer
publishes when the merged image is rolled out; the read-back id is recorded on the issue then.
