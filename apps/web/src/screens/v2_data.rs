//! Shared readers for the V2 destination screens.
//!
//! An action output is either a bare array or a `{items: [...]}` page. The screens read both
//! through one pair of helpers, so a list and a page cannot drift into two renderers.

use serde_json::Value;

use crate::api::{ApiError, Client, InvokeOptions};

/// The rows of an action output: `{items: [...]}` (a page) or a bare array.
pub fn rows(value: &Value) -> Vec<Value> {
    value
        .get("items")
        .and_then(Value::as_array)
        .or_else(|| value.as_array())
        .cloned()
        .unwrap_or_default()
}

/// A string field, or `—` when absent or not a string.
pub fn text(row: &Value, key: &str) -> String {
    row.get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| "—".to_owned())
}

/// An integer field, or 0 when absent or not an integer.
pub fn int(row: &Value, key: &str) -> i64 {
    row.get(key).and_then(Value::as_i64).unwrap_or(0)
}

/// The number of items in an array field.
pub fn count(row: &Value, key: &str) -> usize {
    row.get(key)
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0)
}

/// Invoke one read action and return its rows. The caller gates on `has_action` first.
pub async fn read_rows(client: &Client, name: &str, input: Value) -> Result<Vec<Value>, ApiError> {
    let invocation = client.invoke(name, input, InvokeOptions::default()).await?;
    Ok(rows(&invocation.output))
}
