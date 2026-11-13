#![forbid(unsafe_code)]
//! `conveyor admin` tools.
//!
//! `trace-diff`, `fixture` and `diff board` are file tools and run before `Config::from_env()`
//! and before any pool is built: they must work in a bare workspace. The registry, board and
//! `upstream-token` tools run after the configuration and the pool exist.

pub mod action;
/// `conveyor admin intake` — a GitHub issue becomes a card on a project's board.
pub mod intake;
/// `conveyor admin retire-plan-seed` — archive the retired plan-seed cards.
pub mod retire_plan_seed;
pub mod trace_diff;
/// `conveyor admin upstream-token` — seal a tenant's BYO bearer for `laya` / `inference`.
pub mod upstream_tokens;
