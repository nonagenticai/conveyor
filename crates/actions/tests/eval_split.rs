//! `evals.draw_split` — the stratified draw and its one-shot lock (S19 step 6).
//!
//! The determinism assertion is made against the domain function itself: the split the action
//! locked must be exactly `draw_split(cases, ratio, seed)` over the case ids the API returned.
//! That proves the seed reached the draw and that no entropy was added — a test that only
//! called the action twice could not, because the second call is refused by the lock.

mod support;

use conveyor_domain::eval::{SplitCase, draw_split};
use serde_json::{Value, json};
use uuid::Uuid;

const TOKEN: &str = "dev:alice";

async fn create_eval(running: &support::Running, name: &str) -> String {
    let (status, body) = running
        .invoke(
            "evals.create",
            TOKEN,
            json!({
                "name": name,
                "subject_kind": "completions",
                "entry_action": "completions.create",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    body["output"]["id"].as_str().expect("eval id").to_owned()
}

async fn create_version(running: &support::Running, eval_id: &str) -> String {
    let (status, body) = running
        .invoke(
            "eval_versions.create",
            TOKEN,
            json!({ "eval_id": eval_id }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    body["output"]["id"]
        .as_str()
        .expect("version id")
        .to_owned()
}

async fn add_case(running: &support::Running, version_id: &str, input: &str, tags: Value) {
    let (status, body) = running
        .invoke(
            "eval_cases.create",
            TOKEN,
            json!({ "version_id": version_id, "input": input, "tags": tags }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
}

/// Every case of a version as the domain's split input.
async fn split_cases(running: &support::Running, version_id: &str) -> Vec<(Uuid, Vec<String>)> {
    let (status, body) = running
        .invoke(
            "eval_cases.list",
            TOKEN,
            json!({ "version_id": version_id, "limit": 200 }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    body["output"]["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|item| {
            (
                Uuid::parse_str(item["id"].as_str().expect("case id")).expect("uuid"),
                item["tags"]
                    .as_array()
                    .expect("tags")
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect(),
            )
        })
        .collect()
}

#[tokio::test]
async fn a_draw_locks_once_reports_per_tag_counts_and_matches_the_domain_function() {
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    let eval_id = create_eval(&running, "split").await;
    let version_id = create_version(&running, &eval_id).await;
    for n in 0..3 {
        add_case(
            &running,
            &version_id,
            &format!("alpha {n}"),
            json!(["alpha"]),
        )
        .await;
    }
    for n in 0..2 {
        add_case(
            &running,
            &version_id,
            &format!("beta {n}"),
            json!(["beta", "extra"]),
        )
        .await;
    }

    let (status, body) = running
        .invoke(
            "evals.draw_split",
            TOKEN,
            json!({ "eval_id": eval_id, "train_ratio": 0.5, "seed": "seed-one" }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let lock = body["output"]["version"]["split_sha256"]
        .as_str()
        .expect("the lock names its hash")
        .to_owned();
    assert_eq!(lock.len(), 64, "{lock}");
    assert!(lock.chars().all(|c| c.is_ascii_hexdigit()), "{lock}");
    assert!(
        body["output"]["version"]["split_locked_at"].is_string(),
        "{body}"
    );
    assert_eq!(body["output"]["warnings"], json!([]));

    let per_tag = body["output"]["per_tag"].as_array().expect("per_tag");
    assert_eq!(
        per_tag,
        &vec![
            json!({ "tag": "alpha", "total": 3, "train": 2, "test": 1 }),
            json!({ "tag": "beta", "total": 2, "train": 1, "test": 1 }),
        ],
        "each stratum keeps both sides: {body}"
    );

    // The action locked exactly what the domain function draws over the same ids and seed.
    let cases = split_cases(&running, &version_id).await;
    assert_eq!(cases.len(), 5);
    let expected = {
        let rows: Vec<SplitCase<'_>> = cases
            .iter()
            .map(|(id, tags)| SplitCase {
                id: *id,
                tags: tags.as_slice(),
            })
            .collect();
        draw_split(&rows, 0.5, b"seed-one").expect("the same draw")
    };
    assert_eq!(
        lock, expected.split_sha256,
        "the seed must reach the draw, and nothing else may"
    );
    assert_eq!(per_tag.len(), expected.per_tag.len());
    for (reported, computed) in per_tag.iter().zip(expected.per_tag.iter()) {
        assert_eq!(reported["tag"].as_str(), Some(computed.tag.as_str()));
        assert_eq!(reported["total"].as_i64(), Some(computed.total as i64));
        assert_eq!(reported["train"].as_i64(), Some(computed.train as i64));
        assert_eq!(reported["test"].as_i64(), Some(computed.test as i64));
    }

    // …and the seed genuinely moves the assignment: the same ids over several seeds do not all
    // hash the same. Without this, the equality above could hold for an ignored seed.
    let rows: Vec<SplitCase<'_>> = cases
        .iter()
        .map(|(id, tags)| SplitCase {
            id: *id,
            tags: tags.as_slice(),
        })
        .collect();
    let mut distinct = std::collections::BTreeSet::new();
    for seed in [
        b"seed-one".as_slice(),
        b"seed-two".as_slice(),
        b"seed-three".as_slice(),
        b"seed-four".as_slice(),
        b"seed-five".as_slice(),
        b"seed-six".as_slice(),
    ] {
        distinct.insert(draw_split(&rows, 0.5, seed).expect("draw").split_sha256);
    }
    assert!(
        distinct.len() > 1,
        "the seed must reach the whole assignment: {distinct:?}"
    );

    // Every case now carries a side, and the split is locked.
    let (status, body) = running
        .invoke(
            "eval_cases.list",
            TOKEN,
            json!({ "version_id": version_id }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    for item in body["output"]["items"].as_array().expect("items") {
        assert!(
            item["split"] == "train" || item["split"] == "test",
            "a locked split leaves no case unsplit: {item}"
        );
    }

    // One-shot: the second draw is refused by name, and the version it names is the same one.
    let (status, body) = running
        .invoke(
            "evals.draw_split",
            TOKEN,
            json!({ "eval_id": eval_id, "train_ratio": 0.5 }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "split_locked");
    assert!(!body["detail"].as_str().unwrap_or_default().is_empty());

    running.shutdown().await;
}

#[tokio::test]
async fn a_draw_needs_a_version_two_cases_and_a_ratio_inside_the_unit_interval() {
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    // No version yet: readiness cannot be computed, so the draw says 409 eval_not_ready.
    let bare = create_eval(&running, "no version").await;
    let (status, body) = running
        .invoke(
            "evals.draw_split",
            TOKEN,
            json!({ "eval_id": bare, "train_ratio": 0.5 }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "eval_not_ready");

    // A version with one case is a 422 split_too_small; a split needs two sides.
    let version_id = create_version(&running, &bare).await;
    let (status, body) = running
        .invoke(
            "evals.draw_split",
            TOKEN,
            json!({ "eval_id": bare, "train_ratio": 0.5 }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["type"], "split_too_small");

    add_case(&running, &version_id, "only", json!(["solo"])).await;
    let (status, body) = running
        .invoke(
            "evals.draw_split",
            TOKEN,
            json!({ "eval_id": bare, "train_ratio": 0.5 }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["type"], "split_too_small");

    // A ratio outside the open unit interval is refused, and nothing is locked.
    let eval_id = create_eval(&running, "ratios").await;
    let version_id = create_version(&running, &eval_id).await;
    for n in 0..4 {
        add_case(&running, &version_id, &format!("c{n}"), json!(["suite"])).await;
    }
    for ratio in [
        json!(0.0),
        json!(1.0),
        json!(-0.5),
        json!(1.5),
        json!(100.0),
    ] {
        let (status, body) = running
            .invoke(
                "evals.draw_split",
                TOKEN,
                json!({ "eval_id": eval_id, "train_ratio": ratio }),
                None,
                None,
                None,
            )
            .await;
        assert_eq!(status, 422, "ratio {ratio}: {body}");
        let fields: Vec<&str> = body["errors"]
            .as_array()
            .expect("errors[]")
            .iter()
            .filter_map(|error| error["field"].as_str())
            .collect();
        assert!(fields.contains(&"train_ratio"), "ratio {ratio}: {fields:?}");
    }

    // The refusals above wrote nothing: a valid draw still succeeds and locks.
    let (status, body) = running
        .invoke(
            "evals.draw_split",
            TOKEN,
            json!({ "eval_id": eval_id, "train_ratio": 0.75 }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["per_tag"][0]["total"], 4);
    assert_eq!(body["output"]["per_tag"][0]["train"], 3);
    assert_eq!(body["output"]["per_tag"][0]["test"], 1);

    // An eval that does not exist in the tenant is a 404.
    let (status, body) = running
        .invoke(
            "evals.draw_split",
            TOKEN,
            json!({ "eval_id": Uuid::now_v7(), "train_ratio": 0.5 }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "{body}");

    running.shutdown().await;
}
