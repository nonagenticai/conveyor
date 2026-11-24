#![forbid(unsafe_code)]
//! The L1 case: what the ladder asks laya, built in one place (S21 step 10, issue #656).
//!
//! SPEC_V2 §7.2's wire shape is `{model, state, questions}`. A caller supplies the **facts** — the
//! text the answer is about, the acceptance criteria, the card/project context; the ladder
//! supplies the **question**: the model (the tenant's `laya_inference_config.model_ref`), the
//! type, the instructions and the scale. `ladder.case` returns the exact body a caller must post
//! to `ladder.precheck`, and `ladder.evaluate` rebuilds it to verify the engine's record of that
//! call — so the same facts must produce the same body, and the body's canonical hash is the
//! witness the verification compares.
//!
//! The wire document itself is [`crate::process::judge::request`]: one builder for every laya
//! question the conveyor asks, so a change to the bridge's contract lands in one place. What this
//! module adds is the two things the L1 path needs and the SOP judge does not: the canonical
//! hash of the body and the idempotency key the call must carry.

use conveyor_domain::TenantId;
use conveyor_store as store;
use serde_json::Value;

/// The question key every L1 case asks and reads (the SOP judge's, so one parser reads both).
pub const CASE_QUESTION: &str = crate::process::judge::JUDGE_QUESTION;

/// One case: the body, its canonical hash, and the claim the call must carry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Case {
    /// `{model, state, questions}` — the body posted to `ladder.precheck`, unchanged.
    pub body: Value,
    /// [`store::idempotency::request_hash`] of `body`. Equality with the engine's recorded
    /// `idempotency_key.request_hash` is what proves the call was about *this* case.
    pub hash: String,
    /// The `Idempotency-Key` the caller must send. Derived from the case, never chosen by a
    /// caller, and tenant-scoped so two tenants' identical facts never share a claim.
    pub call_key: String,
}

/// Build the case for one L1 decision.
///
/// `model` is the tenant's laya config's `model_ref`; `output` is the text the question is about;
/// `criteria` is what it is judged against; `facts` is anything the answer may consume. The
/// numbers are no part of the case: the same inputs must always hash the same.
pub fn build(
    tenant: TenantId,
    kind: &str,
    gate: Option<&str>,
    model: Option<&str>,
    output: &Value,
    criteria: &Value,
    facts: Option<&Value>,
) -> Case {
    let body = crate::process::judge::request(model, output, criteria, facts);
    let hash = store::idempotency::request_hash(&body);
    let call_key = call_key(tenant, kind, gate, &hash);
    Case {
        body,
        hash,
        call_key,
    }
}

/// The claim a case's call must carry.
///
/// The scope of an external action's idempotency row is the action's name, so the key only has to
/// separate cases within `ladder.precheck`: the tenant, the case, its gate and the body's hash.
/// A key that names a different body is refused by [`store::idempotency::lookup`] as
/// `ConflictHash`, which is exactly the property `ladder.evaluate` relies on.
pub fn call_key(tenant: TenantId, kind: &str, gate: Option<&str>, hash: &str) -> String {
    format!(
        "ladder.precheck:{tenant}:{kind}:{}:{hash}",
        gate.unwrap_or("-")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tenant() -> TenantId {
        TenantId::from_uuid(uuid::Uuid::parse_str("018f0000-0000-7000-8000-000000000001").unwrap())
            .expect("a tenant id")
    }

    #[test]
    fn the_same_facts_produce_the_same_case_and_key() {
        let facts = json!({"card": "c1"});
        let criteria = json!([{"criterion": "names the change"}]);
        let a = build(
            tenant(),
            "gate.precheck",
            Some("g"),
            Some("system-1"),
            &json!({"draft": "text"}),
            &criteria,
            Some(&facts),
        );
        let b = build(
            tenant(),
            "gate.precheck",
            Some("g"),
            Some("system-1"),
            &json!({"draft": "text"}),
            &criteria,
            Some(&facts),
        );
        assert_eq!(a.hash, b.hash);
        assert_eq!(a.call_key, b.call_key);
    }

    #[test]
    fn different_facts_produce_a_different_case() {
        let criteria = json!([{"criterion": "names the change"}]);
        let a = build(
            tenant(),
            "gate.precheck",
            None,
            Some("system-1"),
            &json!({"draft": "one"}),
            &criteria,
            None,
        );
        let b = build(
            tenant(),
            "gate.precheck",
            None,
            Some("system-1"),
            &json!({"draft": "two"}),
            &criteria,
            None,
        );
        assert_ne!(a.hash, b.hash);
        assert_ne!(a.call_key, b.call_key);
    }

    #[test]
    fn the_body_is_the_wire_shape_and_names_the_question() {
        let case = build(
            tenant(),
            "gate.precheck",
            None,
            Some("system-1"),
            &json!({"draft": "text"}),
            &json!([{"criterion": "x"}]),
            None,
        );
        assert_eq!(case.body["model"], json!("system-1"));
        assert!(case.body["state"].as_str().is_some_and(|s| !s.is_empty()));
        assert_eq!(
            case.body["questions"][CASE_QUESTION]["type"],
            json!("score")
        );
        assert!(case.call_key.starts_with("ladder.precheck:"));
        assert!(case.call_key.ends_with(&case.hash));
    }

    #[test]
    fn a_tenant_and_a_gate_separate_two_otherwise_identical_cases() {
        let criteria = json!([{"criterion": "x"}]);
        let one = build(
            tenant(),
            "gate.precheck",
            Some("a"),
            Some("system-1"),
            &json!({}),
            &criteria,
            None,
        );
        let two = build(
            tenant(),
            "gate.precheck",
            Some("b"),
            Some("system-1"),
            &json!({}),
            &criteria,
            None,
        );
        assert_eq!(one.hash, two.hash, "the body is the same document");
        assert_ne!(one.call_key, two.call_key, "the gate is part of the claim");
    }
}
