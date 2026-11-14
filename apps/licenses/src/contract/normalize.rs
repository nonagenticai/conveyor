//! The canonical projection the lock check compares, and the diff it reports.
//!
//! The capture is FastAPI's document; the service publishes utoipa's. They describe the same wire
//! but differ in representation, so the comparison runs on a projection. The projection is
//! **shrink-only**: every transform below removes documentation metadata or a representation
//! difference that has no wire meaning. Nothing else is ignored — paths, methods, status codes,
//! parameters (name/in/required), request and response media types, property names, required
//! sets, types, formats, enums, bounds and defaults must all agree.
//!
//! The complete transform list:
//!
//! 1. `info`, `servers`, `tags`, `externalDocs` and per-operation `operationId`/`summary`/
//!    `description`/`tags` are documentation, not wire.
//! 2. `title`, `description`, `example`, `examples` on schemas are documentation.
//! 3. Nullability has three spellings — `{"type":[T,"null"]}`, `{"nullable":true}`, and
//!    `{"anyOf":[T,{"type":"null"}]}` — and is normalized to the union form. utoipa emits the
//!    first for `Option<T>`; Pydantic emits the third.
//! 4. `oneOf` is treated as `anyOf`. The capture contains no `oneOf`; if utoipa ever emits one, the
//!    variants are by construction mutually exclusive and the distinction is not observable to a
//!    client that validates responses.
//! 5. `propertyNames` is dropped: JSON object keys are always strings.
//! 6. `additionalProperties` absent, `true` and `{}` all mean "any value" in JSON Schema and are
//!    normalized to `"any"`.
//! 7. Numbers compare by value (`2.0` and `2` are equal).
//! 8. `required` and `enum` are order-insensitive sets.
//! 9. A **header** parameter's schema is not compared — at the wire an HTTP header is always a
//!    string, and requiredness already carries the only contract (present/absent). Path and query
//!    parameters are compared in full, because their schema drives parsing and validation.
//! 10. Numeric `format` (`int32`, `int64`, `float`, `double`) is dropped: JSON numbers carry no
//!     width, and the `type` distinction (`integer` vs `number`) is still compared. `uuid`,
//!     `date-time` and every other format are compared.

use serde_json::{Map, Value, json};

/// Methods a path item may carry, in OpenAPI's spelling.
const METHODS: [&str; 7] = ["get", "put", "post", "delete", "options", "head", "patch"];

/// Project a document to the comparable form.
pub fn normalize(document: &Value) -> Value {
    json!({
        "paths": paths(document),
        "components": components(document),
    })
}

fn paths(document: &Value) -> Value {
    let mut out = Map::new();
    let Some(items) = document.get("paths").and_then(Value::as_object) else {
        return Value::Object(out);
    };
    for (path, item) in items {
        let Some(item) = item.as_object() else {
            continue;
        };
        let mut operations = Map::new();
        for method in METHODS {
            if let Some(operation) = item.get(method) {
                operations.insert(method.to_owned(), self::operation(operation));
            }
        }
        out.insert(path.clone(), Value::Object(operations));
    }
    Value::Object(out)
}

fn operation(operation: &Value) -> Value {
    let mut parameters: Vec<Value> = operation
        .get("parameters")
        .and_then(Value::as_array)
        .map(|items| items.iter().map(parameter).collect())
        .unwrap_or_default();
    parameters.sort_by_key(Value::to_string);
    json!({
        "parameters": parameters,
        "request": request_body(operation.get("requestBody")),
        "responses": responses(operation.get("responses")),
    })
}

fn parameter(parameter: &Value) -> Value {
    let location = parameter.get("in").and_then(Value::as_str).unwrap_or("");
    let mut out = Map::new();
    out.insert(
        "name".to_owned(),
        parameter.get("name").cloned().unwrap_or(Value::Null),
    );
    out.insert("in".to_owned(), json!(location));
    out.insert(
        "required".to_owned(),
        json!(
            parameter
                .get("required")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        ),
    );
    // Rule 9: a header's schema is documentation; path and query schemas are the contract.
    if location != "header"
        && let Some(schema) = parameter.get("schema")
    {
        out.insert("schema".to_owned(), self::schema(schema));
    }
    Value::Object(out)
}

fn request_body(body: Option<&Value>) -> Value {
    let Some(body) = body else {
        return Value::Null;
    };
    json!({
        "required": body.get("required").and_then(Value::as_bool).unwrap_or(false),
        "content": content(body.get("content")),
    })
}

fn responses(responses: Option<&Value>) -> Value {
    let mut out = Map::new();
    let Some(items) = responses.and_then(Value::as_object) else {
        return Value::Object(out);
    };
    for (status, response) in items {
        out.insert(
            status.clone(),
            json!({"content": content(response.get("content"))}),
        );
    }
    Value::Object(out)
}

fn content(content: Option<&Value>) -> Value {
    let mut out = Map::new();
    let Some(items) = content.and_then(Value::as_object) else {
        return Value::Object(out);
    };
    for (media, body) in items {
        let schema = body.get("schema").map(schema).unwrap_or_else(any);
        out.insert(media.clone(), schema);
    }
    Value::Object(out)
}

fn any() -> Value {
    json!({"any": true})
}

fn schema(value: &Value) -> Value {
    if let Some(reference) = value.get("$ref").and_then(Value::as_str) {
        let name = reference.rsplit('/').next().unwrap_or(reference);
        return json!({"ref": name});
    }

    if let Some(items) = value
        .get("anyOf")
        .or_else(|| value.get("oneOf"))
        .and_then(Value::as_array)
    {
        return union(items);
    }

    if value.get("nullable").and_then(Value::as_bool) == Some(true) {
        let mut stripped = value.clone();
        if let Some(object) = stripped.as_object_mut() {
            object.remove("nullable");
        }
        return union(&[stripped, json!({"type": "null"})]);
    }

    match value.get("type") {
        Some(Value::Array(types)) => {
            // `{"type":["array","null"], "items": …}` — the sibling keys belong to the non-null
            // variants; the null variant is bare.
            let variants: Vec<Value> = types
                .iter()
                .filter_map(Value::as_str)
                .map(|kind| {
                    if kind == "null" {
                        json!({"type": "null"})
                    } else {
                        typed(kind, value)
                    }
                })
                .collect();
            union_of(variants)
        }
        Some(Value::String(kind)) => typed(kind, value),
        _ => any(),
    }
}

fn typed(kind: &str, value: &Value) -> Value {
    match kind {
        "object" => {
            let mut out = Map::new();
            out.insert("type".to_owned(), json!("object"));
            if let Some(properties) = value.get("properties").and_then(Value::as_object) {
                let mut props = Map::new();
                for (name, property) in properties {
                    props.insert(name.clone(), schema(property));
                }
                out.insert("properties".to_owned(), Value::Object(props));
            }
            let required: Vec<String> = value
                .get("required")
                .and_then(Value::as_array)
                .map(|names| {
                    let mut names: Vec<String> = names
                        .iter()
                        .filter_map(|name| name.as_str().map(str::to_owned))
                        .collect();
                    names.sort();
                    names
                })
                .unwrap_or_default();
            out.insert("required".to_owned(), json!(required));
            // Rule 6: absent == true == {} == "any".
            let additional = match value.get("additionalProperties") {
                None | Some(Value::Bool(true)) => json!("any"),
                Some(Value::Bool(false)) => json!("none"),
                Some(Value::Object(map)) if map.is_empty() => json!("any"),
                Some(other) => schema(other),
            };
            out.insert("additional".to_owned(), additional);
            Value::Object(out)
        }
        "array" => json!({
            "type": "array",
            "items": value.get("items").map(schema).unwrap_or_else(any),
        }),
        other => {
            let mut out = Map::new();
            out.insert("type".to_owned(), json!(other));
            // Rule 10: JSON numbers carry no width. utoipa annotates Rust primitives
            // (`int64`, `double`); Pydantic does not. uuid/date-time and every other format are
            // compared.
            if let Some(format) = value.get("format")
                && !matches!(
                    format.as_str(),
                    Some("int32" | "int64" | "float" | "double")
                )
            {
                out.insert("format".to_owned(), format.clone());
            }
            if let Some(default) = value.get("default") {
                out.insert("default".to_owned(), scalar(default));
            }
            if let Some(field) = value.get("enum").and_then(Value::as_array) {
                let mut values: Vec<Value> = field.iter().map(scalar).collect();
                values.sort_by_key(Value::to_string);
                values.dedup();
                out.insert("enum".to_owned(), json!(values));
            }
            if let Some(field) = value.get("pattern") {
                out.insert("pattern".to_owned(), field.clone());
            }
            for key in [
                "minimum",
                "maximum",
                "exclusiveMinimum",
                "exclusiveMaximum",
                "minLength",
                "maxLength",
                "minItems",
                "maxItems",
                "multipleOf",
            ] {
                if let Some(field) = value.get(key) {
                    out.insert(key.to_owned(), number(field));
                }
            }
            Value::Object(out)
        }
    }
}

fn union(items: &[Value]) -> Value {
    union_of(items.iter().map(schema).collect())
}

/// Flatten, sort and deduplicate variants that are **already normalized**.
///
/// Re-normalizing them here used to run `schema()` over a canonical shape, which silently turned
/// nested `{"ref": …}` and `{"additional": …}` markers into `any` — a projection that hid exactly
/// the differences it exists to report.
fn union_of(variants: Vec<Value>) -> Value {
    let mut flat: Vec<Value> = Vec::new();
    for variant in variants {
        match variant.get("union").and_then(Value::as_array) {
            Some(nested) => flat.extend(nested.iter().cloned()),
            None => flat.push(variant),
        }
    }
    flat.sort_by_key(Value::to_string);
    flat.dedup();
    json!({"union": flat})
}

/// Rule 7: numbers compare by value.
fn number(value: &Value) -> Value {
    match value.as_f64() {
        Some(number) => json!(number),
        None => value.clone(),
    }
}

fn scalar(value: &Value) -> Value {
    match value {
        Value::Number(_) => number(value),
        Value::Array(items) => {
            let mut out: Vec<Value> = items.iter().map(scalar).collect();
            out.sort_by_key(Value::to_string);
            out.dedup();
            json!(out)
        }
        other => other.clone(),
    }
}

fn components(document: &Value) -> Value {
    let mut out = Map::new();
    let Some(schemas) = document
        .get("components")
        .and_then(|components| components.get("schemas"))
        .and_then(Value::as_object)
    else {
        return Value::Object(out);
    };
    for (name, schema_value) in schemas {
        out.insert(name.clone(), schema(schema_value));
    }
    Value::Object(out)
}

/// Every way `actual` differs from `expected`, as JSON-pointer paths.
///
/// Bounded: a document missing entirely would otherwise produce thousands of rows, and the first
/// handful name the cause just as well.
pub fn diff(expected: &Value, actual: &Value) -> Vec<String> {
    let mut out = Vec::new();
    walk("", expected, actual, &mut out, MAX_DIFFS);
    out
}

const MAX_DIFFS: usize = 25;
const MAX_VALUE: usize = 160;

fn walk(path: &str, expected: &Value, actual: &Value, out: &mut Vec<String>, limit: usize) {
    if out.len() >= limit || expected == actual {
        return;
    }
    match (expected, actual) {
        (Value::Object(expected), Value::Object(actual)) => {
            for (key, value) in expected {
                let child = format!("{path}/{key}");
                match actual.get(key) {
                    Some(actual_value) => walk(&child, value, actual_value, out, limit),
                    None => out.push(format!("{child}: missing")),
                }
            }
            for key in actual.keys() {
                if !expected.contains_key(key) {
                    out.push(format!("{path}/{key}: unexpected"));
                }
            }
        }
        (Value::Array(expected), Value::Array(actual)) => {
            if expected.len() != actual.len() {
                out.push(format!(
                    "{path}: expected {} items, served {}",
                    expected.len(),
                    actual.len()
                ));
                return;
            }
            for (index, (value, actual_value)) in expected.iter().zip(actual).enumerate() {
                walk(&format!("{path}/{index}"), value, actual_value, out, limit);
            }
        }
        _ => out.push(format!(
            "{path}: expected {}, served {}",
            render(expected),
            render(actual)
        )),
    }
}

fn render(value: &Value) -> String {
    let mut text = value.to_string();
    if text.len() > MAX_VALUE {
        text.truncate(MAX_VALUE);
        text.push('…');
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nullability_spellings_normalize_to_one_form() {
        let as_array = json!({"type": "string", "nullable": true});
        let as_type_array = json!({"type": ["string", "null"]});
        let as_union = json!({"anyOf": [{"type": "string"}, {"type": "null"}]});
        let expected = union(&[json!({"type": "string"}), json!({"type": "null"})]);
        assert_eq!(schema(&as_array), expected);
        assert_eq!(schema(&as_type_array), expected);
        assert_eq!(schema(&as_union), expected);
    }

    #[test]
    fn additional_properties_absent_true_and_empty_are_the_same() {
        let absent = schema(&json!({"type": "object"}));
        let true_ = schema(&json!({"type": "object", "additionalProperties": true}));
        let empty = schema(&json!({"type": "object", "additionalProperties": {}}));
        assert_eq!(absent, true_);
        assert_eq!(absent, empty);
    }

    #[test]
    fn property_names_and_titles_are_dropped() {
        let with = schema(&json!({
            "type": "object",
            "title": "T",
            "propertyNames": {"type": "string"},
            "properties": {"a": {"type": "string", "title": "A", "description": "x"}},
            "additionalProperties": {"type": "string"}
        }));
        let without = schema(&json!({
            "type": "object",
            "properties": {"a": {"type": "string"}},
            "additionalProperties": {"type": "string"}
        }));
        assert_eq!(with, without);
    }

    #[test]
    fn numbers_compare_by_value() {
        assert_eq!(
            schema(&json!({"type": "number", "maximum": 2.0})),
            schema(&json!({"type": "number", "maximum": 2}))
        );
        // Pydantic prints i64::MAX as a float; the values are the same after the cast.
        assert_eq!(
            schema(&json!({"type": "integer", "maximum": 9.223372036854776e18})),
            schema(&json!({"type": "integer", "maximum": 9223372036854775807i64}))
        );
    }

    #[test]
    fn a_missing_property_is_named() {
        let expected = schema(&json!({"type": "object", "properties": {"a": {"type": "string"}}}));
        let actual = schema(&json!({"type": "object", "properties": {}}));
        let diffs = diff(&expected, &actual);
        assert!(
            diffs
                .iter()
                .any(|row| row.contains("/properties/a: missing")),
            "{diffs:?}"
        );
    }

    #[test]
    fn a_changed_type_is_named() {
        let expected = schema(&json!({"type": "object", "properties": {"a": {"type": "string"}}}));
        let actual = schema(&json!({"type": "object", "properties": {"a": {"type": "integer"}}}));
        let diffs = diff(&expected, &actual);
        assert!(
            diffs.iter().any(|row| row.contains("/properties/a/type")),
            "{diffs:?}"
        );
    }

    #[test]
    fn a_dropped_required_entry_is_named() {
        let expected = schema(
            &json!({"type": "object", "required": ["a", "b"], "properties": {"a": {"type": "string"}, "b": {"type": "string"}}}),
        );
        let actual = schema(
            &json!({"type": "object", "required": ["a"], "properties": {"a": {"type": "string"}, "b": {"type": "string"}}}),
        );
        let diffs = diff(&expected, &actual);
        assert!(
            diffs.iter().any(|row| row.contains("/required")),
            "{diffs:?}"
        );
    }
}
