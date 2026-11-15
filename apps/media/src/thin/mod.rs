//! The thin binary: `tts_gateway`, `youtube_upload_service`.
//!
//! Neither needs a browser or a media toolchain: they hold a job/store row and call somebody else
//! over HTTP. The model backend behind TTS stays a remote host (ADR 0006 §Consequences); the
//! Google API behind YouTube is reached per request and is never exercised by a capture.

use axum::Router;

use crate::common::health;
use crate::contract::Lock;

/// The name this binary reports on `/health`.
pub const SERVICE: &str = "conveyor-media-thin";

pub const DEFAULT_PORT: u16 = 8813;

/// The captured contracts this binary must serve.
pub const LOCKS: &[Lock] = &[Lock {
    service: "tts_gateway",
    json: include_str!("../../contract/tts_gateway/openapi.lock.json"),
}];

/// The operations this binary actually serves, in `METHOD path` spelling.
pub const SERVED: &[(&str, &str)] = &[
    ("GET", "/health"),
    ("GET", "/livez"),
    ("GET", "/readyz"),
];

/// The HTTP surface.
pub fn router() -> Router {
    health::router(SERVICE)
}
