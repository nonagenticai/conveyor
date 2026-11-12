//! The slice's central proof: the banked eval logs reproduce the banked verdicts **row for row**
//! (S17 build step 7, CONTRACT §19 "parser parity is a test").
//!
//! The fixtures are a deliberate freeze of `conveyor-bench/results/2026-09-17-full/artefacts/`:
//! every run directory that carries both a `results.jsonl` and its `eval-logs.tgz` (17 runs,
//! **557 graded cases**). The old repository is not a test dependency; these files are.
//!
//! What is compared, per row: the verdict (`status`), `resolved`, the four `tests_status` lists,
//! and swebench's `infra_failure` flag. The instance data (answers, parser key) comes from the
//! committed seed artifact, so the whole produce → grade path is exercised against the same
//! inputs the 2026-09-17 measurement used.
//!
//! ⚠️ Two runs are **known non-controls**: `control-gold-09161231d54b` and
//! `control-gold-091612332af6` graded 0/50. They are historical pre-fix runs and are
//! *reproduced*, never asserted as controls — only `091615504508` and `091709203869` are the
//! 50/50 gold controls, and both empty runs are the 0/50 negative controls.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use conveyor_domain::benchmark::arm::Arm;
use conveyor_domain::benchmark::grade::{InstanceSpec, grade_log};
use conveyor_domain::benchmark::mcnemar::{CompareError, compare};
use flate2::read::GzDecoder;
use serde::Deserialize;
use tar::Archive;

const RUNS_WITH_LOGS: usize = 17;
const GRADED_CASES: usize = 555;
/// Two rows are harness-level k8s API timeouts (`report` absent, no log): the sandbox driver's
/// `infra_error`, not a grader verdict.
const UNGRADED_ROWS: usize = 2;
const TOTAL_ROWS: usize = GRADED_CASES + UNGRADED_ROWS;

#[derive(Debug, Deserialize)]
struct Seed {
    instances: Vec<SeedInstance>,
}

#[derive(Debug, Deserialize)]
struct SeedInstance {
    instance_id: String,
    repo: String,
    fail_to_pass: Vec<String>,
    pass_to_pass: Vec<String>,
    parser: String,
}

#[derive(Debug, Deserialize)]
struct ResultRow {
    instance_id: String,
    status: String,
    resolved: bool,
    #[serde(default)]
    report: Option<Report>,
}

#[derive(Debug, Deserialize)]
struct Report {
    #[serde(default)]
    tests_status: Option<TestsStatus>,
    #[serde(default)]
    infra_failure: bool,
}

#[derive(Debug, Deserialize)]
struct TestsStatus {
    #[serde(rename = "FAIL_TO_PASS")]
    fail_to_pass: TestLists,
    #[serde(rename = "PASS_TO_PASS")]
    pass_to_pass: TestLists,
    #[serde(rename = "FAIL_TO_FAIL")]
    fail_to_fail: TestLists,
    #[serde(rename = "PASS_TO_FAIL")]
    pass_to_fail: TestLists,
}

#[derive(Debug, Deserialize)]
struct TestLists {
    success: Vec<String>,
    failure: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct PredictionRow {
    instance_id: String,
    model_patch: Option<String>,
    model_name_or_path: String,
}

fn seed() -> Seed {
    serde_json::from_str(include_str!("../../../seed/benchmark/verified50-v1.json"))
        .expect("the committed seed artifact parses")
}

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/benchmark/2026-09-17")
}

fn runs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(fixtures_dir())
        .expect("fixtures directory exists")
        .map(|entry| entry.expect("dir entry").path())
        .filter(|path| path.is_dir())
        .collect();
    dirs.sort();
    dirs
}

fn jsonl<T: for<'de> Deserialize<'de>>(path: &Path) -> Vec<T> {
    fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap_or_else(|e| panic!("parse {line:?}: {e}")))
        .collect()
}

/// `logs/<instance_id>.log` from the run's `eval-logs.tgz`.
fn logs(path: &Path) -> BTreeMap<String, String> {
    let file = fs::File::open(path).unwrap_or_else(|e| panic!("open {}: {e}", path.display()));
    let mut archive = Archive::new(GzDecoder::new(file));
    let mut out = BTreeMap::new();
    for entry in archive.entries().expect("tar entries") {
        let mut entry = entry.expect("tar entry");
        let entry_path = entry.path().expect("entry path").to_path_buf();
        let Some(name) = entry_path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        // macOS AppleDouble resource forks, not logs.
        if name.starts_with("._") {
            continue;
        }
        let Some(instance_id) = name.strip_suffix(".log") else {
            continue;
        };
        let mut text = String::new();
        entry.read_to_string(&mut text).expect("read log");
        out.insert(instance_id.to_owned(), text);
    }
    out
}

fn verdicts(run: &Path) -> BTreeMap<String, bool> {
    jsonl::<ResultRow>(&run.join("results.jsonl"))
        .into_iter()
        .map(|row| (row.instance_id, row.resolved))
        .collect()
}

fn run_label(run: &Path) -> String {
    let preds = jsonl::<PredictionRow>(&run.join("predictions.jsonl"));
    let labels: std::collections::BTreeSet<&str> = preds
        .iter()
        .map(|p| p.model_name_or_path.as_str())
        .collect();
    assert_eq!(
        labels.len(),
        1,
        "{}: one predictions file must carry one label",
        run.display()
    );
    labels.into_iter().next().expect("one label").to_owned()
}

#[test]
fn every_banked_log_reproduces_its_banked_verdict() {
    let seed = seed();
    let instances: BTreeMap<&str, &SeedInstance> = seed
        .instances
        .iter()
        .map(|i| (i.instance_id.as_str(), i))
        .collect();

    let runs = runs();
    assert_eq!(runs.len(), RUNS_WITH_LOGS, "17 run directories");

    let mut cases = 0usize;
    let mut ungraded = 0usize;
    let mut per_run: BTreeMap<String, (usize, usize)> = BTreeMap::new();

    for run in &runs {
        let name = run
            .file_name()
            .expect("run name")
            .to_string_lossy()
            .to_string();
        let results = jsonl::<ResultRow>(&run.join("results.jsonl"));
        let predictions: BTreeMap<String, PredictionRow> =
            jsonl::<PredictionRow>(&run.join("predictions.jsonl"))
                .into_iter()
                .map(|p| (p.instance_id.clone(), p))
                .collect();
        let logs = logs(&run.join("eval-logs.tgz"));

        let mut resolved = 0usize;
        let mut graded_rows = 0usize;
        for row in &results {
            let Some(report) = &row.report else {
                // The harness never reached the grader (e.g. a k8s API timeout): no log, no
                // report. That is the sandbox driver's `infra_error`, so it is out of the
                // grader's parity set by construction.
                assert_eq!(
                    row.status, "infra_error",
                    "{name}: {} has no report but is not infra_error",
                    row.instance_id
                );
                if let Some(log) = logs.get(&row.instance_id) {
                    assert!(
                        log.is_empty(),
                        "{name}: {} has no report but a non-empty log",
                        row.instance_id
                    );
                }
                ungraded += 1;
                continue;
            };
            let instance = instances
                .get(row.instance_id.as_str())
                .unwrap_or_else(|| panic!("{name}: {} is not in the seed", row.instance_id));
            let prediction = predictions
                .get(&row.instance_id)
                .unwrap_or_else(|| panic!("{name}: {} has no prediction", row.instance_id));
            let log = logs
                .get(&row.instance_id)
                .unwrap_or_else(|| panic!("{name}: {} has no eval log", row.instance_id));

            let spec = InstanceSpec {
                instance_id: &instance.instance_id,
                repo: &instance.repo,
                fail_to_pass: &instance.fail_to_pass,
                pass_to_pass: &instance.pass_to_pass,
                parser: &instance.parser,
            };
            let grade = grade_log(&spec, prediction.model_patch.as_deref(), log)
                .unwrap_or_else(|e| panic!("{name}: {}: {e}", row.instance_id));

            assert_eq!(
                grade.status.as_str(),
                row.status,
                "{name}: {} verdict",
                row.instance_id
            );
            assert_eq!(
                grade.resolved, row.resolved,
                "{name}: {} resolved",
                row.instance_id
            );
            assert_eq!(
                grade.infra_failure, report.infra_failure,
                "{name}: {} infra_failure",
                row.instance_id
            );
            match (&grade.tests_status, &report.tests_status) {
                (Some(got), Some(want)) => {
                    assert_eq!(
                        got.fail_to_pass.success, want.fail_to_pass.success,
                        "{name}: {} F2P success",
                        row.instance_id
                    );
                    assert_eq!(
                        got.fail_to_pass.failure, want.fail_to_pass.failure,
                        "{name}: {} F2P failure",
                        row.instance_id
                    );
                    assert_eq!(
                        got.pass_to_pass.success, want.pass_to_pass.success,
                        "{name}: {} P2P success",
                        row.instance_id
                    );
                    assert_eq!(
                        got.pass_to_pass.failure, want.pass_to_pass.failure,
                        "{name}: {} P2P failure",
                        row.instance_id
                    );
                    assert_eq!(
                        got.fail_to_fail.success, want.fail_to_fail.success,
                        "{name}: {} F2F success",
                        row.instance_id
                    );
                    assert_eq!(
                        got.pass_to_fail.failure, want.pass_to_fail.failure,
                        "{name}: {} P2F failure",
                        row.instance_id
                    );
                }
                (None, None) => {}
                (got, want) => panic!(
                    "{name}: {} tests_status presence differs (got {}, banked {})",
                    row.instance_id,
                    got.is_some(),
                    want.is_some()
                ),
            }

            cases += 1;
            graded_rows += 1;
            if row.resolved {
                resolved += 1;
            }
        }
        per_run.insert(name, (graded_rows, resolved));
    }

    assert_eq!(cases, GRADED_CASES, "555 graded cases");
    assert_eq!(ungraded, UNGRADED_ROWS, "2 harness-level infra rows");
    assert_eq!(cases + ungraded, TOTAL_ROWS, "557 rows in total");
    println!("parity: {cases} rows across {} runs", runs.len());
    for (name, (n, resolved)) in &per_run {
        println!("  {name}: {resolved}/{n} resolved");
    }

    // ── the known answers (§19: a grader that cannot fail is worthless) ─────────────────────
    let gold_ok = [
        "verified50-v1-control-gold-091615504508",
        "verified50-v1-control-gold-091709203869",
    ];
    for name in gold_ok {
        let (n, resolved) = per_run
            .get(name)
            .unwrap_or_else(|| panic!("{name} missing"));
        assert_eq!(
            (*resolved, *n),
            (50, 50),
            "{name}: the gold control must be 50/50"
        );
    }
    for name in [
        "verified50-v1-control-empty-09161555116e",
        "verified50-v1-control-empty-0917093013f5",
    ] {
        let (n, resolved) = per_run
            .get(name)
            .unwrap_or_else(|| panic!("{name} missing"));
        assert_eq!(
            (*resolved, *n),
            (0, 50),
            "{name}: the empty control must be 0/50"
        );
    }
    // The two early gold runs are reproduced as 0/50 on purpose (see the module docs).
    for name in [
        "verified50-v1-control-gold-09161231d54b",
        "verified50-v1-control-gold-091612332af6",
    ] {
        let (n, resolved) = per_run
            .get(name)
            .unwrap_or_else(|| panic!("{name} missing"));
        assert_eq!(
            (*resolved, *n),
            (0, if name.ends_with("d54b") { 2 } else { 50 }),
            "{name}: historical pre-fix gold run, reproduced as banked"
        );
    }
}

#[test]
fn the_banked_comparable_pair_is_p_0289_and_the_oracle_mismatch_is_refused() {
    let fixtures = fixtures_dir();
    let a_run = fixtures.join("verified50-v1-grade-09180535e0cc");
    let b_run = fixtures.join("verified50-v1-grade-091719199e42");
    let other_run = fixtures.join("verified50-v1-grade-09171352d352");

    let a_arm = Arm::derive(&run_label(&a_run));
    let b_arm = Arm::derive(&run_label(&b_run));
    let other_arm = Arm::derive(&run_label(&other_run));
    assert!(
        a_arm.comparable(&b_arm),
        "the valid pair shares oracle+prompt"
    );

    let m = compare(a_arm, b_arm, &verdicts(&a_run), &verdicts(&b_run)).expect("comparable");
    assert_eq!(
        (
            m.counts.both,
            m.counts.neither,
            m.counts.a_only,
            m.counts.b_only
        ),
        (22, 20, 2, 6),
        "the banked 2x2"
    );
    assert_eq!(m.counts.paired, 50);
    assert_eq!(m.counts.missing_in_one, 0);
    assert!(
        (m.p_value - 0.289).abs() < 0.0005,
        "exact two-sided McNemar p = {} (want 0.289)",
        m.p_value
    );

    let refused = compare(other_arm, b_arm, &verdicts(&other_run), &verdicts(&b_run))
        .expect_err("files vs files+test must be refused");
    assert!(matches!(refused, CompareError::NotComparable { .. }));
}
