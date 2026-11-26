//! The media group's toolkit: one action per capability kind, plus the probe (SPEC_V2 §11.3–11.4).
//!
//! A run never reaches a media capability directly. Each capability kind this module declares is
//! an action here; the action validates the capability's parameters and enqueues exactly one job
//! of that kind. `apps/server`'s runner dispatch turns the job into a pod, and its
//! `runner_image(kind)` lookup is the fail-closed readiness gate: a kind with no configured image
//! never dispatches, whatever this registry says.
//!
//! Four properties are deliberate:
//!
//! - **One action per kind, named exactly the kind.** The catalogue a broker binds to and the job
//!   kind the dispatcher looks up are the same string, so there is no mapping table to drift.
//! - **Closed schemas.** Every input type denies unknown fields and carries only parameters the
//!   executor reads (the handlers in `apps/media/src/browser/render.rs` and
//!   `apps/media/src/ffmpeg/`). A parameter the executor refuses, ignores, or answers as a no-op
//!   is not in the schema; a caller cannot smuggle it past the broker.
//! - **Effects are declared, not inferred from the verb.** The four measuring kinds are `read`;
//!   the four that produce an artefact are `write`.
//! - **The probe is the catalogue.** `media.capabilities` reads this process's own registry, so it
//!   cannot list a kind this build does not declare, or disagree about an effect class.
//!
//! Idempotency is the store's, not a wider claim: `store::jobs::enqueue_tx` dedupes on
//! `(tenant, kind, dedupe_key)` among live rows. Every action derives the key from the SHA-256 of
//! the canonical job payload, so a replay while the first job is still pending or running returns
//! that job instead of racing it with a second worker. The store keeps no history beyond the live
//! row, so a call after the job has finished starts a new job; `idempotent = true` declares exactly
//! the live-window coalescing that is implemented.

use conveyor_domain::runner::{Input, Output};
use conveyor_store as store;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::{FieldError, sha256_hex};

/// The probe: no job, no effect on tenant state.
pub const CAPABILITIES: &str = "media.capabilities";

/// `media.capabilities` takes nothing, and refuses a caller who sends anything.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CapabilitiesIn {}

/// The browser kind — an HTML document to a PDF or a PNG.
pub const RENDER_HTML: &str = "media.render_html";
/// Shots to one encoded video.
pub const COMPILE_VIDEO: &str = "media.compile_video";
/// Poster frames from a video.
pub const THUMBNAIL: &str = "media.thumbnail";
/// Audio clips joined into one track.
pub const CONCAT_AUDIO: &str = "media.concat_audio";
/// What a file is, before anything is decided about it.
pub const PROBE_VIDEO: &str = "media.probe_video";
/// The reference's own video measurements.
pub const ANALYZE_VIDEO: &str = "media.analyze_video";
/// The reference's own audio measurements.
pub const ANALYZE_AUDIO: &str = "media.analyze_audio";
/// Text out of an image.
pub const OCR: &str = "media.ocr";
/// A deliverable assembled from explicit shot definitions.
pub const PRODUCE_VIDEO: &str = "media.produce_video";
/// A browser page stepped through virtual time and encoded into a video.
pub const RECORD_BROWSER: &str = "media.record_browser";
/// A native Gherkin feature executed against one page.
pub const RUN_GHERKIN: &str = "media.run_gherkin";

/// Every kind this module publishes, in the order the group's evidence names them.
pub const KINDS: [&str; 11] = [
    RENDER_HTML,
    COMPILE_VIDEO,
    THUMBNAIL,
    CONCAT_AUDIO,
    PROBE_VIDEO,
    ANALYZE_VIDEO,
    ANALYZE_AUDIO,
    OCR,
    PRODUCE_VIDEO,
    RECORD_BROWSER,
    RUN_GHERKIN,
];

/// `media.generate_image` — the one capability served by an external model host rather than by the
/// media executor (#174).
///
/// It lives outside [`KINDS`] on purpose: `KINDS` are the job kinds the executor dispatches, and this
/// one has no executor envelope and no job row — the engine calls mini1's MOSS-Image host directly
/// and stores the render. It is still a capability this service advertises, so `capabilities()` lists
/// it (the probe reads the registry, not this list).
pub const GENERATE_IMAGE: &str = "media.generate_image";

/// The capability kinds [`KINDS`] does not cover: declared here so the probe's test can name them.
///
/// `media.transcribe` joins `media.generate_image`: both are served directly by an external model
/// host rather than by an executor job, and both spend the tenant's money instead of writing the
/// store alone (#173).
pub const DIRECT_KINDS: [&str; 2] = [GENERATE_IMAGE, "media.transcribe"];

/// The kinds that produce an artefact. The other four only measure.
pub const WRITE_KINDS: [&str; 7] = [
    RENDER_HTML,
    COMPILE_VIDEO,
    THUMBNAIL,
    CONCAT_AUDIO,
    PRODUCE_VIDEO,
    RECORD_BROWSER,
    RUN_GHERKIN,
];

/// One dispatch plus the single infrastructure retry the runner ledger allows.
const MAX_ATTEMPTS: i32 = 2;

/// The page-segmentation modes the OCR executor serves (`apps/media/src/ffmpeg/ocr.rs`).
const OCR_PSM: [u32; 6] = [3, 6, 7, 11, 12, 13];

/// The languages the OCR image installs.
const OCR_LANGS: [&str; 1] = ["eng"];

fn invalid(field: &str, detail: impl Into<String>) -> ActionError {
    ActionError::invalid_input(vec![FieldError {
        field: field.to_owned(),
        code: "invalid_value".into(),
        detail: detail.into(),
    }])
}

/// A bare file name: what the executor may place directly in its work or output directory.
///
/// The executors resolve names inside the job's own directories; accepting a path separator here
/// would turn the broker into a way to name a file the job was never given.
fn require_file_name(field: &str, value: &str) -> Result<(), ActionError> {
    if value.is_empty()
        || value.len() > 128
        || value == "."
        || value == ".."
        || !value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '.' | '-')
        })
    {
        return Err(invalid(
            field,
            "must match ^[A-Za-z0-9_.-]+$ and be at most 128 characters",
        ));
    }
    Ok(())
}

/// An input the executor reads from its work directory: a bare name, or the `inputs/<name>` path
/// the runner downloads a declared input to.
fn require_input_name(field: &str, value: &str) -> Result<(), ActionError> {
    match value.strip_prefix("inputs/") {
        Some(leaf) => require_file_name(field, leaf),
        None => require_file_name(field, value),
    }
}

/// A media source reference. `concat_audio` and `thumbnail` strip a `file://` prefix and refuse a
/// remote URL by name; the other kinds take a work-directory path directly.
fn require_media_source(field: &str, value: &str) -> Result<(), ActionError> {
    if value.starts_with("http://") || value.starts_with("https://") {
        return Err(invalid(
            field,
            "a media job has no network egress; pass a file in the job's work directory",
        ));
    }
    require_input_name(field, value.strip_prefix("file://").unwrap_or(value))
}

/// The largest body this build accepts for one media artefact.
///
/// `runner/v1` makes the executor check its own `Output.max_bytes` and the route check the same
/// number again, so this is the one place a kind's ceiling is chosen. 256 MiB is the ceiling the
/// compress/record kinds were captured with; a kind that can exceed it must say so here.
const MAX_ARTEFACT_BYTES: i64 = 256 * 1024 * 1024;

/// The job envelope the runner dispatch reads: the params, the documents, the closed output set.
///
/// `Instruction.inputs` and `Instruction.outputs` are the only way bytes cross the pod boundary
/// (`apps/server/src/runner_dispatch.rs::instruction_for` reads exactly these two keys), and the
/// executor uploads ONLY a declared name (`apps/media/src/common/runner.rs`). An action that
/// emitted neither produced a job that could not read a file and could not persist one.
fn job_payload(params: Value, inputs: Vec<Input>, outputs: Vec<Output>) -> Value {
    if inputs.is_empty() && outputs.is_empty() {
        return json!({ "params": params });
    }
    json!({ "params": params, "inputs": inputs, "outputs": outputs })
}

/// Resolve the caller's `documents` map into `runner/v1` inputs.
///
/// A document must be `active` and carry bytes — the same three conditions
/// `crate::documents::actions::fetch_bytes` enforces, because the route the pod then calls applies
/// them again and a job that starts only to fail `404` on its first download is worse than a 400
/// here. The hash is the document's own, so the executor can refuse a download that does not
/// match before it renders anything.
async fn declared_inputs(
    tx: &mut PgConnection,
    tenant: conveyor_domain::TenantId,
    documents: Option<std::collections::BTreeMap<String, Uuid>>,
) -> Result<Vec<Input>, ActionError> {
    let Some(documents) = documents else {
        return Ok(Vec::new());
    };
    let mut inputs = Vec::with_capacity(documents.len());
    for (name, document_id) in documents {
        require_input_name("documents", &name)?;
        let record = store::document::get_document_tx(&mut *tx, tenant, document_id)
            .await
            .map_err(|error| {
                tracing::error!(%error, %document_id, "media: read the input document");
                ActionError::internal()
            })?
            .ok_or_else(|| {
                invalid(
                    "documents",
                    "names a document this tenant does not have, or that has no bytes",
                )
            })?;
        let (hash, content_type) = match (record.status.as_str(), record.object_key.as_deref()) {
            ("active", Some(_)) => (
                record
                    .content_hash
                    .clone()
                    .ok_or_else(ActionError::internal)?,
                record
                    .meta
                    .get("content_type")
                    .and_then(Value::as_str)
                    .unwrap_or("application/octet-stream")
                    .to_owned(),
            ),
            _ => {
                return Err(invalid(
                    "documents",
                    "names a document that is not active or has no stored bytes",
                ));
            }
        };
        let bytes = record
            .meta
            .get("size_bytes")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        inputs.push(Input {
            name,
            document_id,
            sha256: hash,
            content_type,
            bytes,
        });
    }
    Ok(inputs)
}

/// One artefact a kind writes, as the executor will name it in `out_dir`.
#[derive(Debug)]
struct Declared {
    name: String,
    content_type: &'static str,
}

/// What a kind writes, derived from the params the executor will receive.
///
/// Every arm mirrors a specific executor line; the name must match byte for byte or the executor
/// fails `output_missing` after doing the work. A kind that legitimately writes nothing returns
/// an empty set, and the reason is stated — `outputs: []` is always a decision here.
fn declared_outputs(kind: &str, params: &Value) -> Vec<Declared> {
    let text = |field: &str| params.get(field).and_then(Value::as_str);
    match kind {
        // browser/render.rs:78 — `format!("{name}.{format}")`, name `render`, format `pdf`.
        RENDER_HTML => {
            let name = text("name").unwrap_or("render");
            let format = text("format").unwrap_or("pdf");
            let content_type = if format == "png" {
                "image/png"
            } else {
                "application/pdf"
            };
            vec![Declared {
                name: format!("{name}.{format}"),
                content_type,
            }]
        }
        // ffmpeg/compile.rs:92 — `DEFAULT_NAME = "compile.mp4"`, the param verbatim otherwise.
        COMPILE_VIDEO => vec![Declared {
            name: text("name").unwrap_or("compile.mp4").to_owned(),
            content_type: "video/mp4",
        }],
        // browser/produce.rs:1892 reads `params.name`, which ProduceVideoIn never emits, so the
        // artefact is always its DEFAULT_NAME.
        PRODUCE_VIDEO => vec![Declared {
            name: "produce.mp4".to_owned(),
            content_type: "video/mp4",
        }],
        // browser/record.rs:182 — `{base}.mp4`, base = `name` (never emitted by the action) else
        // `part_id` else `recording`.
        RECORD_BROWSER => {
            let base = text("part_id").unwrap_or("recording");
            let name = if base.to_ascii_lowercase().ends_with(".mp4") {
                base.to_owned()
            } else {
                format!("{base}.mp4")
            };
            vec![Declared {
                name,
                content_type: "video/mp4",
            }]
        }
        // ⛔ NOT DECLARED, with the reason, because a silent `[]` is how an artefact goes missing:
        //
        // `media.thumbnail` and `media.concat_audio` write under `{namespace}/` (thumbnail.rs:88,
        // concat.rs:173) and `POST /api/jobs/{id}/outputs/{name}` is ONE path segment, so no route
        // can carry that name. `media.run_gherkin` names its artefacts per scenario
        // (gherkin.rs:159), which no up-front closed set can enumerate. `media.probe_video`,
        // `media.analyze_video`, `media.analyze_audio` and `media.ocr` write no artefact at all —
        // their answer IS the result document. Flattening the first two is a follow-up unit; see
        // the deviation table in #185.
        _ => Vec::new(),
    }
}

/// Enqueue one job of `kind` and return its id, or the id of the live job this call joined.
///
/// The dedupe key is the SHA-256 of the exact payload the dispatcher will read. Two calls that
/// differ in one parameter therefore produce two jobs, and two identical calls made while the
/// first is live produce one.
async fn enqueue_job(
    tx: &mut PgConnection,
    ctx: &HandlerCtx,
    kind: &'static str,
    params: Value,
    documents: Option<std::collections::BTreeMap<String, Uuid>>,
) -> Result<MediaJobOut, ActionError> {
    let inputs = declared_inputs(tx, ctx.tenant, documents).await?;
    let outputs: Vec<Output> = declared_outputs(kind, &params)
        .into_iter()
        .map(|declared| Output {
            name: declared.name,
            content_type: declared.content_type.to_owned(),
            max_bytes: MAX_ARTEFACT_BYTES,
        })
        .collect();
    let payload = job_payload(params, inputs, outputs);
    let encoded = serde_json::to_vec(&payload).map_err(|error| {
        tracing::error!(%error, kind, "media: the job payload does not serialise");
        ActionError::internal()
    })?;
    let dedupe = sha256_hex(&encoded);
    let enqueued = store::jobs::enqueue_tx(
        &mut *tx,
        ctx.tenant,
        kind,
        &dedupe,
        payload,
        None,
        MAX_ATTEMPTS,
    )
    .await
    .map_err(|error| {
        tracing::error!(%error, kind, "media: enqueue");
        ActionError::internal()
    })?;
    let job_id = match enqueued {
        Some(job_id) => job_id,
        // A live job already holds this key. The newest row is the live one; the store's partial
        // unique index is what makes this a read rather than a race.
        None => store::jobs::by_dedupe(&mut *tx, ctx.tenant, kind, &dedupe)
            .await
            .map_err(|error| {
                tracing::error!(%error, kind, "media: read the live job");
                ActionError::internal()
            })?
            .into_iter()
            .next()
            .map(|job| job.id)
            .ok_or_else(ActionError::internal)?,
    };
    Ok(MediaJobOut {
        job_id,
        kind: kind.to_owned(),
    })
}

// ── views ────────────────────────────────────────────────────────────────────────────────

/// What a media action returns: the job the broker polls, and the kind it will execute.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct MediaJobOut {
    /// The `job` row; the runner dispatch claims it and turns it into a pod.
    pub job_id: Uuid,
    /// The capability kind, exactly the action's own name.
    pub kind: String,
}

/// One kind in the probe's answer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MediaCapabilityView {
    /// The kind, e.g. `media.render_html`.
    pub kind: String,
    /// `read` or `write`, as the action declares it.
    pub effect: String,
}

/// `media.capabilities` output — this build's media catalogue.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MediaCapabilitiesOut {
    /// Every media kind this build publishes, sorted by kind.
    pub kinds: Vec<MediaCapabilityView>,
}

/// The probe document, read from this process's own registry.
pub fn capabilities() -> MediaCapabilitiesOut {
    let mut kinds: Vec<MediaCapabilityView> = crate::registry::registry()
        .iter()
        .filter(|spec| spec.name.starts_with("media.") && spec.name != CAPABILITIES)
        .map(|spec| MediaCapabilityView {
            kind: spec.name.to_string(),
            effect: spec.effect.as_str().to_owned(),
        })
        .collect();
    kinds.sort_by(|left, right| left.kind.cmp(&right.kind));
    MediaCapabilitiesOut { kinds }
}

// ── shared parameter shapes ──────────────────────────────────────────────────────────────

/// The `tags` object the concat and thumbnail executors read. Only `run_id` is honoured; it
/// namespaces the output directory.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MediaTagsIn {
    /// The run identifier the executor sanitises into an output namespace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
}

// ── media.render_html ────────────────────────────────────────────────────────────────────

/// `pdf` or `png`. The executor's default is `pdf`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum RenderFormat {
    Png,
    Pdf,
}

/// `media.render_html` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RenderHtmlIn {
    /// Stored documents this job's work directory must contain, keyed by the name the params
    /// reference (`video_url: "clip.mp4"` pairs with `{"clip.mp4": <document id>}`). Read by the
    /// action, resolved to a hash and content type, and sent to the executor as `runner/v1`
    /// inputs — never echoed back into `params`.
    #[serde(default, skip_serializing)]
    pub documents: Option<std::collections::BTreeMap<String, Uuid>>,
    /// `pdf` (default) or `png`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<RenderFormat>,
    /// The PNG viewport width; defaults to 1280.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    /// The PNG viewport height; defaults to 720.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    /// Virtual-time budget in milliseconds; absent means no budget.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait_ms: Option<u64>,
    /// The output file stem; defaults to `render`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The document itself. Pass exactly one of `html` and `input`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub html: Option<String>,
    /// A document already in the job's work directory. Pass exactly one of `html` and `input`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,
}

impl RenderHtmlIn {
    fn params(self) -> Result<Value, ActionError> {
        match (&self.html, &self.input) {
            (Some(html), None) if html.is_empty() => {
                return Err(invalid("html", "must have at least 1 character"));
            }
            (Some(_), None) => {}
            (None, Some(input)) => require_input_name("input", input)?,
            (Some(_), Some(_)) => {
                return Err(invalid(
                    "input",
                    "pass exactly one of html or input, not both",
                ));
            }
            (None, None) => return Err(invalid("html", "one of html or input is required")),
        }
        if let Some(name) = &self.name {
            require_file_name("name", name)?;
        }
        serde_json::to_value(self).map_err(|_| ActionError::internal())
    }
}

// ── media.compile_video ─────────────────────────────────────────────────────────────────

/// How a shot's source fills the canvas.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum CompileFit {
    Contain,
    Cover,
}

/// One shot of a compile request.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompileShotIn {
    /// The shot id, `^[A-Za-z0-9_.-]+$`, at most 128 characters.
    pub id: String,
    /// The shot's seconds on the timeline, in `(0, 3600]`.
    pub seconds: f64,
    /// Footage in the job's work directory. At most one of `video_url` and `image_url`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub video_url: Option<String>,
    /// A still in the job's work directory. At most one of `video_url` and `image_url`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    /// Seconds skipped at the head of the footage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clip_lead_seconds: Option<f64>,
    /// Seconds consumed from the footage; must exceed `clip_lead_seconds`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clip_out_seconds: Option<f64>,
    /// Playback speed in `[0.25, 4.0]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed: Option<f64>,
    /// How the source fills the canvas; `contain` by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fit: Option<CompileFit>,
    /// The shot's declared source rate; the master cadence selector reads it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_fps: Option<f64>,
    /// The line this shot belongs to; only emitted when `emit_shot_timeline` is true.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_id: Option<String>,
}

impl CompileShotIn {
    fn validate(&self, index: usize) -> Result<(), ActionError> {
        let id = format!("shots.{index}.id");
        if self.id.is_empty()
            || self.id.len() > 128
            || !self.id.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '_' | '.' | '-')
            })
        {
            return Err(invalid(
                &id,
                "must match ^[A-Za-z0-9_.-]+$ and be at most 128 characters",
            ));
        }
        if !(self.seconds > 0.0 && self.seconds <= 3600.0) {
            return Err(invalid(
                &format!("shots.{index}.seconds"),
                "must be a number in (0, 3600]",
            ));
        }
        match (&self.video_url, &self.image_url) {
            (Some(_), Some(_)) => {
                return Err(invalid(
                    &format!("shots.{index}"),
                    "pass at most one of video_url or image_url",
                ));
            }
            (Some(video), None) => {
                require_input_name(&format!("shots.{index}.video_url"), video)?;
            }
            (None, Some(image)) => {
                require_input_name(&format!("shots.{index}.image_url"), image)?;
            }
            (None, None) => {}
        }
        if let Some(lead) = self.clip_lead_seconds
            && lead < 0.0
        {
            return Err(invalid(
                &format!("shots.{index}.clip_lead_seconds"),
                "must be a non-negative number",
            ));
        }
        if let Some(out) = self.clip_out_seconds
            && out <= 0.0
        {
            return Err(invalid(
                &format!("shots.{index}.clip_out_seconds"),
                "must be a positive number",
            ));
        }
        if let (Some(out), lead) = (self.clip_out_seconds, self.clip_lead_seconds.unwrap_or(0.0))
            && out <= lead
        {
            return Err(invalid(
                &format!("shots.{index}.clip_out_seconds"),
                "must be greater than clip_lead_seconds",
            ));
        }
        if let Some(speed) = self.speed
            && !(0.25..=4.0).contains(&speed)
        {
            return Err(invalid(
                &format!("shots.{index}.speed"),
                "must be a number in [0.25, 4.0]",
            ));
        }
        if let Some(fps) = self.target_fps
            && fps <= 0.0
        {
            return Err(invalid(
                &format!("shots.{index}.target_fps"),
                "must be a positive number",
            ));
        }
        Ok(())
    }
}

/// `media.compile_video` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompileVideoIn {
    /// Stored documents this job's work directory must contain, keyed by the name the params
    /// reference (`video_url: "clip.mp4"` pairs with `{"clip.mp4": <document id>}`). Read by the
    /// action, resolved to a hash and content type, and sent to the executor as `runner/v1`
    /// inputs — never echoed back into `params`.
    #[serde(default, skip_serializing)]
    pub documents: Option<std::collections::BTreeMap<String, Uuid>>,
    /// The shots to join, in timeline order; at least one.
    pub shots: Vec<CompileShotIn>,
    /// The master cadence; snapped to the executor's legal set. Defaults to 30.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cadence: Option<u32>,
    /// Emit the per-shot timeline in the result document.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emit_shot_timeline: Option<bool>,
    /// The output file name; defaults to `compile.mp4`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Canvas width; a positive even integer. Defaults to 1920.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    /// Canvas height; a positive even integer. Defaults to 1080.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
}

impl CompileVideoIn {
    fn params(self) -> Result<Value, ActionError> {
        if self.shots.is_empty() {
            return Err(invalid("shots", "must contain at least 1 shot"));
        }
        for (index, shot) in self.shots.iter().enumerate() {
            shot.validate(index)?;
        }
        if let Some(cadence) = self.cadence
            && cadence == 0
        {
            return Err(invalid("cadence", "must be positive"));
        }
        for (field, value) in [("width", self.width), ("height", self.height)] {
            if let Some(value) = value
                && (value == 0 || value % 2 != 0)
            {
                return Err(invalid(field, "must be a positive even integer"));
            }
        }
        if let Some(name) = &self.name {
            require_file_name("name", name)?;
        }
        serde_json::to_value(self).map_err(|_| ActionError::internal())
    }
}

// ── media.thumbnail ─────────────────────────────────────────────────────────────────────

/// `media.thumbnail` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ThumbnailIn {
    /// Stored documents this job's work directory must contain, keyed by the name the params
    /// reference (`video_url: "clip.mp4"` pairs with `{"clip.mp4": <document id>}`). Read by the
    /// action, resolved to a hash and content type, and sent to the executor as `runner/v1`
    /// inputs — never echoed back into `params`.
    #[serde(default, skip_serializing)]
    pub documents: Option<std::collections::BTreeMap<String, Uuid>>,
    /// The video in the job's work directory.
    pub video_url: String,
    /// How many candidates to sample; clamped by the executor to 1–12, default 3.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
    /// A deliverable aspect (`16x9`, `9x16`, `1x1`, `4x5`); anything else keeps the source size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    /// The output namespace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<MediaTagsIn>,
}

impl ThumbnailIn {
    fn params(self) -> Result<Value, ActionError> {
        require_media_source("video_url", &self.video_url)?;
        serde_json::to_value(self).map_err(|_| ActionError::internal())
    }
}

// ── media.concat_audio ──────────────────────────────────────────────────────────────────

/// The containers the concat executor can write.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ConcatFormat {
    Wav,
    Mp3,
    M4b,
}

/// One clip of a concat request. `seconds` is deliberately absent: the executor validates and
/// carries it as billing metadata but never reads it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConcatClipIn {
    /// The clip id, `^[A-Za-z0-9_.-]+$`, at most 128 characters.
    pub id: String,
    /// The audio file in the job's work directory.
    pub audio_url: String,
}

/// `media.concat_audio` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConcatAudioIn {
    /// Stored documents this job's work directory must contain, keyed by the name the params
    /// reference (`video_url: "clip.mp4"` pairs with `{"clip.mp4": <document id>}`). Read by the
    /// action, resolved to a hash and content type, and sent to the executor as `runner/v1`
    /// inputs — never echoed back into `params`.
    #[serde(default, skip_serializing)]
    pub documents: Option<std::collections::BTreeMap<String, Uuid>>,
    /// The clips to join, in order; at least one, at most 10 000.
    pub clips: Vec<ConcatClipIn>,
    /// `wav` (default), `mp3` or `m4b`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_format: Option<ConcatFormat>,
    /// Silence between clips, in milliseconds; at most 5 000.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gap_ms: Option<u64>,
    /// The output namespace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<MediaTagsIn>,
}

impl ConcatAudioIn {
    fn params(self) -> Result<Value, ActionError> {
        if self.clips.is_empty() {
            return Err(invalid("clips", "must contain at least one clip"));
        }
        if self.clips.len() > 10_000 {
            return Err(invalid("clips", "must contain at most 10 000 entries"));
        }
        for (index, clip) in self.clips.iter().enumerate() {
            let id = format!("clips[{index}].id");
            if clip.id.is_empty()
                || clip.id.len() > 128
                || !clip.id.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '_' | '.' | '-')
                })
            {
                return Err(invalid(
                    &id,
                    "must match ^[A-Za-z0-9_.-]+$ and be at most 128 characters",
                ));
            }
            require_media_source(&format!("clips[{index}].audio_url"), &clip.audio_url)?;
        }
        if let Some(gap) = self.gap_ms
            && gap > 5_000
        {
            return Err(invalid("gap_ms", "must be at most 5000"));
        }
        serde_json::to_value(self).map_err(|_| ActionError::internal())
    }
}

// ── media.probe_video ───────────────────────────────────────────────────────────────────

/// `media.probe_video` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProbeVideoIn {
    /// Stored documents this job's work directory must contain, keyed by the name the params
    /// reference (`video_url: "clip.mp4"` pairs with `{"clip.mp4": <document id>}`). Read by the
    /// action, resolved to a hash and content type, and sent to the executor as `runner/v1`
    /// inputs — never echoed back into `params`.
    #[serde(default, skip_serializing)]
    pub documents: Option<std::collections::BTreeMap<String, Uuid>>,
    /// The file in the job's work directory to probe.
    pub input: String,
}

impl ProbeVideoIn {
    fn params(self) -> Result<Value, ActionError> {
        require_input_name("input", &self.input)?;
        serde_json::to_value(self).map_err(|_| ActionError::internal())
    }
}

// ── media.analyze_video ─────────────────────────────────────────────────────────────────

/// `media.analyze_video` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AnalyzeVideoIn {
    /// Stored documents this job's work directory must contain, keyed by the name the params
    /// reference (`video_url: "clip.mp4"` pairs with `{"clip.mp4": <document id>}`). Read by the
    /// action, resolved to a hash and content type, and sent to the executor as `runner/v1`
    /// inputs — never echoed back into `params`.
    #[serde(default, skip_serializing)]
    pub documents: Option<std::collections::BTreeMap<String, Uuid>>,
    /// The video in the job's work directory.
    pub input: String,
    /// The frame-sampling interval in seconds; a positive number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame_interval_seconds: Option<f64>,
    /// The frame cap; a non-positive value means no cap.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_frames: Option<i64>,
    /// The `scdet` scene threshold on its 0–100 scale; defaults to 10.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene_threshold: Option<f64>,
}

impl AnalyzeVideoIn {
    fn params(self) -> Result<Value, ActionError> {
        require_input_name("input", &self.input)?;
        if let Some(interval) = self.frame_interval_seconds
            && interval <= 0.0
        {
            return Err(invalid(
                "frame_interval_seconds",
                "must be a positive number",
            ));
        }
        if let Some(threshold) = self.scene_threshold
            && !(0.0..=100.0).contains(&threshold)
        {
            return Err(invalid("scene_threshold", "must be a number in 0..=100"));
        }
        serde_json::to_value(self).map_err(|_| ActionError::internal())
    }
}

// ── media.analyze_audio ─────────────────────────────────────────────────────────────────

/// `media.analyze_audio` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AnalyzeAudioIn {
    /// Stored documents this job's work directory must contain, keyed by the name the params
    /// reference (`video_url: "clip.mp4"` pairs with `{"clip.mp4": <document id>}`). Read by the
    /// action, resolved to a hash and content type, and sent to the executor as `runner/v1`
    /// inputs — never echoed back into `params`.
    #[serde(default, skip_serializing)]
    pub documents: Option<std::collections::BTreeMap<String, Uuid>>,
    /// The audio file in the job's work directory.
    pub input: String,
}

impl AnalyzeAudioIn {
    fn params(self) -> Result<Value, ActionError> {
        require_input_name("input", &self.input)?;
        serde_json::to_value(self).map_err(|_| ActionError::internal())
    }
}

// ── media.ocr ───────────────────────────────────────────────────────────────────────────

/// `media.ocr` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OcrIn {
    /// Stored documents this job's work directory must contain, keyed by the name the params
    /// reference (`video_url: "clip.mp4"` pairs with `{"clip.mp4": <document id>}`). Read by the
    /// action, resolved to a hash and content type, and sent to the executor as `runner/v1`
    /// inputs — never echoed back into `params`.
    #[serde(default, skip_serializing)]
    pub documents: Option<std::collections::BTreeMap<String, Uuid>>,
    /// The image in the job's work directory.
    pub input: String,
    /// The installed language; this image serves `eng` only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
    /// Tesseract's page-segmentation mode: 3, 6, 7, 11, 12 or 13.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub psm: Option<u32>,
}

impl OcrIn {
    fn params(self) -> Result<Value, ActionError> {
        require_input_name("input", &self.input)?;
        if let Some(lang) = &self.lang
            && !OCR_LANGS.contains(&lang.as_str())
        {
            return Err(invalid(
                "lang",
                format!(
                    "{lang:?} is not installed in this image; available: {}",
                    OCR_LANGS.join(", ")
                ),
            ));
        }
        if let Some(psm) = self.psm
            && !OCR_PSM.contains(&psm)
        {
            return Err(invalid(
                "psm",
                format!("must be one of {OCR_PSM:?}, not {psm}"),
            ));
        }
        serde_json::to_value(self).map_err(|_| ActionError::internal())
    }
}

// ── media.produce_video ─────────────────────────────────────────────────────────────────

/// `media.produce_video` input — three captured request shapes, discriminated by which required
/// field is present.
///
/// The nested documents (`shot_list`, `theme`, `params`) are carried as JSON: their schemas are the
/// capture's own and the executor validates them. The **top level** is closed here, which is what
/// stops a broker call from smuggling a field the capability never sees.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProduceVideoIn {
    /// Stored documents this job's work directory must contain, keyed by the name the params
    /// reference (`video_url: "clip.mp4"` pairs with `{"clip.mp4": <document id>}`). Read by the
    /// action, resolved to a hash and content type, and sent to the executor as `runner/v1`
    /// inputs — never echoed back into `params`.
    #[serde(default, skip_serializing)]
    pub documents: Option<std::collections::BTreeMap<String, Uuid>>,
    /// Shot-list mode: the request's own shot definitions and supplied recordings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shot_list: Option<Value>,
    /// Animation mode: the part identifier (also the output stem).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part_id: Option<String>,
    /// Animation mode: the page the frames are rendered from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub html: Option<String>,
    /// Animation mode: the clip's length in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    /// Title-card mode: the card's heading.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Title-card mode: the line under the heading.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    /// Title-card mode: the bullet list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bullet_points: Option<Vec<String>>,
    /// Title-card mode: the card's colours.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<Value>,
    /// Title-card mode: how long the still is held, in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<f64>,
    /// The canvas width.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    /// The canvas height.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    /// The compile canvas width (shot-list mode).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_width: Option<u32>,
    /// The compile canvas height (shot-list mode).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_height: Option<u32>,
    /// The master cadence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fps: Option<u32>,
    /// The encoder's constant-rate factor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crf: Option<u32>,
    /// Animation mode: the page's own render parameters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl ProduceVideoIn {
    fn params(self) -> Result<Value, ActionError> {
        if let Some(part_id) = &self.part_id {
            require_file_name("part_id", part_id)?;
        }
        let shot_list = self.shot_list.is_some();
        let animation = self.part_id.is_some() || self.html.is_some();
        let title = self.title.is_some();
        if [shot_list, animation, title]
            .iter()
            .filter(|set| **set)
            .count()
            != 1
        {
            return Err(invalid(
                "shot_list",
                "pass exactly one of a shot_list, an animation (part_id + html), or a title card",
            ));
        }
        serde_json::to_value(self).map_err(|_| ActionError::internal())
    }
}

// ── media.record_browser ────────────────────────────────────────────────────────────────

/// `media.record_browser` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecordBrowserIn {
    /// Stored documents this job's work directory must contain, keyed by the name the params
    /// reference (`video_url: "clip.mp4"` pairs with `{"clip.mp4": <document id>}`). Read by the
    /// action, resolved to a hash and content type, and sent to the executor as `runner/v1`
    /// inputs — never echoed back into `params`.
    #[serde(default, skip_serializing)]
    pub documents: Option<std::collections::BTreeMap<String, Uuid>>,
    /// The page, as inline HTML. Pass exactly one page source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub html: Option<String>,
    /// The page, as a document already in the job's work directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,
    /// The page, as a `file://` document or — with `allow_network` — an `http(s)` URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// The output stem; the captured part id when the request carries one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part_id: Option<String>,
    /// The clip's length in milliseconds.
    pub duration_ms: u64,
    /// The cadence; defaults to the capture's measured 25.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fps: Option<u32>,
    /// The viewport width.
    pub width: u32,
    /// The viewport height.
    pub height: u32,
    /// The deployment's explicit permission to reach an `http(s)` URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_network: Option<bool>,
}

impl RecordBrowserIn {
    fn params(self) -> Result<Value, ActionError> {
        let sources = [
            self.html.is_some(),
            self.input.is_some(),
            self.url.is_some(),
        ]
        .iter()
        .filter(|set| **set)
        .count();
        if sources != 1 {
            return Err(invalid("html", "pass exactly one of html, input or url"));
        }
        if let Some(input) = &self.input {
            require_input_name("input", input)?;
        }
        if let Some(part_id) = &self.part_id {
            require_file_name("part_id", part_id)?;
        }
        if let Some(url) = &self.url {
            let remote = url.starts_with("http://") || url.starts_with("https://");
            if remote && self.allow_network != Some(true) {
                return Err(invalid(
                    "url",
                    "a network URL requires allow_network: true; the deployment grants it per kind",
                ));
            }
        }
        serde_json::to_value(self).map_err(|_| ActionError::internal())
    }
}

// ── media.run_gherkin ───────────────────────────────────────────────────────────────────

/// `media.run_gherkin` input — a Gherkin document and the page it runs against.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunGherkinIn {
    /// Stored documents this job's work directory must contain, keyed by the name the params
    /// reference (`video_url: "clip.mp4"` pairs with `{"clip.mp4": <document id>}`). Read by the
    /// action, resolved to a hash and content type, and sent to the executor as `runner/v1`
    /// inputs — never echoed back into `params`.
    #[serde(default, skip_serializing)]
    pub documents: Option<std::collections::BTreeMap<String, Uuid>>,
    /// The Gherkin feature document.
    pub feature: String,
    /// The page under test: a `file://` document, or an `http(s)` URL with `allow_network`.
    pub target_url: String,
    /// Per-scenario ceiling in milliseconds (1000–600000); absent means the executor's default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_per_test_ms: Option<u64>,
    /// How many times a failed scenario is re-run (0–10).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retries: Option<u32>,
    /// Write a screenshot into the job's output directory when a scenario fails.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screenshot_on_failure: Option<bool>,
    /// Write the step-by-step trace of a failed scenario into the job's output directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_on_failure: Option<bool>,
    /// The deployment's explicit permission to reach an `http(s)` URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_network: Option<bool>,
}

impl RunGherkinIn {
    fn params(self) -> Result<Value, ActionError> {
        if self.feature.trim().is_empty() {
            return Err(invalid("feature", "must contain at least one scenario"));
        }
        let remote =
            self.target_url.starts_with("http://") || self.target_url.starts_with("https://");
        if !remote && !self.target_url.starts_with("file://") {
            return Err(invalid(
                "target_url",
                "must be a file:// document or an http(s) URL",
            ));
        }
        if remote && self.allow_network != Some(true) {
            return Err(invalid(
                "target_url",
                "a network URL requires allow_network: true; the deployment grants it per kind",
            ));
        }
        serde_json::to_value(self).map_err(|_| ActionError::internal())
    }
}

// ── actions ──────────────────────────────────────────────────────────────────────────────

/// The probe the broker calls before binding: every media kind and its effect class.
///
/// This answers from the compiled registry, so it can never advertise a kind this build does not
/// declare, or disagree with an action about its effect. Whether an image is configured for a kind
/// is the dispatcher's own fail-closed gate (`runner_image(kind)`); the probe describes the
/// catalogue, not the deployment.
#[crate::action(
    name = "media.capabilities",
    version = 1,
    kind = "internal",
    scopes = ["capabilities:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn media_capabilities(
    _tx: &mut PgConnection,
    _ctx: HandlerCtx,
    _input: CapabilitiesIn,
) -> Result<MediaCapabilitiesOut, ActionError> {
    Ok(capabilities())
}

/// Render an HTML document to a PDF or a PNG.
#[crate::action(
    name = "media.render_html",
    version = 1,
    kind = "internal",
    scopes = ["capabilities:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn media_render_html(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: RenderHtmlIn,
) -> Result<MediaJobOut, ActionError> {
    let documents = input.documents.clone();
    enqueue_job(tx, &ctx, RENDER_HTML, input.params()?, documents).await
}

/// Compile shots into one encoded video.
#[crate::action(
    name = "media.compile_video",
    version = 1,
    kind = "internal",
    scopes = ["capabilities:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn media_compile_video(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: CompileVideoIn,
) -> Result<MediaJobOut, ActionError> {
    let documents = input.documents.clone();
    enqueue_job(tx, &ctx, COMPILE_VIDEO, input.params()?, documents).await
}

/// Extract poster candidates from a video.
#[crate::action(
    name = "media.thumbnail",
    version = 1,
    kind = "internal",
    scopes = ["capabilities:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn media_thumbnail(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ThumbnailIn,
) -> Result<MediaJobOut, ActionError> {
    let documents = input.documents.clone();
    enqueue_job(tx, &ctx, THUMBNAIL, input.params()?, documents).await
}

/// Join audio clips into one track.
#[crate::action(
    name = "media.concat_audio",
    version = 1,
    kind = "internal",
    scopes = ["capabilities:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn media_concat_audio(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ConcatAudioIn,
) -> Result<MediaJobOut, ActionError> {
    let documents = input.documents.clone();
    enqueue_job(tx, &ctx, CONCAT_AUDIO, input.params()?, documents).await
}

/// Report what a media file is.
#[crate::action(
    name = "media.probe_video",
    version = 1,
    kind = "internal",
    scopes = ["capabilities:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn media_probe_video(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ProbeVideoIn,
) -> Result<MediaJobOut, ActionError> {
    let documents = input.documents.clone();
    enqueue_job(tx, &ctx, PROBE_VIDEO, input.params()?, documents).await
}

/// Measure a video: duration, dimensions, frame rate, brightness and scene changes.
#[crate::action(
    name = "media.analyze_video",
    version = 1,
    kind = "internal",
    scopes = ["capabilities:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn media_analyze_video(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: AnalyzeVideoIn,
) -> Result<MediaJobOut, ActionError> {
    let documents = input.documents.clone();
    enqueue_job(tx, &ctx, ANALYZE_VIDEO, input.params()?, documents).await
}

/// Measure an audio track: its exact decoded duration and stream layout.
#[crate::action(
    name = "media.analyze_audio",
    version = 1,
    kind = "internal",
    scopes = ["capabilities:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn media_analyze_audio(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: AnalyzeAudioIn,
) -> Result<MediaJobOut, ActionError> {
    let documents = input.documents.clone();
    enqueue_job(tx, &ctx, ANALYZE_AUDIO, input.params()?, documents).await
}

/// Read text out of an image.
#[crate::action(
    name = "media.ocr",
    version = 1,
    kind = "internal",
    scopes = ["capabilities:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn media_ocr(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: OcrIn,
) -> Result<MediaJobOut, ActionError> {
    let documents = input.documents.clone();
    enqueue_job(tx, &ctx, OCR, input.params()?, documents).await
}

/// Assemble one deliverable from explicit shot definitions.
#[crate::action(
    name = "media.produce_video",
    version = 1,
    kind = "internal",
    scopes = ["capabilities:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn media_produce_video(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ProduceVideoIn,
) -> Result<MediaJobOut, ActionError> {
    let documents = input.documents.clone();
    enqueue_job(tx, &ctx, PRODUCE_VIDEO, input.params()?, documents).await
}

/// Record a browser page into a video.
#[crate::action(
    name = "media.record_browser",
    version = 1,
    kind = "internal",
    scopes = ["capabilities:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn media_record_browser(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: RecordBrowserIn,
) -> Result<MediaJobOut, ActionError> {
    let documents = input.documents.clone();
    enqueue_job(tx, &ctx, RECORD_BROWSER, input.params()?, documents).await
}

/// Run a Gherkin feature against one page.
#[crate::action(
    name = "media.run_gherkin",
    version = 1,
    kind = "internal",
    scopes = ["capabilities:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn media_run_gherkin(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: RunGherkinIn,
) -> Result<MediaJobOut, ActionError> {
    let documents = input.documents.clone();
    enqueue_job(tx, &ctx, RUN_GHERKIN, input.params()?, documents).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::Effect;
    use conveyor_auth::Role;

    fn spec(name: &str) -> &'static crate::spec::ActionSpec {
        crate::registry::registry()
            .get(name)
            .unwrap_or_else(|| panic!("{name} is not registered"))
    }

    #[test]
    fn every_media_kind_has_an_action_with_its_declared_effect() {
        for kind in KINDS {
            let action = spec(kind);
            assert_eq!(action.kind, crate::spec::ActionKind::Internal, "{kind}");
            let expected = if WRITE_KINDS.contains(&kind) {
                Effect::Write
            } else {
                Effect::Read
            };
            assert_eq!(action.effect, expected, "{kind} declares the wrong effect");
            assert!(action.idempotent, "{kind} must declare the store's dedupe");
        }
    }

    #[test]
    fn a_write_kind_is_not_callable_without_the_write_scope() {
        for kind in WRITE_KINDS {
            let action = spec(kind);
            assert_eq!(
                action.scope_names(),
                vec!["capabilities:write"],
                "{kind} must require the write scope"
            );
            assert!(
                !action.permits(Role::Viewer),
                "{kind}: a viewer holds no capabilities:write"
            );
            assert!(action.permits(Role::Member), "{kind}: a member may run it");
        }
        // The measuring kinds stay available to every role: they write nothing.
        for kind in [PROBE_VIDEO, ANALYZE_VIDEO, ANALYZE_AUDIO, OCR] {
            assert!(
                spec(kind).permits(Role::Viewer),
                "{kind}: a read must stay open"
            );
        }
    }

    #[test]
    fn the_probe_lists_all_eight_kinds_with_their_effects() {
        let probe = capabilities();
        assert_eq!(
            probe.kinds.len(),
            KINDS.len() + DIRECT_KINDS.len(),
            "{:?}",
            probe.kinds
        );
        let mut kinds: Vec<&str> = probe.kinds.iter().map(|view| view.kind.as_str()).collect();
        kinds.sort_unstable();
        let mut expected: Vec<&str> = KINDS.iter().chain(DIRECT_KINDS.iter()).copied().collect();
        expected.sort_unstable();
        assert_eq!(kinds, expected);
        for view in &probe.kinds {
            // A directly served capability owns its own effect: `media.generate_image` spends the
            // tenant's money rather than enqueueing an executor job.
            let expected = if DIRECT_KINDS.contains(&view.kind.as_str()) {
                "spend"
            } else if WRITE_KINDS.contains(&view.kind.as_str()) {
                "write"
            } else {
                "read"
            };
            assert_eq!(view.effect, expected, "{}", view.kind);
        }
        assert!(
            !kinds.contains(&CAPABILITIES),
            "the probe must not list itself as a capability"
        );
    }

    #[test]
    fn every_media_input_schema_is_closed() {
        for name in KINDS.iter().chain(&[CAPABILITIES]) {
            let schema = spec(name).input_schema();
            assert_eq!(
                schema["additionalProperties"],
                json!(false),
                "{name}: the input schema must refuse unknown fields"
            );
        }
    }

    #[test]
    fn render_html_requires_exactly_one_document() {
        let both = RenderHtmlIn {
            documents: None,
            format: None,
            width: None,
            height: None,
            wait_ms: None,
            name: None,
            html: Some("<p>x</p>".into()),
            input: Some("inputs/source".into()),
        };
        assert!(both.params().is_err(), "two documents must be refused");
        let neither = RenderHtmlIn {
            documents: None,
            format: None,
            width: None,
            height: None,
            wait_ms: None,
            name: None,
            html: None,
            input: None,
        };
        assert!(neither.params().is_err(), "no document must be refused");
        let empty = RenderHtmlIn {
            documents: None,
            format: None,
            width: None,
            height: None,
            wait_ms: None,
            name: None,
            html: Some(String::new()),
            input: None,
        };
        assert!(empty.params().is_err(), "an empty document must be refused");
    }

    #[test]
    fn compile_refuses_an_empty_shot_list() {
        let input = CompileVideoIn {
            documents: None,
            shots: Vec::new(),
            cadence: None,
            emit_shot_timeline: None,
            name: None,
            width: None,
            height: None,
        };
        assert!(input.params().is_err(), "a compile needs at least one shot");
    }

    #[test]
    fn ocr_refuses_a_language_and_a_mode_the_image_does_not_serve() {
        let language = OcrIn {
            documents: None,
            input: "inputs/page.png".into(),
            lang: Some("deu".into()),
            psm: None,
        };
        assert!(
            language.params().is_err(),
            "an uninstalled language is refused"
        );
        let psm = OcrIn {
            documents: None,
            input: "inputs/page.png".into(),
            lang: None,
            psm: Some(4),
        };
        assert!(psm.params().is_err(), "an unserved psm is refused");
    }
}

#[cfg(test)]
mod declared {
    use super::*;

    /// The kinds that write exactly one artefact, and the ones that write none.
    ///
    /// This table is the decision, so a new kind cannot arrive without someone choosing a side:
    /// `declared_outputs` returns an empty set for anything unlisted, and an artefact that is
    /// written but not declared is silently discarded by the executor.
    const ONE_ARTEFACT: [&str; 4] = [RENDER_HTML, COMPILE_VIDEO, PRODUCE_VIDEO, RECORD_BROWSER];

    #[test]
    fn every_kind_is_accounted_for() {
        for kind in KINDS {
            let outputs = declared_outputs(kind, &json!({}));
            let expected = usize::from(ONE_ARTEFACT.contains(&kind));
            assert_eq!(outputs.len(), expected, "{kind} declares {outputs:?}");
        }
    }

    #[test]
    fn render_names_the_file_the_executor_writes() {
        // browser/render.rs:78 — `format!("{name}.{format}")`.
        let default = declared_outputs(RENDER_HTML, &json!({}));
        assert_eq!(default.len(), 1);
        assert_eq!(default[0].name, "render.pdf");
        assert_eq!(default[0].content_type, "application/pdf");

        let png = declared_outputs(RENDER_HTML, &json!({"name": "poster", "format": "png"}));
        assert_eq!(png[0].name, "poster.png");
        assert_eq!(png[0].content_type, "image/png");
    }

    #[test]
    fn compile_names_the_file_the_executor_writes() {
        // ffmpeg/compile.rs:92 — `DEFAULT_NAME = "compile.mp4"`, the param verbatim otherwise.
        assert_eq!(
            declared_outputs(COMPILE_VIDEO, &json!({}))[0].name,
            "compile.mp4"
        );
        assert_eq!(
            declared_outputs(COMPILE_VIDEO, &json!({"name": "film.mp4"}))[0].name,
            "film.mp4"
        );
    }

    #[test]
    fn produce_and_record_name_the_files_their_executors_write() {
        // browser/produce.rs:1892 reads `params.name`, which ProduceVideoIn never emits.
        assert_eq!(
            declared_outputs(PRODUCE_VIDEO, &json!({"part_id": "p1"}))[0].name,
            "produce.mp4"
        );
        // browser/record.rs:182 — `{base}.mp4`, base = `part_id` else `recording`.
        assert_eq!(
            declared_outputs(RECORD_BROWSER, &json!({}))[0].name,
            "recording.mp4"
        );
        assert_eq!(
            declared_outputs(RECORD_BROWSER, &json!({"part_id": "anim001"}))[0].name,
            "anim001.mp4"
        );
        assert_eq!(
            declared_outputs(RECORD_BROWSER, &json!({"part_id": "anim001.mp4"}))[0].name,
            "anim001.mp4",
            "an already-suffixed stem is not doubled"
        );
    }

    #[test]
    fn the_kinds_that_write_no_artefact_declare_none() {
        // Their answer IS the result document, or their name is not expressible as one route
        // segment (`thumbnail`/`concat_audio` write under `{namespace}/`) — see the comment on
        // `declared_outputs`. Either way the empty set is deliberate.
        for kind in [
            PROBE_VIDEO,
            ANALYZE_VIDEO,
            ANALYZE_AUDIO,
            OCR,
            THUMBNAIL,
            CONCAT_AUDIO,
            RUN_GHERKIN,
        ] {
            assert!(
                declared_outputs(kind, &json!({})).is_empty(),
                "{kind} must declare no artefact"
            );
        }
    }

    #[test]
    fn a_document_map_never_leaks_into_params() {
        // `documents` is read by the action and sent as `runner/v1` inputs; the executor receives
        // it as a downloaded file, so echoing the ids back into `params` would be a second,
        // unchecked source of truth.
        let input: RenderHtmlIn = serde_json::from_value(json!({
            "html": "<p>x</p>",
            "documents": {"page.html": Uuid::new_v4()},
        }))
        .expect("the map is accepted");
        assert!(input.documents.is_some());
        let params = input.params().expect("params");
        assert!(
            params.get("documents").is_none(),
            "params must not carry the document map: {params}"
        );
    }
}
