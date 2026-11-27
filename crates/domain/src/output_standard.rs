//! The programme output standard: one vocabulary and its legacy aliases.
//!
//! `seed/output-standard.json` is the source of truth; the Python seed builder, the generated
//! console copy and this module read the same file. A document category is canonicalised through
//! the alias map before it is stored, and anything neither canonical nor an alias is refused with
//! `422 unknown_category` by the action layer.

use std::collections::{BTreeMap, HashSet};
use std::fmt;

use serde_json::Value;

/// The embedded standard. `include_str!` fails the build if the file moves or disappears.
const STANDARD: &str = include_str!("../../../seed/output-standard.json");

/// A category that is neither canonical nor an alias in the standard.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownCategory(pub String);

impl fmt::Display for UnknownCategory {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} is not an output category in seed/output-standard.json",
            self.0
        )
    }
}

impl std::error::Error for UnknownCategory {}

fn standard() -> Value {
    serde_json::from_str(STANDARD).expect("seed/output-standard.json is valid JSON")
}

/// Canonicalise one category through the standard's aliases.
///
/// A canonical value passes through; an alias becomes its target; anything else is an error.
/// The caller trims the slot first, so an empty string is an error here rather than a silent null.
pub fn canonical_category(raw: &str) -> Result<String, UnknownCategory> {
    let raw = raw.trim();
    let document = standard();
    let categories = document["categories"]
        .as_array()
        .expect("the standard has a categories array");
    let aliases = document["aliases"]
        .as_object()
        .expect("the standard has an aliases object");
    let canonical = aliases.get(raw).and_then(Value::as_str).unwrap_or(raw);
    if categories
        .iter()
        .any(|value| value.as_str() == Some(canonical))
    {
        Ok(canonical.to_owned())
    } else {
        Err(UnknownCategory(raw.to_owned()))
    }
}

/// The standard's version (the seed's `version` field).
pub fn standard_version() -> u64 {
    standard()["version"].as_u64().unwrap_or(0)
}

/// The canonical vocabulary, in the standard's order.
pub fn categories() -> Vec<String> {
    standard()["categories"]
        .as_array()
        .expect("the standard has a categories array")
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

/// One category's authoring template: the required sections and the starter skeleton.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Template {
    /// The headings a publishable output must carry.
    pub required: Vec<String>,
    /// The headings a template suggests but does not require.
    pub optional: Vec<String>,
    /// The Markdown a compose dialog seeds.
    pub skeleton: String,
}

fn strings(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// The template for one category, or `None` for a category the standard does not know.
pub fn template(category: &str) -> Option<Template> {
    let document = standard();
    let entry = document["templates"].as_object()?.get(category)?;
    Some(Template {
        required: strings(entry.get("required")),
        optional: strings(entry.get("optional")),
        skeleton: entry
            .get("skeleton")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
    })
}

/// The headings a category's published body must carry. Unknown categories have none.
pub fn required_sections(category: &str) -> Vec<String> {
    template(category)
        .map(|template| template.required)
        .unwrap_or_default()
}

/// The required headings absent from `body`, in the template's order.
///
/// Comparison is case-insensitive and whitespace-collapsed; ATX headings at any level count, and
/// a heading inside a fenced code block does not. A category with no template is never blocked.
pub fn missing_sections(category: &str, body: &str) -> Vec<String> {
    let required = required_sections(category);
    if required.is_empty() {
        return Vec::new();
    }
    let present = headings(body);
    required
        .into_iter()
        .filter(|heading| !present.contains(&heading_key(heading)))
        .collect()
}

fn heading_key(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// The ATX headings of a Markdown body, normalised, ignoring fenced code.
fn headings(markdown: &str) -> HashSet<String> {
    let mut found = HashSet::new();
    let mut fence: Option<char> = None;
    for line in markdown.lines() {
        let trimmed = line.trim_start();
        if let Some(marker) = trimmed
            .strip_prefix("```")
            .map(|_| '`')
            .or_else(|| trimmed.strip_prefix("~~~").map(|_| '~'))
        {
            match fence {
                None => fence = Some(marker),
                Some(current) if current == marker => fence = None,
                Some(_) => {}
            }
            continue;
        }
        if fence.is_some() {
            continue;
        }
        let level = trimmed.chars().take_while(|value| *value == '#').count();
        if !(1..=6).contains(&level) {
            continue;
        }
        let rest = &trimmed[level..];
        if !rest.starts_with(char::is_whitespace) {
            continue;
        }
        let text = rest.trim().trim_end_matches('#').trim();
        if !text.is_empty() {
            found.insert(heading_key(text));
        }
    }
    found
}

/// One project class from the standard: the facts that select it and the output kinds it declares.
///
/// The list is first-match-wins, exactly as the retired `scripts/check-output-standard.py`
/// resolved it. The register publishes the resolved class instead of a checker recomputing it, so
/// there is one derivation and the site shows its result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Class {
    /// The class name (`p0`, `archived`, …).
    pub name: String,
    /// The project facts that select the class; every pair must match.
    pub when: BTreeMap<String, String>,
    /// The kinds the class requires while the project is live.
    pub now: Vec<String>,
    /// The kinds it requires for closure.
    pub closure: Vec<String>,
}

/// Every class, in the standard's order.
pub fn classes() -> Vec<Class> {
    standard()["classes"]
        .as_array()
        .expect("the standard has a classes array")
        .iter()
        .map(|class| Class {
            name: class["name"].as_str().unwrap_or_default().to_owned(),
            when: class["when"]
                .as_object()
                .map(|facts| {
                    facts
                        .iter()
                        .filter_map(|(key, value)| Some((key.clone(), value.as_str()?.to_owned())))
                        .collect()
                })
                .unwrap_or_default(),
            now: strings(class.get("now")),
            closure: strings(class.get("closure")),
        })
        .collect()
}

/// The class a project's facts select: the first whose `when` facts all match.
///
/// A class with no facts would match everything, so it is never selected; the standard's own
/// checker refused such a class, and a project with no matching class is a register gap.
pub fn class_for(facts: &BTreeMap<String, String>) -> Option<Class> {
    classes().into_iter().find(|class| {
        !class.when.is_empty()
            && class
                .when
                .iter()
                .all(|(key, value)| facts.get(key) == Some(value))
    })
}

#[cfg(test)]
mod tests {
    use super::{
        UnknownCategory, canonical_category, categories, class_for, classes, heading_key, headings,
        missing_sections, required_sections, template,
    };
    use std::collections::BTreeMap;

    fn facts(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect()
    }

    #[test]
    fn every_class_names_its_categories_and_the_selection_is_first_match() {
        let all = classes();
        assert!(!all.is_empty(), "the standard declares classes");
        for class in &all {
            assert!(!class.name.is_empty(), "a class with no name");
            assert!(
                !class.when.is_empty(),
                "class {} matches everything",
                class.name
            );
        }
        let selected = class_for(&facts(&[("key", "P0"), ("status", "active")]))
            .expect("the p0 class selects by key");
        assert_eq!(selected.name, "p0");
        assert!(!selected.now.is_empty(), "p0 declares live outputs");
        assert_eq!(
            class_for(&facts(&[("key", "NOT-A-PROJECT"), ("status", "active")])),
            None
        );
        assert_eq!(class_for(&BTreeMap::new()), None);
        // A class compares only the facts it names: the archived class matches on measured status.
        let archived = class_for(&facts(&[("key", "T17"), ("measured_status", "archived")]))
            .expect("an archived project");
        assert_eq!(archived.name, "archived");
    }

    #[test]
    fn aliases_map_to_the_canonical_names() {
        assert_eq!(
            canonical_category("analysis").expect("alias"),
            "inception-analysis"
        );
        assert_eq!(
            canonical_category("architecture").expect("alias"),
            "architecture-design"
        );
        assert_eq!(canonical_category("brief").expect("alias"), "project-brief");
        assert_eq!(
            canonical_category("project-brief").expect("canonical"),
            "project-brief"
        );
    }

    #[test]
    fn an_unknown_category_is_refused() {
        assert_eq!(
            canonical_category("not-a-category"),
            Err(UnknownCategory("not-a-category".to_owned()))
        );
        assert!(canonical_category("").is_err());
    }

    #[test]
    fn every_canonical_category_round_trips() {
        let vocabulary = categories();
        assert!(!vocabulary.is_empty());
        for category in vocabulary {
            assert_eq!(
                canonical_category(&category).expect("canonical"),
                category,
                "the vocabulary must be closed"
            );
        }
    }

    /// The rigour vocabulary (#221) is part of the closed set: a class may require it and a
    /// document may carry it. Removing one is a standard change, never a silent rename.
    #[test]
    fn the_rigour_categories_are_in_the_vocabulary() {
        let vocabulary = categories();
        for category in [
            "benefit-profile",
            "benefits-map",
            "projects-dossier",
            "tranche-plan",
            "tranche-review",
            "benefits-strategy",
            "realization-plan",
            "stakeholder-profiles",
            "architecture-principles",
            "adm-profile",
            "artifact-standard",
            "architecture-artifact",
            "statement-of-architecture-work",
            "compliance-review",
            "waiver",
            "maturity-assessment",
            "skills-assessment",
        ] {
            assert!(
                vocabulary.iter().any(|value| value == category),
                "{category} is missing from seed/output-standard.json"
            );
            assert_eq!(
                canonical_category(category).expect("rigour category"),
                category,
                "a rigour category canonicalises to itself"
            );
        }
    }

    /// The TOGAF/SAFe artefact vocabulary the live document set uses (#819) is part of the closed
    /// set: a phase template may require it and a document may carry it. Removing one is a
    /// standard change, never a silent rename.
    #[test]
    fn the_togaf_artefact_categories_are_in_the_vocabulary() {
        let vocabulary = categories();
        for category in [
            "candidate-solutions",
            "gap-analysis",
            "project-context-diagram",
            "benefits-diagram",
            "mandate",
            "pi-plan",
            "pi-objectives",
            "adr-register",
            "architecture-artifact-instance",
            "benefit-evaluation",
            "benefits-registry",
            "inspect-adapt",
            "programme-risk-register",
            "system-demo",
        ] {
            assert!(
                vocabulary.iter().any(|value| value == category),
                "{category} is missing from seed/output-standard.json"
            );
            assert_eq!(
                canonical_category(category).expect("TOGAF/SAFe category"),
                category,
                "a TOGAF/SAFe category canonicalises to itself"
            );
        }
    }

    /// The standard is the only source: every category it accepts also has a template, and the
    /// required headings are the ones the skeleton seeds.
    #[test]
    fn every_category_has_a_complete_template() {
        for category in categories() {
            let template = template(&category).expect("every category has a template");
            assert!(
                !template.required.is_empty(),
                "{category} has no required headings"
            );
            assert!(
                !template.skeleton.trim().is_empty(),
                "{category} has no skeleton"
            );
            let present = headings(&template.skeleton);
            for heading in &template.required {
                assert!(
                    present.contains(&heading_key(heading)),
                    "{category} skeleton lacks the required heading {heading}"
                );
            }
        }
    }

    /// A required heading is matched case- and whitespace-insensitively; a missing one is named.
    #[test]
    fn a_missing_section_is_named_and_a_present_one_is_not() {
        let body = "# Vision\n\n## Purpose\n\nWhy.\n\n## Outcomes\n\nWhat.\n";
        let missing = missing_sections("vision", body);
        assert!(!missing.contains(&"Purpose".to_owned()));
        assert!(!missing.contains(&"Outcomes".to_owned()));
        assert!(missing.contains(&"Boundary".to_owned()));
        assert!(missing.contains(&"Who it serves".to_owned()));
    }

    /// A heading an author wrote inside a fence is code, not a section.
    #[test]
    fn a_heading_in_a_fenced_block_does_not_count() {
        let body = "# Vision\n\n```\n## Purpose\n## Outcomes\n## Boundary\n## Who it serves\n```\n";
        assert_eq!(missing_sections("vision", body).len(), 4);
    }

    /// An unknown category is never blocked: the standard cannot say what it requires.
    #[test]
    fn an_unknown_category_is_never_blocked() {
        assert!(missing_sections("not-a-category", "# Anything\n").is_empty());
        assert!(required_sections("not-a-category").is_empty());
    }
}
