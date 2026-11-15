//! Declarative CSS extraction (`extraction_schema` -> `extracted_data`).
//!
//! Ported from `services/extraction_schema.py`: one record per `baseSelector`
//! match, each field selected relative to its record, with `text`, `attribute`,
//! `html`, `nested`, `list` and `nested_list` semantics and the reference's
//! bounded fan-out.

use std::collections::BTreeMap;

use scraper::{ElementRef, Html, Selector};
use serde_json::Value;

use crate::extract::text_with_separator;
use crate::models::{ExtractionField, ExtractionFieldType, ExtractionSchema, Transform};

const MAX_EXTRACTED_ITEMS: usize = 1_000;
const MAX_EXTRACTED_VALUE_LENGTH: usize = 50_000;
const MAX_EXTRACTED_LIST_ITEMS: usize = 500;

/// Apply a schema to page HTML. An empty input or an unmatched base selector
/// yields an empty vector, which the response keeps distinct from `null`.
pub fn extract(html: &str, schema: &ExtractionSchema) -> Vec<BTreeMap<String, Value>> {
    if html.is_empty() {
        return Vec::new();
    }
    let document = Html::parse_document(html);
    let Ok(base_selector) = Selector::parse(&schema.base_selector) else {
        return Vec::new();
    };

    document
        .select(&base_selector)
        .take(MAX_EXTRACTED_ITEMS)
        .map(|element| extract_item(element, &schema.fields))
        .filter(|record| !record.is_empty())
        .collect()
}

fn extract_item(element: ElementRef<'_>, fields: &[ExtractionField]) -> BTreeMap<String, Value> {
    let mut record = BTreeMap::new();
    for field in fields {
        if let Some(value) = extract_field(element, field) {
            record.insert(field.name.clone(), value);
        }
    }
    record
}

fn extract_field(element: ElementRef<'_>, field: &ExtractionField) -> Option<Value> {
    match field.r#type {
        ExtractionFieldType::Nested => {
            let matches = select(element, field.selector.as_deref());
            match matches.first() {
                Some(nested) => {
                    let record = extract_item(*nested, &field.fields);
                    Some(Value::Object(record.into_iter().collect()))
                }
                None => field.default_value.clone().map(Value::String),
            }
        }
        ExtractionFieldType::List | ExtractionFieldType::NestedList => {
            let matches: Vec<ElementRef<'_>> = select(element, field.selector.as_deref())
                .into_iter()
                .take(MAX_EXTRACTED_LIST_ITEMS)
                .collect();
            let records: Vec<Value> = matches
                .into_iter()
                .map(|matched| {
                    Value::Object(extract_item(matched, &field.fields).into_iter().collect())
                })
                .collect();
            Some(Value::Array(records))
        }
        _ => extract_scalar(element, field),
    }
}

fn extract_scalar(element: ElementRef<'_>, field: &ExtractionField) -> Option<Value> {
    let target = if field
        .selector
        .as_deref()
        .is_some_and(|selector| !selector.is_empty())
    {
        let matches = select(element, field.selector.as_deref());
        match matches.first() {
            Some(target) => *target,
            None => return field.default_value.clone().map(Value::String),
        }
    } else {
        element
    };

    let mut value = match field.r#type {
        ExtractionFieldType::Attribute => target
            .attr(field.attribute.as_deref().unwrap_or_default())
            .map(str::to_string),
        ExtractionFieldType::Html => Some(target.html()),
        _ => Some(text_with_separator(&target, "")),
    };

    if value.is_none() {
        return field.default_value.clone().map(Value::String);
    }

    if let Some(text) = value.as_mut() {
        match field.transform {
            Some(Transform::Lowercase) => *text = text.to_lowercase(),
            Some(Transform::Uppercase) => *text = text.to_uppercase(),
            Some(Transform::Strip) => *text = text.trim().to_string(),
            None => {}
        }
        if text.chars().count() > MAX_EXTRACTED_VALUE_LENGTH {
            *text = text.chars().take(MAX_EXTRACTED_VALUE_LENGTH).collect();
        }
    }

    value.map(Value::String)
}

fn select<'a>(element: ElementRef<'a>, selector: Option<&str>) -> Vec<ElementRef<'a>> {
    let Some(selector) = selector.filter(|selector| !selector.is_empty()) else {
        return Vec::new();
    };
    let Ok(selector) = Selector::parse(selector) else {
        return Vec::new();
    };
    element.select(&selector).collect()
}
