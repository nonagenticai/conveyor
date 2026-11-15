//! The ffmpeg binary: `video_compiler_service`, `video_analyzer_service`.
//!
//! `ffmpeg` is invoked as a child process — there is no Rust decoder in this workspace and there is
//! no reason for one: the reference shells out to the same binary, and its own arguments are what
//! the capture pins.

use axum::Router;

use crate::common::health;
use crate::contract::Lock;

/// The name this binary reports on `/health`.
pub const SERVICE: &str = "conveyor-media-ffmpeg";

pub const DEFAULT_PORT: u16 = 8812;

/// The captured contracts this binary must serve; `--check-lock` refuses a green while it is empty.
pub const LOCKS: &[Lock] = &[];

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
