#![forbid(unsafe_code)]
//! The deterministic half of the prompt audit: the inventory and the signal scan (S16).
//!
//! `prompt_audits.check` runs this synchronously over ONE draft and persists nothing; its
//! `high`-confidence findings can refuse a publish. The model-backed `prompt_audit.run` uses the
//! same inventory as its input, so the two paths audit the same text.
//!
//! **The action-description surface is absent by construction** (S16 CCR-7): `ActionSpec` carries
//! no authored description — the OpenAPI text is generated boilerplate (`crates/actions/src/
//! openapi.rs`). The `actions` surface therefore yields no items until specs carry descriptions.
//! Process templates and the copilot system prompt are the two real surfaces.

use conveyor_domain::prompt_audit::{Confidence, FindingAction, FindingDraft, Pattern};
use conveyor_store::process::{ProcessRecord, ProcessStepRecord};

/// One piece of prompt text the audit can inspect.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SurfaceItem {
    /// `process:<id>` | `process_step:<id>` | `copilot:SYSTEM_PROMPT`.
    pub location: String,
    /// The text itself.
    pub text: String,
}

/// The inventory of one process version: its title, description and every string leaf of every
/// step's `input_template`.
pub fn process_items(process: &ProcessRecord, steps: &[ProcessStepRecord]) -> Vec<SurfaceItem> {
    let mut items = Vec::new();
    let mut push = |part: &str, text: &str| {
        if !text.trim().is_empty() {
            items.push(SurfaceItem {
                location: format!("process:{}", process.id),
                text: format!("{part}: {text}"),
            });
        }
    };
    push("title", &process.title);
    push("description", &process.description);
    for step in steps {
        let mut leaves = Vec::new();
        collect_strings(&step.input_template, &mut leaves);
        for leaf in leaves {
            if !leaf.trim().is_empty() {
                items.push(SurfaceItem {
                    location: format!("process_step:{}", step.id),
                    text: leaf,
                });
            }
        }
    }
    items
}

/// The copilot system prompt as one surface item.
pub fn copilot_items(system_prompt: &str) -> Vec<SurfaceItem> {
    if system_prompt.trim().is_empty() {
        return Vec::new();
    }
    vec![SurfaceItem {
        location: "copilot:SYSTEM_PROMPT".to_owned(),
        text: system_prompt.to_owned(),
    }]
}

/// Every string leaf of a JSON value, in document order.
pub fn collect_strings(value: &serde_json::Value, out: &mut Vec<String>) {
    match value {
        serde_json::Value::String(text) => out.push(text.clone()),
        serde_json::Value::Array(items) => {
            for item in items {
                collect_strings(item, out);
            }
        }
        serde_json::Value::Object(map) => {
            for (_, item) in map {
                collect_strings(item, out);
            }
        }
        _ => {}
    }
}

/// The deterministic scan over a set of items.
///
/// The result is ordered by `(location, pattern, evidence)` and contains no duplicates, so two runs
/// over the same input are byte-identical — the reproducibility the publish gate relies on.
pub fn scan(items: &[SurfaceItem]) -> Vec<FindingDraft> {
    let mut findings: Vec<FindingDraft> = items.iter().flat_map(signals_for).collect();
    findings.sort_by(|left, right| {
        (&left.location, left.pattern.as_str(), &left.evidence).cmp(&(
            &right.location,
            right.pattern.as_str(),
            &right.evidence,
        ))
    });
    findings.dedup_by(|left, right| {
        left.location == right.location
            && left.pattern == right.pattern
            && left.evidence == right.evidence
    });
    findings
}

/// The findings a single item produces.
pub fn signals_for(item: &SurfaceItem) -> Vec<FindingDraft> {
    let text = &item.text;
    let lower = text.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut add = |pattern: Pattern,
                   confidence: Confidence,
                   action: FindingAction,
                   evidence: String,
                   why: &str| {
        out.push(FindingDraft {
            location: item.location.clone(),
            evidence,
            pattern,
            why_obsolete: why.to_owned(),
            confidence,
            action,
            replacement: None,
        });
    };

    // Group 1b — scaffolds the current API replaced, and thinking incantations.
    for needle in [
        "think step by step",
        "take a deep breath",
        "<scratchpad>",
        "<thinking>",
        "think harder",
        "don't overthink",
        "do not think",
    ] {
        if lower.contains(needle) {
            add(
                Pattern::ObsoleteApiFeature,
                Confidence::High,
                FindingAction::Remove,
                around(text, needle),
                "adaptive thinking supersedes the incantation; depth is controlled by effort, not prose",
            );
        }
    }
    // Group 1d — retired model names and request-config fossils in prompt text.
    for needle in [
        "claude-2",
        "claude-3",
        "claude-instant",
        "claude-opus-4",
        "claude-sonnet-4",
        "gpt-4",
        "budget_tokens",
    ] {
        if lower.contains(needle) {
            add(
                Pattern::StaleModelName,
                Confidence::High,
                FindingAction::Flag,
                around(text, needle),
                "the prompt names a retired model or a request parameter current models reject",
            );
        }
    }
    // Group 1c — step-by-step choreography for judgment work.
    if lower.contains("step 1") && (lower.contains("step 2") || lower.contains("step 3")) {
        add(
            Pattern::WrongAltitude,
            Confidence::Medium,
            FindingAction::Flag,
            first_line(text),
            "current models plan without a handwritten script; state the outcome, not the steps",
        );
    }
    // Group 1a — pressure density. A single emphasis is a scoped fix; a run of them is a register.
    let pressure = ["must", "never", "always", "critical", "important"]
        .iter()
        .map(|word| lower.matches(word).count())
        .sum::<usize>();
    if pressure >= 3 {
        add(
            Pattern::RedundantConstraint,
            Confidence::Medium,
            FindingAction::Flag,
            first_line(text),
            "several absolute markers at once stop carrying information and over-apply on instruction-following models",
        );
    }
    // Group 1e — a run of unconditional prohibitions.
    let prohibitions = text
        .lines()
        .filter(|line| {
            let line = line.trim_start().to_ascii_lowercase();
            line.starts_with("do not") || line.starts_with("never") || line.starts_with("avoid")
        })
        .count();
    if prohibitions >= 3 {
        add(
            Pattern::ContradictoryRule,
            Confidence::Medium,
            FindingAction::Flag,
            first_line(text),
            "describe the desired behavior positively; a prohibition list anchors the failure it enumerates",
        );
    }
    // Group 1f — numeric output ceilings and update choreography.
    for needle in ["under ", "at most ", "every "] {
        if lower.contains(needle)
            && (lower.contains(" word")
                || lower.contains(" sentence")
                || lower.contains(" bullet")
                || lower.contains("tool call"))
        {
            add(
                Pattern::OverSpecifiedFormat,
                Confidence::Medium,
                FindingAction::Flag,
                around(text, needle),
                "numeric output caps and fixed update cadences were tuned against an older model's verbosity",
            );
        }
    }
    // Group 1a — leftover hedges on actual requirements.
    for needle in ["try to ", "if possible"] {
        if lower.contains(needle) {
            add(
                Pattern::DatedInstruction,
                Confidence::Low,
                FindingAction::Flag,
                around(text, needle),
                "a hedge is read literally as permission to under-deliver; state the requirement",
            );
        }
    }
    // Group 4 — a per-call prefix interpolated ahead of the stable instruction text.
    if cache_hostile_order(text) {
        add(
            Pattern::CacheHostileOrder,
            Confidence::Medium,
            FindingAction::Flag,
            first_line(text),
            "a per-run prefix before the stable text defeats prompt caching; put stable content first",
        );
    }

    out
}

/// True when a template opens with a `{{…}}` interpolation and then carries substantial static text.
fn cache_hostile_order(text: &str) -> bool {
    let trimmed = text.trim_start();
    if !trimmed.starts_with("{{") {
        return false;
    }
    let Some(close) = trimmed.find("}}") else {
        return false;
    };
    let rest = trimmed[close + 2..].trim();
    rest.len() >= 40 && !rest.starts_with("{{")
}

fn first_line(text: &str) -> String {
    let line = text.lines().next().unwrap_or(text).trim();
    line.chars().take(200).collect()
}

fn around(text: &str, needle: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let Some(at) = lower.find(needle) else {
        return first_line(text);
    };
    let start = text[..at]
        .char_indices()
        .rev()
        .nth(40)
        .map_or(0, |(index, _)| index);
    text[start..]
        .chars()
        .take(200)
        .collect::<String>()
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(text: &str) -> SurfaceItem {
        SurfaceItem {
            location: "process_step:abc".to_owned(),
            text: text.to_owned(),
        }
    }

    #[test]
    fn a_thinking_incantation_is_a_high_confidence_finding() {
        let findings = signals_for(&item("Think step by step and answer."));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].pattern, Pattern::ObsoleteApiFeature);
        assert_eq!(findings[0].confidence, Confidence::High);
    }

    #[test]
    fn a_retired_model_name_is_high_confidence() {
        let findings = signals_for(&item("Use claude-3-sonnet for this step."));
        assert!(
            findings
                .iter()
                .any(|finding| finding.pattern == Pattern::StaleModelName
                    && finding.confidence == Confidence::High)
        );
    }

    #[test]
    fn pressure_density_is_medium_and_a_single_marker_is_not_a_finding() {
        assert!(signals_for(&item("You MUST return JSON.")).is_empty());
        let findings = signals_for(&item(
            "You MUST return JSON. NEVER add prose. ALWAYS include the id. CRITICAL: be exact.",
        ));
        assert!(
            findings
                .iter()
                .any(|finding| finding.pattern == Pattern::RedundantConstraint
                    && finding.confidence == Confidence::Medium)
        );
    }

    #[test]
    fn clean_text_produces_nothing_and_two_scans_agree() {
        let items = vec![
            item("Summarise the ticket in one paragraph for the on-call engineer."),
            SurfaceItem {
                location: "process:xyz".to_owned(),
                text: "title: Triage".to_owned(),
            },
        ];
        let first = scan(&items);
        let second = scan(&items);
        assert!(
            first.is_empty(),
            "a clean surface changes nothing: {first:?}"
        );
        assert_eq!(first, second);
    }

    #[test]
    fn a_volatile_prefix_before_stable_text_is_cache_hostile() {
        let findings = signals_for(&item(
            "{{card.id}} Read the following policy carefully and answer with the exact clause.",
        ));
        assert!(
            findings
                .iter()
                .any(|finding| finding.pattern == Pattern::CacheHostileOrder)
        );
        // Stable text first is not a finding.
        assert!(signals_for(&item(
            "Read the following policy carefully and answer with the exact clause for {{card.id}}."
        ))
        .is_empty());
    }

    #[test]
    fn findings_are_deterministic_and_deduplicated() {
        let items = vec![item("Think step by step. Think step by step.")];
        let findings = scan(&items);
        assert_eq!(
            findings.len(),
            1,
            "one finding per location and pattern: {findings:?}"
        );
    }
}
