# Contributing

conveyor is licensed under the Apache License 2.0. By submitting a contribution you agree that it
is licensed under the same terms (Apache-2.0, section 5), so no separate contributor license
agreement is required.

## Before you write code

- **Open an issue first.** The issue is the unit of work: it carries the definition of ready, the
  acceptance criteria and the definition of done. Use the `Unit of work` template.
- **Claim it.** Run the `issue-pickup` skill in this repository, or perform the four checks it
  names; the pushed branch is the lock.
- One issue = one branch `issue/<n>-<slug>` = one pull request with `Closes #<n>`.

## Workflow

1. Fork the repository and branch from `main` as `issue/<n>-<slug>`.
2. Sign off every commit with `git commit -s`, certifying the Developer Certificate of Origin 1.1
   (<https://developercertificate.org/>).
3. Run the local gate before opening the pull request.
4. Open the pull request against `main`. Required checks and a maintainer review gate the merge.

## The local gate

Run the same checks CI runs (the `checks` job in `.github/workflows/ci-public.yml`):

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo nextest run --no-tests=fail --color never --locked
cargo test --doc --locked
cargo audit
```

The tests need Postgres 18 and Redis. The `checks` job shows the exact container commands; locally,
bind them to a free port rather than the defaults so two checkouts can run at once. A single
wrapper, `scripts/ci/gate.sh`, lands with the workspace baseline; until then run the commands above.
For the design pack, run `node docs/design/scripts/check-docs.cjs` and the other
`docs/design/scripts/check-*.cjs` checks.

## What never lands

- A credential, a private hostname, a private address or a private repository path. gitleaks and
  `scripts/ci/no-private-refs.sh` run on every change; a hit fails CI.
- Code imported from a private repository. A rewritten service is written against a captured
  contract with synthetic fixtures.
- A change to `.github/workflows/` outside the CI owner's issue.

## Review and merge

- A maintainer reviews every pull request, and required checks must be green.
- A merge is a human decision. A green run alone does not merge anything.
- The design and roadmap are published at <https://nonagentic.ai/design/>.

## Reporting a vulnerability

Do not open a public issue. See [`SECURITY.md`](SECURITY.md).
