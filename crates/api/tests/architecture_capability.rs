//! T9 (#81) D2 — the Preliminary capability: the profile, the Board, maturity and skills.
//!
//! `rbuild -- test -p conveyor-api --test architecture_capability`.

#[path = "architecture_support/mod.rs"]
mod arch;
#[path = "../../actions/tests/support/mod.rs"]
mod support;

use serde_json::json;

const DIMENSIONS: [&str; 9] = [
    "architecture_process",
    "architecture_development",
    "business_linkage",
    "senior_management",
    "operating_unit_participation",
    "architecture_communication",
    "it_security",
    "architecture_governance",
    "investment_strategy",
];

const SKILLS: [&str; 7] = [
    "general",
    "business",
    "architecture",
    "programme_management",
    "it_knowledge",
    "technical",
    "legal",
];

#[tokio::test]
async fn the_profile_names_the_mandatory_set() {
    let f = arch::fixture().await;
    arch::tailor(
        &f.running,
        json!({ "a": { "psa": ["stakeholder_map"] } }),
        json!({ "a": "statement_of_architecture_work" }),
    )
    .await;

    let board = arch::open_board(&f.running).await;
    let board_id = arch::id_of(&board);
    let read = arch::ok(
        &f.running,
        "architecture.board_get",
        json!({ "key": "architecture-board" }),
        None,
    )
    .await;
    assert_eq!(read["id"], board_id, "{read}");
    assert_eq!(read["kind"], "architecture_board", "{read}");
    assert!(
        read["charter"]
            .as_str()
            .unwrap_or_default()
            .contains("PF-1"),
        "the board charter is not the row's: {read}"
    );
    assert_eq!(
        read["commissioning_authority"], "P0 programme board",
        "{read}"
    );

    let profile = arch::ok(&f.running, "architecture.profile_get", json!({}), None).await;
    assert_eq!(
        profile["profile"]["artifact_requirements"]["a"]["psa"][0], "stakeholder_map",
        "{profile}"
    );
    assert_eq!(
        profile["profile"]["contract_kinds"]["a"], "statement_of_architecture_work",
        "{profile}"
    );
    let standard = profile["standard"].as_array().expect("the standard");
    let map = standard
        .iter()
        .find(|row| row["key"] == "stakeholder_map")
        .expect("the seeded standard carries stakeholder_map");
    assert_eq!(map["mandatory"], true, "{map}");
    assert_eq!(map["phase_code"], "a", "{map}");
    assert_eq!(map["plateau"], "psa", "{map}");

    let decision = arch::created(
        &f.running,
        "architecture.board_decide",
        json!({
            "body_id": board_id,
            "decision": "approved",
            "evidence": { "charter": "PF-1" },
        }),
        None,
    )
    .await;
    assert!(decision["approval_id"].is_string(), "{decision}");
}

#[tokio::test]
async fn a_maturity_assessment_needs_every_dimension() {
    let f = arch::fixture().await;
    let eight: Vec<_> = DIMENSIONS[..8]
        .iter()
        .map(|key| json!({ "key": key, "score": 2 }))
        .collect();
    let (status, refused) = arch::call(
        &f.running,
        "architecture.maturity_assess",
        json!({ "stage": "baseline", "scores": eight }),
        None,
    )
    .await;
    assert_eq!(status, 422, "{refused}");
    assert_eq!(
        refused["errors"][0]["code"], "assessment_incomplete",
        "{refused}"
    );

    let all: Vec<_> = DIMENSIONS
        .iter()
        .map(|key| json!({ "key": key, "score": 3 }))
        .collect();
    let out = arch::created(
        &f.running,
        "architecture.maturity_assess",
        json!({ "stage": "baseline", "scores": all }),
        None,
    )
    .await;
    assert_eq!(
        out["dimensions"].as_array().expect("dimensions").len(),
        9,
        "{out}"
    );
}

#[tokio::test]
async fn a_skills_assessment_names_its_role() {
    let f = arch::fixture().await;
    let six: Vec<_> = SKILLS[..6]
        .iter()
        .map(|key| json!({ "key": key, "score": 3 }))
        .collect();
    let (status, refused) = arch::call(
        &f.running,
        "architecture.skills_assess",
        json!({ "role": "solution_architect", "job_key": "architect", "scores": six }),
        None,
    )
    .await;
    assert_eq!(status, 422, "{refused}");
    assert_eq!(
        refused["errors"][0]["code"], "assessment_incomplete",
        "{refused}"
    );

    let all: Vec<_> = SKILLS
        .iter()
        .map(|key| json!({ "key": key, "score": 4 }))
        .collect();
    let out = arch::created(
        &f.running,
        "architecture.skills_assess",
        json!({ "role": "solution_architect", "job_key": "architect", "scores": all }),
        None,
    )
    .await;
    assert_eq!(out["role"], "solution_architect", "{out}");
    assert_eq!(
        out["categories"].as_array().expect("categories").len(),
        7,
        "{out}"
    );
}

#[tokio::test]
async fn maturity_list_returns_every_assessment_dimension() {
    let f = arch::fixture().await;
    let all: Vec<_> = DIMENSIONS
        .iter()
        .map(|key| json!({ "key": key, "score": 3 }))
        .collect();
    let assessed = arch::created(
        &f.running,
        "architecture.maturity_assess",
        json!({ "stage": "baseline", "scores": all }),
        None,
    )
    .await;
    let group = assessed["assessment_id"]
        .as_str()
        .expect("group")
        .to_owned();

    let listed = arch::ok(&f.running, "architecture.maturity_list", json!({}), None).await;
    let items = listed["items"].as_array().expect("items");
    assert_eq!(items.len(), 1, "{listed}");
    assert_eq!(items[0]["assessment_id"], group, "{listed}");
    assert_eq!(items[0]["stage"], "baseline", "{listed}");
    assert_eq!(
        items[0]["dimensions"].as_array().expect("dimensions").len(),
        9,
        "{listed}"
    );

    let filtered = arch::ok(
        &f.running,
        "architecture.maturity_list",
        json!({ "assessment_id": group, "stage": "other" }),
        None,
    )
    .await;
    assert!(
        filtered["items"].as_array().expect("items").is_empty(),
        "the stage filter did not apply: {filtered}"
    );
}

#[tokio::test]
async fn skills_list_returns_one_entry_per_role() {
    let f = arch::fixture().await;
    let group = "018f0000-0000-7000-8000-000000000385";
    let all: Vec<_> = SKILLS
        .iter()
        .map(|key| json!({ "key": key, "score": 4 }))
        .collect();
    for role in ["solution_architect", "delivery_lead"] {
        arch::created(
            &f.running,
            "architecture.skills_assess",
            json!({
                "assessment_id": group,
                "role": role,
                "job_key": "architect",
                "scores": all,
            }),
            None,
        )
        .await;
    }

    let listed = arch::ok(&f.running, "architecture.skills_list", json!({}), None).await;
    let items = listed["items"].as_array().expect("items");
    assert_eq!(items.len(), 2, "{listed}");
    for item in items {
        assert_eq!(item["assessment_id"], group, "{listed}");
        assert_eq!(
            item["categories"].as_array().expect("categories").len(),
            7,
            "{listed}"
        );
    }

    let one = arch::ok(
        &f.running,
        "architecture.skills_list",
        json!({ "role": "delivery_lead" }),
        None,
    )
    .await;
    let items = one["items"].as_array().expect("items");
    assert_eq!(items.len(), 1, "{one}");
    assert_eq!(items[0]["role"], "delivery_lead", "{one}");
}

#[tokio::test]
async fn board_list_returns_the_opened_body() {
    let f = arch::fixture().await;
    let opened = arch::open_board(&f.running).await;
    let id = arch::id_of(&opened);

    let listed = arch::ok(
        &f.running,
        "architecture.board_list",
        json!({ "kind": "architecture_board" }),
        None,
    )
    .await;
    let items = listed["items"].as_array().expect("items");
    assert_eq!(items.len(), 1, "{listed}");
    assert_eq!(items[0]["id"], id, "{listed}");
    assert_eq!(items[0]["key"], "architecture-board", "{listed}");
}
