//! Canonical JSON, content hashing and the revision diff (S07 §5.3).
//!
//! Every governed-asset mutation stores its post-mutation snapshot in `revision.content` and
//! the SHA-256 of the *canonical* form of that snapshot in `revision.content_hash`. The hash
//! is what makes two writers of the same document agree, so the canonical form is pinned here
//! and nowhere else.
//!
//! [`canonical_json`] implements the subset of RFC 8785 (JCS) this estate needs:
//!
//! - object keys are sorted by **UTF-16 code unit**, not by Unicode scalar value (the two
//!   disagree for astral-plane keys, so the comparison is explicit);
//! - no whitespace anywhere;
//! - strings are escaped exactly as `serde_json` escapes them;
//! - numbers are rendered by `serde_json`'s `Number` formatter. **This is a documented
//!   limitation, not a bug to "fix" here:** [`canonical_json`] does not normalise floats
//!   beyond what `serde_json` produces (`1.0` renders as `1.0`, where strict JCS would emit
//!   `1`), and it does not converge `-0.0`, very large integers or exponent spelling. Two
//!   values that `serde_json` renders identically canonicalise identically — which is the
//!   property the revision hash depends on — but the output is not byte-for-byte RFC 8785
//!   for every possible number. Snapshots are written through `serde_json::Value`, so every
//!   writer shares this rendering.
//!
//! [`diff`] is the read half of the revision model: two `revision.content` documents produce
//! a field-level `added`/`removed`/`changed` report. Array values whose elements are objects
//! carrying a string `id` (an `acceptance` list) are matched **by id**, so renaming or
//! reordering a criterion is one `changed` entry rather than a positional churn.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Render `value` as canonical JSON (the RFC 8785 subset described in the module docs).
///
/// Deterministic: equal values always produce the same string, independent of the order in
/// which an object's keys were inserted.
pub fn canonical_json(value: &Value) -> String {
    let mut out = String::new();
    write_canonical(&mut out, value);
    out
}

/// SHA-256 of [`canonical_json`]`(value)`, lowercase hex.
///
/// This is the single hashing function for `revision.content_hash`; the store and any later
/// verifier call it rather than carrying a second implementation.
pub fn content_hash(value: &Value) -> String {
    let digest = Sha256::digest(canonical_json(value).as_bytes());
    let mut hash = String::with_capacity(64);
    for byte in digest.iter() {
        use std::fmt::Write as _;
        let _ = write!(hash, "{byte:02x}");
    }
    hash
}

fn write_canonical(out: &mut String, value: &Value) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(number) => out.push_str(&number.to_string()),
        Value::String(text) => {
            let encoded = serde_json::to_string(text).expect("a JSON string always serialises");
            out.push_str(&encoded);
        }
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_canonical(out, item);
            }
            out.push(']');
        }
        Value::Object(object) => {
            let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
            keys.sort_by(|left, right| utf16_cmp(left, right));
            out.push('{');
            for (index, key) in keys.into_iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                let encoded = serde_json::to_string(key).expect("a JSON key always serialises");
                out.push_str(&encoded);
                out.push(':');
                let value = object
                    .get(key)
                    .expect("the key was read from this same object");
                write_canonical(out, value);
            }
            out.push('}');
        }
    }
}

fn utf16_cmp(left: &str, right: &str) -> Ordering {
    left.encode_utf16().cmp(right.encode_utf16())
}

/// One field that differs between two revision snapshots.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FieldChange {
    /// The field path: a top-level key, or `"<key>.<id>"` for an id-matched array element.
    pub field: String,
    /// The value in the older revision; `None` when the field was added.
    pub from: Option<Value>,
    /// The value in the newer revision; `None` when the field was removed.
    pub to: Option<Value>,
}

/// The field-level difference between two revision snapshots.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RevisionDiff {
    /// Fields present only in the newer revision.
    pub added: Vec<FieldChange>,
    /// Fields present only in the older revision.
    pub removed: Vec<FieldChange>,
    /// Fields present in both whose values differ.
    pub changed: Vec<FieldChange>,
}

/// Diff two revision snapshots.
///
/// Both documents are expected to be JSON objects (`revision.content` always is). Two
/// non-object documents are compared whole: a difference becomes one `changed` entry with the
/// empty field name, because there is no key to name.
///
/// When both sides of a top-level key are arrays of objects carrying unique string `id`s, the
/// elements are matched by id and reported as `"<key>.<id>"`. Anything else — a scalar, an
/// array of scalars, an array without ids, or an array with duplicate ids — is compared as a
/// whole value under its key.
pub fn diff(from: &Value, to: &Value) -> RevisionDiff {
    let (Value::Object(from_object), Value::Object(to_object)) = (from, to) else {
        if from == to {
            return RevisionDiff::default();
        }
        return RevisionDiff {
            added: Vec::new(),
            removed: Vec::new(),
            changed: vec![FieldChange {
                field: String::new(),
                from: Some(from.clone()),
                to: Some(to.clone()),
            }],
        };
    };

    let mut keys: Vec<&str> = from_object
        .keys()
        .chain(to_object.keys())
        .map(String::as_str)
        .collect();
    keys.sort_by(|left, right| utf16_cmp(left, right));
    keys.dedup();

    let mut diff = RevisionDiff::default();
    for key in keys {
        match (from_object.get(key), to_object.get(key)) {
            (Some(left), Some(right)) => {
                if left != right {
                    diff_value(key, left, right, &mut diff);
                }
            }
            (Some(left), None) => diff.removed.push(FieldChange {
                field: key.to_owned(),
                from: Some(left.clone()),
                to: None,
            }),
            (None, Some(right)) => diff.added.push(FieldChange {
                field: key.to_owned(),
                from: None,
                to: Some(right.clone()),
            }),
            (None, None) => {}
        }
    }
    diff
}

fn diff_value(key: &str, from: &Value, to: &Value, diff: &mut RevisionDiff) {
    if let Some(indexed) = index_by_id(from, to) {
        let (from_elements, to_elements) = indexed;
        for (id, left) in &from_elements {
            match to_elements.get(id) {
                None => diff.removed.push(FieldChange {
                    field: format!("{key}.{id}"),
                    from: Some((*left).clone()),
                    to: None,
                }),
                Some(right) if *right != *left => diff.changed.push(FieldChange {
                    field: format!("{key}.{id}"),
                    from: Some((*left).clone()),
                    to: Some((*right).clone()),
                }),
                Some(_) => {}
            }
        }
        for (id, right) in &to_elements {
            if !from_elements.contains_key(id) {
                diff.added.push(FieldChange {
                    field: format!("{key}.{id}"),
                    from: None,
                    to: Some((*right).clone()),
                });
            }
        }
        return;
    }
    diff.changed.push(FieldChange {
        field: key.to_owned(),
        from: Some(from.clone()),
        to: Some(to.clone()),
    });
}

type IdIndex<'a> = std::collections::BTreeMap<String, &'a Value>;

/// Index both arrays by their elements' string `id`, or `None` when the pair cannot be
/// matched that way.
fn index_by_id<'a>(from: &'a Value, to: &'a Value) -> Option<(IdIndex<'a>, IdIndex<'a>)> {
    let (Value::Array(from_items), Value::Array(to_items)) = (from, to) else {
        return None;
    };
    let from_elements = index_elements(from_items)?;
    let to_elements = index_elements(to_items)?;
    Some((from_elements, to_elements))
}

fn index_elements(items: &[Value]) -> Option<IdIndex<'_>> {
    let mut index = IdIndex::new();
    for item in items {
        let Value::Object(object) = item else {
            return None;
        };
        let Some(Value::String(id)) = object.get("id") else {
            return None;
        };
        // A duplicated id is not an identity; fall back to a whole-value comparison.
        if index.insert(id.clone(), item).is_some() {
            return None;
        }
    }
    Some(index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn canonical_json_sorts_keys_and_emits_no_whitespace() {
        let value = json!({"b": 1, "a": {"z": [1, 2], "y": true}});
        assert_eq!(
            canonical_json(&value),
            r#"{"a":{"y":true,"z":[1,2]},"b":1}"#
        );
    }

    /// RFC 8785 sorts by UTF-16 code unit. The astral-plane key (a surrogate pair, lead
    /// `0xD83D`) sorts *before* U+FF00, which is the opposite of code-point/UTF-8 order.
    /// A `str` comparison would put U+FF00 first and fail this test.
    #[test]
    fn keys_sort_by_utf16_code_unit_not_by_code_point() {
        let emoji = "\u{1F600}";
        let fullwidth = "\u{FF00}";
        let value = json!({fullwidth: 1, emoji: 2});
        assert_eq!(
            canonical_json(&value),
            format!("{{\"{emoji}\":2,\"{fullwidth}\":1}}")
        );
    }

    #[test]
    fn reordered_object_keys_hash_identically() {
        // `serde_json::Value` keeps its objects sorted, so "insertion order" is built here by
        // constructing both spellings; the assertion is the contract a `preserve_order`
        // build must still honour.
        let left = json!({"title": "A requirement", "status": "draft", "version": 1});
        let right = json!({"version": 1, "title": "A requirement", "status": "draft"});
        assert_eq!(canonical_json(&left), canonical_json(&right));
        assert_eq!(content_hash(&left), content_hash(&right));
    }

    #[test]
    fn content_hash_is_the_lowercase_hex_sha256_of_the_canonical_form() {
        assert_eq!(
            content_hash(&json!({"a": 1})),
            "015abd7f5cc57a2dd94b7590f04ad8084273905ee33ec5cebeae62276a97f862"
        );
        assert_eq!(
            content_hash(&json!({"b": [true, null, "x"], "a": 1})),
            "eca8cfb31ab74533e1eb2f4c74d2d55dfe3c79ac704787e54be8647ea7777eb1"
        );
        let hash = content_hash(&json!({"title": "x"}));
        assert_eq!(hash.len(), 64);
        assert!(
            hash.chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
            "lowercase hex: {hash}"
        );
    }

    #[test]
    fn a_status_change_is_one_changed_field() {
        let id = "0195f48b-d000-7000-8000-000000000001";
        let from = json!({"id": id, "title": "A requirement", "status": "draft"});
        let to = json!({"id": id, "title": "A requirement", "status": "in_review"});
        let change = diff(&from, &to);
        assert!(change.added.is_empty(), "{:?}", change.added);
        assert!(change.removed.is_empty(), "{:?}", change.removed);
        assert_eq!(change.changed.len(), 1, "{:?}", change.changed);
        assert_eq!(change.changed[0].field, "status");
        assert_eq!(change.changed[0].from, Some(json!("draft")));
        assert_eq!(change.changed[0].to, Some(json!("in_review")));
    }

    #[test]
    fn acceptance_is_matched_by_id_across_a_reorder_and_an_edit() {
        let from = json!({
            "acceptance": [
                {"id": "a1", "text": "Given a board, when a card moves, then it appears"},
                {"id": "a2", "text": "The list is sorted"}
            ]
        });
        let to = json!({
            "acceptance": [
                {"id": "a2", "text": "The list is sorted"},
                {"id": "a1", "text": "Given a board, when a card moves, then it appears in order"}
            ]
        });
        let change = diff(&from, &to);
        assert!(change.added.is_empty(), "{:?}", change.added);
        assert!(change.removed.is_empty(), "{:?}", change.removed);
        assert_eq!(change.changed.len(), 1, "{:?}", change.changed);
        assert_eq!(change.changed[0].field, "acceptance.a1");
        assert_eq!(
            change.changed[0].to,
            Some(json!({
                "id": "a1",
                "text": "Given a board, when a card moves, then it appears in order"
            }))
        );
    }

    #[test]
    fn an_id_matched_array_reports_added_and_removed_elements() {
        let from = json!({"acceptance": [{"id": "a1", "text": "one"}]});
        let to = json!({"acceptance": [{"id": "a1", "text": "one"}, {"id": "a2", "text": "two"}]});
        let forward = diff(&from, &to);
        assert!(forward.changed.is_empty());
        assert_eq!(forward.added.len(), 1);
        assert_eq!(forward.added[0].field, "acceptance.a2");
        assert_eq!(forward.added[0].from, None);

        let reverse = diff(&to, &from);
        assert!(reverse.changed.is_empty());
        assert_eq!(reverse.removed.len(), 1);
        assert_eq!(reverse.removed[0].field, "acceptance.a2");
        assert_eq!(reverse.removed[0].to, None);
    }

    #[test]
    fn an_array_without_ids_is_compared_whole() {
        let from = json!({"acceptance": ["one", "two"]});
        let to = json!({"acceptance": ["two", "one"]});
        let change = diff(&from, &to);
        assert_eq!(change.changed.len(), 1);
        assert_eq!(change.changed[0].field, "acceptance");
    }

    #[test]
    fn duplicate_ids_fall_back_to_a_whole_value_comparison() {
        let from =
            json!({"acceptance": [{"id": "a1", "text": "one"}, {"id": "a1", "text": "two"}]});
        let to = json!({"acceptance": [{"id": "a1", "text": "two"}, {"id": "a1", "text": "one"}]});
        let change = diff(&from, &to);
        assert_eq!(change.changed.len(), 1);
        assert_eq!(change.changed[0].field, "acceptance");
    }

    #[test]
    fn top_level_additions_and_removals_are_reported() {
        let change = diff(&json!({"a": 1, "b": 2}), &json!({"a": 1, "c": 3}));
        assert_eq!(change.removed.len(), 1);
        assert_eq!(change.removed[0].field, "b");
        assert_eq!(change.removed[0].to, None);
        assert_eq!(change.added.len(), 1);
        assert_eq!(change.added[0].field, "c");
        assert_eq!(change.added[0].from, None);
        assert!(change.changed.is_empty());
    }

    #[test]
    fn identical_documents_have_an_empty_diff() {
        let document = json!({"id": "x", "acceptance": [{"id": "a1", "text": "one"}]});
        assert_eq!(diff(&document, &document), RevisionDiff::default());
    }

    #[test]
    fn two_non_object_documents_compare_whole() {
        let change = diff(&json!([1, 2]), &json!([2, 1]));
        assert_eq!(change.changed.len(), 1);
        assert_eq!(change.changed[0].field, "");
        assert_eq!(diff(&json!(1), &json!(1)), RevisionDiff::default());
    }
}
