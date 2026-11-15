//! `media.record_browser` — a browser session captured into a video.
//!
//! The reference records a live Playwright context for the duration of the session. The port's
//! mechanism and the exact deviation are decided with the capture in
//! `contract/video_production_service/README.md`.

use serde_json::Value;

use crate::common::{CapabilityError, Config};

/// The kind slug this module implements.
pub const KIND: &str = "media.record_browser";

/// Record one session into a video.
pub async fn record(_params: &Value, _config: &Config) -> Result<Value, CapabilityError> {
    Err(CapabilityError::refused(
        "not_implemented",
        "media.record_browser is not implemented yet",
    ))
}
