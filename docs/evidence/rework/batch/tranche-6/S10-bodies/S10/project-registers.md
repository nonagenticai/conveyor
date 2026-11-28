# Project Registers — Copilot / AG-UI — one action surface, proposals, conversations

## Risks

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-registers",
  "count": 4,
  "rows": [
    {
      "baseline_ref": null,
      "body": "The copilot does not absorb inference-router (D14). `completions.create` is an external action with a 60-second timeout and a process-local breaker; an outage, a timeout or an open breaker ends the turn `failed` with zero tool invocations. The fail-closed rule is deliberate, but it makes the conversation unavailable whenever the router is, and the suite proves the refusal rather than a fallback.",
      "code": "S10-R1",
      "created_at": "2026-11-28T07:29:25.580266+00:00",
      "id": "01a2210f-124f-7b7d-bd76-9020a9e064f7",
      "impact": 3,
      "kind": "risk",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "docs/plans/slices/S10-copilot-ag-ui.md#5"
        ]
      },
      "meta": {
        "owner": "the operations lead (conveyor-e2e)"
      },
      "owner_job_id": null,
      "probability": "medium",
      "programme_id": null,
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "response": "Track the router's availability with the estate's external probe and record any outage during a delivery window; if the failure rate is material, revisit the retry budget with S14 rather than adding a heuristic substitute.",
      "review_at": "2026-12-11",
      "severity": "medium",
      "status": "open",
      "title": "The model boundary is external, so a router outage stops every turn",
      "updated_at": "2026-11-28T07:29:25.580266+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "Execution takes a conditional claim with a 120-second lease, re-checks the proposer's scopes and invokes under the proposal idempotency key. The crash-replay and double-decision suites prove the tested cases, but a live estate with concurrent workers and long-running actions can still exercise lease expiry and worker replacement in ways a single-process suite does not measure. The risk is a duplicated effect, not a lost approval.",
      "code": "S10-R2",
      "created_at": "2026-11-28T07:29:25.725467+00:00",
      "id": "01a2210f-12e0-7fd6-b737-b123e23e702b",
      "impact": 3,
      "kind": "risk",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "docs/plans/slices/S10-copilot-ag-ui.md#7"
        ]
      },
      "meta": {
        "owner": "the release manager (conveyor-e2e)"
      },
      "owner_job_id": null,
      "probability": "low",
      "programme_id": null,
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "response": "At S14, run the proposal execution with two workers against the deployed queue, record the invocation count for a forced kill between claim and invoke, and re-read this risk against that receipt.",
      "review_at": "2026-12-11",
      "severity": "medium",
      "status": "open",
      "title": "Exactly-once execution depends on the claim lease and the idempotency key together",
      "updated_at": "2026-11-28T07:29:25.725467+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "A secret-bearing action's exact input is sealed with the tenant key in the approval row and only the redacted projection is readable by members. Execution refuses when the seal cannot be opened, which is the safe outcome, but a key rotation or a missing KEK during a delivery window can strand an approved proposal until operations restores the key. The risk is availability of a pending decision, not exposure of the secret.",
      "code": "S10-R3",
      "created_at": "2026-11-28T07:29:25.872902+00:00",
      "id": "01a2210f-1373-7e85-9d4b-cf14fe0cebe1",
      "impact": 3,
      "kind": "risk",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "crates/api/src/copilot/proposal.rs"
        ]
      },
      "meta": {
        "owner": "the operations lead (conveyor-e2e)"
      },
      "owner_job_id": null,
      "probability": "low",
      "programme_id": null,
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "response": "Include proposal execution in the key-rotation runbook, alert on the refusal path, and record the tenant key id on the approval row so a restored key can be matched to the pending proposal.",
      "review_at": "2026-12-11",
      "severity": "medium",
      "status": "open",
      "title": "A sealed proposal input depends on the tenant key being available at execution",
      "updated_at": "2026-11-28T07:29:25.872902+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The plan keeps AG-UI only if S0 parity evidence shows a live consumer, and the tree keeps `agui.rs` as a serializer over the native stream. `agui_parity.rs` proves that a completed turn serializes to the golden frames and that every frame type is one of fr's eight; it does not prove that a deployed consumer reads them. The adapter could be dead weight, and the open question in the plan remains open.",
      "code": "S10-R4",
      "created_at": "2026-11-28T07:29:26.019354+00:00",
      "id": "01a2210f-1406-7169-828a-1a450bc3ba96",
      "impact": 2,
      "kind": "risk",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "docs/plans/slices/S10-copilot-ag-ui.md#5"
        ]
      },
      "meta": {
        "owner": "the programme architect (conveyor-e2e)"
      },
      "owner_job_id": null,
      "probability": "medium",
      "programme_id": null,
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "response": "At S14, measure `/api/agui*` ingress before and after cutover; if no live consumer appears, record the dropped rows in `parity.yaml` and remove the adapter rather than carrying it on faith.",
      "review_at": "2026-12-11",
      "severity": "low",
      "status": "open",
      "title": "The AG-UI adapter's live consumer is not proven by the golden-frame suite",
      "updated_at": "2026-11-28T07:29:26.019354+00:00",
      "version": 1
    }
  ],
  "source": "msp.json#/register_items"
}
```

## Issues

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-registers",
  "count": 5,
  "rows": [
    {
      "baseline_ref": null,
      "body": "Closed in an unauthorised tranche before the tranche review; accepted by the delegate as a retrospective exception under the standing delegation. The project is reworked in its batch and re-closed through the gates.",
      "code": "U3-EXC-S10",
      "created_at": "2026-11-21T01:17:37.715997+00:00",
      "id": "01a1fbae-2b08-70fc-9704-ad41ef5c3be8",
      "impact": null,
      "kind": "issue",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "response": null,
      "review_at": null,
      "severity": "P0",
      "status": "accepted",
      "title": "Out-of-sequence closure accepted as a retrospective exception (S10)",
      "updated_at": "2026-11-21T01:17:37.715997+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "At the 2026-11-27 snapshot the project row records status closed and measured_status built, while the pre-pack receipt `docs/evidence/rework/S10.json` (2026-11-25) is red on the baseline, charter, closure-output, contract, review, closure-gate, PI, WSJF, demo, lessons, artefact-standard and rework checks. The capability and its suites exist; the lifecycle record did not. This pack supplies the missing set retrospectively and records the deferred deployed proof explicitly.",
      "code": "S10-I1",
      "created_at": "2026-11-28T07:29:26.164028+00:00",
      "id": "01a2210f-1497-76d4-823f-5c4f7cc8e918",
      "impact": null,
      "kind": "issue",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "response": "This pack is the repair: the lifecycle set, the receipt and the Phase-G review are authored and content-checked at the pinned snapshot; the deployed UI and router proof stay S12 and S14 work.",
      "review_at": null,
      "severity": "medium",
      "status": "open",
      "title": "The record was closed with no charter, baseline, contract, artefact set or Phase-G review",
      "updated_at": "2026-11-28T07:29:26.164028+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The plan lists `migrations/0011_copilot.sql` and `crates/store/src/jobs/kind/copilot.rs`; neither file exists at the snapshot. The built tables are in `migrations/0018_copilot.sql`, the sweeps are scheduled in `crates/store/src/schedule.rs`, and the behaviour lives in the `crates/api/src/copilot` modules. The plan is still the authoritative scope statement, but a reader following its file list would look in the wrong places.",
      "code": "S10-I2",
      "created_at": "2026-11-28T07:29:26.308018+00:00",
      "id": "01a2210f-1527-702b-8b88-337666445b83",
      "impact": null,
      "kind": "issue",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "response": "Record the built paths in this pack and correct the plan's deliverable block on the next non-retrospective edit; do not renumber or move the migration, which is forward-only.",
      "review_at": null,
      "severity": "low",
      "status": "open",
      "title": "The slice plan's deliverable paths have drifted from the tree",
      "updated_at": "2026-11-28T07:29:26.308018+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The header of `crates/actions/src/agent_surface.rs` says an action outside the surface is `action-not-found`, never `403`. The plan's section 5.2, the S04-D6 note added on 2026-11-21 and `copilot_surface.rs` all assert `403 action-forbidden` for a known but omitted name. The code and the suite agree with the plan; the comment is stale, and a reader who trusts it would misdescribe the refusal.",
      "code": "S10-I3",
      "created_at": "2026-11-28T07:29:26.451428+00:00",
      "id": "01a2210f-15b6-75c3-a3a3-64e53d5e4ed7",
      "impact": null,
      "kind": "issue",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "response": "Correct the module header to the tested refusal when the file is next edited; the pack records the suite's behaviour and does not restate the stale comment as fact.",
      "review_at": null,
      "severity": "low",
      "status": "open",
      "title": "The agent-surface module header contradicts the suite on the omitted-call refusal",
      "updated_at": "2026-11-28T07:29:26.451428+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The plan's build order names eight API suites; the tree carries fourteen `copilot_*.rs` suites plus `agui_parity.rs`. The extra suites cover retry, round cap, truncation, malformed arguments, crash replay, a destructive-target regression and the S16 W7 recipe identity and ledger. The pack records them as found; it does not re-scope S10 to absorb S16, and it does not claim the plan anticipated them.",
      "code": "S10-I4",
      "created_at": "2026-11-28T07:29:26.594428+00:00",
      "id": "01a2210f-1645-7bb0-bb4d-5f3a360d7124",
      "impact": null,
      "kind": "issue",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "response": "Keep the extra suites in the evidence set and name their provenance in the evidence output; reconcile the plan's build order only if a future slice edits that section.",
      "review_at": null,
      "severity": "low",
      "status": "open",
      "title": "The suite set has grown beyond the plan's eight named API suites",
      "updated_at": "2026-11-28T07:29:26.594428+00:00",
      "version": 1
    }
  ],
  "source": "msp.json#/register_items"
}
```

## Assumptions

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-registers",
  "count": 4,
  "rows": [
    {
      "baseline_ref": null,
      "body": "The delivery contract judges acceptance against the record at the pinned rework snapshot; the content checker refuses any id that appears nowhere in it. Every judgement in this pack is read from that snapshot, so if it is incomplete or stale for S10 the pack's facts and any re-close move with it.",
      "code": "S10-A1",
      "created_at": "2026-11-28T07:29:26.738407+00:00",
      "id": "01a2210f-16d5-75bc-9a54-a3da934b68c0",
      "impact": null,
      "kind": "assumption",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "held",
      "title": "The 2026-11-27 rework snapshot is the authoritative record for S10",
      "updated_at": "2026-11-28T07:29:26.738407+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The fourteen copilot suites and `agui_parity.rs` exercise the routes against the test store and loopback HTTP. They are not a deployed chat UI, a production inference router or a live MCP consumer. The pack treats the suite assertions as the local proof and leaves the deployed paths to S12 and S14, so no sentence in the pack depends on a run that does not exist.",
      "code": "S10-A2",
      "created_at": "2026-11-28T07:29:26.882223+00:00",
      "id": "01a2210f-1765-7a85-b7fc-c186f6b22af4",
      "impact": null,
      "kind": "assumption",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "held",
      "title": "The suites are the proof boundary; no deployed copilot run is claimed",
      "updated_at": "2026-11-28T07:29:26.882223+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The proposal tests assert `expires_at IS NULL`, and the turn runner passes no expiry when it creates the approval. The assumption is that the owner ruling of 2026-11-09 still stands: a tenant that refuses to answer stalls its own project, and time must not silently kill a gate. If that ruling changes, the proposal creation and the test must change together.",
      "code": "S10-A3",
      "created_at": "2026-11-28T07:29:27.027517+00:00",
      "id": "01a2210f-17f6-786c-9093-77f11f0b8e27",
      "impact": null,
      "kind": "assumption",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "held",
      "title": "Approvals never expire is the owner ruling the proposal rows follow",
      "updated_at": "2026-11-28T07:29:27.027517+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "A secret-bearing proposal stores the exact input sealed with the tenant key and replays only the sealed value. The suite runs with the sealer configured. The assumption is that the deployed process has the same key material and key id when the proposal executes; if it does not, execution refuses rather than running the redacted projection, which is the designed failure but still a stalled decision.",
      "code": "S10-A4",
      "created_at": "2026-11-28T07:29:27.172943+00:00",
      "id": "01a2210f-1887-7ccf-b949-f37af9ad5719",
      "impact": null,
      "kind": "assumption",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "held",
      "title": "The tenant key used to seal a proposal input is available to the executing process",
      "updated_at": "2026-11-28T07:29:27.172943+00:00",
      "version": 1
    }
  ],
  "source": "msp.json#/register_items"
}
```

## Dependencies

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-registers",
  "count": 3,
  "rows": [
    {
      "baseline_ref": null,
      "body": "The project row records depends_on needs S02, needs S03, needs S04 and needs S06. S02 supplies the event and audit substrate, S03 the approval storage and tenant row-level security, S04 the action registry, the `/mcp` transport and the invocation path, and S06 the approval state machine and decision route. All four are recorded at the same snapshot and the copilot suites exercise the resulting seams.",
      "code": "S10-D1",
      "created_at": "2026-11-28T07:29:27.320421+00:00",
      "id": "01a2210f-191b-76e5-86a2-a13313feba58",
      "impact": null,
      "kind": "dependency",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "met",
      "title": "S10 depends on S02, S03, S04 and S06",
      "updated_at": "2026-11-28T07:29:27.320421+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "S10 declares the conversation stream, the proposal card and the read models as data; it does not build the chat UI. The plan assigns rendering to S12 and the deployed cutover evidence to S14. Until those runs exist the copilot's user-facing behaviour is evidenced at the route and test-database boundary only, and the pack keeps this as an open dependency rather than a delivered outcome.",
      "code": "S10-D2",
      "created_at": "2026-11-28T07:29:27.463696+00:00",
      "id": "01a2210f-19ab-70c3-bc8d-d21b21333a37",
      "impact": null,
      "kind": "dependency",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "S12 renders the declared screens and S14 carries the deployed proof",
      "updated_at": "2026-11-28T07:29:27.463696+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The plan's section 5.5 says one MCP surface serves machine consumers by principal, and S06b supplies the licence-bearer principal and the agent tool list. S06b is not one of the four depends_on edges the project row records, so the pack states the seam without rewriting the row: the copilot route and `/mcp` render the same ActionSpec set, and the agent's own deployed task remains S14's gate.",
      "code": "S10-D3",
      "created_at": "2026-11-28T07:29:27.608029+00:00",
      "id": "01a2210f-1a3b-706c-a873-a07acb9e4aaf",
      "impact": null,
      "kind": "dependency",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "S06b's agent principal is the machine consumer of the same projection",
      "updated_at": "2026-11-28T07:29:27.608029+00:00",
      "version": 1
    }
  ],
  "source": "msp.json#/register_items"
}
```

## Benefits

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-registers",
  "count": 5,
  "rows": [
    {
      "baseline_ref": null,
      "body": "The surface is a query over the same `ActionSpec` set that HTTP and `/mcp` serve, so a change to an action changes every rendering or none. The benefit is structural: fr's 46-file AG-UI surface and its 34 action modules are not carried, and the surface equality suite is the evidence that the two renderings cannot drift silently.",
      "code": "S10-B1",
      "created_at": "2026-11-28T07:29:27.753811+00:00",
      "id": "01a2210f-1acc-7f1b-9bb0-6c58f908f082",
      "impact": null,
      "kind": "benefit",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "One action surface replaces a second copilot registry",
      "updated_at": "2026-11-28T07:29:27.753811+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "A call whose effect is destructive, whose confirmation is not none or whose hints require a proposal is not executed; it creates one `approval(kind='copilot.action')` in the same transaction as the terminal turn, with the exact input sealed and the approval target recorded. The benefit is an auditable question before the effect, proven by the proposal, approval and destructive-target suites.",
      "code": "S10-B2",
      "created_at": "2026-11-28T07:29:27.897762+00:00",
      "id": "01a2210f-1b5c-7fed-a85e-6b6bd427ebde",
      "impact": null,
      "kind": "benefit",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "A destructive tool call becomes a decidable approval",
      "updated_at": "2026-11-28T07:29:27.897762+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The two conversation tables record the dialogue, while tool calls remain `action_invocation` rows cited from the turn meta. The terminal turn and its event commit together, so a reconnect replays the turn once and a crash leaves an interrupted turn rather than a claimed effect. The benefit is a record that can be audited without reconstructing a parallel log.",
      "code": "S10-B3",
      "created_at": "2026-11-28T07:29:28.044318+00:00",
      "id": "01a2210f-1bef-7919-95c1-42e7605d5d90",
      "impact": null,
      "kind": "benefit",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "Conversations are durable and replayable without a second execution log",
      "updated_at": "2026-11-28T07:29:28.044318+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "`copilot_coverage.rs` asserts that every mutating action is in the surface, that an injected unmapped operation is returned by name, and that the one transport exemption is declared. The benefit is preventive: a new screen element that forgets its action fails the build instead of silently joining an allowlist, which is the failure the plan's trap 1 names.",
      "code": "S10-B4",
      "created_at": "2026-11-28T07:29:28.190597+00:00",
      "id": "01a2210f-1c81-784d-bb66-fa2b85abf1de",
      "impact": null,
      "kind": "benefit",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "The coverage gate fails by name rather than widening an exemption",
      "updated_at": "2026-11-28T07:29:28.190597+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The approval row is readable by every member who may list approvals, so the exact input is sealed under the tenant key and the visible input and diff redact every `x-secret` property at any depth. Execution replays only the sealed value and refuses when the seal cannot be opened. The benefit is that a person can decide a secret-bearing action without the secret becoming conversation content.",
      "code": "S10-B5",
      "created_at": "2026-11-28T07:29:28.33902+00:00",
      "id": "01a2210f-1d15-7a05-951c-c34751a705ba",
      "impact": null,
      "kind": "benefit",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "A proposal does not expose a secret-bearing input to approvers",
      "updated_at": "2026-11-28T07:29:28.33902+00:00",
      "version": 1
    }
  ],
  "source": "msp.json#/register_items"
}
```

## Changes

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-registers",
  "count": 1,
  "rows": [
    {
      "baseline_ref": "01a2210f-1db4-7dd2-b55b-ac22df9e2110",
      "body": "The retrospective MSP/TOGAF/SAFe rework changed this project's record: its artefacts were re-versioned with documents and links, its register was written, and its schedule and cost baselines were taken. This change records that rework against the frozen scope baseline.",
      "code": "CH-S10-39f8bc",
      "created_at": "2026-11-28T07:29:41.500927+00:00",
      "id": "01a2210f-5082-73fc-9a57-fa71dd39f8bc",
      "impact": null,
      "kind": "change",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {
        "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
        "impact": {
          "scope": "the retrospective rework's record acts"
        }
      },
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "approved",
      "title": "The retrospective rework of S10",
      "updated_at": "2026-11-28T07:29:41.879864+00:00",
      "version": 2
    }
  ],
  "source": "msp.json#/register_items"
}
```
