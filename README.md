# conveyor

The new conveyor: one Rust application (workspace in `crates/` and `apps/`), Postgres 18, Leptos
web UI.

- Design and roadmap: <https://nonagentic.ai/design/>
- Product spec: `docs/SPEC.md`
- Plan index and slice statuses: `docs/plans/00-INDEX.md`
- Delivery protocol: `docs/plans/03-DELIVERY.md`
- Remaining-work audit: `docs/evidence/2026-11-10-plan-audit.md`
- Decisions: `docs/decisions/`

Work is claimed by a GitHub issue. The queue is `gh issue list --label unit --state open`; run the
`issue-pickup` skill before creating a branch. One issue = one branch `issue/<n>-<slug>` = one PR.

Contributing: [`CONTRIBUTING.md`](CONTRIBUTING.md) · Security: [`SECURITY.md`](SECURITY.md) ·
Conduct: [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md) · Licence: Apache-2.0 ([`LICENSE`](LICENSE)).
