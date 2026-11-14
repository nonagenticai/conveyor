# conveyor

[![CI](https://github.com/nonagenticai/conveyor/actions/workflows/ci-public.yml/badge.svg)](https://github.com/nonagenticai/conveyor/actions/workflows/ci-public.yml)
[![Release](https://img.shields.io/github/v/release/nonagenticai/conveyor)](https://github.com/nonagenticai/conveyor/releases)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

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

## Security

Report vulnerabilities privately through the repository's Security tab — see
[`SECURITY.md`](SECURITY.md). Do not open a public issue for a security problem.

## Contributing

Issues and pull requests are welcome. See
[`CONTRIBUTING.md`](https://github.com/nonagenticai/.github/blob/main/CONTRIBUTING.md) for the
organisation-wide rules; setup, test and lint commands are in this README. Commits carry DCO
sign-off.

## Licensing

The code in this repository is Apache-2.0 and needs no licence key to build, test or run.
`LICENCES_BASE_URL` is **optional**: unset, licence sync is unavailable and every other job
runs unaffected. With it set, sync failures are retried and never written on a guess.

The licensing stack is public too — [`licenses`](https://github.com/nonagenticai/licenses) (entitlements, plans and tokens) and [`license-authz`](https://github.com/nonagenticai/license-authz) (the Envoy `ext_authz` enforcement point). Neither is required to run this code.

## License

Apache-2.0 — see [`LICENSE`](LICENSE) and [`NOTICE`](NOTICE).
