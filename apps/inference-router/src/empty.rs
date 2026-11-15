//! Blank and vacuous completion classification.
//!
//! The Rust port of the reference router's `_empty_completion_reason`,
//! `_every_choice_blank`, `_is_vacuous_json` and `_is_empty_value`
//! (`inference-router/src/services/router_service.py`). The two reasons are kept apart because
//! they have different causes and different consequences: [`EmptyReason::Blank`] is a truncation
//! defect, [`EmptyReason::EmptyResult`] is a model that looked and found nothing.
//!
//! The crate has no other markdown-fence stripper (`contract/` holds none), so
//! [`strip_markdown_fences`] is the single copy here rather than a second regex beside a live one.

use serde_json::Value;

use crate::contract::schemas::{ChatCompletionResponse, ChatMessage};

/// Depth cap for the vacuity walk. Beyond this depth the answer is "not empty" — the default of
/// the whole classifier: a no-result is a shallow shape, and `serde_json` already refuses bodies
/// nested past its own recursion limit (that parse failure is likewise "not empty").
const VACUITY_MAX_DEPTH: usize = 12;

/// Why a completed 2xx response carries no usable answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmptyReason {
    /// The assistant produced no bytes at all: `content` is null or whitespace-only.
    Blank,
    /// `content` is valid JSON whose payload carries no result at all.
    EmptyResult,
}

/// Classify `choices[0]` of a completed response, or `None` when it is a usable answer.
///
/// ⛔ The tool-call guard covers both reasons: a tool-call reply with no text is a complete
/// answer, and the provider deliberately keeps its `content` null, so it would otherwise look
/// exactly like the blank defect.
///
/// A non-string, non-null `content` (a list of content parts) is USABLE — we do not second-guess
/// a shape we did not expect. `Blank` is checked before the JSON arm so the two reasons stay
/// mutually exclusive by construction rather than by a coincidence of JSON grammar.
pub fn completion_reason(response: &ChatCompletionResponse) -> Option<EmptyReason> {
    let Some(choice) = response.choices.first() else {
        // No choice produced any bytes; the reference's `_response_text` yields "" here.
        return Some(EmptyReason::Blank);
    };
    if has_tool_calls(&choice.message) {
        return None;
    }
    match &choice.message.content {
        Value::Null => Some(EmptyReason::Blank),
        Value::String(text) => {
            if text.trim().is_empty() {
                Some(EmptyReason::Blank)
            } else if is_vacuous_json(text) {
                Some(EmptyReason::EmptyResult)
            } else {
                None
            }
        }
        // An array of content parts: a shape we did not expect, so a usable answer.
        _ => None,
    }
}

/// True when NO choice carries text or a tool call.
///
/// `completion_reason` judges `choices[0]` only; this widens the retry/refuse decision to "every
/// choice" so an `n>1` reply with one usable choice is never refused. Content is judged with
/// `trim()` for the same reason as `completion_reason`. A non-string content is not blank.
///
/// Vacuously true for an empty `choices` list, matching the reference's loop.
pub fn every_choice_blank(response: &ChatCompletionResponse) -> bool {
    response
        .choices
        .iter()
        .all(|choice| !has_tool_calls(&choice.message) && content_is_blank(&choice.message.content))
}

/// Is `text` valid JSON whose payload carries no result at all?
///
/// Vacuous: an object all of whose values are empty (including `{}`), or a top-level `[]`.
///
/// ⛔ Not vacuous, each a false positive waiting to happen: anything that is not JSON (that is
/// `invalid`, an outcome of its own), a top-level scalar (`3`, `"done"`, `true`, `null`), and an
/// object with any scalar value (`{"n": 0}` is a complete answer). Markdown fences are stripped
/// first so a fenced body classifies the same as the identical unfenced one.
pub fn is_vacuous_json(text: &str) -> bool {
    let Ok(parsed) = serde_json::from_str::<Value>(&strip_markdown_fences(text)) else {
        return false;
    };
    match parsed {
        Value::Array(items) => items.is_empty(),
        Value::Object(map) => map.values().all(|value| is_empty_value(value)),
        // A top-level scalar: a bare literal is not confident evidence of "nothing".
        _ => false,
    }
}

/// Is one JSON value devoid of content? See [`is_vacuous_json`].
///
/// ⛔ Scalars are NEVER empty: `0`, `false` and `0.0` are answers (`{"n": 0}` answers "how many"),
/// and most live routing schemas have no array or object property at all.
///
/// `null` and a whitespace-only string are empty — the wire form of "absent", and a rationale of
/// `""` carries exactly as much as `[]`. A NON-EMPTY LIST IS NEVER EMPTY even if every element is
/// hollow (`{"relationships": [{}]}` asserts one relationship exists); recursion runs through
/// object values only.
fn is_empty_value(value: &Value) -> bool {
    is_empty_value_at(value, 0)
}

fn is_empty_value_at(value: &Value, depth: usize) -> bool {
    if depth >= VACUITY_MAX_DEPTH {
        return false;
    }
    match value {
        Value::Null => true,
        Value::String(text) => text.trim().is_empty(),
        Value::Array(items) => items.is_empty(),
        Value::Object(map) => map
            .values()
            .all(|inner| is_empty_value_at(inner, depth + 1)),
        Value::Bool(_) | Value::Number(_) => false,
    }
}

fn has_tool_calls(message: &ChatMessage) -> bool {
    message
        .tool_calls
        .as_ref()
        .is_some_and(|calls| !calls.is_empty())
}

fn content_is_blank(content: &Value) -> bool {
    match content {
        Value::Null => true,
        Value::String(text) => text.trim().is_empty(),
        _ => false,
    }
}

/// The body of the first markdown fence in `text`, or the whole text trimmed when there is none.
///
/// A fenced JSON body, with or without a language tag on the opening fence, and prose followed by
/// an inline fence both yield the body; an unclosed fence is not a fence and leaves the text
/// unchanged (so it stays non-JSON).
fn strip_markdown_fences(text: &str) -> String {
    let Some(open) = text.find("```") else {
        return text.trim().to_owned();
    };
    let after = text[open + 3..].trim_start();
    // Drop the opening fence's language tag (`json`), when there is one.
    let after = match after.find(char::is_whitespace) {
        Some(end)
            if end > 0
                && after[..end]
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '+')) =>
        {
            &after[end..]
        }
        _ => after,
    };
    let body = after.trim_start();
    let Some(close) = body.find("```") else {
        return text.trim().to_owned();
    };
    body[..close].trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fence_with_a_language_tag_yields_its_body() {
        assert_eq!(
            strip_markdown_fences("```json\n{\"a\": []}\n```"),
            "{\"a\": []}"
        );
        assert_eq!(strip_markdown_fences("```\n{}\n```"), "{}");
        assert_eq!(strip_markdown_fences("```json {}```"), "{}");
        assert_eq!(strip_markdown_fences("answer: ```json\n[]\n``` done"), "[]");
    }

    #[test]
    fn an_unclosed_fence_is_not_a_fence() {
        assert_eq!(strip_markdown_fences("```json\n{}"), "```json\n{}");
        assert_eq!(strip_markdown_fences("{\"a\": []}"), "{\"a\": []}");
    }
}
