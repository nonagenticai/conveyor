#![forbid(unsafe_code)]
//! `prompt_recipes.create` · `.get` · `.list` · `.activate` · `.retire` (S21 step 3, #15).
//!
//! A recipe version is immutable. `create` is the only way a version appears and it always
//! produces a `draft`; `activate` retires the key's current active row and activates the target
//! in one transaction; `retire` stops new runs pinning a version without deleting its history.
//! A run resolves only an `active` row (`crate::prompt::resolve`), so the lifecycle is the gate.
//!
//! Every mutation is authorized by `prompts:write` and audited by the ordinary
//! `action_invocation` row; no separate audit table exists. The selector is validated here, not
//! by the database: `prompt_section.selector` is an object, but only the source knows whether
//! `{kinds, limit, include_digest}` or `{query, top_k}` is meaningful.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgConnection;

use conveyor_domain::prompt::{KnowledgeSelector, MemorySelector, PromptSource};
use conveyor_store as store;
use conveyor_store::prompt::PromptStoreError;

use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::{FieldError, HandlerOutput};
use crate::views::{Page, PageIn};

/// One section in a create request.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PromptSectionIn {
    /// 1-based position; unique within the recipe.
    pub position: i32,
    /// `system | process | capability | role | knowledge | memory | project_card`.
    pub source_kind: String,
    /// Whether a compile with no source refuses by name.
    #[serde(default)]
    pub required: bool,
    /// The literal text, for `system` only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    /// The selection config for `knowledge` or `memory`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selector: Option<Value>,
    /// The section's own character ceiling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget_chars: Option<u32>,
}

/// `prompt_recipes.create`'s input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PromptCreateIn {
    /// The recipe key (`^[a-z][a-z0-9_]*$`).
    pub key: String,
    /// The human title.
    pub title: String,
    /// The recipe's sections.
    pub sections: Vec<PromptSectionIn>,
}

/// `prompt_recipes.get`'s input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PromptGetIn {
    /// The recipe key.
    pub key: String,
    /// An exact version; absent means the key's active version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<i32>,
}

/// `prompt_recipes.list`'s input.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct PromptListIn {
    /// Restrict to one key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// Restrict to one status (`draft | active | retired`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// CONTRACT §6's page envelope.
    #[serde(flatten)]
    pub page: PageIn,
}

/// `prompt_recipes.activate` and `.retire`'s input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PromptVersionIn {
    /// The recipe key.
    pub key: String,
    /// The exact version.
    pub version: i32,
}

/// One section in a response.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PromptSectionOut {
    /// 1-based position.
    pub position: i32,
    /// The source kind, as stored.
    pub source_kind: String,
    /// Whether it is required.
    pub required: bool,
    /// The literal text, for `system`.
    pub template: String,
    /// The selection config.
    pub selector: Value,
    /// Its own character ceiling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget_chars: Option<u32>,
}

/// A recipe version in a response.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PromptOut {
    /// The recipe key.
    pub key: String,
    /// The version.
    pub version: i32,
    /// `draft | active | retired`.
    pub status: String,
    /// The human title.
    pub title: String,
    /// The sections, in position order.
    pub sections: Vec<PromptSectionOut>,
    /// The whole-prompt character ceiling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget_chars: Option<u32>,
}

/// One catalogue row in `prompt_recipes.list`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PromptSummaryOut {
    /// The recipe key.
    pub key: String,
    /// The version.
    pub version: i32,
    /// The human title.
    pub title: String,
    /// `draft | active | retired`.
    pub status: String,
    /// When the version was created.
    pub created_at: DateTime<Utc>,
    /// When it last changed.
    pub updated_at: DateTime<Utc>,
}

/// Create the next version of a key as a draft.
#[crate::action(
    name = "prompt_recipes.create",
    version = 1,
    kind = "internal",
    scopes = ["prompts:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn prompt_recipes_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: PromptCreateIn,
) -> Result<HandlerOutput<PromptOut>, ActionError> {
    if !valid_key(&input.key) {
        return Err(invalid(
            "key",
            "invalid_value",
            "a recipe key is lower-case ASCII letters, digits and underscores, starting with a letter",
        ));
    }
    if input.title.trim().is_empty() {
        return Err(invalid("title", "required", "must not be empty"));
    }
    if input.sections.is_empty() {
        return Err(invalid(
            "sections",
            "required",
            "a recipe needs at least one section",
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut sections = Vec::with_capacity(input.sections.len());
    for (index, section) in input.sections.into_iter().enumerate() {
        if section.position < 1 {
            return Err(invalid(
                &format!("sections[{index}].position"),
                "invalid_value",
                "position starts at 1",
            ));
        }
        if !seen.insert(section.position) {
            return Err(invalid(
                &format!("sections[{index}].position"),
                "duplicate",
                "two sections share a position",
            ));
        }
        sections.push(section_of(section, index)?);
    }
    let record = store::prompt::create_prompt_tx(
        &mut *tx,
        ctx.tenant,
        &store::prompt::NewPrompt {
            key: input.key,
            title: input.title.trim().to_owned(),
            sections,
        },
    )
    .await
    .map_err(|error| recipe_error(error, "prompt_recipes.create"))?;
    Ok(HandlerOutput::created(prompt_out(&record)))
}

/// One version by key, or the key's active version.
#[crate::action(
    name = "prompt_recipes.get",
    version = 1,
    kind = "internal",
    scopes = ["prompts:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn prompt_recipes_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: PromptGetIn,
) -> Result<PromptOut, ActionError> {
    let record =
        store::prompt::get_prompt_by_key_tx(&mut *tx, ctx.tenant, &input.key, input.version)
            .await
            .map_err(|error| recipe_error(error, "prompt_recipes.get"))?
            .ok_or_else(not_found)?;
    Ok(prompt_out(&record))
}

/// One page of catalogue rows.
#[crate::action(
    name = "prompt_recipes.list",
    version = 1,
    kind = "internal",
    scopes = ["prompts:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn prompt_recipes_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: PromptListIn,
) -> Result<Page<PromptSummaryOut>, ActionError> {
    let status = input.status.as_deref().map(parse_status).transpose()?;
    let limit = store::actions::clamp_limit(input.page.limit);
    let rows =
        store::prompt::list_prompts_tx(&mut *tx, ctx.tenant, input.key.as_deref(), status, limit)
            .await
            .map_err(|error| recipe_error(error, "prompt_recipes.list"))?;
    Ok(Page {
        items: rows
            .into_iter()
            .map(|row| PromptSummaryOut {
                key: row.key,
                version: row.version,
                title: row.title,
                status: row.status,
                created_at: row.created_at,
                updated_at: row.updated_at,
            })
            .collect(),
        next_cursor: None,
    })
}

/// Activate one version, retiring the key's current active version in the same transaction.
#[crate::action(
    name = "prompt_recipes.activate",
    version = 1,
    kind = "internal",
    scopes = ["prompts:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn prompt_recipes_activate(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: PromptVersionIn,
) -> Result<HandlerOutput<PromptOut>, ActionError> {
    let record = store::prompt::activate_prompt_tx(&mut *tx, ctx.tenant, &input.key, input.version)
        .await
        .map_err(|error| recipe_error(error, "prompt_recipes.activate"))?;
    Ok(HandlerOutput::ok(prompt_out(&record)))
}

/// Retire one version. A retired pin stays readable; a new run may not resolve it.
#[crate::action(
    name = "prompt_recipes.retire",
    version = 1,
    kind = "internal",
    scopes = ["prompts:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn prompt_recipes_retire(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: PromptVersionIn,
) -> Result<HandlerOutput<PromptOut>, ActionError> {
    let record = store::prompt::retire_prompt_tx(&mut *tx, ctx.tenant, &input.key, input.version)
        .await
        .map_err(|error| recipe_error(error, "prompt_recipes.retire"))?;
    Ok(HandlerOutput::ok(prompt_out(&record)))
}

fn valid_key(key: &str) -> bool {
    let mut bytes = key.bytes();
    matches!(bytes.next(), Some(first) if first.is_ascii_lowercase())
        && bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn empty_object(value: &Value) -> bool {
    value.as_object().is_some_and(|object| object.is_empty())
}

fn section_of(
    section: PromptSectionIn,
    index: usize,
) -> Result<store::prompt::NewPromptSection, ActionError> {
    let field = |name: &str| format!("sections[{index}].{name}");
    let source = PromptSource::parse(&section.source_kind).ok_or_else(|| {
        invalid(
            &field("source_kind"),
            "invalid_value",
            "unknown section source kind",
        )
    })?;
    let template = section.template.unwrap_or_default();
    if source == PromptSource::System && template.trim().is_empty() {
        return Err(invalid(
            &field("template"),
            "required",
            "a system section must carry its text",
        ));
    }
    if source != PromptSource::System && !template.trim().is_empty() {
        return Err(invalid(
            &field("template"),
            "invalid_value",
            "only a system section carries template text",
        ));
    }
    let selector = section.selector.unwrap_or_else(|| json!({}));
    if !selector.is_object() {
        return Err(invalid(
            &field("selector"),
            "invalid_value",
            "the selector must be an object",
        ));
    }
    match source {
        PromptSource::Memory => {
            MemorySelector::parse(&selector)
                .map_err(|error| invalid(&field("selector"), "invalid_value", error.to_string()))?;
        }
        PromptSource::Knowledge => {
            KnowledgeSelector::parse(&selector)
                .map_err(|error| invalid(&field("selector"), "invalid_value", error.to_string()))?;
        }
        _ if !empty_object(&selector) => {
            return Err(invalid(
                &field("selector"),
                "invalid_value",
                "only knowledge and memory sections have a selector",
            ));
        }
        _ => {}
    }
    if section.budget_chars == Some(0) {
        return Err(invalid(
            &field("budget_chars"),
            "invalid_value",
            "a budget must be greater than 0",
        ));
    }
    Ok(store::prompt::NewPromptSection {
        position: section.position,
        source,
        required: section.required,
        template,
        selector,
        budget_chars: section.budget_chars,
    })
}

fn prompt_out(record: &store::prompt::PromptRecord) -> PromptOut {
    PromptOut {
        key: record.definition.key.clone(),
        version: record.definition.version,
        status: record.status.clone(),
        title: record.definition.title.clone(),
        sections: record
            .definition
            .sections
            .iter()
            .map(|section| PromptSectionOut {
                position: section.position,
                source_kind: section.source.as_str().to_owned(),
                required: section.required,
                template: section.template.clone(),
                selector: section.selector.clone(),
                budget_chars: section.budget_chars,
            })
            .collect(),
        budget_chars: record.definition.budget_chars,
    }
}

fn parse_status(raw: &str) -> Result<&'static str, ActionError> {
    match raw {
        "draft" => Ok("draft"),
        "active" => Ok("active"),
        "retired" => Ok("retired"),
        _ => Err(invalid(
            "status",
            "invalid_value",
            "status is draft, active or retired",
        )),
    }
}

fn invalid(field: &str, code: &str, detail: impl Into<String>) -> ActionError {
    ActionError::invalid_input(vec![FieldError {
        field: field.to_owned(),
        code: code.to_owned(),
        detail: detail.into(),
    }])
}

fn not_found() -> ActionError {
    ActionError::new(404, "prompt.not_found", "no prompt version in this tenant")
}

fn recipe_error(error: PromptStoreError, action: &str) -> ActionError {
    match error {
        PromptStoreError::NotFound => not_found(),
        PromptStoreError::AlreadyActive => ActionError::new(
            409,
            "prompt.already_active",
            "the version is already active",
        ),
        PromptStoreError::AlreadyRetired => ActionError::new(
            409,
            "prompt.already_retired",
            "the version is already retired",
        ),
        PromptStoreError::Conflict => ActionError::new(
            409,
            "prompt.conflict",
            "a concurrent write won the race; read the catalogue and retry",
        ),
        PromptStoreError::Selector(error) => {
            invalid("selector", "invalid_value", error.to_string())
        }
        PromptStoreError::UnknownSource { .. } | PromptStoreError::Memory(_) => {
            ActionError::internal()
        }
        PromptStoreError::Db(error) => {
            tracing::error!(%error, action, "the prompt catalogue write failed");
            ActionError::internal()
        }
    }
}
