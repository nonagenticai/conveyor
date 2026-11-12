# Security policy

conveyor is pre-1.0. Only the latest commit on `main` is supported; there are no maintained
release branches yet.

## Reporting a vulnerability

**Do not open a public issue, discussion or pull request for a vulnerability.**

Use GitHub's private vulnerability reporting on this repository:

<https://github.com/nonagenticai/conveyor/security/advisories/new>

The report is visible only to the maintainers. If the form is unavailable, email
<george@nonagentic.ai> with `conveyor security` in the subject.

Please include the affected commit, a minimal reproduction and the impact you believe it has.

## What to expect

- Acknowledgement: best effort, usually within 5 working days.
- No SLA and no bug bounty.
- We will confirm the issue, prepare a fix and coordinate disclosure with you. Our default
  disclosure window is 90 days after the report, or earlier by agreement once a fix is available.
- We will credit you in the advisory unless you ask us not to.

## Scope

In scope: the code in this repository, its CI workflow and the container images it publishes.

Out of scope: third-party dependencies (report those upstream), the infrastructure this project is
deployed on, and findings that require a compromised maintainer account.

## Safe harbour

We will not pursue legal action for good-faith research that follows this policy, avoids privacy
violations, data destruction and service disruption, and gives us reasonable time to fix the issue
before public disclosure.
