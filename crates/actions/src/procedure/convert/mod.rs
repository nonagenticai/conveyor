#![forbid(unsafe_code)]
//! Deterministic converters for structural process documents (T16, SPEC_V2 §5.6, decision D4).
//!
//! An RDF/OWL/Turtle document is NOT converted here: the owner decision of 2026-11-19 sends it to
//! `strata_ingest` as a knowledge pack (#413), and conveyor runs no parser, store or reasoner.
//! What remains is the BPMN 2.0 reader — a pull parser over untrusted XML, producing draft steps
//! only; nothing it reads is executed.

pub mod bpmn;
