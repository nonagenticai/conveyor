# conveyor

[![CI](https://github.com/nonagenticai/conveyor/actions/workflows/ci-public.yml/badge.svg)](https://github.com/nonagenticai/conveyor/actions/workflows/ci-public.yml)
[![Release](https://img.shields.io/github/v/release/nonagenticai/conveyor)](https://github.com/nonagenticai/conveyor/releases)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

The new conveyor: one Rust application (workspace in `crates/` and `apps/`), Postgres 18, Leptos
web UI.

- Design and roadmap: <https://conveyor.nonagentic.ai/design/>
- Product spec: `docs/SPEC.md`
- Plan index and slice statuses: `docs/plans/00-INDEX.md`
- Delivery protocol: `docs/plans/03-DELIVERY.md`
- Remaining-work audit: `docs/evidence/2026-11-10-plan-audit.md`
- Decisions: `docs/decisions/`

New here? [`CONTRIBUTING.md`](CONTRIBUTING.md) starts with five-minute paths that do not need Rust,
and every merged contribution is credited in [`CONTRIBUTORS.md`](CONTRIBUTORS.md).

Behaviour changes start from a maintainer-triaged GitHub unit issue. The queue is
`gh issue list --label unit --state open`; one issue = one branch `issue/<n>-<slug>` = one PR. A
change that does not alter behaviour — a typo, a broken link, a docs clarification — can go straight
to a pull request; see [`CONTRIBUTING.md`](CONTRIBUTING.md).

Contributing: [`CONTRIBUTING.md`](CONTRIBUTING.md) · Security: [`SECURITY.md`](SECURITY.md) ·
Conduct: [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md) · Licence: Apache-2.0 ([`LICENSE`](LICENSE)).

## Security

Report vulnerabilities privately through the repository's Security tab — see
[`SECURITY.md`](SECURITY.md). Do not open a public issue for a security problem.

## Contributing

Issues and pull requests are welcome. [`CONTRIBUTING.md`](CONTRIBUTING.md) lists the five-minute
first steps, the local gate, DCO sign-off, the small-fix path and how maintainers accept a
contribution; [`CONTRIBUTORS.md`](CONTRIBUTORS.md) records every merged contribution.

## Licensing

The code in this repository is Apache-2.0 and needs no licence key to build, test or run.
`LICENCES_BASE_URL` is **optional**: unset, licence sync is unavailable and every other job
runs unaffected. With it set, sync failures are retried and never written on a guess.

The licensing stack is public too — [`licenses`](https://github.com/nonagenticai/licenses) (entitlements, plans and tokens) and [`license-authz`](https://github.com/nonagenticai/license-authz) (the Envoy `ext_authz` enforcement point). Neither is required to run this code.

## License

Apache-2.0 — see [`LICENSE`](LICENSE) and [`NOTICE`](NOTICE).
