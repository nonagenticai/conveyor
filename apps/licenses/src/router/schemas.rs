//! Schemas the captured document carries because FastAPI emits them, not because this service
//! produces them.
//!
//! `422 HTTPValidationError` is on every operation with a request body. The Rust service rejects a
//! malformed body with axum's own rejection (see the tolerance note in `tokens.rs`), but the
//! published contract must still describe the captured 422 body: `--check-lock` compares the
//! response schemas, and dropping the entry would be document drift, not a simplification.

use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Serialize, ToSchema)]
pub struct ValidationError {
    /// The location of the error: a path/body index chain.
    pub loc: Vec<serde_json::Value>,
    /// The human-readable message.
    pub msg: String,
    /// The error type.
    #[serde(rename = "type")]
    #[schema(rename = "type")]
    pub kind: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HTTPValidationError {
    pub detail: Option<Vec<ValidationError>>,
}
