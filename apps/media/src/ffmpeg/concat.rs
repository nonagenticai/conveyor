//! `media.concat_audio` — several audio inputs joined into one track.

use serde_json::Value;

use crate::common::{CapabilityError, Config};

/// The kind slug this module implements.
pub const KIND: &str = "media.concat_audio";

/// Concatenate the audio inputs the request names.
pub async fn concat_audio(_params: &Value, _config: &Config) -> Result<Value, CapabilityError> {
    Err(CapabilityError::refused(
        "not_implemented",
        "media.concat_audio is not implemented yet",
    ))
}
