#![forbid(unsafe_code)]
//! The inference-ledger read surface (S16): `inference_calls.list` and `inference_usage.report`.
//!
//! Both are ordinary read actions over [`conveyor_store::inference`]; neither writes the ledger —
//! that is the engine's job at the call seam (CONTRACT §18), so no action can record a call that
//! was never made.

pub mod calls;
pub mod usage;
mod views;

pub use views::{InferenceCallView, InferenceCallsListIn, InferenceUsageReportIn, UsageReportOut};

use crate::invoke::ActionError;
use crate::spec::FieldError;

/// A 422 with one field error, matching the surface's other modules.
pub(crate) fn invalid(field: &str, code: &str, detail: impl Into<String>) -> ActionError {
    ActionError::invalid_input(vec![FieldError {
        field: field.to_owned(),
        code: code.to_owned(),
        detail: detail.into(),
    }])
}
