#![forbid(unsafe_code)]
//! The portfolio action surface's request and response documents (S08 Part A).
//!
//! Every type derives `serde` **and** `schemars::JsonSchema` from the same fields, so the
//! input validator, the OpenAPI operation and the MCP tool cannot disagree (S04 §5.1).
//! Inputs are writable columns only — never `tenant_id` — and every output carries the row
//! plus its `version`, which is the `ETag` (S08 §5.2).
//!
//! The shared envelopes are CONTRACT §6's and are re-exported, never re-specified: [`PageIn`]
//! / [`Page`] from [`crate::views`], [`IdIn`] from the board surface that first shipped it.
//!
//! Ids on the wire are strings (an output document is a representation); ids in an input are
//! `Uuid`, so a malformed id is a schema error at the edge rather than a string the store has
//! to parse.
//!
//! **`project_id` is a create-time field on every family.** It is absent from the four
//! update inputs: a row that moves project is a delete and a create, so a parent never
//! changes underneath an `If-Match`.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub use crate::board::views::IdIn;
pub use crate::views::{Page, PageIn};

// ── programmes ──────────────────────────────────────────────────────────────────────────

/// `programmes.create` — the fields a new programme may carry.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct ProgrammeCreateIn {
    /// The display name; unique per tenant.
    pub name: String,
    /// The status; `active` when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// The accountable user, resolved under the caller's tenant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_user_id: Option<Uuid>,
}

/// `programmes.update` — the editable fields. `If-Match` is the precondition, never a body
/// field, and an absent field is left alone.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct ProgrammeUpdateIn {
    /// The programme to edit, resolved under the caller's tenant.
    pub id: Uuid,
    /// The new display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The new status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// The new accountable user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_user_id: Option<Uuid>,
}

/// `programmes.list` — no filters; the shared page envelope only.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct ProgrammeListIn {
    /// The shared pagination envelope (cursor + limit).
    #[serde(flatten)]
    pub page: PageIn,
}

/// One programme as every `programmes.*` action returns it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProgrammeOut {
    /// The programme id.
    pub id: String,
    /// The display name.
    pub name: String,
    /// The status.
    pub status: String,
    /// The accountable user, when one is recorded.
    pub owner_user_id: Option<String>,
    /// The row version (`ETag`).
    pub version: i64,
    /// When the programme was created.
    pub created_at: DateTime<Utc>,
    /// When it last changed.
    pub updated_at: DateTime<Utc>,
}

// ── stakeholders ────────────────────────────────────────────────────────────────────────

/// `stakeholders.create` — the stakeholder and the project they belong to.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct StakeholderCreateIn {
    /// The owning project, resolved under the caller's tenant.
    pub project_id: Uuid,
    /// The linked `app_user`, when the stakeholder has an account.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<Uuid>,
    /// The display name.
    pub name: String,
    /// The contact email.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// Their role on the project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

/// `stakeholders.update` — the editable fields. `project_id` is not one of them.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct StakeholderUpdateIn {
    /// The stakeholder to edit, resolved under the caller's tenant.
    pub id: Uuid,
    /// The new linked user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<Uuid>,
    /// The new display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The new contact email.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// The new role.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

/// `stakeholders.list` — the project filter plus the shared page envelope.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct StakeholderListIn {
    /// Only stakeholders of this project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<Uuid>,
    /// The shared pagination envelope.
    #[serde(flatten)]
    pub page: PageIn,
}

/// One stakeholder as every `stakeholders.*` action returns it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct StakeholderOut {
    /// The row id.
    pub id: String,
    /// The owning project.
    pub project_id: String,
    /// The linked `app_user`, when the stakeholder has an account.
    pub user_id: Option<String>,
    /// The display name.
    pub name: String,
    /// The contact email.
    pub email: Option<String>,
    /// Their role on the project.
    pub role: String,
    /// The row version (`ETag`).
    pub version: i64,
    /// When the stakeholder was recorded.
    pub created_at: DateTime<Utc>,
    /// When the row last changed.
    pub updated_at: DateTime<Utc>,
}
