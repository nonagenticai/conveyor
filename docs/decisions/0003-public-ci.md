# 0003 — CI for a public repository: GitHub-hosted, self-contained, ghcr, Flux downstream

Status: Accepted · Date: 2026-11-12 · Issue: #54

## Decision

`conveyor` is public. Its CI is `.github/workflows/ci-public.yml`, a **self-contained** workflow
that runs **only on GitHub-hosted `ubuntu-latest`**, references **no repository secret**, and ends
at an image push to `ghcr.io/nonagenticai/conveyor`. Deployment is **not** in this repository: Flux
in `gitops-production` watches the ghcr package through the existing `ImagePolicy conveyor`.

Every job is gated on `if: ${{ !github.event.repository.private }}`, so the file is inert (jobs
*skipped*, never red) while the repository is still private and activates with no edit at the flip.

## Context — what was measured (2026-11-12)

| fact | value |
|---|---|
| current workflow | one job: `uses: <gitops repo>/.github/workflows/rust-service.yml@main`, `secrets: inherit` |
| reusable-workflow visibility | a public caller may call only **public** workflows → that job cannot resolve on a public repo |
| org runner groups | `Default` and `amd64runners`, both `allows_public_repositories: false` |
| GitHub-hosted, public repo | free and unlimited standard runners; measured probe: 4 vCPU, 86 GB free on `/`, zero queue wait |
| private-repo billing | the account's past "payments failed" 403 applied to **private-repo** minutes; public standard minutes are not billed (probe above succeeded) |
| archetype private plumbing | registry login, an internal registry host, an object-store-backed sccache, a Flux receiver token |
| live deployment | ns `conveyor`, two Deployments, all containers on the internal registry's `conveyor:latest`, updated by `ImagePolicy flux-system/conveyor` (tag `latest`) |
| repo rulesets / branch protection | 403 "Upgrade … or make this repository public" — free once public |

## Why GitHub-hosted, and why the ARC path is closed rather than widened

A fork pull request runs **the workflow file from the PR's merge commit**, so a fork can change
`runs-on`. Any self-hosted runner group that accepts this repository therefore accepts arbitrary
untrusted code from anyone on the internet, inside the estate's pod network. Flipping
`allows_public_repositories` is not a checkbox; it is publishing the cluster. GitHub-hosted VMs
are free for a public repo and are the only shape where an untrusted `cargo test` or `build.rs`
never touches the estate. Nothing in CI needs a secret, a cluster credential or an internal host,
so no isolation escape is needed either.

## Why the workflow is inline, not a reusable archetype

The estate rule is archetype-first. This is the one case where it cannot hold: a public repository
cannot call a private reusable workflow, and publishing the archetype would publish the estate's
addressing and deploy mechanics. The gates (fmt, clippy `-D warnings`, nextest + doctests, cargo
audit, the image smoke) are copied step-for-step from `rust-library.yml`, with a header comment on
each job naming its archetype counterpart. The private plumbing is dropped, not reimplemented.

## Why ghcr + Flux downstream

The public workflow has `permissions: packages: write` and uses the automatic `github.token` — no
stored credential. On a push to main it pushes `sha-<7>`, `main-<6-digit run>` and `latest`. Flux in
`gitops-production` resolves `latest` (the mechanism the archetype's promote step used) and writes
the digest into the two Deployments, exactly as it does for every other service. The cluster pulls
anonymously because the package is public.

The public CI therefore holds **no** cluster access: no kubeconfig, no receiver token, no registry
credential. A merge to main still deploys, because the image tag is the only contract between the
two repos, and that contract is one-way.

## Base images

`Dockerfile` takes `ARG BASE_IMAGE=docker.io/library/debian:12-slim@sha256:…` (digest-pinned) for
all three stages. **This is an exception to the estate's internal-mirror rule**, and the only one:
a public build cannot reach the mirror, and a digest pin gives the reproducibility the mirror was
there to provide. An estate build can override it with `--build-arg` without touching the file.

## Consequences

- The estate's `validate-no-upstream-refs` rule (no Docker Hub refs) cannot apply to this repo; its
  inverse does: `scripts/ci/no-private-refs.sh` fails CI if any internal hostname, tailnet address
  or private repository path is published. That script is the enforcement half of the "no IPs"
  rule; `gitleaks dir` is the credential half.
- `rbuild --gate` no longer finds a `rust-service.yml` caller in this repo and will fall back to
  archetype defaults; its conveyor parity is a separate follow-up.
- The legacy `.github/workflows/ci.yml` stays until the flip (it is the only real CI while the repo
  is private), self-disables on a public repo, and is deleted by the flip session.
