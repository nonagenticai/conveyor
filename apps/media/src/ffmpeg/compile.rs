//! `media.compile_video` — shots to one encoded video.
//!
//! The reference's own request document is the schema: `CompileRequest` in
//! `contract/video_compiler_service/openapi.lock.json`, with `shots` carrying thirty fields each.
//! Assembly, retime and concat are ffmpeg child processes; a path this port does not implement is
//! refused **by name**, never accepted and ignored.

use serde_json::Value;

use crate::common::{CapabilityError, Config};

/// The kind slug this module implements.
pub const KIND: &str = "media.compile_video";

/// Compile one request into the video it names.
pub async fn compile(_params: &Value, _config: &Config) -> Result<Value, CapabilityError> {
    Err(CapabilityError::refused(
        "not_implemented",
        "media.compile_video is not implemented yet",
    ))
}
