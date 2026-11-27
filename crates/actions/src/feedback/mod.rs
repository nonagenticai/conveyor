#![forbid(unsafe_code)]
//! #842 — the tenant's feedback about the platform: a bug, a feature request, or other.
//!
//! `feedback.create` writes the tenant's own row and `feedback.list` reads it. The platform
//! operator's cross-tenant read is the admin CLI (`conveyor admin feedback list`), not a
//! `Tenancy::Platform` action: the agent surface refuses that tenancy for a tenant-bound caller,
//! and the operator's authority is the global `platform_admin` grant.

pub mod actions;
