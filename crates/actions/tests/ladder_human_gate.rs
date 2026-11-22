//! v1-w1c-2 — the L3 human gate is a person's act, and a generated draft is applied only
//! through the dedicated `draft.apply` gate (SPEC §7.1, §7.4).
//!
//! Every refusal is proved through the real action: `ladder.record` at the human rung refuses a
//! machine principal and an approval that is not an approved, human-decided gate; and
//! `ladder.apply_draft` refuses an accepted laya decision, a generic human kind, a decision with
//! no card, and a decision whose approval is still pending. The success path is one approved
//! `draft.apply` gate plus the human decision that cites it.

mod support;

use conveyor_actions::ladder::DRAFT_APPLY_GATE;
use conveyor_actions::{Authority, InvokeRequest, invoke, registry};
use conveyor_auth::Role;
use conveyor_domain::ladder::{Outcome, Rung};
use conveyor_domain::{DeciderKind, Decision, DecisionKind};
use conveyor_store as store;
use serde_json::{Value, json};
use uuid::Uuid;

/// A direct invocation as an admin caller. `actor_kind` is what the transports derive from the
/// authenticated principal; a test sets it explicitly to prove the handler's own check.
fn request(action: &str, input: Value, actor_kind: &str, actor_id: Uuid) -> InvokeRequest {
    InvokeRequest {
        credential_alias: None,
        action: action.to_owned(),
        input,
        precondition: None,
        idempotency_key: None,
        approval_id: None,
        actor_id,
        actor_kind: actor_kind.to_owned(),
        role: Role::Admin,
        policy: Default::default(),
        authority: Authority::Internal,
        request_id: Some(format!("rid-{action}")),
        trace_id: Some(format!("trace-{action}")),
        depth: 0,
        app_key: "conveyor-test".to_owned(),
        usage: None::<conveyor_actions::UsageContext>,
    }
}

fn id_of(body: &Value, field: &str) -> String {
    body["output"][field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} is not a string in {body}"))
        .to_owned()
}

fn uuid_of(body: &Value, field: &str) -> Uuid {
    id_of(body, field).parse().expect("a uuid")
}

/// The uuid string `field` of a nested object (a board, a lane) — not of the response envelope.
fn field_id(value: &Value, field: &str) -> Uuid {
    value[field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} is not a string in {value}"))
        .parse()
        .expect("a uuid")
}

/// The one seeded tenant, a project with an active lane, its card, and the generated draft.
struct Fixture {
    pg: support::Pg,
    seeded: support::Seeded,
    project_id: Uuid,
    card_id: Uuid,
    document_id: Uuid,
}

async fn fixture() -> Fixture {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");

    let project = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request(
            "projects.create",
            json!({"title": "Draft gate project", "project_type_code": "delivery"}),
            "user",
            seeded.user_id,
        ),
    )
    .await
    .expect("create the project");
    assert_eq!(project.status, 201, "{}", project.body);
    let project_id = uuid_of(&project.body, "id");

    let view = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request(
            "projects.get",
            json!({"id": project_id}),
            "user",
            seeded.user_id,
        ),
    )
    .await
    .expect("read the project");
    let board_id = field_id(&view.body["output"]["boards"][0], "id");

    let board = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request(
            "boards.get",
            json!({"id": board_id}),
            "user",
            seeded.user_id,
        ),
    )
    .await
    .expect("read the board");
    let lane = board.body["output"]["lists"]
        .as_array()
        .expect("lists")
        .iter()
        .find(|row| row["list"]["kind"] == "active")
        .unwrap_or_else(|| panic!("no active lane in {}", board.body));
    let lane_id = field_id(&lane["list"], "id");

    let card = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request(
            "cards.create",
            json!({
                "board_id": board_id,
                "list_id": lane_id,
                "name": "The card the draft wants",
                "card_type_code": "feature"
            }),
            "user",
            seeded.user_id,
        ),
    )
    .await
    .expect("create the card");
    assert_eq!(card.status, 201, "{}", card.body);
    let card_id = uuid_of(&card.body, "id");

    let document = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request(
            "documents.compose",
            json!({
                "title": "Generated patch",
                "project_id": project_id,
                "body": "the model's proposal",
                "meta": {"generated": true}
            }),
            "user",
            seeded.user_id,
        ),
    )
    .await
    .expect("compose the draft");
    assert_eq!(document.status, 201, "{}", document.body);

    Fixture {
        pg,
        seeded,
        project_id,
        card_id,
        document_id: uuid_of(&document.body, "id"),
    }
}

impl Fixture {
    /// Insert a `draft.apply` gate on the card, pending unless `decide` names the approver.
    async fn gate(&self, decide: Option<(Uuid, DeciderKind)>) -> Uuid {
        self.insert_gate(DRAFT_APPLY_GATE, Some(self.card_id), None, decide)
            .await
    }

    /// Insert a gate of `gate` on the given card and/or project, pending unless `decide`
    /// names the approver.
    async fn insert_gate(
        &self,
        gate: &str,
        card_id: Option<Uuid>,
        project_id: Option<Uuid>,
        decide: Option<(Uuid, DeciderKind)>,
    ) -> Uuid {
        let mut tx = store::begin_scoped(&self.pg.runtime, self.seeded.tenant)
            .await
            .expect("scope the approval insert");
        let new = store::approval::NewApproval {
            run_id: None,
            card_id,
            project_id,
            eval_id: None,
            gate: gate.to_owned(),
            kind: "gate".to_owned(),
            required_decisions: 1,
            requested_by: None,
            expires_at: None,
            callback_url: None,
            correlation_id: None,
            meta: json!({"target": card_id}),
        };
        let approval = store::approval::insert_approval_tx(&mut tx, self.seeded.tenant, &new)
            .await
            .expect("insert the gate");
        if let Some((approver, decided_by_kind)) = decide {
            let decision = Decision {
                approver_id: approver,
                kind: DecisionKind::Approve,
                note: String::new(),
                decided_by_kind,
                via: None,
                item_decisions: None,
            };
            store::approval::record_decision_tx(
                &mut tx,
                self.seeded.tenant,
                approval.id,
                &decision,
                false,
            )
            .await
            .expect("decide the gate");
        }
        tx.commit().await.expect("commit the gate");
        approval.id
    }

    async fn invoke(
        &self,
        action: &str,
        input: Value,
        actor_kind: &str,
    ) -> Result<conveyor_actions::InvokeOutcome, conveyor_actions::ActionError> {
        invoke(
            &self.pg.runtime,
            registry(),
            self.seeded.tenant,
            request(action, input, actor_kind, self.seeded.user_id),
        )
        .await
    }

    /// [`Fixture::invoke`] with the Idempotency-Key a verified `laya.classify` call needs.
    async fn invoke_keyed(
        &self,
        action: &str,
        input: Value,
        actor_kind: &str,
        key: &str,
    ) -> Result<conveyor_actions::InvokeOutcome, conveyor_actions::ActionError> {
        let mut request = request(action, input, actor_kind, self.seeded.user_id);
        request.idempotency_key = Some(key.to_owned());
        invoke(&self.pg.runtime, registry(), self.seeded.tenant, request).await
    }
}

/// Seal the tenant's `laya.token`, so `laya.classify` reaches its service.
async fn seal_laya_token(pg: &support::Pg, seeded: &support::Seeded) {
    let mut tx = store::begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope the laya token");
    store::secret::create_secret_tx(
        &mut tx,
        seeded.tenant,
        "laya.token",
        b"test-laya-token",
        "tenant",
    )
    .await
    .expect("seal the laya token");
    tx.commit().await.expect("commit the laya token");
}

fn human_record(card_id: Option<Uuid>, approval_id: Uuid, kind: &str) -> Value {
    let mut input = json!({
        "rung": "human",
        "kind": kind,
        "answer": "apply",
        "outcome": "accepted",
        "approval_id": approval_id
    });
    if let Some(card_id) = card_id {
        input["card_id"] = json!(card_id);
    }
    input
}

/// A human record that names its project and, when the caller has one, the gate it answers.
fn human_record_for(
    project_id: Option<Uuid>,
    card_id: Option<Uuid>,
    gate: Option<&str>,
    approval_id: Uuid,
    kind: &str,
) -> Value {
    let mut input = human_record(card_id, approval_id, kind);
    if let Some(project_id) = project_id {
        input["project_id"] = json!(project_id);
    }
    if let Some(gate) = gate {
        input["gate"] = json!(gate);
    }
    input
}

/// A `422` whose `errors[]` carry `code` — the shape `invalid()` produces.
fn assert_field_refusal(error: &conveyor_actions::ActionError, code: &str) {
    assert_eq!(error.status, 422, "{error:?}");
    assert!(
        error.errors.iter().any(|field| field.code == code),
        "expected errors[].code = {code:?} in {error:?}"
    );
}

/// 1. A machine principal cannot record a human decision, whatever approval it cites.
#[tokio::test(flavor = "multi_thread")]
async fn a_service_principal_cannot_record_a_human_decision() {
    let fx = fixture().await;
    let approval = fx.gate(Some((fx.seeded.user_id, DeciderKind::Human))).await;

    let refused = fx
        .invoke(
            "ladder.record",
            human_record(Some(fx.card_id), approval, DRAFT_APPLY_GATE),
            "service",
        )
        .await
        .expect_err("a service principal must not record a human decision");
    assert_eq!(refused.status, 403, "{refused:?}");
    assert_eq!(refused.code, "person_required");
}

/// 2. A human decision is refused unless its cited approval is an approved gate with a human
/// `approve` decision — and, when the decision names a card, the approval covers that card.
#[tokio::test(flavor = "multi_thread")]
async fn a_human_decision_without_an_approved_human_gate_is_refused() {
    let fx = fixture().await;

    // No approval at all.
    let without = json!({
        "rung": "human",
        "kind": "gate.precheck",
        "card_id": fx.card_id,
        "answer": "approve",
        "outcome": "accepted"
    });
    let refused = fx
        .invoke("ladder.record", without, "user")
        .await
        .expect_err("a human decision without an approval must be refused");
    assert_field_refusal(&refused, "approval_missing");

    // A pending gate does not authorise a human decision.
    let pending = fx.gate(None).await;
    let refused = fx
        .invoke(
            "ladder.record",
            human_record(Some(fx.card_id), pending, "gate.precheck"),
            "user",
        )
        .await
        .expect_err("a pending approval cannot authorise a human decision");
    assert_field_refusal(&refused, "approval_missing");

    // A resolved gate whose only approve decision is a machine's is not a human act.
    let machine = fx
        .gate(Some((fx.seeded.user_id, DeciderKind::Service)))
        .await;
    let refused = fx
        .invoke(
            "ladder.record",
            human_record(Some(fx.card_id), machine, "gate.precheck"),
            "user",
        )
        .await
        .expect_err("a machine-approved gate cannot authorise a human decision");
    assert_field_refusal(&refused, "approval_missing");

    // An approved human gate on NO card does not cover a decision that names one.
    let mut tx = store::begin_scoped(&fx.pg.runtime, fx.seeded.tenant)
        .await
        .expect("scope the no-card gate");
    let nocard = store::approval::insert_approval_tx(
        &mut tx,
        fx.seeded.tenant,
        &store::approval::NewApproval {
            run_id: None,
            card_id: None,
            project_id: None,
            eval_id: None,
            gate: "gate.precheck".to_owned(),
            kind: "gate".to_owned(),
            required_decisions: 1,
            requested_by: None,
            expires_at: None,
            callback_url: None,
            correlation_id: None,
            meta: json!({}),
        },
    )
    .await
    .expect("insert the no-card gate");
    store::approval::record_decision_tx(
        &mut tx,
        fx.seeded.tenant,
        nocard.id,
        &Decision::approve(fx.seeded.user_id),
        false,
    )
    .await
    .expect("approve the no-card gate");
    tx.commit().await.expect("commit the no-card gate");
    let refused = fx
        .invoke(
            "ladder.record",
            human_record(Some(fx.card_id), nocard.id, "gate.precheck"),
            "user",
        )
        .await
        .expect_err("an approval on no card cannot authorise a decision on this card");
    assert_field_refusal(&refused, "approval_missing");
}

/// 3. A pending approval cannot gate a draft — not through `ladder.record`, and not through a
/// decision row that cites it.
#[tokio::test(flavor = "multi_thread")]
async fn a_pending_approval_cannot_gate_a_draft() {
    let fx = fixture().await;
    let pending = fx.gate(None).await;

    // The real path refuses to record the human decision at all.
    let refused = fx
        .invoke(
            "ladder.record",
            human_record(Some(fx.card_id), pending, DRAFT_APPLY_GATE),
            "user",
        )
        .await
        .expect_err("a pending approval must not become a human decision");
    assert_field_refusal(&refused, "approval_missing");

    // A decision row that cites the pending gate (written before the check, or by a writer
    // that skipped it) still does not apply the draft.
    let mut tx = store::begin_scoped(&fx.pg.runtime, fx.seeded.tenant)
        .await
        .expect("scope the decision insert");
    let decision = store::ladder::record_decision_tx(
        &mut tx,
        fx.seeded.tenant,
        &store::ladder::NewDecision {
            kind: DRAFT_APPLY_GATE.to_owned(),
            rung: Rung::Human,
            project_id: Some(fx.project_id),
            card_id: Some(fx.card_id),
            question: "Apply the generated draft?".to_owned(),
            answer: "apply".to_owned(),
            option_set_version: None,
            probabilities: json!({}),
            confidence: None,
            input_hash: None,
            policy_id: None,
            policy_version: None,
            policy_refinement_id: None,
            refinement_version: None,
            laya_config_id: None,
            laya_config_version: None,
            laya_invocation_id: None,
            laya_request_hash: None,
            laya_response_hash: None,
            actor_kind: "user".to_owned(),
            actor_id: Some(fx.seeded.user_id),
            approval_id: Some(pending),
            latency_ms: None,
            outcome: Outcome::Accepted,
            decided_at: None,
            meta: json!({}),
            options: Vec::new(),
        },
    )
    .await
    .expect("insert the decision that cites the pending gate");
    tx.commit().await.expect("commit the decision");

    let refused = fx
        .invoke(
            "ladder.apply_draft",
            json!({
                "document_id": fx.document_id,
                "card_id": fx.card_id,
                "decision_id": decision.id
            }),
            "user",
        )
        .await
        .expect_err("a pending approval must not gate a draft");
    assert_eq!(refused.status, 422, "{refused:?}");
    assert_eq!(refused.code, "draft_requires_gate");
}

/// 4. A generated draft needs the dedicated `draft.apply` gate: an accepted laya decision, a
/// human decision of another kind, and a decision that names no card all refuse.
#[tokio::test(flavor = "multi_thread")]
async fn only_a_human_draft_apply_decision_can_gate_a_draft() {
    let fx = fixture().await;
    let approved = fx.gate(Some((fx.seeded.user_id, DeciderKind::Human))).await;

    // A human decision of a different kind (the routine classification the finding named).
    let classification = fx
        .invoke(
            "ladder.record",
            human_record(Some(fx.card_id), approved, "request.classify"),
            "user",
        )
        .await
        .expect("record the classification");
    assert_eq!(classification.status, 201, "{}", classification.body);
    let refused = fx
        .invoke(
            "ladder.apply_draft",
            json!({
                "document_id": fx.document_id,
                "card_id": fx.card_id,
                "decision_id": uuid_of(&classification.body, "id")
            }),
            "user",
        )
        .await
        .expect_err("a generic human kind must not gate a draft");
    assert_eq!(refused.status, 422, "{refused:?}");
    assert_eq!(refused.code, "draft_requires_gate");

    // A `draft.apply` human decision that names no card.
    let nocard = fx
        .invoke(
            "ladder.record",
            human_record(None, approved, DRAFT_APPLY_GATE),
            "user",
        )
        .await
        .expect("record the no-card decision");
    assert_eq!(nocard.status, 201, "{}", nocard.body);
    let refused = fx
        .invoke(
            "ladder.apply_draft",
            json!({
                "document_id": fx.document_id,
                "card_id": fx.card_id,
                "decision_id": uuid_of(&nocard.body, "id")
            }),
            "user",
        )
        .await
        .expect_err("a decision that names no card must not gate a draft");
    assert_eq!(refused.status, 422, "{refused:?}");
    assert_eq!(refused.code, "draft_requires_gate");

    // An accepted L1 decision on the card, which the old check admitted. It is recorded through
    // the verified path: prepare the document, run the engine-owned classify call, then
    // evaluate the key that call used.
    fx.invoke(
        "decision_policies.create",
        json!({
            "kind": "gate.precheck",
            "gate": "capability_ready",
            "threshold": 0.8,
            "fail_policy": "closed",
            "action": "block"
        }),
        "user",
    )
    .await
    .expect("create the policy");
    let laya_config = fx
        .invoke(
            "laya_configs.create",
            json!({"model_ref": "jev-latest", "timeout_ms": 2500}),
            "user",
        )
        .await
        .expect("create the laya config");
    assert_eq!(laya_config.status, 201, "{}", laya_config.body);
    let laya_config_id = id_of(&laya_config.body, "id");
    seal_laya_token(&fx.pg, &fx.seeded).await;
    let stub = support::LayaStub::start(json!({
        "answers": {"decision": {"score": 0.9}}
    }))
    .await;
    support::set_env("CONVEYOR_EXTERNAL_LAYA_URL", &stub.base);
    let prepared = fx
        .invoke(
            "ladder.prepare",
            json!({
                "kind": "gate.precheck",
                "gate": "capability_ready",
                "question": "Is the capability ready?",
                "input": {"card": fx.card_id},
                "laya_config_id": laya_config_id
            }),
            "user",
        )
        .await
        .expect("prepare the laya document");
    assert_eq!(prepared.status, 200, "{}", prepared.body);
    let request = prepared.body["output"]["request"].clone();
    let key = "ladder-human-gate-laya";
    let classified = fx
        .invoke_keyed("laya.classify", request, "user", key)
        .await
        .expect("classify through laya");
    assert_eq!(classified.status, 200, "{}", classified.body);
    let laya = fx
        .invoke(
            "ladder.evaluate",
            json!({
                "kind": "gate.precheck",
                "gate": "capability_ready",
                "card_id": fx.card_id,
                "question": "Is the capability ready?",
                "input": {"card": fx.card_id},
                "laya_config_id": laya_config_id,
                "laya_key": key
            }),
            "user",
        )
        .await
        .expect("evaluate the laya decision");
    assert_eq!(laya.status, 201, "{}", laya.body);
    let refused = fx
        .invoke(
            "ladder.apply_draft",
            json!({
                "document_id": fx.document_id,
                "card_id": fx.card_id,
                "decision_id": uuid_of(&laya.body, "id")
            }),
            "user",
        )
        .await
        .expect_err("an accepted laya decision must not gate a draft");
    assert_eq!(refused.status, 422, "{refused:?}");
    assert_eq!(refused.code, "draft_requires_gate");
}

/// 5. The human path: one approved `draft.apply` gate and the human decision that cites it
/// apply the generated draft.
#[tokio::test(flavor = "multi_thread")]
async fn a_human_draft_apply_decision_and_approval_applies_the_draft() {
    let fx = fixture().await;
    let approval = fx.gate(Some((fx.seeded.user_id, DeciderKind::Human))).await;

    let decision = fx
        .invoke(
            "ladder.record",
            human_record(Some(fx.card_id), approval, DRAFT_APPLY_GATE),
            "user",
        )
        .await
        .expect("record the human draft.apply decision");
    assert_eq!(decision.status, 201, "{}", decision.body);
    assert_eq!(
        decision.body["output"]["actor_kind"], "user",
        "{}",
        decision.body
    );

    let applied = fx
        .invoke(
            "ladder.apply_draft",
            json!({
                "document_id": fx.document_id,
                "card_id": fx.card_id,
                "decision_id": uuid_of(&decision.body, "id")
            }),
            "user",
        )
        .await
        .expect("apply the generated draft");
    assert_eq!(applied.status, 200, "{}", applied.body);
    assert_eq!(
        applied.body["output"]["card_id"],
        fx.card_id.to_string(),
        "{}",
        applied.body
    );
}

/// 6. A human decision answers one gate: an approval of another gate on the same card does
/// not authorise it, and an approval of the gate the decision names does.
#[tokio::test(flavor = "multi_thread")]
async fn a_human_decision_cannot_cite_an_approval_for_another_gate() {
    let fx = fixture().await;
    let publish = fx
        .insert_gate(
            "publish",
            Some(fx.card_id),
            None,
            Some((fx.seeded.user_id, DeciderKind::Human)),
        )
        .await;

    let refused = fx
        .invoke(
            "ladder.record",
            human_record_for(
                None,
                Some(fx.card_id),
                Some("precheck"),
                publish,
                "gate.precheck",
            ),
            "user",
        )
        .await
        .expect_err("an approval for another gate must not authorise the decision");
    assert_field_refusal(&refused, "approval_missing");

    let precheck = fx
        .insert_gate(
            "precheck",
            Some(fx.card_id),
            None,
            Some((fx.seeded.user_id, DeciderKind::Human)),
        )
        .await;
    let recorded = fx
        .invoke(
            "ladder.record",
            human_record_for(
                None,
                Some(fx.card_id),
                Some("precheck"),
                precheck,
                "gate.precheck",
            ),
            "user",
        )
        .await
        .expect("the matching gate authorises the decision");
    assert_eq!(recorded.status, 201, "{}", recorded.body);
}

/// 7. A gateless human decision that names no card is bound to its project: an approval in
/// another project does not authorise it, and one in the named project does.
#[tokio::test(flavor = "multi_thread")]
async fn a_gateless_human_decision_without_a_card_must_cite_its_projects_approval() {
    let fx = fixture().await;

    let other = fx
        .invoke(
            "projects.create",
            json!({"title": "Another gate project", "project_type_code": "delivery"}),
            "user",
        )
        .await
        .expect("create the other project");
    assert_eq!(other.status, 201, "{}", other.body);
    let other_project_id = uuid_of(&other.body, "id");

    let foreign = fx
        .insert_gate(
            "precheck",
            None,
            Some(other_project_id),
            Some((fx.seeded.user_id, DeciderKind::Human)),
        )
        .await;
    let refused = fx
        .invoke(
            "ladder.record",
            human_record_for(Some(fx.project_id), None, None, foreign, "gate.precheck"),
            "user",
        )
        .await
        .expect_err("an approval in another project must not authorise the decision");
    assert_field_refusal(&refused, "approval_missing");

    let own = fx
        .insert_gate(
            "precheck",
            None,
            Some(fx.project_id),
            Some((fx.seeded.user_id, DeciderKind::Human)),
        )
        .await;
    let recorded = fx
        .invoke(
            "ladder.record",
            human_record_for(Some(fx.project_id), None, None, own, "gate.precheck"),
            "user",
        )
        .await
        .expect("the project's own approval authorises the decision");
    assert_eq!(recorded.status, 201, "{}", recorded.body);
}
