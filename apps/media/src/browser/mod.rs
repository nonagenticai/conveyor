//! The Chromium image: page capture and recording, HTML→PDF/PNG, and the native Gherkin runner.
//!
//! These are the group's browser capabilities after the drop table: `html_render_service`,
//! `video_production_service`'s recording paths and `frontend_test_runner_service`. Each is a job
//! kind, not a route — the executor takes one `kind`, its `params`, and writes named outputs
//! (ADR 0007 §3–4). Chromium is driven over CDP; Playwright-runner-specific behaviour is refused
//! by name rather than imitated.
//!
//! `slide_service` is **not** here: ADR 0007 §7 moves deck authoring to `crates/deck`, which emits
//! OOXML and renders PDF/PNG through `crates/render`.

pub mod produce;
pub mod record;
pub mod render;

use futures_util::future::BoxFuture;
use serde_json::Value;

use crate::common::{CapabilityError, Config};
use crate::contract::Capture;

/// The image name reported by `--list` and in logs.
pub const SERVICE: &str = "conveyor-media-browser";

/// The capability kinds this image implements. Must equal the union of [`CAPTURES`]' kinds.
pub const KINDS: &[&str] = &[produce::KIND, record::KIND, render::KIND];

/// The captured contracts whose kinds this image owns.
pub const CAPTURES: &[Capture] = &[
    Capture {
        service: "html_render_service",
        lock_json: include_str!("../../contract/html_render_service/openapi.lock.json"),
        capabilities: include_str!("../../contract/html_render_service/capabilities.txt"),
    },
    Capture {
        service: "video_production_service",
        lock_json: include_str!("../../contract/video_production_service/openapi.lock.json"),
        capabilities: include_str!("../../contract/video_production_service/capabilities.txt"),
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
            produce::KIND => produce::produce(&params, &config).await,
            record::KIND => record::record(&params, &config).await,
            render::KIND => render::render(&params, &config).await,
            other => Err(CapabilityError::UnknownKind(other.to_string())),
        }
    })
}
