#![forbid(unsafe_code)]
//! T14 — the estate import wave and its health (SPEC_V2 §11.2, §11.8, §11.9; issue #83).
//!
//! T13 shipped the platform: the four tables, the broker and `tools.call`. This module is the
//! half that makes a broker call possible at all — a **published** toolkit version and its tool
//! rows — plus the two surfaces §11.9 promises over it: a probe, and a zero-use report.
//!
//! * [`publish`] — `toolkits.publish`, the import wave. Content-addressed, immutable versions,
//!   input schemas copied from the registry.
//! * [`probe`] — `tools.probe`, read-only health per destination; a failure disables the
//!   toolkit's bindings so the broker refuses new runs `409 tool_not_ready` before any call.
//! * [`usage`] — `tools.usage`, every published tool version with its calls in a window and the
//!   zero-use subset named.
//! * [`manifest`] — the embedded `seed/toolkits/*.json` declarations these three read.
//!
//! What a toolkit is **not**: the conveyor's own in-process capabilities. `media.*`, `decks.*`,
//! `sheets.*` and the analysis family are compiled actions, already callable — a toolkit would add
//! a second name for the same handler. A toolkit is an **external estate service** the conveyor
//! reaches over HTTP, which is exactly what `HandlerRef::External` marks.

pub mod manifest;
pub mod probe;
pub mod publish;
pub mod usage;

pub use manifest::{ManifestError, ToolManifest, ToolkitManifest, content_hash, manifests};
pub use probe::{ToolkitProbe, ToolsProbeIn, ToolsProbeOut};
pub use publish::{PublishedToolkit, ToolkitsPublishOut};
pub use usage::{ToolUsage, ToolsUsageIn, ToolsUsageOut};
