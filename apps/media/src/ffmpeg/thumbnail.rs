//! `media.thumbnail` — one frame of a video, extracted at a chosen time.

use serde_json::Value;

use crate::common::{CapabilityError, Config};

/// The kind slug this module implements.
pub const KIND: &str = "media.thumbnail";

/// Extract one thumbnail.
pub async fn thumbnail(_params: &Value, _config: &Config) -> Result<Value, CapabilityError> {
    Err(CapabilityError::refused(
        "not_implemented",
        "media.thumbnail is not implemented yet",
    ))
}
