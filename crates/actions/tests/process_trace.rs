//! S13 step 9 — the process trace export and its diff (`admin trace-diff --process-run`).
//!
//! Each test drives a real two-step process run through the executor, exports the stored
//! `run_step` rows into JSONL, and compares the way the admin command does: identical → 0, one
//! changed effect → 1 naming its position and attempt, a corrupt file → refused (exit 2 at the
//! CLI, which maps the refusal to `Exit(2)`).

mod support;

use std::path::PathBuf;

use conveyor_actions::process::trace::{
    ProcessTraceDelta, ProcessTraceError, ProcessTraceRecord, diff_text, export_tx, parse_jsonl,
    to_jsonl,
};
use conveyor_store::begin_scoped;
use serde_json::{Value, json};
use uuid::Uuid;

const TOKEN: &str = "dev:alice";

async fn fixture() -> (support::Pg, support::Running, support::Seeded) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");
    let running = support::Running::start(&pg).await;
    (pg, running, seeded)
}

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

fn text(value: &Value, field: &str) -> String {
    value[field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} is not a string in {value}"))
        .to_owned()
}

/// A project with its template board, and that board's `active` lane.
async fn board_and_lane(running: &support::Running) -> (String, String) {
    let (status, created) = running
        .invoke(
            "projects.create",
            TOKEN,
            json!({"title": format!("Trace test {}", Uuid::now_v7().simple()), "project_type_code": "delivery"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    let project_id = text(output(&created), "id");

    let (status, view) = running
        .invoke(
            "projects.get",
            TOKEN,
            json!({"id": project_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{view}");
    let board_id = text(&output(&view)["boards"][0], "id");

    let (status, board) = running
        .invoke(
            "boards.get",
            TOKEN,
            json!({"id": board_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{board}");
    let lane = output(&board)["lists"]
        .as_array()
        .expect("lists")
        .iter()
        .find(|row| row["list"]["kind"] == "active")
        .unwrap_or_else(|| panic!("no active lane in {board}"));
    (board_id, text(&lane["list"], "id"))
}

async fn create_card(running: &support::Running, board: &str, lane: &str) -> String {
    let (status, created) = running
        .invoke(
            "cards.create",
            TOKEN,
            json!({
                "board_id": board,
                "list_id": lane,
                "name": format!("Card {}", Uuid::now_v7().simple()),
                "card_type_code": "feature",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    text(output(&created), "id")
}

async fn create_process(running: &support::Running, key: &str, sop: &str, steps: Value) -> String {
    // S13: every step names the published SOP it is judged against.
    let mut steps = steps;
    if let Some(items) = steps.as_array_mut() {
        for step in items {
            step["procedure_id"] = json!(sop);
        }
    }
    let (status, body) = running
        .invoke(
            "processes.create",
            TOKEN,
            json!({"key": key, "title": format!("Process {key}"), "steps": steps}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    text(output(&body), "id")
}

async fn publish(running: &support::Running, id: &str) {
    let (status, body) = running
        .invoke(
            "processes.publish",
            TOKEN,
            json!({"process_id": id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
}

async fn start(running: &support::Running, card: &str, key: &str, inputs: Value) -> String {
    let (status, body) = running
        .invoke(
            "process_runs.start",
            TOKEN,
            json!({"card_id": card, "process_key": key, "inputs": inputs}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    text(output(&body), "id")
}

/// Run one step attempt exactly as the worker's `process.step` arm does.
async fn execute(
    pg: &support::Pg,
    seeded: &support::Seeded,
    run: &str,
    position: i32,
    attempt: i32,
) {
    conveyor_actions::process::step::execute_step(
        &pg.runtime,
        seeded.tenant,
        "conveyor",
        Uuid::parse_str(run).expect("run id"),
        position,
        attempt,
    )
    .await
    .unwrap_or_else(|failure| panic!("step {position}/{attempt} failed: {}", failure.message));
}

/// One `run_step` row, flipped so its output hash can no longer match the recording.
async fn flip_output(pg: &support::Pg, seeded: &support::Seeded, run: &str, position: i32) {
    sqlx::query(
        "UPDATE run_step SET output = $3
          WHERE tenant_id = $1 AND run_id = $2 AND position = $4 AND attempt = 1",
    )
    .bind(seeded.tenant_id)
    .bind(Uuid::parse_str(run).expect("run id"))
    .bind(json!({"flipped": true}))
    .bind(position)
    .execute(&pg.super_pool)
    .await
    .expect("flip the stored output");
}

/// Export the run from a tenant-scoped read, as the admin command must.
async fn export(pg: &support::Pg, seeded: &support::Seeded, run: &str) -> (Vec<Value>, String) {
    let run_id = Uuid::parse_str(run).expect("run id");
    let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("a tenant-scoped read");
    let records = export_tx(&mut tx, seeded.tenant, run_id)
        .await
        .expect("export the trace");
    tx.commit().await.expect("commit the read");
    let jsonl = to_jsonl(&records).expect("serialise the trace");
    (records, jsonl)
}

fn temp_path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "conveyor-process-trace-{label}-{}.jsonl",
        Uuid::now_v7().simple()
    ))
}

/// The exit code `admin trace-diff --process-run` returns: 0 identical, 1 a delta, 2 when either
/// side cannot be read or compared.
fn exit_code(expect: &str, new: &str) -> i32 {
    match diff_text(expect, new) {
        Ok(report) => report.exit_code(),
        Err(_) => 2,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_exported_process_trace_round_trips_and_a_changed_output_is_a_positioned_delta() {
    let (pg, running, seeded) = fixture().await;
    let sop = support::publish_sop(&running, "process-trace-sop").await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let process = create_process(
        &running,
        "dev-trace",
        &sop,
        json!([
            {"action": "cards.comment", "input_template": {"id": "{{card.id}}", "body": "one"}},
            {"action": "cards.comment", "input_template": {"id": "{{card.id}}", "body": "two"}}
        ]),
    )
    .await;
    publish(&running, &process).await;
    let run = start(&running, &card, "dev-trace", json!({})).await;
    execute(&pg, &seeded, &run, 1, 1).await;
    execute(&pg, &seeded, &run, 2, 1).await;

    let (records, recorded) = export(&pg, &seeded, &run).await;

    // ── the record shape: the header, then one record per step ──────────────────────────────
    assert_eq!(records.len(), 3, "the header and two steps: {records:?}");
    assert_eq!(records[0]["kind"], json!("run"));
    assert_eq!(records[0]["run_id"], json!(run));
    assert_eq!(records[0]["process_id"], json!(process));
    assert_eq!(records[0]["process_version"], json!(1));
    for position in 1..=2 {
        let step = &records[position as usize];
        assert_eq!(step["kind"], json!("step"));
        assert_eq!(step["position"], json!(position));
        assert_eq!(step["attempt"], json!(1));
        assert_eq!(step["action"], json!("cards.comment"));
        assert_eq!(step["status"], json!("succeeded"));
        assert_eq!(step["judge"]["decision"], json!("accept"), "{step}");
        assert!(step["process_step_id"].is_string(), "{step}");
        assert!(text(step, "input_sha256").starts_with("sha256:"), "{step}");
        assert!(text(step, "output_sha256").starts_with("sha256:"), "{step}");
        assert_eq!(step["error"], json!(null));
        // Carried for the reader; never compared (a timestamp is volatile by definition).
        assert!(
            step["started_at"].is_string(),
            "the timing is carried: {step}"
        );
        assert!(
            step["finished_at"].is_string(),
            "the timing is carried: {step}"
        );
    }

    // ── export → write → re-export → identical ──────────────────────────────────────────────
    let path = temp_path("recorded");
    std::fs::write(&path, &recorded).expect("write the recording");
    let reread = std::fs::read_to_string(&path).expect("read the recording");
    let parsed = parse_jsonl(&reread).expect("the recording parses");
    assert_eq!(parsed.len(), 3);
    assert!(
        matches!(&parsed[0], ProcessTraceRecord::Run(_)),
        "the file starts with the header"
    );
    let (_, fresh) = export(&pg, &seeded, &run).await;
    assert_eq!(exit_code(&reread, &fresh), 0, "a re-export is identical");

    // ── one flipped output is one delta, naming the position and the field ──────────────────
    flip_output(&pg, &seeded, &run, 2).await;
    let (_, changed) = export(&pg, &seeded, &run).await;
    let after = diff_text(&reread, &changed).expect("both traces are complete");
    assert_eq!(after.exit_code(), 1);
    assert_eq!(
        after.deltas.len(),
        1,
        "only the flipped step: {:?}",
        after.deltas
    );
    match &after.deltas[0] {
        ProcessTraceDelta::Mismatch {
            position,
            attempt,
            field,
            ..
        } => assert_eq!(
            (*position, *attempt, *field),
            (2, 1, "output_sha256"),
            "the delta names the step and the field"
        ),
        other => panic!("unexpected delta: {other:?}"),
    }

    // ── a corrupt file is refused (exit 2), never diffed as if it were a trace ──────────────
    let corrupt_path = temp_path("corrupt");
    std::fs::write(&corrupt_path, "not a trace\n").expect("write the corrupt file");
    let corrupt = std::fs::read_to_string(&corrupt_path).expect("read the corrupt file");
    match diff_text(&corrupt, &fresh) {
        Err(ProcessTraceError::Parse { line: 1, .. }) => {}
        other => panic!("a corrupt file is a parse refusal, got {other:?}"),
    }
    assert_eq!(exit_code(&corrupt, &fresh), 2);

    // A trace that starts with a step has no header, so it cannot name a process version.
    let headerless = fresh.lines().skip(1).collect::<Vec<_>>().join("\n");
    match diff_text(&headerless, &fresh) {
        Err(ProcessTraceError::Parse { line: 1, .. }) => {}
        other => panic!("a header-less trace is a parse refusal, got {other:?}"),
    }
    assert_eq!(exit_code(&headerless, &fresh), 2);

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&corrupt_path);
}
