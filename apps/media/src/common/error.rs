//! What a capability can fail with.
//!
//! A media job's failure is reported to conveyor as a **result**, not as an HTTP error envelope:
//! the executor's only interface is the runner contract (ADR 0007 §4). The variants are the
//! distinctions a caller can act on — refuse the request, retry it, or fail the job — so a
//! transient browser crash is never reported as invalid input, and a missing tool never looks like
//! a bad parameter.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CapabilityError {
    /// The request cannot be served as asked, by design. Never retried.
    #[error("{code}: {message}")]
    Refused { code: String, message: String },

    /// The parameters do not match the kind's schema.
    #[error("invalid parameters: {0}")]
    Invalid(String),

    /// The kind is not implemented by this image. A dispatch error, never a silent no-op.
    #[error("unknown capability kind: {0}")]
    UnknownKind(String),

    /// A tool the image needs is missing (chromium, ffmpeg, tesseract).
    #[error("{tool} is not available: {source}")]
    ToolMissing {
        tool: String,
        #[source]
        source: std::io::Error,
    },

    /// The work started and failed in a way that a fresh attempt may not reproduce.
    #[error("{0}")]
    Failed(String),
}

impl CapabilityError {
    /// A capability this port refuses **by name**, so the caller is told which capability cannot be
    /// served and why — never silently ignored.
    pub fn refused(code: &str, message: impl Into<String>) -> Self {
        Self::Refused {
            code: code.to_string(),
            message: message.into(),
        }
    }
}
