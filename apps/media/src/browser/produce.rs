//! `media.produce_video` — a deliverable assembled from explicit shot definitions.
//!
//! The captured `CompileRequest` is the schema; the measured reference path is per-shot PNG render
//! plus ffmpeg encode (its animation recorder seeks a page per frame, screenshots PNGs and encodes
//! them). Nothing here records in real time — that is [`crate::browser::record`].

use serde_json::Value;

use crate::common::{CapabilityError, Config};

/// The kind slug this module implements.
pub const KIND: &str = "media.produce_video";

/// Assemble one video from the shot definitions the request carries.
pub async fn produce(_params: &Value, _config: &Config) -> Result<Value, CapabilityError> {
    Err(CapabilityError::refused(
        "not_implemented",
        "media.produce_video is not implemented yet",
    ))
}
