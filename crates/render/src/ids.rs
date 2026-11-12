#![forbid(unsafe_code)]
//! Identifiers and text escaping — the two places a renderer stops being deterministic.

use uuid::Uuid;

/// An element's SVG id.
pub(crate) fn element(id: Uuid) -> String {
    format!("e-{id}")
}

/// A relationship's SVG id.
pub(crate) fn relationship(id: Uuid) -> String {
    format!("r-{id}")
}

/// A view's SVG id.
pub(crate) fn view(id: Uuid) -> String {
    format!("v-{id}")
}

/// A `view_object`'s SVG id.
pub(crate) fn object(id: Uuid) -> String {
    format!("o-{id}")
}

/// A `view_connection`'s SVG id.
pub(crate) fn connection(id: Uuid) -> String {
    format!("c-{id}")
}

/// Escape text for XML character data and attribute values.
///
/// One function for both contexts: `"` and `'` are escaped as well as `&<>`, so a name that
/// arrives from an import cannot break out of a `label="…"` attribute.
pub(crate) fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // XML 1.0 cannot carry most control characters at all. Dropping them is the only
            // option that keeps the document parseable; `char::is_control` covers the C0/C1
            // range and leaves tab/newline/CR alone, which are legal.
            character if character.is_control() && !matches!(character, '\t' | '\n' | '\r') => {}
            character => out.push(character),
        }
    }
    out
}

/// Format a coordinate for the document.
///
/// Two decimals, trailing zeros trimmed, and `-0` normalised to `0` — because `-0` and `0` are
/// the same coordinate but not the same bytes.
pub(crate) fn number(value: f64) -> String {
    if !value.is_finite() {
        return "0".to_owned();
    }
    let mut text = format!("{value:.2}");
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    if text == "-0" {
        text = "0".to_owned();
    }
    text
}
