# T7 — Acceptance evidence (executed 2026-11-24)

## Command

`cargo-guard.py nextest run --locked -p conveyor-actions --test ops --test ops_write --test ops_read --test ops_host`

Summary [   5.752s] 31 tests run: 31 passed, 1 skipped

## Clause-to-test map (SPEC_V2 §12 T7)

| §12 clause | Executed test(s) | Suite |
|---|---|---|
| an injected pod failure is triaged | `an_incident_is_triaged_with_no_write` | `ops.rs` |
| mitigated by an approved write | `an_approved_mitigation_sets_mitigated_at_with_a_read_back`, `the_write_reads_back` | `ops.rs`, `ops_write.rs` |
| resolved and postmortemed | `an_incident_resolves_with_a_linked_postmortem` | `ops.rs` |
| a destructive action without the tenant's configured approvals is refused | `a_destructive_call_without_approval_is_refused`, `an_approval_for_another_target_is_refused`, `a_dry_run_makes_no_write`, `a_caller_without_the_scope_is_refused` | `ops_write.rs` |
| restart, scale, rollback and flag-flip are callable | `the_four_write_verbs_are_callable_and_read_back`, `an_undeclared_verb_is_refused` | `ops.rs` |
| the tenant's approval policy sets the quorum | `the_tenant_policy_sets_the_quorum` | `ops.rs` |
| a systemic finding opens exactly one kata | `a_systemic_finding_opens_exactly_one_kata` | `ops.rs` |
| k8s read actions | `an_unregistered_host_refuses`; `the_live_read_returns_the_pod_list` (skipped: its harness needs a live API stub) | `ops_host.rs`, `ops_read.rs` |

## Executed tests

```
PASS [   0.019s] ( 1/31) conveyor-actions::ops support::redis::tests::a_private_url_falls_back_to_the_container
PASS [   0.019s] ( 2/31) conveyor-actions::ops support::redis::tests::a_private_url_from_the_environment_wins_without_starting_a_container
PASS [   0.020s] ( 3/31) conveyor-actions::ops support::redis::tests::a_private_url_from_the_environment_wins
PASS [   0.010s] ( 4/31) conveyor-actions::ops_host support::redis::tests::a_private_url_from_the_environment_wins_without_starting_a_container
PASS [   0.013s] ( 5/31) conveyor-actions::ops_host support::redis::tests::a_private_url_from_the_environment_wins
PASS [   0.014s] ( 6/31) conveyor-actions::ops_host support::redis::tests::a_private_url_falls_back_to_the_container
PASS [   0.017s] ( 7/31) conveyor-actions::ops_read a_machine_caller_without_a_channel_sees_no_ops_reads
PASS [   3.412s] ( 8/31) conveyor-actions::ops_read a_caller_without_the_scope_is_refused
PASS [   0.011s] ( 9/31) conveyor-actions::ops_read support::redis::tests::a_private_url_falls_back_to_the_container
PASS [   0.010s] (10/31) conveyor-actions::ops_read support::redis::tests::a_private_url_from_the_environment_wins
PASS [   0.012s] (11/31) conveyor-actions::ops_read support::redis::tests::a_private_url_from_the_environment_wins_without_starting_a_container
PASS [   3.569s] (12/31) conveyor-actions::ops_host an_unregistered_host_refuses
PASS [   0.090s] (13/31) conveyor-actions::ops_read the_ops_reads_are_on_the_agent_surface
PASS [   3.543s] (14/31) conveyor-actions::ops_read an_unconfigured_client_refuses
PASS [   3.788s] (15/31) conveyor-actions::ops_host the_restart_reads_back
PASS [   0.017s] (16/31) conveyor-actions::ops_write support::redis::tests::a_private_url_falls_back_to_the_container
PASS [   0.016s] (17/31) conveyor-actions::ops_write support::redis::tests::a_private_url_from_the_environment_wins
PASS [   0.015s] (18/31) conveyor-actions::ops_write support::redis::tests::a_private_url_from_the_environment_wins_without_starting_a_container
PASS [   0.015s] (19/31) conveyor-actions::ops_write the_ops_writes_are_destructive_on_the_surface
PASS [   3.893s] (20/31) conveyor-actions::ops_host an_agent_refusal_is_not_a_restart
PASS [   4.016s] (21/31) conveyor-actions::ops an_approved_mitigation_sets_mitigated_at_with_a_read_back
PASS [   4.182s] (22/31) conveyor-actions::ops an_incident_is_triaged_with_no_write
PASS [   4.369s] (23/31) conveyor-actions::ops the_four_write_verbs_are_callable_and_read_back
PASS [   4.404s] (24/31) conveyor-actions::ops an_undeclared_verb_is_refused
PASS [   4.418s] (25/31) conveyor-actions::ops an_incident_resolves_with_a_linked_postmortem
PASS [   4.424s] (26/31) conveyor-actions::ops the_tenant_policy_sets_the_quorum
PASS [   4.575s] (27/31) conveyor-actions::ops a_systemic_finding_opens_exactly_one_kata
PASS [   1.735s] (28/31) conveyor-actions::ops_write the_write_reads_back
PASS [   2.086s] (29/31) conveyor-actions::ops_write a_dry_run_makes_no_write
PASS [   2.127s] (30/31) conveyor-actions::ops_write an_approval_for_another_target_is_refused
PASS [   2.181s] (31/31) conveyor-actions::ops_write a_destructive_call_without_approval_is_refused
```

## Not executed — and not claimed

- The cluster live proof `scripts/ops-live-proof.sh` did **not** run: the operator CLI it drives
  (`$HOME/.claude/skills/conveyor-api/scripts/conveyor-ctl`) is absent on this box, and the
  deployed conveyor is digest `sha256:60898434…` (rollout 2026-11-21T04:38Z). It is the
  deploy-bound item; the manifest's gate is the executed suite above, not this note.
- `the_live_read_returns_the_pod_list` is skipped by its own harness env gate.
