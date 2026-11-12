//! S06c — the tasks wire contract, without a transport.
//!
//! Two properties that are cheap here and expensive in a protocol test:
//! * exactly ONE action is task-capable (`projects.execute`), so a stray `task = true` cannot
//!   turn an ordinary action's successful call into a task the client did not ask for;
//! * the task seed names the run, advertises the seven-day TTL and the five-second poll
//!   interval, and refuses a body that has no run id.
//!
//! The lifecycle (start → input_required → update → terminal → cancel) is proven over the
//! real `/mcp` transport in `crates/api/tests/mcp_tasks.rs`.

use serde_json::json;

#[test]
fn only_projects_execute_is_task_capable() {
    let capable: Vec<&str> = conveyor_actions::registry::registry()
        .iter()
        .filter(|spec| spec.task_capable)
        .map(|spec| spec.name)
        .collect();
    assert_eq!(
        capable,
        ["projects.execute"],
        "a task-capable action changes the transport contract; exactly one is declared"
    );
}

#[test]
fn a_task_start_names_the_run_and_carries_the_ttl() {
    let body = json!({
        "output": {
            "id": "01a1bf1a-ada3-784d-8e33-82b019008192",
            "created_at": "2026-11-09T06:59:19.074130Z"
        }
    });
    let result = conveyor_actions::mcp::tasks::start_result(&body).expect("a task seed");
    let wire = serde_json::to_value(&result).expect("serialize the task");
    assert_eq!(wire["resultType"], "task", "{wire}");
    assert_eq!(wire["taskId"], "01a1bf1a-ada3-784d-8e33-82b019008192");
    assert_eq!(wire["status"], "working");
    assert_eq!(wire["ttlMs"], 7 * 24 * 60 * 60 * 1000u64);
    assert_eq!(wire["pollIntervalMs"], 5_000);
    assert_eq!(wire["createdAt"], wire["lastUpdatedAt"]);
}

#[test]
fn a_task_start_refuses_a_body_with_no_run() {
    let error = conveyor_actions::mcp::tasks::start_result(&json!({"output": {}}))
        .expect_err("no run id, no task");
    assert!(
        error.message.contains("no run id"),
        "the refusal names the missing value: {}",
        error.message
    );
}
