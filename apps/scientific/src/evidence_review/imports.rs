//! The imports family — `ER-002`: batches, rows, records and quarantine.
//!
//! Ported from the reference's `api/routers/imports.py` (minus `from-discovery`, whose upstream
//! is dead and which the capture records as dropped) and `application/services/imports.py`, plus
//! the bounded adapters in `infrastructure/imports/import_parser.py`.
//!
//! Three properties the code below is arranged around:
//!
//! * **One transaction, whole import.** Batch, rows, records and versions commit together. A
//!   half-committed batch would make `identified_total` (derived from `import_rows`) describe a
//!   corpus that never existed.
//! * **The unique constraint is the replay guarantee, not the digest check.** The `SELECT` at the
//!   top of [`import_payload`] is check-then-act; `uq_import_batch_payload_digest` plus
//!   `ON CONFLICT DO NOTHING` is what makes a retried import a replay instead of a second copy.
//! * **A malformed row never discards a valid one.** Parse failures quarantine with their original
//!   bytes; only a payload that is not the declared format at all is refused whole.
//!
//! The parsers are hand-written rather than delegated to `regex`/`csv`/`bibtexparser`: those
//! crates are not in this crate's manifest and the manifest is not this slice's to change. Each
//! one keeps the reference's bounds (`MAX_PAYLOAD_BYTES`, `MAX_ROWS`, `MAX_JSON_DEPTH`) so the
//! refusal a caller sees is the same one the reference emitted.

use std::collections::BTreeMap;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{Map, Value, json};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::actor::Actor;
use crate::evidence_review::reviews::{load_review, require_review_lead_for, require_review_open};
use crate::evidence_review::{hash_bytes, hash_value, new_id, pool};
use crate::problem::Problem;
use crate::state::Shared;

/// The schema pointer echoed on every record version. A label, not a resolvable reference: no
/// released `review-record` document exists, and saying so is truer than minting a URN.
pub const RECORD_SCHEMA_REF: &str =
    "https://contracts.nonagentic.ai/evidence/review-record.v1.schema.json";

/// The generic route's vocabulary. `discovery_session` is deliberately absent — see
/// [`VERIFIED_ONLY_SOURCE_KINDS`].
pub const SOURCE_KINDS: [&str; 4] = ["ris", "bibtex", "csv", "canonical_json"];

/// A source kind that names an upstream, so it may only be created by the route that fetches it.
/// That route is dropped in this port; the refusal below keeps the hole from reopening if one is
/// ever added back.
pub const VERIFIED_ONLY_SOURCE_KINDS: [&str; 1] = ["discovery_session"];

/// Recognised and deliberately unsupported, as opposed to unrecognised: answers with the
/// `AHTA-DISC-*` blocker instead of "must be one of …".
pub const BLOCKED_SOURCE_KINDS: [&str; 5] = ["xlsx", "xls", "workbook", "catchii", "cachii"];

/// Bounds from the reference, unchanged: refused before parsing, not discovered mid-parse.
const MAX_PAYLOAD_BYTES: usize = 32 * 1024 * 1024;
const MAX_ROWS: usize = 50_000;
const MAX_FIELD_CHARS: usize = 100_000;
/// A record TYPE is drawn from closed vocabularies; anything past this is not a type a producer
/// meant, and it lands in append-only audit storage.
const MAX_RECORD_TYPE_ANNOTATION_CHARS: usize = 64;
/// A bibliographic record is a flat object with a list of authors in it.
const MAX_JSON_DEPTH: usize = 64;
/// The envelope adds one level over the payload the parser will judge it by.
const ENVELOPE_DEPTH_BOUND: usize = MAX_JSON_DEPTH + 1;

/// `ER-002.7` — the guarded pre-ADR vocabulary. A body carrying one of these at any depth is
/// refused, never silently stripped: a dropped key imports while the caller believes the record
/// was typed as a canonical regulatory entity.
const FALLBACK_FORBIDDEN_KEYS: [&str; 11] = [
    "work_type",
    "regulatory_instrument",
    "regulatory_provision",
    "regulatory_effect",
    "legal_event",
    "legal_status",
    "regulatory_applicability",
    "regulatory_change",
    "normative_citation",
    "consolidation_snapshot",
    "recommendation_regulatory_link",
];

// ── query helpers ────────────────────────────────────────────────────────────────────────────

fn query_str<'a>(query: &'a Value, name: &str) -> Option<&'a str> {
    query
        .get(name)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn query_bool(query: &Value, name: &str) -> bool {
    match query.get(name) {
        Some(Value::Bool(value)) => *value,
        Some(Value::String(value)) => matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "true" | "1" | "yes" | "on"
        ),
        _ => false,
    }
}

fn query_int(query: &Value, name: &str, default: i64) -> Result<i64, Problem> {
    match query.get(name) {
        None | Some(Value::Null) => Ok(default),
        Some(Value::String(text)) => text
            .trim()
            .parse::<i64>()
            .map_err(|_| Problem::validation(format!("{name} must be an integer"))),
        Some(Value::Number(number)) => number
            .as_i64()
            .ok_or_else(|| Problem::validation(format!("{name} must be an integer"))),
        Some(_) => Err(Problem::validation(format!("{name} must be an integer"))),
    }
}

// ── the guarded-discriminator denylist (`ER-002.7`) ──────────────────────────────────────────

/// Fold a key to its comparable form: camelCase gets a boundary, then everything but lowercase
/// ASCII alphanumerics is dropped. `workType`, `work_type` and `x-work-type` are one token.
fn normalized_key(token: &str) -> String {
    let chars: Vec<char> = token.chars().collect();
    let mut spaced = String::with_capacity(token.len() + 4);
    for (index, character) in chars.iter().enumerate() {
        if index > 0 {
            let previous = chars[index - 1];
            let boundary = (previous.is_ascii_lowercase() || previous.is_ascii_digit())
                && character.is_ascii_uppercase();
            if boundary {
                spaced.push('_');
            }
        }
        spaced.push(*character);
    }
    spaced
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .map(|character| character.to_ascii_lowercase())
        .collect()
}

fn extension_prefixed(token: &str) -> bool {
    let mut chars = token.chars();
    match (chars.next(), chars.next()) {
        (Some('x') | Some('X'), Some(separator)) => matches!(separator, '-' | '_' | '.' | ' '),
        _ => false,
    }
}

/// Every spelling of `token` that means the same guarded key.
fn comparable_forms(token: &str) -> Vec<String> {
    let mut forms = vec![normalized_key(token)];
    if extension_prefixed(token) {
        let without_prefix: String = token.chars().skip(2).collect();
        let folded = normalized_key(&without_prefix);
        if !forms.contains(&folded) {
            forms.push(folded);
        }
    }
    forms
}

fn forbidden_set() -> Vec<String> {
    FALLBACK_FORBIDDEN_KEYS
        .iter()
        .map(|key| normalized_key(key))
        .collect()
}

/// Every JSON path at which a guarded key appears. Empty means clean. Iterative and depth-first
/// with a key's own hit emitted before descending, so `forbidden_paths` reads in document order.
fn find_forbidden_keys(payload: &Value) -> Vec<String> {
    let forbidden = forbidden_set();
    let mut hits = Vec::new();
    // (node, path, the key this node was reached through — `None` at the root and for elements).
    let mut stack: Vec<(&Value, String, Option<&str>)> = vec![(payload, "$".to_owned(), None)];
    while let Some((node, path, key)) = stack.pop() {
        if let Some(key) = key {
            let forms = comparable_forms(key);
            if forms.iter().any(|form| forbidden.contains(form)) {
                hits.push(path.clone());
            }
        }
        match node {
            Value::Object(map) => {
                for (child_key, value) in map.iter().rev() {
                    stack.push((
                        value,
                        format!("{path}.{child_key}"),
                        Some(child_key.as_str()),
                    ));
                }
            }
            Value::Array(items) => {
                for (index, item) in items.iter().enumerate().rev() {
                    stack.push((item, format!("{path}[{index}]"), None));
                }
            }
            _ => {}
        }
    }
    hits
}

fn guard_denylist(payload: &Value) -> Result<(), Problem> {
    let hits = find_forbidden_keys(payload);
    if hits.is_empty() {
        return Ok(());
    }
    Err(Problem::new(
        StatusCode::BAD_REQUEST,
        "forbidden_discriminator",
        "Forbidden Discriminator",
        "the request carries a guarded pre-ADR regulatory discriminator; \
         ADR-DATACOMPANY-REGULATORY-MODEL is not accepted, so this cannot be imported as a \
         canonical regulatory entity. A regulatory source document imports as an ordinary \
         record with record_type='report'.",
    )
    .with("forbidden_paths", json!(hits)))
}

/// True when a JSON value nests deeper than `max` levels (the root is depth 1).
fn deeper_than(value: &Value, max: usize) -> Option<String> {
    let mut stack: Vec<(&Value, usize, String)> = vec![(value, 1, "$".to_owned())];
    while let Some((node, depth, path)) = stack.pop() {
        if depth > max {
            return Some(path);
        }
        match node {
            Value::Object(map) => {
                for (key, child) in map {
                    stack.push((child, depth + 1, format!("{path}.{key}")));
                }
            }
            Value::Array(items) => {
                for (index, child) in items.iter().enumerate() {
                    stack.push((child, depth + 1, format!("{path}[{index}]")));
                }
            }
            _ => {}
        }
    }
    None
}

// ── normalization ────────────────────────────────────────────────────────────────────────────

/// NFKC + casefold + strip punctuation/symbols + collapse whitespace, as far as std allows.
///
/// Simplification, stated rather than hidden: the `unicode-normalization` and
/// `unicode-general-category` crates are not in this crate's manifest, so compatibility folding
/// (ligatures, full-width forms) and the full casefold (`ß` → `ss`) are not reproduced. The
/// comparable property — punctuation and symbols become spaces, case is folded, whitespace is
/// collapsed — is kept; a title whose only difference is a ligature or a capital sharp s
/// normalizes differently from the reference.
pub fn normalize_text(value: Option<&str>) -> Option<String> {
    let value = value?;
    let mut stripped = String::with_capacity(value.len());
    for character in value.chars() {
        if character.is_alphanumeric() {
            stripped.extend(character.to_lowercase());
        } else {
            stripped.push(' ');
        }
    }
    let collapsed = stripped.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        None
    } else {
        Some(collapsed)
    }
}

fn strip_url_noise(mut candidate: &str) -> &str {
    for scheme in ["https://", "http://"] {
        if let Some(rest) = candidate.strip_prefix(scheme) {
            candidate = rest;
            break;
        }
    }
    if let Some(rest) = candidate.strip_prefix("www.") {
        candidate = rest;
    }
    candidate
}

fn is_doi_shape(candidate: &str) -> bool {
    let Some(rest) = candidate.strip_prefix("10.") else {
        return false;
    };
    // Registrant code: 4-9 digits, with the rare dotted sub-prefix.
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    if !(4..=9).contains(&digits.len()) {
        return false;
    }
    let mut remainder = &rest[digits.len()..];
    while let Some(after_dot) = remainder.strip_prefix('.') {
        let group: String = after_dot.chars().take_while(char::is_ascii_digit).collect();
        if group.is_empty() {
            return false;
        }
        remainder = &after_dot[group.len()..];
    }
    let Some(suffix) = remainder.strip_prefix('/') else {
        return false;
    };
    !suffix.is_empty() && !suffix.chars().any(char::is_whitespace)
}

pub fn normalize_doi(value: Option<&str>) -> Option<String> {
    let value = value?;
    let lowered = strip_url_noise(value.trim()).to_lowercase();
    let mut candidate = lowered.as_str();
    for prefix in ["doi.org/", "dx.doi.org/", "doi:", "doi "] {
        if let Some(rest) = candidate.strip_prefix(prefix) {
            candidate = rest;
            break;
        }
    }
    let candidate = candidate.trim();
    if is_doi_shape(candidate) {
        Some(candidate.to_owned())
    } else {
        None
    }
}

/// A trailing `[PubMed - indexed for MEDLINE]`-style status annotation, one group, non-nesting.
fn strip_status_annotation(value: &str) -> &str {
    let trimmed = value.trim_end();
    let Some(close) = trimmed.strip_suffix(']') else {
        return value;
    };
    let Some(open) = close.rfind('[') else {
        return value;
    };
    let inside = &close[open + 1..];
    if inside.contains('[') || inside.contains(']') {
        return value;
    }
    close[..open].trim_end()
}

/// `33,933,206` → `33933206`; strictly grouped so prose commas are untouched.
fn strip_thousands(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    let mut out = String::with_capacity(value.len());
    let mut index = 0;
    while index < chars.len() {
        if chars[index].is_ascii_digit() {
            let digit_start = index;
            while index < chars.len() && chars[index].is_ascii_digit() {
                index += 1;
            }
            let first_group = index - digit_start;
            let mut groups = 0;
            let mut cursor = index;
            while cursor + 1 < chars.len() && chars[cursor] == ',' {
                let group_start = cursor + 1;
                let mut group_end = group_start;
                while group_end < chars.len() && chars[group_end].is_ascii_digit() {
                    group_end += 1;
                }
                if group_end - group_start != 3 {
                    break;
                }
                groups += 1;
                cursor = group_end;
            }
            let at_boundary = digit_start == 0 || !chars[digit_start - 1].is_alphanumeric();
            let ends_boundary = cursor >= chars.len() || !chars[cursor].is_alphanumeric();
            if first_group <= 3 && groups > 0 && at_boundary && ends_boundary {
                for character in &chars[digit_start..index] {
                    out.push(*character);
                }
                let mut inner = index;
                while inner < cursor {
                    if chars[inner] != ',' {
                        out.push(chars[inner]);
                    }
                    inner += 1;
                }
                index = cursor;
            } else {
                for character in &chars[digit_start..index] {
                    out.push(*character);
                }
            }
        } else {
            out.push(chars[index]);
            index += 1;
        }
    }
    out
}

fn ci_word_boundary_at(haystack: &str, position: usize, word: &str) -> bool {
    if !haystack
        .get(position..)
        .map(|rest| rest.starts_with(word))
        .unwrap_or(false)
    {
        return false;
    }
    let before_ok = haystack[..position]
        .chars()
        .next_back()
        .map(|character| !(character.is_alphanumeric() || character == '_'))
        .unwrap_or(true);
    let after_ok = haystack[position + word.len()..]
        .chars()
        .next()
        .map(|character| !(character.is_alphanumeric() || character == '_'))
        .unwrap_or(true);
    before_ok && after_ok
}

/// A PMID label at the START of a field value. Anchoring is the whole defence against MEDLINE's
/// `Comment in:` / `Erratum in:` notes, which carry another paper's PMID.
fn labelled_pmid(value: &str) -> Option<String> {
    let trimmed = value.trim_start();
    let lowered = trimmed.to_lowercase();
    if !lowered.starts_with("pmid") {
        return None;
    }
    let mut rest = &trimmed[4..];
    rest = rest.trim_start();
    if let Some(stripped) = rest.strip_prefix([':', '#', '-']) {
        rest = stripped.trim_start();
    }
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() || digits.len() > 8 {
        return None;
    }
    let after = rest[digits.len()..].chars().next();
    let boundary = after
        .map(|character| !(character.is_alphanumeric() || character == '_'))
        .unwrap_or(true);
    if boundary { Some(digits) } else { None }
}

/// Any occurrence of `word` (already lowercased) that is a whole word.
fn has_word(haystack: &str, word: &str) -> bool {
    let mut from = 0usize;
    while let Some(offset) = haystack[from..].find(word) {
        let at = from + offset;
        if ci_word_boundary_at(haystack, at, word) {
            return true;
        }
        from = at + word.len().max(1);
        if from >= haystack.len() {
            break;
        }
    }
    false
}

fn has_pubmed_source(source: &str) -> bool {
    let lowered = source.to_lowercase();
    has_word(&lowered, "pubmed") || has_word(&lowered, "medline")
}

fn names_pubmed_central(source: &str) -> bool {
    let lowered = source.to_lowercase();
    let mut from = 0;
    while let Some(offset) = lowered[from..].find("pubmed") {
        let at = from + offset;
        let mut cursor = at + "pubmed".len();
        let rest = &lowered[cursor..];
        let whitespace = rest.len() - rest.trim_start_matches(char::is_whitespace).len();
        cursor += whitespace;
        if lowered[cursor..].starts_with("central") {
            return true;
        }
        from = at + 1;
        if from >= lowered.len() {
            break;
        }
    }
    has_word(&lowered, "pmc")
}

/// The PMID, if this RIS entry carries one about itself. See the reference's `_ris_pmid` for why
/// every rule here is as narrow as it is — `normalized_pmid` merges papers in the dedup engine.
fn ris_pmid(tags: &BTreeMap<String, Vec<String>>) -> Option<String> {
    for tag in ["AN", "ID", "C2", "N1"] {
        if let Some(values) = tags.get(tag) {
            for value in values {
                if let Some(pmid) = labelled_pmid(value) {
                    return Some(pmid);
                }
            }
        }
    }
    let mut source_parts: Vec<String> = Vec::new();
    for tag in ["DB", "DP"] {
        if let Some(values) = tags.get(tag) {
            source_parts.extend(values.iter().cloned());
        }
    }
    let source = source_parts.join(" ").trim().to_owned();
    let corroborated = if source.is_empty() {
        false
    } else if !has_pubmed_source(&source) || names_pubmed_central(&source) {
        return None;
    } else {
        true
    };
    for value in tags.get("AN").into_iter().flatten() {
        let candidate = value.trim();
        let digits = !candidate.is_empty() && candidate.chars().all(|c| c.is_ascii_digit());
        if !digits {
            continue;
        }
        if corroborated && (1..=8).contains(&candidate.len()) {
            return Some(candidate.to_owned());
        }
        if !corroborated && (7..=8).contains(&candidate.len()) && !candidate.starts_with('0') {
            return Some(candidate.to_owned());
        }
    }
    None
}

/// A positive PubMed integer, optionally wearing its label; anything else yields no key at all.
pub fn normalize_pmid(value: Option<&str>) -> Option<String> {
    let value = value?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lowered = strip_url_noise(trimmed).to_lowercase();
    let mut candidate: &str = &lowered;
    for prefix in ["pubmed.ncbi.nlm.nih.gov/", "ncbi.nlm.nih.gov/pubmed/"] {
        if let Some(rest) = candidate.strip_prefix(prefix) {
            candidate = rest;
            break;
        }
    }
    candidate = strip_status_annotation(candidate);
    let candidate = candidate.trim().trim_end_matches(['.', '/']).trim();
    let candidate = strip_thousands(candidate);
    let candidate = candidate.trim();

    let mut rest = candidate;
    if let Some(after) = rest.strip_prefix("pubmed") {
        let after = after.trim_start();
        if !after.is_empty() && after.len() != rest.len() {
            rest = after;
        }
    }
    if rest.starts_with("pmid") {
        let after = &rest["pmid".len()..];
        let after = after.trim_start();
        let after = after
            .strip_prefix([':', '#', '-'])
            .map(str::trim_start)
            .unwrap_or(after);
        if !after.is_empty() {
            rest = after;
        }
    }
    let all_digits = !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit());
    if all_digits && rest.len() <= 8 && !rest.starts_with('0') {
        Some(rest.to_owned())
    } else {
        None
    }
}

pub fn normalize_pmcid(value: Option<&str>) -> Option<String> {
    let digits: String = value?
        .chars()
        .filter(|character| character.is_ascii_digit())
        .collect();
    if digits.is_empty() {
        None
    } else {
        Some(format!("PMC{digits}"))
    }
}

/// A four-digit year, or nothing. A wrong year is a false dedup key; a missing one falls through.
pub fn normalize_year_text(value: &str) -> Option<i32> {
    let bytes: Vec<char> = value.chars().collect();
    for window in bytes.windows(4) {
        if window.iter().all(char::is_ascii_digit) && (window[0] == '1' || window[0] == '2') {
            let text: String = window.iter().collect();
            return text.parse::<i32>().ok();
        }
    }
    None
}

// ── the parsed record ───────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ParsedRecord {
    pub raw: Value,
    pub record_type: String,
    pub title: Option<String>,
    pub abstract_: Option<String>,
    pub publication_year: Option<i32>,
    pub authors: Vec<String>,
    pub container_title: Option<String>,
    pub doi: Option<String>,
    pub pmid: Option<String>,
    pub pmcid: Option<String>,
    pub arxiv_id: Option<String>,
    pub url: Option<String>,
    /// `field -> the source key it was actually read from`. Empty for the adapters that read one
    /// spelling per identifier; the field's own name is then the honest answer.
    pub field_sources: BTreeMap<String, String>,
    /// Source keys that carried something unusable and were skipped, never refused.
    pub unusable_sources: Vec<String>,
    /// Set when the row could not be understood. A quarantined row still carries its `raw`.
    pub quarantine_reason: Option<String>,
}

impl ParsedRecord {
    fn blank(raw: Value, reason: &str) -> Self {
        Self {
            raw,
            record_type: "other".to_owned(),
            title: None,
            abstract_: None,
            publication_year: None,
            authors: Vec::new(),
            container_title: None,
            doi: None,
            pmid: None,
            pmcid: None,
            arxiv_id: None,
            url: None,
            field_sources: BTreeMap::new(),
            unusable_sources: Vec::new(),
            quarantine_reason: Some(reason.to_owned()),
        }
    }

    fn content(&self) -> Value {
        json!({
            "record_type": self.record_type,
            "title": self.title,
            "abstract": self.abstract_,
            "publication_year": self.publication_year,
            "authors": self.authors,
            "container_title": self.container_title,
            "doi": self.doi,
            "pmid": self.pmid,
            "pmcid": self.pmcid,
            "arxiv_id": self.arxiv_id,
            "url": self.url,
        })
    }
}

#[derive(Debug)]
enum ParseError {
    /// These bytes are not the declared format. `422 unrecognized_payload`.
    Unreadable(String),
    /// A format deliberately not supported; carries its blocker. `422 unprocessable_state`.
    Unsupported(String, Option<String>),
    /// A declared bound was exceeded. `422 validation_failed`.
    TooLarge(String),
}

fn parser_problem(error: ParseError) -> Problem {
    match error {
        ParseError::Unreadable(detail) => Problem::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "unrecognized_payload",
            "Unrecognized Payload",
            detail,
        ),
        ParseError::Unsupported(detail, blocker) => {
            let problem = Problem::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "unprocessable_state",
                "Unprocessable State",
                detail,
            );
            match blocker {
                Some(blocker) => problem.with("blocker", json!(blocker)),
                None => problem,
            }
        }
        ParseError::TooLarge(detail) => Problem::validation(detail),
    }
}

fn check_bounds(payload: &[u8]) -> Result<(), ParseError> {
    if payload.len() > MAX_PAYLOAD_BYTES {
        return Err(ParseError::TooLarge(format!(
            "payload is {} bytes; the limit is {MAX_PAYLOAD_BYTES}",
            payload.len()
        )));
    }
    Ok(())
}

fn truncate(value: Option<&str>) -> Option<String> {
    value.map(|text| text.chars().take(MAX_FIELD_CHARS).collect())
}

/// UTF-16/UTF-32 (which decode as NUL-interleaved words) and embedded NUL are refused whole:
/// a quarantined row must persist its raw bytes, and a NUL makes that impossible.
fn decode_text(payload: &[u8], what: &str) -> Result<String, ParseError> {
    for (bom, name) in [
        (&[0xFFu8, 0xFE, 0x00, 0x00][..], "UTF-32 (little-endian)"),
        (&[0x00u8, 0x00, 0xFE, 0xFF][..], "UTF-32 (big-endian)"),
        (&[0xFFu8, 0xFE][..], "UTF-16 (little-endian)"),
        (&[0xFEu8, 0xFF][..], "UTF-16 (big-endian)"),
    ] {
        if payload.starts_with(bom) {
            return Err(ParseError::Unreadable(format!(
                "{what} is encoded as {name}, not UTF-8 — Excel's 'Unicode Text' and EndNote on \
                 Windows both do this. Decoding it as UTF-8 would produce a corpus of records \
                 whose every field is interleaved with NUL bytes. Re-export as UTF-8."
            )));
        }
    }
    if payload.contains(&0x00) {
        return Err(ParseError::Unreadable(format!(
            "{what} contains a NUL byte (U+0000), which PostgreSQL stores in neither text nor \
             jsonb. This is almost always a UTF-16 file saved without a byte-order mark. \
             Re-export as UTF-8."
        )));
    }
    let body = payload.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(payload);
    Ok(String::from_utf8_lossy(body).into_owned())
}

fn unsupported_format_blocker(format_hint: &str) -> ParseError {
    let lowered = format_hint.to_lowercase();
    if lowered.contains("catchii") || lowered.contains("cachii") {
        return ParseError::Unsupported(
            "Catchii/Cachii workbook mappings are not supplied; importing one would require \
             guessing column semantics"
                .to_owned(),
            Some("AHTA-DISC-003".to_owned()),
        );
    }
    if matches!(lowered.as_str(), "xlsx" | "xls" | "workbook") {
        return ParseError::Unsupported(
            "workbook import requires supplied mappings and round-trip fixtures".to_owned(),
            Some("AHTA-DISC-004".to_owned()),
        );
    }
    ParseError::Unsupported(format!("unsupported import format: {format_hint}"), None)
}

// ── RIS ─────────────────────────────────────────────────────────────────────────────────────

fn ris_line(line: &str) -> Option<(String, String)> {
    let chars: Vec<char> = line.chars().collect();
    if chars.len() < 5 {
        return None;
    }
    if !chars[0].is_ascii_uppercase()
        || !(chars[1].is_ascii_uppercase() || chars[1].is_ascii_digit())
    {
        return None;
    }
    if !chars[2].is_whitespace() || !chars[3].is_whitespace() || chars[4] != '-' {
        return None;
    }
    let mut rest: String = chars[5..].iter().collect();
    if rest.starts_with(' ') {
        rest = rest[1..].to_owned();
    }
    let tag: String = chars[0..2].iter().collect();
    Some((tag, rest.trim().to_owned()))
}

fn parse_ris(payload: &[u8]) -> Result<Vec<ParsedRecord>, ParseError> {
    check_bounds(payload)?;
    let text = decode_text(payload, "the RIS payload")?;
    let mut records = Vec::new();
    let mut current: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut last_tag: Option<String> = None;

    let flush = |current: &mut BTreeMap<String, Vec<String>>, records: &mut Vec<ParsedRecord>| {
        if current.is_empty() {
            return Ok(());
        }
        if records.len() >= MAX_ROWS {
            return Err(ParseError::TooLarge(format!(
                "more than {MAX_ROWS} records"
            )));
        }
        records.push(ris_to_record(std::mem::take(current)));
        Ok(())
    };

    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.trim().is_empty() {
            continue;
        }
        if let Some((tag, value)) = ris_line(line) {
            if tag == "ER" {
                flush(&mut current, &mut records)?;
                last_tag = None;
                continue;
            }
            if tag == "TY" && !current.is_empty() {
                flush(&mut current, &mut records)?;
            }
            current.entry(tag.clone()).or_default().push(value);
            last_tag = Some(tag);
        } else if let Some(tag) = last_tag.as_ref()
            && let Some(values) = current.get_mut(tag)
            && let Some(last) = values.last_mut()
        {
            last.push(' ');
            last.push_str(line.trim());
        }
    }
    flush(&mut current, &mut records)?;
    if records.is_empty() {
        return Err(ParseError::Unreadable(
            "no RIS entry was found. RIS entries are `TAG  - value` lines terminated by `ER  -`; \
             a file that is not RIS produces exactly this. Re-export as UTF-8 RIS, or correct \
             source_kind."
                .to_owned(),
        ));
    }
    Ok(records)
}

fn ris_to_record(tags: BTreeMap<String, Vec<String>>) -> ParsedRecord {
    let mut raw = Map::new();
    for (tag, values) in &tags {
        let value = if values.len() > 1 {
            json!(values)
        } else {
            json!(values.first().cloned().unwrap_or_default())
        };
        raw.insert(tag.clone(), value);
    }
    let raw = Value::Object(raw);
    let first = |tag: &str| {
        tags.get(tag)
            .and_then(|values| values.first())
            .map(String::as_str)
    };
    let first_of = |tags: &[&str]| tags.iter().find_map(|tag| first(tag));

    let Some(title) = first_of(&["TI", "T1", "BT"]) else {
        return ParsedRecord::blank(raw, "RIS entry has no title tag (TI/T1/BT)");
    };
    let record_type = match first("TY")
        .unwrap_or_default()
        .trim()
        .to_uppercase()
        .as_str()
    {
        "JOUR" | "EJOUR" | "ABST" => "journal_article",
        "INPR" | "UNPB" => "preprint",
        "RPRT" | "GOVDOC" | "STAND" => "report",
        "DATA" => "dataset",
        _ => "other",
    };
    ParsedRecord {
        raw,
        record_type: record_type.to_owned(),
        title: truncate(Some(title)),
        abstract_: truncate(first("AB")),
        publication_year: normalize_year_text(first_of(&["PY", "Y1"]).unwrap_or_default()),
        authors: tags
            .get("AU")
            .map(|values| {
                values
                    .iter()
                    .filter(|value| !value.is_empty())
                    .cloned()
                    .collect()
            })
            .unwrap_or_default(),
        container_title: first_of(&["JO", "JF", "T2"]).map(str::to_owned),
        doi: first("DO").map(str::to_owned),
        pmid: ris_pmid(&tags),
        pmcid: None,
        arxiv_id: None,
        url: first("UR").map(str::to_owned),
        field_sources: BTreeMap::new(),
        unusable_sources: Vec::new(),
        quarantine_reason: None,
    }
}

// ── BibTeX ──────────────────────────────────────────────────────────────────────────────────

/// Remove brace protection, keeping what it protected. An unbalanced closing brace is KEPT:
/// malformed input should stay visibly malformed.
fn strip_brace_protection(value: Option<&str>) -> Option<String> {
    let value = value?;
    let mut depth = 0usize;
    let mut out = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '{' => depth += 1,
            '}' if depth > 0 => depth -= 1,
            _ => out.push(character),
        }
    }
    let trimmed = out.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

#[derive(Debug, Default)]
struct BibEntry {
    entry_type: String,
    fields: BTreeMap<String, String>,
}

fn find_close(text: &str, open: usize) -> Option<usize> {
    let open_char = text.as_bytes()[open] as char;
    let close_char = if open_char == '{' { '}' } else { ')' };
    let mut depth = 0usize;
    for (offset, character) in text[open..].char_indices() {
        if character == open_char {
            depth += 1;
        } else if character == close_char {
            depth -= 1;
            if depth == 0 {
                return Some(open + offset);
            }
        }
    }
    None
}

/// Split on `separator` at brace depth 0 and outside quotes.
fn split_top_level(body: &str, separator: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut quoted = false;
    let mut start = 0;
    for (index, character) in body.char_indices() {
        match character {
            '{' if !quoted => depth += 1,
            '}' if !quoted => depth -= 1,
            '"' if depth == 0 => quoted = !quoted,
            character if character == separator && depth == 0 && !quoted => {
                parts.push(&body[start..index]);
                start = index + character.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(&body[start..]);
    parts
}

fn bib_value(part: &str, macros: &BTreeMap<String, String>) -> Option<String> {
    let part = part.trim();
    if part.is_empty() {
        return None;
    }
    if part.strip_prefix('{').is_some() {
        let close = find_close(part, 0)?;
        return Some(part[1..close].to_owned());
    }
    if let Some(inner) = part.strip_prefix('"') {
        let mut out = String::new();
        let mut escaped = false;
        for character in inner.chars() {
            if escaped {
                out.push(character);
                escaped = false;
            } else if character == '\\' {
                out.push(character);
                escaped = true;
            } else if character == '"' {
                break;
            } else {
                out.push(character);
            }
        }
        return Some(out);
    }
    let token = part.trim();
    Some(
        macros
            .get(&token.to_lowercase())
            .cloned()
            .unwrap_or_else(|| token.to_owned()),
    )
}

fn parse_bibtex_entries(text: &str) -> Result<Vec<BibEntry>, String> {
    let mut macros: BTreeMap<String, String> = BTreeMap::new();
    for (name, value) in [
        ("jan", "January"),
        ("feb", "February"),
        ("mar", "March"),
        ("apr", "April"),
        ("may", "May"),
        ("jun", "June"),
        ("jul", "July"),
        ("aug", "August"),
        ("sep", "September"),
        ("oct", "October"),
        ("nov", "November"),
        ("dec", "December"),
    ] {
        macros.insert(name.to_owned(), value.to_owned());
    }
    let mut entries = Vec::new();
    let mut cursor = 0usize;
    while let Some(offset) = text[cursor..].find('@') {
        let at = cursor + offset;
        let mut position = at + 1;
        let rest = &text[position..];
        let type_len = rest
            .chars()
            .take_while(|character| character.is_ascii_alphabetic())
            .map(char::len_utf8)
            .sum::<usize>();
        if type_len == 0 {
            cursor = at + 1;
            continue;
        }
        let entry_type = text[position..position + type_len].to_lowercase();
        position += type_len;
        let remainder = &text[position..];
        let open_offset = remainder
            .char_indices()
            .find(|(_, character)| !character.is_whitespace())
            .map(|(index, _)| index);
        let Some(open_offset) = open_offset else {
            break;
        };
        let open = position + open_offset;
        if !matches!(text.as_bytes()[open], b'{' | b'(') {
            cursor = open + 1;
            continue;
        }
        let Some(close) = find_close(text, open) else {
            return Err(format!(
                "unbalanced braces in @{entry_type} entry starting at byte {at}"
            ));
        };
        let body = &text[open + 1..close];
        match entry_type.as_str() {
            "comment" | "preamble" => {}
            "string" => {
                for assignment in split_top_level(body, ',') {
                    if let Some((name, value)) = assignment.split_once('=')
                        && let Some(resolved) = bib_value(value, &macros)
                    {
                        macros.insert(name.trim().to_lowercase(), resolved);
                    }
                }
            }
            _ => {
                let mut segments = split_top_level(body, ',');
                if !segments.is_empty() {
                    segments.remove(0); // the cite key
                }
                let mut entry = BibEntry {
                    entry_type: entry_type.clone(),
                    fields: BTreeMap::new(),
                };
                for segment in segments {
                    let Some((name, value)) = segment.split_once('=') else {
                        continue;
                    };
                    let mut resolved = String::new();
                    for (index, part) in split_top_level(value, '#').iter().enumerate() {
                        if let Some(part_value) = bib_value(part, &macros) {
                            if index > 0 {
                                resolved.push(' ');
                            }
                            resolved.push_str(&part_value);
                        }
                    }
                    entry
                        .fields
                        .insert(name.trim().to_lowercase(), resolved.trim().to_owned());
                }
                entries.push(entry);
            }
        }
        cursor = close + 1;
    }
    Ok(entries)
}

fn parse_bibtex(payload: &[u8]) -> Result<Vec<ParsedRecord>, ParseError> {
    check_bounds(payload)?;
    let text = decode_text(payload, "the BibTeX payload")?;
    let entries = match parse_bibtex_entries(&text) {
        Ok(entries) => entries,
        Err(detail) => {
            let shortened: String = text.chars().take(MAX_FIELD_CHARS).collect();
            return Ok(vec![ParsedRecord::blank(
                json!({"raw_text": shortened}),
                &format!("BibTeX parse failed: {detail}"),
            )]);
        }
    };
    if entries.len() > MAX_ROWS {
        return Err(ParseError::TooLarge(format!(
            "more than {MAX_ROWS} entries"
        )));
    }
    let mut records = Vec::with_capacity(entries.len());
    for entry in entries {
        let mut raw = Map::new();
        raw.insert("ENTRYTYPE".to_owned(), json!(entry.entry_type));
        for (name, value) in &entry.fields {
            raw.insert(name.clone(), json!(value));
        }
        let raw = Value::Object(raw);
        let field = |name: &str| entry.fields.get(name).map(String::as_str);
        let Some(title) = strip_brace_protection(field("title")) else {
            records.push(ParsedRecord::blank(raw, "BibTeX entry has no title field"));
            continue;
        };
        let record_type = match entry.entry_type.as_str() {
            "article" | "inproceedings" | "conference" => "journal_article",
            "unpublished" => "preprint",
            "techreport" | "manual" => "report",
            "dataset" => "dataset",
            _ => "other",
        };
        let authors: Vec<String> = field("author")
            .map(|value| {
                split_on_and(value)
                    .into_iter()
                    .filter(|name| !name.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        records.push(ParsedRecord {
            raw,
            record_type: record_type.to_owned(),
            title: truncate(Some(&title)),
            abstract_: truncate(field("abstract")),
            publication_year: normalize_year_text(field("year").unwrap_or_default()),
            authors,
            container_title: strip_brace_protection(
                field("journal").or_else(|| field("booktitle")),
            ),
            doi: field("doi").map(str::to_owned),
            pmid: field("pmid").map(str::to_owned),
            pmcid: None,
            arxiv_id: None,
            url: field("url").map(str::to_owned),
            field_sources: BTreeMap::new(),
            unusable_sources: Vec::new(),
            quarantine_reason: None,
        });
    }
    if records.is_empty() {
        return Err(ParseError::Unreadable(
            "no BibTeX entry was found. BibTeX entries start with `@article{`, `@misc{` and so \
             on; a file with none of those is not BibTeX. Correct source_kind, or re-export."
                .to_owned(),
        ));
    }
    Ok(records)
}

/// `author = {A and B}` — split on the literal ` and ` with surrounding whitespace, as the
/// reference did with `\s+and\s+`.
fn split_on_and(value: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = value;
    loop {
        let lowered = rest.to_lowercase();
        let mut found: Option<(usize, usize)> = None;
        let mut from = 0usize;
        while let Some(offset) = lowered[from..].find("and") {
            let at = from + offset;
            let before = rest[..at].chars().next_back();
            let after = rest[at + 3..].chars().next();
            if before.map(char::is_whitespace).unwrap_or(false)
                && after.map(char::is_whitespace).unwrap_or(false)
            {
                let start = rest[..at].trim_end().len();
                let end = at + 3 + (rest[at + 3..].len() - rest[at + 3..].trim_start().len());
                found = Some((start, end));
                break;
            }
            from = at + 1;
            if from >= lowered.len() {
                break;
            }
        }
        match found {
            Some((start, end)) => {
                names.push(rest[..start].trim().to_owned());
                rest = &rest[end..];
            }
            None => {
                names.push(rest.trim().to_owned());
                break;
            }
        }
    }
    names
}

// ── CSV ─────────────────────────────────────────────────────────────────────────────────────

/// RFC 4180 with `,` and `"` quoting: doubled quotes, embedded newlines, CRLF or LF.
fn split_csv(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    let mut any = false;
    while let Some(character) = chars.next() {
        any = true;
        if quoted {
            if character == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                }
            } else {
                field.push(character);
            }
            continue;
        }
        match character {
            '"' if field.is_empty() => quoted = true,
            ',' => {
                row.push(std::mem::take(&mut field));
            }
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
                any = false;
            }
            '\n' => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
                any = false;
            }
            _ => field.push(character),
        }
    }
    if any || !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

fn csv_alias(header: &str) -> Option<&'static str> {
    Some(match header.trim().to_lowercase().as_str() {
        "title" | "article title" => "title",
        "abstract" => "abstract",
        "year" | "publication year" => "publication_year",
        "authors" | "author" => "authors",
        "journal" | "source title" => "container_title",
        "doi" => "doi",
        "pmid" | "pubmed id" => "pmid",
        "pmcid" => "pmcid",
        "arxiv" => "arxiv_id",
        "url" => "url",
        "record_type" => "record_type",
        _ => return None,
    })
}

fn parse_csv(payload: &[u8]) -> Result<Vec<ParsedRecord>, ParseError> {
    check_bounds(payload)?;
    let text = decode_text(payload, "the CSV payload")?;
    let mut rows = split_csv(&text);
    if rows.is_empty() {
        return Err(ParseError::Unreadable(
            "the CSV payload is empty; a search that found nothing still exports its header row"
                .to_owned(),
        ));
    }
    let headers: Vec<String> = rows.remove(0);
    if !headers.iter().any(|header| csv_alias(header).is_some()) {
        return Err(ParseError::Unreadable(
            "no column in the header row matched a recognised bibliographic header, so this file \
             is not a CSV of records. An empty search result still exports its header row."
                .to_owned(),
        ));
    }
    let mut records = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        if index >= MAX_ROWS {
            return Err(ParseError::TooLarge(format!("more than {MAX_ROWS} rows")));
        }
        if row.iter().all(|field| field.is_empty()) {
            continue;
        }
        let mut raw = Map::new();
        let mut mapped: BTreeMap<&'static str, String> = BTreeMap::new();
        for (position, header) in headers.iter().enumerate() {
            let value = row.get(position).cloned().unwrap_or_default();
            if !header.is_empty() {
                raw.insert(header.clone(), json!(value));
            }
            if let Some(key) = csv_alias(header)
                && !value.is_empty()
            {
                mapped.insert(key, value);
            }
        }
        let raw = Value::Object(raw);
        let Some(title) = mapped.get("title") else {
            records.push(ParsedRecord::blank(
                raw,
                "CSV row has no recognisable title column",
            ));
            continue;
        };
        let record_type = mapped
            .get("record_type")
            .map(|value| value.trim().to_lowercase())
            .filter(|value| {
                matches!(
                    value.as_str(),
                    "journal_article" | "preprint" | "report" | "dataset" | "other"
                )
            })
            .unwrap_or_else(|| "other".to_owned());
        let authors = mapped
            .get("authors")
            .map(|value| {
                value
                    .split([';', '|'])
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        records.push(ParsedRecord {
            raw,
            record_type,
            title: truncate(Some(title)),
            abstract_: truncate(mapped.get("abstract").map(String::as_str)),
            publication_year: normalize_year_text(
                mapped
                    .get("publication_year")
                    .map(String::as_str)
                    .unwrap_or_default(),
            ),
            authors,
            container_title: mapped.get("container_title").cloned(),
            doi: mapped.get("doi").cloned(),
            pmid: mapped.get("pmid").cloned(),
            pmcid: mapped.get("pmcid").cloned(),
            arxiv_id: mapped.get("arxiv_id").cloned(),
            url: mapped.get("url").cloned(),
            field_sources: BTreeMap::new(),
            unusable_sources: Vec::new(),
            quarantine_reason: None,
        });
    }
    Ok(records)
}

// ── canonical JSON ──────────────────────────────────────────────────────────────────────────

/// One iterative pass for what survives a JSON decode and then kills the write: over-deep
/// nesting and NUL characters. Unpaired surrogates are already refused by the decoder.
fn audit_json(data: &Value, what: &str) -> Result<(), ParseError> {
    let mut stack: Vec<(&Value, usize)> = vec![(data, 1)];
    while let Some((node, depth)) = stack.pop() {
        if depth > MAX_JSON_DEPTH {
            return Err(ParseError::Unreadable(format!(
                "{what} nests more than {MAX_JSON_DEPTH} levels deep. A bibliographic record is a \
                 flat object with a list of authors in it; nothing legitimate is this deep."
            )));
        }
        match node {
            Value::String(text) => {
                if text.contains('\0') {
                    return Err(ParseError::Unreadable(format!(
                        "{what} contains a NUL character (U+0000). PostgreSQL stores U+0000 in \
                         neither text nor jsonb, so this record cannot be persisted even as a \
                         quarantined row. Remove it and re-submit."
                    )));
                }
            }
            Value::Object(map) => {
                for (key, value) in map {
                    if key.contains('\0') {
                        return Err(ParseError::Unreadable(format!(
                            "{what} carries a NUL character (U+0000) in an object key. PostgreSQL \
                             stores U+0000 in neither text nor jsonb."
                        )));
                    }
                    stack.push((value, depth + 1));
                }
            }
            Value::Array(items) => {
                for item in items {
                    stack.push((item, depth + 1));
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn decode_json(payload: &[u8], what: &str) -> Result<Value, ParseError> {
    let body = payload.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(payload);
    let text = std::str::from_utf8(body).map_err(|error| {
        ParseError::Unreadable(format!(
            "{what} is not UTF-8: {error}. JSON is UTF-8 by RFC 8259, so re-export the file as \
             UTF-8 rather than Latin-1 or UTF-16."
        ))
    })?;
    let data: Value = serde_json::from_str(text)
        .map_err(|error| ParseError::Unreadable(format!("{what} is not valid JSON: {error}")))?;
    audit_json(&data, what)?;
    Ok(data)
}

fn parse_canonical_json(payload: &[u8]) -> Result<Vec<ParsedRecord>, ParseError> {
    check_bounds(payload)?;
    let data = decode_json(payload, "payload")?;
    let items = if data.is_object() {
        data.get("records").cloned()
    } else {
        Some(data)
    };
    let Some(Value::Array(items)) = items else {
        return Err(ParseError::Unsupported(
            "canonical JSON must be a list of records, or {'records': [...]}".to_owned(),
            None,
        ));
    };
    if items.len() > MAX_ROWS {
        return Err(ParseError::TooLarge(format!(
            "more than {MAX_ROWS} records"
        )));
    }
    Ok(items
        .into_iter()
        .map(|item| match item {
            Value::Object(map) => json_to_record(Value::Object(map.clone()), &map),
            other => ParsedRecord::blank(json!({"value": other}), "record is not an object"),
        })
        .collect())
}

// ── canonical-JSON field mapping ───────────────────────────────────────────────────────────

#[derive(Debug)]
struct FieldShape {
    field: String,
    expected: &'static str,
    actual: &'static str,
}

impl FieldShape {
    fn problem(&self) -> String {
        format!(
            "unusable field '{}' must be {}; got {}",
            self.field, self.expected, self.actual
        )
    }
}

fn value_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(number) if number.is_f64() => "float",
        Value::Number(_) => "int",
        Value::String(_) => "str",
        Value::Array(_) => "list",
        Value::Object(_) => "dict",
    }
}

fn require_text(field: &str, value: Option<&Value>) -> Result<Option<String>, FieldShape> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.clone())),
        Some(other) => Err(FieldShape {
            field: field.to_owned(),
            expected: "a string",
            actual: value_kind(other),
        }),
    }
}

fn require_scalar(field: &str, value: Option<&Value>) -> Result<Option<String>, FieldShape> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.clone())),
        Some(Value::Number(number)) => Ok(Some(number.to_string())),
        Some(other) => Err(FieldShape {
            field: field.to_owned(),
            expected: "a single string or number",
            actual: value_kind(other),
        }),
    }
}

fn identifier_text(field: &str, value: Option<&Value>) -> Result<Option<String>, FieldShape> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.clone())),
        Some(Value::Number(number)) if number.is_i64() || number.is_u64() => {
            Ok(Some(number.to_string()))
        }
        Some(other) => Err(FieldShape {
            field: field.to_owned(),
            expected: "a string or an integer",
            actual: value_kind(other),
        }),
    }
}

/// Python truthiness, which decides which alias of a field wins: `null`, `false`, `0`, `""`, `[]`
/// and `{}` all fall through to the next spelling.
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(number) => number.as_f64().map(|n| n != 0.0).unwrap_or(true),
        Value::String(text) => !text.is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(map) => !map.is_empty(),
    }
}

fn first_truthy<'a>(item: &'a Map<String, Value>, names: &[&str]) -> Option<&'a Value> {
    names
        .iter()
        .filter_map(|name| item.get(*name))
        .find(|value| truthy(value))
}

/// `value is not None and value != ""`.
fn present(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::String(text)) => !text.is_empty(),
        Some(_) => true,
    }
}

/// One identifier, its source label, and any fallback source that was unusable.
fn identified<F>(
    field: &str,
    primary: Option<&Value>,
    fallbacks: &[(&str, Option<&Value>)],
    validate: F,
) -> Result<(String, Option<String>, Vec<String>), FieldShape>
where
    F: Fn(&str, Option<&Value>) -> Result<Option<String>, FieldShape>,
{
    let primary_value = validate(field, primary)?;
    if primary_value
        .as_deref()
        .map(|v| !v.is_empty())
        .unwrap_or(false)
    {
        return Ok((field.to_owned(), primary_value, Vec::new()));
    }
    let mut unusable = Vec::new();
    for (label, candidate) in fallbacks {
        if !present(*candidate) {
            continue;
        }
        match validate(label, *candidate) {
            Ok(resolved) => {
                if resolved.as_deref().map(|v| !v.is_empty()).unwrap_or(false) {
                    return Ok(((*label).to_owned(), resolved, unusable));
                }
            }
            Err(_) => unusable.push((*label).to_owned()),
        }
    }
    Ok((field.to_owned(), primary_value, unusable))
}

/// Spellings real producers emit for a type that IS in the vocabulary.
fn record_type_alias(normalized: &str) -> Option<&'static str> {
    Some(match normalized {
        "article" | "research_article" | "jour" => "journal_article",
        "posted_content" => "preprint",
        "techreport" | "tech_report" | "rprt" => "report",
        "data" => "dataset",
        _ => return None,
    })
}

/// The stored `record_type`, plus an annotation when a declared value was dropped. A spelling is
/// resolved; a genuinely unknown type stores as `other` and travels in `field_provenance`, so
/// "the producer said systematic_review and we could not store it" stays distinguishable.
fn canonical_record_type(declared: Option<&str>) -> (String, Vec<String>) {
    let Some(declared) = declared.filter(|value| !value.is_empty()) else {
        return ("other".to_owned(), Vec::new());
    };
    let normalized = declared.trim().to_lowercase().replace(['-', ' '], "_");
    if matches!(
        normalized.as_str(),
        "journal_article" | "preprint" | "report" | "dataset" | "other"
    ) {
        return (normalized, Vec::new());
    }
    if let Some(alias) = record_type_alias(&normalized) {
        return (alias.to_owned(), Vec::new());
    }
    let mut annotation = declared.trim().to_owned();
    if annotation.chars().count() > MAX_RECORD_TYPE_ANNOTATION_CHARS {
        annotation = annotation
            .chars()
            .take(MAX_RECORD_TYPE_ANNOTATION_CHARS)
            .collect();
        annotation.push('…');
    }
    (
        "other".to_owned(),
        vec![format!("record_type={annotation}")],
    )
}

fn json_to_record(raw: Value, item: &Map<String, Value>) -> ParsedRecord {
    match json_record_fields(raw.clone(), item) {
        Ok(record) => record,
        Err(shape) => ParsedRecord::blank(raw, &shape.problem()),
    }
}

fn json_record_fields(raw: Value, item: &Map<String, Value>) -> Result<ParsedRecord, FieldShape> {
    let title = require_text("title", first_truthy(item, &["title", "name"]))?;
    let Some(title) = title.filter(|value| !value.is_empty()) else {
        return Ok(ParsedRecord::blank(raw, "record has no title"));
    };
    let mut authors: Vec<String> = Vec::new();
    match item.get("authors").filter(|value| truthy(value)) {
        None => {}
        Some(Value::String(text)) => {
            authors = text
                .split([';', '|', ','])
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_owned)
                .collect();
        }
        Some(Value::Array(items)) => {
            for entry in items {
                match entry {
                    Value::String(text) if !text.is_empty() => authors.push(text.clone()),
                    Value::Object(map) => {
                        if let Some(name) = map.get("name").and_then(Value::as_str)
                            && !name.is_empty()
                        {
                            authors.push(name.to_owned());
                        }
                    }
                    Value::Number(number) => authors.push(number.to_string()),
                    _ => {}
                }
            }
        }
        Some(other) => {
            return Err(FieldShape {
                field: "authors".to_owned(),
                expected: "a list or a delimited string",
                actual: value_kind(other),
            });
        }
    }
    let (record_type, record_type_unusable) =
        canonical_record_type(require_text("record_type", item.get("record_type"))?.as_deref());

    let (external_ids, external_unusable): (Map<String, Value>, Vec<String>) =
        match item.get("external_ids") {
            None | Some(Value::Null) => (Map::new(), Vec::new()),
            Some(Value::Object(map)) => (map.clone(), Vec::new()),
            Some(_) => (Map::new(), vec!["external_ids".to_owned()]),
        };
    let id = |name: &str| external_ids.get(name);

    let (doi_field, doi, doi_unusable) = identified(
        "doi",
        item.get("doi"),
        &[("external_ids.doi", id("doi"))],
        require_text,
    )?;
    let (pmid_field, pmid, pmid_unusable) = identified(
        "pmid",
        item.get("pmid"),
        &[
            ("external_ids.pmid", id("pmid")),
            ("external_ids.pubmed_id", id("pubmed_id")),
        ],
        identifier_text,
    )?;
    let (pmcid_field, pmcid, pmcid_unusable) = identified(
        "pmcid",
        item.get("pmcid"),
        &[("external_ids.pmcid", id("pmcid"))],
        identifier_text,
    )?;
    let (arxiv_field, arxiv_id, arxiv_unusable) = identified(
        "arxiv_id",
        first_truthy(item, &["arxiv_id", "arxiv"]),
        &[("external_ids.arxiv_id", id("arxiv_id"))],
        identifier_text,
    )?;
    let (url_field, url, url_unusable) = identified(
        "url",
        first_truthy(item, &["url", "link"]),
        &[("pdf_url", item.get("pdf_url"))],
        require_text,
    )?;

    let mut field_sources = BTreeMap::new();
    field_sources.insert("doi".to_owned(), doi_field);
    field_sources.insert("pmid".to_owned(), pmid_field);
    field_sources.insert("pmcid".to_owned(), pmcid_field);
    field_sources.insert("arxiv_id".to_owned(), arxiv_field);
    field_sources.insert("url".to_owned(), url_field);

    let mut unusable_sources = record_type_unusable;
    unusable_sources.extend(external_unusable);
    unusable_sources.extend(doi_unusable);
    unusable_sources.extend(pmid_unusable);
    unusable_sources.extend(pmcid_unusable);
    unusable_sources.extend(arxiv_unusable);
    unusable_sources.extend(url_unusable);

    Ok(ParsedRecord {
        raw,
        record_type,
        title: truncate(Some(&title)),
        abstract_: truncate(
            require_text("abstract", first_truthy(item, &["abstract", "summary"]))?.as_deref(),
        ),
        publication_year: normalize_year_text(
            require_scalar(
                "publication_year",
                first_truthy(item, &["publication_year", "year", "published"]),
            )?
            .as_deref()
            .unwrap_or_default(),
        ),
        authors,
        container_title: require_text(
            "container_title",
            first_truthy(item, &["container_title", "journal", "venue"]),
        )?,
        doi,
        pmid,
        pmcid: normalize_pmcid(pmcid.as_deref()),
        arxiv_id,
        url,
        field_sources,
        unusable_sources,
        quarantine_reason: None,
    })
}

// ── the import service ─────────────────────────────────────────────────────────────────────

fn parse_payload(source_kind: &str, payload: &[u8]) -> Result<Vec<ParsedRecord>, ParseError> {
    match source_kind {
        "ris" => parse_ris(payload),
        "bibtex" => parse_bibtex(payload),
        "csv" => parse_csv(payload),
        "canonical_json" => parse_canonical_json(payload),
        other => Err(unsupported_format_blocker(other)),
    }
}

fn field_provenance(record: &ParsedRecord, row_id: Uuid) -> Value {
    let source = |field: &str| {
        record
            .field_sources
            .get(field)
            .cloned()
            .unwrap_or_else(|| field.to_owned())
    };
    let mut provenance = json!({
        "normalized_title": "title",
        "normalized_doi": source("doi"),
        "normalized_pmid": source("pmid"),
        "source_row": row_id.to_string(),
    });
    if !record.unusable_sources.is_empty()
        && let Some(object) = provenance.as_object_mut()
    {
        object.insert(
            "unusable_sources".to_owned(),
            json!(record.unusable_sources),
        );
    }
    provenance
}

#[derive(Debug, sqlx::FromRow)]
struct BatchRow {
    import_batch_id: Uuid,
    review_id: Uuid,
    source_kind: String,
    source_ref: Option<Value>,
    raw_payload_digest: String,
    accepted_count: i32,
    quarantined_count: i32,
    rejected_count: i32,
    created_at: DateTime<Utc>,
}

fn batch_summary(batch: &BatchRow, replayed: bool) -> Value {
    json!({
        "import_batch_id": batch.import_batch_id.to_string(),
        "review_id": batch.review_id.to_string(),
        "source_kind": batch.source_kind,
        "raw_payload_digest": batch.raw_payload_digest,
        "accepted_count": batch.accepted_count,
        "quarantined_count": batch.quarantined_count,
        "rejected_count": batch.rejected_count,
        "source_ref": batch.source_ref,
        "replayed": replayed,
        "created_at": iso(batch.created_at),
    })
}

fn iso(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Micros, true)
}

async fn batch_by_id(
    tx: &mut Transaction<'_, Postgres>,
    import_batch_id: Uuid,
) -> Result<Option<BatchRow>, Problem> {
    sqlx::query_as::<_, BatchRow>(
        "SELECT import_batch_id, review_id, source_kind, source_ref, raw_payload_digest,
                accepted_count, quarantined_count, rejected_count, created_at
           FROM import_batches WHERE import_batch_id = $1",
    )
    .bind(import_batch_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading the import batch", error))
}

async fn batch_by_digest(
    tx: &mut Transaction<'_, Postgres>,
    review_id: Uuid,
    digest: &str,
) -> Result<Option<BatchRow>, Problem> {
    sqlx::query_as::<_, BatchRow>(
        "SELECT import_batch_id, review_id, source_kind, source_ref, raw_payload_digest,
                accepted_count, quarantined_count, rejected_count, created_at
           FROM import_batches WHERE review_id = $1 AND raw_payload_digest = $2",
    )
    .bind(review_id)
    .bind(digest)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading the replayed import batch", error))
}

/// Import one batch into a review — **lead only** (`ER-002`).
///
/// The lead gate is independent of `load_review`: visibility is a tenant fact every reviewer
/// shares, and the sealed corpus every later claim hangs off is the lead's to assemble.
pub async fn import_payload(
    tx: &mut Transaction<'_, Postgres>,
    actor: &Actor,
    review_id: Uuid,
    source_kind: &str,
    payload: &[u8],
    declared_source_ref: Option<Value>,
    allow_empty: bool,
) -> Result<Value, Problem> {
    // Provenance is established here, so a machine is refused before anything else runs — and
    // independently of `require_review_lead_for`, which holds the same gate for its own reasons.
    actor.require_human()?;
    let subject = require_review_lead_for(tx, actor, review_id, "importing records").await?;
    require_review_open(tx, review_id, "importing records").await?;
    let digest = hash_bytes(payload);

    if let Some(existing) = batch_by_digest(tx, review_id, &digest).await? {
        // Identical bytes into the same review are the same import. This is what makes worker
        // death and client retry safe without a lease.
        return Ok(batch_summary(&existing, true));
    }

    let parsed = parse_payload(source_kind, payload).map_err(parser_problem)?;
    if parsed.is_empty() && !allow_empty {
        return Err(Problem::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "empty_source_not_declared",
            "Empty Source Not Declared",
            format!(
                "the {source_kind} payload parsed cleanly and contained no records. An import of \
                 zero records is indistinguishable from a successful import once it is written, \
                 and the review's identified_total is derived from it. If this search genuinely \
                 returned nothing, re-submit with allow_empty=true to record that; otherwise \
                 check the export."
            ),
        ));
    }
    // The denylist runs over what was actually parsed, so a guarded key nested inside a record
    // body is caught, not only one in the envelope.
    for (index, record) in parsed.iter().enumerate() {
        let hits = find_forbidden_keys(&record.raw);
        if !hits.is_empty() {
            let paths: Vec<String> = hits
                .into_iter()
                .map(|path| path.replacen('$', &format!("$[{index}]"), 1))
                .collect();
            return Err(Problem::new(
                StatusCode::BAD_REQUEST,
                "forbidden_discriminator",
                "Forbidden Discriminator",
                "the request carries a guarded pre-ADR regulatory discriminator; \
                 ADR-DATACOMPANY-REGULATORY-MODEL is not accepted, so this cannot be imported as \
                 a canonical regulatory entity. A regulatory source document imports as an \
                 ordinary record with record_type='report'.",
            )
            .with("forbidden_paths", json!(paths)));
        }
    }

    // ON CONFLICT DO NOTHING, then re-select: the loser writes no batch, no rows, no records and
    // no versions, and answers with the winner's batch as a replay.
    let batch_id = new_id();
    let inserted: Option<Uuid> = sqlx::query_scalar(
        "INSERT INTO import_batches
             (import_batch_id, review_id, tenant_id, source_kind, source_ref,
              raw_payload_digest, submitted_by_subject_id)
         VALUES ($1,$2,$3,$4,$5,$6,$7)
         ON CONFLICT ON CONSTRAINT uq_import_batch_payload_digest DO NOTHING
         RETURNING import_batch_id",
    )
    .bind(batch_id)
    .bind(review_id)
    .bind(actor.tenant)
    .bind(source_kind)
    .bind(declared_source_ref.as_ref())
    .bind(&digest)
    .bind(&subject)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("writing the import batch", error))?;
    if inserted.is_none() {
        let winner = batch_by_digest(tx, review_id, &digest)
            .await?
            .ok_or_else(|| {
                Problem::new(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "unprocessable_state",
                    "Unprocessable State",
                    "the import batch disappeared while being written",
                )
            })?;
        return Ok(batch_summary(&winner, true));
    }

    let mut accepted = 0i32;
    let mut quarantined = 0i32;
    for (index, record) in parsed.iter().enumerate() {
        let row_id = new_id();
        let index = i32::try_from(index).map_err(|_| Problem::validation("too many rows"))?;
        if let Some(reason) = record.quarantine_reason.as_deref() {
            sqlx::query(
                "INSERT INTO import_rows
                     (import_row_id, import_batch_id, review_id, tenant_id, row_index,
                      raw_representation, status, quarantine_reason)
                 VALUES ($1,$2,$3,$4,$5,$6,'quarantined',$7)",
            )
            .bind(row_id)
            .bind(batch_id)
            .bind(review_id)
            .bind(actor.tenant)
            .bind(index)
            .bind(&record.raw)
            .bind(reason)
            .execute(&mut **tx)
            .await
            .map_err(|error| Problem::from_db("writing a quarantined import row", error))?;
            quarantined += 1;
            continue;
        }

        let record_id = new_id();
        sqlx::query(
            "INSERT INTO import_rows
                 (import_row_id, import_batch_id, review_id, tenant_id, row_index,
                  raw_representation, status, review_record_id)
             VALUES ($1,$2,$3,$4,$5,$6,'accepted',$7)",
        )
        .bind(row_id)
        .bind(batch_id)
        .bind(review_id)
        .bind(actor.tenant)
        .bind(index)
        .bind(&record.raw)
        .bind(record_id)
        .execute(&mut **tx)
        .await
        .map_err(|error| Problem::from_db("writing an accepted import row", error))?;
        sqlx::query(
            "INSERT INTO review_records (review_record_id, review_id, tenant_id) VALUES ($1,$2,$3)",
        )
        .bind(record_id)
        .bind(review_id)
        .bind(actor.tenant)
        .execute(&mut **tx)
        .await
        .map_err(|error| Problem::from_db("writing the record identity", error))?;

        let content = record.content();
        let content_hash = hash_value(&content);
        let version_id = new_id();
        sqlx::query(
            "INSERT INTO review_record_versions
                 (review_record_version_id, review_record_id, review_id, tenant_id, version_number,
                  schema_ref, content_hash, record_type, title, abstract, publication_year, authors,
                  container_title, doi, pmid, pmcid, arxiv_id, url, normalized_title,
                  normalized_doi, normalized_pmid, field_provenance, source_import_row_id,
                  created_by_subject_id)
             VALUES ($1,$2,$3,$4,1,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,
                     $21,$22,$23)",
        )
        .bind(version_id)
        .bind(record_id)
        .bind(review_id)
        .bind(actor.tenant)
        .bind(RECORD_SCHEMA_REF)
        .bind(&content_hash)
        .bind(&record.record_type)
        .bind(&record.title)
        .bind(&record.abstract_)
        .bind(record.publication_year)
        .bind(json!(record.authors))
        .bind(&record.container_title)
        .bind(&record.doi)
        .bind(&record.pmid)
        .bind(&record.pmcid)
        .bind(&record.arxiv_id)
        .bind(&record.url)
        .bind(normalize_text(record.title.as_deref()))
        .bind(normalize_doi(record.doi.as_deref()))
        .bind(normalize_pmid(record.pmid.as_deref()))
        .bind(field_provenance(record, row_id))
        .bind(row_id)
        .bind(&subject)
        .execute(&mut **tx)
        .await
        .map_err(|error| Problem::from_db("writing the record version", error))?;
        accepted += 1;
    }

    // The append-only trigger permits exactly these three columns, and only in the transaction
    // that inserted the batch.
    sqlx::query(
        "UPDATE import_batches
            SET accepted_count = $2, quarantined_count = $3, rejected_count = 0
          WHERE import_batch_id = $1",
    )
    .bind(batch_id)
    .bind(accepted)
    .bind(quarantined)
    .execute(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("completing the import batch tallies", error))?;

    // The event and its cause reach the database in one transaction; nothing relays the outbox.
    sqlx::query(
        "INSERT INTO domain_outbox
             (event_id, event_type, schema_ref, producer, resource_scope, tenant_id,
              aggregate_type, aggregate_id, aggregate_version, payload)
         VALUES ($1,'review.import.completed.v1','urn:ahtapol:research-event-envelope:1',
                 'evidence-review','tenant',$2,'import_batch',$3,1,$4)",
    )
    .bind(new_id())
    .bind(actor.tenant)
    .bind(batch_id)
    .bind(json!({
        "review_id": review_id.to_string(),
        "import_batch_id": batch_id.to_string(),
        "accepted_count": accepted,
        "rejected_count": 0,
        "quarantined_count": quarantined,
        "raw_payload_digest": digest,
    }))
    .execute(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("emitting the import-completed event", error))?;

    let batch = batch_by_id(tx, batch_id).await?.ok_or_else(|| {
        Problem::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "unprocessable_state",
            "Unprocessable State",
            "the import batch disappeared while being written",
        )
    })?;
    Ok(batch_summary(&batch, false))
}

/// The tail of every record's version chain, in this review.
///
/// "Tail" is `version_number = max(version_number)` per record; the unique
/// `(review_record_id, version_number)` makes the max form exact. Ordered by version id so
/// `limit`/`offset` pagination is stable across calls.
pub async fn current_record_version_ids(
    tx: &mut Transaction<'_, Postgres>,
    review_id: Uuid,
) -> Result<Vec<Uuid>, Problem> {
    sqlx::query_scalar::<_, Uuid>(
        "SELECT v.review_record_version_id
           FROM review_record_versions v
           JOIN (SELECT review_record_id AS rid, max(version_number) AS tail
                   FROM review_record_versions
                  WHERE review_id = $1
                  GROUP BY review_record_id) t
             ON v.review_record_id = t.rid AND v.version_number = t.tail
          WHERE v.review_id = $1
          ORDER BY v.review_record_version_id",
    )
    .bind(review_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading the current record versions", error))
}

fn record_summary(version_id: Uuid, record_id: Uuid, row: &sqlx::postgres::PgRow) -> Value {
    let text = |name: &str| row.get::<Option<String>, _>(name);
    let year: Option<i32> = row.get("publication_year");
    json!({
        "review_record_id": record_id.to_string(),
        "review_record_version_id": version_id.to_string(),
        "version_number": row.get::<i32, _>("version_number"),
        "record_type": row.get::<String, _>("record_type"),
        "title": text("title"),
        "abstract": text("abstract"),
        "publication_year": year,
        "authors": row.get::<Option<Value>, _>("authors").unwrap_or_else(|| json!([])),
        "container_title": text("container_title"),
        "doi": text("doi"),
        "pmid": text("pmid"),
        "pmcid": text("pmcid"),
        "arxiv_id": text("arxiv_id"),
        "url": text("url"),
        "content_hash": row.get::<String, _>("content_hash"),
        "schema_ref": row.get::<String, _>("schema_ref"),
    })
}

// ── handlers ────────────────────────────────────────────────────────────────────────────────

#[utoipa::path(
    post,
    path = "/api/reviews/{review_id}/imports",
    operation_id = "create_import",
    request_body(content = String, content_type = "application/octet-stream"),
    responses((status = 201, description = "ok"))
)]
pub async fn create_import(
    State(state): State<Shared>,
    actor: Actor,
    Path(review_id): Path<Uuid>,
    Query(query): Query<Value>,
    body: Bytes,
) -> Result<(StatusCode, Json<Value>), Problem> {
    if body.is_empty() {
        return Err(Problem::validation("import body is empty"));
    }

    let mut kind = query_str(&query, "source_kind")
        .or_else(|| query_str(&query, "format"))
        .map(str::to_owned);
    let mut allow_empty = query_bool(&query, "allow_empty");
    let mut payload = body.to_vec();

    // The envelope form. The reference gated this on `Content-Type: application/json`; this
    // handler's extractor set carries no headers, so the gate is structural — a JSON object that
    // names its `source_kind` — which is the same test the reference applied to the decoded body.
    if body.first() == Some(&b'{')
        && let Ok(envelope) = serde_json::from_slice::<Value>(&body)
        && let Some(object) = envelope.as_object()
        && object.contains_key("source_kind")
    {
        if let Some(path) = deeper_than(&envelope, ENVELOPE_DEPTH_BOUND) {
            return Err(Problem::validation(format!(
                "the import envelope nests more than {ENVELOPE_DEPTH_BOUND} levels deep (at \
                 {path}). An import envelope is a flat object with a `source_kind` and a \
                 `payload` in it."
            )));
        }
        // Guarded BEFORE the inner payload is unwrapped, so a forbidden key in the envelope is
        // caught even if the inner payload is clean.
        guard_denylist(&envelope)?;
        kind = Some(match object.get("source_kind") {
            Some(Value::String(text)) => text.clone(),
            Some(other) => other.to_string(),
            None => String::new(),
        });
        allow_empty = allow_empty || object.get("allow_empty").map(truthy).unwrap_or(false);
        let inner = match object.get("payload") {
            None | Some(Value::Null) => {
                return Err(Problem::validation("envelope has no 'payload'"));
            }
            Some(inner) => inner,
        };
        payload = match inner {
            Value::String(text) => text.clone().into_bytes(),
            other => serde_json::to_vec(other).map_err(|error| {
                Problem::validation(format!(
                    "the envelope's 'payload' could not be re-encoded: {error}"
                ))
            })?,
        };
    }

    if let Some(kind) = kind.as_deref() {
        let lowered = kind.to_lowercase();
        if BLOCKED_SOURCE_KINDS.contains(&lowered.as_str()) {
            return Err(parser_problem(unsupported_format_blocker(kind)));
        }
        if VERIFIED_ONLY_SOURCE_KINDS.contains(&lowered.as_str()) {
            return Err(Problem::validation(format!(
                "source_kind {kind:?} cannot be imported from a request body. These bytes would \
                 be digested and sealed as a discovery-engine corpus without this service ever \
                 having asked discovery-engine for them. Name the frozen batch instead; this \
                 port does not serve that route."
            )));
        }
        if !SOURCE_KINDS.contains(&kind) {
            return Err(Problem::validation(format!(
                "source_kind must be one of {}; got {kind:?}",
                SOURCE_KINDS.join(", ")
            )));
        }
    } else {
        return Err(Problem::validation(format!(
            "source_kind must be one of {}; got none",
            SOURCE_KINDS.join(", ")
        )));
    }
    let kind = kind.expect("checked above");

    let pool = pool(&state)?;
    let mut tx = crate::db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the import transaction", error))?;
    load_review(&mut tx, review_id).await?;
    let summary = import_payload(
        &mut tx,
        &actor,
        review_id,
        &kind,
        &payload,
        None,
        allow_empty,
    )
    .await?;
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the import", error))?;
    Ok((StatusCode::CREATED, Json(summary)))
}

#[utoipa::path(get, path = "/api/reviews/{review_id}/imports", operation_id = "list_imports", responses((status = 200, description = "ok")))]
pub async fn list_imports(
    State(state): State<Shared>,
    actor: Actor,
    Path(review_id): Path<Uuid>,
) -> Result<(StatusCode, Json<Value>), Problem> {
    let pool = pool(&state)?;
    let mut tx = crate::db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the imports transaction", error))?;
    load_review(&mut tx, review_id).await?;
    let rows = sqlx::query_as::<_, BatchRow>(
        "SELECT import_batch_id, review_id, source_kind, source_ref, raw_payload_digest,
                accepted_count, quarantined_count, rejected_count, created_at
           FROM import_batches WHERE review_id = $1 ORDER BY created_at",
    )
    .bind(review_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("listing the import batches", error))?;
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the imports read", error))?;
    Ok((
        StatusCode::OK,
        Json(json!({
            "items": rows.iter().map(|batch| batch_summary(batch, false)).collect::<Vec<_>>(),
            "count": rows.len(),
        })),
    ))
}

#[utoipa::path(get, path = "/api/reviews/{review_id}/records", operation_id = "list_records", responses((status = 200, description = "ok")))]
pub async fn list_records(
    State(state): State<Shared>,
    actor: Actor,
    Path(review_id): Path<Uuid>,
    Query(query): Query<Value>,
) -> Result<(StatusCode, Json<Value>), Problem> {
    let limit = query_int(&query, "limit", 100)?;
    let offset = query_int(&query, "offset", 0)?;
    if !(1..=500).contains(&limit) {
        return Err(Problem::validation("limit must be between 1 and 500"));
    }
    if offset < 0 {
        return Err(Problem::validation("offset must be zero or greater"));
    }
    let pool = pool(&state)?;
    let mut tx = crate::db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the records transaction", error))?;
    load_review(&mut tx, review_id).await?;
    let current = current_record_version_ids(&mut tx, review_id).await?;
    let total = current.len();
    let page: Vec<Uuid> = current
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .collect();
    let mut items = Vec::new();
    if !page.is_empty() {
        let rows = sqlx::query(
            "SELECT review_record_version_id, review_record_id, version_number, schema_ref,
                    content_hash, record_type, title, abstract, publication_year, authors,
                    container_title, doi, pmid, pmcid, arxiv_id, url
               FROM review_record_versions
              WHERE review_record_version_id = ANY($1)
              ORDER BY created_at, review_record_version_id",
        )
        .bind(&page)
        .fetch_all(&mut *tx)
        .await
        .map_err(|error| Problem::from_db("reading the record page", error))?;
        for row in rows {
            let version_id: Uuid = row.get("review_record_version_id");
            let record_id: Uuid = row.get("review_record_id");
            items.push(record_summary(version_id, record_id, &row));
        }
    }
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the records read", error))?;
    Ok((
        StatusCode::OK,
        Json(json!({
            "items": items,
            "count": items.len(),
            "total": total,
            "offset": offset,
        })),
    ))
}

#[utoipa::path(get, path = "/api/reviews/{review_id}/records/{review_record_id}", operation_id = "get_record", responses((status = 200, description = "ok")))]
pub async fn get_record(
    State(state): State<Shared>,
    actor: Actor,
    Path((review_id, review_record_id)): Path<(Uuid, Uuid)>,
) -> Result<(StatusCode, Json<Value>), Problem> {
    let pool = pool(&state)?;
    let mut tx = crate::db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the record transaction", error))?;
    load_review(&mut tx, review_id).await?;
    let record = sqlx::query(
        "SELECT review_record_id, review_id, created_at FROM review_records
          WHERE review_record_id = $1",
    )
    .bind(review_record_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("loading the record", error))?;
    let Some(record) = record else {
        return Err(Problem::not_found("no such record for this review"));
    };
    let owner: Uuid = record.get("review_id");
    if owner != review_id {
        return Err(Problem::not_found("no such record for this review"));
    }
    let created_at: DateTime<Utc> = record.get("created_at");

    let rows = sqlx::query(
        "SELECT review_record_version_id, review_record_id, version_number, schema_ref,
                content_hash, record_type, title, abstract, publication_year, authors,
                container_title, doi, pmid, pmcid, arxiv_id, url, supersedes_version_id,
                field_provenance, normalized_title, normalized_doi, normalized_pmid,
                source_import_row_id, created_at
           FROM review_record_versions
          WHERE review_record_id = $1 ORDER BY version_number",
    )
    .bind(review_record_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("reading the record versions", error))?;
    let mut versions = Vec::with_capacity(rows.len());
    for row in &rows {
        let version_id: Uuid = row.get("review_record_version_id");
        let mut summary = record_summary(version_id, review_record_id, row);
        if let Some(object) = summary.as_object_mut() {
            object.insert(
                "supersedes_version_id".to_owned(),
                json!(
                    row.get::<Option<Uuid>, _>("supersedes_version_id")
                        .map(|id| id.to_string())
                ),
            );
            object.insert(
                "field_provenance".to_owned(),
                row.get::<Value, _>("field_provenance"),
            );
            object.insert(
                "normalized_title".to_owned(),
                json!(row.get::<Option<String>, _>("normalized_title")),
            );
            object.insert(
                "normalized_doi".to_owned(),
                json!(row.get::<Option<String>, _>("normalized_doi")),
            );
            object.insert(
                "normalized_pmid".to_owned(),
                json!(row.get::<Option<String>, _>("normalized_pmid")),
            );
            object.insert(
                "source_import_row_id".to_owned(),
                json!(
                    row.get::<Option<Uuid>, _>("source_import_row_id")
                        .map(|id| id.to_string())
                ),
            );
            object.insert(
                "created_at".to_owned(),
                json!(iso(row.get::<DateTime<Utc>, _>("created_at"))),
            );
        }
        versions.push(summary);
    }
    let current_version_number = rows.last().map(|row| row.get::<i32, _>("version_number"));
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the record read", error))?;
    Ok((
        StatusCode::OK,
        Json(json!({
            "review_record_id": review_record_id.to_string(),
            "review_id": review_id.to_string(),
            "created_at": iso(created_at),
            "current_version_number": current_version_number,
            "versions": versions,
            "version_count": versions.len(),
        })),
    ))
}

#[utoipa::path(get, path = "/api/reviews/{review_id}/quarantine", operation_id = "list_quarantined_rows", responses((status = 200, description = "ok")))]
pub async fn list_quarantine(
    State(state): State<Shared>,
    actor: Actor,
    Path(review_id): Path<Uuid>,
) -> Result<(StatusCode, Json<Value>), Problem> {
    let pool = pool(&state)?;
    let mut tx = crate::db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the quarantine transaction", error))?;
    load_review(&mut tx, review_id).await?;
    let rows = sqlx::query(
        "SELECT import_row_id, import_batch_id, row_index, quarantine_reason, raw_representation
           FROM import_rows
          WHERE review_id = $1 AND status = 'quarantined'
          ORDER BY created_at",
    )
    .bind(review_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("listing the quarantined rows", error))?;
    let items: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "import_row_id": row.get::<Uuid, _>("import_row_id").to_string(),
                "import_batch_id": row.get::<Uuid, _>("import_batch_id").to_string(),
                "row_index": row.get::<i32, _>("row_index"),
                "quarantine_reason": row.get::<Option<String>, _>("quarantine_reason"),
                "raw_representation": row.get::<Value, _>("raw_representation"),
            })
        })
        .collect();
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the quarantine read", error))?;
    Ok((
        StatusCode::OK,
        Json(json!({ "items": items, "count": items.len() })),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_guarded_key_is_refused_in_every_spelling_and_at_any_depth() {
        for key in [
            "work_type",
            "workType",
            "WorkType",
            "x_work_type",
            "work type",
        ] {
            let payload = json!({"value": {key: "regulatory_instrument"}});
            let hits = find_forbidden_keys(&payload);
            assert_eq!(hits.len(), 1, "{key} must be caught");
            assert_eq!(hits[0], format!("$.value.{key}"));
        }
        assert!(find_forbidden_keys(&json!({"title": "a legal event"})).is_empty());
    }

    #[test]
    fn a_guarded_discriminator_is_a_400_with_its_paths() {
        let problem = guard_denylist(&json!({"work_type": "x"})).expect_err("refused");
        assert_eq!(problem.status, StatusCode::BAD_REQUEST);
        assert_eq!(problem.code, "forbidden_discriminator");
    }

    #[test]
    fn identifiers_are_extracted_before_they_are_judged() {
        assert_eq!(
            normalize_pmid(Some("33933206")).as_deref(),
            Some("33933206")
        );
        assert_eq!(
            normalize_pmid(Some("PMID: 33933206 [PubMed - indexed for MEDLINE]")).as_deref(),
            Some("33933206")
        );
        assert_eq!(
            normalize_pmid(Some("https://pubmed.ncbi.nlm.nih.gov/33933206/")).as_deref(),
            Some("33933206")
        );
        assert_eq!(
            normalize_pmid(Some("33,933,206")).as_deref(),
            Some("33933206")
        );
        assert_eq!(normalize_pmid(Some("PMC3312970")), None);
        assert_eq!(normalize_pmid(Some("2019123456")), None);
        assert_eq!(
            normalize_doi(Some("https://doi.org/10.1000/ABC")).as_deref(),
            Some("10.1000/abc")
        );
        assert_eq!(normalize_doi(Some("NA")), None);
        assert_eq!(
            normalize_pmcid(Some("pmc 8123456")).as_deref(),
            Some("PMC8123456")
        );
        assert_eq!(normalize_year_text("c2019"), Some(2019));
        assert_eq!(normalize_year_text("9999"), None);
    }

    #[test]
    fn a_ris_entry_is_read_and_a_missing_title_is_quarantined() {
        let payload = b"TY  - JOUR\nTI  - A paper\nAU  - Doe, Jane\nPY  - 2019\nER  - \n\
                        TY  - JOUR\nAB  - no title here\nER  - \n";
        let records = parse_ris(payload).expect("parsed");
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].title.as_deref(), Some("A paper"));
        assert_eq!(records[0].record_type, "journal_article");
        assert_eq!(records[0].publication_year, Some(2019));
        assert!(records[1].quarantine_reason.is_some());
        assert!(parse_ris(b"%PDF-1.7").is_err());
    }

    #[test]
    fn a_ris_continuation_line_belongs_to_its_tag() {
        let payload = b"TY  - JOUR\nTI  - A very\n      long title\nER  - \n";
        let records = parse_ris(payload).expect("parsed");
        assert_eq!(records[0].title.as_deref(), Some("A very long title"));
    }

    #[test]
    fn a_csv_with_no_recognised_header_is_refused_rather_than_imported() {
        assert!(parse_csv(b"%PDF-1.7\n1 0 obj\n").is_err());
        let records =
            parse_csv(b"Title,Authors,Year\n\"A, quoted title\",Doe;Roe,2021\n").expect("parsed");
        assert_eq!(records[0].title.as_deref(), Some("A, quoted title"));
        assert_eq!(records[0].authors, vec!["Doe".to_owned(), "Roe".to_owned()]);
        assert_eq!(records[0].publication_year, Some(2021));
    }

    #[test]
    fn bibtex_entries_are_read_and_brace_protection_is_removed() {
        let payload = br#"@article{key1,
            title = {The {DNA} Helix},
            author = {Doe, Jane and Roe, John},
            year = {2020},
            journal = {Nature}
        }"#;
        let records = parse_bibtex(payload).expect("parsed");
        assert_eq!(records[0].title.as_deref(), Some("The DNA Helix"));
        assert_eq!(records[0].authors.len(), 2);
        assert_eq!(records[0].record_type, "journal_article");
    }

    #[test]
    fn canonical_json_reads_nested_identifiers_and_refuses_nul() {
        let payload = br#"{"records": [{"title": "A paper",
            "external_ids": {"doi": "10.1000/x", "pubmed_id": 33933206}}]}"#;
        let records = parse_canonical_json(payload).expect("parsed");
        assert_eq!(records[0].doi.as_deref(), Some("10.1000/x"));
        assert_eq!(records[0].pmid.as_deref(), Some("33933206"));
        assert_eq!(
            records[0].field_sources.get("doi").map(String::as_str),
            Some("external_ids.doi")
        );

        assert!(parse_canonical_json(b"[\"only a string\"]").is_ok());
        assert!(parse_canonical_json(b"{\"records\": [\"a\"]}").is_ok());
        assert!(parse_canonical_json(b"{\"a\": \"\\u0000\"}").is_err());
        assert!(parse_canonical_json(b"{\"records\": 7}").is_err());
    }

    #[test]
    fn a_record_type_outside_the_vocabulary_is_annotated_not_lost() {
        let (stored, annotation) = canonical_record_type(Some("systematic_review"));
        assert_eq!(stored, "other");
        assert_eq!(annotation, vec!["record_type=systematic_review".to_owned()]);
        assert_eq!(
            canonical_record_type(Some("journal-article")).0,
            "journal_article"
        );
    }

    #[test]
    fn an_empty_payload_is_a_typed_refusal_not_a_batch_with_zero_records() {
        assert!(matches!(parse_ris(b""), Err(ParseError::Unreadable(_))));
        assert!(matches!(parse_bibtex(b""), Err(ParseError::Unreadable(_))));
    }
}
