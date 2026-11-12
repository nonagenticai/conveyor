//! Golden parity for the eval-script transform (`S17` steps 1–2).
//!
//! `fixtures/benchmark/golden-eval-scripts.json` was produced **once**, in development, by the
//! pinned harness itself: `swebench==5.0.2`'s `make_test_spec(instance).eval_script` for the 50
//! verified50 instances, read from the pinned parquet
//! (`SWE-bench/SWE-bench_Verified` @ `78f471bf655a3137b2e8a75af1501690ec009ec3`). The command is
//! recorded in `docs/evidence/S17-CORE.md`; no Python is part of the build or the image, and
//! this fixture is the durable record of what the harness produced.
//!
//! The assertion is byte equality for all 50. A mismatch is a failed port, not a tolerance.

use std::collections::BTreeSet;

use conveyor_domain::benchmark::parsers::parser_for;
use conveyor_domain::benchmark::specgen::eval_script;
use serde_json::Value;

fn golden() -> Vec<Value> {
    let raw = include_str!("fixtures/benchmark/golden-eval-scripts.json");
    serde_json::from_str(raw).expect("golden fixture parses")
}

#[test]
fn every_golden_script_is_reproduced_byte_for_byte() {
    let rows = golden();
    assert_eq!(rows.len(), 50, "the verified50 suite has 50 instances");

    let mut ids = BTreeSet::new();
    for row in &rows {
        let iid = row["instance_id"].as_str().expect("instance_id");
        assert!(ids.insert(iid.to_owned()), "duplicate instance id {iid}");

        let dataset_script = row["dataset_eval_script"]
            .as_str()
            .expect("dataset eval_script");
        let want = row["eval_script"].as_str().expect("harness eval_script");
        let got = eval_script(dataset_script);
        assert_eq!(got, want, "{iid}: eval script differs from swebench 5.0.2");
    }
    assert_eq!(ids.len(), 50);
}

#[test]
fn every_golden_instance_names_a_parser_we_can_run() {
    for row in golden() {
        let iid = row["instance_id"].as_str().expect("instance_id");
        let key = row["log_parser"].as_str().expect("log_parser");
        assert!(
            parser_for(key).is_some(),
            "{iid}: unknown parser key {key:?}"
        );
    }
}

#[test]
fn the_transform_is_deterministic_across_runs() {
    for row in golden() {
        let raw = row["dataset_eval_script"].as_str().expect("dataset script");
        assert_eq!(eval_script(raw), eval_script(raw));
    }
}
