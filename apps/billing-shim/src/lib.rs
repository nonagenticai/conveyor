#![forbid(unsafe_code)]
//! `billing-shim` — one read, one shape: the conveyor's money rollup over billing-service.
//!
//! # HTTP contract
//!
//! * `POST /api/usage/rollup` with
//!   `{"since":"YYYY-MM-DD","until":"YYYY-MM-DD","group_by":["day","project","actor","app","provider","model"]}`.
//!   `group_by` is optional and defaults to those six dimensions; a subset (and an explicit
//!   empty list, the ungrouped grand total) is translated unchanged.
//! * The answer is
//!   `{"rows":[{"dimensions":{...},"cost_usd":12.34}],"total_usd":12.34,"source":"billing-service"}`.
//! * An upstream failure (transport, non-2xx, or a body outside the upstream contract) is
//!   `502 {"type":"billing_upstream_error","detail":"..."}`.
//! * An unconfigured `BILLING_SERVICE_URL` is `503 {"type":"billing_unconfigured",...}`.
//! * A caller mistake the shim can see locally (a bad date, an unknown dimension, a missing
//!   tenant) is `400 {"type":"invalid_request"|"tenant_required",...}` — never a 502, which
//!   would blame the upstream for the caller's error.
//! * `GET /health` answers `200 {"status":"ok"}` without touching the upstream.
//!
//! # Upstream mapping (state the mapping; never invent data)
//!
//! billing-service has no `POST /api/usage/rollup`. The only rollup that carries **all six**
//! requested dimensions is `GET /api/usage/breakdown` (`routers/usage_breakdown.py`, prefix
//! `/api/usage`), which reads `billing_cost_usage_daily` (`CostUsageDaily`) — the one table
//! with project AND actor AND app AND provider AND model together, bucketed by UTC day.
//!
//! | shim | billing-service |
//! |---|---|
//! | `POST /api/usage/rollup` | `GET /api/usage/breakdown` |
//! | `since` | `from` (inclusive UTC day) |
//! | `until` | `to` (inclusive UTC day) |
//! | `group_by: ["day", …]` | `group_by=day,…` (CSV; identical dimension names) |
//! | inbound `X-Tenant-Id` | `X-Tenant-Id` (see below) |
//! | `BILLING_SERVICE_TOKEN` | `Authorization: Bearer …`, omitted when unset |
//! | `data.rows[].dims` | `rows[].dimensions` (null preserved: spend with no project/actor/app) |
//! | `data.rows[].cost_usd` (decimal string) | `rows[].cost_usd` (JSON number) |
//! | `data.totals.cost_usd` (decimal string) | `total_usd` (JSON number) |
//!
//! * **Tenant.** `GET /api/usage/breakdown` is tenant-scoped and refuses a read without a
//!   tenant id (`tenant_id` query parameter or `X-Tenant-Id` header). The shim's request body
//!   is fixed and tenant-free, so it forwards the caller's `X-Tenant-Id` header unchanged and
//!   answers `400 tenant_required` when it is absent. It never picks a tenant of its own.
//! * **Totals.** `total_usd` is the upstream `totals.cost_usd`, NOT a sum of the returned
//!   rows: the upstream caps rows (`MAX_ROWS` 5000) while its totals cover every group, so
//!   summing rows would silently invent a smaller total.
//! * **Money.** The upstream serialises `Decimal` as a JSON string; this contract asks for a
//!   JSON number, so each string is parsed with `serde_json` (IEEE-754) and emitted as a
//!   number. The shim performs no arithmetic, attribution or defaulting; a missing or
//!   malformed upstream field is a 502, never a zero.
//!
//! # Configuration
//!
//! `BILLING_SERVICE_URL` (default `http://billing-service:8157`; present-but-blank means
//! unconfigured), `BILLING_SERVICE_TOKEN` (bearer; absent or blank sends no header), `PORT`
//! (default 8158). The process binds `0.0.0.0`.

pub mod config;
pub mod error;
pub mod router;
pub mod upstream;

/// The service's own name, for logs and deploy metadata.
pub const SERVICE_NAME: &str = "conveyor-billing-shim";

/// The `source` value every successful rollup carries: the data came from billing-service.
pub const SOURCE: &str = "billing-service";
