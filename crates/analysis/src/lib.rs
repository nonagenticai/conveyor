#![forbid(unsafe_code)]
//! `analyse.*` — rubrics, scoring and the deterministic site scan (ADR 0007 §5).
//!
//! **A library, not a service.** Analysing a page is sub-second work over bytes the caller
//! already holds: as per-job pods it would pay a cold start per call, and as a service it would
//! duplicate conveyor's job, retry and artefact handling. So this crate opens no connection,
//! reads no clock, writes no row and holds no credential. The caller owns all four.
//!
//! # The determinism contract
//!
//! `(document, rubric) ⇒ byte-identical [`Report`]`, on any machine, in any process order.
//! Three things make that true, and each is structural rather than a convention:
//!
//! 1. **No identity is minted here.** The source stamped a `uuid7` on every finding; this crate
//!    does not, because a time-based id makes two runs of the same input differ. An id belongs
//!    to the row that persists a finding, not to the finding.
//! 2. **No collection is unordered.** Scores are a [`BTreeMap`](std::collections::BTreeMap), the
//!    dedupe keeps first-seen order and sorts its contributor list.
//! 3. **The arithmetic is the source's, including its rounding.** [`rubric::round_1dp`] is
//!    half-to-even like Python's `round(x, 1)`; `f64::round` is half-away-from-zero and would
//!    move every golden that lands on a boundary.
//!
//! # What is deliberately absent
//!
//! Fetching is not analysis. `apps/research` owns the fetch contract and `apps/media` owns the
//! browser, so this crate takes a [`Document`] — URL plus HTML — and never goes to the network.
//! The same rule covers `robots.txt`, `sitemap.xml` and `llms.txt`: they are inputs, not crawls.
//! The family's LLM-dependent enrichments (the content-gap analyser, the recommendation writer,
//! the report critic) arrive as a caller-supplied trait when they are ported; nothing in this
//! crate holds a client today, and `Cargo.toml` has no transport dependency to hold one in.

pub mod model;
pub mod rubric;
pub mod site;

pub use model::{Category, Document, Finding, Grade, Report, Scorecard, Severity};
pub use rubric::Rubric;
