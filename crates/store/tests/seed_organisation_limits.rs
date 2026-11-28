//! The seeded job definitions carry the budget the store resolver reads (SPEC_V2 §4.2).
//!
//! `crates/store/src/inference.rs` reads `limits.cost_budget_usd` from the release anatomy; a
//! seed that says `budget_usd` compiles a release with no enforceable ceiling
//! (`v1-u5a-budget-1`). The display string is provenance, not the machine value.

const SEED: &str = include_str!("../../../seed/organisation.json");

fn walk(value: &serde_json::Value, limits: &mut Vec<serde_json::Value>) {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(limit) = map.get("limits").filter(|value| value.is_object()) {
                limits.push(limit.clone());
            }
            for value in map.values() {
                walk(value, limits);
            }
        }
        serde_json::Value::Array(items) => {
            for value in items {
                walk(value, limits);
            }
        }
        _ => {}
    }
}

#[test]
fn every_seeded_limit_is_the_budget_the_resolver_reads() {
    let seed: serde_json::Value =
        serde_json::from_str(SEED).expect("seed/organisation.json parses");
    let mut limits = Vec::new();
    walk(&seed, &mut limits);
    assert_eq!(limits.len(), 5, "the seed declares five job limits");
    for limit in &limits {
        assert!(
            limit.get("budget_usd").is_none(),
            "budget_usd is not read by the resolver: {limit}"
        );
        let cost = limit
            .get("cost_budget_usd")
            .and_then(serde_json::Value::as_f64);
        assert!(
            cost.is_some_and(|cost| cost >= 0.0),
            "cost_budget_usd must be a non-negative number: {limit}"
        );
        // `display` was removed on purpose (cae69dc32: no consumer reads it). A reappearing
        // field is fixture drift, not a budget limit.
        let display = limit.get("display");
        assert!(
            display.is_none(),
            "display is not part of an organisation limit: {limit}"
        );
    }
}
