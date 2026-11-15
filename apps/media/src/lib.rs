//! `apps/media` — the media group of ADR 0006, one crate and three binaries.
//!
//! The estate's media capability ships as few deployables as its runtime shapes allow
//! (ADR 0006 §2): a browser binary (Chromium), an ffmpeg binary, and a thin binary for the
//! services that only fan out over HTTP. Each reference service maps to exactly one binary — the
//! one whose heaviest runtime it needs — so no captured route set is split across deployables, and
//! the toolkits they publish stay separate catalogue entries.

pub mod common;
pub mod contract;

pub mod browser;
pub mod ffmpeg;
pub mod thin;
