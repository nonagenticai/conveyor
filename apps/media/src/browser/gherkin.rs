//! `media.run_gherkin` — a Gherkin feature executed natively against one page.
//!
//! ADR 0007 §1 absorbs `frontend_test_runner_service` as **one native kind**: the feature document
//! is the request, the image's own Chromium is the runner, and no Node, Playwright or package
//! download is involved. The captured reference executed an arbitrary Playwright ZIP; its drop
//! table (`contract/frontend_test_runner_service/README.md`) replaces `package_url`/`bdd_job_id`
//! with the feature text and drops the suite/recording knobs.
//!
//! ## One session, driven over CDP
//!
//! A scenario launches its own headless Chromium (`--remote-debugging-port=0`, a fresh profile),
//! attaches one page target and runs **every step of that scenario over that session**: the page
//! is opened once, interactions are real browser input, and an assertion reads the live DOM after
//! the steps before it. The client is [`super::cdp::Session`] — no Node, no Playwright, no package
//! download.
//!
//! A step whose text is outside the table below is **refused by name** with [`code::STEP`]
//! (`step_not_supported`), naming the step, rather than imitated or quietly skipped. A test that
//! passes because its click never ran is worse than one that refuses. A selector that matches no
//! element fails its step with a typed error naming the step — never a silent no-op.
//! `Background:` is refused with [`code::BACKGROUND`], and every Gherkin construct outside
//! `Feature:`/`Scenario:`/steps is a malformed feature (`Invalid`, with the line number).
//!
//! Text assertions are **substring matches on the dumped DOM** (`Runtime.evaluate`
//! `document.documentElement.outerHTML`), not a rendered-text extraction: text split across
//! elements is not seen, and a word that also appears in the page's own `<script>` source is found
//! whether or not it is rendered. Selectors are a simple textual subset (`#id`, `.class`, `tag`,
//! `[attr=value]`) for **both** assertion and interaction steps — assertions match it with a small
//! tag/attribute scanner, interactions hand it to the page's own CSS engine. A compound selector
//! (`div > p`, `article:has(h1)`, `h1, h2`, `:nth-child(...)`) is refused with
//! [`code::SELECTOR`].
//!
//! ## Steps
//!
//! | step | meaning |
//! |---|---|
//! | `I open <url>` / `I visit <url>` | load the page under test |
//! | `I click "<selector>"` | click the element |
//! | `I type "<text>" into "<selector>"` | type into the focused element (appends) |
//! | `I scroll down <pixels>` / `I scroll up <pixels>` | one wheel event on the page |
//! | `I hover "<selector>"` | move the pointer to the element's centre |
//! | `I upload "<path>" into "<selector>"` | set a file input's files |
//! | `I expect to see "<text>"` / `the page contains "<text>"` | the DOM contains the text |
//! | `I expect not to see "<text>"` | the DOM does not contain the text |
//! | `I expect the title to be "<title>"` / `the title is "<title>"` | exact `<title>` |
//! | `I expect the page to have a "<selector>" element` | an element matches the selector |
//! | `I expect no "<selector>" element` | no element matches the selector |
//!
//! Step keywords (`Given|When|Then|And|*`) and step text are matched case-insensitively; quoted
//! arguments keep their case. Tags (`@smoke`) and `#` comments are ignored.
//!
//! ## Request and result
//!
//! | field | meaning |
//! |---|---|
//! | `feature` | the Gherkin document |
//! | `target_url` | the page under test: `file://`, or `http(s)://` with `allow_network: true` |
//! | `timeout_per_test_ms` | 1000–600000, default 30000; bounds each scenario attempt |
//! | `retries` | 0–10, default 0; re-runs a failed scenario |
//! | `screenshot_on_failure` | write `<scenario>.png` into `out_dir` when a scenario (still) fails |
//! | `trace_on_failure` | write `<scenario>.json` — the step list and page URL — when a scenario (still) fails |
//! | `allow_network` | the deployment's permission to reach `http(s)` URLs |
//!
//! A scenario runs its steps in order on one session; the first step that needs the page loads
//! `target_url`, and a navigation step replaces it. Assertions read **one dump per batch**: the
//! first assertion after a step that changed the page takes the dump, and that batch's assertions
//! share it. A navigation step is itself such a boundary, so its verdict is whether the page
//! loaded.
//!
//! The screenshot and trace names are slugified, so a scenario name can never escape `out_dir`.
//!
//! `target_url` and every navigation URL may be `file://` unconditionally; an `http(s)` URL needs
//! `allow_network: true`, otherwise [`code::EGRESS_DENIED`] — the `media.record_browser`
//! convention. A media pod has no egress unless the deployment granted it for this kind.
//!
//! The result document is `{feature, target_url, scenarios, passed, failed, exit_status}`, where
//! `feature` is the `Feature:` name, `exit_status` is 0 when every scenario passed and 1 otherwise,
//! and each scenario is `{name, status, steps, duration_ms, trace?}` with each step
//! `{text, status, detail}` (`status` is `"passed"` or `"failed"`). `trace` is the trace file's
//! name and is present only when a failed scenario wrote one.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};
use tokio::process::{Child, Command};

use crate::browser::cdp::Session;
use crate::common::{CapabilityError, Config, process};

/// The kind slug this module implements.
pub const KIND: &str = "media.run_gherkin";

/// The stable refusal codes. Each names one capability this port does not serve, so a caller can
/// branch on the code instead of reading a sentence.
pub mod code {
    pub const STEP: &str = "step_not_supported";
    pub const BACKGROUND: &str = "background_not_supported";
    pub const SELECTOR: &str = "selector_not_supported";
    pub const EGRESS_DENIED: &str = "egress_denied";
    pub const BROWSER: &str = "browser_not_available";
}

/// The browser binary. The image installs Chromium at `/usr/bin/chromium`; a developer box points
/// this at whatever Chrome it has, and may fall through to a stock installation.
const CHROME_ENV: &str = "CHROME_BIN";
const CHROME_DEFAULT: &str = "/usr/bin/chromium";
const CHROME_DEVELOPER: &str = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";

/// The captured default and range of `timeout_per_test_ms`.
const DEFAULT_TIMEOUT_MS: u64 = 30_000;
const TIMEOUT_MIN_MS: u64 = 1_000;
const TIMEOUT_MAX_MS: u64 = 600_000;
/// The captured range of `retries`.
const RETRIES_MAX: u64 = 10;

/// The failure screenshot's viewport. The request carries no viewport for this kind, so the
/// screenshot takes the renderer's default.
const SCREENSHOT_WIDTH: u32 = 1280;
const SCREENSHOT_HEIGHT: u32 = 720;

/// Execute one Gherkin run.
pub async fn run(params: &Value, config: &Config) -> Result<Value, CapabilityError> {
    let options = Options::parse(params)?;
    let feature = parse(&options.feature)?;

    // Refuse an un-granted remote URL before any browser process starts: a DNS error at Chromium
    // would read like a transient fault, not like the policy it is.
    check_url(
        "target_url",
        &options.target_url,
        options.allow_network,
        None,
    )?;
    for scenario in &feature.scenarios {
        for step in &scenario.steps {
            if let Action::Open(url) = &step.action {
                check_url(
                    "navigation step",
                    url,
                    options.allow_network,
                    Some(step.line),
                )?;
            }
        }
    }

    let browser = Browser::new(config, options.timeout_ms)?;
    std::fs::create_dir_all(&config.out_dir).map_err(|error| {
        CapabilityError::Failed(format!("cannot create {:?}: {error}", config.out_dir))
    })?;

    let mut scenarios = Vec::with_capacity(feature.scenarios.len());
    for scenario in &feature.scenarios {
        let mut result =
            run_with_retries(scenario, &options.target_url, &browser, options.retries).await;
        if result.status == Status::Failed && options.screenshot_on_failure {
            let output = config.out_dir.join(format!("{}.png", slug(&scenario.name)));
            if let Err(error) = browser.screenshot(&result.last_url, &output).await {
                // The verdict is already "failed"; a screenshot that cannot be written must not
                // mask it, but it must not vanish silently either.
                tracing::warn!(
                    scenario = %scenario.name,
                    path = %output.display(),
                    "screenshot_on_failure could not write the screenshot: {error}"
                );
            }
        }
        if result.status == Status::Failed && options.trace_on_failure {
            let name = format!("{}.json", slug(&scenario.name));
            let output = config.out_dir.join(&name);
            match write_trace(&output, &result) {
                Ok(()) => result.trace = Some(name),
                // Same rule as the screenshot: the verdict stands, and the missing trace is
                // reported rather than swallowed.
                Err(error) => tracing::warn!(
                    scenario = %scenario.name,
                    path = %output.display(),
                    "trace_on_failure could not write the trace: {error}"
                ),
            }
        }
        scenarios.push(result);
    }

    let passed = scenarios
        .iter()
        .filter(|scenario| scenario.status == Status::Passed)
        .count();
    let failed = scenarios.len() - passed;

    Ok(json!({
        "feature": feature.name,
        "target_url": options.target_url,
        "scenarios": scenarios,
        "passed": passed,
        "failed": failed,
        "exit_status": if failed == 0 { 0 } else { 1 },
    }))
}

// ── the request ─────────────────────────────────────────────────────────────────────────────

/// The validated request fields this kind serves.
struct Options {
    feature: String,
    target_url: String,
    timeout_ms: u64,
    retries: u64,
    screenshot_on_failure: bool,
    trace_on_failure: bool,
    allow_network: bool,
}

impl Options {
    fn parse(params: &Value) -> Result<Self, CapabilityError> {
        let map = params
            .as_object()
            .ok_or_else(|| CapabilityError::Invalid("params must be a JSON object".to_string()))?;

        // The refusal pass runs before anything else, so a dropped Playwright knob is named even
        // when the rest of the request is incomplete.
        for key in map.keys() {
            match key.as_str() {
                "feature"
                | "target_url"
                | "timeout_per_test_ms"
                | "retries"
                | "screenshot_on_failure"
                | "trace_on_failure"
                | "allow_network" => {}
                other => {
                    return Err(CapabilityError::Invalid(format!(
                        "unknown parameter {other:?}"
                    )));
                }
            }
        }

        Ok(Self {
            feature: required_string(map, "feature")?,
            target_url: required_string(map, "target_url")?,
            timeout_ms: bounded_integer(
                map,
                "timeout_per_test_ms",
                DEFAULT_TIMEOUT_MS,
                TIMEOUT_MIN_MS,
                TIMEOUT_MAX_MS,
            )?,
            retries: bounded_integer(map, "retries", 0, 0, RETRIES_MAX)?,
            screenshot_on_failure: optional_bool(map, "screenshot_on_failure")?,
            trace_on_failure: optional_bool(map, "trace_on_failure")?,
            allow_network: optional_bool(map, "allow_network")?,
        })
    }
}

fn required_string(map: &Map<String, Value>, key: &str) -> Result<String, CapabilityError> {
    match map.get(key) {
        Some(Value::String(value)) => Ok(value.clone()),
        Some(_) => Err(CapabilityError::Invalid(format!("{key} must be a string"))),
        None => Err(CapabilityError::Invalid(format!("{key} is required"))),
    }
}

fn optional_bool(map: &Map<String, Value>, key: &str) -> Result<bool, CapabilityError> {
    match map.get(key).filter(|value| !value.is_null()) {
        None => Ok(false),
        Some(Value::Bool(value)) => Ok(*value),
        Some(_) => Err(CapabilityError::Invalid(format!("{key} must be a boolean"))),
    }
}

fn bounded_integer(
    map: &Map<String, Value>,
    key: &str,
    default: u64,
    min: u64,
    max: u64,
) -> Result<u64, CapabilityError> {
    let Some(value) = map.get(key).filter(|value| !value.is_null()) else {
        return Ok(default);
    };
    let number = value
        .as_u64()
        .ok_or_else(|| CapabilityError::Invalid(format!("{key} must be a non-negative integer")))?;
    if !(min..=max).contains(&number) {
        return Err(CapabilityError::Invalid(format!(
            "{key} must be between {min} and {max}, not {number}"
        )));
    }
    Ok(number)
}

/// A `file://` URL is always allowed; an `http(s)` URL needs the deployment's explicit grant.
fn check_url(
    label: &str,
    url: &str,
    allow_network: bool,
    line: Option<usize>,
) -> Result<(), CapabilityError> {
    let prefix = line
        .map(|line| format!("line {line}: "))
        .unwrap_or_default();
    if let Some(path) = url.strip_prefix("file://") {
        if path.is_empty() {
            return Err(CapabilityError::Invalid(format!(
                "{prefix}{label} needs a path after file://"
            )));
        }
        return Ok(());
    }
    if url.starts_with("http://") || url.starts_with("https://") {
        if !allow_network {
            return Err(CapabilityError::refused(
                code::EGRESS_DENIED,
                format!(
                    "{prefix}{label} {url:?} needs network egress; pass allow_network: true only \
                     when the deployment grants this kind egress"
                ),
            ));
        }
        return Ok(());
    }
    Err(CapabilityError::Invalid(format!(
        "{prefix}{label} {url:?} must start with 'file://', 'http://' or 'https://'"
    )))
}

// ── the feature document ────────────────────────────────────────────────────────────────────

struct Feature {
    name: String,
    line: usize,
    scenarios: Vec<Scenario>,
}

struct Scenario {
    name: String,
    line: usize,
    steps: Vec<Step>,
}

struct Step {
    /// The whole trimmed step line, as the result document reports it.
    text: String,
    line: usize,
    action: Action,
}

enum Action {
    Open(String),
    /// A selector already validated against the vocabulary's subset.
    Click(String),
    Type {
        selector: String,
        text: String,
    },
    /// Pixels, positive is down.
    Scroll(i64),
    Hover(String),
    Upload {
        selector: String,
        path: String,
    },
    Assert(Assertion),
}

enum Assertion {
    See(String),
    NotSee(String),
    Title(String),
    Has(Selector),
    HasNot(Selector),
}

/// Parse the feature text, refusing every construct this runner cannot execute.
fn parse(document: &str) -> Result<Feature, CapabilityError> {
    let mut feature: Option<Feature> = None;
    let mut current: Option<Scenario> = None;

    for (index, raw) in document.lines().enumerate() {
        let number = index + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('@') {
            continue;
        }
        if let Some(name) = line.strip_prefix("Feature:") {
            if feature.is_some() {
                return Err(malformed(
                    number,
                    "a feature document has exactly one Feature: line",
                ));
            }
            let name = name.trim();
            if name.is_empty() {
                return Err(malformed(number, "Feature: needs a name"));
            }
            feature = Some(Feature {
                name: name.to_string(),
                line: number,
                scenarios: Vec::new(),
            });
            continue;
        }
        if line.starts_with("Background:") {
            return Err(CapabilityError::refused(
                code::BACKGROUND,
                format!(
                    "line {number}: Background: is not supported; move its steps into each \
                     Scenario:"
                ),
            ));
        }
        if let Some(name) = line.strip_prefix("Scenario:") {
            let name = name.trim();
            if name.is_empty() {
                return Err(malformed(number, "Scenario: needs a name"));
            }
            let Some(feature) = feature.as_mut() else {
                return Err(malformed(number, "Scenario: appears before any Feature:"));
            };
            close_scenario(current.take(), feature)?;
            current = Some(Scenario {
                name: name.to_string(),
                line: number,
                steps: Vec::new(),
            });
            continue;
        }
        if let Some(text) = step_text(line) {
            if feature.is_none() {
                return Err(malformed(number, "a step appears before any Feature:"));
            }
            let Some(scenario) = current.as_mut() else {
                return Err(malformed(number, "a step appears before any Scenario:"));
            };
            let action = compile_step(text, number)?;
            scenario.steps.push(Step {
                text: line.to_string(),
                line: number,
                action,
            });
            continue;
        }
        return Err(malformed(
            number,
            format!("not a line this runner understands: {line:?}"),
        ));
    }

    if let Some(scenario) = current.take() {
        let Some(feature) = feature.as_mut() else {
            return Err(malformed(1, "the document has no Feature: line"));
        };
        close_scenario(Some(scenario), feature)?;
    }
    let Some(feature) = feature else {
        return Err(malformed(1, "the document has no Feature: line"));
    };
    if feature.scenarios.is_empty() {
        return Err(malformed(
            feature.line,
            format!("feature {:?} has no scenarios", feature.name),
        ));
    }
    Ok(feature)
}

/// A scenario with no steps would pass without asserting anything.
fn close_scenario(
    scenario: Option<Scenario>,
    feature: &mut Feature,
) -> Result<(), CapabilityError> {
    if let Some(scenario) = scenario {
        if scenario.steps.is_empty() {
            return Err(malformed(
                scenario.line,
                format!("scenario {:?} has no steps", scenario.name),
            ));
        }
        feature.scenarios.push(scenario);
    }
    Ok(())
}

/// The text after a step keyword, or `None` when the line is not a step.
fn step_text(line: &str) -> Option<&str> {
    for keyword in ["Given", "When", "Then", "And", "*"] {
        if let Some(rest) = line.strip_prefix(keyword)
            && (rest.is_empty() || rest.chars().next().is_some_and(char::is_whitespace))
        {
            return Some(rest.trim());
        }
    }
    None
}

/// Compile the one supported vocabulary, matched case-insensitively on the step text.
fn compile_step(text: &str, line: usize) -> Result<Action, CapabilityError> {
    let text = text.trim();
    if text.is_empty() {
        return Err(malformed(line, "a step needs text"));
    }
    let lower = text.to_ascii_lowercase();

    for prefix in ["i open ", "i visit "] {
        if lower.starts_with(prefix) {
            let url = unquote(text[prefix.len()..].trim());
            if url.is_empty() {
                return Err(malformed(line, "the navigation step needs a URL"));
            }
            return Ok(Action::Open(url.to_string()));
        }
    }

    if let Some(argument) = quoted_argument(text, &lower, "i click ", "") {
        return interaction_step(line, argument, Action::Click);
    }
    if let Some(argument) = quoted_argument(text, &lower, "i hover ", "") {
        return interaction_step(line, argument, Action::Hover);
    }
    if let Some((typed, selector)) = quoted_pair(text, &lower, "i type ", " into ") {
        if typed.is_empty() {
            return Err(malformed(line, "the type step needs text"));
        }
        Ok(Action::Type {
            selector: require_selector(line, selector)?.text(),
            text: typed.to_string(),
        })
    } else if lower.starts_with("i type ") {
        Err(malformed(
            line,
            "the type step is 'I type \"<text>\" into \"<selector>\"'",
        ))
    } else if lower.starts_with("i scroll ") {
        let amount = scroll_amount(&lower).ok_or_else(|| {
            malformed(
                line,
                "the scroll step is 'I scroll down <pixels>' or 'I scroll up <pixels>'",
            )
        })?;
        Ok(Action::Scroll(amount))
    } else if let Some((path, selector)) = quoted_pair(text, &lower, "i upload ", " into ") {
        if path.trim().is_empty() {
            return Err(malformed(line, "the upload step needs a file path"));
        }
        Ok(Action::Upload {
            selector: require_selector(line, selector)?.text(),
            path: path.to_string(),
        })
    } else if lower.starts_with("i upload ") {
        Err(malformed(
            line,
            "the upload step is 'I upload \"<path>\" into \"<selector>\"'",
        ))
    } else if let Some(action) = assertion_step(text, &lower, line)? {
        Ok(action)
    } else {
        Err(CapabilityError::refused(
            code::STEP,
            format!(
                "line {line}: the step '{text}' is not supported; the vocabulary is navigation, \
                 click, type, scroll, hover, upload and the documented assertions"
            ),
        ))
    }
}

/// The assertion half of the vocabulary, or `None` when the text names no assertion.
fn assertion_step(text: &str, lower: &str, line: usize) -> Result<Option<Action>, CapabilityError> {
    if let Some(argument) = quoted_argument(text, lower, "i expect to see ", "") {
        return text_assertion(line, argument, Assertion::See).map(Some);
    }
    if let Some(argument) = quoted_argument(text, lower, "the page contains ", "") {
        return text_assertion(line, argument, Assertion::See).map(Some);
    }
    if let Some(argument) = quoted_argument(text, lower, "i expect not to see ", "") {
        return text_assertion(line, argument, Assertion::NotSee).map(Some);
    }
    if let Some(argument) = quoted_argument(text, lower, "i expect the title to be ", "") {
        return text_assertion(line, argument, Assertion::Title).map(Some);
    }
    if let Some(argument) = quoted_argument(text, lower, "the title is ", "") {
        return text_assertion(line, argument, Assertion::Title).map(Some);
    }
    if let Some(argument) = quoted_argument(text, lower, "i expect the page to have a ", " element")
    {
        return selector_step(line, argument, Assertion::Has).map(Some);
    }
    if let Some(argument) = quoted_argument(text, lower, "i expect no ", " element") {
        return selector_step(line, argument, Assertion::HasNot).map(Some);
    }
    Ok(None)
}

/// `I scroll down 200`, `I scroll up 200 px`: pixels, positive down.
fn scroll_amount(lower: &str) -> Option<i64> {
    let (sign, rest) = if let Some(rest) = lower.strip_prefix("i scroll down ") {
        (1i64, rest)
    } else {
        (-1i64, lower.strip_prefix("i scroll up ")?)
    };
    let rest = rest.trim();
    let rest = rest
        .strip_suffix(" pixels")
        .or_else(|| rest.strip_suffix(" px"))
        .unwrap_or(rest);
    let amount: i64 = rest.trim().parse().ok()?;
    (amount > 0).then_some(sign * amount)
}

fn interaction_step(
    line: usize,
    argument: &str,
    make: fn(String) -> Action,
) -> Result<Action, CapabilityError> {
    if argument.is_empty() {
        return Err(malformed(line, "the interaction step needs a selector"));
    }
    Ok(make(require_selector(line, argument)?.text()))
}

fn text_assertion(
    line: usize,
    argument: &str,
    make: fn(String) -> Assertion,
) -> Result<Action, CapabilityError> {
    if argument.is_empty() {
        return Err(malformed(line, "the step's quoted argument is empty"));
    }
    Ok(Action::Assert(make(argument.to_string())))
}

fn selector_step(
    line: usize,
    argument: &str,
    make: fn(Selector) -> Assertion,
) -> Result<Action, CapabilityError> {
    Ok(Action::Assert(make(require_selector(line, argument)?)))
}

/// The vocabulary's one selector subset, shared by assertion and interaction steps: `#id`,
/// `.class`, `tag`, `[attr=value]`. `None` is the by-name refusal.
fn require_selector(line: usize, argument: &str) -> Result<Selector, CapabilityError> {
    if argument.is_empty() {
        return Err(malformed(line, "the element step needs a selector"));
    }
    parse_selector(argument).ok_or_else(|| {
        CapabilityError::refused(
            code::SELECTOR,
            format!(
                "line {line}: selector {argument:?} is outside the supported subset (#id, .class, \
                 tag, [attr=value]); every step uses that one subset, so a compound selector is \
                 refused rather than resolved by the browser"
            ),
        )
    })
}

/// The quoted argument of a step, where the step is `prefix` + `"` + argument + `"` + `after`.
/// `lower` is the ASCII-lowercased step text, so matching is case-insensitive while the returned
/// slice keeps the caller's case.
fn quoted_argument<'a>(text: &'a str, lower: &str, prefix: &str, after: &str) -> Option<&'a str> {
    let rest = lower.strip_prefix(prefix)?;
    let rest = rest.strip_prefix('"')?;
    let rest = rest.strip_suffix(after)?;
    rest.strip_suffix('"')?;
    let start = prefix.len() + 1;
    let end = text.len() - after.len() - 1;
    Some(&text[start..end])
}

/// The two quoted arguments of a step shaped `prefix + "<first>" + separator + "<second>"`.
/// `lower` is the ASCII-lowercased step text, so matching is case-insensitive while the returned
/// slices keep the caller's case. A quote inside the first argument is kept: the split is the
/// first closing quote that is followed by the separator.
fn quoted_pair<'a>(
    text: &'a str,
    lower: &str,
    prefix: &str,
    separator: &str,
) -> Option<(&'a str, &'a str)> {
    let rest = lower.strip_prefix(prefix)?;
    rest.strip_prefix('"')?;
    let body = text[prefix.len()..].strip_prefix('"')?;
    let mut search = 0;
    while let Some(found) = body[search..].find('"') {
        let close = search + found;
        let after = &body[close + 1..];
        if after.len() >= separator.len()
            && after.as_bytes()[..separator.len()].eq_ignore_ascii_case(separator.as_bytes())
        {
            let tail = &after[separator.len()..];
            if let Some(tail) = tail
                .strip_prefix('"')
                .and_then(|tail| tail.strip_suffix('"'))
                && !tail.is_empty()
                && !tail.contains('"')
            {
                return Some((&body[..close], tail));
            }
        }
        search = close + 1;
    }
    None
}

fn unquote(value: &str) -> &str {
    for quote in ['"', '\''] {
        if let Some(inner) = value
            .strip_prefix(quote)
            .and_then(|rest| rest.strip_suffix(quote))
        {
            return inner;
        }
    }
    value
}

fn malformed(line: usize, message: impl std::fmt::Display) -> CapabilityError {
    CapabilityError::Invalid(format!("line {line}: {message}"))
}

// ── selectors and the dumped DOM ────────────────────────────────────────────────────────────

enum Selector {
    Id(String),
    Class(String),
    Tag(String),
    Attribute { name: String, value: String },
}

impl Selector {
    /// The selector as the caller wrote it, for a failure detail.
    fn text(&self) -> String {
        match self {
            Self::Id(id) => format!("#{id}"),
            Self::Class(class) => format!(".{class}"),
            Self::Tag(tag) => tag.clone(),
            Self::Attribute { name, value } => format!("[{name}={value}]"),
        }
    }
}

/// The simple subset only: `#id`, `.class`, `tag`, `[attr=value]`. `None` is the by-name refusal.
fn parse_selector(text: &str) -> Option<Selector> {
    if let Some(id) = text.strip_prefix('#') {
        return simple_name(id).then(|| Selector::Id(id.to_string()));
    }
    if let Some(class) = text.strip_prefix('.') {
        return simple_name(class).then(|| Selector::Class(class.to_string()));
    }
    if let Some(inner) = text
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
    {
        let (name, value) = inner.split_once('=')?;
        let name = name.trim();
        let value = value.trim();
        if !attribute_name(name) {
            return None;
        }
        let value = attribute_value(value)?;
        return Some(Selector::Attribute {
            name: name.to_ascii_lowercase(),
            value: value.to_string(),
        });
    }
    if tag_name(text) {
        Some(Selector::Tag(text.to_ascii_lowercase()))
    } else {
        None
    }
}

fn simple_name(text: &str) -> bool {
    !text.is_empty()
        && text.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        })
}

fn tag_name(text: &str) -> bool {
    let mut characters = text.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '-')
}

fn attribute_name(text: &str) -> bool {
    let mut characters = text.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_' || first == ':')
        && characters.all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | ':' | '.')
        })
}

fn attribute_value(text: &str) -> Option<&str> {
    for quote in ['"', '\''] {
        if let Some(inner) = text
            .strip_prefix(quote)
            .and_then(|rest| rest.strip_suffix(quote))
        {
            return (!inner.is_empty()).then_some(inner);
        }
    }
    if text.is_empty()
        || text.chars().any(|character| {
            character.is_whitespace() || matches!(character, '"' | '\'' | ']' | '=')
        })
    {
        return None;
    }
    Some(text)
}

fn selector_matches(selector: &Selector, dom: &str) -> bool {
    elements(dom).iter().any(|element| match selector {
        Selector::Id(id) => element
            .attributes
            .iter()
            .any(|(name, value)| name == "id" && value == id),
        Selector::Class(class) => element.attributes.iter().any(|(name, value)| {
            name == "class" && value.split_whitespace().any(|token| token == class)
        }),
        Selector::Tag(tag) => element.tag == *tag,
        Selector::Attribute { name, value } => element
            .attributes
            .iter()
            .any(|(attribute, found)| attribute == name && found == value),
    })
}

/// One start tag from the dumped DOM.
struct Element {
    tag: String,
    attributes: Vec<(String, String)>,
}

/// A minimal start-tag scanner: enough to answer the selector subset without an HTML parser
/// dependency. Comments, doctypes, processing instructions and closing tags are skipped.
fn elements(dom: &str) -> Vec<Element> {
    let bytes = dom.as_bytes();
    let mut elements = Vec::new();
    let mut index = 0usize;

    while let Some(relative) = dom[index..].find('<') {
        let start = index + relative;
        let rest = &dom[start..];
        if rest.starts_with("<!--") {
            index = rest.find("-->").map_or(dom.len(), |end| start + end + 3);
            continue;
        }
        if rest.starts_with("</") || rest.starts_with("<!") || rest.starts_with("<?") {
            index = rest.find('>').map_or(dom.len(), |end| start + end + 1);
            continue;
        }

        let name_start = start + 1;
        let mut cursor = name_start;
        while cursor < bytes.len()
            && !bytes[cursor].is_ascii_whitespace()
            && !matches!(bytes[cursor], b'>' | b'/')
        {
            cursor += 1;
        }
        if cursor == name_start {
            // A stray `<`: step past it so the scan always progresses.
            index = name_start;
            continue;
        }
        let tag = dom[name_start..cursor].to_ascii_lowercase();
        let mut attributes: Vec<(String, String)> = Vec::new();

        loop {
            while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
                cursor += 1;
            }
            if cursor >= bytes.len() {
                break;
            }
            match bytes[cursor] {
                b'>' => {
                    cursor += 1;
                    break;
                }
                b'/' => {
                    cursor += 1;
                    continue;
                }
                _ => {}
            }

            let name_start = cursor;
            while cursor < bytes.len()
                && !bytes[cursor].is_ascii_whitespace()
                && !matches!(bytes[cursor], b'=' | b'>' | b'/')
            {
                cursor += 1;
            }
            if cursor == name_start {
                cursor += 1;
                continue;
            }
            let name = dom[name_start..cursor].to_ascii_lowercase();
            while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
                cursor += 1;
            }

            let mut value = String::new();
            if cursor < bytes.len() && bytes[cursor] == b'=' {
                cursor += 1;
                while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
                    cursor += 1;
                }
                if cursor < bytes.len() && matches!(bytes[cursor], b'"' | b'\'') {
                    let quote = bytes[cursor];
                    cursor += 1;
                    let value_start = cursor;
                    while cursor < bytes.len() && bytes[cursor] != quote {
                        cursor += 1;
                    }
                    value.push_str(&dom[value_start..cursor]);
                    if cursor < bytes.len() {
                        cursor += 1;
                    }
                } else {
                    let value_start = cursor;
                    while cursor < bytes.len()
                        && !bytes[cursor].is_ascii_whitespace()
                        && !matches!(bytes[cursor], b'>' | b'/')
                    {
                        cursor += 1;
                    }
                    value.push_str(&dom[value_start..cursor]);
                }
            }
            attributes.push((name, value));
        }

        elements.push(Element { tag, attributes });
        index = cursor;
    }
    elements
}

/// The document's own `<title>` text.
fn page_title(dom: &str) -> Option<String> {
    let lower = dom.to_ascii_lowercase();
    let start = lower.find("<title")?;
    let open = start + lower[start..].find('>')? + 1;
    let close = open + lower[open..].find("</title")?;
    Some(dom[open..close].trim().to_string())
}

// ── the browser ────────────────────────────────────────────────────────────────────────────

/// The image's own Chromium, one fresh profile per launch.
struct Browser {
    binary: String,
    timeout: Duration,
    profiles: PathBuf,
}

impl Browser {
    fn new(config: &Config, timeout_ms: u64) -> Result<Self, CapabilityError> {
        let binary = chrome_binary().ok_or_else(|| {
            CapabilityError::refused(
                code::BROWSER,
                "no Chromium or Chrome binary was found; set CHROME_BIN",
            )
        })?;
        std::fs::create_dir_all(&config.work_dir).map_err(|error| {
            CapabilityError::Failed(format!("cannot create {:?}: {error}", config.work_dir))
        })?;
        Ok(Self {
            binary: binary.to_string_lossy().into_owned(),
            timeout: Duration::from_millis(timeout_ms),
            profiles: config.work_dir.join("gherkin-profiles"),
        })
    }

    /// A profile directory no other launch uses: two scenarios never share browser state.
    fn next_profile(&self) -> Result<PathBuf, CapabilityError> {
        static NEXT_PROFILE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        std::fs::create_dir_all(&self.profiles).map_err(|error| {
            CapabilityError::Failed(format!("cannot create {:?}: {error}", self.profiles))
        })?;
        Ok(self.profiles.join(format!(
            "{}-{}",
            std::process::id(),
            NEXT_PROFILE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        )))
    }

    /// The flags the CDP session adds to its fixed headless set: a fixed viewport, so a click's
    /// viewport coordinates mean the same thing in every run.
    fn headless_args(&self) -> Vec<String> {
        vec![format!(
            "--window-size={SCREENSHOT_WIDTH},{SCREENSHOT_HEIGHT}"
        )]
    }

    async fn screenshot(&self, url: &str, output: &Path) -> Result<(), CapabilityError> {
        let finished = self
            .launch(
                vec![
                    format!("--window-size={SCREENSHOT_WIDTH},{SCREENSHOT_HEIGHT}"),
                    format!("--screenshot={}", output.display()),
                ],
                url,
            )
            .await?;
        if !finished.ok() {
            return Err(CapabilityError::Failed(format!(
                "the browser failed to screenshot {url:?}: {}",
                finished.tail()
            )));
        }
        let bytes = std::fs::read(output).map_err(|error| {
            CapabilityError::Failed(format!(
                "the browser reported success but wrote no screenshot at {output:?}: {error}"
            ))
        })?;
        if bytes.is_empty() {
            return Err(CapabilityError::Failed(format!(
                "the browser wrote an empty screenshot at {output:?}"
            )));
        }
        Ok(())
    }

    async fn launch(
        &self,
        extra: Vec<String>,
        url: &str,
    ) -> Result<process::Finished, CapabilityError> {
        // A profile per invocation is what keeps two Chromium launches independent; it also means
        // a scenario never inherits state from the one before it (the `render.rs` measurement).
        let profile = self.next_profile()?;
        let mut args: Vec<String> = vec![
            "--headless=new".to_string(),
            "--disable-gpu".to_string(),
            "--no-sandbox".to_string(),
            "--no-first-run".to_string(),
            "--disable-extensions".to_string(),
            "--hide-scrollbars".to_string(),
            "--disable-dev-shm-usage".to_string(),
            format!("--user-data-dir={}", profile.display()),
            // `--virtual-time-budget=0` never expires (the trap `render.rs` records), so the
            // per-test timeout is also the page's settling budget.
            format!("--virtual-time-budget={}", self.timeout.as_millis()),
        ];
        args.extend(extra);
        args.push(url.to_string());
        let finished = run_bounded(&self.binary, &args, self.timeout).await;
        // Best effort: a profile that cannot be removed is scratch bytes in the job's emptyDir,
        // never a reason to fail a run that already produced its verdict.
        let _ = std::fs::remove_dir_all(&profile);
        finished
    }
}

/// The browser this process drives: `CHROME_BIN`, then the image's Chromium, then a developer
/// box's stock Chrome. A configured path that does not exist is not a browser; it falls through
/// to the candidates rather than failing the launch. `None` is the "no browser is installed"
/// refusal.
fn chrome_binary() -> Option<PathBuf> {
    std::env::var_os(CHROME_ENV)
        .map(PathBuf::from)
        .into_iter()
        .chain([
            PathBuf::from(CHROME_DEFAULT),
            PathBuf::from(CHROME_DEVELOPER),
        ])
        .find(|candidate| candidate.is_file())
}

/// One tool invocation bounded by the request's per-test ceiling, not by [`process::TOOL_TIMEOUT`]
/// (the captured range reaches 600 s).
async fn run_bounded(
    tool: &str,
    args: &[String],
    timeout: Duration,
) -> Result<process::Finished, CapabilityError> {
    let mut command = Command::new(tool);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let child = command
        .spawn()
        .map_err(|source| CapabilityError::ToolMissing {
            tool: tool.to_string(),
            source,
        })?;
    let output = match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => {
            return Err(CapabilityError::Failed(format!(
                "{tool} could not be waited on: {error}"
            )));
        }
        Err(_) => {
            return Err(CapabilityError::Failed(format!(
                "the browser exceeded its {} ms per-test ceiling",
                timeout.as_millis()
            )));
        }
    };
    Ok(process::Finished {
        status: output.status,
        stdout: output.stdout,
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

// ── executing a scenario ───────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
enum Status {
    Passed,
    Failed,
}

#[derive(serde::Serialize)]
struct StepResult {
    text: String,
    status: Status,
    detail: String,
}

#[derive(serde::Serialize)]
struct ScenarioResult {
    name: String,
    status: Status,
    steps: Vec<StepResult>,
    duration_ms: u64,
    /// The trace file's name in `out_dir`, when a failed scenario wrote one.
    #[serde(skip_serializing_if = "Option::is_none")]
    trace: Option<String>,
    /// The last URL the scenario loaded, so a failure screenshot shows what the test saw.
    #[serde(skip)]
    last_url: String,
}

impl ScenarioResult {
    /// The scenario's own ceiling fired. Every step is reported failed: the browser was killed
    /// mid-run, so no step's outcome is a verdict.
    fn timed_out(scenario: &Scenario, target_url: &str, timeout: Duration) -> Self {
        let detail = format!(
            "the scenario exceeded its {} ms per-test ceiling",
            timeout.as_millis()
        );
        Self {
            name: scenario.name.clone(),
            status: Status::Failed,
            steps: scenario
                .steps
                .iter()
                .map(|step| StepResult {
                    text: step.text.clone(),
                    status: Status::Failed,
                    detail: detail.clone(),
                })
                .collect(),
            duration_ms: timeout.as_millis() as u64,
            trace: None,
            last_url: target_url.to_string(),
        }
    }
}

/// The trace document: the step list with each step's status, and the page the scenario ended on.
fn write_trace(output: &Path, result: &ScenarioResult) -> Result<(), String> {
    let document = json!({
        "scenario": &result.name,
        "status": result.status,
        "url": &result.last_url,
        "steps": &result.steps,
    });
    let body = serde_json::to_string_pretty(&document).map_err(|error| error.to_string())?;
    std::fs::write(output, body).map_err(|error| format!("{}: {error}", output.display()))
}

/// A failed scenario is re-run `retries` more times; the final attempt is the reported verdict.
async fn run_with_retries(
    scenario: &Scenario,
    target_url: &str,
    browser: &Browser,
    retries: u64,
) -> ScenarioResult {
    let mut attempt = 0;
    loop {
        // The scenario is the browser run now, so the request's per-test ceiling bounds the whole
        // attempt. Dropping the future drops the session, which kills the child.
        let result = match tokio::time::timeout(
            browser.timeout,
            run_scenario(scenario, target_url, browser),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => ScenarioResult::timed_out(scenario, target_url, browser.timeout),
        };
        if result.status == Status::Passed || attempt >= retries {
            return result;
        }
        attempt += 1;
    }
}

async fn run_scenario(scenario: &Scenario, target_url: &str, browser: &Browser) -> ScenarioResult {
    let started = Instant::now();
    let mut steps = Vec::with_capacity(scenario.steps.len());
    let mut page = Page::new(target_url);

    for step in &scenario.steps {
        let result = match &step.action {
            Action::Open(url) => match page.navigate(browser, url).await {
                Ok(()) => StepResult {
                    text: step.text.clone(),
                    status: Status::Passed,
                    detail: format!("loaded {url}"),
                },
                Err(error) => StepResult {
                    text: step.text.clone(),
                    status: Status::Failed,
                    detail: load_failure(&error),
                },
            },
            Action::Click(selector) => {
                let outcome = page.click(browser, selector).await;
                interaction(&step.text, format!("clicked {selector:?}"), outcome)
            }
            Action::Type { selector, text } => {
                let outcome = page.type_text(browser, selector, text).await;
                // Never echo the typed text: a real feature types passwords through this step.
                interaction(
                    &step.text,
                    format!(
                        "typed {} characters into {selector:?}",
                        text.chars().count()
                    ),
                    outcome,
                )
            }
            Action::Scroll(dy) => {
                let outcome = page.scroll(browser, *dy).await;
                interaction(&step.text, format!("scrolled by {dy} px"), outcome)
            }
            Action::Hover(selector) => {
                let outcome = page.hover(browser, selector).await;
                interaction(&step.text, format!("hovered {selector:?}"), outcome)
            }
            Action::Upload { selector, path } => {
                let outcome = page.upload(browser, selector, path).await;
                interaction(
                    &step.text,
                    format!("uploaded {path:?} into {selector:?}"),
                    outcome,
                )
            }
            Action::Assert(assertion) => match page.dom(browser).await {
                Ok(dom) => evaluate(&step.text, assertion, dom),
                Err(error) => StepResult {
                    text: step.text.clone(),
                    status: Status::Failed,
                    detail: load_failure(&error),
                },
            },
        };
        steps.push(result);
    }

    let failed = steps.iter().any(|step| step.status == Status::Failed);
    ScenarioResult {
        name: scenario.name.clone(),
        status: if failed {
            Status::Failed
        } else {
            Status::Passed
        },
        steps,
        duration_ms: started.elapsed().as_millis() as u64,
        trace: None,
        last_url: page.target_url,
    }
}

fn interaction(text: &str, detail: String, outcome: Result<(), CapabilityError>) -> StepResult {
    match outcome {
        Ok(()) => StepResult {
            text: text.to_string(),
            status: Status::Passed,
            detail,
        },
        Err(error) => StepResult {
            text: text.to_string(),
            status: Status::Failed,
            detail: error.to_string(),
        },
    }
}

// ── the page under test ────────────────────────────────────────────────────────────────────

/// One scenario's browser session and the page state its assertions read.
///
/// The session is launched lazily, on the first step that needs it, and the first such step also
/// loads `target_url` unless a navigation step already named another page. A launch or load
/// failure is cached: later steps report it instead of paying for the same failed attempt once
/// per assertion.
struct Page {
    live: Option<Live>,
    /// Why the browser could not be started; cleared by a new navigation step.
    unavailable: Option<String>,
    /// The page the scenario last asked for; `target_url` until a navigation step names another.
    target_url: String,
    loaded: bool,
    /// The reason the current page could not be loaded; cleared by a new navigation step.
    load_error: Option<String>,
    /// The dump taken after the last step that changed the page.
    dom: Option<String>,
}

impl Page {
    fn new(target_url: &str) -> Self {
        Self {
            live: None,
            unavailable: None,
            target_url: target_url.to_string(),
            loaded: false,
            load_error: None,
            dom: None,
        }
    }

    async fn live(&mut self, browser: &Browser) -> Result<&mut Live, CapabilityError> {
        if let Some(reason) = &self.unavailable {
            return Err(CapabilityError::Failed(reason.clone()));
        }
        if self.live.is_none() {
            match Live::launch(browser).await {
                Ok(live) => self.live = Some(live),
                Err(error) => {
                    self.unavailable = Some(error.to_string());
                    return Err(error);
                }
            }
        }
        Ok(self.live.as_mut().expect("the session was just launched"))
    }

    async fn navigate(&mut self, browser: &Browser, url: &str) -> Result<(), CapabilityError> {
        self.target_url = url.to_string();
        self.loaded = true;
        self.load_error = None;
        self.dom = None;
        self.unavailable = None;
        let outcome = match self.live(browser).await {
            Ok(live) => live.session.navigate(url).await,
            Err(error) => Err(error),
        };
        self.load_error = outcome.as_ref().err().map(|error| error.to_string());
        outcome
    }

    async fn ensure_loaded(&mut self, browser: &Browser) -> Result<(), CapabilityError> {
        if let Some(reason) = &self.load_error {
            return Err(CapabilityError::Failed(reason.clone()));
        }
        if !self.loaded {
            let url = self.target_url.clone();
            self.navigate(browser, &url).await?;
        }
        Ok(())
    }

    /// The live DOM for this batch. A dump is taken once per change; every assertion after the
    /// same change reads it, which is what makes the outcome independent of how many assertions a
    /// scenario writes.
    async fn dom(&mut self, browser: &Browser) -> Result<&str, CapabilityError> {
        if self.dom.is_none() {
            self.ensure_loaded(browser).await?;
            let dump = {
                let live = self.live(browser).await?;
                live.session.dump_dom().await?
            };
            self.dom = Some(dump);
        }
        Ok(self.dom.as_deref().expect("the dump was just taken"))
    }

    async fn click(&mut self, browser: &Browser, selector: &str) -> Result<(), CapabilityError> {
        self.ensure_loaded(browser).await?;
        self.dom = None;
        self.live(browser).await?.session.click(selector).await
    }

    async fn type_text(
        &mut self,
        browser: &Browser,
        selector: &str,
        text: &str,
    ) -> Result<(), CapabilityError> {
        self.ensure_loaded(browser).await?;
        self.dom = None;
        self.live(browser)
            .await?
            .session
            .type_text(selector, text)
            .await
    }

    async fn scroll(&mut self, browser: &Browser, dy: i64) -> Result<(), CapabilityError> {
        self.ensure_loaded(browser).await?;
        self.dom = None;
        self.live(browser).await?.session.scroll(dy).await
    }

    async fn hover(&mut self, browser: &Browser, selector: &str) -> Result<(), CapabilityError> {
        self.ensure_loaded(browser).await?;
        self.dom = None;
        self.live(browser).await?.session.hover(selector).await
    }

    async fn upload(
        &mut self,
        browser: &Browser,
        selector: &str,
        path: &str,
    ) -> Result<(), CapabilityError> {
        self.ensure_loaded(browser).await?;
        self.dom = None;
        self.live(browser)
            .await?
            .session
            .upload(selector, Path::new(path))
            .await
    }
}

/// One scenario's browser: the child process, its CDP session, and the profile to remove.
struct Live {
    child: Child,
    session: Session,
    profile: PathBuf,
}

impl Live {
    async fn launch(browser: &Browser) -> Result<Self, CapabilityError> {
        let profile = browser.next_profile()?;
        let (child, session) =
            Session::launch(&browser.binary, &profile, &browser.headless_args()).await?;
        Ok(Self {
            child,
            session,
            profile,
        })
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
        // Best effort: a profile that cannot be removed is scratch bytes in the job's emptyDir,
        // never a reason to fail a run that already produced its verdict.
        let _ = std::fs::remove_dir_all(&self.profile);
    }
}

fn load_failure(error: &CapabilityError) -> String {
    format!("the page could not be loaded: {error}")
}

fn evaluate(text: &str, assertion: &Assertion, dom: &str) -> StepResult {
    let failure = match assertion {
        Assertion::See(want) => {
            (!dom.contains(want.as_str())).then(|| format!("the page does not contain {want:?}"))
        }
        Assertion::NotSee(want) => dom
            .contains(want.as_str())
            .then(|| format!("the page contains {want:?}")),
        Assertion::Title(want) => match page_title(dom) {
            Some(found) if found == *want => None,
            Some(found) => Some(format!("expected title {want:?}, found {found:?}")),
            None => Some(format!("the page has no <title>; expected {want:?}")),
        },
        Assertion::Has(selector) => (!selector_matches(selector, dom))
            .then(|| format!("no element matches {}", selector.text())),
        Assertion::HasNot(selector) => selector_matches(selector, dom)
            .then(|| format!("an element matches {}", selector.text())),
    };
    match failure {
        None => StepResult {
            text: text.to_string(),
            status: Status::Passed,
            detail: "ok".to_string(),
        },
        Some(detail) => StepResult {
            text: text.to_string(),
            status: Status::Failed,
            detail,
        },
    }
}

/// A safe file stem for a failure screenshot: lowercase alphanumerics and single dashes, so a
/// scenario name can never become a path.
fn slug(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            out.push(character.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "scenario".to_string()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    const FIXTURE: &str = r#"<!doctype html>
<html>
  <head><title>Media fixture</title></head>
  <body>
    <h1 id="headline" class="hero big">Hello conveyor</h1>
    <p data-testid="note">A local page.</p>
  </body>
</html>
"#;

    /// The expected word is built at click time (`['cl', 'icked']`), so a passing assertion can
    /// only read the live DOM: the page's own source never contains it.
    const CLICK_PAGE: &str = r#"<!doctype html>
<html>
  <head><title>Click fixture</title></head>
  <body>
    <button id="go">Go</button>
    <p id="out">idle</p>
    <script>
      document.getElementById('go').addEventListener('click', () => {
        document.getElementById('out').textContent = ['cl', 'icked'].join('');
      });
    </script>
  </body>
</html>
"#;

    /// Every observable word is assembled in the page, never written contiguously in the source.
    const INTERACTION_PAGE: &str = r#"<!doctype html>
<html>
  <head><title>Interaction fixture</title></head>
  <body>
    <input id="name">
    <p id="typed">typed: none</p>
    <div id="hover-target">hover target</div>
    <p id="hover">waiting</p>
    <input id="file" type="file">
    <p id="files">files: 0</p>
    <p id="scroll">scrolled: no</p>
    <div style="height: 3000px"></div>
    <script>
      const name = document.getElementById('name');
      name.addEventListener('input', () => {
        document.getElementById('typed').textContent = 'typed: ' + name.value;
      });
      document.getElementById('hover-target').addEventListener('mouseenter', () => {
        document.getElementById('hover').textContent = 'hover' + 'ed';
      });
      document.getElementById('file').addEventListener('change', (event) => {
        document.getElementById('files').textContent = 'files: ' + event.target.files.length;
      });
      window.addEventListener('scroll', () => {
        document.getElementById('scroll').textContent =
          'scrolled: ' + (window.scrollY > 0 ? 'yes' : 'no');
      });
    </script>
  </body>
</html>
"#;

    fn scratch(tag: &str) -> Config {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or_default();
        let base = std::env::temp_dir().join(format!(
            "conveyor-media-gherkin-{tag}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(base.join("work")).expect("create the work directory");
        std::fs::create_dir_all(base.join("out")).expect("create the out directory");
        Config {
            service: "conveyor-media-test".to_string(),
            work_dir: base.join("work"),
            out_dir: base.join("out"),
        }
    }

    #[tokio::test]
    async fn a_two_scenario_feature_runs_against_a_local_page() {
        // The only case that needs a browser; every refusal below is decided before one starts.
        if chrome_binary().is_none() {
            return;
        }
        let config = scratch("two-scenarios");
        let page = config.work_dir.join("page.html");
        std::fs::write(&page, FIXTURE).expect("write the fixture page");
        let target = format!("file://{}", page.display());
        let feature = format!(
            "@smoke\n# the page under test\nFeature: the local page\n\n  Scenario: the page \
             loads\n    Given I open \"{target}\"\n    Then I expect to see \"Hello conveyor\"\n    \
             And the page contains \"A local page.\"\n    And I expect the title to be \"Media \
             fixture\"\n    And I expect the page to have a \"#headline\" element\n    And I \
             expect the page to have a \"[data-testid=note]\" element\n    And I expect the page \
             to have a \".hero\" element\n\n  Scenario: a missing heading\n    When I visit \
             \"{target}\"\n    Then I expect to see \"Goodbye conveyor\"\n"
        );
        let params = json!({
            "feature": feature,
            "target_url": target.clone(),
            "timeout_per_test_ms": 60000,
            "screenshot_on_failure": true,
        });

        let result = run(&params, &config).await.expect("the run completes");

        assert_eq!(result["feature"], json!("the local page"));
        assert_eq!(result["target_url"], json!(target));
        assert_eq!(result["passed"], json!(1));
        assert_eq!(result["failed"], json!(1));
        assert_eq!(result["exit_status"], json!(1));
        assert_eq!(result["scenarios"][0]["name"], json!("the page loads"));
        assert_eq!(result["scenarios"][0]["status"], json!("passed"));
        assert_eq!(result["scenarios"][1]["name"], json!("a missing heading"));
        assert_eq!(result["scenarios"][1]["status"], json!("failed"));
        let detail = result["scenarios"][1]["steps"][1]["detail"]
            .as_str()
            .expect("the failing step has a detail");
        assert!(detail.contains("Goodbye conveyor"), "{detail}");
        assert!(
            config.out_dir.join("a-missing-heading.png").is_file(),
            "a failed scenario writes its screenshot"
        );
        assert!(
            !config.out_dir.join("the-page-loads.png").exists(),
            "a passing scenario writes no screenshot"
        );
    }

    #[tokio::test]
    async fn an_unsupported_step_is_refused_by_name() {
        let feature = "Feature: interactions\n  Scenario: a drag\n    When I drag \"#card\" onto \
                       \"#done\"\n";
        let params = json!({
            "feature": feature,
            "target_url": "file:///tmp/page.html",
        });
        let error = run(&params, &scratch("drag")).await.unwrap_err();
        assert!(error.to_string().contains("step_not_supported"), "{error}");
        assert!(
            error
                .to_string()
                .contains("I drag \"#card\" onto \"#done\""),
            "{error}"
        );
    }

    #[tokio::test]
    async fn an_http_target_without_allow_network_is_refused_by_name() {
        let feature = "Feature: remote\n  Scenario: loads\n    Then I expect to see \"Hello\"\n";
        let params = json!({
            "feature": feature,
            "target_url": "https://example.com/",
        });
        let error = run(&params, &scratch("egress")).await.unwrap_err();
        assert!(error.to_string().contains("egress_denied"), "{error}");
        assert!(error.to_string().contains("target_url"), "{error}");
    }

    #[tokio::test]
    async fn a_compound_selector_is_refused_by_name() {
        let config = scratch("selector");
        for selector in [
            "div > p",
            "article:has(h1)",
            "h1, h2",
            "li:nth-child(2)",
            "div p",
        ] {
            let feature = format!(
                "Feature: selectors\n  Scenario: one\n    Then I expect the page to have a \
                 \"{selector}\" element\n"
            );
            let params = json!({
                "feature": feature,
                "target_url": "file:///tmp/page.html",
            });
            let error = run(&params, &config).await.unwrap_err();
            assert!(
                error.to_string().contains("selector_not_supported"),
                "{selector}: {error}"
            );
            assert!(error.to_string().contains(selector), "{selector}: {error}");
        }
    }

    #[tokio::test]
    async fn a_malformed_feature_names_the_line() {
        let feature = "Feature: broken\n  Scenario: fine\n    Then I expect to see \"x\"\n  \
                       Scenario Outline: not supported\n    Then I expect to see \"y\"\n";
        let params = json!({
            "feature": feature,
            "target_url": "file:///tmp/page.html",
        });
        let error = run(&params, &scratch("malformed")).await.unwrap_err();
        assert!(error.to_string().contains("line 4"), "{error}");
    }

    #[tokio::test]
    async fn background_is_refused_by_name() {
        let feature = "Feature: background\n  Background:\n    Given I open \
                       \"file:///tmp/page.html\"\n  Scenario: one\n    Then I expect to see \
                       \"x\"\n";
        let params = json!({
            "feature": feature,
            "target_url": "file:///tmp/page.html",
        });
        let error = run(&params, &scratch("background")).await.unwrap_err();
        assert!(
            error.to_string().contains("background_not_supported"),
            "{error}"
        );
    }

    #[tokio::test]
    async fn a_click_step_changes_the_page_state() {
        if chrome_binary().is_none() {
            return;
        }
        let config = scratch("click-step");
        let page = config.work_dir.join("click.html");
        std::fs::write(&page, CLICK_PAGE).expect("write the click page");
        let target = format!("file://{}", page.display());
        let feature = format!(
            "Feature: click\n  Scenario: the button changes the page\n    Given I open \
             \"{target}\"\n    When I click \"#go\"\n    Then I expect to see \"clicked\"\n"
        );
        let params = json!({
            "feature": feature,
            "target_url": target,
            "timeout_per_test_ms": 60000,
        });

        let result = run(&params, &config).await.expect("the run completes");

        assert_eq!(result["passed"], json!(1), "{result}");
        assert_eq!(
            result["scenarios"][0]["steps"][1]["status"],
            json!("passed"),
            "the click itself passes: {result}"
        );
        assert_eq!(
            result["scenarios"][0]["steps"][2]["status"],
            json!("passed"),
            "the assertion sees the click's effect: {result}"
        );
    }

    #[tokio::test]
    async fn the_interaction_vocabulary_runs_over_cdp() {
        if chrome_binary().is_none() {
            return;
        }
        let config = scratch("interactions");
        let page = config.work_dir.join("interactions.html");
        std::fs::write(&page, INTERACTION_PAGE).expect("write the interaction page");
        let upload = config.work_dir.join("upload.txt");
        std::fs::write(&upload, "conveyor").expect("write the upload file");
        let target = format!("file://{}", page.display());
        let feature = format!(
            "Feature: interactions\n  Scenario: every interaction reaches the page\n    Given I \
             open \"{target}\"\n    When I type \"conveyor\" into \"#name\"\n    And I hover \
             \"#hover-target\"\n    And I upload \"{upload}\" into \"#file\"\n    And I scroll \
             down 400\n    Then I expect to see \"typed: conveyor\"\n    And I expect to see \
             \"hovered\"\n    And I expect to see \"files: 1\"\n    And I expect to see \
             \"scrolled: yes\"\n",
            upload = upload.display(),
        );
        let params = json!({
            "feature": feature,
            "target_url": target,
            "timeout_per_test_ms": 60000,
        });

        let result = run(&params, &config).await.expect("the run completes");

        assert_eq!(result["passed"], json!(1), "{result}");
        let steps = result["scenarios"][0]["steps"]
            .as_array()
            .expect("the scenario reports its steps");
        for (index, step) in steps.iter().enumerate() {
            assert_eq!(step["status"], json!("passed"), "step {index}: {step}");
        }
    }

    #[tokio::test]
    async fn a_failed_scenario_writes_its_trace() {
        if chrome_binary().is_none() {
            return;
        }
        let config = scratch("trace");
        let page = config.work_dir.join("page.html");
        std::fs::write(&page, FIXTURE).expect("write the fixture page");
        let target = format!("file://{}", page.display());
        let feature = format!(
            "Feature: trace\n  Scenario: a failing assertion\n    Given I open \"{target}\"\n    \
             Then I expect to see \"Goodbye conveyor\"\n"
        );
        let params = json!({
            "feature": feature,
            "target_url": target,
            "timeout_per_test_ms": 60000,
            "trace_on_failure": true,
        });

        let result = run(&params, &config).await.expect("the run completes");

        assert_eq!(result["failed"], json!(1), "{result}");
        let name = result["scenarios"][0]["trace"]
            .as_str()
            .expect("the failed scenario names its trace");
        assert_eq!(name, "a-failing-assertion.json");
        let raw = std::fs::read_to_string(config.out_dir.join(name)).expect("the trace is written");
        let trace: Value = serde_json::from_str(&raw).expect("the trace is JSON");
        assert_eq!(trace["url"], json!(target), "{trace}");
        assert_eq!(trace["steps"][0]["status"], json!("passed"), "{trace}");
        assert_eq!(trace["steps"][1]["status"], json!("failed"), "{trace}");
        assert!(
            trace["steps"][1]["detail"]
                .as_str()
                .unwrap_or_default()
                .contains("Goodbye conveyor"),
            "{trace}"
        );
    }

    #[tokio::test]
    async fn a_selector_that_matches_nothing_names_the_step() {
        if chrome_binary().is_none() {
            return;
        }
        let config = scratch("missing-selector");
        let page = config.work_dir.join("page.html");
        std::fs::write(&page, FIXTURE).expect("write the fixture page");
        let target = format!("file://{}", page.display());
        let feature = format!(
            "Feature: a missing selector\n  Scenario: the click finds nothing\n    Given I open \
             \"{target}\"\n    When I click \"#no-such-element\"\n    Then I expect to see \
             \"Hello conveyor\"\n"
        );
        let params = json!({
            "feature": feature,
            "target_url": target,
            "timeout_per_test_ms": 60000,
        });

        let result = run(&params, &config).await.expect("the run completes");

        assert_eq!(result["failed"], json!(1), "{result}");
        let detail = result["scenarios"][0]["steps"][1]["detail"]
            .as_str()
            .expect("the failing click has a detail");
        assert!(detail.contains("click"), "{detail}");
        assert!(detail.contains("#no-such-element"), "{detail}");
        assert_eq!(
            result["scenarios"][0]["steps"][2]["status"],
            json!("passed"),
            "the page is still live after the missing selector: {result}"
        );
    }
}
