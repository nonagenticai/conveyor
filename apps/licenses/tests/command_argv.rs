//! The CronJob commands' argv contract, end to end through the real binary.
//!
//! The reference scripts (`licenses/scripts/*.py` @ 4e8a08d) are invoked by their CronJobs with
//! NO arguments, and their contract is "exactly one JSON line on stdout, ALWAYS". Three of the
//! four `main_cli`s inspect `sys.argv` and refuse ANY argument with a usage line on stderr plus
//! `_fail(None, "this job takes no arguments; configure it via <ENV>_* env")` — one JSON line,
//! exit 2. `revoke_expired_tokens.py` never inspects `sys.argv`, so extra arguments are IGNORED
//! and the sweep runs. These tests pin that asymmetry, and that the no-argument path is unchanged.

mod common;

use std::process::Command;

use conveyor_licenses::db;
use serde_json::json;

/// The real binary with a CLEARED environment: a developer's own `EXPIRE_*` / `REAP_*` /
/// `RECONCILE_*` / `REVOKE_*` / `DATABASE_URL` exports must not change what these tests prove.
fn binary() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_conveyor-licenses"));
    command.env_clear();
    command
}

/// `stdout` must be exactly ONE line, and it is returned.
fn one_line(stdout: &[u8]) -> &str {
    let stdout = std::str::from_utf8(stdout).expect("stdout is utf-8");
    let mut lines = stdout.lines();
    let line = lines.next().expect("stdout is empty");
    assert!(lines.next().is_none(), "not exactly one line: {stdout:?}");
    line
}

/// The three refusing commands: usage line on stderr, the reference's `_fail` payload on stdout,
/// exit 2 — and the payload is the whole line, so a stray extra line cannot pass.
fn assert_refused(command: &str, program: &str, env: &str, expected: &str) {
    let output = binary()
        .args([command, "surplus"])
        .output()
        .expect("run the binary");
    assert_eq!(output.status.code(), Some(2), "{command}");
    assert_eq!(one_line(&output.stdout), expected, "{command}");
    assert_eq!(
        std::str::from_utf8(&output.stderr).expect("stderr is utf-8"),
        format!("usage: {program} (configured via {env} env)\n"),
        "{command}"
    );
}

#[test]
fn expire_stale_refuses_an_extra_argument() {
    assert_refused(
        "expire-stale",
        "conveyor-licenses expire-stale",
        "EXPIRE_*",
        "{\"actor\":null,\"error\":\"this job takes no arguments; configure it via EXPIRE_* \
         env\",\"event\":\"expire_stale_licenses\",\"exit_code\":2}",
    );
}

#[test]
fn reap_expired_refuses_an_extra_argument() {
    assert_refused(
        "reap-expired",
        "conveyor-licenses reap-expired",
        "REAP_*",
        "{\"actor\":null,\"error\":\"this job takes no arguments; configure it via REAP_* \
         env\",\"event\":\"reap_expired_licenses\",\"exit_code\":2}",
    );
}

#[test]
fn reconcile_metering_refuses_an_extra_argument() {
    // `reconcile_metering.py::_fail` is the one that carries `license_id` — null here, because
    // the refusal happens before a licence is selected.
    assert_refused(
        "reconcile-metering",
        "conveyor-licenses reconcile-metering",
        "RECONCILE_*",
        "{\"actor\":null,\"error\":\"this job takes no arguments; configure it via RECONCILE_* \
         env\",\"event\":\"metering_reconcile\",\"exit_code\":2,\"license_id\":null}",
    );
}

#[tokio::test]
async fn revoke_expired_tokens_ignores_an_extra_argument_and_runs_the_sweep() {
    // `revoke_expired_tokens.py` never inspects `sys.argv`, so the extra word must be dropped and
    // the real sweep must run: a converged, clean estate is exit 0 and `status: "ok"`.
    let db = common::fresh().await;
    db::migrate(&db.pool).await.expect("migrate");
    let output = binary()
        .args(["revoke-expired-tokens", "surplus"])
        .env("DATABASE_URL", &db.url)
        .env("REVOKE_DRY_RUN", "false")
        .output()
        .expect("run the binary");

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let payload: serde_json::Value = serde_json::from_str(one_line(&output.stdout)).expect("JSON");
    assert_eq!(payload["job"], "revoke_expired_tokens");
    assert_eq!(payload["status"], "ok");
    assert_eq!(payload["converged"], true);
    assert_eq!(payload["acted"], json!(0));
    let stderr = std::str::from_utf8(&output.stderr).expect("stderr is utf-8");
    assert!(
        !stderr.contains("usage:"),
        "it refused instead of running: {stderr:?}"
    );
}

#[test]
fn the_no_argument_path_is_unchanged() {
    // `expire-stale` with no arguments is the CronJob path: the job runs and reports its own
    // config refusal (nothing is exported), NOT the argv refusal.
    let output = binary()
        .arg("expire-stale")
        .output()
        .expect("run the binary");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        one_line(&output.stdout),
        "{\"actor\":\"cronjob:expire-stale-licences\",\"error\":\"DATABASE_URL is required (it \
         lives in the licenses-env Secret)\",\"error_kind\":\"bad_expire_env\",\"event\":\
         \"expire_stale_licenses\",\"exit_code\":2}"
    );
    let stderr = std::str::from_utf8(&output.stderr).expect("stderr is utf-8");
    assert!(!stderr.contains("usage:"), "{stderr:?}");
}

#[test]
fn the_document_flags_still_work() {
    // The pre-clap argv contract must not swallow `--check-lock` (nor `--print-openapi`): the flag
    // reaches `document()`, which prints its verdict on stderr and NEVER writes to stdout. Whether
    // the served contract currently matches its capture is another slice's question (it drifts on
    // this branch, so the exit is 0 or 1, never the argv refusal's 2-with-JSON); what this test
    // pins is that the flag is not intercepted as argv.
    let output = binary()
        .arg("--check-lock")
        .output()
        .expect("run the binary");
    assert!(
        matches!(output.status.code(), Some(0) | Some(1)),
        "not a document verdict: {output:?}"
    );
    assert!(
        output.stdout.is_empty(),
        "the flag was intercepted: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = std::str::from_utf8(&output.stderr).expect("stderr is utf-8");
    assert!(
        stderr.starts_with("lock OK: ") || stderr.starts_with("error: "),
        "not the lock verdict: {stderr:?}"
    );
}
