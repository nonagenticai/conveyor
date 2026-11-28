//! T7 (SPEC_V2 §5.8, D69): the work-package spec's BDD story, its Gherkin acceptance, the
//! INVEST Definition of Ready and the acceptance-based Definition of Done.
//!
//! A work package is one card carrying exactly one spec: `As a <role>, I want <capability>, so
//! that <value>`, a Gherkin `feature` with a `@verify=<id>` tag on every scenario, the six
//! INVEST criteria each with the work package's own evidence, and a DoD *derived* from the
//! `card_done` gate's evidence — never authored. This module owns the pure rules. The doors
//! that call them are `cards.spec_put` (writes), `runs.create` (the DoR refusal) and
//! `assert_card_done_tx` (the DoD refusal); the database CHECK is the second door.
//!
//! **Parsing.** `feature` is parsed by the cucumber-rs `gherkin` crate, so the spec and the
//! issue contract share one grammar. The authored text is stored; [`validate`] returns the
//! derived `{key, name, given[], when[], then[], verify}` form. `Scenario Outline` is expanded
//! at parse time: one scenario per example row, with `<column>` placeholders substituted in the
//! name and the steps, and the row's cells appended to the name (`"name [cell, cell]"`) so
//! every row has a distinct key. The key is the slug of the expanded scenario name and is
//! stable for the life of the spec; a duplicate key is refused because evidence could not name
//! one scenario unambiguously.
//!
//! **DoR and DoD are separate gates.** [`readiness`] is asked at run start; [`dod`] is asked
//! when a card enters a `done` list. Neither is folded into [`validate`], so a spec's structure
//! can be read without deciding that the work is ready to start or done.

use std::collections::HashSet;

use gherkin::{Feature, GherkinEnv, StepType};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

/// The six INVEST criteria, lowercase, in the order the specification lists them.
pub const DOR_CRITERIA: [&str; 6] = [
    "independent",
    "negotiable",
    "valuable",
    "estimable",
    "small",
    "testable",
];

/// The authored input to one work-package spec (`cards.spec_put`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkPackageSpecIn {
    /// The story's `As a <role>` clause.
    pub story_persona: String,
    /// The story's `I want <capability>` clause.
    pub story_action: String,
    /// The story's `so that <value>` clause.
    pub story_benefit: String,
    /// The Gherkin feature document, exactly as authored.
    pub feature: String,
    /// The definition of ready: `{criterion: {met: bool, evidence: String}}`.
    pub dor: Value,
}

/// One parsed acceptance scenario.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scenario {
    /// The slug of [`Self::name`]; the stable id evidence and steps reference.
    pub key: String,
    /// The scenario name, after outline substitution.
    pub name: String,
    /// The `Given` steps' text (an `And`/`But` continuation keeps its leading type).
    pub given: Vec<String>,
    /// The `When` steps' text.
    pub when: Vec<String>,
    /// The `Then` steps' text.
    pub then: Vec<String>,
    /// The id named by the scenario's `@verify=<id>` tag.
    pub verify: String,
}

/// One derived Definition-of-Done row: exactly one per scenario.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DodRow {
    /// The scenario key this row judges.
    pub scenario: String,
    /// Whether the evidence names this key with a run or step of the card.
    pub verified: bool,
    /// The evidence objects that verify the scenario.
    pub evidence: Vec<Value>,
}

/// Why a work-package spec is refused.
///
/// The actions map these to the typed 422 codes §5.8 names:
/// [`Self::StoryIncomplete`] and [`Self::ScenariosMissing`] carry their own codes,
/// [`Self::ScenarioInvalid`] is a missing acceptance scenario, and [`Self::DorUnmet`] is the
/// run-start refusal.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum SpecError {
    /// One of the three story clauses is blank.
    #[error("the user story is incomplete")]
    StoryIncomplete,
    /// The feature parses but expands to no scenario at all.
    #[error("the feature has no scenarios")]
    ScenariosMissing,
    /// A scenario is not acceptance: the feature does not parse, the document holds a second
    /// `Feature`, a scenario has no non-blank `@verify=<id>` tag, a given/when/then clause is
    /// empty, or two scenarios share a key.
    #[error("invalid scenario: {0}")]
    ScenarioInvalid(String),
    /// A Definition-of-Ready criterion is absent or malformed, or carries no evidence.
    #[error("definition of ready unmet: {0}")]
    DorUnmet(String),
}

/// Parse and validate the authored spec.
///
/// Requires three non-blank story clauses, exactly one `Feature`, at least one scenario (after
/// `Scenario Outline` expansion), non-empty Given/When/Then on every scenario, a non-blank
/// `@verify=<id>` tag on every scenario, and one unique key per scenario.
///
/// The Definition of Ready is [`readiness`]'s job and the Definition of Done is [`dod`]'s: they
/// are separate refusals at run start and at the `done` list.
pub fn validate(input: &WorkPackageSpecIn) -> Result<Vec<Scenario>, SpecError> {
    if [
        &input.story_persona,
        &input.story_action,
        &input.story_benefit,
    ]
    .iter()
    .any(|clause| clause.trim().is_empty())
    {
        return Err(SpecError::StoryIncomplete);
    }

    // `Feature::parse` appends a newline when the input lacks one, and the parsed span is an
    // offset into *that* string. Normalise here so the span also addresses our own text.
    let mut document = input.feature.clone();
    if !document.ends_with('\n') {
        document.push('\n');
    }
    let feature = Feature::parse(&document, GherkinEnv::default()).map_err(|error| {
        SpecError::ScenarioInvalid(format!("the feature does not parse: {error}"))
    })?;
    if !is_single_feature(&feature, &document) {
        return Err(SpecError::ScenarioInvalid(
            "the document carries more than one Feature".to_owned(),
        ));
    }

    let mut raw: Vec<&gherkin::Scenario> = feature.scenarios.iter().collect();
    for rule in &feature.rules {
        raw.extend(rule.scenarios.iter());
    }

    let mut scenarios = Vec::new();
    for scenario in raw {
        scenarios.extend(expand(scenario)?);
    }
    if scenarios.is_empty() {
        return Err(SpecError::ScenariosMissing);
    }

    let mut keys = HashSet::with_capacity(scenarios.len());
    for scenario in &scenarios {
        if !keys.insert(scenario.key.clone()) {
            return Err(SpecError::ScenarioInvalid(format!(
                "duplicate scenario key '{}'",
                scenario.key
            )));
        }
    }
    Ok(scenarios)
}

/// Judge the six INVEST criteria of a Definition of Ready.
///
/// Every one of [`DOR_CRITERIA`] must be present, an object `{met: bool, evidence: String}`,
/// with non-blank evidence. Returns `Ok(true)` only when all six are `met`; a criterion that is
/// `met: false` yields `Ok(false)` (the gate refuses), while an absent or malformed criterion —
/// including blank evidence — is [`SpecError::DorUnmet`]. Extra keys are ignored: a card type
/// may add criteria and never remove one.
pub fn readiness(dor: &Value) -> Result<bool, SpecError> {
    let object = dor.as_object().ok_or_else(|| {
        SpecError::DorUnmet("the definition of ready must be an object".to_owned())
    })?;
    let mut ready = true;
    for criterion in DOR_CRITERIA {
        let entry = object
            .get(criterion)
            .ok_or_else(|| SpecError::DorUnmet(format!("missing criterion '{criterion}'")))?;
        let entry = entry.as_object().ok_or_else(|| {
            SpecError::DorUnmet(format!("criterion '{criterion}' must be an object"))
        })?;
        let met = entry.get("met").and_then(Value::as_bool).ok_or_else(|| {
            SpecError::DorUnmet(format!(
                "criterion '{criterion}' must carry a boolean 'met'"
            ))
        })?;
        let evidenced = entry
            .get("evidence")
            .and_then(Value::as_str)
            .is_some_and(|evidence| !evidence.trim().is_empty());
        if !evidenced {
            return Err(SpecError::DorUnmet(format!(
                "criterion '{criterion}' has no evidence"
            )));
        }
        ready &= met;
    }
    Ok(ready)
}

/// Derive one Definition-of-Done row per scenario from the `card_done` gate's evidence.
///
/// A row is verified when at least one evidence object names its key — in `scenario` or
/// `scenario_id` — and carries a `run_id` or `step_id`. Only those objects are included: an
/// object that names the key without a run or step verifies nothing ("a step that verifies
/// nothing is not evidence"), and evidence for any other key is never attached to this row.
pub fn dod(scenarios: &[Scenario], evidence: &[Value]) -> Vec<DodRow> {
    scenarios
        .iter()
        .map(|scenario| {
            let matched: Vec<Value> = evidence
                .iter()
                .filter(|item| names_scenario(item, &scenario.key) && carries_verification(item))
                .cloned()
                .collect();
            DodRow {
                scenario: scenario.key.clone(),
                verified: !matched.is_empty(),
                evidence: matched,
            }
        })
        .collect()
}

/// A draft scenario before the key, the clause checks and the verification tag are applied.
struct Draft {
    name: String,
    given: Vec<String>,
    when: Vec<String>,
    then: Vec<String>,
    verify: String,
}

/// Expand one parsed scenario: a plain scenario yields one, an outline yields one per example
/// row with `<column>` placeholders substituted in the name and every step.
fn expand(raw: &gherkin::Scenario) -> Result<Vec<Scenario>, SpecError> {
    let verify = verify_id(&raw.tags).ok_or_else(|| {
        SpecError::ScenarioInvalid(format!(
            "scenario '{}' has no @verify=<id> tag",
            raw.name.trim()
        ))
    })?;
    let (given, when, then) = steps(raw);
    let name = raw.name.trim();

    let mut drafts = Vec::new();
    if raw.examples.is_empty() {
        drafts.push(Draft {
            name: name.to_owned(),
            given,
            when,
            then,
            verify,
        });
    } else {
        for examples in &raw.examples {
            let table = examples.table.as_ref().ok_or_else(|| {
                SpecError::ScenarioInvalid(format!(
                    "scenario '{name}' has an Examples block with no table"
                ))
            })?;
            let Some((headers, rows)) = table.rows.split_first() else {
                return Err(SpecError::ScenarioInvalid(format!(
                    "scenario '{name}' has an empty Examples table"
                )));
            };
            if rows.is_empty() {
                return Err(SpecError::ScenarioInvalid(format!(
                    "scenario '{name}' has an Examples table with no data rows"
                )));
            }
            for cells in rows {
                if cells.len() != headers.len() {
                    return Err(SpecError::ScenarioInvalid(format!(
                        "scenario '{name}' has an example row with the wrong number of cells"
                    )));
                }
                drafts.push(Draft {
                    name: format!(
                        "{} [{}]",
                        substitute(name, headers, cells),
                        cells.join(", ")
                    ),
                    given: given
                        .iter()
                        .map(|step| substitute(step, headers, cells))
                        .collect(),
                    when: when
                        .iter()
                        .map(|step| substitute(step, headers, cells))
                        .collect(),
                    then: then
                        .iter()
                        .map(|step| substitute(step, headers, cells))
                        .collect(),
                    verify: verify.clone(),
                });
            }
        }
    }
    drafts.into_iter().map(finish).collect()
}

/// Check a draft's clauses and name, and derive its key.
fn finish(draft: Draft) -> Result<Scenario, SpecError> {
    let key = slug(&draft.name);
    if key.is_empty() {
        return Err(SpecError::ScenarioInvalid(
            "a scenario needs a name".to_owned(),
        ));
    }
    if draft.given.is_empty() || draft.when.is_empty() || draft.then.is_empty() {
        return Err(SpecError::ScenarioInvalid(format!(
            "scenario '{}' needs Given, When and Then",
            draft.name
        )));
    }
    Ok(Scenario {
        key,
        name: draft.name,
        given: draft.given,
        when: draft.when,
        then: draft.then,
        verify: draft.verify,
    })
}

/// The scenario's steps, grouped by the type each `And`/`But` resolves to.
fn steps(raw: &gherkin::Scenario) -> (Vec<String>, Vec<String>, Vec<String>) {
    let mut given = Vec::new();
    let mut when = Vec::new();
    let mut then = Vec::new();
    for step in &raw.steps {
        let value = step.value.trim().to_owned();
        match step.ty {
            StepType::Given => given.push(value),
            StepType::When => when.push(value),
            StepType::Then => then.push(value),
        }
    }
    (given, when, then)
}

/// The id of the first non-blank `@verify=<id>` tag; `None` when the scenario has none.
fn verify_id(tags: &[String]) -> Option<String> {
    tags.iter().find_map(|tag| {
        tag.strip_prefix("verify=")
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
    })
}

/// Replace each `<column>` placeholder with the example row's cell.
fn substitute(text: &str, headers: &[String], cells: &[String]) -> String {
    let mut result = text.to_owned();
    for (header, cell) in headers.iter().zip(cells) {
        result = result.replace(&format!("<{header}>"), cell);
    }
    result
}

/// The stable scenario key: lowercase alphanumeric runs joined by `-`.
fn slug(name: &str) -> String {
    let mut key = String::with_capacity(name.len());
    let mut separated = false;
    for character in name.chars() {
        if character.is_alphanumeric() {
            key.extend(character.to_lowercase());
            separated = false;
        } else if !key.is_empty() && !separated {
            key.push('-');
            separated = true;
        }
    }
    while key.ends_with('-') {
        key.pop();
    }
    key
}

/// Whether the whole document holds one `Feature`.
///
/// The crate's grammar parses one feature and does not anchor the input, so a second
/// `Feature:` block after the first would otherwise be silently ignored.
fn is_single_feature(feature: &Feature, document: &str) -> bool {
    let Some(tail) = document.get(feature.span.end..) else {
        return false;
    };
    let keyword = format!("{}:", feature.keyword.trim());
    !tail
        .lines()
        .any(|line| line.trim_start().starts_with(&keyword))
}

/// Whether an evidence object names `key` in `scenario` or `scenario_id`.
fn names_scenario(item: &Value, key: &str) -> bool {
    let Some(object) = item.as_object() else {
        return false;
    };
    ["scenario", "scenario_id"].iter().any(|field| {
        object
            .get(*field)
            .and_then(Value::as_str)
            .is_some_and(|name| name == key)
    })
}

/// Whether an evidence object carries a non-blank `run_id` or `step_id`.
fn carries_verification(item: &Value) -> bool {
    let Some(object) = item.as_object() else {
        return false;
    };
    ["run_id", "step_id"].iter().any(|field| {
        object.get(*field).is_some_and(|id| match id {
            Value::String(text) => !text.trim().is_empty(),
            Value::Number(_) => true,
            _ => false,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const FEATURE: &str = r#"Feature: work packages

  @verify=P2-one
  Scenario: One spec per card
    Given a card in a tenant
    When a second spec is inserted for it
    Then the unique constraint refuses it
    And the first spec remains

  @verify=P2-two
  Scenario: A spec of another tenant is invisible
    Given a spec of another tenant
    When this tenant reads its specs
    Then the other tenant's spec is invisible
"#;

    fn ready_dor() -> Value {
        json!({
            "independent": {"met": true, "evidence": "no blocked-by link"},
            "negotiable": {"met": true, "evidence": "outcome fixed; approach settled at delivery"},
            "valuable": {"met": true, "evidence": "the story's so-that names the benefit"},
            "estimable": {"met": true, "evidence": "baseline and every file are named"},
            "small": {"met": true, "evidence": "one work package, one run"},
            "testable": {"met": true, "evidence": "every scenario carries @verify"},
        })
    }

    fn input() -> WorkPackageSpecIn {
        WorkPackageSpecIn {
            story_persona: "a tenant running delivery work".to_owned(),
            story_action: "each work-package card to carry its spec".to_owned(),
            story_benefit: "work is accepted by criteria, not a checkbox".to_owned(),
            feature: FEATURE.to_owned(),
            dor: ready_dor(),
        }
    }

    #[test]
    fn a_valid_feature_yields_one_scenario_per_name_slug() {
        let scenarios = validate(&input()).expect("a valid spec");
        assert_eq!(scenarios.len(), 2);

        assert_eq!(scenarios[0].key, "one-spec-per-card");
        assert_eq!(scenarios[0].name, "One spec per card");
        assert_eq!(scenarios[0].given, vec!["a card in a tenant".to_owned()]);
        assert_eq!(
            scenarios[0].when,
            vec!["a second spec is inserted for it".to_owned()]
        );
        // `And` resolves to the preceding `Then`.
        assert_eq!(
            scenarios[0].then,
            vec![
                "the unique constraint refuses it".to_owned(),
                "the first spec remains".to_owned(),
            ]
        );
        assert_eq!(scenarios[0].verify, "P2-one");

        assert_eq!(scenarios[1].key, "a-spec-of-another-tenant-is-invisible");
        assert_eq!(scenarios[1].verify, "P2-two");
    }

    #[test]
    fn a_feature_that_does_not_parse_or_holds_two_features_is_refused() {
        let garbage = WorkPackageSpecIn {
            feature: "this is not a Gherkin document".to_owned(),
            ..input()
        };
        assert!(matches!(
            validate(&garbage),
            Err(SpecError::ScenarioInvalid(_))
        ));

        let doubled = WorkPackageSpecIn {
            feature: format!("{FEATURE}\nFeature: a second one\n"),
            ..input()
        };
        assert!(matches!(
            validate(&doubled),
            Err(SpecError::ScenarioInvalid(message)) if message.contains("more than one Feature")
        ));
    }

    #[test]
    fn a_scenario_without_a_verify_tag_is_refused() {
        let feature =
            "Feature: unverified\n\n  Scenario: no tag\n    Given a\n    When b\n    Then c\n";
        let unverified = WorkPackageSpecIn {
            feature: feature.to_owned(),
            ..input()
        };
        assert!(matches!(
            validate(&unverified),
            Err(SpecError::ScenarioInvalid(message)) if message.contains("@verify")
        ));
    }

    #[test]
    fn a_scenario_without_given_when_or_then_is_refused() {
        let feature = "Feature: incomplete\n\n  @verify=P2\n  Scenario: only a then\n    Then c\n";
        let incomplete = WorkPackageSpecIn {
            feature: feature.to_owned(),
            ..input()
        };
        assert!(matches!(
            validate(&incomplete),
            Err(SpecError::ScenarioInvalid(message)) if message.contains("Given, When and Then")
        ));
    }

    #[test]
    fn a_duplicate_scenario_key_is_refused() {
        let feature = "Feature: duplicates\n\n  @verify=P2-a\n  Scenario: Same name\n    Given a\n    When b\n    Then c\n\n  @verify=P2-b\n  Scenario: Same name\n    Given a\n    When b\n    Then c\n";
        let duplicated = WorkPackageSpecIn {
            feature: feature.to_owned(),
            ..input()
        };
        assert!(matches!(
            validate(&duplicated),
            Err(SpecError::ScenarioInvalid(message)) if message.contains("duplicate scenario key")
        ));
    }

    #[test]
    fn a_scenario_outline_expands_one_scenario_per_example_row() {
        let feature = r#"Feature: outlines

  @verify=P2-outline
  Scenario Outline: a card moves
    Given a card in <list>
    When it moves to <target>
    Then it appears in <target>

    Examples:
      | list  | target |
      | todo  | doing  |
      | doing | done   |
"#;
        let outline = WorkPackageSpecIn {
            feature: feature.to_owned(),
            ..input()
        };
        let scenarios = validate(&outline).expect("an expanded outline");
        assert_eq!(scenarios.len(), 2);

        assert_eq!(scenarios[0].key, "a-card-moves-todo-doing");
        assert_eq!(scenarios[0].given, vec!["a card in todo".to_owned()]);
        assert_eq!(scenarios[0].then, vec!["it appears in doing".to_owned()]);
        assert_eq!(scenarios[0].verify, "P2-outline");

        assert_eq!(scenarios[1].key, "a-card-moves-doing-done");
        assert_eq!(scenarios[1].given, vec!["a card in doing".to_owned()]);
        assert_eq!(scenarios[1].when, vec!["it moves to done".to_owned()]);
    }

    #[test]
    fn a_blank_story_clause_is_incomplete() {
        let blank = WorkPackageSpecIn {
            story_benefit: "   ".to_owned(),
            ..input()
        };
        assert_eq!(validate(&blank), Err(SpecError::StoryIncomplete));
    }

    #[test]
    fn a_feature_with_no_scenarios_is_missing_them() {
        let empty = WorkPackageSpecIn {
            feature: "Feature: empty\n".to_owned(),
            ..input()
        };
        assert_eq!(validate(&empty), Err(SpecError::ScenariosMissing));
    }

    #[test]
    fn all_six_invest_criteria_must_be_met_with_evidence() {
        assert_eq!(readiness(&ready_dor()), Ok(true));

        let mut unmet = ready_dor();
        unmet["small"] = json!({"met": false, "evidence": "two runs planned"});
        assert_eq!(readiness(&unmet), Ok(false));

        let missing = json!({});
        assert!(matches!(
            readiness(&missing),
            Err(SpecError::DorUnmet(message)) if message.contains("independent")
        ));

        let mut blank = ready_dor();
        blank["testable"] = json!({"met": true, "evidence": "  "});
        assert!(matches!(
            readiness(&blank),
            Err(SpecError::DorUnmet(message)) if message.contains("testable")
        ));

        let mut not_boolean = ready_dor();
        not_boolean["valuable"] = json!({"met": "yes", "evidence": "a benefit"});
        assert!(matches!(
            readiness(&not_boolean),
            Err(SpecError::DorUnmet(message)) if message.contains("valuable")
        ));

        assert!(matches!(
            readiness(&json!("ready")),
            Err(SpecError::DorUnmet(_))
        ));
    }

    #[test]
    fn the_dod_is_one_row_per_scenario_with_only_its_evidence() {
        let scenarios = validate(&input()).expect("a valid spec");
        let evidence = vec![
            json!({"scenario": "one-spec-per-card", "run_id": "run-1"}),
            json!({"scenario_id": "a-spec-of-another-tenant-is-invisible", "step_id": "step-9"}),
            // Names the key but carries no run or step: verifies nothing.
            json!({"scenario": "one-spec-per-card"}),
            // Verifies some other scenario.
            json!({"scenario": "some-other-scenario", "run_id": "run-2"}),
        ];
        let rows = dod(&scenarios, &evidence);
        assert_eq!(rows.len(), 2);

        assert_eq!(rows[0].scenario, "one-spec-per-card");
        assert!(rows[0].verified);
        assert_eq!(
            rows[0].evidence,
            vec![json!({"scenario": "one-spec-per-card", "run_id": "run-1"})]
        );

        assert_eq!(rows[1].scenario, "a-spec-of-another-tenant-is-invisible");
        assert!(rows[1].verified);
        assert_eq!(rows[1].evidence.len(), 1);

        let none = dod(&scenarios, &[]);
        assert!(
            none.iter()
                .all(|row| !row.verified && row.evidence.is_empty())
        );
    }
}
