//! The `runner/v1` schemas are generated from the domain types, never hand-maintained.
//!
//! `docs/contracts/runner/v1/*.schema.json` is the file the media repository vendors with a
//! `sha256` pin (ADR 0007 §Decision 4). Regenerate with:
//!
//! ```text
//! UPDATE_RUNNER_SCHEMA=1 cargo test -p conveyor-domain --test runner_contract
//! ```

use std::path::{Path, PathBuf};

use conveyor_domain::Artefact;
use conveyor_domain::runner::{CONTRACT, Input, Instruction, Limits, Output, RunResult, Status};

/// Where the checked-in schemas live, relative to this crate.
fn schema_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/contracts/runner/v1")
}

fn render_instruction() -> String {
    let schema = schemars::schema_for!(Instruction);
    format!(
        "{}\n",
        serde_json::to_string_pretty(&schema).expect("a schema serialises to JSON")
    )
}

fn render_result() -> String {
    let schema = schemars::schema_for!(RunResult);
    format!(
        "{}\n",
        serde_json::to_string_pretty(&schema).expect("a schema serialises to JSON")
    )
}

fn sha(byte: char) -> String {
    byte.to_string().repeat(64)
}

fn instruction() -> Instruction {
    Instruction {
        schema: CONTRACT.to_owned(),
        job_id: uuid::Uuid::now_v7(),
        tenant: uuid::Uuid::now_v7(),
        kind: "media.render_html".to_owned(),
        attempt: 1,
        correlation_id: uuid::Uuid::now_v7(),
        deadline_unix_ms: 1_763_000_000_000,
        params: serde_json::json!({"html": "<p>hi</p>", "viewport": {"width": 1280}}),
        inputs: vec![Input {
            name: "source".to_owned(),
            document_id: uuid::Uuid::now_v7(),
            sha256: sha('a'),
            content_type: "text/html".to_owned(),
            bytes: 11,
        }],
        outputs: vec![Output {
            name: "page".to_owned(),
            content_type: "image/png".to_owned(),
            max_bytes: 8 * 1024 * 1024,
        }],
        limits: Limits {
            cpu_millis: 500,
            memory_bytes: 512 * 1024 * 1024,
            wall_secs: 60,
            max_output_bytes: 16 * 1024 * 1024,
        },
        conveyor_base_url: "http://conveyor.conveyor.svc.cluster.local:8080".to_owned(),
        trace_id: "trace-1".to_owned(),
    }
}

#[test]
fn checked_in_schemas_match_the_types() {
    let update = std::env::var_os("UPDATE_RUNNER_SCHEMA").is_some();
    let dir = schema_dir();
    if update {
        std::fs::create_dir_all(&dir).expect("create the schema directory");
    }
    let files = [
        ("instruction.schema.json", render_instruction()),
        ("result.schema.json", render_result()),
    ];
    for (name, body) in files {
        let path = dir.join(name);
        if update {
            std::fs::write(&path, body).expect("write the schema");
            continue;
        }
        let checked_in = std::fs::read_to_string(&path).unwrap_or_default();
        assert_eq!(
            checked_in, body,
            "{name} is stale; regenerate with \
             `UPDATE_RUNNER_SCHEMA=1 cargo test -p conveyor-domain --test runner_contract`"
        );
    }
}

#[test]
fn an_instruction_round_trips_byte_identically() {
    let original = instruction();
    let json = serde_json::to_string(&original).expect("serialise");
    let parsed: Instruction = serde_json::from_str(&json).expect("parse");
    assert_eq!(original, parsed);
    assert_eq!(json, serde_json::to_string(&parsed).expect("re-serialise"));
}

#[test]
fn a_result_round_trips_byte_identically() {
    let instruction = instruction();
    let original = RunResult {
        schema: CONTRACT.to_owned(),
        job_id: instruction.job_id,
        attempt: instruction.attempt,
        status: Status::Succeeded,
        error: None,
        artefacts: vec![Artefact {
            name: "page".to_owned(),
            sha256: sha('b'),
            bytes: 1024,
            content_type: "image/png".to_owned(),
        }],
        metrics: serde_json::json!({"frames": 1}),
        started_at: chrono::DateTime::from_timestamp(1_762_999_000, 0).expect("timestamp"),
        finished_at: chrono::DateTime::from_timestamp(1_762_999_030, 0).expect("timestamp"),
    };
    let json = serde_json::to_string(&original).expect("serialise");
    let parsed: RunResult = serde_json::from_str(&json).expect("parse");
    assert_eq!(original, parsed);
    original.validate(&instruction).expect("valid result");
}
