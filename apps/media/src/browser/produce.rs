//! `media.produce_video` — a deliverable assembled from the request's own explicit definitions.
//!
//! Three request shapes from `contract/video_production_service/` are served here, discriminated
//! by their captured required fields:
//!
//! * **shot list** (`CompileRequest`) — `shot_list` + `recordings`. Each part names either a
//!   supplied video (through its `recordings` entry), a supplied image (`image_url` /
//!   `slide_image_url`) or title-card text, plus the duration it occupies. Each shot is
//!   normalised to one canvas and cadence, encoded as an intermediate, and the intermediates are
//!   joined by the concat demuxer — the reference's documented intended
//!   `initialize → process_segments → assemble_video` pipeline.
//! * **animation** (`RecordAnimationRequest`) — `part_id` + `html`. The measured reference path is
//!   frame-stepped: seek the page to `t`, screenshot a PNG, encode the sequence. This port injects
//!   `window.render(t)` into the job's own copy of the page and takes one headless Chromium
//!   screenshot per frame, then encodes the PNGs with ffmpeg. That is the reference's mechanism,
//!   not a wall-clock screencast.
//! * **title card** (`TitleCardRequest`) — `title` (+ `subtitle`, `bullet_points`, an explicit
//!   `theme`). The reference paints the card with Pillow; this port paints the same text on a
//!   styled page in the image's own Chromium, screenshots it, and encodes the still for the
//!   requested duration. Text → mp4, the measured working path.
//!
//! ## The reference defect, and what is *not* claimed
//!
//! `POST /api/compile` cannot complete on the captured digest: every job dies at `_initialize`
//! with `AttributeError: 'CompilationJob' object has no attribute 'fps'` (`compiler_service.py:525`)
//! and the row stays `processing` forever — production runs that same digest. The capture
//! therefore pins **intake only**; no completed compile golden exists. The shot-list path here has
//! **no completed-render parity claim**. Its mechanism is the reference's documented intended
//! pipeline and its evidence is this module's own tests, which produce real bytes and probe them.
//! This port also does not copy the defect: the cadence is a request field (`fps`), never an ORM
//! attribute or a preset lookup.
//!
//! ## Declared deviations
//!
//! * **Catalogue references are refused, not embedded.** `preset`, `template`/`template_id`,
//!   `theme_name` and the animation `quality_preset` need catalogue rows the executor does not
//!   carry (the reference's drop table gives them to the config/ledger store). The executor takes
//!   explicit `fps`, `crf`, canvas and colours instead.
//! * **Title-card layout is this port's own.** The capture pins the request schema, the theme field
//!   set and the catalogue's default theme values, not the card's pixels. The mapping used here is
//!   documented at [`card_html`]: background from `gradient_start`/`gradient_end` (or
//!   `background_color`), title in `text_color`, subtitle in `subtitle_color`, the rule in
//!   `primary_color`, bullet markers in `accent_color`.
//! * **Cadence.** The animation cadence default (30) is the capture's own `fps` default. The
//!   captured `CompileRequest` and `TitleCardRequest` carry **no** frame rate, so the executor
//!   accepts an explicit `fps` and defaults to 30; the capture pins the title card's width, height
//!   and duration, not its frame rate.
//! * **Animation seeking.** The reference seeks a live Playwright page. This port writes one
//!   deterministic page per frame (the caller's HTML with `window.render(t_ms)` invoked on load)
//!   and screenshots it — same frame-stepped PNG → ffmpeg result, no long-lived browser session.
//! * **Result field names.** The captured `TitleCardResponse` returns an absolute `video_path`
//!   (which embeds a fresh `titlecard-<uuid7>` directory) and `duration`. The executor emits a bare
//!   `output` file name and `duration_seconds`, and never mints `job_id`/`poll_url`.
//! * **Title cards carry no audio track** (the capture does not say one exists); the animation
//!   encode carries the silent AAC track the measured reference output has.
//!
//! Everything a caller asks for that this port does not honour is refused **by name** with a
//! stable [`code`] — nothing is accepted and silently ignored.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use crate::common::{CapabilityError, Config, process};

/// The kind slug this module implements.
pub const KIND: &str = "media.produce_video";

/// The stable refusal codes. Each names one capability this port does not implement, so a caller
/// can branch on the code instead of reading a sentence.
pub mod code {
    // Shot-list mode: catalogue references and unsupported edit/assembly stages.
    pub const PRESET: &str = "preset_not_supported";
    pub const TEMPLATE: &str = "template_not_supported";
    pub const THEME: &str = "theme_not_supported";
    pub const RECORDING_ORDER: &str = "recording_order_not_supported";
    pub const TRANSITION: &str = "transition_not_supported";
    pub const EFFECTS: &str = "effects_not_supported";
    pub const REMOTE_URL: &str = "remote_url_not_supported";
    pub const CLIPS_DIR: &str = "clips_dir_not_supported";
    pub const NARRATION: &str = "narration_not_supported";
    pub const LLM_EDIT: &str = "llm_edit_not_supported";
    pub const WEBHOOK: &str = "webhook_not_supported";
    pub const TRIM_END: &str = "trim_end_not_supported";
    pub const SPEED_CHANGE: &str = "speed_change_not_supported";
    pub const DEAD_TIME: &str = "dead_time_removal_not_supported";
    // Shot-list parts: reference fields this port does not render.
    pub const BACKGROUND_IMAGE: &str = "background_image_not_supported";
    pub const LOGO: &str = "logo_not_supported";
    pub const AI_GENERATION: &str = "ai_generation_not_supported";
    pub const ASSET_REFERENCE: &str = "asset_reference_not_supported";
    pub const MOTION_PRESET: &str = "motion_preset_not_supported";
    pub const CHAPTER: &str = "chapter_not_supported";
    pub const ANIMATION: &str = "animation_not_supported";
    pub const SLIDE_ROLE: &str = "slide_role_not_supported";
    pub const CLIP_TYPE: &str = "clip_type_not_supported";
    pub const PROVENANCE: &str = "provenance_not_supported";
    pub const APPROVAL_STATUS: &str = "approval_status_not_supported";
    pub const AI_AUDIO: &str = "ai_audio_not_supported";
    // Title-card mode.
    pub const FONT: &str = "custom_font_not_supported";
    // Animation mode.
    pub const OUTPUT_FORMAT: &str = "output_format_not_supported";
    pub const TRANSPARENT: &str = "transparent_not_supported";
    pub const DURATION_MS_REQUIRED: &str = "duration_ms_required";
}

/// The browser binary. The image installs Chromium at `/usr/bin/chromium`; a developer box points
/// this at whatever Chrome it has (the same convention as [`crate::browser::render`]).
const CHROME_ENV: &str = "CHROME_BIN";
const CHROME_DEFAULT: &str = "/usr/bin/chromium";

/// The render canvas when the request names none. The captured `CompileRequest` leaves
/// `target_width`/`target_height` null; this is the delivery default `compile.rs` uses too.
const DEFAULT_WIDTH: u32 = 1920;
const DEFAULT_HEIGHT: u32 = 1080;

/// The animation schema's own `fps` default, and the executor default for the two paths whose
/// captured schema names no frame rate (declared in the module docs).
const DEFAULT_CADENCE: u32 = 30;

/// The capture names CRF 23 as the animation service's default (`quality_preset` "standard").
const DEFAULT_CRF: u32 = 23;

/// The captured `TitleCardRequest.duration` default.
const DEFAULT_TITLE_DURATION: f64 = 5.0;

/// The bare output name when the request does not supply the executor's `name`.
const DEFAULT_NAME: &str = "produce.mp4";

/// Time a rendered page is given to settle before the screenshot (fonts, layout, the injected
/// `render(t)`). The reference waits for the page too; a zero budget can screenshot a half-laid
/// page.
const SETTLE_MS: u64 = 250;

/// `RecordAnimationRequest.html`'s captured `maxLength`.
const MAX_HTML_CHARS: usize = 5_000_000;

/// The brand navy the reference pads a letterboxed frame with (`_pad_color`).
const PAD_COLOUR: &str = "0x0B1F3A";

/// One shot may not occupy more than this; the captured shot list has no published bound, and a
/// request that asks for an hour-long shot is not a per-shot render.
const MAX_SECONDS: f64 = 3600.0;

/// The captured default theme (`GET /api/compile/themes/default`), used when the request supplies
/// no theme. One explicit default, not the catalogue: the other five themes are not embedded.
const THEME_BACKGROUND: &str = "#0f172a";
const THEME_GRADIENT_END: &str = "#1e293b";
const THEME_TEXT: &str = "#f8fafc";
const THEME_SUBTITLE: &str = "#94a3b8";
const THEME_PRIMARY: &str = "#6366f1";
const THEME_ACCENT: &str = "#818cf8";

/// Assemble one deliverable from the request shape it names.
pub async fn produce(params: &Value, config: &Config) -> Result<Value, CapabilityError> {
    let map = params
        .as_object()
        .ok_or_else(|| CapabilityError::Invalid("params must be a JSON object".to_string()))?;
    match Mode::detect(map)? {
        Mode::ShotList => produce_shot_list(map, config).await,
        Mode::Animation => produce_animation(map, config).await,
        Mode::TitleCard => produce_title_card(map, config).await,
    }
}

/// Which captured request shape this job carries.
#[derive(Clone, Copy)]
enum Mode {
    ShotList,
    Animation,
    TitleCard,
}

impl Mode {
    /// The three captured models have unambiguous required fields: `shot_list`+`recordings`,
    /// `part_id`, `title`. Mixing two shapes is refused rather than guessed.
    fn detect(map: &Map<String, Value>) -> Result<Self, CapabilityError> {
        let shot_list = map.contains_key("shot_list") || map.contains_key("recordings");
        let animation = map.contains_key("part_id");
        let title_card = map.contains_key("title");
        let shapes = usize::from(shot_list) + usize::from(animation) + usize::from(title_card);
        match (shapes, shot_list, animation) {
            (1, true, _) => Ok(Self::ShotList),
            (1, false, true) => Ok(Self::Animation),
            (1, false, false) => Ok(Self::TitleCard),
            (0, _, _) => Err(CapabilityError::Invalid(
                "one of shot_list+recordings, part_id, or title is required".to_string(),
            )),
            _ => Err(CapabilityError::Invalid(
                "pass exactly one request shape: shot_list+recordings, part_id, or title"
                    .to_string(),
            )),
        }
    }
}

// ── shot list ─────────────────────────────────────────────────────────────────────────────────

/// One parsed `CompileRequest`, reduced to what this port honours.
struct ShotPlan {
    width: u32,
    height: u32,
    fps: u32,
    crf: u32,
    name: String,
    shots: Vec<Shot>,
}

/// One shot, its source and the duration it occupies on the timeline.
struct Shot {
    id: String,
    title: String,
    duration: f64,
    source: ShotSource,
}

enum ShotSource {
    /// A supplied clip read from `in_seconds` for `shot.duration` seconds.
    Video { path: PathBuf, in_seconds: f64 },
    /// A supplied still, held for `shot.duration`.
    Image(PathBuf),
    /// Title-card text painted by this image's Chromium. Boxed: the card carries the theme's
    /// strings, and a shot's source should not make every shot as large as a card.
    Card(Box<Card>),
}

impl ShotPlan {
    fn parse(map: &Map<String, Value>, work_dir: &Path) -> Result<Self, CapabilityError> {
        // The refusal pass runs before anything is resolved, so a request carrying an
        // unsupported stage is named as such even when its files are missing.
        let mut theme = Theme::default();
        for (key, value) in map {
            match key.as_str() {
                "shot_list" | "recordings" | "target_width" | "target_height"
                | "edit_decisions" | "crf" | "name" | "fps" => {}
                "preset" => non_null_refuse(value, code::PRESET, "preset")?,
                "template" => non_null_refuse(value, code::TEMPLATE, "template")?,
                "theme_name" => non_null_refuse(value, code::THEME, "theme_name")?,
                "theme" => {
                    if !value.is_null() {
                        theme = Theme::parse(value)?;
                    }
                }
                "recording_order" => {
                    if !value.is_null() && !is_empty_list(value) {
                        return Err(refuse(
                            code::RECORDING_ORDER,
                            "recording_order is not implemented; the shot list's own order is used",
                        ));
                    }
                }
                "transition_type" | "xfade_transition" => {
                    non_null_refuse(value, code::TRANSITION, key)?
                }
                "crossfade_duration" => {
                    non_null_refuse(value, code::TRANSITION, "crossfade_duration")?
                }
                "color_correction" => non_null_refuse(value, code::EFFECTS, "color_correction")?,
                "subtitles" => non_null_refuse(value, code::EFFECTS, "subtitles")?,
                "watermark" => non_null_refuse(value, code::EFFECTS, "watermark")?,
                "clips_dir" => non_null_refuse(value, code::CLIPS_DIR, "clips_dir")?,
                "narration_audio_dir" => {
                    non_null_refuse(value, code::NARRATION, "narration_audio_dir")?;
                }
                "llm_config" => non_null_refuse(value, code::LLM_EDIT, "llm_config")?,
                "webhook_url" | "webhook_secret" => non_null_refuse(value, code::WEBHOOK, key)?,
                other => {
                    return Err(CapabilityError::Invalid(format!(
                        "unknown parameter {other:?}"
                    )));
                }
            }
        }

        let width = dimension(map, "target_width", DEFAULT_WIDTH)?;
        let height = dimension(map, "target_height", DEFAULT_HEIGHT)?;
        let fps = cadence(map, 120)?;
        let crf = crf(map)?;
        let name = output_name(map)?;

        let shot_list = map
            .get("shot_list")
            .ok_or_else(|| CapabilityError::Invalid("shot_list: Field required".to_string()))?;
        let shot_list = shot_list.as_object().ok_or_else(|| {
            CapabilityError::Invalid("shot_list: Input should be a valid dictionary".to_string())
        })?;
        for (key, value) in shot_list {
            match key.as_str() {
                "parts" => {}
                "recording_order" => {
                    if !value.is_null() && !is_empty_list(value) {
                        return Err(refuse(
                            code::RECORDING_ORDER,
                            "shot_list.recording_order is not implemented; the parts' own order \
                             is used",
                        ));
                    }
                }
                other => {
                    return Err(CapabilityError::Invalid(format!(
                        "shot_list: unknown field {other:?}"
                    )));
                }
            }
        }
        let parts = shot_list
            .get("parts")
            .ok_or_else(|| CapabilityError::Invalid("shot_list.parts: Field required".to_string()))?
            .as_array()
            .ok_or_else(|| {
                CapabilityError::Invalid(
                    "shot_list.parts: Input should be a valid list".to_string(),
                )
            })?;
        if parts.is_empty() {
            return Err(CapabilityError::Invalid(
                "shot_list.parts: List should have at least 1 item".to_string(),
            ));
        }

        let recordings = map
            .get("recordings")
            .ok_or_else(|| CapabilityError::Invalid("recordings: Field required".to_string()))?
            .as_array()
            .ok_or_else(|| {
                CapabilityError::Invalid("recordings: Input should be a valid list".to_string())
            })?;
        let mut clips = parse_recordings(recordings, work_dir)?;
        let mut edits = parse_edit_decisions(map.get("edit_decisions"))?;

        let mut specs = Vec::with_capacity(parts.len());
        for (index, raw) in parts.iter().enumerate() {
            specs.push(PartSpec::parse(index, raw)?);
        }

        let mut shots = Vec::with_capacity(specs.len());
        for spec in specs {
            let duration = spec.duration;
            let source = match spec.kind {
                PartKind::Video => {
                    let path = clips.remove(&spec.id).ok_or_else(|| {
                        CapabilityError::Invalid(format!(
                            "shot_list.parts[{}]: no recordings entry for part {:?}",
                            spec.index, spec.id
                        ))
                    })?;
                    let in_seconds = edits.remove(&spec.id).unwrap_or(0.0);
                    ShotSource::Video { path, in_seconds }
                }
                PartKind::Image => {
                    if edits.remove(&spec.id).is_some() {
                        return Err(CapabilityError::Invalid(format!(
                            "shot_list.parts[{}]: an in-point is only read for a supplied video",
                            spec.index
                        )));
                    }
                    ShotSource::Image(process::resolve_input(
                        work_dir,
                        spec.image.as_deref().unwrap_or_default(),
                    )?)
                }
                PartKind::Card => {
                    if edits.remove(&spec.id).is_some() {
                        return Err(CapabilityError::Invalid(format!(
                            "shot_list.parts[{}]: an in-point is only read for a supplied video",
                            spec.index
                        )));
                    }
                    ShotSource::Card(Box::new(Card {
                        title: spec.title.clone(),
                        subtitle: spec.subtitle,
                        bullets: spec.bullets,
                        theme: theme.clone(),
                    }))
                }
            };
            shots.push(Shot {
                id: spec.id,
                title: spec.title,
                duration,
                source,
            });
        }
        if let Some(unused) = clips.keys().next() {
            return Err(CapabilityError::Invalid(format!(
                "recordings: part_id {unused:?} names no shot list part"
            )));
        }
        if let Some(unused) = edits.keys().next() {
            return Err(CapabilityError::Invalid(format!(
                "edit_decisions: part_id {unused:?} names no shot list part"
            )));
        }

        Ok(Self {
            width,
            height,
            fps,
            crf,
            name,
            shots,
        })
    }
}

/// One shot-list part after its refusal/schema pass, before its source is resolved.
struct PartSpec {
    index: usize,
    id: String,
    title: String,
    subtitle: String,
    bullets: Vec<String>,
    kind: PartKind,
    duration: f64,
    /// For an image part: the file name one of `image_url` / `slide_image_url` names.
    image: Option<String>,
}

#[derive(Clone, Copy)]
enum PartKind {
    Video,
    Image,
    Card,
}

impl PartSpec {
    fn parse(index: usize, raw: &Value) -> Result<Self, CapabilityError> {
        let map = raw.as_object().ok_or_else(|| {
            CapabilityError::Invalid(format!(
                "shot_list.parts[{index}]: Input should be a valid dictionary"
            ))
        })?;

        for (key, value) in map {
            match key.as_str() {
                "id"
                | "title"
                | "subtitle"
                | "part_type"
                | "estimated_duration_seconds"
                | "image_url"
                | "slide_image_url"
                | "bullet_points" => {}
                "strip_ai_audio" => match value.as_bool() {
                    None if value.is_null() => {}
                    Some(true) => {}
                    Some(false) => {
                        return Err(refuse(
                            code::AI_AUDIO,
                            format!(
                                "shot_list.parts[{index}].strip_ai_audio=false is not implemented"
                            ),
                        ));
                    }
                    None => {
                        return Err(CapabilityError::Invalid(format!(
                            "shot_list.parts[{index}].strip_ai_audio: must be a boolean"
                        )));
                    }
                },
                "ai_prompt" | "ai_provider" => non_null_refuse(
                    value,
                    code::AI_GENERATION,
                    &format!("shot_list.parts[{index}].{key}"),
                )?,
                "animation" => non_null_refuse(
                    value,
                    code::ANIMATION,
                    &format!("shot_list.parts[{index}].animation"),
                )?,
                "approval_status" => non_null_refuse(
                    value,
                    code::APPROVAL_STATUS,
                    &format!("shot_list.parts[{index}].approval_status"),
                )?,
                "asset_id" | "asset_kind" | "asset_source" => non_null_refuse(
                    value,
                    code::ASSET_REFERENCE,
                    &format!("shot_list.parts[{index}].{key}"),
                )?,
                "background_image" => non_null_refuse(
                    value,
                    code::BACKGROUND_IMAGE,
                    &format!("shot_list.parts[{index}].background_image"),
                )?,
                "chapter" => non_null_refuse(
                    value,
                    code::CHAPTER,
                    &format!("shot_list.parts[{index}].chapter"),
                )?,
                "clip_type" => non_null_refuse(
                    value,
                    code::CLIP_TYPE,
                    &format!("shot_list.parts[{index}].clip_type"),
                )?,
                "logo_path" | "logo_position" | "logo_scale" => non_null_refuse(
                    value,
                    code::LOGO,
                    &format!("shot_list.parts[{index}].{key}"),
                )?,
                "motion_preset" => non_null_refuse(
                    value,
                    code::MOTION_PRESET,
                    &format!("shot_list.parts[{index}].motion_preset"),
                )?,
                "provenance" => non_null_refuse(
                    value,
                    code::PROVENANCE,
                    &format!("shot_list.parts[{index}].provenance"),
                )?,
                "slide_role" => non_null_refuse(
                    value,
                    code::SLIDE_ROLE,
                    &format!("shot_list.parts[{index}].slide_role"),
                )?,
                other => {
                    return Err(CapabilityError::Invalid(format!(
                        "shot_list.parts[{index}]: unknown field {other:?}"
                    )));
                }
            }
        }

        let id = map
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .ok_or_else(|| {
                CapabilityError::Invalid(format!("shot_list.parts[{index}].id: Field required"))
            })?
            .to_string();

        let part_type = match map.get("part_type").filter(|value| !value.is_null()) {
            None => "browser_recording",
            Some(value) => value.as_str().ok_or_else(|| {
                CapabilityError::Invalid(format!(
                    "shot_list.parts[{index}].part_type: must be a string"
                ))
            })?,
        };
        let kind = match part_type {
            "browser_recording" => PartKind::Video,
            "title_card" => PartKind::Card,
            "slide_preview" | "generated_image" | "comparison_frame" => PartKind::Image,
            other => {
                return Err(CapabilityError::Invalid(format!(
                    "shot_list.parts[{index}].part_type: {other:?} is not one of \
                     browser_recording, title_card, slide_preview, generated_image, comparison_frame"
                )));
            }
        };

        let title = string_field(map, "title", index)?.unwrap_or_default();
        let subtitle = string_field(map, "subtitle", index)?.unwrap_or_default();
        let bullets = match map.get("bullet_points").filter(|value| !value.is_null()) {
            None => Vec::new(),
            Some(value) => value
                .as_array()
                .ok_or_else(|| {
                    CapabilityError::Invalid(format!(
                        "shot_list.parts[{index}].bullet_points: must be a list"
                    ))
                })?
                .iter()
                .map(|item| {
                    item.as_str().map(str::to_string).ok_or_else(|| {
                        CapabilityError::Invalid(format!(
                            "shot_list.parts[{index}].bullet_points: items must be strings"
                        ))
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
        };

        let duration = match map
            .get("estimated_duration_seconds")
            .filter(|value| !value.is_null())
        {
            None => 10.0,
            Some(value) => value
                .as_f64()
                .filter(|seconds| *seconds > 0.0 && *seconds <= MAX_SECONDS)
                .ok_or_else(|| {
                    CapabilityError::Invalid(format!(
                        "shot_list.parts[{index}].estimated_duration_seconds: must be a number \
                         in (0, {MAX_SECONDS}]"
                    ))
                })?,
        };

        let image_url = string_field(map, "image_url", index)?;
        let slide_image_url = string_field(map, "slide_image_url", index)?;
        let image = match kind {
            PartKind::Image => match (image_url, slide_image_url) {
                (Some(one), None) => {
                    refuse_remote(&one, &format!("shot_list.parts[{index}].image_url"))?;
                    Some(one)
                }
                (None, Some(other)) => {
                    refuse_remote(&other, &format!("shot_list.parts[{index}].slide_image_url"))?;
                    Some(other)
                }
                (Some(_), Some(_)) => {
                    return Err(CapabilityError::Invalid(format!(
                        "shot_list.parts[{index}]: pass at most one of image_url or slide_image_url"
                    )));
                }
                (None, None) => {
                    return Err(CapabilityError::Invalid(format!(
                        "shot_list.parts[{index}]: an image part needs image_url or slide_image_url"
                    )));
                }
            },
            PartKind::Video | PartKind::Card => {
                if image_url.is_some() || slide_image_url.is_some() {
                    return Err(CapabilityError::Invalid(format!(
                        "shot_list.parts[{index}]: image_url is only read for an image part"
                    )));
                }
                None
            }
        };
        if !matches!(kind, PartKind::Card) && !bullets.is_empty() {
            return Err(CapabilityError::Invalid(format!(
                "shot_list.parts[{index}].bullet_points is only painted by a title_card part"
            )));
        }

        Ok(Self {
            index,
            id,
            title,
            subtitle,
            bullets,
            kind,
            duration,
            image,
        })
    }
}

/// Resolve every `recordings` entry to a file in the job's work directory.
fn parse_recordings(
    recordings: &[Value],
    work_dir: &Path,
) -> Result<HashMap<String, PathBuf>, CapabilityError> {
    let mut clips = HashMap::with_capacity(recordings.len());
    for (index, raw) in recordings.iter().enumerate() {
        let map = raw.as_object().ok_or_else(|| {
            CapabilityError::Invalid(format!(
                "recordings[{index}]: Input should be a valid dictionary"
            ))
        })?;
        for (key, value) in map {
            match key.as_str() {
                // Recording metadata is machine-produced and re-measured from the file; only
                // `success` changes whether the entry is usable at all.
                "part_id" | "video_path" | "duration_seconds" | "file_size_bytes" | "width"
                | "height" | "has_audio" => {}
                "success" => match value.as_bool() {
                    None if value.is_null() => {}
                    Some(false) => {
                        return Err(CapabilityError::Invalid(format!(
                            "recordings[{index}].success: a failed recording cannot be assembled"
                        )));
                    }
                    Some(true) => {}
                    None => {
                        return Err(CapabilityError::Invalid(format!(
                            "recordings[{index}].success: must be a boolean"
                        )));
                    }
                },
                other => {
                    return Err(CapabilityError::Invalid(format!(
                        "recordings[{index}]: unknown field {other:?}"
                    )));
                }
            }
        }
        let part_id = map
            .get("part_id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .ok_or_else(|| {
                CapabilityError::Invalid(format!("recordings[{index}].part_id: Field required"))
            })?;
        let video_path = map
            .get("video_path")
            .and_then(Value::as_str)
            .filter(|path| !path.is_empty())
            .ok_or_else(|| {
                CapabilityError::Invalid(format!("recordings[{index}].video_path: Field required"))
            })?;
        refuse_remote(video_path, &format!("recordings[{index}].video_path"))?;
        let path = process::resolve_input(work_dir, video_path)?;
        if clips.insert(part_id.to_string(), path).is_some() {
            return Err(CapabilityError::Invalid(format!(
                "recordings[{index}].part_id: {part_id:?} is recorded twice"
            )));
        }
    }
    Ok(clips)
}

/// The only edit decision this port honours: an in-point (`trim_start_s`) per supplied video.
fn parse_edit_decisions(value: Option<&Value>) -> Result<HashMap<String, f64>, CapabilityError> {
    let mut edits = HashMap::new();
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return Ok(edits);
    };
    let list = value.as_array().ok_or_else(|| {
        CapabilityError::Invalid("edit_decisions: Input should be a valid list".to_string())
    })?;
    for (index, raw) in list.iter().enumerate() {
        let map = raw.as_object().ok_or_else(|| {
            CapabilityError::Invalid(format!(
                "edit_decisions[{index}]: Input should be a valid dictionary"
            ))
        })?;
        let mut trim_start = 0.0;
        for (key, value) in map {
            match key.as_str() {
                "part_id" => {}
                "trim_start_s" => {
                    trim_start = number_or_default(
                        value,
                        0.0,
                        &format!("edit_decisions[{index}].trim_start_s"),
                    )?;
                    if trim_start < 0.0 {
                        return Err(CapabilityError::Invalid(format!(
                            "edit_decisions[{index}].trim_start_s: must be non-negative"
                        )));
                    }
                }
                "trim_end_s" => {
                    let trim_end = number_or_default(
                        value,
                        0.0,
                        &format!("edit_decisions[{index}].trim_end_s"),
                    )?;
                    if trim_end.abs() > f64::EPSILON {
                        return Err(refuse(
                            code::TRIM_END,
                            format!("edit_decisions[{index}].trim_end_s is not implemented"),
                        ));
                    }
                }
                "speed_factor" => {
                    let speed = number_or_default(
                        value,
                        1.0,
                        &format!("edit_decisions[{index}].speed_factor"),
                    )?;
                    if (speed - 1.0).abs() > f64::EPSILON {
                        return Err(refuse(
                            code::SPEED_CHANGE,
                            format!("edit_decisions[{index}].speed_factor is not implemented"),
                        ));
                    }
                }
                "remove_dead_time" => match value.as_bool() {
                    None if value.is_null() => {}
                    Some(true) => {
                        return Err(refuse(
                            code::DEAD_TIME,
                            format!("edit_decisions[{index}].remove_dead_time is not implemented"),
                        ));
                    }
                    Some(false) => {}
                    None => {
                        return Err(CapabilityError::Invalid(format!(
                            "edit_decisions[{index}].remove_dead_time: must be a boolean"
                        )));
                    }
                },
                other => {
                    return Err(CapabilityError::Invalid(format!(
                        "edit_decisions[{index}]: unknown field {other:?}"
                    )));
                }
            }
        }
        let part_id = map
            .get("part_id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .ok_or_else(|| {
                CapabilityError::Invalid(format!("edit_decisions[{index}].part_id: Field required"))
            })?
            .to_string();
        if edits.insert(part_id.clone(), trim_start).is_some() {
            return Err(CapabilityError::Invalid(format!(
                "edit_decisions[{index}].part_id: {part_id:?} has two decisions"
            )));
        }
    }
    Ok(edits)
}

/// Assemble a shot list into one encoded video.
async fn produce_shot_list(
    map: &Map<String, Value>,
    config: &Config,
) -> Result<Value, CapabilityError> {
    let plan = ShotPlan::parse(map, &config.work_dir)?;
    make_directories(config)?;

    // One intermediate per shot, all encoded with identical parameters, so the join is the
    // reference's concat-demuxer stream copy rather than a second re-encode.
    let mut parts = Vec::with_capacity(plan.shots.len());
    for (index, shot) in plan.shots.iter().enumerate() {
        let part = config.work_dir.join(format!("produce-part-{index}.mp4"));
        encode_shot(shot, &plan, &part, config).await?;
        parts.push(part);
    }
    let list = config.work_dir.join("produce-concat.txt");
    let mut listing = String::new();
    for part in &parts {
        listing.push_str("file '");
        listing.push_str(&part.to_string_lossy().replace('\'', "'\\''"));
        listing.push_str("'\n");
    }
    std::fs::write(&list, listing).map_err(|error| {
        CapabilityError::Failed(format!("cannot write the concat list {list:?}: {error}"))
    })?;

    let output = config.out_dir.join(&plan.name);
    let args = strings(&[
        "-y",
        "-hide_banner",
        "-loglevel",
        "error",
        "-f",
        "concat",
        "-safe",
        "0",
        "-i",
        &list.display().to_string(),
        "-c",
        "copy",
        "-movflags",
        "+faststart",
        "-f",
        "mp4",
        &output.display().to_string(),
    ]);
    let finished = process::run("ffmpeg", &args).await?;
    if !finished.ok() {
        return Err(CapabilityError::Failed(format!(
            "ffmpeg could not join the shots: {}",
            finished.tail()
        )));
    }
    let bytes = read_output(&output)?;

    let mut elapsed = 0.0_f64;
    let mut timeline = Vec::with_capacity(plan.shots.len());
    for shot in &plan.shots {
        let start = elapsed;
        elapsed += shot.duration;
        timeline.push(json!({
            "id": shot.id,
            "title": shot.title,
            "kind": shot_kind(&shot.source),
            "start": round(start),
            "end": round(elapsed),
        }));
    }
    Ok(json!({
        "status": "completed",
        "mode": "shot_list",
        "output": plan.name,
        "content_type": "video/mp4",
        "bytes": bytes.len(),
        "sha256": hex(&Sha256::digest(&bytes)),
        "width": plan.width,
        "height": plan.height,
        "fps": plan.fps,
        "duration_seconds": round(elapsed),
        "shots": timeline,
    }))
}

fn shot_kind(source: &ShotSource) -> &'static str {
    match source {
        ShotSource::Video { .. } => "video",
        ShotSource::Image(_) => "image",
        ShotSource::Card(_) => "title_card",
    }
}

/// Encode one shot into an intermediate whose codec parameters are identical across shots.
async fn encode_shot(
    shot: &Shot,
    plan: &ShotPlan,
    output: &Path,
    config: &Config,
) -> Result<(), CapabilityError> {
    let mut args = strings(&["-y", "-hide_banner", "-loglevel", "error"]);
    let mut filters: Vec<String> = Vec::new();
    let mut maps: Vec<String> = Vec::new();
    let chain = video_chain(plan, shot.duration);

    match &shot.source {
        ShotSource::Video { path, in_seconds } => {
            args.push("-ss".to_string());
            args.push(seconds_text(*in_seconds));
            args.push("-t".to_string());
            args.push(seconds_text(shot.duration));
            args.push("-i".to_string());
            args.push(path.display().to_string());
            filters.push(format!("[0:v]{chain}[v]"));
            maps.push("[v]".to_string());
            if has_audio(path).await? {
                filters.push("[0:a]aformat=sample_fmts=fltp:channel_layouts=stereo[a]".to_string());
                maps.push("[a]".to_string());
            } else {
                push_silence(&mut args, shot.duration);
                maps.push("1:a".to_string());
            }
        }
        ShotSource::Image(path) => {
            args.extend(strings(&["-loop", "1", "-i", &path.display().to_string()]));
            filters.push(format!("[0:v]{chain}[v]"));
            maps.push("[v]".to_string());
            push_silence(&mut args, shot.duration);
            maps.push("1:a".to_string());
        }
        ShotSource::Card(card) => {
            let png = render_card(card, plan.width, plan.height, config).await?;
            args.push("-loop".to_string());
            args.push("1".to_string());
            args.push("-i".to_string());
            args.push(png.display().to_string());
            filters.push(format!("[0:v]{chain}[v]"));
            maps.push("[v]".to_string());
            push_silence(&mut args, shot.duration);
            maps.push("1:a".to_string());
        }
    }

    args.push("-filter_complex".to_string());
    args.push(filters.join(";"));
    for map in &maps {
        args.push("-map".to_string());
        args.push(map.clone());
    }
    args.push("-t".to_string());
    args.push(seconds_text(shot.duration));
    args.push("-r".to_string());
    args.push(plan.fps.to_string());
    args.extend(strings(&[
        "-c:v",
        "libx264",
        "-preset",
        "veryfast",
        "-crf",
        &plan.crf.to_string(),
        "-pix_fmt",
        "yuv420p",
        "-c:a",
        "aac",
        "-b:a",
        "128k",
        "-ar",
        "48000",
        "-ac",
        "2",
    ]));
    args.push(output.display().to_string());

    let finished = process::run("ffmpeg", &args).await?;
    if !finished.ok() {
        return Err(CapabilityError::Failed(format!(
            "ffmpeg failed on shot {:?}: {}",
            shot.id,
            finished.tail()
        )));
    }
    Ok(())
}

/// Fit/scale/pad a frame onto the canvas, then hold it long enough that the intermediate is
/// exactly `duration` seconds long.
fn video_chain(plan: &ShotPlan, duration: f64) -> String {
    let width = plan.width;
    let height = plan.height;
    format!(
        "scale={width}:{height}:force_original_aspect_ratio=decrease,\
         pad={width}:{height}:(ow-iw)/2:(oh-ih)/2:color={PAD_COLOUR},\
         setsar=1,fps={},tpad=stop_mode=clone:stop_duration={}",
        plan.fps,
        seconds_text(duration)
    )
}

/// Add a bounded silent stereo track so every intermediate carries the same stream layout; the
/// concat demuxer's stream-copy path requires it.
fn push_silence(args: &mut Vec<String>, duration: f64) {
    let seconds = seconds_text(duration);
    args.extend(strings(&[
        "-f",
        "lavfi",
        "-t",
        &seconds,
        "-i",
        "anullsrc=r=48000:cl=stereo",
    ]));
}

/// Whether a file carries an audio stream. An absent track is silence in the reference's
/// behaviour, so the caller substitutes one rather than dropping the stream.
async fn has_audio(path: &Path) -> Result<bool, CapabilityError> {
    let args = strings(&[
        "-v",
        "error",
        "-select_streams",
        "a",
        "-show_entries",
        "stream=index",
        "-of",
        "csv=p=0",
        &path.display().to_string(),
    ]);
    let finished = process::run("ffprobe", &args).await?;
    if !finished.ok() {
        return Err(CapabilityError::Failed(format!(
            "ffprobe failed on {:?}: {}",
            path,
            finished.tail()
        )));
    }
    Ok(!finished.stdout.iter().all(u8::is_ascii_whitespace))
}

// ── title card ────────────────────────────────────────────────────────────────────────────────

/// The card's text and the explicit style fields this port paints.
#[derive(Clone)]
struct Card {
    title: String,
    subtitle: String,
    bullets: Vec<String>,
    theme: Theme,
}

/// The captured theme field set, resolved to concrete colours. The catalogue's default values are
/// the base; an explicit `theme` object overrides individual fields.
#[derive(Clone)]
struct Theme {
    /// A solid background; `None` means the gradient below is painted.
    background: Option<String>,
    gradient_start: Option<String>,
    gradient_end: Option<String>,
    text: String,
    subtitle: String,
    primary: String,
    accent: String,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            background: None,
            gradient_start: Some(THEME_BACKGROUND.to_string()),
            gradient_end: Some(THEME_GRADIENT_END.to_string()),
            text: THEME_TEXT.to_string(),
            subtitle: THEME_SUBTITLE.to_string(),
            primary: THEME_PRIMARY.to_string(),
            accent: THEME_ACCENT.to_string(),
        }
    }
}

impl Theme {
    /// Parse an explicit `ThemeInput`. A catalogue name, a font file, a logo or a background image
    /// is refused by name; explicit colours are taken as given.
    fn parse(value: &Value) -> Result<Self, CapabilityError> {
        let map = value
            .as_object()
            .ok_or_else(|| CapabilityError::Invalid("theme must be an object".to_string()))?;
        let mut theme = Self::default();
        let mut solid = false;
        let mut gradient = false;
        for (key, value) in map {
            match key.as_str() {
                "theme_name" => non_null_refuse(value, code::THEME, "theme.theme_name")?,
                "font_path" | "font_bold_path" => {
                    non_null_refuse(value, code::FONT, &format!("theme.{key}"))?;
                }
                "logo_path" | "logo_position" | "logo_scale" => {
                    non_null_refuse(value, code::LOGO, &format!("theme.{key}"))?;
                }
                "background_image" => {
                    non_null_refuse(value, code::BACKGROUND_IMAGE, "theme.background_image")?;
                }
                "animation" => non_null_refuse(value, code::ANIMATION, "theme.animation")?,
                "background_color" => {
                    if !value.is_null() {
                        theme.background = Some(colour(value, "theme.background_color")?);
                        theme.gradient_start = None;
                        theme.gradient_end = None;
                        solid = true;
                    }
                }
                "gradient_start" => {
                    if !value.is_null() {
                        theme.gradient_start = Some(colour(value, "theme.gradient_start")?);
                        theme.background = None;
                        gradient = true;
                    }
                }
                "gradient_end" => {
                    if !value.is_null() {
                        theme.gradient_end = Some(colour(value, "theme.gradient_end")?);
                        theme.background = None;
                        gradient = true;
                    }
                }
                "text_color" => {
                    if !value.is_null() {
                        theme.text = colour(value, "theme.text_color")?;
                    }
                }
                "subtitle_color" => {
                    if !value.is_null() {
                        theme.subtitle = colour(value, "theme.subtitle_color")?;
                    }
                }
                "primary_color" => {
                    if !value.is_null() {
                        theme.primary = colour(value, "theme.primary_color")?;
                    }
                }
                "accent_color" => {
                    if !value.is_null() {
                        theme.accent = colour(value, "theme.accent_color")?;
                    }
                }
                other => {
                    return Err(CapabilityError::Invalid(format!(
                        "theme: unknown field {other:?}"
                    )));
                }
            }
        }
        if solid && gradient {
            return Err(CapabilityError::Invalid(
                "theme: supply background_color or gradient_start/gradient_end, not both"
                    .to_string(),
            ));
        }
        if theme.background.is_none()
            && (theme.gradient_start.is_none() || theme.gradient_end.is_none())
        {
            return Err(CapabilityError::Invalid(
                "theme: supply gradient_start and gradient_end together, or background_color"
                    .to_string(),
            ));
        }
        Ok(theme)
    }

    /// The CSS `background` declaration.
    fn background(&self) -> String {
        match (&self.gradient_start, &self.gradient_end) {
            (Some(start), Some(end)) => format!("linear-gradient(135deg, {start}, {end})"),
            _ => self
                .background
                .clone()
                .unwrap_or_else(|| THEME_BACKGROUND.to_string()),
        }
    }
}

/// Render a title card into `params`' own shape.
async fn produce_title_card(
    map: &Map<String, Value>,
    config: &Config,
) -> Result<Value, CapabilityError> {
    let mut theme = Theme::default();
    for (key, value) in map {
        match key.as_str() {
            "title" | "subtitle" | "width" | "height" | "duration" | "bullet_points" | "name"
            | "fps" => {}
            "theme" => {
                if !value.is_null() {
                    theme = Theme::parse(value)?;
                }
            }
            // The captured schema says these two are inert on this route; the animation name is a
            // catalogue reference, and `chapter` has no captured rendering rule.
            "background_image" => {
                non_null_refuse(value, code::BACKGROUND_IMAGE, "background_image")?;
            }
            "logo_path" | "logo_position" | "logo_scale" => {
                non_null_refuse(value, code::LOGO, key)?;
            }
            "animation" => non_null_refuse(value, code::ANIMATION, "animation")?,
            "chapter" => non_null_refuse(value, code::CHAPTER, "chapter")?,
            other => {
                return Err(CapabilityError::Invalid(format!(
                    "unknown parameter {other:?}"
                )));
            }
        }
    }

    let title = map
        .get("title")
        .and_then(Value::as_str)
        .ok_or_else(|| CapabilityError::Invalid("title: Field required".to_string()))?
        .to_string();
    let subtitle = match map.get("subtitle").filter(|value| !value.is_null()) {
        None => String::new(),
        Some(value) => value
            .as_str()
            .ok_or_else(|| CapabilityError::Invalid("subtitle must be a string".to_string()))?
            .to_string(),
    };
    let bullets = match map.get("bullet_points").filter(|value| !value.is_null()) {
        None => Vec::new(),
        Some(value) => value
            .as_array()
            .ok_or_else(|| CapabilityError::Invalid("bullet_points must be a list".to_string()))?
            .iter()
            .map(|item| {
                item.as_str().map(str::to_string).ok_or_else(|| {
                    CapabilityError::Invalid("bullet_points items must be strings".to_string())
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
    };
    let width = dimension(map, "width", DEFAULT_WIDTH)?;
    let height = dimension(map, "height", DEFAULT_HEIGHT)?;
    let fps = cadence(map, 120)?;
    let duration = duration_field(map, DEFAULT_TITLE_DURATION)?;
    let name = output_name(map)?;

    make_directories(config)?;
    let card = Card {
        title,
        subtitle,
        bullets,
        theme,
    };
    let png = render_card(&card, width, height, config).await?;
    let output = config.out_dir.join(&name);
    encode_still(&png, duration, fps, DEFAULT_CRF, &output).await?;
    let bytes = read_output(&output)?;

    Ok(json!({
        "status": "completed",
        "mode": "title_card",
        "output": name,
        "content_type": "video/mp4",
        "bytes": bytes.len(),
        "sha256": hex(&Sha256::digest(&bytes)),
        "width": width,
        "height": height,
        "fps": fps,
        "duration_seconds": round(duration),
    }))
}

/// The card page. The capture pins the theme field set and the default theme's values, not the
/// card's layout; the mapping declared in the module docs is applied here.
fn card_html(card: &Card, width: u32, height: u32) -> String {
    let title_px = (height / 8).max(16);
    let subtitle_px = (height / 20).max(10);
    let bullet_px = (height / 26).max(9);
    let rule_height = (height / 120).max(1);
    let gap = (height / 40).max(4);
    let rule_width = (width / 5).max(8);

    let subtitle = if card.subtitle.is_empty() {
        String::new()
    } else {
        format!("<p class=\"subtitle\">{}</p>", escape_html(&card.subtitle))
    };
    let bullets = if card.bullets.is_empty() {
        String::new()
    } else {
        let items: String = card
            .bullets
            .iter()
            .map(|item| format!("<li>{}</li>", escape_html(item)))
            .collect();
        format!("<ul class=\"bullets\">{items}</ul>")
    };

    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <style>\n\
         html, body {{ margin: 0; width: 100%; height: 100%; }}\n\
         body {{ background: {}; display: flex; align-items: center; justify-content: center; \
         font-family: system-ui, -apple-system, 'Segoe UI', Helvetica, Arial, sans-serif; }}\n\
         .card {{ width: 80%; text-align: center; }}\n\
         h1 {{ margin: 0; font-size: {title_px}px; line-height: 1.15; color: {}; }}\n\
         .rule {{ width: {rule_width}px; height: {rule_height}px; margin: {gap}px auto; \
         background: {}; }}\n\
         .subtitle {{ margin: 0; font-size: {subtitle_px}px; color: {}; }}\n\
         .bullets {{ list-style: disc inside; margin: {gap}px 0 0; padding: 0; \
         font-size: {bullet_px}px; color: {}; }}\n\
         .bullets li::marker {{ color: {}; }}\n\
         </style>\n</head>\n<body>\n<div class=\"card\">\n\
         <h1>{}</h1>\n<div class=\"rule\"></div>\n{subtitle}\n{bullets}\n\
         </div>\n</body>\n</html>\n",
        card.theme.background(),
        card.theme.text,
        card.theme.primary,
        card.theme.subtitle,
        card.theme.subtitle,
        card.theme.accent,
        escape_html(&card.title),
    )
}

/// Paint the card page and screenshot it at the requested size.
async fn render_card(
    card: &Card,
    width: u32,
    height: u32,
    config: &Config,
) -> Result<PathBuf, CapabilityError> {
    let page = config.work_dir.join("produce-card.html");
    std::fs::write(&page, card_html(card, width, height)).map_err(|error| {
        CapabilityError::Failed(format!("cannot write the card page {page:?}: {error}"))
    })?;
    let png = config.work_dir.join("produce-card.png");
    Chrome::new(config)
        .screenshot(&page, &png, width, height)
        .await?;
    Ok(png)
}

// ── animation ─────────────────────────────────────────────────────────────────────────────────

/// Render a self-contained motion page frame by frame and encode the PNG sequence.
async fn produce_animation(
    map: &Map<String, Value>,
    config: &Config,
) -> Result<Value, CapabilityError> {
    for (key, value) in map {
        match key.as_str() {
            "part_id" | "html" | "duration_ms" | "fps" | "width" | "height" | "params" | "name" => {
            }
            "template_id" => non_null_refuse(value, code::TEMPLATE, "template_id")?,
            "quality_preset" => non_null_refuse(value, code::PRESET, "quality_preset")?,
            "webhook_url" => non_null_refuse(value, code::WEBHOOK, "webhook_url")?,
            "transparent" => match value.as_bool() {
                None if value.is_null() => {}
                Some(true) => {
                    return Err(refuse(
                        code::TRANSPARENT,
                        "transparent=true needs a VP9 alpha encode, which is not implemented",
                    ));
                }
                Some(false) => {}
                None => {
                    return Err(CapabilityError::Invalid(
                        "transparent must be a boolean".to_string(),
                    ));
                }
            },
            "output_format" => match value.as_str() {
                Some("mp4") => {}
                Some("webm" | "gif") => {
                    return Err(refuse(
                        code::OUTPUT_FORMAT,
                        format!(
                            "output_format={:?} is not implemented; only mp4 is produced",
                            value.as_str().unwrap_or_default()
                        ),
                    ));
                }
                Some(other) => {
                    return Err(CapabilityError::Invalid(format!(
                        "output_format: {other:?} is not one of mp4, webm, gif"
                    )));
                }
                None if value.is_null() => {}
                None => {
                    return Err(CapabilityError::Invalid(
                        "output_format must be a string".to_string(),
                    ));
                }
            },
            other => {
                return Err(CapabilityError::Invalid(format!(
                    "unknown parameter {other:?}"
                )));
            }
        }
    }

    let part_id = map
        .get("part_id")
        .and_then(Value::as_str)
        .ok_or_else(|| CapabilityError::Invalid("part_id: Field required".to_string()))?;
    if part_id.is_empty()
        || part_id.len() > 128
        || !part_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
    {
        return Err(CapabilityError::Invalid(
            "part_id: must match ^[a-zA-Z0-9_.-]+$ and be at most 128 characters".to_string(),
        ));
    }
    let html = match map.get("html").filter(|value| !value.is_null()) {
        Some(value) => {
            let text = value.as_str().ok_or_else(|| {
                CapabilityError::Invalid("html: Input should be a valid string".to_string())
            })?;
            if text.is_empty() {
                return Err(CapabilityError::Invalid(
                    "html: String should have at least 1 character".to_string(),
                ));
            }
            text
        }
        None => {
            return Err(CapabilityError::Invalid(
                "html: one of html or template_id is required (and template_id needs a catalogue \
                 this executor does not carry)"
                    .to_string(),
            ));
        }
    };
    if html.chars().count() > MAX_HTML_CHARS {
        return Err(CapabilityError::Invalid(format!(
            "html: String should have at most {MAX_HTML_CHARS} characters"
        )));
    }

    let fps = cadence(map, 60)?;
    let width = bounded_dimension(map, "width", DEFAULT_HEIGHT, 64, 7680)?;
    let height = bounded_dimension(map, "height", DEFAULT_HEIGHT, 64, 4320)?;
    let name = output_name(map)?;
    let injected_params = match map.get("params").filter(|value| !value.is_null()) {
        None => None,
        Some(value) => {
            if !value.is_object() {
                return Err(CapabilityError::Invalid(
                    "params must be a JSON object".to_string(),
                ));
            }
            Some(script_safe(value))
        }
    };

    let duration_ms = match map.get("duration_ms").filter(|value| !value.is_null()) {
        Some(value) => value
            .as_u64()
            .filter(|ms| (100..=120_000).contains(ms))
            .ok_or_else(|| {
                CapabilityError::Invalid(
                    "duration_ms: must be an integer in [100, 120000]".to_string(),
                )
            })?,
        None => page_duration_ms(html, config).await?,
    };

    make_directories(config)?;
    let frames_dir = config.work_dir.join("produce-frames");
    // A previous run in the same work directory must not leak frames into this one: ffmpeg's
    // `%04d` reader stops at the first missing index, so a stale gap would silently truncate.
    match std::fs::remove_dir_all(&frames_dir) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(CapabilityError::Failed(format!(
                "cannot clear {frames_dir:?}: {error}"
            )));
        }
    }
    std::fs::create_dir_all(&frames_dir).map_err(|error| {
        CapabilityError::Failed(format!("cannot create {frames_dir:?}: {error}"))
    })?;

    let chrome = Chrome::new(config);
    let page = config.work_dir.join("produce-animation.html");
    let frame_count = (duration_ms as f64 * f64::from(fps) / 1000.0)
        .round()
        .max(1.0) as u64;
    for frame in 0..frame_count {
        let time_ms = frame as f64 * 1000.0 / f64::from(fps);
        let mut document = html.to_string();
        if let Some(params) = &injected_params {
            document = inject_head(
                &document,
                &format!("<script>window.PARAMS = {params};</script>"),
            );
        }
        let call = format!(
            "<script>(function(){{function go(){{if(typeof window.render === 'function'){{\
             window.render({time_ms:.3});}}}}if(document.readyState === 'loading'){{\
             document.addEventListener('DOMContentLoaded', go);}}else{{go();}}}})();</script>"
        );
        document = inject_tail(&document, &call);
        std::fs::write(&page, document).map_err(|error| {
            CapabilityError::Failed(format!("cannot write the frame page {page:?}: {error}"))
        })?;
        let png = frames_dir.join(format!("frame-{:04}.png", frame + 1));
        chrome.screenshot(&page, &png, width, height).await?;
    }

    let output = config.out_dir.join(&name);
    let args = strings(&[
        "-y",
        "-hide_banner",
        "-loglevel",
        "error",
        "-framerate",
        &fps.to_string(),
        "-i",
        &frames_dir.join("frame-%04d.png").display().to_string(),
        "-f",
        "lavfi",
        "-i",
        "anullsrc=r=48000:cl=stereo",
        "-c:v",
        "libx264",
        "-preset",
        "veryfast",
        "-crf",
        &DEFAULT_CRF.to_string(),
        "-pix_fmt",
        "yuv420p",
        "-c:a",
        "aac",
        "-b:a",
        "128k",
        "-ar",
        "48000",
        "-ac",
        "2",
        "-shortest",
        "-movflags",
        "+faststart",
        &output.display().to_string(),
    ]);
    let finished = process::run("ffmpeg", &args).await?;
    if !finished.ok() {
        return Err(CapabilityError::Failed(format!(
            "ffmpeg could not encode the animation: {}",
            finished.tail()
        )));
    }
    let bytes = read_output(&output)?;

    Ok(json!({
        "status": "completed",
        "mode": "animation",
        "part_id": part_id,
        "output": name,
        "content_type": "video/mp4",
        "bytes": bytes.len(),
        "sha256": hex(&Sha256::digest(&bytes)),
        "width": width,
        "height": height,
        "fps": fps,
        "frames": frame_count,
        "duration_seconds": round(frame_count as f64 / f64::from(fps)),
    }))
}

/// Read the page's own `window.RENDER_DURATION_MS`: the captured schema's documented fallback
/// when the request omits `duration_ms`. The value is written into the DOM by an injected script
/// and read back with `--dump-dom`, so no second browser protocol is needed.
async fn page_duration_ms(html: &str, config: &Config) -> Result<u64, CapabilityError> {
    let marker = "conveyor-render-duration-ms=";
    let probe = inject_tail(
        html,
        &format!(
            "<script>document.title = '{marker}' + String(window.RENDER_DURATION_MS);</script>"
        ),
    );
    let page = config.work_dir.join("produce-duration.html");
    make_directories(config)?;
    std::fs::write(&page, probe).map_err(|error| {
        CapabilityError::Failed(format!("cannot write the probe page {page:?}: {error}"))
    })?;
    let dom = Chrome::new(config).dump_dom(&page).await?;
    let value = dom
        .split(marker)
        .nth(1)
        .map(|rest| {
            rest.chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect::<String>()
        })
        .filter(|raw| !raw.is_empty())
        .and_then(|raw| raw.parse::<f64>().ok())
        .filter(|ms| ms.fract() == 0.0 && (100.0..=120_000.0).contains(ms))
        .map(|ms| ms as u64);
    value.ok_or_else(|| {
        refuse(
            code::DURATION_MS_REQUIRED,
            "duration_ms was omitted and the page exposes no window.RENDER_DURATION_MS in \
             [100, 120000]",
        )
    })
}

// ── the browser ──────────────────────────────────────────────────────────────────────────────

/// The image's own Chromium, invoked one screenshot at a time.
struct Chrome {
    binary: String,
    profile_root: PathBuf,
}

impl Chrome {
    fn new(config: &Config) -> Self {
        let binary = std::env::var(CHROME_ENV).unwrap_or_else(|_| CHROME_DEFAULT.to_string());
        Self {
            binary,
            profile_root: config.work_dir.join("chrome-profiles"),
        }
    }

    /// One headless invocation with a **fresh profile**, removed afterwards.
    ///
    /// A per-invocation profile is what keeps two Chromium launches from ever contending on the
    /// process-singleton lock — measured on this box while another test binary was launching
    /// browsers: a reused profile can leave the second launch parked in the AppKit loop, never
    /// writing its screenshot. It also means a profile written by one Chrome build is never
    /// migrated by the next (the box's Chrome updated mid-session).
    async fn run(&self, extra: Vec<String>) -> Result<process::Finished, CapabilityError> {
        static NEXT_PROFILE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        // One headless browser at a time in this process. A pod runs one job, so this costs
        // production nothing; in a test binary it keeps a frame render from competing with every
        // other Chromium launch on the machine (measured: concurrent launches can leave a
        // screenshot unwritten).
        static CHROME_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
        let _guard = CHROME_LOCK.lock().await;
        std::fs::create_dir_all(&self.profile_root).map_err(|error| {
            CapabilityError::Failed(format!("cannot create {:?}: {error}", self.profile_root))
        })?;
        let profile = self.profile_root.join(format!(
            "{}-{}",
            std::process::id(),
            NEXT_PROFILE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let mut args = vec![
            "--headless=new".to_string(),
            "--disable-gpu".to_string(),
            "--no-sandbox".to_string(),
            "--no-first-run".to_string(),
            "--disable-extensions".to_string(),
            "--hide-scrollbars".to_string(),
            "--disable-dev-shm-usage".to_string(),
            "--force-device-scale-factor=1".to_string(),
            format!("--user-data-dir={}", profile.display()),
            format!("--virtual-time-budget={SETTLE_MS}"),
        ];
        args.extend(extra);
        let finished = process::run(&self.binary, &args).await;
        // Best effort: a profile that cannot be removed is scratch bytes in the job's emptyDir,
        // never a reason to fail a render that already succeeded.
        let _ = std::fs::remove_dir_all(&profile);
        finished
    }

    async fn screenshot(
        &self,
        page: &Path,
        output: &Path,
        width: u32,
        height: u32,
    ) -> Result<(), CapabilityError> {
        // One retry, and the reason is measured rather than defensive: the lock above is
        // per-process, so two test binaries (nextest runs each test in its own process) still
        // launch Chromium at the same moment, and under that contention the browser's own
        // teardown watchdog fires (`Teardown watchdog expired; recording dump and terminating`)
        // and the frame is never written. A fresh launch with a fresh profile succeeds; a real
        // page error fails twice and is reported.
        let mut last = String::new();
        for _ in 0..2 {
            let finished = self
                .run(vec![
                    format!("--window-size={width},{height}"),
                    format!("--screenshot={}", output.display()),
                    format!("file://{}", page.display()),
                ])
                .await?;
            if !finished.ok() {
                last = finished.tail();
                continue;
            }
            let bytes = std::fs::read(output).map_err(|error| {
                CapabilityError::Failed(format!(
                    "the browser reported success but wrote no screenshot at {output:?}: {error}"
                ))
            })?;
            if bytes.is_empty() {
                last = "the browser wrote an empty screenshot".to_string();
                continue;
            }
            return Ok(());
        }
        Err(CapabilityError::Failed(format!(
            "the browser failed to screenshot {page:?} twice: {last}"
        )))
    }

    async fn dump_dom(&self, page: &Path) -> Result<String, CapabilityError> {
        let finished = self
            .run(vec![
                "--dump-dom".to_string(),
                format!("file://{}", page.display()),
            ])
            .await?;
        if !finished.ok() {
            return Err(CapabilityError::Failed(format!(
                "the browser failed to dump {page:?}: {}",
                finished.tail()
            )));
        }
        Ok(String::from_utf8_lossy(&finished.stdout).into_owned())
    }
}

/// Insert a fragment immediately after the document's `<head>` opening tag, so it runs before the
/// page's own scripts. A document without a head falls back to `<html>`, then to the front.
fn inject_head(html: &str, fragment: &str) -> String {
    for tag in ["head", "html"] {
        if let Some(position) = find_tag_open(html, tag)
            && let Some(end) = html[position..].find('>')
        {
            let at = position + end + 1;
            let mut document = String::with_capacity(html.len() + fragment.len());
            document.push_str(&html[..at]);
            document.push_str(fragment);
            document.push_str(&html[at..]);
            return document;
        }
    }
    format!("{fragment}{html}")
}

/// Find `<tag>` or `<tag ...>`, so `<header>` is never mistaken for `<head>`.
fn find_tag_open(html: &str, tag: &str) -> Option<usize> {
    let opening = format!("<{tag}");
    let mut from = 0;
    while let Some(offset) = find_ci(&html[from..], &opening) {
        let position = from + offset;
        match html[position + opening.len()..].chars().next() {
            Some('>') => return Some(position),
            Some(character) if character.is_ascii_whitespace() => return Some(position),
            Some(_) => from = position + opening.len(),
            None => return None,
        }
    }
    None
}

/// Insert a fragment immediately before `</body>` (or append it), so it runs after the page's own
/// scripts have defined their functions.
fn inject_tail(html: &str, fragment: &str) -> String {
    if let Some(position) = find_ci(html, "</body>") {
        let mut document = String::with_capacity(html.len() + fragment.len());
        document.push_str(&html[..position]);
        document.push_str(fragment);
        document.push_str(&html[position..]);
        document
    } else {
        format!("{html}{fragment}")
    }
}

/// An ASCII-case-insensitive search that preserves byte offsets.
fn find_ci(haystack: &str, needle: &str) -> Option<usize> {
    haystack.to_ascii_lowercase().find(needle)
}

/// JSON embedded in a `<script>` must not be able to close it: `<` is escaped so a string
/// containing `</script>` cannot break out of the injection this port owns.
fn script_safe(value: &Value) -> String {
    value.to_string().replace('<', "\\u003c")
}

// ── shared parsing and encoding helpers ──────────────────────────────────────────────────────

fn make_directories(config: &Config) -> Result<(), CapabilityError> {
    std::fs::create_dir_all(&config.work_dir).map_err(|error| {
        CapabilityError::Failed(format!("cannot create {:?}: {error}", config.work_dir))
    })?;
    std::fs::create_dir_all(&config.out_dir).map_err(|error| {
        CapabilityError::Failed(format!("cannot create {:?}: {error}", config.out_dir))
    })
}

/// Encode one still as a clip of exactly `duration` seconds. The captured title-card response
/// reports no audio stream, so none is added.
async fn encode_still(
    png: &Path,
    duration: f64,
    fps: u32,
    crf: u32,
    output: &Path,
) -> Result<(), CapabilityError> {
    let seconds = seconds_text(duration);
    let mut args = strings(&[
        "-y",
        "-hide_banner",
        "-loglevel",
        "error",
        "-loop",
        "1",
        "-framerate",
        &fps.to_string(),
        "-i",
        &png.display().to_string(),
    ]);
    args.extend(strings(&[
        "-t",
        &seconds,
        "-r",
        &fps.to_string(),
        "-vf",
        "setsar=1",
        "-c:v",
        "libx264",
        "-preset",
        "veryfast",
        "-crf",
        &crf.to_string(),
        "-pix_fmt",
        "yuv420p",
    ]));
    args.extend(strings(&[
        "-movflags",
        "+faststart",
        &output.display().to_string(),
    ]));
    let finished = process::run("ffmpeg", &args).await?;
    if !finished.ok() {
        return Err(CapabilityError::Failed(format!(
            "ffmpeg could not encode the still {png:?}: {}",
            finished.tail()
        )));
    }
    Ok(())
}

fn read_output(path: &Path) -> Result<Vec<u8>, CapabilityError> {
    let bytes = std::fs::read(path).map_err(|error| {
        CapabilityError::Failed(format!(
            "ffmpeg reported success but wrote no video at {path:?}: {error}"
        ))
    })?;
    if bytes.is_empty() {
        return Err(CapabilityError::Failed(format!(
            "ffmpeg wrote an empty video at {path:?}"
        )));
    }
    Ok(bytes)
}

fn non_null_refuse(value: &Value, code: &str, field: &str) -> Result<(), CapabilityError> {
    if value.is_null() {
        Ok(())
    } else {
        Err(refuse(code, format!("{field} is not implemented")))
    }
}

fn refuse(code: &str, message: impl Into<String>) -> CapabilityError {
    CapabilityError::refused(code, message)
}

fn is_empty_list(value: &Value) -> bool {
    value.as_array().is_some_and(|items| items.is_empty())
}

/// A file name that must stay a bare name: the executor writes it under `out_dir`.
fn output_name(map: &Map<String, Value>) -> Result<String, CapabilityError> {
    let name = match map.get("name").filter(|value| !value.is_null()) {
        None => DEFAULT_NAME,
        Some(value) => value
            .as_str()
            .ok_or_else(|| CapabilityError::Invalid("name must be a string".to_string()))?,
    };
    if name.is_empty() || Path::new(name).file_name().and_then(|part| part.to_str()) != Some(name) {
        return Err(CapabilityError::Invalid(
            "name must be a bare file name, not a path".to_string(),
        ));
    }
    Ok(name.to_string())
}

/// A positive even dimension. H.264's 4:2:0 chroma plane cannot encode an odd width or height.
///
/// The even constraint is the port's: the captured `TitleCardRequest` has no bounds and the
/// `RecordAnimationRequest` bounds do not require even values, but an odd frame cannot be encoded
/// as yuv420p, so the request is refused rather than silently rescaled.
fn dimension(map: &Map<String, Value>, key: &str, default: u32) -> Result<u32, CapabilityError> {
    let value = dimension_value(map, key, default)?;
    check_even(key, value)
}

/// A dimension inside the captured `RecordAnimationRequest` bounds (min 64; 7680 wide, 4320 tall).
fn bounded_dimension(
    map: &Map<String, Value>,
    key: &str,
    default: u32,
    minimum: u32,
    maximum: u32,
) -> Result<u32, CapabilityError> {
    let value = dimension_value(map, key, default)?;
    if !(minimum..=maximum).contains(&value) {
        return Err(CapabilityError::Invalid(format!(
            "{key} must be an integer in [{minimum}, {maximum}]"
        )));
    }
    check_even(key, value)
}

fn dimension_value(
    map: &Map<String, Value>,
    key: &str,
    default: u32,
) -> Result<u32, CapabilityError> {
    match map.get(key).filter(|value| !value.is_null()) {
        None => Ok(default),
        Some(value) => {
            let raw = value
                .as_u64()
                .ok_or_else(|| CapabilityError::Invalid(format!("{key} must be an integer")))?;
            u32::try_from(raw)
                .map_err(|_| CapabilityError::Invalid(format!("{key} is out of range")))
        }
    }
}

fn check_even(key: &str, value: u32) -> Result<u32, CapabilityError> {
    if value == 0 || !value.is_multiple_of(2) {
        return Err(CapabilityError::Invalid(format!(
            "{key} must be a positive even integer"
        )));
    }
    Ok(value)
}

/// The request's `fps` (`RecordAnimationRequest`'s own field, or the executor's explicit cadence
/// for the two paths whose schema names none).
fn cadence(map: &Map<String, Value>, maximum: u32) -> Result<u32, CapabilityError> {
    match map.get("fps").filter(|value| !value.is_null()) {
        None => Ok(DEFAULT_CADENCE),
        Some(value) => {
            let fps = value
                .as_u64()
                .and_then(|raw| u32::try_from(raw).ok())
                .filter(|fps| (1..=maximum).contains(fps))
                .ok_or_else(|| {
                    CapabilityError::Invalid(format!("fps must be an integer in [1, {maximum}]"))
                })?;
            Ok(fps)
        }
    }
}

fn crf(map: &Map<String, Value>) -> Result<u32, CapabilityError> {
    match map.get("crf").filter(|value| !value.is_null()) {
        None => Ok(DEFAULT_CRF),
        Some(value) => value
            .as_u64()
            .and_then(|raw| u32::try_from(raw).ok())
            .filter(|crf| *crf <= 51)
            .ok_or_else(|| {
                CapabilityError::Invalid("crf must be an integer in [0, 51]".to_string())
            }),
    }
}

fn duration_field(map: &Map<String, Value>, default: f64) -> Result<f64, CapabilityError> {
    match map.get("duration").filter(|value| !value.is_null()) {
        None => Ok(default),
        Some(value) => value
            .as_f64()
            .filter(|seconds| *seconds > 0.0 && *seconds <= MAX_SECONDS)
            .ok_or_else(|| {
                CapabilityError::Invalid(format!("duration must be a number in (0, {MAX_SECONDS}]"))
            }),
    }
}

fn string_field(
    map: &Map<String, Value>,
    key: &str,
    index: usize,
) -> Result<Option<String>, CapabilityError> {
    match map.get(key).filter(|value| !value.is_null()) {
        None => Ok(None),
        Some(value) => value
            .as_str()
            .map(|text| Some(text.to_string()))
            .ok_or_else(|| {
                CapabilityError::Invalid(format!(
                    "shot_list.parts[{index}].{key}: must be a string"
                ))
            }),
    }
}

fn number_or_default(value: &Value, default: f64, field: &str) -> Result<f64, CapabilityError> {
    if value.is_null() {
        return Ok(default);
    }
    value
        .as_f64()
        .ok_or_else(|| CapabilityError::Invalid(format!("{field}: must be a number")))
}

/// A URL is not a file: a media job has no egress, and the runner fetches named inputs before
/// dispatch. Refusing the scheme here names the problem instead of failing at the filesystem.
fn refuse_remote(value: &str, field: &str) -> Result<(), CapabilityError> {
    if value.contains("://") {
        return Err(refuse(
            code::REMOTE_URL,
            format!(
                "{field}={value:?} is remote; inputs are file names inside the job's work directory"
            ),
        ));
    }
    Ok(())
}

/// A hex or named CSS colour. The values are written into a stylesheet, so anything that could
/// leave the declaration is refused rather than interpolated.
fn colour(value: &Value, field: &str) -> Result<String, CapabilityError> {
    let text = value
        .as_str()
        .ok_or_else(|| CapabilityError::Invalid(format!("{field}: must be a string")))?;
    let hex = text.strip_prefix('#').is_some_and(|digits| {
        matches!(digits.len(), 3 | 4 | 6 | 8) && digits.chars().all(|c| c.is_ascii_hexdigit())
    });
    let named = !text.is_empty() && text.chars().all(|c| c.is_ascii_alphabetic());
    if hex || named {
        Ok(text.to_string())
    } else {
        Err(CapabilityError::Invalid(format!(
            "{field}: {text:?} is not a hex or named CSS colour"
        )))
    }
}

fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

fn round(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        })
}

fn seconds_text(seconds: f64) -> String {
    format!("{seconds:.3}")
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// The macOS Chrome this workstation has; the image's binary is `/usr/bin/chromium`.
    const MAC_CHROME: &str = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";

    /// Resolve a browser once and point `CHROME_BIN` at it, so a case that needs Chromium skips
    /// only when the host genuinely has none. `set_var` is unsafe in edition 2024; the call is
    /// serialised by the `OnceLock` and completed before any test dispatches a browser job.
    fn ensure_chrome() -> Option<String> {
        static RESOLVED: OnceLock<Option<String>> = OnceLock::new();
        RESOLVED
            .get_or_init(|| {
                if let Ok(configured) = std::env::var(CHROME_ENV)
                    && Path::new(&configured).is_file()
                {
                    return Some(configured);
                }
                for candidate in [CHROME_DEFAULT, MAC_CHROME] {
                    if Path::new(candidate).is_file() {
                        // Safety: this runs once, inside `get_or_init`, before any test that
                        // reads the variable dispatches a browser job.
                        unsafe { std::env::set_var(CHROME_ENV, candidate) };
                        return Some(candidate.to_string());
                    }
                }
                None
            })
            .clone()
    }

    fn scratch(tag: &str) -> Config {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or_default();
        let base = std::env::temp_dir().join(format!(
            "conveyor-media-produce-{tag}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(base.join("work")).expect("create the work directory");
        std::fs::create_dir_all(base.join("out")).expect("create the out directory");
        Config {
            service: "conveyor-media-test".to_string(),
            work_dir: base.join("work"),
            out_dir: base.join("out"),
        }
    }

    /// A one-second 320x180 colour clip: the supplied asset a shot reads from.
    async fn colour_clip(directory: &Path, name: &str, colour_name: &str) {
        let source = format!("color=c={colour_name}:s=320x180:d=1:r=30");
        let output = directory.join(name);
        let args = strings(&[
            "-y",
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            &source,
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            &output.display().to_string(),
        ]);
        let finished = process::run("ffmpeg", &args).await.expect("ffmpeg runs");
        assert!(finished.ok(), "generating {name}: {}", finished.tail());
    }

    async fn measured_duration(path: &Path) -> f64 {
        let args = strings(&[
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            &path.display().to_string(),
        ]);
        let finished = process::run("ffprobe", &args).await.expect("ffprobe runs");
        assert!(finished.ok(), "probing {path:?}: {}", finished.tail());
        String::from_utf8_lossy(&finished.stdout)
            .trim()
            .parse()
            .expect("ffprobe reports a duration")
    }

    async fn measured_size(path: &Path) -> (u32, u32) {
        let args = strings(&[
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height",
            "-of",
            "csv=p=0",
            &path.display().to_string(),
        ]);
        let finished = process::run("ffprobe", &args).await.expect("ffprobe runs");
        assert!(finished.ok(), "probing {path:?}: {}", finished.tail());
        let text = String::from_utf8_lossy(&finished.stdout);
        let mut parts = text.trim().split(',');
        let width = parts.next().unwrap_or_default().parse().expect("a width");
        let height = parts.next().unwrap_or_default().parse().expect("a height");
        (width, height)
    }

    #[tokio::test]
    async fn two_shots_produce_one_encoded_video() {
        if !(process::available("ffmpeg") && process::available("ffprobe")) {
            eprintln!("ffmpeg/ffprobe not installed; skipping the end-to-end case");
            return;
        }
        let config = scratch("two-shots");
        colour_clip(&config.work_dir, "red.mp4", "red").await;
        colour_clip(&config.work_dir, "blue.mp4", "blue").await;

        // The captured compile request's own shape: a shot list plus its recordings.
        let params = json!({
            "target_width": 320,
            "target_height": 240,
            "fps": 30,
            "shot_list": {"parts": [
                {"id": "intro", "title": "Capture Intro", "part_type": "browser_recording",
                 "estimated_duration_seconds": 1.0},
                {"id": "body", "title": "Body", "part_type": "browser_recording",
                 "estimated_duration_seconds": 0.75}
            ]},
            "recordings": [
                {"part_id": "intro", "video_path": "red.mp4", "duration_seconds": 1.0,
                 "width": 320, "height": 180},
                {"part_id": "body", "video_path": "blue.mp4", "duration_seconds": 1.0,
                 "width": 320, "height": 180}
            ],
            "edit_decisions": [{"part_id": "body", "trim_start_s": 0.25}]
        });
        let result = produce(&params, &config)
            .await
            .expect("the two shots produce");

        let output = config.out_dir.join(DEFAULT_NAME);
        assert!(output.is_file(), "the encoded file lands in out_dir");
        assert_eq!(result["status"], json!("completed"));
        assert_eq!(result["mode"], json!("shot_list"));
        assert_eq!(result["duration_seconds"], json!(1.75));
        assert_eq!(result["width"], json!(320));
        assert_eq!(result["height"], json!(240));
        assert_eq!(result["fps"], json!(30));
        assert!(result["bytes"].as_u64().expect("bytes is a number") > 0);
        assert_eq!(result["shots"][0]["kind"], json!("video"));
        assert_eq!(result["shots"][1]["start"], json!(1.0));

        let measured = measured_duration(&output).await;
        assert!(
            (measured - 1.75).abs() < 0.35,
            "the mux is {measured}s, expected ~1.75s"
        );
        assert_eq!(measured_size(&output).await, (320, 240));
    }

    #[tokio::test]
    async fn an_animation_renders_the_requested_duration() {
        if ensure_chrome().is_none() {
            return;
        }
        if !(process::available("ffmpeg") && process::available("ffprobe")) {
            eprintln!("ffmpeg/ffprobe not installed; skipping the end-to-end case");
            return;
        }
        let config = scratch("animation");
        // The captured animation fixture: a self-contained page with RENDER_DURATION_MS and
        // render(t). 320x240 @ 10 fps for 1 s is the measured reference request.
        let html = "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><style>\
                    html,body{margin:0;height:100%;background:#101c2c}\
                    #bar{position:absolute;top:45%;left:0;height:10%;width:12%;background:#4fd1c5}\
                    </style></head><body><div id=\"bar\"></div><script>\
                    window.RENDER_DURATION_MS = 1000; window.render = function (t) { \
                    var p = Math.max(0, Math.min(1, t / window.RENDER_DURATION_MS)); \
                    document.getElementById('bar').style.left = (p * 88) + '%'; };\
                    </script></body></html>";
        let params = json!({
            "part_id": "anim001",
            "html": html,
            "fps": 10,
            "width": 320,
            "height": 240,
            "duration_ms": 1000
        });
        let result = produce(&params, &config)
            .await
            .expect("the animation renders");

        let output = config.out_dir.join(DEFAULT_NAME);
        assert!(output.is_file(), "the encoded animation lands in out_dir");
        assert!(std::fs::metadata(&output).expect("metadata").len() > 0);
        assert_eq!(result["mode"], json!("animation"));
        assert_eq!(result["part_id"], json!("anim001"));
        assert_eq!(result["frames"], json!(10));
        assert_eq!(result["fps"], json!(10));

        // "within one frame": one frame at 10 fps is 0.1 s.
        let measured = measured_duration(&output).await;
        assert!(
            (measured - 1.0).abs() <= 0.1,
            "the animation is {measured}s, expected 1.0s +/- one frame"
        );
        assert_eq!(measured_size(&output).await, (320, 240));
    }

    #[tokio::test]
    async fn a_title_card_renders_its_text_for_the_requested_duration() {
        if ensure_chrome().is_none() {
            return;
        }
        if !(process::available("ffmpeg") && process::available("ffprobe")) {
            eprintln!("ffmpeg/ffprobe not installed; skipping the end-to-end case");
            return;
        }
        let config = scratch("title-card");
        let params = json!({
            "title": "Capture Title Card",
            "subtitle": "Synthetic",
            "width": 320,
            "height": 240,
            "duration": 1.0
        });
        let result = produce(&params, &config)
            .await
            .expect("the title card renders");

        let output = config.out_dir.join(DEFAULT_NAME);
        assert!(output.is_file(), "the encoded card lands in out_dir");
        assert_eq!(result["mode"], json!("title_card"));
        assert_eq!(result["width"], json!(320));
        assert_eq!(result["height"], json!(240));
        assert_eq!(result["duration_seconds"], json!(1.0));

        let measured = measured_duration(&output).await;
        assert!(
            (measured - 1.0).abs() < 0.15,
            "the card is {measured}s, expected ~1.0s"
        );
        assert_eq!(measured_size(&output).await, (320, 240));
    }

    #[tokio::test]
    async fn an_animation_duration_may_come_from_the_page() {
        if ensure_chrome().is_none() {
            return;
        }
        if !process::available("ffmpeg") {
            eprintln!("ffmpeg not installed; skipping the end-to-end case");
            return;
        }
        let config = scratch("animation-page-duration");
        let html = "<!doctype html><html><head><style>html,body{margin:0;background:#000}\
                    </style></head><body><script>window.RENDER_DURATION_MS = 500;\
                    window.render = function () {};</script></body></html>";
        let params = json!({
            "part_id": "page001",
            "html": html,
            "fps": 10,
            "width": 64,
            "height": 64
        });
        let result = produce(&params, &config)
            .await
            .expect("the page names its duration");
        assert_eq!(result["frames"], json!(5));
    }

    #[tokio::test]
    async fn a_page_without_a_duration_is_refused_by_name() {
        if ensure_chrome().is_none() {
            return;
        }
        let config = scratch("animation-no-duration");
        let params = json!({
            "part_id": "page002",
            "html": "<!doctype html><html><body>static</body></html>",
            "fps": 10
        });
        let error = produce(&params, &config)
            .await
            .expect_err("a duration must be named by the request or the page");
        assert!(
            error.to_string().contains(code::DURATION_MS_REQUIRED),
            "{error}"
        );
    }

    macro_rules! refused_by_name {
        ($name:ident, $code:expr, $params:expr) => {
            #[tokio::test]
            async fn $name() {
                let config = scratch(stringify!($name));
                let error = produce(&$params, &config)
                    .await
                    .expect_err("the unsupported field must be refused");
                let rendered = error.to_string();
                assert!(rendered.contains($code), "expected {} in {rendered}", $code);
            }
        };
    }

    // Shot-list mode.
    refused_by_name!(
        a_catalogue_preset_is_refused_by_name,
        code::PRESET,
        json!({
            "shot_list": {"parts": [{"id": "s1"}]},
            "recordings": [],
            "preset": "quick-preview"
        })
    );
    refused_by_name!(
        a_catalogue_template_is_refused_by_name,
        code::TEMPLATE,
        json!({
            "shot_list": {"parts": [{"id": "s1"}]},
            "recordings": [],
            "template": "corporate-demo"
        })
    );
    refused_by_name!(
        a_catalogue_theme_is_refused_by_name,
        code::THEME,
        json!({
            "shot_list": {"parts": [{"id": "s1"}]},
            "recordings": [],
            "theme_name": "default"
        })
    );
    refused_by_name!(
        a_recording_order_is_refused_by_name,
        code::RECORDING_ORDER,
        json!({
            "shot_list": {"parts": [{"id": "s1"}], "recording_order": ["s1"]},
            "recordings": []
        })
    );
    refused_by_name!(
        transitions_are_refused_by_name,
        code::TRANSITION,
        json!({
            "shot_list": {"parts": [{"id": "s1"}]},
            "recordings": [],
            "crossfade_duration": 0.5
        })
    );
    refused_by_name!(
        effects_are_refused_by_name,
        code::EFFECTS,
        json!({
            "shot_list": {"parts": [{"id": "s1"}]},
            "recordings": [],
            "color_correction": {"contrast": 1.1}
        })
    );
    refused_by_name!(
        a_remote_input_is_refused_by_name,
        code::REMOTE_URL,
        json!({
            "shot_list": {"parts": [{"id": "s1"}]},
            "recordings": [{"part_id": "s1", "video_path": "https://example.test/clip.mp4"}]
        })
    );
    refused_by_name!(
        a_clips_directory_is_refused_by_name,
        code::CLIPS_DIR,
        json!({
            "shot_list": {"parts": [{"id": "s1"}]},
            "recordings": [],
            "clips_dir": "clips"
        })
    );
    refused_by_name!(
        narration_is_refused_by_name,
        code::NARRATION,
        json!({
            "shot_list": {"parts": [{"id": "s1"}]},
            "recordings": [],
            "narration_audio_dir": "voice"
        })
    );
    refused_by_name!(
        llm_editing_is_refused_by_name,
        code::LLM_EDIT,
        json!({
            "shot_list": {"parts": [{"id": "s1"}]},
            "recordings": [],
            "llm_config": {"model": "x"}
        })
    );
    refused_by_name!(
        a_webhook_is_refused_by_name,
        code::WEBHOOK,
        json!({
            "shot_list": {"parts": [{"id": "s1"}]},
            "recordings": [],
            "webhook_url": "https://example.test/hook"
        })
    );
    refused_by_name!(
        a_trim_end_is_refused_by_name,
        code::TRIM_END,
        json!({
            "shot_list": {"parts": [{"id": "s1"}]},
            "recordings": [],
            "edit_decisions": [{"part_id": "s1", "trim_end_s": 0.5}]
        })
    );
    refused_by_name!(
        a_speed_change_is_refused_by_name,
        code::SPEED_CHANGE,
        json!({
            "shot_list": {"parts": [{"id": "s1"}]},
            "recordings": [],
            "edit_decisions": [{"part_id": "s1", "speed_factor": 2.0}]
        })
    );
    refused_by_name!(
        dead_time_removal_is_refused_by_name,
        code::DEAD_TIME,
        json!({
            "shot_list": {"parts": [{"id": "s1"}]},
            "recordings": [],
            "edit_decisions": [{"part_id": "s1", "remove_dead_time": true}]
        })
    );

    // Shot-list parts.
    refused_by_name!(
        a_background_image_is_refused_by_name,
        code::BACKGROUND_IMAGE,
        json!({
            "shot_list": {"parts": [{"id": "s1", "background_image": "bg.png"}]},
            "recordings": []
        })
    );
    refused_by_name!(
        a_logo_is_refused_by_name,
        code::LOGO,
        json!({
            "shot_list": {"parts": [{"id": "s1", "logo_path": "logo.png"}]},
            "recordings": []
        })
    );
    refused_by_name!(
        ai_generation_is_refused_by_name,
        code::AI_GENERATION,
        json!({
            "shot_list": {"parts": [{"id": "s1", "ai_prompt": "a hero image"}]},
            "recordings": []
        })
    );
    refused_by_name!(
        an_asset_reference_is_refused_by_name,
        code::ASSET_REFERENCE,
        json!({
            "shot_list": {"parts": [{"id": "s1", "asset_id": "asset-1"}]},
            "recordings": []
        })
    );
    refused_by_name!(
        a_motion_preset_is_refused_by_name,
        code::MOTION_PRESET,
        json!({
            "shot_list": {"parts": [{"id": "s1", "motion_preset": "kenburns"}]},
            "recordings": []
        })
    );
    refused_by_name!(
        a_chapter_is_refused_by_name,
        code::CHAPTER,
        json!({
            "shot_list": {"parts": [{"id": "s1", "chapter": 1}]},
            "recordings": []
        })
    );
    refused_by_name!(
        an_animation_name_is_refused_by_name,
        code::ANIMATION,
        json!({
            "shot_list": {"parts": [{"id": "s1", "animation": "slide-in"}]},
            "recordings": []
        })
    );
    refused_by_name!(
        a_slide_role_is_refused_by_name,
        code::SLIDE_ROLE,
        json!({
            "shot_list": {"parts": [{"id": "s1", "slide_role": "hero"}]},
            "recordings": []
        })
    );
    refused_by_name!(
        a_clip_type_is_refused_by_name,
        code::CLIP_TYPE,
        json!({
            "shot_list": {"parts": [{"id": "s1", "clip_type": "intro"}]},
            "recordings": []
        })
    );
    refused_by_name!(
        provenance_is_refused_by_name,
        code::PROVENANCE,
        json!({
            "shot_list": {"parts": [{"id": "s1", "provenance": {"source": "capture"}}]},
            "recordings": []
        })
    );
    refused_by_name!(
        an_approval_status_is_refused_by_name,
        code::APPROVAL_STATUS,
        json!({
            "shot_list": {"parts": [{"id": "s1", "approval_status": "approved"}]},
            "recordings": []
        })
    );
    refused_by_name!(
        keeping_ai_audio_is_refused_by_name,
        code::AI_AUDIO,
        json!({
            "shot_list": {"parts": [{"id": "s1", "strip_ai_audio": false}]},
            "recordings": []
        })
    );

    // Title-card mode.
    refused_by_name!(
        a_custom_font_is_refused_by_name,
        code::FONT,
        json!({"title": "T", "theme": {"font_path": "brand.ttf"}})
    );
    refused_by_name!(
        an_inert_card_background_is_refused_by_name,
        code::BACKGROUND_IMAGE,
        json!({"title": "T", "background_image": "bg.png"})
    );

    // Animation mode.
    refused_by_name!(
        a_template_animation_is_refused_by_name,
        code::TEMPLATE,
        json!({"part_id": "p1", "template_id": "social_card"})
    );
    refused_by_name!(
        a_webm_animation_is_refused_by_name,
        code::OUTPUT_FORMAT,
        json!({"part_id": "p1", "html": "<html></html>", "output_format": "webm"})
    );
    refused_by_name!(
        a_transparent_animation_is_refused_by_name,
        code::TRANSPARENT,
        json!({"part_id": "p1", "html": "<html></html>", "transparent": true})
    );
    refused_by_name!(
        an_animation_quality_preset_is_refused_by_name,
        code::PRESET,
        json!({"part_id": "p1", "html": "<html></html>", "quality_preset": "preview"})
    );
}
