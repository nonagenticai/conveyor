//! The Chromium binary: `html_render_service`, `slide_service`, `frontend_test_runner_service`,
//! `video_production_service`.
//!
//! These four are grouped because they all need a browser, which is the heavy runtime ADR 0006 §2
//! says must not be folded into a service that does not. Rendering is driven over CDP; the
//! Playwright-runner-specific behaviour of `frontend_test_runner_service` is refused by name rather
//! than imitated (see that service's contract README).

use axum::Router;

use crate::common::health;
use crate::contract::Lock;

/// The name this binary reports on `/health`.
pub const SERVICE: &str = "conveyor-media-browser";

/// Chrome's default in the estate's other deployables is the reference's own port; this one is the
/// group's, chosen to be free everywhere (8000 is out of bounds on this estate).
pub const DEFAULT_PORT: u16 = 8811;

/// The captured contracts this binary must serve. One per reference service; filled as each
/// capture lands, and `--check-lock` refuses to report green while it is empty.
pub const LOCKS: &[Lock] = &[];

/// The operations this binary actually serves, in `METHOD path` spelling.
///
/// It must never name a route the router does not answer: an operation added here without a
/// handler is a claim, not a port.
pub const SERVED: &[(&str, &str)] = &[
    ("GET", "/health"),
    ("GET", "/livez"),
    ("GET", "/readyz"),
];

/// The HTTP surface.
pub fn router() -> Router {
    health::router(SERVICE)
}
