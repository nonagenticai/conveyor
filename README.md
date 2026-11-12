# conveyor

The new conveyor: one Rust application (workspace in `crates/` and `apps/`), Postgres 18, Leptos
web UI.

- Product spec: `docs/SPEC.md`
- Plan index and slice statuses: `docs/plans/00-INDEX.md`
- Delivery protocol: `docs/plans/03-DELIVERY.md`
- Remaining-work audit: `docs/evidence/2026-11-10-plan-audit.md`

Work is claimed by a GitHub issue. The queue is `gh issue list --label unit --state open`; run the
`issue-pickup` skill before creating a branch. One issue = one branch `issue/<n>-<slug>` = one PR.
