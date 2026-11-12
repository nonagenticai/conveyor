//! The seed's eval-script transform — a Rust port of swebench 5.0.2's `make_test_spec`.
//!
//! In 5.0.2 the harness no longer builds the script from a per-repo spec table: the dataset row
//! carries `eval_script`, and `swebench.harness.utils.make_test_spec` only normalises it
//! (`parse_eval_script`, `utils.py:215-220`) and records the test command's own exit status
//! around the end-of-output marker (`record_test_exit_code`, `utils.py:223-245`). `TestSpec`'s
//! `eval_script` property (`types.py:44-49`) then re-adds the shebang and `set -uxo pipefail`.
//!
//! This module reproduces that pipeline **byte for byte**. The golden fixture
//! `tests/fixtures/benchmark/golden-eval-scripts.json` was generated once by the pinned harness
//! (`swebench==5.0.2`) for the 50 instances; `crates/domain/tests/benchmark_specgen.rs` asserts
//! equality for every one of them.
//!
//! Why the exit-code lines matter: the eval script ends with a `git checkout` that resets the
//! test files, and runs under `set -uxo pipefail` without `-e`, so the script's exit status is
//! the reset's, not the tests'. The recorded `SWEBENCH_TEST_EXIT_CODE` lets grading cross-check
//! a log that claims every test passed against the command that actually ran
//! (`swebench/harness/grading.py:162-176`).

/// The end-of-test-output marker emitted by every eval script.
pub const END_TEST_OUTPUT: &str = ">>>>> End Test Output";

/// The shell variable the eval script stashes the test command's `$?` in.
pub const TEST_EXIT_CODE_VAR: &str = "SWEBENCH_TEST_EXIT_CODE";

/// Build the eval script exactly as `make_test_spec(instance).eval_script` does.
///
/// `dataset_eval_script` is the `eval_script` column of the pinned dataset row.
pub fn eval_script(dataset_eval_script: &str) -> String {
    // `parse_eval_script`: `eval_script.strip().split("\n")`, dropping the two header lines.
    let mut lines: Vec<String> = dataset_eval_script
        .trim()
        .split('\n')
        .filter(|line| *line != "#!/bin/bash" && *line != "set -uxo pipefail")
        .map(str::to_owned)
        .collect();

    // `record_test_exit_code`: insert the recorder before the first end marker (and return
    // untouched when there is none — the fallback is deliberate in the harness).
    if let Some(i) = lines.iter().position(|line| line.contains(END_TEST_OUTPUT)) {
        lines.insert(i, format!("{TEST_EXIT_CODE_VAR}=$?"));
        lines.insert(
            i + 2,
            format!("echo \">>>>> Test Exit Code: ${TEST_EXIT_CODE_VAR}\""),
        );
    }

    // `TestSpec.eval_script`: the header is re-added and the list joined with one trailing newline.
    let mut all: Vec<String> = Vec::with_capacity(lines.len() + 2);
    all.push("#!/bin/bash".to_owned());
    all.push("set -uxo pipefail".to_owned());
    all.extend(lines);
    format!("{}\n", all.join("\n"))
}

/// The paths a unified diff modifies: the `b/` side of every `diff --git` header.
///
/// Ported from `conveyor-bench/drive.py:131-143`. Deduped, order preserved; a rename yields the
/// new path only; an empty patch yields no paths.
pub fn oracle_files(patch: &str) -> Vec<String> {
    use std::collections::HashSet;

    use regex::Regex;

    let header = Regex::new(r"(?m)^diff --git a/(.+?) b/(.+)$").expect("static regex");
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for caps in header.captures_iter(patch) {
        let path = caps.get(2).expect("group b").as_str().to_owned();
        if seen.insert(path.clone()) {
            out.push(path);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const RAW: &str = "#!/bin/bash\nset -uxo pipefail\nsource /opt/miniconda3/bin/activate\ncd /testbed\n: '>>>>> Start Test Output'\npytest -rA tests/test_x.py\n: '>>>>> End Test Output'\ngit checkout deadbeef tests/test_x.py\n";

    #[test]
    fn the_exit_code_lines_are_inserted_around_the_end_marker() {
        let got = eval_script(RAW);
        let want = "#!/bin/bash\nset -uxo pipefail\n\
                    source /opt/miniconda3/bin/activate\n\
                    cd /testbed\n\
                    : '>>>>> Start Test Output'\n\
                    pytest -rA tests/test_x.py\n\
                    SWEBENCH_TEST_EXIT_CODE=$?\n\
                    : '>>>>> End Test Output'\n\
                    echo \">>>>> Test Exit Code: $SWEBENCH_TEST_EXIT_CODE\"\n\
                    git checkout deadbeef tests/test_x.py\n";
        assert_eq!(got, want);
    }

    #[test]
    fn the_header_is_normalised_not_duplicated() {
        let got = eval_script("set -uxo pipefail\ncd /testbed\n");
        assert_eq!(got, "#!/bin/bash\nset -uxo pipefail\ncd /testbed\n");
    }

    #[test]
    fn a_script_without_the_marker_is_left_alone() {
        let got = eval_script("#!/bin/bash\nset -uxo pipefail\ncd /testbed\n");
        assert_eq!(got, "#!/bin/bash\nset -uxo pipefail\ncd /testbed\n");
        assert!(!got.contains(TEST_EXIT_CODE_VAR));
    }

    #[test]
    fn oracle_files_are_the_b_side_deduped_in_order() {
        let patch = "diff --git a/astropy/core.py b/astropy/core.py\n\
                     index 1..2 100644\n\
                     --- a/astropy/core.py\n\
                     +++ b/astropy/core.py\n\
                     @@ -1 +1 @@\n\
                     -old\n\
                     +new\n\
                     diff --git a/astropy/core.py b/astropy/core.py\n\
                     diff --git a/old_name.py b/new_name.py\n";
        assert_eq!(
            oracle_files(patch),
            vec!["astropy/core.py".to_owned(), "new_name.py".to_owned()]
        );
    }

    #[test]
    fn an_empty_patch_has_no_oracle_files() {
        assert!(oracle_files("").is_empty());
    }
}
