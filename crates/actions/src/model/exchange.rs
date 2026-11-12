#![forbid(unsafe_code)]
//! `models.render`, `models.export_xmi`, `models.import_xmi` — the exchange surface
//! (S08 §5.9, Part B2).
//!
//! # Why the payload travels inside the JSON document
//!
//! An action's output is JSON (CONTRACT §6), and that is the right shape here: the same
//! declaration becomes the OpenAPI operation and the MCP tool, and a client that receives a
//! diagram as bytes has no way to tell what it is. So a rendered document is carried as
//! `content_b64` with its `media_type`, `byte_len` and `content_sha256`, and an XMI document —
//! which is text — is carried as `xml` directly. The `GET /api/models/{id}/render` route
//! exists for browsers and streams the identical bytes; both call the same renderer.
//!
//! # Why import writes a draft revision, not a publication
//!
//! [`conveyor_store::model::write_head_revision_tx`] freezes the imported graph at
//! `seq = head_revision + 1` with status `draft`. Publishing here would be a lie: `models.publish`
//! is the act of declaring a graph reviewable, and `models.compare` compares published
//! revisions — an import that published itself would create a review record nobody performed.
//!
//! # What import refuses
//!
//! A `DOCTYPE`, a duplicate id, a document over [`MAX_XMI_BYTES`], and a declared language that
//! disagrees with the document. What it does **not** refuse is an out-of-profile construct:
//! that is reported in `unsupported[]` and the rest is imported (slice §5.7), because an Archi
//! export containing one physical-layer element is not a corrupt file.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use conveyor_domain::Version;
use conveyor_render::{Format, RENDERER_VERSION, Unsupported, export_xmi, import_xmi, render};
use conveyor_store::model::{
    NewElement, NewModel, NewRelationship, create_element_tx, create_model_tx,
    create_relationship_tx, load_snapshot_tx, write_head_revision_tx,
};
use conveyor_store::view::{
    NewConnection, NewObject, NewView, create_connection_tx, create_object_tx, create_view_tx,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::{FieldError, HandlerOutput};

use super::{MODEL_CREATED, MODEL_EXPORTED, emit_model_event, etag_of, model_error, version_i64};

/// The largest XMI document an invocation may carry, in bytes.
///
/// The action request body is axum's 2 MiB default and JSON inflates a string by escaping, so a
/// 1 MiB document is the largest that reliably arrives. `MAX_DOCUMENT_BYTES` is the codec's own
/// cap; this is the one the action enforces, and it is deliberately the smaller of the two.
pub const MAX_XMI_BYTES: usize = 1024 * 1024;

// ── models.render ─────────────────────────────────────────────────────────────────────────

/// Render one view of a model.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ModelRenderIn {
    /// The model to draw.
    pub model_id: Uuid,
    /// The view to draw. Absent draws the whole model, auto-laid-out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view_id: Option<Uuid>,
    /// `svg` (the default), `png` or `pdf`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
}

/// The rendered document, its digest, and what it was rendered from.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ModelRenderOut {
    /// The model.
    pub model_id: String,
    /// The view drawn; absent when the whole model was auto-laid-out.
    pub view_id: Option<String>,
    /// `svg`, `png` or `pdf`.
    pub format: String,
    /// The bytes' `Content-Type`.
    pub media_type: String,
    /// [`RENDERER_VERSION`] — part of the determinism key.
    pub renderer_version: String,
    /// SHA-256 of the decoded bytes; the byte route's `ETag`.
    pub content_sha256: String,
    /// The decoded length.
    pub byte_len: i64,
    /// SHA-256 of the graph that was drawn, so a caller can pin what it saw.
    pub graph_hash: String,
    /// The document, base64.
    pub content_b64: String,
}

/// Draw one view of a model.
#[crate::action(
    name = "models.render",
    version = 1,
    kind = "internal",
    scopes = ["model:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn models_render(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ModelRenderIn,
) -> Result<HandlerOutput<ModelRenderOut>, ActionError> {
    let format = match input.format.as_deref() {
        None | Some("") => Format::Svg,
        Some(name) => Format::parse(name).ok_or_else(|| {
            invalid_fields(vec![FieldError {
                field: "format".to_owned(),
                code: "invalid_value".to_owned(),
                detail: format!("{name:?} is not svg, png or pdf"),
            }])
        })?,
    };

    let model = load_snapshot_tx(&mut *tx, ctx.tenant, input.model_id)
        .await
        .map_err(|error| model_error(error, ctx.action))?;
    let graph_hash = model.content_hash();
    let document = render(&model, input.view_id, format).map_err(render_error)?;

    emit_model_event(
        &mut *tx,
        &ctx,
        MODEL_EXPORTED,
        input.model_id,
        json!({"format": format.as_str(), "view_id": input.view_id, "byte_len": document.bytes.len()}),
    )
    .await?;

    Ok(HandlerOutput::ok(ModelRenderOut {
        model_id: input.model_id.to_string(),
        view_id: input.view_id.map(|id| id.to_string()),
        format: format.as_str().to_owned(),
        media_type: document.media_type.to_owned(),
        renderer_version: RENDERER_VERSION.to_owned(),
        content_sha256: document.sha256,
        byte_len: document.bytes.len() as i64,
        graph_hash,
        content_b64: BASE64.encode(&document.bytes),
    }))
}

// ── models.export_xmi ─────────────────────────────────────────────────────────────────────

/// Export a model as XMI.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ModelExportXmiIn {
    /// The model to export.
    pub model_id: Uuid,
}

/// The document and everything the codec could not carry.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ModelExportXmiOut {
    /// The model.
    pub model_id: String,
    /// `archimate` or `uml`.
    pub language: String,
    /// The XML document.
    pub xml: String,
    /// Its namespace URI.
    pub namespace: String,
    /// `3.2` or `2.5.1`.
    pub version: String,
    /// SHA-256 of `xml`.
    pub content_sha256: String,
    /// Constructs the codec could not carry. Never silently dropped.
    pub unsupported: Vec<UnsupportedOut>,
}

/// One construct the codec could not carry.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct UnsupportedOut {
    /// `element`, `relationship`, `view`, `object` or `connection`.
    pub construct: String,
    /// The document's id for it.
    pub id: String,
    /// Why it was not carried.
    pub reason: String,
}

fn unsupported_out(items: Vec<Unsupported>) -> Vec<UnsupportedOut> {
    items
        .into_iter()
        .map(|item| UnsupportedOut {
            construct: item.construct.to_owned(),
            id: item.id,
            reason: item.reason,
        })
        .collect()
}

/// Serialise a model as XMI.
#[crate::action(
    name = "models.export_xmi",
    version = 1,
    kind = "internal",
    scopes = ["model:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn models_export_xmi(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ModelExportXmiIn,
) -> Result<HandlerOutput<ModelExportXmiOut>, ActionError> {
    let model = load_snapshot_tx(&mut *tx, ctx.tenant, input.model_id)
        .await
        .map_err(|error| model_error(error, ctx.action))?;
    let document = export_xmi(&model).map_err(render_error)?;
    let digest = conveyor_render::sha256_hex(document.xml.as_bytes());

    emit_model_event(
        &mut *tx,
        &ctx,
        MODEL_EXPORTED,
        input.model_id,
        json!({"format": "xmi", "language": model.model.language, "unsupported": document.unsupported.len()}),
    )
    .await?;

    Ok(HandlerOutput::ok(ModelExportXmiOut {
        model_id: input.model_id.to_string(),
        language: model.model.language.clone(),
        namespace: document.namespace.to_owned(),
        version: document.version,
        content_sha256: digest,
        unsupported: unsupported_out(document.unsupported),
        xml: document.xml,
    }))
}

// ── models.import_xmi ─────────────────────────────────────────────────────────────────────

/// Import a model from XMI.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ModelImportXmiIn {
    /// The project to attach the model to; absent leaves it unattached.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<Uuid>,
    /// Override the document's title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The document. `archimate` and `uml` are detected from the root element.
    pub xml: String,
}

/// What an import created.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ModelImportXmiOut {
    /// The new model.
    pub model_id: String,
    /// The draft revision written for it.
    pub revision_id: String,
    /// That revision's `seq`.
    pub seq: i64,
    /// `revision.content_hash` — SHA-256 of the canonical graph.
    pub content_hash: String,
    /// The model's version, for `If-Match`.
    pub version: i64,
    /// `archimate` or `uml`.
    pub language: String,
    /// Elements created.
    pub elements: i64,
    /// Relationships created.
    pub relationships: i64,
    /// Views created.
    pub views: i64,
    /// Constructs the codec could not carry.
    pub unsupported: Vec<UnsupportedOut>,
}

/// Import a model from an XMI document.
#[crate::action(
    name = "models.import_xmi",
    version = 1,
    kind = "internal",
    scopes = ["model:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn models_import_xmi(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ModelImportXmiIn,
) -> Result<HandlerOutput<ModelImportXmiOut>, ActionError> {
    if input.xml.len() > MAX_XMI_BYTES {
        return Err(invalid_fields(vec![FieldError {
            field: "xml".to_owned(),
            code: "too_large".to_owned(),
            detail: format!(
                "the document is {} bytes; the cap for an invocation is {MAX_XMI_BYTES}",
                input.xml.len()
            ),
        }]));
    }
    let document = import_xmi(&input.xml).map_err(render_error)?;
    let title = input
        .title
        .clone()
        .filter(|title| !title.trim().is_empty())
        .unwrap_or(document.title.clone());

    let model = create_model_tx(
        &mut *tx,
        ctx.tenant,
        &NewModel {
            project_id: input.project_id,
            language: document.language.clone(),
            language_version: document.language_version.clone(),
            title,
            description: document.description.clone(),
            meta: json!({"import": {"source": "xmi"}}),
        },
    )
    .await
    .map_err(|error| model_error(error, ctx.action))?;

    // The document's ids are only meaningful inside the document — the store mints a fresh
    // uuid for every row (there is no id on `NewElement`). This map is what makes a
    // relationship's endpoints resolve to the elements the document meant.
    let mut elements = std::collections::BTreeMap::new();
    for element in &document.elements {
        let created = create_element_tx(
            &mut *tx,
            ctx.tenant,
            &NewElement {
                model_id: model.id,
                kind: element.kind.clone(),
                name: element.name.clone(),
                documentation: element.documentation.clone(),
                parent_element_id: element.parent_id.and_then(|id| elements.get(&id).copied()),
                meta: json!({}),
            },
        )
        .await
        .map_err(|error| model_error(error, ctx.action))?;
        elements.insert(element.id, created.id);
    }

    let mut relationships = std::collections::BTreeMap::new();
    for relationship in &document.relationships {
        let (Some(source), Some(target)) = (
            elements.get(&relationship.source_id).copied(),
            elements.get(&relationship.target_id).copied(),
        ) else {
            // The codec reports a skipped endpoint; reaching here would mean it did not.
            continue;
        };
        let created = create_relationship_tx(
            &mut *tx,
            ctx.tenant,
            &NewRelationship {
                model_id: model.id,
                kind: relationship.kind.clone(),
                source_id: source,
                target_id: target,
                meta: json!({}),
            },
        )
        .await
        .map_err(|error| model_error(error, ctx.action))?;
        relationships.insert(relationship.id, created.id);
    }

    let mut views = 0i64;
    for view in &document.views {
        let created_view = create_view_tx(
            &mut *tx,
            ctx.tenant,
            &NewView {
                model_id: model.id,
                kind: view.kind.clone(),
                name: view.name.clone(),
                meta: match &view.diagram_kind {
                    Some(kind) => json!({"diagram_kind": kind}),
                    None => json!({}),
                },
            },
        )
        .await
        .map_err(|error| model_error(error, ctx.action))?;
        views += 1;

        let mut objects = std::collections::BTreeMap::new();
        for object in &view.objects {
            let created = create_object_tx(
                &mut *tx,
                ctx.tenant,
                &NewObject {
                    view_id: created_view.id,
                    element_id: object.element_id.and_then(|id| elements.get(&id).copied()),
                    parent_object_id: object.parent_id.and_then(|id| objects.get(&id).copied()),
                    kind: object.kind.clone(),
                    bounds: object.bounds.clone(),
                    style: json!({}),
                    label: object.label.clone(),
                    position: object.position,
                },
            )
            .await
            .map_err(|error| model_error(error, ctx.action))?;
            objects.insert(object.id, created.id);
        }

        for connection in &view.connections {
            let (Some(source), Some(target)) = (
                objects.get(&connection.source_id).copied(),
                objects.get(&connection.target_id).copied(),
            ) else {
                continue;
            };
            create_connection_tx(
                &mut *tx,
                ctx.tenant,
                &NewConnection {
                    view_id: created_view.id,
                    relationship_id: connection
                        .relationship_id
                        .and_then(|id| relationships.get(&id).copied()),
                    source_object_id: source,
                    target_object_id: target,
                    bendpoints: connection.bendpoints.clone(),
                    style: json!({}),
                },
            )
            .await
            .map_err(|error| model_error(error, ctx.action))?;
        }
    }

    // A draft revision: the graph is recorded and `head_revision` advances, but nothing is
    // published — see the module docs.
    let frozen = write_head_revision_tx(&mut *tx, ctx.tenant, model.id, Version::INITIAL, None)
        .await
        .map_err(|error| model_error(error, ctx.action))?;

    let unsupported = unsupported_out(document.unsupported);
    emit_model_event(
        &mut *tx,
        &ctx,
        MODEL_CREATED,
        model.id,
        json!({
            "model_id": model.id,
            "language": document.language,
            "imported": {
                "elements": elements.len(),
                "relationships": relationships.len(),
                "views": views,
            },
            "unsupported": unsupported,
        }),
    )
    .await?;

    let etag = etag_of(version_i64(frozen.model.version));
    Ok(HandlerOutput::created(ModelImportXmiOut {
        model_id: model.id.to_string(),
        revision_id: frozen.revision_id.to_string(),
        seq: i64::from(frozen.seq),
        content_hash: frozen.content_hash,
        version: version_i64(frozen.model.version),
        language: model.language,
        elements: elements.len() as i64,
        relationships: relationships.len() as i64,
        views,
        unsupported,
    })
    .with_etag(etag))
}

/// `invalid` for a whole error vector — the family helper takes one field at a time.
fn invalid_fields(errors: Vec<FieldError>) -> ActionError {
    ActionError::invalid_input(errors)
}

/// A renderer or codec refusal, as the action's problem.
fn render_error(error: conveyor_render::RenderError) -> ActionError {
    use conveyor_render::RenderError;
    match error {
        RenderError::ViewNotFound(id) => ActionError::resource_not_found(&format!("view {id}")),
        RenderError::MissingGlyph { font, character } => invalid_fields(vec![FieldError {
            field: "model_id".to_owned(),
            code: "missing_glyph".to_owned(),
            detail: format!("the embedded face {font} has no glyph for {character:?}"),
        }]),
        RenderError::TooLarge { format, bytes, cap } => invalid_fields(vec![FieldError {
            field: "format".to_owned(),
            code: "too_large".to_owned(),
            detail: format!("the {format} document is {bytes} bytes, over the {cap}-byte cap"),
        }]),
        RenderError::UnknownProfile { language, version } => invalid_fields(vec![FieldError {
            field: "model_id".to_owned(),
            code: "unknown_profile".to_owned(),
            detail: format!("no profile for {language} {version}"),
        }]),
        RenderError::Xmi(detail) => invalid_fields(vec![FieldError {
            field: "xml".to_owned(),
            code: "invalid_document".to_owned(),
            detail,
        }]),
        other => {
            tracing::error!(%other, "rendering failed");
            ActionError::internal()
        }
    }
}
