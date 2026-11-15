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

/// Every kind this module publishes, in the order the group's evidence names them.
pub const KINDS: [&str; 10] = [
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
];

/// The kinds that produce an artefact. The other four only measure.
pub const WRITE_KINDS: [&str; 6] = [
    RENDER_HTML,
    COMPILE_VIDEO,
    THUMBNAIL,
    CONCAT_AUDIO,
    PRODUCE_VIDEO,
    RECORD_BROWSER,
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

/// The job envelope the runner dispatch reads: the capability's parameters only.
fn job_payload(params: Value) -> Value {
    json!({ "params": params })
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
) -> Result<MediaJobOut, ActionError> {
    let payload = job_payload(params);
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
            kind: spec.name.to_owned(),
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
    enqueue_job(tx, &ctx, RENDER_HTML, input.params()?).await
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
    enqueue_job(tx, &ctx, COMPILE_VIDEO, input.params()?).await
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
    enqueue_job(tx, &ctx, THUMBNAIL, input.params()?).await
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
    enqueue_job(tx, &ctx, CONCAT_AUDIO, input.params()?).await
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
    enqueue_job(tx, &ctx, PROBE_VIDEO, input.params()?).await
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
    enqueue_job(tx, &ctx, ANALYZE_VIDEO, input.params()?).await
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
    enqueue_job(tx, &ctx, ANALYZE_AUDIO, input.params()?).await
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
    enqueue_job(tx, &ctx, OCR, input.params()?).await
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
    enqueue_job(tx, &ctx, PRODUCE_VIDEO, input.params()?).await
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
    enqueue_job(tx, &ctx, RECORD_BROWSER, input.params()?).await
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
        assert_eq!(probe.kinds.len(), KINDS.len(), "{:?}", probe.kinds);
        let mut kinds: Vec<&str> = probe.kinds.iter().map(|view| view.kind.as_str()).collect();
        kinds.sort_unstable();
        let mut expected = KINDS.to_vec();
        expected.sort_unstable();
        assert_eq!(kinds, expected);
        for view in &probe.kinds {
            let expected = if WRITE_KINDS.contains(&view.kind.as_str()) {
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
            input: "inputs/page.png".into(),
            lang: Some("deu".into()),
            psm: None,
        };
        assert!(
            language.params().is_err(),
            "an uninstalled language is refused"
        );
        let psm = OcrIn {
            input: "inputs/page.png".into(),
            lang: None,
            psm: Some(4),
        };
        assert!(psm.params().is_err(), "an unserved psm is refused");
    }
}
