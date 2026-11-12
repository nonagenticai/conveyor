# Contributing

conveyor is licensed under the Apache License 2.0. By submitting a contribution
you agree that it is licensed under the same terms (Apache-2.0, section 5), so no
separate contributor license agreement is required.

- Open an issue before non-trivial work; the unit contracts live in the issue
  tracker, not in a conversation.
- Sign off every commit with `git commit -s`, certifying the Developer
  Certificate of Origin 1.1 (https://developercertificate.org/).
- Run the local gate before opening a pull request: `rbuild --gate`; for the
  design pack, `node docs/design/scripts/check-docs.cjs` and the other
  `docs/design/scripts/check-*.cjs` checks.
- Never commit a credential, a private hostname or a private address. The
  `scripts/ci/no-private-refs.sh` guard and gitleaks run on every change.
