//! Schemas the captured document carries because FastAPI emits them, not because this service
//! produces them.
//!
//! `422 HTTPValidationError` is on every operation with a request body. The Rust service rejects a
//! malformed body with axum's own rejection (see the tolerance note in `tokens.rs`), but the
//! published contract must still describe the captured 422 body: `--check-lock` compares the
//! response schemas, and dropping the entry would be document drift, not a simplification.

use serde::Serialize;
use utoipa::ToSchema;

/// One element of a FastAPI validation `loc`: a field name or an array index.
///
/// The capture's `loc.items` is `anyOf: [{type: "string"}, {type: "integer"}]`. Nothing in the
/// service serialises this schema (it exists for the 422 contract), so a manual `PartialSchema`
/// pins the union exactly rather than inventing a named component the capture does not carry.
pub struct LocItem;

impl utoipa::PartialSchema for LocItem {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        utoipa::openapi::schema::OneOfBuilder::new()
            .item(
                utoipa::openapi::schema::ObjectBuilder::new()
                    .schema_type(utoipa::openapi::schema::Type::String),
            )
            .item(
                utoipa::openapi::schema::ObjectBuilder::new()
                    .schema_type(utoipa::openapi::schema::Type::Integer),
            )
            .into()
    }
}

impl ToSchema for LocItem {}

#[derive(Debug, Serialize, ToSchema)]
pub struct ValidationError {
    /// The location of the error: a path/body index chain of strings and integers.
    #[schema(value_type = Vec<LocItem>, inline)]
    pub loc: Vec<serde_json::Value>,
    /// The human-readable message.
    pub msg: String,
    /// The error type.
    #[serde(rename = "type")]
    #[schema(rename = "type")]
    pub kind: String,
    /// The error context, present for some error types.
    #[schema(value_type = Object, required = false)]
    pub ctx: Option<serde_json::Value>,
    /// The offending input, when FastAPI attaches one; any JSON value, unconstrained.
    #[schema(value_type = Value, required = false)]
    pub input: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HTTPValidationError {
    // The captured model has no default and no `required` entry: the field is optional, but its
    // schema is a plain array, never a nullable union. A bare `Vec` plus `required = false` is
    // the only spelling that produces both facts.
    #[schema(required = false)]
    pub detail: Vec<ValidationError>,
}
