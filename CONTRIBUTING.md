# Contributing

conveyor is licensed under the Apache License 2.0. By submitting a contribution you agree that it
is licensed under the same terms (Apache-2.0, section 5), so no separate contributor license
agreement is required. You keep your authorship, and merged contributions are credited in
[`CONTRIBUTORS.md`](CONTRIBUTORS.md) unless the contributor opts out.

## Ways to contribute

You do not need to understand the whole repository, run the programme, or write Rust.

| Time | Contribution | Start here |
|---|---|---|
| 5 minutes | Report design friction | Open [Design feedback](https://github.com/nonagenticai/conveyor/issues/new?template=feedback.yml). Name the page, what you expected and what happened; a screenshot is optional. |
| 15–30 minutes | Review the proposal | Read [`SPEC_V2`](docs/SPEC_V2.md) or the [public design](https://conveyor.nonagentic.ai/design/). Report one unclear term, a missing decision or an assumption that fails in practice. |
| 30+ minutes | Improve docs or code | Take a [good first issue](https://github.com/nonagenticai/conveyor/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22) or a [ready unit](https://github.com/nonagenticai/conveyor/issues?q=is%3Aissue+is%3Aopen+label%3A%22status%3Aready%22); for a small fix, open the pull request directly. |

Feedback is useful even when it disagrees with the design. Explain the scenario you have in mind; a
maintainer can turn an accepted report into a scoped unit of work.

**AI assistants are welcome.** Use one to read the repository, explain a unit or draft a change.
You remain the author: review the diff, run the checks and sign off your own commits.

## What is live, and what is proposed?

V1 is the live application and its build record. V2 is specified in [`SPEC_V2`](docs/SPEC_V2.md);
the public design marks what is live and what is still proposed. Design feedback is most useful
before implementation starts, while a decision is still cheap.

## Small fixes go straight to a pull request

A change that does not alter behaviour — a typo, a broken link, a comment, a docs clarification, or
a test that asserts behaviour already present — can open a pull request without a unit issue and
without a claim. Branch from `main` as `fix/<short-slug>`, describe what and why, and sign off the
commit.

The small-fix path is for human and external contributors. The repository's own agent sessions keep
the stricter session contract in [`CLAUDE.md`](CLAUDE.md) — a unit issue and an
`issue/<n>-<slug>` branch — and their local guard enforces it; that guard is not a requirement for
your pull request.

Anything that changes behaviour or adds a feature starts from a report or a unit issue. A maintainer
triages an accepted report into a `unit` issue — the full contract, with the definition of ready,
acceptance criteria and definition of done — and links the report rather than rewriting it in place.

## Before you write code

- **Find or open the unit.** The queue is the [good first issues](https://github.com/nonagenticai/conveyor/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22)
  and [ready units](https://github.com/nonagenticai/conveyor/issues?q=is%3Aissue+is%3Aopen+label%3A%22status%3Aready%22).
  A unit is ready when it is open, carries `status:ready`, and its body has no `## Claim` block,
  live branch, pull request or claim comment. If none fits, open a [unit proposal](https://github.com/nonagenticai/conveyor/issues/new?template=unit.md)
  or a design report and wait for maintainer triage before coding.
- **Claim it.** With write access, push `issue/<n>-<slug>`; the pushed branch is the lock. Without
  write access, comment your plan on the unit issue; a maintainer applies `status:claimed`, and the
  branch lives in your fork as `<you>:issue/<n>-<slug>`.
- One issue = one branch `issue/<n>-<slug>` = one pull request with `Closes #<n>`.

## Workflow

1. Fork the repository and branch from upstream `main` as `issue/<n>-<slug>`.
2. Sign off every commit with `git commit -s`, certifying the Developer Certificate of Origin 1.1
   (DCO, <https://developercertificate.org/>). The `dco` check fails a pull request whose commits
   carry no `Signed-off-by:` trailer and names the offending commit.
3. Run the local gate before opening the pull request.
4. Open the pull request against `main`; the head is `issue/<n>-<slug>` (from a fork,
   `<you>:issue/<n>-<slug>`). Required checks and a maintainer review gate the merge.
   If you are not a member of the repository, GitHub holds the workflow runs from your pull request
   until a maintainer approves them: the check shows as *Waiting for approval* and nothing is wrong
   with your branch. Only a maintainer can release that hold; the approval is also the first look
   at your change.

## The local gate

Run the same checks CI runs (the `checks` job in `.github/workflows/ci-public.yml`):

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo nextest run --no-tests=fail --color never --locked
cargo test --doc --locked
cargo audit
```

The tests need Postgres 18 and Redis. The `checks` job shows the exact container commands; the
wrapper `scripts/ci/gate.sh` runs the same commands with an ephemeral Postgres 18 and Redis on
random local ports, and removes them on exit.

A docs-only change does not need the Rust suite. CI still runs `secret-scan`, `no-private-refs`,
`dco`, `lint` and `design-pack`, and creates the Rust and image jobs as skipped. For the design
pack, run `node docs/design/scripts/check-docs.cjs` and the other `docs/design/scripts/check-*.cjs`
checks.

## How your work is credited

- A merged contribution gets a row in [`CONTRIBUTORS.md`](CONTRIBUTORS.md): the name and handle you
  prefer, one line on what landed, and the pull request link. Say so in the pull request if you
  would rather not be listed; the work still lands.
- The merge comment names the contributor and the work.
- Release notes list the pull requests and their authors.
- We rebase-merge, so the commits you wrote, with your sign-off, stay yours.

## For maintainers: accepting a contribution

You are the merge decision. A green run is necessary, not sufficient. The
`@nonagenticai/maintainers` team is the maintainer of record.

**Triage.** Answer a first report; it is the most fragile contribution there is. A change that does
not alter behaviour is a small fix — no unit, no claim; the pull request is the record. A change
that alters behaviour needs a unit with acceptance criteria. Label approachable units
`good first issue`; that label is the newcomer queue. Link the report from the unit; never rewrite
the report in place.

**Accept.**

1. Read the diff (`gh pr diff <n>`) and confirm it matches its description.
2. Review a sensitive path yourself: `.github/workflows/**`, `.github/CODEOWNERS`, `SECURITY.md` or
   `scripts/ci/**`. It runs with your authority.
3. Check DCO: every commit's `Signed-off-by:` is the author's. Never sign off for someone else and
   never add a sign-off to their commit.
4. Check that every check the pull request runs is green, not only the required contexts. A
   docs-only pull request skips the Rust and image jobs; that is expected. `dco` and `design-pack`
   are not required contexts, so read them — `design-pack` is what validates a design-pack change.
5. Merge with `gh pr merge <n> --auto` — `main` has a merge queue, so this enqueues the pull
   request and the queue builds it on the latest base. `--squash`/`--delete-branch` are rejected
   while the queue is enabled (the queue squashes; delete the branch after the merge). Confirm
   with `state == MERGED`, never with the command's exit code. If the branch is behind `main`,
   the queue handles it; never force-push a contributor's branch.
6. Credit the work: add the row to `CONTRIBUTORS.md` and comment on the pull request naming it.
   When cutting a release, `gh release create <tag> --generate-notes` lists every pull request and
   author.
7. Close the record: close the unit issue. A small fix that turned out to change behaviour gets a
   unit opened after the fact, with the pull request linked.

A first-time fork pull request may wait for your approval before its workflow runs; approve it from
the pull request's Checks tab.

## What never lands

- A credential, a private hostname, a private address or a private repository path. betterleaks and
  `scripts/ci/no-private-refs.sh` run on every change; a hit fails CI.
- Code imported from a private repository. A rewritten service is written against a captured
  contract with synthetic fixtures.
- A change to `.github/workflows/` outside the CI owner's issue.

## Review and merge

- A contribution from outside the team is reviewed by a maintainer before it merges; the team's
  own pull requests merge through the queue on green, with the janitor doing the enqueue.
- Required checks must be green in every case. A green run alone merges nothing unless the
  author is one of ours (the janitor's allowlist) and the pull request is in the queue.
- The sensitive paths above are owned by `@nonagenticai/maintainers` in `.github/CODEOWNERS`;
  GitHub requests that team's review on every pull request that touches one.
- The programme is published at <https://conveyor.nonagentic.ai/design/>.

## Reporting a vulnerability

Do not open a public issue. See [`SECURITY.md`](SECURITY.md).
