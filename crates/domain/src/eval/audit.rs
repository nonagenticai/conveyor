#![forbid(unsafe_code)]
//! The eval health audit's mechanical half (S19 Part C step 14; SPEC D25).
//!
//! The `claude-api` `eval-audit` method is a checklist in six sections. Half of it can be read
//! off the instrument's own rows — duplicates, gold provenance, tag balance, split state,
//! truncation and failure classes, whether cost is measurable, whether the controls hold — and
//! that half is here, pure, so an audit of an eval is reproducible without a model in the loop.
//! The probes that need the instrument to *run* (the oracle, the null, the judge's three known
//! negatives) are computed by the worker and passed in as [`Controls`]; this module only turns
//! the resulting facts into findings.
//!
//! **A finding is a suggestion, never a verdict.** The section, severity and evidence are what a
//! person reads; nothing here changes the instrument, and no finding is ever applied
//! automatically (D25 — the audit proposes, a person disposes).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// The checklist section a finding cites (`1` task design … `6` reporting).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Section {
    /// §1 Task design — the cases themselves.
    Task,
    /// §2 Harness design — the scaffolding around the model call.
    Harness,
    /// §3 Metrics hygiene — cost and latency measured, not estimated.
    Metrics,
    /// §4 Grader design — the scoring function.
    Grader,
    /// §5 Can it detect the change you're after — resolution.
    Detectability,
    /// §6 Reporting findings to the user.
    Reporting,
}

impl Section {
    /// The stored code (`eval_audit_finding.section`).
    pub fn as_str(self) -> &'static str {
        match self {
            Section::Task => "1",
            Section::Harness => "2",
            Section::Metrics => "3",
            Section::Grader => "4",
            Section::Detectability => "5",
            Section::Reporting => "6",
        }
    }

    /// Every section, in order — the audit always reports all six, even when a section is clear.
    pub fn all() -> [Section; 6] {
        [
            Section::Task,
            Section::Harness,
            Section::Metrics,
            Section::Grader,
            Section::Detectability,
            Section::Reporting,
        ]
    }
}

/// How much a finding matters (`eval_audit_finding.severity`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    /// The numbers are likely actively misleading.
    Misleading,
    /// Adds noise or limits generality without flipping a conclusion.
    Noise,
    /// The eval generalises less than its author may think.
    Generality,
}

impl Severity {
    /// The stored code.
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Misleading => "misleading",
            Severity::Noise => "noise",
            Severity::Generality => "generality",
        }
    }
}

/// One finding, before it is stored.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FindingDraft {
    /// The checklist section.
    pub section: Section,
    /// The check's stable name, e.g. `duplicate_cases`.
    pub item: String,
    /// How much it matters.
    pub severity: Severity,
    /// What was observed, with the case ids or counts.
    pub evidence: String,
    /// One concrete change worth considering.
    pub suggestion: String,
}

impl FindingDraft {
    fn new(
        section: Section,
        item: &str,
        severity: Severity,
        evidence: String,
        suggestion: &str,
    ) -> Self {
        Self {
            section,
            item: item.to_owned(),
            severity,
            evidence,
            suggestion: suggestion.to_owned(),
        }
    }
}

/// One case, as the checks read it (never its answer key).
#[derive(Clone, Debug)]
pub struct CaseFacts {
    /// The case row id.
    pub id: Uuid,
    /// Its stable position.
    pub ordinal: i32,
    /// The input text, for the duplicate check.
    pub input: String,
    /// The input text's length in characters.
    pub input_len: usize,
    /// The first tag, when the case has one.
    pub tag: Option<String>,
    /// `train | test | none`.
    pub split: String,
    /// `human_written | human_verified | model`.
    pub gold_provenance: String,
    /// The model behind a `model`-provenance key.
    pub gold_model: Option<String>,
}

/// One declared metric.
#[derive(Clone, Debug)]
pub struct MetricFacts {
    /// The report's column key.
    pub key: String,
    /// `quality | perf`.
    pub role: String,
    /// `binary | judge | float`.
    pub kind: String,
    /// `higher | lower`.
    pub better: String,
    /// `programmatic | pairwise | rubric | human | none`.
    pub grader_kind: String,
    /// The grader's configuration.
    pub grader: Value,
    /// The rubric text, when the metric has one.
    pub rubric: Option<String>,
}

/// One run's measured facts, when an audit names a target run.
#[derive(Clone, Debug)]
pub struct RunFacts {
    /// Repetitions per case.
    pub reps: i32,
    /// Slots that produced a scorable result.
    pub ok: usize,
    /// Slots the runner cut at `max_tokens`.
    pub truncated: usize,
    /// Failed attempts (append-only rows, not slots).
    pub failed: usize,
    /// Cases that contributed at least one scorable value to the headline metric.
    pub scored_cases: usize,
}

/// The instrument, as the mechanical checks read it.
#[derive(Clone, Debug)]
pub struct InstrumentFacts<'a> {
    /// The subject model, when the eval declares one.
    pub model_ref: Option<&'a str>,
    /// Whether the version's split is locked.
    pub split_locked: bool,
    /// The version's price document.
    pub prices: &'a Value,
    /// Every case in the version.
    pub cases: &'a [CaseFacts],
    /// Every declared metric.
    pub metrics: &'a [MetricFacts],
    /// The target run, when the audit names one.
    pub run: Option<RunFacts>,
}

/// What the controls measured. `None` means the control could not run for this instrument.
#[derive(Clone, Debug, Default)]
pub struct Controls {
    /// The oracle score: the answer key graded by the instrument's own programmatic graders.
    pub oracle: Option<f64>,
    /// The null score: an empty answer graded the same way.
    pub null: Option<f64>,
    /// How many of the judge's three known negatives were run.
    pub judge_checked: usize,
    /// How many of them the judge failed to reject (a higher score than 0.5 is a failure to reject).
    pub judge_accepted: usize,
    /// One line per judge that accepted a known-bad answer.
    pub judge_problems: Vec<String>,
}

/// The smallest improvement an audit can honestly call measurable.
///
/// The method's line is noise floor vs headroom vs the smallest delta the owner would ship; the
/// instrument does not know that last number, so the audit states the two it measured and says
/// the delta is not declared — never invents one.
pub fn resolution_line(noise_floor: Option<f64>, headroom: Option<f64>) -> String {
    match (noise_floor, headroom) {
        (Some(floor), Some(headroom)) => format!(
            "noise floor {floor:.3} vs headroom {headroom:.3}; the smallest delta worth shipping \
             is not declared on the instrument, so the floor is the bar a change must clear"
        ),
        (Some(floor), None) => format!(
            "noise floor {floor:.3}; headroom is not measurable on this metric (no bounded ceiling)"
        ),
        (None, Some(headroom)) => {
            format!("headroom {headroom:.3}; no noise floor — run at least one scored case first")
        }
        (None, None) => {
            String::from("no scores yet: neither a noise floor nor a headroom can be computed")
        }
    }
}

/// Every mechanical finding the instrument supports, in section order.
pub fn findings(facts: &InstrumentFacts<'_>, controls: &Controls) -> Vec<FindingDraft> {
    let mut out = Vec::new();
    let cases = facts.cases;

    // ── §1 task design ──────────────────────────────────────────────────────────────────
    let mut seen: std::collections::BTreeMap<&str, Vec<i32>> = std::collections::BTreeMap::new();
    for case in cases {
        seen.entry(case.input.as_str())
            .or_default()
            .push(case.ordinal);
    }
    let duplicates: Vec<String> = seen
        .values()
        .filter(|ordinals| ordinals.len() > 1)
        .map(|ordinals| format!("ordinals {ordinals:?}"))
        .collect();
    if !duplicates.is_empty() {
        out.push(FindingDraft::new(
            Section::Task,
            "duplicate_cases",
            Severity::Noise,
            format!(
                "{} case(s) share an input byte for byte: {}",
                duplicates.len(),
                duplicates.join("; ")
            ),
            "Drop the duplicates or make the inputs distinct; a duplicated case silently doubles its weight.",
        ));
    }

    let model_gold: Vec<String> = cases
        .iter()
        .filter(|case| case.gold_provenance == "model")
        .map(|case| {
            format!(
                "#{} by {}",
                case.ordinal,
                case.gold_model.as_deref().unwrap_or("an unnamed model")
            )
        })
        .collect();
    if !model_gold.is_empty() {
        let same_model = facts.model_ref.is_some_and(|model| {
            cases
                .iter()
                .any(|case| case.gold_model.as_deref() == Some(model))
        });
        out.push(FindingDraft::new(
            Section::Task,
            "model_gold",
            if same_model {
                Severity::Misleading
            } else {
                Severity::Generality
            },
            format!(
                "{} case(s) use a model's output as the answer key: {}",
                model_gold.len(),
                model_gold.join(", ")
            ),
            "Reference-match scoring on model gold rewards imitating that model; prefer a rubric or a human-verified sample.",
        ));
    }

    let untagged = cases.iter().filter(|case| case.tag.is_none()).count();
    if untagged > 0 {
        out.push(FindingDraft::new(
            Section::Task,
            "untagged_cases",
            Severity::Noise,
            format!("{untagged} of {} case(s) carry no tag", cases.len()),
            "Tag every case: the split is stratified by `tags[0]` and the per-tag report has nowhere to put an untagged case.",
        ));
    }
    let mut tags: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    for case in cases {
        if let Some(tag) = case.tag.as_deref() {
            tags.insert(tag);
        }
    }
    if cases.len() > 1 && tags.len() <= 1 {
        out.push(FindingDraft::new(
            Section::Task,
            "single_tag",
            Severity::Generality,
            format!(
                "{} case(s) share {} tag(s)",
                cases.len(),
                tags.len()
            ),
            "Add a second tag family; a one-tag set cannot show whether a change generalises across categories.",
        ));
    }
    if cases.len() > 4 {
        let lengths: Vec<usize> = cases.iter().map(|case| case.input_len).collect();
        let shortest = lengths.iter().min().copied().unwrap_or(0);
        let longest = lengths.iter().max().copied().unwrap_or(0);
        if shortest > 0 && longest > shortest.saturating_mul(10) {
            out.push(FindingDraft::new(
                Section::Task,
                "input_length_spread",
                Severity::Noise,
                format!("input lengths run from {shortest} to {longest} characters"),
                "Check the longest cases for truncation against `max_tokens`, and the shortest for under-specification.",
            ));
        }
    }

    // ── §2 harness design ──────────────────────────────────────────────────────────────
    if let Some(run) = facts.run.as_ref() {
        if run.truncated > 0 {
            out.push(FindingDraft::new(
                Section::Harness,
                "truncated_slots",
                Severity::Noise,
                format!(
                    "{} of {} scored slot(s) were cut at `max_tokens` and excluded from every mean",
                    run.truncated, run.ok
                ),
                "Raise the output budget or shorten the cases; a clipped answer is not a wrong answer, but it is not a measurement either.",
            ));
        }
        if run.failed > 0 {
            out.push(FindingDraft::new(
                Section::Harness,
                "failed_attempts",
                Severity::Noise,
                format!("{} failed attempt(s) sit beside the slots they never filled", run.failed),
                "Read `eval_attempt_error.failure_class`; a harness or serving class concentrated in one case is a wiring bug, not a model result.",
            ));
        }
        if run.reps < 2 {
            out.push(FindingDraft::new(
                Section::Harness,
                "single_rep",
                Severity::Generality,
                format!("the run measures 1 rep per case ({} scored)", run.ok),
                "Run at least two reps; a single rep is a point estimate with no error bar, and reps are the cheapest way to shrink the noise floor.",
            ));
        }
    }

    // ── §3 metrics hygiene ─────────────────────────────────────────────────────────────
    if !facts.metrics.iter().any(|metric| metric.role == "perf") {
        out.push(FindingDraft::new(
            Section::Metrics,
            "no_perf_metric",
            Severity::Noise,
            "no `perf` metric is declared".to_owned(),
            "Declare at least one perf column (cost, latency or tokens); a pass rate alone cannot answer what quality costs.",
        ));
    }
    let priced = facts.prices.get("input").and_then(Value::as_f64).is_some()
        && facts.prices.get("output").and_then(Value::as_f64).is_some();
    if !priced {
        out.push(FindingDraft::new(
            Section::Metrics,
            "no_prices",
            Severity::Noise,
            "the version declares no input/output price rates".to_owned(),
            "Fill `eval_version.prices`; without them the report can only say how many tokens a run spent, not what it cost.",
        ));
    }

    // ── §4 grader design ───────────────────────────────────────────────────────────────
    for metric in facts.metrics {
        if (metric.grader_kind == "rubric" || metric.grader_kind == "pairwise")
            && metric
                .grader
                .get("judge_model")
                .and_then(Value::as_str)
                .is_none()
        {
            out.push(FindingDraft::new(
                Section::Grader,
                "judge_model_missing",
                Severity::Misleading,
                format!(
                    "metric `{}` is {} but declares no `grader.judge_model`",
                    metric.key, metric.grader_kind
                ),
                "Name the judge model on the metric; a run refuses `422` without it, and the digest must cover which model grades.",
            ));
        }
    }
    if let Some(oracle) = controls.oracle
        && oracle < 1.0
    {
        out.push(FindingDraft::new(
            Section::Grader,
            "oracle_below_one",
            Severity::Misleading,
            format!(
                "the answer key scores {:.3} against the instrument's own programmatic graders",
                oracle
            ),
            "Read a failing case's explanation first: either the answer key or the grader is wrong, and every subject is being measured through it.",
        ));
    }
    if let Some(null) = controls.null
        && null > 0.0
    {
        out.push(FindingDraft::new(
            Section::Grader,
            "null_above_zero",
            Severity::Misleading,
            format!("an empty answer scores {null:.3}"),
            "Tighten the grader: a no-op output that passes is the cheapest way for a model to look good without doing the work.",
        ));
    }
    if controls.judge_accepted > 0 {
        out.push(FindingDraft::new(
            Section::Grader,
            "judge_accepts_negatives",
            Severity::Misleading,
            format!(
                "{} of {} known-bad answer(s) were not rejected: {}",
                controls.judge_accepted,
                controls.judge_checked,
                controls.judge_problems.join("; ")
            ),
            "Iterate the judge prompt until an empty answer, an \"I don't know\" and a confident wrong answer all score below 0.5.",
        ));
    }

    // ── §5 resolution ──────────────────────────────────────────────────────────────────
    if !facts.split_locked {
        out.push(FindingDraft::new(
            Section::Detectability,
            "split_unlocked",
            Severity::Misleading,
            "the version has no locked split".to_owned(),
            "Draw and lock the split before the first full pass; a split chosen after seeing scores is selected on the outcome.",
        ));
    } else if !cases.iter().any(|case| case.split == "test") {
        out.push(FindingDraft::new(
            Section::Detectability,
            "no_test_split",
            Severity::Misleading,
            "the split is locked but no case is in the test half".to_owned(),
            "Re-draw the split with a non-zero test share; a held-out number is the only one a change can be judged on.",
        ));
    }

    out
}

/// Whether the audit's own numbers say the eval can see the effect it is for.
///
/// `None` when either number is missing or the floor fits inside the headroom.
pub fn resolution_shortfall(
    noise_floor: Option<f64>,
    headroom: Option<f64>,
) -> Option<FindingDraft> {
    let (floor, headroom) = (noise_floor?, headroom?);
    if floor <= headroom {
        return None;
    }
    Some(FindingDraft::new(
        Section::Detectability,
        "floor_above_headroom",
        Severity::Misleading,
        format!("the noise floor {floor:.3} exceeds the headroom {headroom:.3}"),
        "Add reps (cheapest), add cases, or move to a continuous metric; at this size a real improvement cannot clear the noise.",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case(ordinal: i32, input: &str, tag: Option<&str>) -> CaseFacts {
        CaseFacts {
            id: Uuid::now_v7(),
            ordinal,
            input: input.to_owned(),
            input_len: input.chars().count(),
            tag: tag.map(str::to_owned),
            split: "train".to_owned(),
            gold_provenance: "human_written".to_owned(),
            gold_model: None,
        }
    }

    fn prices() -> &'static Value {
        static PRICES: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        PRICES.get_or_init(|| serde_json::json!({"input": 1.0, "output": 2.0}))
    }

    fn facts<'a>(cases: &'a [CaseFacts], metrics: &'a [MetricFacts]) -> InstrumentFacts<'a> {
        InstrumentFacts {
            model_ref: Some("subject-model"),
            split_locked: true,
            prices: prices(),
            cases,
            metrics,
            run: None,
        }
    }

    fn exact() -> MetricFacts {
        MetricFacts {
            key: "pass".to_owned(),
            role: "quality".to_owned(),
            kind: "binary".to_owned(),
            better: "higher".to_owned(),
            grader_kind: "programmatic".to_owned(),
            grader: serde_json::json!({"kind": "exact"}),
            rubric: None,
        }
    }

    #[test]
    fn a_duplicate_input_is_reported() {
        let cases = vec![
            case(0, "same", Some("a")),
            case(1, "same", Some("a")),
            case(2, "other", Some("b")),
        ];
        let metrics = vec![exact()];
        let found = findings(&facts(&cases, &metrics), &Controls::default());
        assert!(found.iter().any(|draft| draft.item == "duplicate_cases"));
    }

    #[test]
    fn model_gold_from_the_subject_itself_is_misleading() {
        let mut cases = vec![case(0, "a", Some("a")), case(1, "b", Some("b"))];
        cases[0].gold_provenance = "model".to_owned();
        cases[0].gold_model = Some("subject-model".to_owned());
        let metrics = vec![exact()];
        let found = findings(&facts(&cases, &metrics), &Controls::default());
        let draft = found
            .iter()
            .find(|draft| draft.item == "model_gold")
            .expect("a model-gold finding");
        assert_eq!(draft.severity, Severity::Misleading);
    }

    #[test]
    fn a_floor_above_the_headroom_is_a_finding_and_a_fitting_floor_is_not() {
        assert!(resolution_shortfall(Some(0.14), Some(0.05)).is_some());
        assert!(resolution_shortfall(Some(0.03), Some(0.05)).is_none());
        assert!(resolution_shortfall(None, Some(0.05)).is_none());
    }

    #[test]
    fn the_resolution_line_states_what_was_measured() {
        let line = resolution_line(Some(0.14), Some(0.05));
        assert!(line.contains("0.140") && line.contains("0.050"), "{line}");
        assert!(resolution_line(None, None).contains("no scores yet"));
    }
}
