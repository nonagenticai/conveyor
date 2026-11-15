//! The ffmpeg image: composition/compile, probe/analysis and OCR.
//!
//! The group's ffmpeg capabilities after the drop table: `video_compiler_service`,
//! `video_analyzer_service` (ffprobe metrics and tesseract OCR). `ffmpeg` and `ffprobe` are invoked
//! as child processes — there is no Rust decoder in this workspace and no reason for one; the
//! reference shells out to the same binaries, and its own arguments are what the capture pins.
//!
//! `tts_gateway` and `youtube_upload_service` are **not** here: they are thin, and ADR 0007 §1
//! hosts thin operations in `apps/server` beside the job ledger they already use.

pub mod analyze;
pub mod compile;
pub mod concat;
pub mod ocr;
pub mod probe;
pub mod thumbnail;

use futures_util::future::BoxFuture;
use serde_json::Value;

use crate::common::{CapabilityError, Config};
use crate::contract::Capture;

/// The image name reported by `--list` and in logs.
pub const SERVICE: &str = "conveyor-media-ffmpeg";

/// The capability kinds this image implements. Must equal the union of [`CAPTURES`]' kinds.
pub const KINDS: &[&str] = &[
    compile::KIND,
    concat::KIND,
    probe::KIND,
    thumbnail::KIND,
    analyze::KIND_AUDIO,
    analyze::KIND_VIDEO,
    ocr::KIND,
];

/// The captured contracts whose kinds this image owns.
pub const CAPTURES: &[Capture] = &[
    Capture {
        service: "video_compiler_service",
        lock_json: include_str!("../../contract/video_compiler_service/openapi.lock.json"),
        capabilities: include_str!("../../contract/video_compiler_service/capabilities.txt"),
    },
    Capture {
        service: "video_analyzer_service",
        lock_json: include_str!("../../contract/video_analyzer_service/openapi.lock.json"),
        capabilities: include_str!("../../contract/video_analyzer_service/capabilities.txt"),
    },
];

/// Execute one capability.
pub fn dispatch(
    kind: String,
    params: Value,
    config: Config,
) -> BoxFuture<'static, Result<Value, CapabilityError>> {
    Box::pin(async move {
        match kind.as_str() {
            compile::KIND => compile::compile(&params, &config).await,
            concat::KIND => concat::concat_audio(&params, &config).await,
            probe::KIND => probe::probe(&params, &config).await,
            thumbnail::KIND => thumbnail::thumbnail(&params, &config).await,
            analyze::KIND_AUDIO => analyze::analyze_audio(&params, &config).await,
            analyze::KIND_VIDEO => analyze::analyze_video(&params, &config).await,
            ocr::KIND => ocr::ocr(&params, &config).await,
            other => Err(CapabilityError::UnknownKind(other.to_string())),
        }
    })
}
