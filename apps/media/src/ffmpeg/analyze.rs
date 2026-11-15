//! `media.analyze_video` and `media.analyze_audio` — the reference's own measurements.
//!
//! Where the reference's arithmetic is simple it is recomputed in Rust over ffprobe/ffmpeg output;
//! where it depends on OpenCV the parameter is refused by name rather than approximated.

use serde_json::Value;

use crate::common::{CapabilityError, Config};

/// The video-analysis kind.
pub const KIND_VIDEO: &str = "media.analyze_video";

/// The audio-analysis kind.
pub const KIND_AUDIO: &str = "media.analyze_audio";

/// Analyse a video.
pub async fn analyze_video(_params: &Value, _config: &Config) -> Result<Value, CapabilityError> {
    Err(CapabilityError::refused(
        "not_implemented",
        "media.analyze_video is not implemented yet",
    ))
}

/// Analyse an audio track.
pub async fn analyze_audio(_params: &Value, _config: &Config) -> Result<Value, CapabilityError> {
    Err(CapabilityError::refused(
        "not_implemented",
        "media.analyze_audio is not implemented yet",
    ))
}
