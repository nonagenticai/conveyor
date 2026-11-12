//! Per-repo log parsers, ported from `swebench.harness.log_parsers.python` @ 5.0.2.
//!
//! The twelve repos in the verified50 suite map onto six distinct functions
//! (`swebench/harness/log_parsers/python.py:275-311`):
//!
//! | function | dataset `log_parser` keys |
//! |---|---|
//! | [`parse_log_pytest`] | `parse_log_pytest`, `parse_log_flask`, `parse_log_xarray`, `parse_log_astroid`, `parse_log_marshmallow`, `parse_log_pvlib`, `parse_log_pyvista`, `parse_log_sqlfluff` |
//! | [`parse_log_pytest_options`] | `parse_log_pytest_options`, `parse_log_requests`, `parse_log_pylint`, `parse_log_pydicom` |
//! | [`parse_log_pytest_v2`] | `parse_log_pytest_v2`, `parse_log_astropy`, `parse_log_scikit`, `parse_log_sphinx` |
//! | [`parse_log_django`] | `parse_log_django` |
//! | [`parse_log_seaborn`] | `parse_log_seaborn` |
//! | [`parse_log_sympy`] | `parse_log_sympy` |
//!
//! Each parser returns a `test id → status` map. The grading module slices the log between the
//! start/end markers and falls back to the whole log when the slice yields nothing — exactly as
//! `swebench.harness.grading.get_logs_eval` does.

use std::collections::BTreeMap;

use regex::Regex;

/// swebench's `TestStatus` values (`harness/constants/__init__.py`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TestStatus {
    Failed,
    Passed,
    Skipped,
    Error,
    Xfail,
}

impl TestStatus {
    pub const ALL: [TestStatus; 5] = [
        TestStatus::Failed,
        TestStatus::Passed,
        TestStatus::Skipped,
        TestStatus::Error,
        TestStatus::Xfail,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            TestStatus::Failed => "FAILED",
            TestStatus::Passed => "PASSED",
            TestStatus::Skipped => "SKIPPED",
            TestStatus::Error => "ERROR",
            TestStatus::Xfail => "XFAIL",
        }
    }

    pub fn parse(token: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.as_str() == token)
    }

    /// PASSED and XFAIL grade identically (`grading.py:79`).
    pub fn is_passing(self) -> bool {
        matches!(self, TestStatus::Passed | TestStatus::Xfail)
    }
}

/// A log parser: text in, `test id → status` out.
pub type ParserFn = fn(&str) -> BTreeMap<String, TestStatus>;

/// Resolve a dataset `log_parser` key to its parser. `None` for a key the harness does not know.
pub fn parser_for(key: &str) -> Option<ParserFn> {
    Some(match key {
        "parse_log_pytest"
        | "parse_log_astroid"
        | "parse_log_flask"
        | "parse_log_marshmallow"
        | "parse_log_pvlib"
        | "parse_log_pyvista"
        | "parse_log_sqlfluff"
        | "parse_log_xarray" => parse_log_pytest,
        "parse_log_pytest_options"
        | "parse_log_pydicom"
        | "parse_log_requests"
        | "parse_log_pylint" => parse_log_pytest_options,
        "parse_log_pytest_v2" | "parse_log_astropy" | "parse_log_scikit" | "parse_log_sphinx" => {
            parse_log_pytest_v2
        }
        "parse_log_django" => parse_log_django,
        "parse_log_seaborn" => parse_log_seaborn,
        "parse_log_sympy" => parse_log_sympy,
        "parse_log_matplotlib" => parse_log_matplotlib,
        _ => return None,
    })
}

/// Parse `log` with the parser named by `key`.
pub fn parse(key: &str, log: &str) -> Option<BTreeMap<String, TestStatus>> {
    parser_for(key).map(|f| f(log))
}

fn status_prefix(line: &str) -> Option<TestStatus> {
    TestStatus::ALL
        .into_iter()
        .find(|status| line.starts_with(status.as_str()))
}

fn status_suffix(line: &str) -> Option<TestStatus> {
    TestStatus::ALL
        .into_iter()
        .find(|status| line.ends_with(status.as_str()))
}

fn skip_summary_regex() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\[\d+\]$").expect("static regex"))
}

/// pytest's `SKIPPED [N] path:line: reason` summary line, where `[N]` is not a test name.
fn is_skip_summary(status: TestStatus, name: &str) -> bool {
    status == TestStatus::Skipped && skip_summary_regex().is_match(name)
}

/// `swebench.harness.log_parsers.python.parse_log_pytest`
pub fn parse_log_pytest(log: &str) -> BTreeMap<String, TestStatus> {
    let mut map = BTreeMap::new();
    for line in log.split('\n') {
        let Some(status) = status_prefix(line) else {
            continue;
        };
        let line = if status == TestStatus::Failed {
            line.replace(" - ", " ")
        } else {
            line.to_owned()
        };
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() <= 1 {
            continue;
        }
        if is_skip_summary(status, parts[1]) {
            continue;
        }
        map.insert(parts[1].to_owned(), status);
    }
    map
}

/// `swebench.harness.log_parsers.python.parse_log_pytest_options`
pub fn parse_log_pytest_options(log: &str) -> BTreeMap<String, TestStatus> {
    let option_pattern = Regex::new(r"(.*?)\[(.*)\]").expect("static regex");
    let mut map = BTreeMap::new();
    for line in log.split('\n') {
        let Some(status) = status_prefix(line) else {
            continue;
        };
        let line = if status == TestStatus::Failed {
            line.replace(" - ", " ")
        } else {
            line.to_owned()
        };
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() <= 1 {
            continue;
        }
        if is_skip_summary(status, parts[1]) {
            continue;
        }
        let name = match option_pattern.captures(parts[1]) {
            Some(caps) => {
                let main = caps.get(1).expect("group 1").as_str();
                let mut option = caps.get(2).expect("group 2").as_str().to_owned();
                if option.starts_with('/') && !option.starts_with("//") && !option.contains('*') {
                    option = format!("/{}", option.rsplit('/').next().unwrap_or(""));
                }
                format!("{main}[{option}]")
            }
            None => parts[1].to_owned(),
        };
        map.insert(name, status);
    }
    map
}

/// `swebench.harness.log_parsers.python.parse_log_pytest_v2`
pub fn parse_log_pytest_v2(log: &str) -> BTreeMap<String, TestStatus> {
    let ansi = Regex::new(r"\[(\d+)m").expect("static regex");
    let mut map = BTreeMap::new();
    for line in log.split('\n') {
        // The escape sequence's leading \x1b is removed by the control-character filter below.
        let line = ansi.replace_all(line, "").into_owned();
        let line: String = line
            .chars()
            .filter(|c| !('\u{1}'..='\u{1f}').contains(c))
            .collect();
        if let Some(status) = status_prefix(&line) {
            let line = if status == TestStatus::Failed {
                line.split_once(" - ")
                    .map(|(head, _)| head.to_owned())
                    .unwrap_or(line)
            } else {
                line
            };
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 && !is_skip_summary(status, parts[1]) {
                map.insert(parts[1..].join(" "), status);
            }
        } else if let Some(status) = status_suffix(&line) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                map.insert(parts[..parts.len() - 1].join(" "), status);
            }
        }
    }
    map
}

/// `swebench.harness.log_parsers.python.parse_log_django`
pub fn parse_log_django(log: &str) -> BTreeMap<String, TestStatus> {
    let mut map = BTreeMap::new();
    let mut prev_test: Option<String> = None;

    for raw in log.split('\n') {
        let mut line = raw.trim().to_owned();

        if line.contains("--version is equivalent to version") {
            map.insert(
                "--version is equivalent to version".to_owned(),
                TestStatus::Passed,
            );
        }

        if let Some((before, _)) = line.split_once(" ... ") {
            prev_test = Some(before.to_owned());
        }

        for suffix in [" ... ok", " ... OK", " ...  OK"] {
            if line.ends_with(suffix) {
                if line
                    .trim_start()
                    .starts_with("Applying sites.0002_alter_domain_unique...test_no_migrations")
                {
                    line = line
                        .split_once("...")
                        .map(|(_, tail)| tail.trim().to_owned())
                        .unwrap_or(line);
                }
                let test = line
                    .strip_suffix(suffix)
                    .map(str::to_owned)
                    .unwrap_or_default();
                map.insert(test, TestStatus::Passed);
                break;
            }
        }
        if let Some((test, _)) = line.split_once(" ... skipped") {
            map.insert(test.to_owned(), TestStatus::Skipped);
        }
        if let Some(test) = line.strip_suffix(" ... FAIL") {
            map.insert(test.to_owned(), TestStatus::Failed);
        }
        if let Some(rest) = line.strip_prefix("FAIL:")
            && let Some(test) = rest.split_whitespace().next()
        {
            map.insert(test.trim().to_owned(), TestStatus::Failed);
        }
        if let Some(test) = line.strip_suffix(" ... ERROR") {
            map.insert(test.to_owned(), TestStatus::Error);
        }
        if let Some(rest) = line.strip_prefix("ERROR:")
            && let Some(test) = rest.split_whitespace().next()
        {
            map.insert(test.trim().to_owned(), TestStatus::Error);
        }

        if line.trim_start().starts_with("ok")
            && let Some(prev) = &prev_test
        {
            map.insert(prev.clone(), TestStatus::Passed);
        }
    }

    // The three multiline shapes the django logger produces around `... ok`.
    let patterns = [
        r"^(.*?)\s\.\.\.\sTesting against Django installed in ((?s:.*?))\ silenced\)\.\nok$",
        r"^(.*?)\s\.\.\.\sInternal Server Error: \/(.*)\/\nok$",
        r"^(.*?)\s\.\.\.\sSystem check identified no issues \(0 silenced\)\nok$",
    ];
    for pattern in patterns {
        let re = Regex::new(&format!("(?m){pattern}")).expect("static regex");
        for caps in re.captures_iter(log) {
            let test = caps.get(1).expect("group 1").as_str();
            map.insert(test.to_owned(), TestStatus::Passed);
        }
    }
    map
}

/// `swebench.harness.log_parsers.python.parse_log_seaborn`
pub fn parse_log_seaborn(log: &str) -> BTreeMap<String, TestStatus> {
    let mut map = BTreeMap::new();
    for line in log.split('\n') {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }
        if line.starts_with(TestStatus::Failed.as_str()) {
            map.insert(parts[1].to_owned(), TestStatus::Failed);
        } else if line.contains(" PASSED ") {
            if parts[1] == TestStatus::Passed.as_str() {
                map.insert(parts[0].to_owned(), TestStatus::Passed);
            }
        } else if line.starts_with(TestStatus::Passed.as_str()) {
            map.insert(parts[1].to_owned(), TestStatus::Passed);
        }
    }
    map
}

/// `swebench.harness.log_parsers.python.parse_log_sympy`
pub fn parse_log_sympy(log: &str) -> BTreeMap<String, TestStatus> {
    let mut map = BTreeMap::new();
    let pattern = Regex::new(r"(_*) (.*)\.py:(.*) (_*)").expect("static regex");
    for caps in pattern.captures_iter(log) {
        let file = caps.get(2).expect("group 2").as_str();
        let detail = caps.get(3).expect("group 3").as_str();
        map.insert(format!("{file}.py:{detail}"), TestStatus::Failed);
    }
    for line in log.split('\n') {
        let line = line.trim();
        if !line.starts_with("test_") {
            continue;
        }
        let Some(test) = line.split_whitespace().next() else {
            continue;
        };
        if line.ends_with(" E") {
            map.insert(test.to_owned(), TestStatus::Error);
        }
        if line.ends_with(" F") {
            map.insert(test.to_owned(), TestStatus::Failed);
        }
        if line.ends_with(" ok") {
            map.insert(test.to_owned(), TestStatus::Passed);
        }
    }
    map
}

/// `swebench.harness.log_parsers.python.parse_log_matplotlib`
pub fn parse_log_matplotlib(log: &str) -> BTreeMap<String, TestStatus> {
    let mut map = BTreeMap::new();
    for line in log.split('\n') {
        let line = line
            .replace("MouseButton.LEFT", "1")
            .replace("MouseButton.RIGHT", "3");
        let Some(status) = status_prefix(&line) else {
            continue;
        };
        let line = if status == TestStatus::Failed {
            line.replace(" - ", " ")
        } else {
            line
        };
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() <= 1 {
            continue;
        }
        if is_skip_summary(status, parts[1]) {
            continue;
        }
        map.insert(parts[1].to_owned(), status);
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pytest_takes_the_second_token_and_drops_the_skip_summary() {
        let log = "PASSED tests/test_a.py::test_one\n\
                   FAILED tests/test_b.py::test_two - assert 1 == 2\n\
                   SKIPPED [3] tests/test_c.py:12: needs network\n\
                   XFAIL tests/test_d.py::test_four\n";
        let map = parse_log_pytest(log);
        assert_eq!(
            map.get("tests/test_a.py::test_one"),
            Some(&TestStatus::Passed)
        );
        // The " - assertion" suffix is removed so the id cannot absorb it.
        assert_eq!(
            map.get("tests/test_b.py::test_two"),
            Some(&TestStatus::Failed)
        );
        assert_eq!(map.len(), 3, "the SKIPPED [3] summary is not a test");
        assert_eq!(
            map.get("tests/test_d.py::test_four"),
            Some(&TestStatus::Xfail)
        );
    }

    #[test]
    fn pytest_options_keeps_the_parameter_block() {
        let log = "PASSED tests/test_a.py::test_one[param-1]\n\
                   FAILED tests/test_b.py::test_two[/abs/dir/file.txt] - boom\n";
        let map = parse_log_pytest_options(log);
        assert!(map.contains_key("tests/test_a.py::test_one[param-1]"));
        // An absolute-path parameter keeps only its basename.
        assert!(map.contains_key("tests/test_b.py::test_two[/file.txt]"));
    }

    #[test]
    fn pytest_v2_strips_ansi_and_coloured_output() {
        let log = "\u{1b}[32mPASSED\u{1b}[0m tests/test_a.py::test_one\n\
                   \u{1b}[31mFAILED\u{1b}[0m tests/test_b.py::test_two - assert\n";
        let map = parse_log_pytest_v2(log);
        assert_eq!(
            map.get("tests/test_a.py::test_one"),
            Some(&TestStatus::Passed)
        );
        assert_eq!(
            map.get("tests/test_b.py::test_two"),
            Some(&TestStatus::Failed)
        );
    }

    #[test]
    fn pytest_v2_reads_a_trailing_status() {
        let log = "tests/test_a.py::test_one PASSED\n";
        let map = parse_log_pytest_v2(log);
        assert_eq!(
            map.get("tests/test_a.py::test_one"),
            Some(&TestStatus::Passed)
        );
    }

    #[test]
    fn django_reads_dots_ok_fail_and_error_lines() {
        let log = "test_one (module.Class) ... ok\n\
                   test_two (module.Class) ... skipped 'no db'\n\
                   test_three (module.Class) ... FAIL\n\
                   test_four (module.Class) ... ERROR\n\
                   FAIL: test_five (module.Class)\n\
                   ERROR: test_six (module.Class)\n";
        let map = parse_log_django(log);
        assert_eq!(
            map.get("test_one (module.Class)"),
            Some(&TestStatus::Passed)
        );
        assert_eq!(
            map.get("test_two (module.Class)"),
            Some(&TestStatus::Skipped)
        );
        assert_eq!(
            map.get("test_three (module.Class)"),
            Some(&TestStatus::Failed)
        );
        assert_eq!(
            map.get("test_four (module.Class)"),
            Some(&TestStatus::Error)
        );
        // The `FAIL:`/`ERROR:` fallback keys on the first token after the colon — that is
        // swebench's own behaviour (`log_parsers/python.py:127-135`), not the full Django id.
        assert_eq!(map.get("test_five"), Some(&TestStatus::Failed));
        assert_eq!(map.get("test_six"), Some(&TestStatus::Error));
    }

    #[test]
    fn django_carries_a_test_across_interleaved_output() {
        // The shape documented at swebench log_parsers/python.py:137-141.
        let log = "test_one (module.Class) ... some interleaved output\nok\n";
        let map = parse_log_django(log);
        assert_eq!(
            map.get("test_one (module.Class)"),
            Some(&TestStatus::Passed)
        );
    }

    #[test]
    fn seaborn_reads_both_orders() {
        // The middle branch requires spaces on BOTH sides of PASSED (`python.py:209-212`), so
        // "test_one PASSED" with nothing after it is deliberately not parsed.
        let log = "test_one  PASSED [ 50%]\nPASSED test_two\nFAILED test_three\n";
        let map = parse_log_seaborn(log);
        assert_eq!(map.get("test_one"), Some(&TestStatus::Passed));
        assert_eq!(map.get("test_two"), Some(&TestStatus::Passed));
        assert_eq!(map.get("test_three"), Some(&TestStatus::Failed));
    }

    #[test]
    fn sympy_reads_the_underscored_header_and_the_status_suffixes() {
        let log = "_____ sympy/core/tests/test_arit.py:12 test_add _____\n\
                   test_ok ok\n\
                   test_bad F\n\
                   test_broken E\n";
        let map = parse_log_sympy(log);
        assert_eq!(
            map.get("sympy/core/tests/test_arit.py:12 test_add"),
            Some(&TestStatus::Failed)
        );
        assert_eq!(map.get("test_ok"), Some(&TestStatus::Passed));
        assert_eq!(map.get("test_bad"), Some(&TestStatus::Failed));
        assert_eq!(map.get("test_broken"), Some(&TestStatus::Error));
    }

    #[test]
    fn every_dataset_parser_key_resolves() {
        for key in [
            "parse_log_django",
            "parse_log_sympy",
            "parse_log_sphinx",
            "parse_log_matplotlib",
            "parse_log_scikit",
            "parse_log_astropy",
            "parse_log_xarray",
            "parse_log_pytest",
            "parse_log_seaborn",
            "parse_log_flask",
            "parse_log_requests",
            "parse_log_pylint",
        ] {
            assert!(parser_for(key).is_some(), "{key} must resolve");
        }
        assert!(parser_for("parse_log_not_a_parser").is_none());
    }
}
