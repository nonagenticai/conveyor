//! Rubric v1: the deterministic quality score over an immutable revision (S07 §5.5).
//!
//! `score = 100 · Σ(weight · raw / 5)`, clamped to `0..=100`, with the five weights summing
//! to 1. Every input is a plain value — the revision snapshot's `title`, `body` and
//! `acceptance`, the trace-link counts and the pre-computed maximum trigram similarity — so
//! re-scoring the same revision always produces the same number. Nothing here touches the
//! database; `quality.score` writes the result to `quality_evaluation` and never to the asset.
//!
//! **How a criterion becomes `raw`.** S07 §5.5 states three criteria as conditions and two as
//! explicit scales. A condition is worth the full 5 when it holds and 0 when it does not;
//! [`traceability_raw`] and [`uniqueness_raw`] follow the plan's stated scales exactly.
//!
//! - clarity — the title is 15–80 **characters** (Unicode scalar values) and contains no term
//!   from [`VAGUE_WORDS`] / [`VAGUE_PHRASES`]. Matching is case-insensitive and by whole
//!   word, so "everything" is not "thing".
//! - testability — at least one acceptance entry carries `given`/`when`/`then` (either as
//!   three non-empty keys or as those words in its text) or a numeric threshold.
//! - atomicity — every sentence of the body has at most one conjunction
//!   ([`CONJUNCTIONS`]) and the mean sentence length is at most 25 words.
//! - traceability — 0 links → 0, 1 → 3, 2 or more → 5 **when at least one of them reaches a
//!   card**; two or more with none to a card stays at 3 (S07 §5.5's "including a card").
//! - uniqueness — similarity ≥ 0.9 → 0, ≤ 0.5 → 5, linear in between.
//!
//! The criteria document stored with the score pins the rubric version and each raw and
//! weighted value, so a later rubric cannot be mistaken for this one.

use serde_json::{Value, json};

/// The rubric version recorded in every `quality_evaluation.criteria`.
pub const RUBRIC_VERSION: &str = "v1";

/// The clarity criterion's weight.
pub const CLARITY_WEIGHT: f64 = 0.20;
/// The testability criterion's weight.
pub const TESTABILITY_WEIGHT: f64 = 0.30;
/// The atomicity criterion's weight.
pub const ATOMICITY_WEIGHT: f64 = 0.20;
/// The traceability criterion's weight.
pub const TRACEABILITY_WEIGHT: f64 = 0.15;
/// The uniqueness criterion's weight.
pub const UNIQUENESS_WEIGHT: f64 = 0.15;

/// The top of a criterion's raw scale; `weighted = weight · raw / MAX_RAW`.
const MAX_RAW: f64 = 5.0;

/// The committed vague-term list (S07 §5.5). Matched as whole words, case-insensitively.
pub const VAGUE_WORDS: [&str; 6] = ["thing", "stuff", "various", "etc", "improve", "better"];

/// The committed vague phrases. Matched as substrings, case-insensitively.
pub const VAGUE_PHRASES: [&str; 2] = ["as needed", "and/or"];

/// The conjunctions counted per sentence for the atomicity criterion.
pub const CONJUNCTIONS: [&str; 4] = ["and", "or", "but", "nor"];

/// The input to one rubric evaluation.
#[derive(Clone, Debug, PartialEq)]
pub struct RubricInput {
    /// The revision snapshot's `title`.
    pub title: String,
    /// The revision snapshot's `body`.
    pub body: String,
    /// The revision snapshot's `acceptance` entries.
    pub acceptance: Vec<Value>,
    /// The total number of trace links from the revision's asset.
    pub trace_links: i64,
    /// How many of [`Self::trace_links`] reach a card.
    ///
    /// S07 §5.5's top tier is "≥2 including a card": the total alone cannot earn it.
    pub trace_links_to_card: i64,
    /// The highest trigram similarity to another asset of the same kind, in `0.0..=1.0`.
    pub max_similarity: f64,
}

/// One rubric evaluation.
#[derive(Clone, Debug, PartialEq)]
pub struct QualityScore {
    /// `100 · Σ(weight · raw / 5)`, clamped to `0..=100`.
    pub score: f64,
    /// The auditable breakdown: the rubric version and each criterion's `raw` and
    /// `weighted` value.
    pub criteria: Value,
}

/// Score one revision snapshot with rubric v1.
///
/// Pure and deterministic: the same [`RubricInput`] always produces the same score.
pub fn rubric(input: &RubricInput) -> QualityScore {
    let clarity = clarity_raw(&input.title);
    let testability = testability_raw(&input.acceptance);
    let atomicity = atomicity_raw(&input.body);
    let traceability = traceability_raw(input.trace_links, input.trace_links_to_card);
    let uniqueness = uniqueness_raw(input.max_similarity);

    let weighted = |weight: f64, raw: f64| weight * raw / MAX_RAW;
    let total = weighted(CLARITY_WEIGHT, clarity)
        + weighted(TESTABILITY_WEIGHT, testability)
        + weighted(ATOMICITY_WEIGHT, atomicity)
        + weighted(TRACEABILITY_WEIGHT, traceability)
        + weighted(UNIQUENESS_WEIGHT, uniqueness);
    let score = (100.0 * total).clamp(0.0, 100.0);

    QualityScore {
        score,
        criteria: json!({
            "rubric": RUBRIC_VERSION,
            "clarity": {
                "raw": clarity,
                "weighted": weighted(CLARITY_WEIGHT, clarity),
            },
            "testability": {
                "raw": testability,
                "weighted": weighted(TESTABILITY_WEIGHT, testability),
            },
            "atomicity": {
                "raw": atomicity,
                "weighted": weighted(ATOMICITY_WEIGHT, atomicity),
            },
            "traceability": {
                "raw": traceability,
                "weighted": weighted(TRACEABILITY_WEIGHT, traceability),
            },
            "uniqueness": {
                "raw": uniqueness,
                "weighted": weighted(UNIQUENESS_WEIGHT, uniqueness),
            },
        }),
    }
}

fn clarity_raw(title: &str) -> f64 {
    let length = title.chars().count();
    let length_ok = (15..=80).contains(&length);
    if length_ok && !has_vague_term(title) {
        MAX_RAW
    } else {
        0.0
    }
}

fn has_vague_term(text: &str) -> bool {
    let lower = text.to_lowercase();
    if VAGUE_PHRASES.iter().any(|phrase| lower.contains(phrase)) {
        return true;
    }
    let tokens = words(&lower);
    tokens
        .iter()
        .any(|token| VAGUE_WORDS.contains(&token.as_str()))
}

fn testability_raw(acceptance: &[Value]) -> f64 {
    if acceptance.iter().any(entry_is_testable) {
        MAX_RAW
    } else {
        0.0
    }
}

fn entry_is_testable(entry: &Value) -> bool {
    // The documented object shape: three non-empty keys.
    if let Value::Object(object) = entry {
        let gherkin = ["given", "when", "then"].iter().all(|key| {
            object.get(*key).is_some_and(|value| match value {
                Value::String(text) => !text.trim().is_empty(),
                _ => false,
            })
        });
        if gherkin {
            return true;
        }
    }
    // Otherwise look at the entry's text: either the three words, or a numeric threshold.
    let text = string_values(entry).join(" ");
    let lower = text.to_lowercase();
    let tokens = words(&lower);
    let has = |word: &str| tokens.iter().any(|token| token == word);
    (has("given") && has("when") && has("then")) || has_numeric_threshold(&lower)
}

fn has_numeric_threshold(lower: &str) -> bool {
    if !lower.chars().any(|c| c.is_ascii_digit()) {
        return false;
    }
    const MARKERS: [&str; 9] = [
        "<",
        ">",
        "≤",
        "≥",
        "%",
        "at least",
        "at most",
        "within",
        "no more than",
    ];
    MARKERS.iter().any(|marker| lower.contains(marker)) || lower.contains("no fewer than")
}

fn string_values(value: &Value) -> Vec<String> {
    match value {
        Value::String(text) => vec![text.clone()],
        Value::Array(items) => items.iter().flat_map(string_values).collect(),
        Value::Object(object) => object.values().flat_map(string_values).collect(),
        _ => Vec::new(),
    }
}

fn atomicity_raw(body: &str) -> f64 {
    let sentences = sentences(body);
    if sentences.is_empty() {
        return MAX_RAW;
    }
    let mut total_words = 0usize;
    let mut max_conjunctions = 0usize;
    for sentence in &sentences {
        let tokens = words(sentence);
        total_words += tokens.len();
        let conjunctions = tokens
            .iter()
            .filter(|token| CONJUNCTIONS.contains(&token.as_str()))
            .count();
        max_conjunctions = max_conjunctions.max(conjunctions);
    }
    let mean_words = total_words as f64 / sentences.len() as f64;
    if max_conjunctions <= 1 && mean_words <= 25.0 {
        MAX_RAW
    } else {
        0.0
    }
}

fn sentences(body: &str) -> Vec<&str> {
    body.split(['.', '!', '?', '\n'])
        .map(str::trim)
        .filter(|sentence| !sentence.is_empty())
        .collect()
}

/// The alphanumeric word tokens of `text`, lowercased.
fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect()
}

fn traceability_raw(trace_links: i64, trace_links_to_card: i64) -> f64 {
    if trace_links <= 0 {
        0.0
    } else if trace_links == 1 {
        3.0
    } else if trace_links_to_card >= 1 {
        MAX_RAW
    } else {
        3.0
    }
}

fn uniqueness_raw(max_similarity: f64) -> f64 {
    if !max_similarity.is_finite() {
        // An unknown similarity is treated as "not unique" rather than silently rewarded.
        return 0.0;
    }
    let similarity = max_similarity.clamp(0.0, 1.0);
    if similarity <= 0.5 {
        MAX_RAW
    } else if similarity >= 0.9 {
        0.0
    } else {
        MAX_RAW * (0.9 - similarity) / 0.4
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> RubricInput {
        RubricInput {
            // 34 characters, no vague term.
            title: "Retries are capped at three attempts".to_owned(),
            // Five words, one sentence, no conjunction.
            body: "Every mutating request is idempotent.".to_owned(),
            acceptance: vec![json!({
                "id": "a1",
                "given": "a card in the active list",
                "when": "it is moved to done",
                "then": "the board reflects the move"
            })],
            trace_links: 2,
            trace_links_to_card: 1,
            max_similarity: 0.1,
        }
    }

    fn raw(score: &QualityScore, criterion: &str) -> f64 {
        score.criteria[criterion]["raw"]
            .as_f64()
            .unwrap_or_else(|| panic!("{criterion}.raw is a number: {}", score.criteria))
    }

    fn assert_close(left: f64, right: f64) {
        assert!(
            (left - right).abs() < 1e-9,
            "expected {right}, got {left} (criteria {:?})",
            left
        );
    }

    #[test]
    fn the_five_weights_sum_to_one() {
        assert_close(
            CLARITY_WEIGHT
                + TESTABILITY_WEIGHT
                + ATOMICITY_WEIGHT
                + TRACEABILITY_WEIGHT
                + UNIQUENESS_WEIGHT,
            1.0,
        );
    }

    #[test]
    fn a_fully_scoring_input_scores_exactly_one_hundred() {
        let score = rubric(&input());
        assert_eq!(score.score, 100.0);
        for criterion in [
            "clarity",
            "testability",
            "atomicity",
            "traceability",
            "uniqueness",
        ] {
            assert_eq!(raw(&score, criterion), 5.0, "{criterion}");
        }
        assert_eq!(score.criteria["rubric"], json!("v1"));
        // weighted = weight · raw / 5, so a full raw contributes exactly its weight.
        assert_close(
            score.criteria["clarity"]["weighted"].as_f64().unwrap(),
            CLARITY_WEIGHT,
        );
    }

    #[test]
    fn a_wholly_deficient_input_scores_exactly_zero() {
        let score = rubric(&RubricInput {
            title: "Do the thing".to_owned(), // 12 characters and a vague word
            body: "A and b and c and d.".to_owned(), // three conjunctions in one sentence
            acceptance: Vec::new(),
            trace_links: 0,
            trace_links_to_card: 0,
            max_similarity: 1.0,
        });
        assert_eq!(score.score, 0.0);
        for criterion in [
            "clarity",
            "testability",
            "atomicity",
            "traceability",
            "uniqueness",
        ] {
            assert_eq!(raw(&score, criterion), 0.0, "{criterion}");
        }
    }

    #[test]
    fn clarity_accepts_fifteen_to_eighty_characters() {
        let case = |title: String| rubric(&RubricInput { title, ..input() });
        assert_eq!(raw(&case("abcdefghijklmno".to_owned()), "clarity"), 5.0);
        assert_eq!(raw(&case("abcdefghijklmn".to_owned()), "clarity"), 0.0);
        assert_eq!(raw(&case("a".repeat(80)), "clarity"), 5.0);
        assert_eq!(raw(&case("a".repeat(81)), "clarity"), 0.0);
    }

    #[test]
    fn clarity_rejects_the_committed_vague_terms_as_whole_words() {
        let case = |title: &str| {
            raw(
                &rubric(&RubricInput {
                    title: title.to_owned(),
                    ..input()
                }),
                "clarity",
            )
        };
        assert_eq!(case("The thing is set up correctly"), 0.0);
        assert_eq!(case("Various cards are laid out"), 0.0);
        assert_eq!(case("Cards, lists, etc. are shown"), 0.0);
        assert_eq!(case("Make the board better soon"), 0.0);
        assert_eq!(case("Sorting as needed for the view"), 0.0);
        assert_eq!(case("Cards and/or lists are sorted"), 0.0);
        // "everything" is not the word "thing".
        assert_eq!(case("Everything is properly sized here"), 5.0);
    }

    #[test]
    fn testability_accepts_gherkin_or_a_numeric_threshold() {
        let case = |acceptance: Vec<Value>| {
            raw(
                &rubric(&RubricInput {
                    acceptance,
                    ..input()
                }),
                "testability",
            )
        };
        assert_eq!(
            case(vec![json!({"given": "a", "when": "b", "then": "c"})]),
            5.0
        );
        assert_eq!(
            case(vec![json!(
                "Given a board, when a card moves, then it appears"
            )]),
            5.0
        );
        assert_eq!(
            case(vec![json!({"id": "a2", "text": "responds within 200 ms"})]),
            5.0
        );
        assert_eq!(
            case(vec![json!({"id": "a2", "text": "at least 3 cards fit"})]),
            5.0
        );
        assert_eq!(
            case(vec![json!({"id": "a2", "text": "the board works"})]),
            0.0
        );
        // A bare number is not a threshold.
        assert_eq!(case(vec![json!({"id": "a2", "text": "criterion 3"})]), 0.0);
        assert_eq!(case(Vec::new()), 0.0);
    }

    #[test]
    fn atomicity_counts_conjunctions_per_sentence_and_the_mean_length() {
        let case = |body: &str| {
            raw(
                &rubric(&RubricInput {
                    body: body.to_owned(),
                    ..input()
                }),
                "atomicity",
            )
        };
        assert_eq!(case("The board shows its cards."), 5.0);
        assert_eq!(case("The board shows its cards and lists."), 5.0);
        assert_eq!(case("The board and the list and the card."), 0.0);
        // Two sentences, each with one conjunction: the limit is per sentence.
        assert_eq!(
            case("The board shows cards and lists. The list shows cards and order."),
            5.0
        );
        // Mean length: exactly 25 words passes, 26 fails.
        assert_eq!(case(&format!("{}.", "word ".repeat(25).trim())), 5.0);
        assert_eq!(case(&format!("{}.", "word ".repeat(26).trim())), 0.0);
        // Two sentences of 25 words each: the mean is 25.
        let sentence = format!("{}.", "word ".repeat(25).trim());
        assert_eq!(case(&format!("{sentence} {sentence}")), 5.0);
        // An empty body has no sentence to violate the rule.
        assert_eq!(case(""), 5.0);
    }

    #[test]
    fn traceability_follows_the_documented_tiers() {
        let case = |links: i64, to_card: i64| {
            raw(
                &rubric(&RubricInput {
                    trace_links: links,
                    trace_links_to_card: to_card,
                    ..input()
                }),
                "traceability",
            )
        };
        assert_eq!(case(0, 0), 0.0);
        assert_eq!(case(1, 0), 3.0);
        assert_eq!(case(1, 1), 3.0, "one link is 3 whatever it reaches");
        assert_eq!(
            case(2, 1),
            5.0,
            "two links including a card reaches the top tier"
        );
        assert_eq!(case(3, 0), 3.0, "three links, none to a card, stays at 3");
        assert_eq!(case(50, 5), 5.0);
        assert_eq!(case(-1, 0), 0.0);
    }

    #[test]
    fn uniqueness_is_linear_between_the_two_thresholds() {
        let case = |similarity: f64| {
            raw(
                &rubric(&RubricInput {
                    max_similarity: similarity,
                    ..input()
                }),
                "uniqueness",
            )
        };
        assert_eq!(case(0.9), 0.0);
        assert_eq!(case(0.95), 0.0);
        assert_eq!(case(0.5), 5.0);
        assert_eq!(case(0.0), 5.0);
        assert_close(case(0.7), 2.5);
        assert_close(case(0.6), 5.0 * 0.3 / 0.4);
        assert_eq!(case(f64::NAN), 0.0);
    }

    #[test]
    fn the_same_input_always_scores_the_same() {
        let first = rubric(&input());
        let second = rubric(&input());
        assert_eq!(first, second);
        // The stored criteria carry only the version and per-criterion values.
        let object = first.criteria.as_object().expect("an object");
        assert_eq!(object.len(), 6);
        for criterion in [
            "clarity",
            "testability",
            "atomicity",
            "traceability",
            "uniqueness",
        ] {
            let entry = object[criterion].as_object().expect("a criterion object");
            assert!(entry.contains_key("raw"), "{criterion}");
            assert!(entry.contains_key("weighted"), "{criterion}");
        }
    }
}
