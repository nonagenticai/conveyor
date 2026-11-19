#![forbid(unsafe_code)]
//! T16 — the SOP / process-document intake path: `procedures.ingest_sop` · `.ingest_process` ·
//! `.ingest_rdf` · `.validate` · `.review_source`.
//!
//! A tenant document is **untrusted input**. It enters as a `governed_asset(kind='sop_source')`
//! with provenance, is converted into a **draft** SOP judge and a **draft** executable process
//! (activities → steps, gateways → gates, prose → PDDL steps), and nothing it asks for is granted:
//! the drafts land unpublished, the tool request is recorded and refused by `procedures.validate`,
//! and every publish is a human gate (`processes.publish`, T16 P2).
//!
//! **The strata decision (owner, 2026-11-19; #413).** An RDF/OWL/Turtle document is not parsed
//! here. It is stored as a document and handed to `strata_ingest`; the returned job id is the
//! knowledge pack the derived procedure names. conveyor runs no RDF store, reasoner or parser, and
//! creates no local ontology rows.
//!
//! **Two spaces, never mixed (D53).** The knowledge space is the strata pack; the action space is
//! the step's PDDL action text and its ground-literal preconditions/effects
//! (`conveyor_domain::pddl`). A converted step is `computed` and names the registry action it
//! invokes; the diagram/prose text becomes its `action_text`, never an instruction to the engine.

use base64::Engine as _;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use uuid::Uuid;

use conveyor_domain::process::StepInput;
use conveyor_domain::{AssetKind, AssetStatus};
use conveyor_store as store;
use conveyor_store::governance::NewAsset;
use conveyor_store::process::{NewProcess, ProcessDerivation};

use crate::integrations::convert::{ConvertRequest, ModelDraft};
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;

/// The converter names, recorded on the process so a re-ingest is reproducible.
pub const PROSE_CONVERTER: &str = "prose@1";
/// The BPMN reader's version.
pub const BPMN_CONVERTER: &str = "bpmn@1";
/// A source document larger than this is a mistake, not an SOP.
pub const MAX_SOURCE_BYTES: usize = 10 * 1024 * 1024;
/// The strata secret a tenant must hold before an RDF import can be submitted.
pub const INGEST_TOKEN_SECRET: &str = "strata_ingest.token";

// ── inputs and outputs ───────────────────────────────────────────────────────────────────

/// How the source is carried.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceIn {
    /// `pdf | text | url | bpmn | rdf`.
    pub kind: String,
    /// The media type, when the caller knows it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    /// The file name, for provenance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    /// The bytes, base64-encoded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_base64: Option<String>,
    /// The text, when the source is text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// The address, when the source is a URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// `procedures.ingest_sop` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IngestSopIn {
    /// The SOP's title.
    pub title: String,
    /// The process key the draft takes (an existing key gets the next draft version).
    pub process_key: String,
    /// The `sop_source` asset key; unique per tenant.
    pub source_key: String,
    /// The document.
    pub source: SourceIn,
    /// The project the asset belongs to, when any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<Uuid>,
    /// The model or schema id the router should use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// The industry the derived job is placed in; without it no job draft is created.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub industry_id: Option<Uuid>,
    /// The department the derived job is placed in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub department_id: Option<Uuid>,
    /// When present, the source is also submitted to `strata_ingest` and its job id becomes the
    /// knowledge pack the drafts name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strata_store_id: Option<Uuid>,
}

/// `procedures.ingest_process` input (BPMN 2.0).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IngestProcessIn {
    /// The diagram's title.
    pub title: String,
    /// The process key the draft takes.
    pub process_key: String,
    /// The `sop_source` asset key.
    pub source_key: String,
    /// The diagram (base64 or text).
    pub source: SourceIn,
    /// The project the asset belongs to, when any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<Uuid>,
    /// When present, also submit to `strata_ingest` and name the pack.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strata_store_id: Option<Uuid>,
}

/// `procedures.ingest_rdf` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IngestRdfIn {
    /// The ontology's title.
    pub title: String,
    /// The `sop_source` asset key.
    pub source_key: String,
    /// The document (base64 or text).
    pub source: SourceIn,
    /// The project the asset belongs to, when any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<Uuid>,
    /// The strata store the document lands in. Required: the knowledge lives there.
    pub strata_store_id: Uuid,
}

/// What an ingest returns.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct IngestOut {
    /// The governed source asset.
    pub source_asset_id: Uuid,
    /// The draft SOP judge.
    pub procedure_id: Uuid,
    /// The draft executable process.
    pub process_id: Uuid,
    /// How many steps the draft carries.
    pub steps: usize,
    /// The converter that produced the draft.
    pub converter_version: String,
    /// The strata knowledge pack, `""` when none was paired.
    pub knowledge_pack_ref: String,
    /// The capability draft, when the SOP named one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capability_id: Option<Uuid>,
    /// The agent draft, when the SOP named one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<Uuid>,
    /// The job draft, when the caller supplied a placement.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<Uuid>,
}

/// What an RDF import returns.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct IngestRdfOut {
    /// The governed source asset.
    pub source_asset_id: Uuid,
    /// The stored document.
    pub document_id: Uuid,
    /// The strata job id, recorded as the knowledge pack.
    pub knowledge_pack_ref: String,
}

/// `procedures.validate` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ValidateIn {
    /// The draft process to validate.
    pub process_id: Uuid,
}

/// The validation's answer.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ValidateOut {
    /// Whether the draft may be published (subject to the human gate).
    pub valid: bool,
    /// One line per check that passed.
    pub checks: Vec<String>,
}

/// `procedures.review_source` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReviewSourceIn {
    /// The `sop_source` asset.
    pub asset_id: Uuid,
    /// `approve | reject`.
    pub decision: String,
    /// Why, recorded on the asset.
    #[serde(default)]
    pub note: String,
}

/// The review's answer.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReviewSourceOut {
    /// The reviewed asset.
    pub asset_id: Uuid,
    /// Its status after the review (`approved`, or `draft` after a rejection).
    pub status: String,
}

// ── shared helpers ───────────────────────────────────────────────────────────────────────

fn invalid(field: &str, code: &str, detail: impl Into<String>) -> ActionError {
    ActionError::invalid_input(vec![crate::spec::FieldError {
        field: field.to_owned(),
        code: code.to_owned(),
        detail: detail.into(),
    }])
}

/// The source's bytes, media type and a printable provenance summary.
fn decode_source(source: &SourceIn) -> Result<(Vec<u8>, String), ActionError> {
    let media = source
        .media_type
        .clone()
        .unwrap_or_else(|| match source.kind.as_str() {
            "pdf" => "application/pdf".to_owned(),
            "bpmn" => "application/xml".to_owned(),
            "rdf" => "text/turtle".to_owned(),
            _ => "text/plain".to_owned(),
        });
    let bytes = if let Some(base64) = source.content_base64.as_deref() {
        base64::engine::general_purpose::STANDARD
            .decode(base64.trim())
            .map_err(|error| invalid("source.content_base64", "invalid_value", error.to_string()))?
    } else if let Some(text) = source.text.as_deref() {
        text.as_bytes().to_vec()
    } else if let Some(url) = source.url.as_deref() {
        // A URL is PROVENANCE here: the conversion is the model's fetch, not this action's. The
        // stored bytes are the address itself, so the source asset remains content-addressed.
        url.as_bytes().to_vec()
    } else {
        return Err(invalid(
            "source",
            "required",
            "supply content_base64, text or url",
        ));
    };
    if bytes.is_empty() {
        return Err(invalid("source", "empty", "the source document is empty"));
    }
    if bytes.len() > MAX_SOURCE_BYTES {
        return Err(ActionError::new(
            413,
            "payload_too_large",
            format!(
                "the source is {} bytes; the cap is {MAX_SOURCE_BYTES}",
                bytes.len()
            ),
        ));
    }
    Ok((bytes, media))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Submit a stored document to `strata_ingest` and return the knowledge-pack reference.
///
/// The document is written to the object store and the `document` row in the caller's transaction,
/// exactly as `documents.create` does; the push copies `documents.ingest`'s credential discipline —
/// the tenant's own token, never a shared or default one.
async fn submit_to_strata(
    tx: &mut PgConnection,
    ctx: &HandlerCtx,
    title: &str,
    bytes: &[u8],
    media_type: &str,
    store_id: Uuid,
) -> Result<(Uuid, String), ActionError> {
    let token = match store::secret::open_named_tx(&mut *tx, ctx.tenant, INGEST_TOKEN_SECRET).await
    {
        Ok(bytes) => String::from_utf8(bytes).map_err(|_| {
            ActionError::new(
                422,
                "source.strata_not_configured",
                format!("this tenant's {INGEST_TOKEN_SECRET} is not a UTF-8 bearer token"),
            )
        })?,
        Err(store::secret::SecretError::NotFound) => {
            return Err(ActionError::new(
                422,
                "source.strata_not_configured",
                format!(
                    "this tenant has no {INGEST_TOKEN_SECRET} secret; create one before importing an ontology"
                ),
            ));
        }
        Err(error) => {
            tracing::error!(%error, "read the strata ingest token");
            return Err(ActionError::internal());
        }
    };
    let token = token.trim().to_owned();
    if token.is_empty() || !token.bytes().all(|byte| byte.is_ascii_graphic()) {
        return Err(ActionError::new(
            422,
            "source.strata_not_configured",
            format!("this tenant's {INGEST_TOKEN_SECRET} is not a legal bearer token"),
        ));
    }

    let ingest = crate::integrations::ingest::Ingest::from_env().ok_or_else(|| {
        ActionError::new(
            503,
            "source.strata_unavailable",
            "CONVEYOR_STRATA_INGEST_URL is not configured",
        )
    })?;

    let content_hash = sha256_hex(bytes);
    let objects = crate::integrations::objects::Objects::from_env().ok_or_else(|| {
        ActionError::new(
            503,
            "source.storage_unavailable",
            "the object store is not configured",
        )
    })?;
    let document_id = Uuid::now_v7();
    let key =
        crate::integrations::objects::key_for(ctx.tenant.as_uuid(), document_id, &content_hash);
    objects
        .put(&key, bytes.to_vec().into())
        .await
        .map_err(|error| {
            tracing::error!(%error, "store the source document");
            ActionError::internal()
        })?;
    let meta = json!({
        "content_type": media_type,
        "size_bytes": bytes.len() as i64,
        "source": "procedures.ingest",
    });
    store::document::create_document_tx(
        &mut *tx,
        ctx.tenant,
        &store::document::NewDocument {
            id: document_id,
            project_id: None,
            kind: "document".to_owned(),
            title: title.to_owned(),
            status: "active".to_owned(),
            object_key: Some(key),
            content_hash: Some(content_hash),
            card_id: None,
            parent_document_id: None,
            body: None,
            external_ref: None,
            meta,
            publishable: false,
            category: None,
            unit: None,
            created_by: Some(ctx.actor_id),
            updated_by: Some(ctx.actor_id),
        },
    )
    .await
    .map_err(|error| {
        tracing::error!(%error, "create the source document row");
        ActionError::internal()
    })?;

    let request = crate::integrations::ingest::IngestRequest {
        store_id,
        source_id: Some(format!("conveyor:{}", document_id)),
        source_run: None,
        options: None,
        filename: title.to_owned(),
        content_type: media_type.to_owned(),
        bytes: bytes.to_vec().into(),
    };
    let accepted = ingest.submit(&token, request).await.map_err(|error| {
        tracing::error!(%error, "submit the source to strata_ingest");
        ActionError::new(
            502,
            "source.strata_failed",
            "strata_ingest did not accept the document",
        )
    })?;
    let reference = format!("{store_id}/{}", accepted.job_id);
    store::document::set_external_ref_tx(&mut *tx, ctx.tenant, document_id, &reference)
        .await
        .map_err(|error| {
            tracing::error!(%error, "record the strata job id");
            ActionError::internal()
        })?;
    Ok((document_id, reference))
}

/// Create the source asset. `meta` carries the provenance, the pack reference and the tool
/// request — the last is REFUSED later by `procedures.validate`, never granted here.
async fn create_source_asset(
    tx: &mut PgConnection,
    ctx: &HandlerCtx,
    title: &str,
    key: &str,
    project_id: Option<Uuid>,
    meta: Value,
) -> Result<Uuid, ActionError> {
    let mut asset = NewAsset::new(AssetKind::SopSource, key.trim(), title.trim());
    asset.project_id = project_id;
    asset.owner_user_id = Some(ctx.actor_id);
    asset.author_id = Some(ctx.actor_id);
    asset.meta = meta;
    match store::governance::insert_asset_tx(&mut *tx, ctx.tenant, &asset).await {
        Ok(asset) => Ok(asset.id),
        Err(store::governance::GovernanceStoreError::Database(error))
            if error
                .as_database_error()
                .and_then(|db| db.constraint())
                .is_some_and(|name| name == "governed_asset_tenant_kind_key_key") =>
        {
            Err(ActionError::new(
                409,
                "source.conflict",
                format!("a sop_source with the key {key:?} already exists in this tenant"),
            ))
        }
        Err(error) => {
            tracing::error!(%error, "create the sop_source asset");
            Err(ActionError::internal())
        }
    }
}

/// One draft step as both converters produce it.
struct DraftStep {
    action: String,
    action_text: String,
    actor: Option<String>,
    preconditions: Vec<String>,
    effects: Value,
}

/// Parse the model's draft into canonical steps; a malformed literal refuses the whole draft.
fn model_steps(draft: &ModelDraft) -> Result<Vec<DraftStep>, ActionError> {
    let mut out = Vec::with_capacity(draft.steps.len());
    for (index, step) in draft.steps.iter().enumerate() {
        let preconditions = conveyor_domain::pddl::parse_preconditions(
            "pddl_preconditions",
            &json!(step.preconditions),
        )
        .map_err(|error| {
            invalid(
                &format!("draft.steps[{index}].preconditions"),
                "invalid_value",
                error.detail,
            )
        })?;
        let empty_effects = json!({"add": [], "delete": []});
        let effects_value = if step.effects.is_null() {
            &empty_effects
        } else {
            &step.effects
        };
        let effects = conveyor_domain::pddl::parse_effects("pddl_effects", effects_value).map_err(
            |error| {
                invalid(
                    &format!("draft.steps[{index}].effects"),
                    "invalid_value",
                    error.detail,
                )
            },
        )?;
        out.push(DraftStep {
            action: step.action.trim().to_owned(),
            action_text: if step.action_text.trim().is_empty() {
                format!("step_{}", index + 1)
            } else {
                step.action_text.trim().to_owned()
            },
            actor: step.actor.clone(),
            preconditions: preconditions
                .iter()
                .map(conveyor_domain::pddl::literal_text)
                .collect(),
            effects: effects.to_json(),
        });
    }
    Ok(out)
}

/// Build the `StepInput`s for a draft: every step names the SOP judge, so nothing is judged by
/// nobody (the publish-time rule D28 exists to keep).
fn step_inputs(steps: &[DraftStep], procedure_id: Uuid) -> Vec<StepInput> {
    steps
        .iter()
        .map(|step| StepInput {
            action: step.action.clone(),
            kind: Some("computed".to_owned()),
            actor: step.actor.clone(),
            capability_id: None,
            process_agent_id: None,
            procedure_id: Some(procedure_id),
            child_process_key: None,
            input_template: json!({}),
            gate_kind: None,
            on_result: Some("fail".to_owned()),
            max_attempts: Some(1),
            outputs: Vec::new(),
            action_text: Some(step.action_text.clone()),
            pddl_preconditions: json!(step.preconditions),
            pddl_effects: step.effects.clone(),
        })
        .collect()
}

/// Create the draft SOP judge + the draft executable and their derivation facts.
#[allow(clippy::too_many_arguments)]
async fn create_derived_drafts(
    tx: &mut PgConnection,
    ctx: &HandlerCtx,
    title: &str,
    process_key: &str,
    source_asset_id: Uuid,
    knowledge_pack_ref: &str,
    converter: &str,
    domain: &str,
    init: &Value,
    steps: &[DraftStep],
) -> Result<(Uuid, Uuid), ActionError> {
    // The judge states what "done" means for every converted step; an empty acceptance list would
    // ask the judge to accept anything, which `procedures.publish` refuses.
    let criteria: Vec<Value> = steps
        .iter()
        .enumerate()
        .map(|(index, step)| {
            json!({
                "criterion": format!(
                    "step {} `{}` executes and its declared effect holds",
                    index + 1,
                    step.action_text
                )
            })
        })
        .collect();
    let sop = store::procedure::NewSop {
        key: process_key.to_owned(),
        title: title.to_owned(),
        description: format!("Converted from source {source_asset_id} by {converter}."),
        definition_of_ready: json!({}),
        definition_of_done: json!({}),
        acceptance_criteria: json!(criteria),
        inspection: json!({"kind": "checker", "rule": "non_empty"}),
        outputs: json!([]),
        source_asset_id: Some(source_asset_id),
        knowledge_pack_ref: knowledge_pack_ref.to_owned(),
        action_text: Some(domain.to_owned()),
    };
    let procedure = store::procedure::create_draft_tx(&mut *tx, ctx.tenant, &sop)
        .await
        .map_err(|error| match error {
            store::procedure::SopStoreError::DraftExists => ActionError::new(
                409,
                "procedure.draft_exists",
                format!(
                    "the key {process_key:?} already has a draft SOP; publish it before re-ingesting"
                ),
            ),
            other => {
                tracing::error!(error = %other, "create the derived SOP judge");
                ActionError::internal()
            }
        })?;

    // A key that already exists gets the NEXT version (the version family's rule); its steps are
    // replaced by this conversion, never merged with the previous draft's.
    let inputs = step_inputs(steps, procedure.id);
    let process = match store::process::get_process_by_key_tx(
        &mut *tx,
        ctx.tenant,
        process_key,
        None,
    )
    .await
    .map_err(|error| {
        tracing::error!(%error, "read the derived key's latest version");
        ActionError::internal()
    })? {
        Some(latest) => {
            let forked = store::process::fork_version_tx(
                &mut *tx,
                ctx.tenant,
                latest.id,
                Some(ctx.actor_id),
            )
            .await
            .map_err(|error| match error {
                store::process::ProcessStoreError::DraftExists => ActionError::new(
                    409,
                    "process.draft_exists",
                    format!(
                        "the key {process_key:?} already has a draft; publish or retire it before re-ingesting"
                    ),
                ),
                other => {
                    tracing::error!(error = %other, "fork the derived process");
                    ActionError::internal()
                }
            })?;
            store::process::replace_steps_tx(&mut *tx, ctx.tenant, forked.id, &inputs)
                .await
                .map_err(|error| {
                    tracing::error!(%error, "replace the forked draft's steps");
                    ActionError::internal()
                })?;
            forked
        }
        None => {
            let created = store::process::create_draft_tx(
                &mut *tx,
                ctx.tenant,
                &NewProcess {
                    key: process_key.to_owned(),
                    title: title.to_owned(),
                    description: format!("Converted from source {source_asset_id} by {converter}."),
                    card_type_id: None,
                    list_kind: None,
                    project_type_id: None,
                    outputs: json!([]),
                    capability_id: None,
                    owner_user_id: Some(ctx.actor_id),
                },
            )
            .await
            .map_err(|error| {
                tracing::error!(%error, "create the derived process draft");
                ActionError::internal()
            })?;
            store::process::insert_steps_tx(&mut *tx, ctx.tenant, created.id, &inputs)
                .await
                .map_err(|error| {
                    tracing::error!(%error, "write the derived steps");
                    ActionError::internal()
                })?;
            created
        }
    };
    store::process::set_process_derivation_tx(
        &mut *tx,
        ctx.tenant,
        process.id,
        &ProcessDerivation {
            source_asset_id,
            knowledge_pack_ref: knowledge_pack_ref.to_owned(),
            converter_version: converter.to_owned(),
            pddl_domain: domain.to_owned(),
            pddl_init: init.clone(),
        },
    )
    .await
    .map_err(|error| {
        tracing::error!(%error, "write the derivation facts");
        ActionError::internal()
    })?;
    Ok((procedure.id, process.id))
}

// ── the actions ──────────────────────────────────────────────────────────────────────────

/// Ingest a prose SOP and convert it into a draft procedure.
#[crate::action(
    name = "procedures.ingest_sop",
    version = 1,
    kind = "internal",
    scopes = ["processes:write"],
    idempotent = false,
    timeout_ms = 120_000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn procedures_ingest_sop(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IngestSopIn,
) -> Result<HandlerOutput<IngestOut>, ActionError> {
    if input.title.trim().is_empty() {
        return Err(invalid("title", "required", "must not be empty"));
    }
    if input.process_key.trim().is_empty() {
        return Err(invalid("process_key", "required", "must not be empty"));
    }
    if input.source_key.trim().is_empty() {
        return Err(invalid("source_key", "required", "must not be empty"));
    }
    let (bytes, media_type) = decode_source(&input.source)?;
    let knowledge_pack_ref = match input.strata_store_id {
        Some(store_id) => {
            submit_to_strata(&mut *tx, &ctx, &input.title, &bytes, &media_type, store_id)
                .await?
                .1
        }
        None => String::new(),
    };

    let content_base64 = if input.source.content_base64.is_some() {
        Some(base64::engine::general_purpose::STANDARD.encode(&bytes))
    } else {
        None
    };
    let draft = crate::integrations::convert::convert(&ConvertRequest {
        title: &input.title,
        kind: &input.source.kind,
        media_type: &media_type,
        content_base64: content_base64.as_deref(),
        text: input.source.text.as_deref(),
        url: input.source.url.as_deref(),
        model: input.model.as_deref(),
    })
    .await?;
    let steps = model_steps(&draft)?;
    if steps.is_empty() {
        return Err(ActionError::new(
            422,
            "converter.invalid",
            "the conversion produced no steps",
        ));
    }

    let meta = json!({
        "source_kind": input.source.kind,
        "media_type": media_type,
        "filename": input.source.filename,
        "url": input.source.url,
        "sha256": sha256_hex(&bytes),
        "converter_version": PROSE_CONVERTER,
        "knowledge_pack_ref": knowledge_pack_ref,
        "requested_tools": draft.requested_tools,
    });
    let source_asset_id = create_source_asset(
        &mut *tx,
        &ctx,
        &input.title,
        &input.source_key,
        input.project_id,
        meta,
    )
    .await?;
    let init = conveyor_domain::pddl::parse_state("pddl_init", &json!(draft.init))
        .map_err(|error| invalid("draft.init", "invalid_value", error.detail))?;
    let init = conveyor_domain::pddl::state_json(&init);
    let (procedure_id, process_id) = create_derived_drafts(
        &mut *tx,
        &ctx,
        &input.title,
        &input.process_key,
        source_asset_id,
        &knowledge_pack_ref,
        PROSE_CONVERTER,
        &draft.domain,
        &init,
        &steps,
    )
    .await?;

    // The design-reuse drafts (P1.4): capability, agent and job, all unpublished. The agent's
    // allowed_actions is EMPTY — an SOP cannot grant a tool (P5.2).
    let capability_id = match draft.capability.as_ref() {
        Some(capability) => Some(
            store::capability::create_capability_tx(
                &mut *tx,
                ctx.tenant,
                &store::capability::NewCapability {
                    code: capability.code.clone(),
                    label: capability.label.clone(),
                    description: capability.description.clone(),
                    status: "draft".to_owned(),
                    meta: json!({"source_asset_id": source_asset_id}),
                },
            )
            .await
            .map_err(|error| {
                tracing::error!(%error, "create the derived capability draft");
                ActionError::internal()
            })?
            .id,
        ),
        None => None,
    };
    let agent_id = match (draft.agent.as_ref(), capability_id) {
        (Some(agent), Some(capability_id)) => Some(
            store::capability::create_agent_tx(
                &mut *tx,
                ctx.tenant,
                &store::capability::NewAgent {
                    capability_id,
                    key: agent.key.clone(),
                    status: "draft".to_owned(),
                    // The column is NOT NULL with a non-blank CHECK and the SOP names no model:
                    // the estate default the ladder's seed uses is recorded, so a reviewer sees
                    // what the draft would run on rather than an empty cell.
                    model_ref: if agent.model_ref.trim().is_empty() {
                        "jev-latest".to_owned()
                    } else {
                        agent.model_ref.clone()
                    },
                    prompt_ref: None,
                    params: json!({}),
                    allowed_actions: Vec::new(),
                    max_iterations: 1,
                    timeout_secs: 60,
                },
            )
            .await
            .map_err(|error| {
                tracing::error!(%error, "create the derived agent draft");
                ActionError::internal()
            })?
            .id,
        ),
        _ => None,
    };
    let job_id = match (draft.job.as_ref(), input.industry_id, input.department_id) {
        (Some(job), Some(industry_id), Some(department_id)) => Some(
            store::organisation::job_create_tx(
                &mut *tx,
                ctx.tenant,
                &store::organisation::NewJob {
                    job_key: job.job_key.clone(),
                    industry_id,
                    family: job.family.clone(),
                    title: job.title.clone(),
                    status: "draft".to_owned(),
                    definition: json!({}),
                    meta: json!({"source_asset_id": source_asset_id}),
                    departments: vec![store::organisation::PlacementIn {
                        department_id,
                        ordinal: 1,
                        is_primary: true,
                    }],
                },
            )
            .await
            .map_err(|error| {
                tracing::error!(%error, "create the derived job draft");
                ActionError::internal()
            })?
            .id,
        ),
        _ => None,
    };

    Ok(HandlerOutput::created(IngestOut {
        source_asset_id,
        procedure_id,
        process_id,
        steps: steps.len(),
        converter_version: PROSE_CONVERTER.to_owned(),
        knowledge_pack_ref,
        capability_id,
        agent_id,
        job_id,
    }))
}

/// Ingest a BPMN 2.0 diagram as a draft process.
#[crate::action(
    name = "procedures.ingest_process",
    version = 1,
    kind = "internal",
    scopes = ["processes:write"],
    idempotent = false,
    timeout_ms = 60_000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn procedures_ingest_process(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IngestProcessIn,
) -> Result<HandlerOutput<IngestOut>, ActionError> {
    if input.title.trim().is_empty() || input.process_key.trim().is_empty() {
        return Err(invalid("title", "required", "must not be empty"));
    }
    let (bytes, media_type) = decode_source(&input.source)?;
    let knowledge_pack_ref = match input.strata_store_id {
        Some(store_id) => {
            submit_to_strata(&mut *tx, &ctx, &input.title, &bytes, &media_type, store_id)
                .await?
                .1
        }
        None => String::new(),
    };
    let draft = crate::procedure::convert::bpmn::parse(&bytes).map_err(|error| {
        ActionError::new(
            422,
            "converter.invalid",
            format!("the diagram could not be read: {}", error.detail),
        )
    })?;
    let steps: Vec<DraftStep> = draft
        .steps
        .iter()
        .map(|step| DraftStep {
            action: String::new(),
            action_text: step.action_text.clone(),
            // A diagram names a person's work, not an agent: the activity is mapped to a real
            // action by the tenant before the draft can publish.
            actor: Some("human".to_owned()),
            preconditions: Vec::new(),
            effects: json!({"add": [], "delete": []}),
        })
        .collect();
    if steps.is_empty() {
        return Err(ActionError::new(
            422,
            "converter.invalid",
            "the diagram carries no activities",
        ));
    }
    let meta = json!({
        "source_kind": input.source.kind,
        "media_type": media_type,
        "filename": input.source.filename,
        "sha256": sha256_hex(&bytes),
        "converter_version": BPMN_CONVERTER,
        "knowledge_pack_ref": knowledge_pack_ref,
    });
    let source_asset_id = create_source_asset(
        &mut *tx,
        &ctx,
        &input.title,
        &input.source_key,
        input.project_id,
        meta,
    )
    .await?;
    let (procedure_id, process_id) = create_derived_drafts(
        &mut *tx,
        &ctx,
        &input.title,
        &input.process_key,
        source_asset_id,
        &knowledge_pack_ref,
        BPMN_CONVERTER,
        "",
        &json!([]),
        &steps,
    )
    .await?;
    Ok(HandlerOutput::created(IngestOut {
        source_asset_id,
        procedure_id,
        process_id,
        steps: steps.len(),
        converter_version: BPMN_CONVERTER.to_owned(),
        knowledge_pack_ref,
        capability_id: None,
        agent_id: None,
        job_id: None,
    }))
}

/// Ingest an RDF/OWL/Turtle document as a strata knowledge pack.
#[crate::action(
    name = "procedures.ingest_rdf",
    version = 1,
    kind = "internal",
    scopes = ["processes:write"],
    idempotent = false,
    timeout_ms = 60_000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn procedures_ingest_rdf(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IngestRdfIn,
) -> Result<HandlerOutput<IngestRdfOut>, ActionError> {
    if input.title.trim().is_empty() || input.source_key.trim().is_empty() {
        return Err(invalid("title", "required", "must not be empty"));
    }
    let (bytes, media_type) = decode_source(&input.source)?;
    let (document_id, knowledge_pack_ref) = submit_to_strata(
        &mut *tx,
        &ctx,
        &input.title,
        &bytes,
        &media_type,
        input.strata_store_id,
    )
    .await?;
    let meta = json!({
        "source_kind": input.source.kind,
        "media_type": media_type,
        "filename": input.source.filename,
        "sha256": sha256_hex(&bytes),
        "document_id": document_id,
        "knowledge_pack_ref": knowledge_pack_ref,
    });
    let source_asset_id = create_source_asset(
        &mut *tx,
        &ctx,
        &input.title,
        &input.source_key,
        input.project_id,
        meta,
    )
    .await?;
    Ok(HandlerOutput::created(IngestRdfOut {
        source_asset_id,
        document_id,
        knowledge_pack_ref,
    }))
}

/// Validate a derived draft before its publication gate.
#[crate::action(
    name = "procedures.validate",
    version = 1,
    kind = "internal",
    scopes = ["processes:read"],
    idempotent = true,
    timeout_ms = 30_000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn procedures_validate(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ValidateIn,
) -> Result<ValidateOut, ActionError> {
    let process = store::process::get_process_tx(&mut *tx, ctx.tenant, input.process_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "read the draft to validate");
            ActionError::internal()
        })?
        .ok_or_else(|| ActionError::resource_not_found("process"))?;
    let steps = store::process::list_steps_tx(&mut *tx, ctx.tenant, process.id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "read the draft's steps");
            ActionError::internal()
        })?;
    let mut checks = Vec::new();
    for (index, step) in steps.iter().enumerate() {
        conveyor_domain::pddl::parse_preconditions("pddl_preconditions", &step.pddl_preconditions)
            .and_then(|_| conveyor_domain::pddl::parse_effects("pddl_effects", &step.pddl_effects))
            .map_err(|error| {
                ActionError::new(
                    422,
                    "pddl.invalid",
                    format!("step {}: {}", index + 1, error.detail),
                )
            })?;
        if step.procedure_id.is_none() {
            return Err(ActionError::new(
                422,
                "sop.unjudged_step",
                format!("step {} names no SOP judge", index + 1),
            ));
        }
    }
    checks.push(format!(
        "{} steps carry a SOP judge and valid PDDL",
        steps.len()
    ));

    // P5.2: the SOP may ASK for a tool; the request is never granted and the draft is refused
    // until it is removed. The agent's `allowed_actions` is empty by construction.
    if let Some(source_id) = process.source_asset_id {
        let asset = store::governance::get_asset_record_tx(&mut *tx, ctx.tenant, source_id)
            .await
            .map_err(|error| {
                tracing::error!(%error, "read the draft's source");
                ActionError::internal()
            })?
            .ok_or_else(|| {
                ActionError::new(
                    422,
                    "source.missing",
                    format!(
                        "the draft was derived from source {source_id}, which no longer exists"
                    ),
                )
            })?;
        let requested: Vec<String> = serde_json::from_value(
            asset
                .meta
                .get("requested_tools")
                .cloned()
                .unwrap_or_else(|| json!([])),
        )
        .unwrap_or_default();
        if !requested.is_empty() {
            return Err(ActionError::new(
                422,
                "sop.cannot_grant_tool",
                format!(
                    "the SOP asks for tools ({}) — no SOP can grant a tool; remove the request or approve the tool through T15",
                    requested.join(", ")
                ),
            ));
        }
        checks.push("the SOP requests no tool grant".to_owned());
        checks.push(format!("the source {source_id} is {:?}", asset.status));
    }
    if process.source_asset_id.is_none() {
        checks.push("the draft is hand-authored; no source review applies".to_owned());
    }
    Ok(ValidateOut {
        valid: true,
        checks,
    })
}

/// Review a `sop_source`: the human act every derived artifact waits for (T16 P5.1).
#[crate::action(
    name = "procedures.review_source",
    version = 1,
    kind = "internal",
    scopes = ["processes:write"],
    idempotent = false,
    timeout_ms = 30_000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn procedures_review_source(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ReviewSourceIn,
) -> Result<HandlerOutput<ReviewSourceOut>, ActionError> {
    // A review is a person's act: an agent, rule or service actor cannot approve an untrusted
    // document on a human's behalf.
    if ctx.actor_kind != "user" {
        return Err(ActionError::new(
            403,
            "source.human_review_required",
            format!(
                "a source review is a person's act; this caller is {:?}",
                ctx.actor_kind
            ),
        ));
    }
    let decision = input.decision.trim();
    if decision != "approve" && decision != "reject" {
        return Err(invalid(
            "decision",
            "invalid_value",
            "must be approve or reject",
        ));
    }
    let asset = store::governance::get_asset_record_tx(&mut *tx, ctx.tenant, input.asset_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "read the source to review");
            ActionError::internal()
        })?
        .ok_or_else(|| ActionError::resource_not_found("source"))?;
    if asset.kind != AssetKind::SopSource {
        return Err(ActionError::new(
            422,
            "source.not_sop",
            "the review applies to a sop_source asset",
        ));
    }
    if asset.owner_user_id == Some(ctx.actor_id) {
        return Err(ActionError::new(
            403,
            "source.self_review_forbidden",
            "the person who supplied the source cannot review it",
        ));
    }
    if decision == "approve" {
        // The lifecycle's legal path is draft -> in_review -> approved (the domain refuses a
        // direct draft -> approved). A review IS the in-review step: the reviewer is the person
        // the gate waited for, so both transitions happen in this one act.
        if asset.status == AssetStatus::Draft {
            store::governance::transition_asset_with_prior_tx(
                &mut *tx,
                ctx.tenant,
                asset.id,
                AssetStatus::InReview,
                Some(ctx.actor_id),
            )
            .await
            .map_err(|error| {
                tracing::error!(%error, "move the source to in-review");
                ActionError::internal()
            })?;
        }
        let (updated, _prior) = store::governance::transition_asset_with_prior_tx(
            &mut *tx,
            ctx.tenant,
            asset.id,
            AssetStatus::Approved,
            Some(ctx.actor_id),
        )
        .await
        .map_err(|error| {
            tracing::error!(%error, "approve the source");
            ActionError::internal()
        })?;
        Ok(HandlerOutput::ok(ReviewSourceOut {
            asset_id: updated.id,
            status: updated.status.as_str().to_owned(),
        }))
    } else {
        // A rejection returns the asset to `draft` with the reason recorded on its meta; the
        // derived drafts stay unpublished and the event trail keeps the rejection.
        let mut patch = store::governance::AssetPatch::default();
        let mut meta = asset.meta.clone();
        if let Value::Object(map) = &mut meta {
            map.insert(
                "rejection".to_owned(),
                json!({"by": ctx.actor_id, "note": input.note}),
            );
        }
        patch.meta = Some(meta);
        let updated = store::governance::update_asset_tx(
            &mut *tx,
            ctx.tenant,
            asset.id,
            asset.version,
            &patch,
            Some(ctx.actor_id),
        )
        .await
        .map_err(|error| {
            tracing::error!(%error, "record the source rejection");
            ActionError::internal()
        })?;
        // `in_review -> draft` is the lifecycle's rejection edge; a draft that was rejected
        // before it ever entered review simply stays a draft with the reason recorded.
        if updated.status == AssetStatus::InReview {
            store::governance::transition_asset_with_prior_tx(
                &mut *tx,
                ctx.tenant,
                asset.id,
                AssetStatus::Draft,
                Some(ctx.actor_id),
            )
            .await
            .map_err(|error| {
                tracing::error!(%error, "return the rejected source to draft");
                ActionError::internal()
            })?;
        }
        Ok(HandlerOutput::ok(ReviewSourceOut {
            asset_id: updated.id,
            status: "draft".to_owned(),
        }))
    }
}
