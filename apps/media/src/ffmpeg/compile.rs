//! `media.compile_video` — shots to one encoded video.
//!
//! The reference's own request document is the schema: `CompileRequest` in
//! `contract/video_compiler_service/openapi.lock.json`, with its `Shot` objects carrying every
//! field the capture pins. This port implements the **assembly** path the goldens and the job
//! contract name — each shot's source, in-point and duration, the master cadence, and the concat
//! into one encoded file — and refuses every other render branch **by name** with a stable
//! [`CapabilityError::refused`] code. A caller that sets grading, caption burn-in, an ambient bed,
//! cursor/focus tracks, narration, callouts, overlays, transitions, split screen or per-shot audio
//! is told which capability is missing; nothing is accepted and silently ignored.
//!
//! Every input is already a file in the job's work directory (the runner fetches named inputs
//! before dispatch), so `video_url` and `image_url` are read as **file names** inside
//! [`Config::work_dir`], never as URLs. A shot with neither a video nor an image is the reference's
//! solid card; its `description` text is a refusal because text drawing is not implemented.
//!
//! ## The output and the captured contract
//!
//! The encoded file is written to `config.out_dir/<name>` (`name` defaults to `compile.mp4`), and
//! the result document is the **terminal** form of the captured compile job:
//!
//! ```json
//! {"status":"completed","output":"film.mp4","content_type":"video/mp4","bytes":1234,
//!  "sha256":"…","width":1920,"height":1080,"fps":30,"duration_seconds":2.0,
//!  "shot_timeline":null}
//! ```
//!
//! Tolerated differences from the captured bodies (all named here, none hidden):
//!
//! * `job_id` and `poll_url` — minted per submission by the reference's durable ledger
//!   (`contract/video_compiler_service/README.md`, "Tolerance"). The executor has neither.
//! * `request_id` and `timestamp` — generated per request in every captured error envelope
//!   (same README section). The executor's failures are [`CapabilityError`]s, not envelopes.
//! * `status` — the captured `compile_submit.body` pins the **submit-time** value `processing`;
//!   this synchronous executor reports the same field at the terminal state the captured
//!   `JobStatus` enum calls `completed` (lock: `JobStatus.status`).
//! * absolute temporary paths — never emitted; `output` is a bare file name.
//!
//! Everything else in the result (`output`, `bytes`, `sha256`, `width`, `height`, `fps`,
//! `duration_seconds`, `shot_timeline`) is deterministic and asserted by the tests below.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use crate::common::{CapabilityError, Config, process};

/// The kind slug this module implements.
pub const KIND: &str = "media.compile_video";

/// The stable refusal codes. Each names one capability this port does not implement, so a caller
/// can branch on the code instead of reading a sentence.
pub mod code {
    pub const GRADING: &str = "grading_not_supported";
    pub const CAPTIONS: &str = "captions_not_supported";
    pub const AMBIENT_BED: &str = "ambient_bed_not_supported";
    pub const CURSOR_TRACK: &str = "cursor_track_not_supported";
    pub const FOCUS_TRACK: &str = "focus_track_not_supported";
    pub const CALLOUTS: &str = "callouts_not_supported";
    pub const OVERLAY_CLIPS: &str = "overlay_clips_not_supported";
    pub const TEXT_OVERLAYS: &str = "text_overlays_not_supported";
    pub const TEXT_CARD: &str = "text_card_not_supported";
    pub const NARRATION: &str = "narration_not_supported";
    pub const SHOT_AUDIO: &str = "shot_audio_not_supported";
    pub const SFX: &str = "sfx_not_supported";
    pub const SPLIT_SCREEN: &str = "split_screen_not_supported";
    pub const TRANSITION: &str = "transition_not_supported";
    pub const MAX_SECONDS: &str = "max_seconds_not_supported";
    pub const TAGS: &str = "tags_not_supported";
    pub const CLIP_ANCHOR_END: &str = "clip_anchor_end_not_supported";
    pub const FIT_SCROLL: &str = "fit_scroll_not_supported";
    pub const RETIME_MOTION: &str = "retime_motion_not_supported";
    pub const RETIME_BUDGET: &str = "retime_budget_not_supported";
}

/// The render canvas when the caller does not name one. The captured `CompileRequest` does not
/// carry a canvas: it is a property of the delivery tier, supplied here as a job parameter.
const DEFAULT_WIDTH: u32 = 1920;
const DEFAULT_HEIGHT: u32 = 1080;

/// The master cadence when neither `cadence` nor any `target_fps` names one.
const DEFAULT_CADENCE: u32 = 30;

/// The cadences `cadence` snaps to (the captured field's own description).
const LEGAL_CADENCES: [u32; 5] = [24, 25, 30, 50, 60];

/// The brand navy the reference pads with (`_pad_color` in its renderer).
const PAD_COLOUR: &str = "0x0B1F3A";

/// The output file name when the request does not name one.
const DEFAULT_NAME: &str = "compile.mp4";

/// Compile one request into the video it names.
pub async fn compile(params: &Value, config: &Config) -> Result<Value, CapabilityError> {
    let plan = Plan::parse(params, &config.work_dir)?;

    std::fs::create_dir_all(&config.work_dir).map_err(|error| {
        CapabilityError::Failed(format!("cannot create {:?}: {error}", config.work_dir))
    })?;
    std::fs::create_dir_all(&config.out_dir).map_err(|error| {
        CapabilityError::Failed(format!("cannot create {:?}: {error}", config.out_dir))
    })?;

    // One intermediate per shot, all encoded with identical parameters, so the final join is the
    // reference's fast path: the concat demuxer with a stream copy, not a second re-encode.
    let mut parts = Vec::with_capacity(plan.shots.len());
    for (index, shot) in plan.shots.iter().enumerate() {
        let part = config.work_dir.join(format!("compile-part-{index}.mp4"));
        encode_shot(shot, &plan, &part).await?;
        parts.push(part);
    }

    let list = config.work_dir.join("compile-concat.txt");
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

    let bytes = std::fs::read(&output).map_err(|error| {
        CapabilityError::Failed(format!(
            "ffmpeg reported success but wrote no video at {output:?}: {error}"
        ))
    })?;
    if bytes.is_empty() {
        return Err(CapabilityError::Failed(
            "ffmpeg wrote an empty video".to_string(),
        ));
    }
    Ok(result_document(
        &plan,
        bytes.len() as u64,
        &hex(&Sha256::digest(&bytes)),
    ))
}

/// One parsed `CompileRequest`, reduced to what this port honours.
struct Plan {
    width: u32,
    height: u32,
    cadence: u32,
    emit_shot_timeline: bool,
    name: String,
    shots: Vec<Shot>,
}

/// One shot, with the window it reads from its source and the duration it contributes.
struct Shot {
    id: String,
    line_id: Option<String>,
    source: Source,
    /// Seconds skipped at the head of a footage clip (`clip_lead_seconds`).
    lead: f64,
    /// Seconds consumed from the source (`seconds * speed`, or the explicit window).
    window: f64,
    /// Seconds the shot occupies on the final timeline (after `speed`).
    duration: f64,
    speed: f64,
    fit: Fit,
    /// The declared source rate; only the master-cadence selector consumes it.
    target_fps: Option<f64>,
}

enum Source {
    Video(PathBuf),
    Image(PathBuf),
    /// No visual input: the reference's solid card (`description` text is refused, not drawn).
    Card,
}

#[derive(Clone, Copy)]
enum Fit {
    Contain,
    Cover,
}

impl Plan {
    fn parse(params: &Value, work_dir: &Path) -> Result<Self, CapabilityError> {
        let map = params
            .as_object()
            .ok_or_else(|| CapabilityError::Invalid("params must be a JSON object".to_string()))?;

        // Refuse every captured top-level branch this port does not implement, by name, before
        // reading anything this port does implement. `None`-equivalent values are accepted: the
        // reference itself treats null / an empty list / an all-default empty object as a no-op.
        for (key, value) in map {
            match key.as_str() {
                "shots" | "cadence" | "emit_shot_timeline" | "name" | "width" | "height" => {}
                "ambient" => no_op_or_refuse(value, code::AMBIENT_BED, "ambient", is_empty_list)?,
                "music" => no_op_or_refuse(value, code::AMBIENT_BED, "music", is_empty_object)?,
                "captions" => no_op_or_refuse(value, code::CAPTIONS, "captions", is_empty_list)?,
                "captions_karaoke" => {
                    if value.is_null() {
                        // `None` is the captured default.
                    } else if value.as_bool() == Some(false) {
                        // The captured default: a no-op.
                    } else if value.as_bool() == Some(true) {
                        return Err(refuse(
                            code::CAPTIONS,
                            "captions_karaoke is not implemented",
                        ));
                    } else {
                        return Err(CapabilityError::Invalid(
                            "captions_karaoke must be a boolean".to_string(),
                        ));
                    }
                }
                "captions_render" => {
                    if value.is_null() || value.as_str() == Some("overlay") {
                        // `overlay` is the captured default and needs no burn-in.
                    } else if value.as_str() == Some("burned_in") {
                        return Err(refuse(
                            code::CAPTIONS,
                            "captions_render=burned_in is not implemented",
                        ));
                    } else {
                        return Err(CapabilityError::Invalid(
                            "captions_render must be \"overlay\" or \"burned_in\"".to_string(),
                        ));
                    }
                }
                "caption_box_colour"
                | "caption_box_opacity"
                | "caption_font_family"
                | "caption_mood"
                | "caption_text_colour" => {
                    if !value.is_null() {
                        return Err(refuse(code::CAPTIONS, format!("{key} is not implemented")));
                    }
                }
                "finish" | "mastering" => {
                    no_op_or_refuse(value, code::GRADING, key, is_empty_object)?;
                }
                "master_lufs" => {
                    if !value.is_null() {
                        return Err(refuse(code::GRADING, "master_lufs is not implemented"));
                    }
                }
                "tags" => no_op_or_refuse(value, code::TAGS, "tags", is_empty_object)?,
                other => {
                    return Err(CapabilityError::Invalid(format!(
                        "unknown parameter {other:?}"
                    )));
                }
            }
        }

        let raw_shots = map
            .get("shots")
            .ok_or_else(|| CapabilityError::Invalid("shots: Field required".to_string()))?;
        let raw_shots = raw_shots.as_array().ok_or_else(|| {
            CapabilityError::Invalid("shots: Input should be a valid list".to_string())
        })?;
        if raw_shots.is_empty() {
            return Err(CapabilityError::Invalid(
                "shots: List should have at least 1 item after validation, not 0".to_string(),
            ));
        }
        if raw_shots.len() > 2000 {
            return Err(CapabilityError::Invalid(
                "shots: List should have at most 2000 items after validation".to_string(),
            ));
        }

        let mut shots = Vec::with_capacity(raw_shots.len());
        for (index, raw) in raw_shots.iter().enumerate() {
            shots.push(Shot::parse(index, raw, work_dir)?);
        }

        let cadence = match map.get("cadence").filter(|value| !value.is_null()) {
            Some(value) => {
                let cadence = value.as_u64().ok_or_else(|| {
                    CapabilityError::Invalid("cadence must be an integer".to_string())
                })?;
                if cadence == 0 {
                    return Err(CapabilityError::Invalid(
                        "cadence must be positive".to_string(),
                    ));
                }
                snap_cadence(cadence as f64)
            }
            None => {
                let declared = shots
                    .iter()
                    .filter_map(|shot| shot.target_fps)
                    .fold(0.0_f64, f64::max);
                if declared > 0.0 {
                    snap_cadence(declared)
                } else {
                    DEFAULT_CADENCE
                }
            }
        };

        let emit_shot_timeline = match map.get("emit_shot_timeline") {
            None => false,
            Some(value) => value.as_bool().ok_or_else(|| {
                CapabilityError::Invalid("emit_shot_timeline must be a boolean".to_string())
            })?,
        };

        Ok(Self {
            width: dimension(map, "width", DEFAULT_WIDTH)?,
            height: dimension(map, "height", DEFAULT_HEIGHT)?,
            cadence,
            emit_shot_timeline,
            name: output_name(map)?,
            shots,
        })
    }
}

impl Shot {
    fn parse(index: usize, raw: &Value, work_dir: &Path) -> Result<Self, CapabilityError> {
        let map = raw.as_object().ok_or_else(|| {
            CapabilityError::Invalid(format!(
                "shots.{index}: Input should be a valid dictionary or object"
            ))
        })?;

        // The refusal pass runs before any path is resolved, so an unsupported parameter is
        // reported even when the named source is not present.
        for (key, value) in map {
            match key.as_str() {
                // `footage_viewport` only remaps cursor/zoom coordinates, and both tracks are
                // refused below; without them it is inert in the reference too.
                "id" | "seconds" | "video_url" | "image_url" | "clip_lead_seconds"
                | "clip_out_seconds" | "speed" | "fit" | "target_fps" | "retime"
                | "clip_anchor" | "line_id" | "footage_viewport" => {}
                "adjust" => {
                    if !value.is_null() {
                        return Err(refuse(
                            code::GRADING,
                            format!("shots.{index}.adjust is not implemented"),
                        ));
                    }
                }
                "audio" => no_op_or_refuse(
                    value,
                    code::SHOT_AUDIO,
                    &format!("shots.{index}.audio"),
                    is_empty_object,
                )?,
                "audio_url" => {
                    if !value.is_null() {
                        return Err(refuse(
                            code::NARRATION,
                            format!("shots.{index}.audio_url is not implemented"),
                        ));
                    }
                }
                "callouts" => no_op_or_refuse(
                    value,
                    code::CALLOUTS,
                    &format!("shots.{index}.callouts"),
                    is_empty_list,
                )?,
                "caption" => {
                    if !value.is_null() {
                        return Err(refuse(
                            code::CAPTIONS,
                            format!("shots.{index}.caption is not implemented"),
                        ));
                    }
                }
                "cursor_track" => no_op_or_refuse(
                    value,
                    code::CURSOR_TRACK,
                    &format!("shots.{index}.cursor_track"),
                    is_empty_list,
                )?,
                "description" => {
                    if !value.is_null() && !is_blank_string(value) {
                        return Err(refuse(
                            code::TEXT_CARD,
                            format!("shots.{index}.description is not implemented"),
                        ));
                    }
                }
                "focus_track" => no_op_or_refuse(
                    value,
                    code::FOCUS_TRACK,
                    &format!("shots.{index}.focus_track"),
                    is_empty_list,
                )?,
                "max_seconds" => {
                    if !value.is_null() {
                        return Err(refuse(
                            code::MAX_SECONDS,
                            format!("shots.{index}.max_seconds is not implemented"),
                        ));
                    }
                }
                "overlay_clips" => no_op_or_refuse(
                    value,
                    code::OVERLAY_CLIPS,
                    &format!("shots.{index}.overlay_clips"),
                    is_empty_list,
                )?,
                "overlays" => no_op_or_refuse(
                    value,
                    code::TEXT_OVERLAYS,
                    &format!("shots.{index}.overlays"),
                    is_empty_list,
                )?,
                "sfx" => no_op_or_refuse(
                    value,
                    code::SFX,
                    &format!("shots.{index}.sfx"),
                    is_empty_list,
                )?,
                "split" => {
                    if !value.is_null() {
                        return Err(refuse(
                            code::SPLIT_SCREEN,
                            format!("shots.{index}.split is not implemented"),
                        ));
                    }
                }
                "transition" => {
                    if !value.is_null() {
                        return Err(refuse(
                            code::TRANSITION,
                            format!("shots.{index}.transition is not implemented"),
                        ));
                    }
                }
                "transition_sfx" => {
                    if !value.is_null() {
                        return Err(refuse(
                            code::SFX,
                            format!("shots.{index}.transition_sfx is not implemented"),
                        ));
                    }
                }
                other => {
                    return Err(CapabilityError::Invalid(format!(
                        "shots.{index}: unknown field {other:?}"
                    )));
                }
            }
        }

        let id = map
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| CapabilityError::Invalid(format!("shots.{index}.id: Field required")))?;
        if id.is_empty()
            || id.len() > 128
            || !id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
        {
            return Err(CapabilityError::Invalid(format!(
                "shots.{index}.id: must match ^[A-Za-z0-9_.-]+$ and be at most 128 characters"
            )));
        }

        let seconds = map
            .get("seconds")
            .and_then(Value::as_f64)
            .filter(|seconds| *seconds > 0.0 && *seconds <= 3600.0)
            .ok_or_else(|| {
                CapabilityError::Invalid(format!(
                    "shots.{index}.seconds: must be a number in (0, 3600]"
                ))
            })?;

        let speed = match map.get("speed").filter(|value| !value.is_null()) {
            None => 1.0,
            Some(value) => value
                .as_f64()
                .filter(|speed| (0.25..=4.0).contains(speed))
                .ok_or_else(|| {
                    CapabilityError::Invalid(format!(
                        "shots.{index}.speed: must be a number in [0.25, 4.0]"
                    ))
                })?,
        };

        let fit = match map.get("fit").filter(|value| !value.is_null()) {
            None => Fit::Contain,
            Some(value) => match value.as_str() {
                Some("contain") => Fit::Contain,
                Some("cover") => Fit::Cover,
                Some("scroll") => {
                    return Err(refuse(
                        code::FIT_SCROLL,
                        format!("shots.{index}.fit=scroll is not implemented"),
                    ));
                }
                Some(other) => {
                    return Err(CapabilityError::Invalid(format!(
                        "shots.{index}.fit: {other:?} is not one of contain, cover, scroll"
                    )));
                }
                None => {
                    return Err(CapabilityError::Invalid(format!(
                        "shots.{index}.fit: must be a string"
                    )));
                }
            },
        };

        match map
            .get("clip_anchor")
            .filter(|value| !value.is_null())
            .map(|value| {
                value.as_str().ok_or_else(|| {
                    CapabilityError::Invalid(format!("shots.{index}.clip_anchor: must be a string"))
                })
            }) {
            None => {}
            Some(Ok("start")) => {}
            Some(Ok("end")) => {
                return Err(refuse(
                    code::CLIP_ANCHOR_END,
                    format!("shots.{index}.clip_anchor=end is not implemented"),
                ));
            }
            Some(Ok(other)) => {
                return Err(CapabilityError::Invalid(format!(
                    "shots.{index}.clip_anchor: {other:?} is not one of start, end"
                )));
            }
            Some(Err(error)) => return Err(error),
        }

        // The captured field says an unset/unknown `retime` falls back to the service-wide clean
        // drop/dup path, which is exactly the `fps` filter below. Only the two re-timers this port
        // cannot run are refused.
        match map.get("retime").filter(|value| !value.is_null()) {
            None => {}
            Some(value) => match value.as_str() {
                Some("motion") => {
                    return Err(refuse(
                        code::RETIME_MOTION,
                        format!("shots.{index}.retime=motion is not implemented"),
                    ));
                }
                Some("budget") => {
                    return Err(refuse(
                        code::RETIME_BUDGET,
                        format!("shots.{index}.retime=budget is not implemented"),
                    ));
                }
                Some(_) => {}
                None => {
                    return Err(CapabilityError::Invalid(format!(
                        "shots.{index}.retime: must be a string"
                    )));
                }
            },
        }

        let clip_lead = match map
            .get("clip_lead_seconds")
            .filter(|value| !value.is_null())
        {
            None => 0.0,
            Some(value) => value.as_f64().filter(|lead| *lead >= 0.0).ok_or_else(|| {
                CapabilityError::Invalid(format!(
                    "shots.{index}.clip_lead_seconds: must be a non-negative number"
                ))
            })?,
        };

        let clip_out = match map.get("clip_out_seconds").filter(|value| !value.is_null()) {
            None => None,
            Some(value) => Some(value.as_f64().filter(|out| *out > 0.0).ok_or_else(|| {
                CapabilityError::Invalid(format!(
                    "shots.{index}.clip_out_seconds: must be a positive number"
                ))
            })?),
        };

        let target_fps = match map.get("target_fps").filter(|value| !value.is_null()) {
            None => None,
            Some(value) => Some(value.as_f64().filter(|fps| *fps > 0.0).ok_or_else(|| {
                CapabilityError::Invalid(format!(
                    "shots.{index}.target_fps: must be a positive number"
                ))
            })?),
        };

        let line_id = match map.get("line_id").filter(|value| !value.is_null()) {
            None => None,
            Some(value) => Some(
                value
                    .as_str()
                    .ok_or_else(|| {
                        CapabilityError::Invalid(format!("shots.{index}.line_id: must be a string"))
                    })?
                    .to_string(),
            ),
        };

        let video = optional_file(map, "video_url", index)?;
        let image = optional_file(map, "image_url", index)?;
        let source = match (video, image) {
            (Some(_), Some(_)) => {
                return Err(CapabilityError::Invalid(format!(
                    "shots.{index}: pass at most one of video_url or image_url"
                )));
            }
            (Some(name), None) => Source::Video(process::resolve_input(work_dir, name)?),
            (None, Some(name)) => Source::Image(process::resolve_input(work_dir, name)?),
            (None, None) => Source::Card,
        };

        let (lead, window, duration) = match &source {
            Source::Video(_) => match clip_out {
                Some(out) => {
                    if out <= clip_lead {
                        return Err(CapabilityError::Invalid(format!(
                            "shots.{index}.clip_out_seconds must be greater than clip_lead_seconds"
                        )));
                    }
                    let window = out - clip_lead;
                    (clip_lead, window, window / speed)
                }
                None => (clip_lead, seconds * speed, seconds),
            },
            Source::Image(_) | Source::Card => (0.0, 0.0, seconds),
        };

        Ok(Self {
            id: id.to_string(),
            line_id,
            source,
            lead,
            window,
            duration,
            speed,
            fit,
            target_fps,
        })
    }
}

/// Encode one shot into an intermediate whose codec parameters are identical across shots.
async fn encode_shot(shot: &Shot, plan: &Plan, output: &Path) -> Result<(), CapabilityError> {
    let mut args = strings(&["-y", "-hide_banner", "-loglevel", "error"]);
    let mut filters: Vec<String> = Vec::new();
    let mut maps: Vec<String> = Vec::new();

    match &shot.source {
        Source::Video(path) => {
            args.push("-ss".to_string());
            args.push(seconds_text(shot.lead));
            args.push("-t".to_string());
            args.push(seconds_text(shot.window));
            args.push("-i".to_string());
            args.push(path.display().to_string());
            filters.push(format!("[0:v]{}[v]", video_chain(shot, plan)));
            maps.push("[v]".to_string());
            if has_audio(path).await? {
                filters.push(format!("[0:a]{}[a]", audio_chain(shot.speed)));
                maps.push("[a]".to_string());
            } else {
                push_silence(&mut args, shot);
                maps.push("1:a".to_string());
            }
        }
        Source::Image(path) => {
            args.push("-loop".to_string());
            args.push("1".to_string());
            args.push("-i".to_string());
            args.push(path.display().to_string());
            filters.push(format!("[0:v]{}[v]", still_chain(shot, plan)));
            maps.push("[v]".to_string());
            push_silence(&mut args, shot);
            maps.push("1:a".to_string());
        }
        Source::Card => {
            args.push("-f".to_string());
            args.push("lavfi".to_string());
            args.push("-i".to_string());
            args.push(format!(
                "color=c={PAD_COLOUR}:s={}x{}:r={}:d={}",
                plan.width,
                plan.height,
                plan.cadence,
                seconds_text(shot.duration)
            ));
            filters.push(format!("[0:v]setsar=1,fps={}[v]", plan.cadence));
            maps.push("[v]".to_string());
            push_silence(&mut args, shot);
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
    args.push(plan.cadence.to_string());
    args.extend(strings(&[
        "-c:v", "libx264", "-preset", "veryfast", "-crf", "23", "-pix_fmt", "yuv420p", "-c:a",
        "aac", "-b:a", "128k", "-ar", "48000", "-ac", "2",
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

/// Fit/scale/pad a footage (or still) frame, retime it onto the master cadence, and pad a short
/// source by cloning its last frame so every intermediate is exactly `shot.duration` long.
fn video_chain(shot: &Shot, plan: &Plan) -> String {
    let fit = fit_filter(shot.fit, plan);
    format!(
        "{fit},setsar=1,setpts=PTS/{},fps={},tpad=stop_mode=clone:stop_duration={}",
        number_text(shot.speed),
        plan.cadence,
        seconds_text(shot.duration)
    )
}

fn still_chain(shot: &Shot, plan: &Plan) -> String {
    let fit = fit_filter(shot.fit, plan);
    format!(
        "{fit},setsar=1,fps={},tpad=stop_mode=clone:stop_duration={}",
        plan.cadence,
        seconds_text(shot.duration)
    )
}

fn fit_filter(fit: Fit, plan: &Plan) -> String {
    let width = plan.width;
    let height = plan.height;
    match fit {
        Fit::Contain => format!(
            "scale={width}:{height}:force_original_aspect_ratio=decrease,\
             pad={width}:{height}:(ow-iw)/2:(oh-ih)/2:color={PAD_COLOUR}"
        ),
        Fit::Cover => format!(
            "scale={width}:{height}:force_original_aspect_ratio=increase,crop={width}:{height}"
        ),
    }
}

/// Keep a footage clip's own audio, retimed with it. `atempo` is chained so the whole declared
/// `speed` range (0.25–4.0) stays inside the filter's 0.5–2.0 per-instance range.
fn audio_chain(speed: f64) -> String {
    let mut chain = String::new();
    if let Some(tempo) = atempo(speed) {
        chain.push_str(&tempo);
        chain.push(',');
    }
    chain.push_str("aformat=sample_fmts=fltp:channel_layouts=stereo");
    chain
}

fn atempo(speed: f64) -> Option<String> {
    if (speed - 1.0).abs() < f64::EPSILON {
        return None;
    }
    let mut parts = Vec::new();
    let mut rest = speed;
    while rest > 2.0 {
        parts.push("atempo=2.0".to_string());
        rest /= 2.0;
    }
    while rest < 0.5 {
        parts.push("atempo=0.5".to_string());
        rest *= 2.0;
    }
    parts.push(format!("atempo={rest}"));
    Some(parts.join(","))
}

/// Add a bounded silent stereo track so every intermediate carries the same stream layout; the
/// concat demuxer's stream-copy path requires it.
fn push_silence(args: &mut Vec<String>, shot: &Shot) {
    args.extend(strings(&[
        "-f",
        "lavfi",
        "-t",
        &seconds_text(shot.duration),
        "-i",
        "anullsrc=r=48000:cl=stereo",
    ]));
}

/// Whether a file carries an audio stream. An absent audio track is silence in the reference's
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
    Ok(!finished
        .stdout
        .iter()
        .all(|byte| byte.is_ascii_whitespace()))
}

/// The result document: the terminal state of the captured job, with the deterministic fields the
/// tests pin. Volatile fields are listed in the module comment and are never invented here.
fn result_document(plan: &Plan, bytes: u64, sha256: &str) -> Value {
    let mut elapsed = 0.0_f64;
    let mut timeline = Vec::with_capacity(plan.shots.len());
    for shot in &plan.shots {
        let start = elapsed;
        elapsed += shot.duration;
        timeline.push(json!({
            "line_id": shot.line_id,
            "start": round(start),
            "end": round(elapsed),
        }));
    }
    json!({
        "status": "completed",
        "output": plan.name,
        "content_type": "video/mp4",
        "bytes": bytes,
        "sha256": sha256,
        "width": plan.width,
        "height": plan.height,
        "fps": plan.cadence,
        "duration_seconds": round(elapsed),
        "shot_timeline": if plan.emit_shot_timeline { Value::Array(timeline) } else { Value::Null },
    })
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

fn refuse(code: &str, message: impl Into<String>) -> CapabilityError {
    CapabilityError::refused(code, message)
}

/// Accept a value the reference itself treats as absent (`null`, an empty list, an all-default
/// empty object); refuse anything that actually asks for the unimplemented feature.
fn no_op_or_refuse(
    value: &Value,
    code: &str,
    field: &str,
    is_no_op: fn(&Value) -> bool,
) -> Result<(), CapabilityError> {
    if value.is_null() || is_no_op(value) {
        Ok(())
    } else {
        Err(refuse(code, format!("{field} is not implemented")))
    }
}

fn is_empty_list(value: &Value) -> bool {
    value.as_array().is_some_and(|items| items.is_empty())
}

fn is_empty_object(value: &Value) -> bool {
    value.as_object().is_some_and(|map| map.is_empty())
}

fn is_blank_string(value: &Value) -> bool {
    value.as_str().is_some_and(str::is_empty)
}

fn optional_file<'a>(
    map: &'a Map<String, Value>,
    key: &str,
    index: usize,
) -> Result<Option<&'a str>, CapabilityError> {
    match map.get(key).filter(|value| !value.is_null()) {
        None => Ok(None),
        Some(value) => value.as_str().map(Some).ok_or_else(|| {
            CapabilityError::Invalid(format!("shots.{index}.{key}: must be a string"))
        }),
    }
}

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

fn dimension(map: &Map<String, Value>, key: &str, default: u32) -> Result<u32, CapabilityError> {
    let value = match map.get(key).filter(|value| !value.is_null()) {
        None => default,
        Some(value) => {
            let raw = value
                .as_u64()
                .ok_or_else(|| CapabilityError::Invalid(format!("{key} must be an integer")))?;
            u32::try_from(raw)
                .map_err(|_| CapabilityError::Invalid(format!("{key} is out of range")))?
        }
    };
    if value == 0 || value % 2 != 0 {
        return Err(CapabilityError::Invalid(format!(
            "{key} must be a positive even integer"
        )));
    }
    Ok(value)
}

/// Snap a declared frame rate onto the legal cadence set. A tie resolves to the lower cadence, so
/// the selector is deterministic.
fn snap_cadence(fps: f64) -> u32 {
    LEGAL_CADENCES
        .iter()
        .copied()
        .min_by(|left, right| {
            let left_gap = (fps - f64::from(*left)).abs();
            let right_gap = (fps - f64::from(*right)).abs();
            left_gap
                .partial_cmp(&right_gap)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(left.cmp(right))
        })
        .unwrap_or(DEFAULT_CADENCE)
}

fn seconds_text(seconds: f64) -> String {
    format!("{seconds:.3}")
}

fn number_text(value: f64) -> String {
    let rendered = format!("{value}");
    if rendered.contains('.') {
        rendered
    } else {
        format!("{value}.0")
    }
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// The captured happy-path body: `202 JobAccepted`, whose only deterministic field is the
    /// submit-time `status`. `job_id` and `poll_url` are the README's declared volatile fields.
    const GOLDEN_ACCEPTED: &str =
        include_str!("../../contract/video_compiler_service/golden/compile_submit.body");
    /// The captured 422 body: its message is the one this port repeats for a missing `shots`.
    const GOLDEN_VALIDATION: &str =
        include_str!("../../contract/video_compiler_service/golden/compile_submit_validation.body");

    /// A fresh work/out pair under the system temp directory, unique to one test invocation.
    fn scratch(tag: &str) -> Config {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or_default();
        let base = std::env::temp_dir().join(format!(
            "conveyor-media-compile-{tag}-{}-{nanos}",
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

    /// A one-second 320x180 colour clip, the fixture the real compile test feeds two shots.
    async fn colour_clip(directory: &Path, name: &str, colour: &str) {
        let source = format!("color=c={colour}:s=320x180:d=1:r=30");
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

    /// A one-second 320x180 colour clip that also carries a tone, so the audio path is real.
    async fn clip_with_audio(directory: &Path, name: &str) {
        let output = directory.join(name);
        let args = strings(&[
            "-y",
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=green:s=320x180:d=1:r=30",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=1",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-shortest",
            &output.display().to_string(),
        ]);
        let finished = process::run("ffmpeg", &args).await.expect("ffmpeg runs");
        assert!(finished.ok(), "generating {name}: {}", finished.tail());
    }

    /// The encoded file's real duration, read back with ffprobe.
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

    #[test]
    fn the_result_document_keeps_the_captured_compile_shape() {
        let golden: Value = serde_json::from_str(GOLDEN_ACCEPTED).expect("the capture is JSON");
        assert_eq!(
            golden["status"],
            json!("processing"),
            "the capture pins the submit-time state"
        );
        assert!(golden.get("job_id").is_some());
        assert!(golden.get("poll_url").is_some());

        let plan = Plan {
            width: 1920,
            height: 1080,
            cadence: 30,
            emit_shot_timeline: true,
            name: "film.mp4".to_string(),
            shots: vec![Shot {
                id: "s1".to_string(),
                line_id: Some("line-1".to_string()),
                source: Source::Card,
                lead: 0.0,
                window: 0.0,
                duration: 1.5,
                speed: 1.0,
                fit: Fit::Contain,
                target_fps: None,
            }],
        };
        let result = result_document(&plan, 4321, "abc");

        // Same field, at the terminal state the captured `JobStatus` enum names.
        assert_eq!(result["status"], json!("completed"));
        assert_eq!(result["output"], json!("film.mp4"));
        assert_eq!(result["content_type"], json!("video/mp4"));
        assert_eq!(result["bytes"], json!(4321));
        assert_eq!(result["sha256"], json!("abc"));
        assert_eq!(result["width"], json!(1920));
        assert_eq!(result["height"], json!(1080));
        assert_eq!(result["fps"], json!(30));
        assert_eq!(result["duration_seconds"], json!(1.5));
        assert_eq!(result["shot_timeline"][0]["line_id"], json!("line-1"));
        assert_eq!(result["shot_timeline"][0]["start"], json!(0.0));
        assert_eq!(result["shot_timeline"][0]["end"], json!(1.5));
        for tolerated in ["job_id", "poll_url", "request_id", "timestamp"] {
            assert!(
                result.get(tolerated).is_none(),
                "{tolerated} is volatile and must never be invented by the executor"
            );
        }
    }

    #[tokio::test]
    async fn a_request_without_shots_carries_the_captured_validation_message() {
        let golden: Value = serde_json::from_str(GOLDEN_VALIDATION).expect("the capture is JSON");
        let captured = golden["error"]["message"]
            .as_str()
            .expect("the captured envelope carries a message");
        assert_eq!(captured, "shots: Field required");

        let error = compile(&json!({}), &scratch("validation"))
            .await
            .expect_err("shots is required");
        assert!(error.to_string().contains(captured), "{error}");
    }

    #[tokio::test]
    async fn two_shots_compile_into_one_encoded_video() {
        let config = scratch("two-shots");
        colour_clip(&config.work_dir, "red.mp4", "red").await;
        colour_clip(&config.work_dir, "blue.mp4", "blue").await;

        let params = json!({
            "name": "film.mp4",
            "shots": [
                {"id": "s1", "seconds": 1.0, "video_url": "red.mp4"},
                {
                    "id": "s2",
                    "seconds": 1.0,
                    "video_url": "blue.mp4",
                    "clip_lead_seconds": 0.25,
                    "clip_out_seconds": 1.0
                }
            ]
        });
        let result = compile(&params, &config)
            .await
            .expect("the two shots compile");

        let output = config.out_dir.join("film.mp4");
        assert!(output.is_file(), "the encoded file lands in out_dir");
        assert_eq!(result["status"], json!("completed"));
        assert_eq!(result["output"], json!("film.mp4"));
        assert_eq!(result["duration_seconds"], json!(1.75));
        assert!(result["bytes"].as_u64().expect("bytes is a number") > 0);

        // Shot 1 contributes 1.0s; shot 2 reads [0.25, 1.0) of its source = 0.75s.
        let measured = measured_duration(&output).await;
        assert!(
            (measured - 1.75).abs() < 0.35,
            "the mux is {measured}s, expected ~1.75s"
        );
    }

    #[tokio::test]
    async fn a_footage_clip_keeps_its_audio_and_retimes_it() {
        let config = scratch("footage-audio");
        clip_with_audio(&config.work_dir, "talk.mp4").await;
        let params = json!({
            "name": "talk-out.mp4",
            "shots": [{"id": "s1", "seconds": 0.5, "speed": 2.0, "video_url": "talk.mp4"}]
        });
        let result = compile(&params, &config).await.expect("the clip compiles");
        assert_eq!(result["duration_seconds"], json!(0.5));

        let output = config.out_dir.join("talk-out.mp4");
        let measured = measured_duration(&output).await;
        assert!(
            (measured - 0.5).abs() < 0.15,
            "the retimed mux is {measured}s, expected ~0.5s"
        );
        let args = strings(&[
            "-v",
            "error",
            "-select_streams",
            "a",
            "-show_entries",
            "stream=codec_name",
            "-of",
            "csv=p=0",
            &output.display().to_string(),
        ]);
        let probe = process::run("ffprobe", &args).await.expect("ffprobe runs");
        assert!(probe.ok(), "{}", probe.tail());
        assert!(
            !probe.stdout.iter().all(|byte| byte.is_ascii_whitespace()),
            "the footage clip's own audio must survive the compile"
        );
    }

    #[tokio::test]
    async fn a_speed_retime_shortens_a_silent_segment() {
        let config = scratch("speed");
        colour_clip(&config.work_dir, "slow.mp4", "green").await;
        let params = json!({
            "name": "fast.mp4",
            "shots": [{"id": "s1", "seconds": 0.5, "speed": 2.0, "video_url": "slow.mp4"}]
        });
        let result = compile(&params, &config).await.expect("the clip compiles");
        assert_eq!(result["duration_seconds"], json!(0.5));
        let measured = measured_duration(&config.out_dir.join("fast.mp4")).await;
        assert!(
            (measured - 0.5).abs() < 0.15,
            "the retimed mux is {measured}s, expected ~0.5s"
        );
    }

    #[tokio::test]
    async fn the_captured_submit_request_compiles_without_optional_fields() {
        // Exactly the captured request's shots, plus only the executor's own output name.
        let config = scratch("golden-request");
        let params = json!({
            "name": "golden.mp4",
            "shots": [{"id": "s1", "seconds": 1}, {"id": "s2", "seconds": 1}]
        });
        let result = compile(&params, &config)
            .await
            .expect("the captured request compiles");

        assert!(config.out_dir.join("golden.mp4").is_file());
        assert_eq!(result["duration_seconds"], json!(2.0));
        assert_eq!(result["shot_timeline"], Value::Null);
    }

    macro_rules! refused_by_name {
        ($name:ident, $code:expr, $params:expr) => {
            #[tokio::test]
            async fn $name() {
                let config = scratch(stringify!($name));
                let error = compile(&$params, &config)
                    .await
                    .expect_err("the unsupported parameter must be refused");
                let rendered = error.to_string();
                assert!(rendered.contains($code), "expected {} in {rendered}", $code);
            }
        };
    }

    refused_by_name!(
        grading_is_refused_by_name,
        code::GRADING,
        json!({"shots": [{"id": "s1", "seconds": 1.0, "adjust": {"contrast": 1.2}}]})
    );
    refused_by_name!(
        captions_are_refused_by_name,
        code::CAPTIONS,
        json!({"shots": [{"id": "s1", "seconds": 1.0, "caption": "hello"}]})
    );
    refused_by_name!(
        ambient_bed_is_refused_by_name,
        code::AMBIENT_BED,
        json!({"shots": [{"id": "s1", "seconds": 1.0}], "music": {"input": "bed.mp3"}})
    );
    refused_by_name!(
        cursor_track_is_refused_by_name,
        code::CURSOR_TRACK,
        json!({"shots": [{"id": "s1", "seconds": 1.0, "cursor_track": [{"t": 0.0}]}]})
    );
    refused_by_name!(
        focus_track_is_refused_by_name,
        code::FOCUS_TRACK,
        json!({"shots": [{"id": "s1", "seconds": 1.0, "focus_track": [{"t": 0.0}]}]})
    );
    refused_by_name!(
        callouts_are_refused_by_name,
        code::CALLOUTS,
        json!({"shots": [{"id": "s1", "seconds": 1.0, "callouts": [{"label": "x"}]}]})
    );
    refused_by_name!(
        overlay_clips_are_refused_by_name,
        code::OVERLAY_CLIPS,
        json!({"shots": [{"id": "s1", "seconds": 1.0, "overlay_clips": [{}]}]})
    );
    refused_by_name!(
        text_overlays_are_refused_by_name,
        code::TEXT_OVERLAYS,
        json!({"shots": [{"id": "s1", "seconds": 1.0, "overlays": [{"text": "x"}]}]})
    );
    refused_by_name!(
        text_cards_are_refused_by_name,
        code::TEXT_CARD,
        json!({"shots": [{"id": "s1", "seconds": 1.0, "description": "a card"}]})
    );
    refused_by_name!(
        narration_is_refused_by_name,
        code::NARRATION,
        json!({"shots": [{"id": "s1", "seconds": 1.0, "audio_url": "voice.wav"}]})
    );
    refused_by_name!(
        shot_audio_is_refused_by_name,
        code::SHOT_AUDIO,
        json!({"shots": [{"id": "s1", "seconds": 1.0, "audio": {"gain_db": -3.0}}]})
    );
    refused_by_name!(
        sfx_are_refused_by_name,
        code::SFX,
        json!({"shots": [{"id": "s1", "seconds": 1.0, "sfx": [{"at": 0.0}]}]})
    );
    refused_by_name!(
        split_screen_is_refused_by_name,
        code::SPLIT_SCREEN,
        json!({"shots": [{"id": "s1", "seconds": 1.0, "split": {"mode": "hstack"}}]})
    );
    refused_by_name!(
        transitions_are_refused_by_name,
        code::TRANSITION,
        json!({"shots": [{"id": "s1", "seconds": 1.0, "transition": {"kind": "fade"}}]})
    );
    refused_by_name!(
        max_seconds_is_refused_by_name,
        code::MAX_SECONDS,
        json!({"shots": [{"id": "s1", "seconds": 1.0, "max_seconds": 2.0}]})
    );
    refused_by_name!(
        tags_are_refused_by_name,
        code::TAGS,
        json!({"shots": [{"id": "s1", "seconds": 1.0}], "tags": {"look": "warm"}})
    );
    refused_by_name!(
        clip_anchor_end_is_refused_by_name,
        code::CLIP_ANCHOR_END,
        json!({"shots": [{"id": "s1", "seconds": 1.0, "video_url": "unused.mp4", "clip_anchor": "end"}]})
    );
    refused_by_name!(
        fit_scroll_is_refused_by_name,
        code::FIT_SCROLL,
        json!({"shots": [{"id": "s1", "seconds": 1.0, "video_url": "unused.mp4", "fit": "scroll"}]})
    );
    refused_by_name!(
        retime_motion_is_refused_by_name,
        code::RETIME_MOTION,
        json!({"shots": [{"id": "s1", "seconds": 1.0, "video_url": "unused.mp4", "retime": "motion"}]})
    );
    refused_by_name!(
        retime_budget_is_refused_by_name,
        code::RETIME_BUDGET,
        json!({"shots": [{"id": "s1", "seconds": 1.0, "video_url": "unused.mp4", "retime": "budget"}]})
    );
}
