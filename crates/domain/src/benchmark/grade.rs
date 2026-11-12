//! The verdict ladder — a Rust port of `conveyor-bench/grader.py::grade_log` (175-241) and
//! swebench 5.0.2's `get_eval_report` (`harness/grading.py:329-393`).
//!
//! The ladder, in order (CONTRACT §19):
//!
//! 1. `>>>>> Patch Apply Failed` before the eval marker ⇒ `apply_failed`
//! 2. no `>>>>> conveyor-bench eval` marker ⇒ `infra_error` (the eval never started)
//! 3. `>>>>> conveyor-bench eval timeout` ⇒ `eval_timeout`
//! 4. parse the test statuses; every `FAIL_TO_PASS` passed and every `PASS_TO_PASS` still
//!    passes ⇒ `resolved`, else `unresolved`
//! 5. no statuses **and** no end-of-test marker ⇒ `infra_error` (the eval was cut off);
//!    no statuses with the end marker ⇒ `unresolved` (the code emitted none)
//!
//! The infra classifier is advisory: it never drops an instance from the denominator, it only
//! marks an environment fault so it is not published as a model failure.

use std::collections::BTreeMap;

use regex::Regex;
use thiserror::Error;

use super::parsers::{self, ParserFn, TestStatus};

// ── the harness's marker and status constants (swebench 5.0.2 + conveyor-bench) ────────────
pub const APPLY_PATCH_FAIL: &str = ">>>>> Patch Apply Failed";
/// swebench's success marker, printed by the sandbox runner before the eval.
pub const APPLY_PATCH_PASS: &str = ">>>>> Applied Patch";
const RESET_FAILED: &str = ">>>>> Reset Failed";
const TESTS_ERROR: &str = ">>>>> Tests Errored";
const TESTS_TIMEOUT: &str = ">>>>> Tests Timed Out";
pub const START_TEST_OUTPUT: &str = ">>>>> Start Test Output";
pub const END_TEST_OUTPUT: &str = ">>>>> End Test Output";
pub const TEST_EXIT_CODE: &str = ">>>>> Test Exit Code";
/// The marker the sandbox runner prints before starting the eval.
pub const EVAL_MARKER: &str = ">>>>> conveyor-bench eval";
/// The marker the sandbox runner prints when the eval hit its timeout.
pub const TIMEOUT_MARKER: &str = ">>>>> conveyor-bench eval timeout";

/// The final state of one graded instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GradeStatus {
    Resolved,
    Unresolved,
    ApplyFailed,
    EvalTimeout,
    InfraError,
}

impl GradeStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            GradeStatus::Resolved => "resolved",
            GradeStatus::Unresolved => "unresolved",
            GradeStatus::ApplyFailed => "apply_failed",
            GradeStatus::EvalTimeout => "eval_timeout",
            GradeStatus::InfraError => "infra_error",
        }
    }
}

/// swebench's two evaluation modes (`constants/__init__.py::EvalType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EvalType {
    PassAndFail,
    FailOnly,
}

/// Repos whose log parser records failures only; everything absent counts as passing.
/// `swebench.harness.constants.FAIL_ONLY_REPOS` — none of the verified50 repos is in this set,
/// but the grader must not silently change semantics if a future suite includes one.
const FAIL_ONLY_REPOS: &[&str] = &[
    "chartjs/Chart.js",
    "processing/p5.js",
    "markedjs/marked",
    "bpmn-io/bpmn-js",
    "openlayers/openlayers",
    "eslint/eslint",
];

fn eval_type(repo: &str) -> EvalType {
    if FAIL_ONLY_REPOS.contains(&repo) {
        EvalType::FailOnly
    } else {
        EvalType::PassAndFail
    }
}

/// One test list's outcome (`tests_status` inner shape).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TestStatuses {
    pub success: Vec<String>,
    pub failure: Vec<String>,
}

/// swebench's `tests_status` report.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TestReport {
    pub fail_to_pass: TestStatuses,
    pub pass_to_pass: TestStatuses,
    pub fail_to_fail: TestStatuses,
    pub pass_to_fail: TestStatuses,
}

/// The result of grading one log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grade {
    pub status: GradeStatus,
    pub resolved: bool,
    /// `None` when swebench's `get_eval_report` attached no `tests_status` (no parseable run).
    pub tests_status: Option<TestReport>,
    pub infra_failure: bool,
    pub infra_failure_reason: Option<String>,
    pub patch_is_none: bool,
    pub patch_exists: bool,
    pub patch_successfully_applied: bool,
    pub detail: String,
}

impl Grade {
    fn bare(status: GradeStatus, detail: impl Into<String>) -> Self {
        Self {
            status,
            resolved: false,
            tests_status: None,
            infra_failure: false,
            infra_failure_reason: None,
            patch_is_none: false,
            patch_exists: true,
            patch_successfully_applied: false,
            detail: detail.into(),
        }
    }
}

/// The instance fields grading needs.
#[derive(Debug, Clone, Copy)]
pub struct InstanceSpec<'a> {
    pub instance_id: &'a str,
    pub repo: &'a str,
    pub fail_to_pass: &'a [String],
    pub pass_to_pass: &'a [String],
    /// The dataset's `log_parser` key.
    pub parser: &'a str,
}

/// Why a log could not be graded at all (a suite configuration fault, never a model verdict).
#[derive(Debug, Error, PartialEq, Eq)]
pub enum GradeError {
    #[error("unknown log parser {key:?}")]
    UnknownParser { key: String },
}

/// Grade one eval log. `model_patch` is `None` only when the prediction carried a JSON null.
pub fn grade_log(
    instance: &InstanceSpec<'_>,
    model_patch: Option<&str>,
    log: &str,
) -> Result<Grade, GradeError> {
    let parser = parsers::parser_for(instance.parser).ok_or_else(|| GradeError::UnknownParser {
        key: instance.parser.to_owned(),
    })?;

    // 1. Apply failure, only when it precedes the eval marker (grader.py:178-179).
    let before_eval = log.split_once(EVAL_MARKER).map_or(log, |(head, _)| head);
    if before_eval.contains(APPLY_PATCH_FAIL) {
        let mut grade = Grade::bare(GradeStatus::ApplyFailed, "no apply command succeeded");
        grade.patch_exists = model_patch.is_some();
        grade.patch_is_none = model_patch.is_none();
        return Ok(grade);
    }

    // 2. The eval never started (grader.py:180-183).
    let Some((_, eval_output)) = log.split_once(EVAL_MARKER) else {
        return Ok(Grade::bare(
            GradeStatus::InfraError,
            "eval never started (log has no eval marker)",
        ));
    };

    // 3. Timeout (grader.py:185-186).
    if eval_output.contains(TIMEOUT_MARKER) {
        return Ok(Grade::bare(
            GradeStatus::EvalTimeout,
            "eval exceeded its timeout",
        ));
    }

    // 4-5. swebench's report, then the grader's guard around it.
    let mut grade = report(instance, model_patch, eval_output, parser);

    if !grade.resolved && grade.infra_failure {
        grade.status = GradeStatus::InfraError;
        return Ok(grade);
    }
    if !grade.resolved && grade.tests_status.is_none() {
        // No `tests_status` means the parser found no statuses at all. The marker discriminates
        // "cut off" from "ran and emitted nothing" (grader.py:208-240).
        if !eval_output.contains(END_TEST_OUTPUT) {
            grade.status = GradeStatus::InfraError;
            grade.detail =
                "eval was cut off before the end-of-test marker; no statuses parsed".to_owned();
        } else {
            grade.status = GradeStatus::Unresolved;
            grade.detail = "eval completed but emitted no parseable test statuses".to_owned();
        }
        return Ok(grade);
    }

    grade.status = if grade.resolved {
        GradeStatus::Resolved
    } else {
        GradeStatus::Unresolved
    };
    Ok(grade)
}

/// The port of `get_eval_report` (grading.py:329-393).
fn report(
    instance: &InstanceSpec<'_>,
    model_patch: Option<&str>,
    content: &str,
    parser: ParserFn,
) -> Grade {
    let mut grade = Grade::bare(GradeStatus::Unresolved, "");
    grade.patch_exists = model_patch.is_some();
    if model_patch.is_none() {
        grade.patch_is_none = true;
        return grade;
    }

    let Some(status_map) = logs_eval(content, parser) else {
        // Advisory classification only; `resolved` stays false either way (#586).
        if let Some((reason, tier)) = classify_text(content) {
            grade.infra_failure = tier == Tier::Environment;
            grade.infra_failure_reason = Some(reason.to_owned());
            grade.detail = format!("swebench: {reason}");
        }
        return grade;
    };
    grade.patch_successfully_applied = true;

    let report = tests_report(instance, &status_map);
    grade.resolved = resolution_status(&report);
    grade.tests_status = Some(report);
    grade
}

/// `grading.py:113-176`. Returns the status map only when the run is valid.
fn logs_eval(content: &str, parser: ParserFn) -> Option<BTreeMap<String, TestStatus>> {
    let bad = [APPLY_PATCH_FAIL, RESET_FAILED, TESTS_ERROR, TESTS_TIMEOUT];
    if bad.iter().any(|code| content.contains(code)) {
        return None;
    }
    if !(content.contains(START_TEST_OUTPUT) && content.contains(END_TEST_OUTPUT)) {
        return None;
    }

    let after_start = content
        .split_once(START_TEST_OUTPUT)
        .map(|(_, tail)| tail)?;
    let sliced = after_start
        .split_once(END_TEST_OUTPUT)
        .map_or(after_start, |(head, _)| head);
    let mut status_map = parser(sliced);
    if status_map.is_empty() {
        // Some runners emit results outside the markers.
        status_map = parser(content);
    }
    if status_map.is_empty() && !suite_ran(content) {
        return None;
    }

    // A patch can print its own PASSED lines; cross-check the test command's exit status.
    let exit_code = parse_test_exit_code(content);
    if exit_code.is_some_and(|code| code != 0)
        && !status_map.is_empty()
        && !status_map
            .values()
            .any(|s| matches!(s, TestStatus::Failed | TestStatus::Error))
    {
        return None;
    }

    Some(status_map)
}

fn suite_ran(content: &str) -> bool {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?m)Executed [1-9]\d* of \d+|\
              TOTAL: [1-9]\d* (?:SUCCESS|FAILED)|\
              [1-9]\d* passing|\
              Tests:\s+[1-9]\d*|\
              Test Suites:\s+(?:\d+ \w+, )*[1-9]\d* total|\
              ^# tests [1-9]\d*|\
              [1-9]\d* specs?, \d+ failures?|\
              ': ok$",
        )
        .expect("static regex")
    })
    .is_match(content)
}

fn parse_test_exit_code(content: &str) -> Option<i64> {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re =
        RE.get_or_init(|| Regex::new(r">>>>> Test Exit Code:\s*(-?\d+)").expect("static regex"));
    re.captures(content)
        .and_then(|caps| caps.get(1))
        .and_then(|value| value.as_str().parse().ok())
}

/// swebench's truncation tolerance (`grading.py:61-82`). 676 expected ids in Verified are
/// truncated mid-parameter; only those prefix-match, and only when the candidates agree on
/// pass-vs-fail. Exact ids keep exact-match semantics.
fn resolve_case<'a>(case: &'a str, map: &'a BTreeMap<String, TestStatus>) -> Option<&'a str> {
    if map.contains_key(case) {
        return Some(case);
    }
    if case.matches('[').count() > case.matches(']').count() {
        let matches: Vec<&String> = map.keys().filter(|k| k.starts_with(case)).collect();
        let passing: std::collections::BTreeSet<bool> =
            matches.iter().map(|k| map[*k].is_passing()).collect();
        if !matches.is_empty() && passing.len() == 1 {
            return Some(matches[0].as_str());
        }
    }
    None
}

fn test_passed(case: &str, map: &BTreeMap<String, TestStatus>) -> bool {
    resolve_case(case, map).is_some_and(|key| map[key].is_passing())
}

fn test_maintained(case: &str, map: &BTreeMap<String, TestStatus>) -> bool {
    test_passed(case, map)
        || resolve_case(case, map).is_some_and(|key| map[key] == TestStatus::Skipped)
}

fn test_failed(case: &str, map: &BTreeMap<String, TestStatus>) -> bool {
    resolve_case(case, map).is_none_or(|key| {
        matches!(
            map[key],
            TestStatus::Failed | TestStatus::Error | TestStatus::Skipped
        )
    })
}

/// `grading.py:179-285` with `calculate_to_fail = false` (the four keys are always present).
fn tests_report(instance: &InstanceSpec<'_>, map: &BTreeMap<String, TestStatus>) -> TestReport {
    let fail_only = eval_type(instance.repo) == EvalType::FailOnly;

    let mut fail_to_pass = TestStatuses::default();
    for case in instance.fail_to_pass {
        if fail_only {
            if map.get(case) == Some(&TestStatus::Failed) {
                fail_to_pass.failure.push(case.clone());
            } else {
                fail_to_pass.success.push(case.clone());
            }
        } else if test_passed(case, map) {
            fail_to_pass.success.push(case.clone());
        } else if test_failed(case, map) {
            fail_to_pass.failure.push(case.clone());
        }
    }

    let mut pass_to_pass = TestStatuses::default();
    for case in instance.pass_to_pass {
        if fail_only {
            if map.get(case) == Some(&TestStatus::Failed) {
                pass_to_pass.failure.push(case.clone());
            } else {
                pass_to_pass.success.push(case.clone());
            }
        } else if test_maintained(case, map) {
            pass_to_pass.success.push(case.clone());
        } else if test_failed(case, map) {
            pass_to_pass.failure.push(case.clone());
        }
    }

    TestReport {
        fail_to_pass,
        pass_to_pass,
        fail_to_fail: TestStatuses::default(),
        pass_to_fail: TestStatuses::default(),
    }
}

/// FULL iff every F2P passed and every P2P was maintained; an empty list computes to 1
/// (`grading.py:288-326`).
fn resolution_status(report: &TestReport) -> bool {
    let ratio = |s: &TestStatuses| {
        let total = s.success.len() + s.failure.len();
        if total == 0 {
            1.0
        } else {
            s.success.len() as f64 / total as f64
        }
    };
    ratio(&report.fail_to_pass) == 1.0 && ratio(&report.pass_to_pass) == 1.0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tier {
    Environment,
    Ambiguous,
}

/// swebench's infra classifier (`harness/infra_failure.py`): first matching signature wins.
fn classify_text(text: &str) -> Option<(&'static str, Tier)> {
    const SIGNATURES: &[(&str, Tier, &str)] = &[
        (
            "browser_launch_failed",
            Tier::Environment,
            "Failed to launch|Failed to connect to the bus",
        ),
        (
            "display_unavailable",
            Tier::Environment,
            "cannot open display|Missing X server|unable to open X display",
        ),
        (
            "out_of_memory",
            Tier::Environment,
            "Cannot allocate memory|OutOfMemoryError|^Killed$",
        ),
        (
            "container_unavailable",
            Tier::Environment,
            "Error response from daemon|Cannot connect to the Docker daemon",
        ),
        (
            "network_unreachable",
            Tier::Environment,
            "Could not resolve host|Temporary failure in name resolution",
        ),
        (
            "missing_module",
            Tier::Ambiguous,
            "Cannot find module|MODULE_NOT_FOUND|ModuleNotFoundError",
        ),
        (
            "no_tests_collected",
            Tier::Ambiguous,
            "no tests ran|collected 0 items",
        ),
        (
            "tests_timed_out",
            Tier::Ambiguous,
            r"Timeout error: \d+ seconds exceeded",
        ),
    ];
    if text.is_empty() {
        return None;
    }
    for (reason, tier, pattern) in SIGNATURES {
        let re = Regex::new(&format!("(?m){pattern}")).expect("static regex");
        if re.is_match(text) {
            return Some((reason, *tier));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instance<'a>(f2p: &'a [String], p2p: &'a [String], repo: &'a str) -> InstanceSpec<'a> {
        InstanceSpec {
            instance_id: "x",
            repo,
            fail_to_pass: f2p,
            pass_to_pass: p2p,
            parser: "parse_log_pytest",
        }
    }

    fn names(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    fn log_with(body: &str) -> String {
        format!(">>>>> Applied Patch\n>>>>> conveyor-bench eval\n{body}")
    }

    #[test]
    fn an_apply_failure_before_the_eval_marker_is_apply_failed() {
        let f2p = names(&["a"]);
        let p2p = names(&[]);
        let log = format!("{APPLY_PATCH_FAIL}\n>>>>> conveyor-bench eval\n");
        let g = grade_log(&instance(&f2p, &p2p, "pytest-dev/pytest"), Some("p"), &log).unwrap();
        assert_eq!(g.status, GradeStatus::ApplyFailed);
    }

    #[test]
    fn a_missing_eval_marker_is_infra_error() {
        let f2p = names(&["a"]);
        let p2p = names(&[]);
        // No apply-fail marker and no eval marker: the eval never started.
        let g = grade_log(
            &instance(&f2p, &p2p, "pytest-dev/pytest"),
            Some("p"),
            "the grader pod produced no bench output\n",
        )
        .unwrap();
        assert_eq!(g.status, GradeStatus::InfraError);
    }

    #[test]
    fn the_timeout_marker_is_eval_timeout() {
        let f2p = names(&["a"]);
        let p2p = names(&[]);
        let log = log_with(&format!("{TIMEOUT_MARKER}\n"));
        let g = grade_log(&instance(&f2p, &p2p, "pytest-dev/pytest"), Some("p"), &log).unwrap();
        assert_eq!(g.status, GradeStatus::EvalTimeout);
    }

    #[test]
    fn a_passing_failing_test_and_a_maintained_passing_test_is_resolved() {
        let f2p = names(&["tests/test_x.py::test_a"]);
        let p2p = names(&["tests/test_y.py::test_b"]);
        let log = log_with(
            ">>>>> Start Test Output\n\
             PASSED tests/test_x.py::test_a\n\
             PASSED tests/test_y.py::test_b\n\
             >>>>> End Test Output\n",
        );
        let g = grade_log(&instance(&f2p, &p2p, "pytest-dev/pytest"), Some("p"), &log).unwrap();
        assert_eq!(g.status, GradeStatus::Resolved);
        assert!(g.resolved);
        let report = g.tests_status.expect("tests_status present");
        assert_eq!(report.fail_to_pass.success, f2p);
        assert_eq!(report.pass_to_pass.success, p2p);
        assert!(report.fail_to_fail.success.is_empty());
    }

    #[test]
    fn a_failing_failing_test_is_unresolved() {
        let f2p = names(&["tests/test_x.py::test_a"]);
        let p2p = names(&[]);
        let log = log_with(
            ">>>>> Start Test Output\n\
             FAILED tests/test_x.py::test_a\n\
             >>>>> End Test Output\n",
        );
        let g = grade_log(&instance(&f2p, &p2p, "pytest-dev/pytest"), Some("p"), &log).unwrap();
        assert_eq!(g.status, GradeStatus::Unresolved);
        assert!(!g.resolved);
        assert_eq!(g.tests_status.expect("present").fail_to_pass.failure, f2p);
    }

    #[test]
    fn a_skipped_passing_test_is_a_regression_but_a_skipped_p2p_is_not() {
        let f2p = names(&["tests/test_x.py::test_a"]);
        let p2p = names(&["tests/test_y.py::test_b"]);
        let log = log_with(
            ">>>>> Start Test Output\n\
             SKIPPED tests/test_x.py::test_a\n\
             SKIPPED tests/test_y.py::test_b\n\
             >>>>> End Test Output\n",
        );
        let g = grade_log(&instance(&f2p, &p2p, "pytest-dev/pytest"), Some("p"), &log).unwrap();
        assert_eq!(g.status, GradeStatus::Unresolved);
        let report = g.tests_status.expect("present");
        assert_eq!(report.fail_to_pass.failure, f2p);
        assert_eq!(report.pass_to_pass.success, p2p);
    }

    #[test]
    fn no_statuses_without_the_end_marker_is_infra_error() {
        let f2p = names(&["tests/test_x.py::test_a"]);
        let p2p = names(&[]);
        // Start marker, no statuses, no end marker: the eval was cut off.
        let log = log_with(">>>>> Start Test Output\nnothing to see\n");
        let g = grade_log(&instance(&f2p, &p2p, "pytest-dev/pytest"), Some("p"), &log).unwrap();
        assert_eq!(g.status, GradeStatus::InfraError);
    }

    #[test]
    fn no_statuses_with_the_end_marker_is_unresolved() {
        let f2p = names(&["tests/test_x.py::test_a"]);
        let p2p = names(&[]);
        let log = log_with(
            ">>>>> Start Test Output\n\
             nothing to see\n\
             >>>>> End Test Output\n",
        );
        let g = grade_log(&instance(&f2p, &p2p, "pytest-dev/pytest"), Some("p"), &log).unwrap();
        assert_eq!(g.status, GradeStatus::Unresolved);
    }

    #[test]
    fn a_nonzero_exit_code_with_no_failures_is_not_a_pass() {
        let f2p = names(&["tests/test_x.py::test_a"]);
        let p2p = names(&[]);
        // All tests claim PASSED, but the command exited 1: the log is not describing the run.
        let log = log_with(
            ">>>>> Start Test Output\n\
             PASSED tests/test_x.py::test_a\n\
             >>>>> End Test Output\n\
             >>>>> Test Exit Code: 1\n",
        );
        let g = grade_log(&instance(&f2p, &p2p, "pytest-dev/pytest"), Some("p"), &log).unwrap();
        assert!(
            !g.resolved,
            "exit-code cross-check must refuse the status map"
        );
    }

    #[test]
    fn an_environment_signature_marks_infra_but_an_ambiguous_one_does_not() {
        let f2p = names(&["tests/test_x.py::test_a"]);
        let p2p = names(&[]);
        let env = log_with(">>>>> Start Test Output\nCannot allocate memory\n");
        let g = grade_log(&instance(&f2p, &p2p, "pytest-dev/pytest"), Some("p"), &env).unwrap();
        assert_eq!(g.status, GradeStatus::InfraError);
        assert!(g.infra_failure);
        assert_eq!(g.infra_failure_reason.as_deref(), Some("out_of_memory"));

        let ambiguous = log_with(
            ">>>>> Start Test Output\nModuleNotFoundError: no module named x\n\
             >>>>> End Test Output\n",
        );
        let g = grade_log(
            &instance(&f2p, &p2p, "pytest-dev/pytest"),
            Some("p"),
            &ambiguous,
        )
        .unwrap();
        assert_eq!(g.status, GradeStatus::Unresolved);
        assert!(!g.infra_failure);
        assert_eq!(g.infra_failure_reason.as_deref(), Some("missing_module"));
    }

    #[test]
    fn a_truncated_parametrized_id_prefix_matches_when_the_candidates_agree() {
        // Verified carries 676 ids truncated mid-parameter (swebench #290).
        let f2p = names(&["tests/test_x.py::test_a[param"]);
        let p2p = names(&[]);
        let log = log_with(
            ">>>>> Start Test Output\n\
             PASSED tests/test_x.py::test_a[param-1]\n\
             XFAIL tests/test_x.py::test_a[param-2]\n\
             >>>>> End Test Output\n",
        );
        let g = grade_log(&instance(&f2p, &p2p, "pytest-dev/pytest"), Some("p"), &log).unwrap();
        assert_eq!(g.status, GradeStatus::Resolved);
    }
}
