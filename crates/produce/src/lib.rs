#![forbid(unsafe_code)]
//! The production family (issue #163, ADR 0007 §Decision).
//!
//! P1 is the contract: the content service the estate calls "content-service" served narrative
//! and script operations over HTTP. That service is at zero replicas and is not coming back as a
//! service — its capabilities become `produce.script`, one action. This crate holds the captured
//! document and the classification of every operation it serves, so the absorption is checked
//! against evidence rather than against a description of the service.
//!
//! The document is committed at `docs/evidence/produce-openapi-2026-11-16.json`, captured from
//! the pinned image by `contract/capture.sh`. [`capture`] exposes it and its digest; the parity
//! test refuses an operation that has no disposition.
//!
//! The later parts of the issue add the blueprint manifest type and the sixteen blueprints as
//! data. They are not here yet, and nothing in this module pretends they are.

pub mod capture;
