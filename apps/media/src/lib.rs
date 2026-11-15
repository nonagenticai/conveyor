//! `apps/media` — the media group of ADR 0006, one crate and **two executor images**.
//!
//! ADR 0007 §1–4 settles the shape: a browser image (Chromium) and an ffmpeg image (ffmpeg,
//! ffprobe, tesseract). Each reference service maps to exactly one image — the one whose heaviest
//! runtime it needs — so no captured route set is split across deployables. The images are **job
//! executors, not HTTP services**: each takes one `runner/v1` instruction (a `kind`, its `params`,
//! named inputs and a closed set of outputs) and exits.
//!
//! Thin operations (`tts_gateway`, `youtube_upload_service`) are not here: ADR 0007 §1 hosts them
//! in `apps/server` beside the job ledger, and `slide_service` is `crates/deck` per §7.

pub mod common;
pub mod contract;

pub mod browser;
pub mod ffmpeg;
