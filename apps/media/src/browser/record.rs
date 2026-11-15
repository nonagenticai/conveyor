//! `media.record_browser` — a browser page stepped through virtual time and encoded into a video.
//!
//! The reference (`POST /api/record/{content,url,part,screens}` in
//! `contract/video_production_service/README.md`) records a **live** Playwright context with
//! `record_video_dir`: one screenshot stream per wall-clock second, for as long as the session
//! runs. This port records by **virtual-time stepping** instead: it renders the page at successive
//! `--virtual-time-budget=<ms>` values, one PNG per frame, then encodes the sequence with ffmpeg at
//! the requested cadence. It needs no new dependency, costs no wall-clock duration, and the same
//! request always yields the same frame set.
//!
//! ## The declared deviation
//!
//! A page whose content depends on **real network** or **real wall-clock time** will differ from
//! the reference's recording; a deterministic page matches frame for frame. The reference's scroll
//! timeline has no counterpart here either — the clip's length is the request's duration, not the
//! time a scroll happened to take.
//!
//! ## Egress is opt-in
//!
//! A `url` naming an `http(s)` address is refused by name ([`code::EGRESS_DENIED`]) unless the
//! request carries `allow_network: true`. `html` content and a `file://` URL never need the
//! network. That is how P2.2's "no egress by default" and this kind coexist: the deployment decides
//! per kind whether to grant the flag.
//!
//! ## What this port serves
//!
//! | field | meaning |
//! |---|---|
//! | `html` / `html_content` | one inline self-contained document (exactly one page source) |
//! | `input` | a document file inside [`Config::work_dir`] |
//! | `url` | a `file://` document, or an `http(s)` URL with `allow_network: true` |
//! | `part_id` | the captured part identifier; also the output name (pattern `^[a-zA-Z0-9_\-\.]+$`) |
//! | `name` | the output base name (`name` wins when both are given) |
//! | `duration_ms` / `duration_seconds` | the clip's length (100–120000 ms; exactly one) |
//! | `fps` | 1–60; defaults to 25, the reference's own measured recording cadence |
//! | `width` / `height`, `recording_config.viewport_{width,height}` | the viewport (required, even pixels) |
//! | `recording_config.output_format` | `mp4` (the only container this kind encodes) |
//! | `recording_config.headless` | `true`/absent (the image has no display) |
//! | `recording_config.color_scheme` | `light`/absent (the browser default) |
//!
//! Duration and viewport carry no default: the capture pins none (the one measured recording's
//! 1.8 s came from its scroll config, and its 320×240 viewport was passed explicitly), so
//! `duration_ms`/`duration_seconds`, `width` and `height` are required. `fps` defaults to 25,
//! which the capture does pin: the measured reference recording is "320×240, 1.8 s @ 25 fps" with
//! no rate in the request. Every other captured field — the scroll
//! config, scripted-part `scripts_dir`/`auth_config`, screenshot `steps`/`record_clips`, URL
//! `auth_info`, `webhook_url`, and the recording-config knobs this port cannot honour (capture
//! scale, interaction steps, retries, slow-mo, quality presets, post-load delay, `timeout_ms`,
//! `wait_for_selector`) — is **refused by a stable code**, never accepted and silently ignored.
//!
//! The output is `config.out_dir/<name>.mp4`; the result document is
//! `{"output","bytes","sha256","width","height","fps","duration_seconds","frames","capture"}` with
//! `capture` naming the mechanism (`virtual_time`). `duration_seconds` is the achieved length
//! (`frames / fps`), which is the requested duration within half a frame.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use crate::common::{CapabilityError, Config, process};

/// The kind slug this module implements.
pub const KIND: &str = "media.record_browser";

/// The stable refusal codes. Each names one captured field or branch this port does not serve, so
/// a caller can branch on the code instead of reading a sentence.
pub mod code {
    pub const EGRESS_DENIED: &str = "egress_denied";
    pub const SCRIPTS_DIR: &str = "scripts_dir_not_mounted";
    pub const SCREENS: &str = "screenshot_steps_not_supported";
    pub const AUTH: &str = "auth_not_supported";
    pub const WEBHOOK: &str = "webhook_not_supported";
    pub const SCROLL: &str = "scroll_not_supported";
    pub const OUTPUT_FORMAT: &str = "output_format_not_supported";
    pub const HEADFUL: &str = "headful_not_supported";
    pub const CAPTURE_SCALE: &str = "capture_scale_not_supported";
    pub const COLOUR_SCHEME: &str = "color_scheme_not_supported";
    pub const INTERACTION: &str = "interaction_steps_not_supported";
    pub const RETRIES: &str = "max_retries_not_supported";
    pub const POST_LOAD_DELAY: &str = "post_load_delay_not_supported";
    pub const QUALITY: &str = "quality_preset_not_supported";
    pub const SLOW_MO: &str = "slow_mo_not_supported";
    pub const TIMEOUT: &str = "timeout_ms_not_supported";
    pub const WAIT_SELECTOR: &str = "wait_for_selector_not_supported";
    pub const BROWSER: &str = "browser_not_available";
}

/// The browser binary. The image installs Chromium at `/usr/bin/chromium`; a developer box points
/// this at whatever Chrome it has.
const CHROME_ENV: &str = "CHROME_BIN";
const CHROME_DEFAULT: &str = "/usr/bin/chromium";
/// The developer fallback: a stock Chrome installation under the system Applications directory.
const CHROME_DEVELOPER: &str = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";

/// The reference's measured recording cadence (README: "320×240, 1.8 s @ 25 fps"). The capture
/// pins no other default, so duration and viewport must come from the request.
const DEFAULT_FPS: u32 = 25;

/// The captured ranges: `RecordAnimationRequest.duration_ms` / `.fps`, and
/// `RecordingConfigInput.viewport_{width,height}`.
const DURATION_MIN_MS: u64 = 100;
const DURATION_MAX_MS: u64 = 120_000;
const MIN_FPS: u32 = 1;
const MAX_FPS: u32 = 60;
const WIDTH_MIN: u32 = 320;
const WIDTH_MAX: u32 = 7680;
const HEIGHT_MIN: u32 = 240;
const HEIGHT_MAX: u32 = 4320;

/// The output file name when the request names no part.
const DEFAULT_NAME: &str = "recording";

/// Record one page into a video under `config.out_dir`.
pub async fn record(params: &Value, config: &Config) -> Result<Value, CapabilityError> {
    let plan = Plan::parse(params, &config.work_dir)?;
    let chrome = chrome_binary().ok_or_else(|| {
        CapabilityError::refused(
            code::BROWSER,
            "no Chromium or Chrome binary was found; set CHROME_BIN",
        )
    })?;
    let chrome = chrome.to_string_lossy().into_owned();

    std::fs::create_dir_all(&config.work_dir).map_err(|error| {
        CapabilityError::Failed(format!("cannot create {:?}: {error}", config.work_dir))
    })?;
    std::fs::create_dir_all(&config.out_dir).map_err(|error| {
        CapabilityError::Failed(format!("cannot create {:?}: {error}", config.out_dir))
    })?;

    // The page argument. Inline HTML is written into the job's own work directory, so the
    // `file://` load can only reach a document the job itself supplied.
    let page = match &plan.page {
        Page::Html(html) => {
            let path = config.work_dir.join(format!("{}.html", plan.stem));
            std::fs::write(&path, html).map_err(|error| {
                CapabilityError::Failed(format!("cannot write {path:?}: {error}"))
            })?;
            format!("file://{}", path.display())
        }
        Page::File(path) => format!("file://{}", path.display()),
        Page::Remote(url) => url.clone(),
    };

    // One PNG per frame, all sized by the same viewport so the encoder needs no scaling.
    let frames_dir = config.work_dir.join(format!("{}-frames", plan.stem));
    std::fs::create_dir_all(&frames_dir).map_err(|error| {
        CapabilityError::Failed(format!("cannot create {frames_dir:?}: {error}"))
    })?;
    let profile = config.work_dir.join(format!("{}.profile", plan.stem));
    for index in 0..plan.frames {
        let frame = frames_dir.join(format!("frame-{index:05}.png"));
        let args = chrome_args(
            &profile,
            plan.width,
            plan.height,
            budget_ms(index, plan.fps),
            &frame,
            &page,
        );
        let finished = process::run(&chrome, &args).await?;
        if !finished.ok() {
            return Err(CapabilityError::Failed(format!(
                "the browser failed on frame {index}: {}",
                finished.tail()
            )));
        }
        // A browser that exits 0 without a screenshot would otherwise become a silently empty
        // recording; every frame is required.
        let written = std::fs::metadata(&frame)
            .map(|meta| meta.len())
            .unwrap_or(0);
        if written == 0 {
            return Err(CapabilityError::Failed(format!(
                "the browser reported success but wrote no frame at {frame:?}"
            )));
        }
    }

    let pattern = frames_dir.join("frame-%05d.png");
    let output = config.out_dir.join(&plan.file_name);
    let fps = plan.fps.to_string();
    let pattern_text = pattern.display().to_string();
    let output_text = output.display().to_string();
    let args: Vec<String> = [
        "-y",
        "-hide_banner",
        "-loglevel",
        "error",
        "-framerate",
        &fps,
        "-i",
        &pattern_text,
        "-c:v",
        "libx264",
        "-preset",
        "veryfast",
        "-crf",
        "23",
        "-pix_fmt",
        "yuv420p",
        "-r",
        &fps,
        "-movflags",
        "+faststart",
        &output_text,
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    let finished = process::run("ffmpeg", &args).await?;
    if !finished.ok() {
        return Err(CapabilityError::Failed(format!(
            "ffmpeg could not encode the recorded frames: {}",
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

    Ok(json!({
        "output": plan.file_name,
        "bytes": bytes.len(),
        "sha256": hex(&Sha256::digest(&bytes)),
        "width": plan.width,
        "height": plan.height,
        "fps": plan.fps,
        "duration_seconds": round_seconds(plan.frames as f64 / f64::from(plan.fps)),
        "frames": plan.frames,
        "capture": "virtual_time",
    }))
}

/// One parsed recording request, reduced to what this port honours.
struct Plan {
    page: Page,
    /// The bare output file name (`<base>.mp4`).
    file_name: String,
    /// The output base name without the extension; also names the frame and profile directories.
    stem: String,
    width: u32,
    height: u32,
    fps: u32,
    /// The frames the request asks for (`duration × fps`, at least one).
    frames: u64,
}

/// The document the browser loads.
enum Page {
    /// Caller-supplied HTML, written into the work directory before the first frame.
    Html(String),
    /// A document file the job was given.
    File(PathBuf),
    /// An `http(s)` URL the request explicitly opted into reaching.
    Remote(String),
}

impl Plan {
    fn parse(params: &Value, work_dir: &Path) -> Result<Self, CapabilityError> {
        let map = params
            .as_object()
            .ok_or_else(|| CapabilityError::Invalid("params must be a JSON object".to_string()))?;

        // The refusal pass runs before any source is resolved, so an unsupported captured field is
        // named even when the page it accompanies is missing. `null` (and a value the reference
        // itself treats as absent) is the captured default and therefore a no-op.
        for (key, value) in map {
            match key.as_str() {
                "html" | "html_content" | "input" | "url" | "part_id" | "name" | "duration_ms"
                | "duration_seconds" | "fps" | "width" | "height" | "allow_network"
                | "recording_config" => {}
                "scroll_config" => scroll_refusal(value)?,
                "output_format" => match value {
                    Value::Null => {}
                    Value::String(format) if format == "mp4" => {}
                    Value::String(other) => {
                        return Err(refuse(
                            code::OUTPUT_FORMAT,
                            format!(
                                "output_format={other:?} is not implemented; mp4 is the only \
                                 container this kind encodes"
                            ),
                        ));
                    }
                    _ => {
                        return Err(CapabilityError::Invalid(
                            "output_format must be a string".to_string(),
                        ));
                    }
                },
                "quality_preset" => refuse_when_present(value, code::QUALITY, "quality_preset")?,
                "webhook_url" => refuse_when_present(value, code::WEBHOOK, "webhook_url")?,
                "auth_info" | "auth_config" => {
                    no_op_or_refuse(value, code::AUTH, key, is_empty_object)?;
                }
                "scripts_dir" => refuse_when_present(value, code::SCRIPTS_DIR, "scripts_dir")?,
                "part_type" => match value {
                    Value::Null => {}
                    Value::String(part_type) if part_type == "browser_recording" => {}
                    Value::String(other) => {
                        return Err(refuse(
                            code::SCRIPTS_DIR,
                            format!(
                                "part_type={other:?} is a scripted part; the executor was not \
                                 given a scripts directory"
                            ),
                        ));
                    }
                    _ => {
                        return Err(CapabilityError::Invalid(
                            "part_type must be a string".to_string(),
                        ));
                    }
                },
                "steps" => no_op_or_refuse(value, code::SCREENS, "steps", is_empty_list)?,
                "record_clips" => match value {
                    Value::Null | Value::Bool(false) => {}
                    Value::Bool(true) => {
                        return Err(refuse(
                            code::SCREENS,
                            "record_clips is a screenshot-capture mode, not a video recording",
                        ));
                    }
                    _ => {
                        return Err(CapabilityError::Invalid(
                            "record_clips must be a boolean".to_string(),
                        ));
                    }
                },
                "clip_dwell_ms" => {
                    refuse_when_present(value, code::SCREENS, "clip_dwell_ms")?;
                }
                other => {
                    return Err(CapabilityError::Invalid(format!(
                        "unknown parameter {other:?}"
                    )));
                }
            }
        }

        let allow_network = match map.get("allow_network").filter(|value| !value.is_null()) {
            None => false,
            Some(value) => value.as_bool().ok_or_else(|| {
                CapabilityError::Invalid("allow_network must be a boolean".to_string())
            })?,
        };

        // The recording-config block is the second half of the refusal pass; validating it here
        // keeps an unsupported knob reported even when the page source is missing.
        let recording = recording_config(map)?;
        let page = page_source(map, work_dir, allow_network)?;
        let (width, height) = viewport(map, recording)?;
        let duration = duration_ms(map)?;
        let fps = fps(map)?;
        let frames = ((duration as f64 / 1000.0) * f64::from(fps))
            .round()
            .max(1.0) as u64;
        let (file_name, stem) = output_name(map)?;

        Ok(Self {
            page,
            file_name,
            stem,
            width,
            height,
            fps,
            frames,
        })
    }
}

/// Resolve the one page source the request names, refusing an un-granted remote URL by name.
fn page_source(
    map: &Map<String, Value>,
    work_dir: &Path,
    allow_network: bool,
) -> Result<Page, CapabilityError> {
    let html = string_field(map, "html")?;
    let html_content = string_field(map, "html_content")?;
    let input = string_field(map, "input")?;
    let url = string_field(map, "url")?;

    let present = [
        html.is_some(),
        html_content.is_some(),
        input.is_some(),
        url.is_some(),
    ]
    .into_iter()
    .filter(|present| *present)
    .count();
    if present == 0 {
        return Err(CapabilityError::Invalid(
            "one of html, html_content, input or url is required".to_string(),
        ));
    }
    if present > 1 {
        return Err(CapabilityError::Invalid(
            "pass exactly one of html, html_content, input or url".to_string(),
        ));
    }

    // The captured content request's own constraint (`html_content: minLength 1`). A blank page
    // would record a blank video and turn that refusal into a silent success.
    let inline = html.or(html_content);
    if let Some(document) = inline {
        if document.is_empty() {
            return Err(CapabilityError::Invalid(
                "html must have at least 1 character".to_string(),
            ));
        }
        return Ok(Page::Html(document.to_string()));
    }

    if let Some(name) = input {
        return Ok(Page::File(process::resolve_input(work_dir, name)?));
    }

    let url = url.expect("exactly one page source is present");
    if let Some(path) = url.strip_prefix("file://") {
        if path.is_empty() {
            return Err(CapabilityError::Invalid(
                "url must name a file after file://".to_string(),
            ));
        }
        // A file URL is joined to the job's work directory exactly like a named input, so it
        // cannot reach a path the job was not given.
        return Ok(Page::File(process::resolve_input(work_dir, path)?));
    }
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(CapabilityError::Invalid(
            "url must start with 'http://', 'https://' or 'file://'".to_string(),
        ));
    }
    if !allow_network {
        // P2.2: a media job has no egress unless the deployment granted it for this kind. A URL
        // left to fail at Chromium with a DNS error would read like a transient fault.
        return Err(refuse(
            code::EGRESS_DENIED,
            format!(
                "url {url:?} needs network egress; pass allow_network: true only when the \
                 deployment grants this kind egress"
            ),
        ));
    }
    Ok(Page::Remote(url.to_string()))
}

/// The recorded viewport: `width`/`height`, or `recording_config.viewport_{width,height}`. Both
/// spellings are accepted, but they must agree, and one of them is required — the capture pins no
/// default viewport.
fn viewport(
    map: &Map<String, Value>,
    recording: Option<&Map<String, Value>>,
) -> Result<(u32, u32), CapabilityError> {
    let nested = |key: &str| recording.and_then(|config| config.get(key));
    let top = |key: &str| map.get(key).filter(|value| !value.is_null());
    Ok((
        pixel(
            top("width"),
            nested("viewport_width"),
            "width",
            WIDTH_MIN,
            WIDTH_MAX,
        )?,
        pixel(
            top("height"),
            nested("viewport_height"),
            "height",
            HEIGHT_MIN,
            HEIGHT_MAX,
        )?,
    ))
}

/// Validate the recording-config block, refusing every captured knob this port does not honour.
fn recording_config(
    map: &Map<String, Value>,
) -> Result<Option<&Map<String, Value>>, CapabilityError> {
    let Some(value) = map.get("recording_config").filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    let config = value.as_object().ok_or_else(|| {
        CapabilityError::Invalid("recording_config must be an object".to_string())
    })?;
    for (key, value) in config {
        match key.as_str() {
            "viewport_width" | "viewport_height" => {}
            "output_format" => match value.as_str() {
                None if value.is_null() => {}
                Some("mp4") => {}
                Some("webm") => {
                    return Err(refuse(
                        code::OUTPUT_FORMAT,
                        "output_format=webm is not implemented; mp4 is the only container this \
                         kind encodes",
                    ));
                }
                Some(other) => {
                    return Err(CapabilityError::Invalid(format!(
                        "output_format must be \"mp4\" or \"webm\", not {other:?}"
                    )));
                }
                None => {
                    return Err(CapabilityError::Invalid(
                        "output_format must be a string".to_string(),
                    ));
                }
            },
            "headless" => match value.as_bool() {
                None if value.is_null() => {}
                Some(true) => {}
                Some(false) => {
                    return Err(refuse(
                        code::HEADFUL,
                        "headless=false is not implemented; the image has no display",
                    ));
                }
                None => {
                    return Err(CapabilityError::Invalid(
                        "headless must be a boolean".to_string(),
                    ));
                }
            },
            "color_scheme" => match value.as_str() {
                None if value.is_null() => {}
                Some("light") => {}
                Some("dark") => {
                    return Err(refuse(
                        code::COLOUR_SCHEME,
                        "color_scheme=dark is not implemented; light is the browser default",
                    ));
                }
                Some(other) => {
                    return Err(CapabilityError::Invalid(format!(
                        "color_scheme must be \"light\" or \"dark\", not {other:?}"
                    )));
                }
                None => {
                    return Err(CapabilityError::Invalid(
                        "color_scheme must be a string".to_string(),
                    ));
                }
            },
            "interaction_steps" => {
                no_op_or_refuse(value, code::INTERACTION, key, is_empty_list)?;
            }
            "capture_scale" => refuse_when_present(value, code::CAPTURE_SCALE, key)?,
            "max_retries" => refuse_when_present(value, code::RETRIES, key)?,
            "post_load_delay_ms" => refuse_when_present(value, code::POST_LOAD_DELAY, key)?,
            "quality_preset" => refuse_when_present(value, code::QUALITY, key)?,
            "slow_mo_ms" => refuse_when_present(value, code::SLOW_MO, key)?,
            "timeout_ms" => refuse_when_present(value, code::TIMEOUT, key)?,
            "wait_for_selector" => refuse_when_present(value, code::WAIT_SELECTOR, key)?,
            other => {
                return Err(CapabilityError::Invalid(format!(
                    "unknown parameter \"recording_config.{other}\""
                )));
            }
        }
    }
    Ok(Some(config))
}

/// Accept an auto-scroll block only when it asks for no scrolling; a scrolling session defines a
/// wall-clock timeline this port's duration-driven recording cannot reproduce.
fn scroll_refusal(value: &Value) -> Result<(), CapabilityError> {
    if value.is_null() {
        return Ok(());
    }
    let config = value
        .as_object()
        .ok_or_else(|| CapabilityError::Invalid("scroll_config must be an object".to_string()))?;
    for key in config.keys() {
        if !matches!(
            key.as_str(),
            "enabled" | "speed_px_per_second" | "start_delay_seconds" | "end_delay_seconds"
        ) {
            return Err(CapabilityError::Invalid(format!(
                "unknown parameter \"scroll_config.{key}\""
            )));
        }
    }
    // `enabled` defaults to true in the captured schema, so an absent or null flag still asks for
    // a scrolling session.
    match config.get("enabled") {
        Some(Value::Bool(false)) => Ok(()),
        Some(Value::Bool(true)) | Some(Value::Null) | None => Err(refuse(
            code::SCROLL,
            "scroll_config is not implemented; record a duration with the page already at rest",
        )),
        Some(_) => Err(CapabilityError::Invalid(
            "scroll_config.enabled must be a boolean".to_string(),
        )),
    }
}

fn duration_ms(map: &Map<String, Value>) -> Result<u64, CapabilityError> {
    let millis = match (
        map.get("duration_ms").filter(|value| !value.is_null()),
        map.get("duration_seconds").filter(|value| !value.is_null()),
    ) {
        (Some(_), Some(_)) => {
            return Err(CapabilityError::Invalid(
                "pass exactly one of duration_ms or duration_seconds".to_string(),
            ));
        }
        (Some(value), None) => value.as_u64().ok_or_else(|| {
            CapabilityError::Invalid("duration_ms must be an integer".to_string())
        })?,
        (None, Some(value)) => {
            let seconds = value.as_f64().ok_or_else(|| {
                CapabilityError::Invalid("duration_seconds must be a number".to_string())
            })?;
            if !seconds.is_finite() || seconds <= 0.0 {
                return Err(CapabilityError::Invalid(
                    "duration_seconds must be a positive number".to_string(),
                ));
            }
            // `as u64` saturates, so an out-of-range value lands on the range check below.
            (seconds * 1000.0).round() as u64
        }
        (None, None) => {
            return Err(CapabilityError::Invalid(
                "one of duration_ms or duration_seconds is required".to_string(),
            ));
        }
    };
    if !(DURATION_MIN_MS..=DURATION_MAX_MS).contains(&millis) {
        return Err(CapabilityError::Invalid(format!(
            "duration must be between {DURATION_MIN_MS} and {DURATION_MAX_MS} milliseconds"
        )));
    }
    Ok(millis)
}

fn fps(map: &Map<String, Value>) -> Result<u32, CapabilityError> {
    let Some(value) = map.get("fps").filter(|value| !value.is_null()) else {
        return Ok(DEFAULT_FPS);
    };
    let raw = value
        .as_u64()
        .ok_or_else(|| CapabilityError::Invalid("fps must be an integer".to_string()))?;
    let fps = u32::try_from(raw)
        .map_err(|_| CapabilityError::Invalid("fps is out of range".to_string()))?;
    if !(MIN_FPS..=MAX_FPS).contains(&fps) {
        return Err(CapabilityError::Invalid(format!(
            "fps must be between {MIN_FPS} and {MAX_FPS}"
        )));
    }
    Ok(fps)
}

/// One viewport dimension from either spelling. H.264 4:2:0 needs even pixels, the captured
/// viewport ranges are enforced rather than silently clamped, and a missing dimension is refused
/// because the capture pins no default.
fn pixel(
    top: Option<&Value>,
    nested: Option<&Value>,
    key: &str,
    min: u32,
    max: u32,
) -> Result<u32, CapabilityError> {
    let read = |value: &Value, field: &str| -> Result<u32, CapabilityError> {
        let raw = value
            .as_u64()
            .ok_or_else(|| CapabilityError::Invalid(format!("{field} must be an integer")))?;
        let pixels = u32::try_from(raw)
            .map_err(|_| CapabilityError::Invalid(format!("{field} is out of range")))?;
        if !pixels.is_multiple_of(2) {
            return Err(CapabilityError::Invalid(format!(
                "{field} must be an even integer for H.264 4:2:0"
            )));
        }
        if !(min..=max).contains(&pixels) {
            return Err(CapabilityError::Invalid(format!(
                "{field} must be between {min} and {max}"
            )));
        }
        Ok(pixels)
    };
    let direct_value = top.map(|value| read(value, key)).transpose()?;
    let nested_value = nested
        .map(|value| read(value, &format!("recording_config.viewport_{key}")))
        .transpose()?;
    match (direct_value, nested_value) {
        (Some(a), Some(b)) if a != b => Err(CapabilityError::Invalid(format!(
            "{key} and recording_config.viewport_{key} disagree"
        ))),
        (Some(pixels), _) | (_, Some(pixels)) => Ok(pixels),
        (None, None) => Err(CapabilityError::Invalid(format!(
            "{key}: Field required (the capture pins no default viewport; pass {key} or \
             recording_config.viewport_{key})"
        ))),
    }
}

fn output_name(map: &Map<String, Value>) -> Result<(String, String), CapabilityError> {
    let part_id = match map.get("part_id").filter(|value| !value.is_null()) {
        None => None,
        Some(value) => {
            let part_id = value
                .as_str()
                .ok_or_else(|| CapabilityError::Invalid("part_id must be a string".to_string()))?;
            let legal = !part_id.is_empty()
                && part_id.len() <= 128
                && part_id.chars().all(|character| {
                    character.is_ascii_alphanumeric() || "_-.".contains(character)
                });
            if !legal {
                return Err(CapabilityError::Invalid(
                    "part_id must match ^[a-zA-Z0-9_\\-\\.]+$ and be at most 128 characters"
                        .to_string(),
                ));
            }
            Some(part_id.to_string())
        }
    };
    let name = match map.get("name").filter(|value| !value.is_null()) {
        None => None,
        Some(value) => {
            let name = value
                .as_str()
                .ok_or_else(|| CapabilityError::Invalid("name must be a string".to_string()))?;
            // A bare file name, like the compile port: no path may escape `out_dir`, and `%` is
            // reserved for the frame-sequence pattern.
            if name.is_empty()
                || name.contains('%')
                || Path::new(name).file_name().and_then(|part| part.to_str()) != Some(name)
            {
                return Err(CapabilityError::Invalid(
                    "name must be a bare file name, not a path".to_string(),
                ));
            }
            Some(name.to_string())
        }
    };
    let base = name.or(part_id).unwrap_or_else(|| DEFAULT_NAME.to_string());
    let file_name = if base.to_ascii_lowercase().ends_with(".mp4") {
        base
    } else {
        format!("{base}.mp4")
    };
    let stem = Path::new(&file_name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(DEFAULT_NAME)
        .to_string();
    Ok((file_name, stem))
}

/// Frame `index` is the page at `index / fps` seconds. A zero budget never expires — it waits for
/// a clock that does not advance (the trap `render.rs` records, measured 2026-11-15) — so the
/// first frame samples at 1 ms.
fn budget_ms(index: u64, fps: u32) -> u64 {
    (index as f64 * 1000.0 / f64::from(fps)).round().max(1.0) as u64
}

fn chrome_args(
    profile: &Path,
    width: u32,
    height: u32,
    budget_ms: u64,
    screenshot: &Path,
    page: &str,
) -> Vec<String> {
    vec![
        "--headless=new".to_string(),
        "--disable-gpu".to_string(),
        "--no-sandbox".to_string(),
        "--no-first-run".to_string(),
        "--disable-extensions".to_string(),
        "--hide-scrollbars".to_string(),
        "--disable-dev-shm-usage".to_string(),
        // A virtual-time frame must not capture a half-composited page, and the encoded pixels
        // must be exactly the requested viewport on every host.
        "--run-all-compositor-stages-before-draw".to_string(),
        "--force-device-scale-factor=1".to_string(),
        format!("--virtual-time-budget={budget_ms}"),
        format!("--user-data-dir={}", profile.display()),
        format!("--window-size={width},{height}"),
        format!("--screenshot={}", screenshot.display()),
        page.to_string(),
    ]
}

/// The browser this process drives: `CHROME_BIN`, then the image's Chromium, then a developer
/// box's stock Chrome. `None` is the "no browser is installed" refusal.
fn chrome_binary() -> Option<PathBuf> {
    if let Some(configured) = std::env::var_os(CHROME_ENV) {
        return Some(PathBuf::from(configured));
    }
    [CHROME_DEFAULT, CHROME_DEVELOPER]
        .into_iter()
        .map(PathBuf::from)
        .find(|candidate| candidate.is_file())
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

fn round_seconds(seconds: f64) -> f64 {
    (seconds * 1000.0).round() / 1000.0
}

fn refuse(code: &str, message: impl Into<String>) -> CapabilityError {
    CapabilityError::refused(code, message)
}

/// Refuse a captured field the moment it carries a value; `null` is the captured default.
fn refuse_when_present(value: &Value, code: &str, field: &str) -> Result<(), CapabilityError> {
    if value.is_null() {
        Ok(())
    } else {
        Err(refuse(code, format!("{field} is not implemented")))
    }
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

fn string_field<'a>(
    map: &'a Map<String, Value>,
    key: &str,
) -> Result<Option<&'a str>, CapabilityError> {
    match map.get(key).filter(|value| !value.is_null()) {
        None => Ok(None),
        Some(value) => value
            .as_str()
            .map(Some)
            .ok_or_else(|| CapabilityError::Invalid(format!("{key} must be a string"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// A fresh work/out pair under the system temp directory, unique to one test invocation.
    fn scratch(tag: &str) -> Config {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or_default();
        let base = std::env::temp_dir().join(format!(
            "conveyor-media-record-{tag}-{}-{nanos}",
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

    /// A self-contained two-frame animation: navy until 200 ms, then red. The recorded clip steps
    /// from one to the other, so the encoded frames must differ.
    const TWO_FRAME_ANIMATION: &str = r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><title>Synthetic two-frame animation</title>
<style>html, body { margin: 0; height: 100%; background: #101c2c; }</style></head>
<body><script>
  var start = performance.now();
  function tick() {
    var t = performance.now() - start;
    document.body.style.background = t > 200 ? '#ff0000' : '#101c2c';
    requestAnimationFrame(tick);
  }
  requestAnimationFrame(tick);
</script></body></html>
"#;

    /// The encoded file's real duration, read back with ffprobe.
    async fn measured_duration(path: &Path) -> f64 {
        let args: Vec<String> = [
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            &path.display().to_string(),
        ]
        .into_iter()
        .map(str::to_string)
        .collect();
        let finished = process::run("ffprobe", &args).await.expect("ffprobe runs");
        assert!(finished.ok(), "probing {path:?}: {}", finished.tail());
        String::from_utf8_lossy(&finished.stdout)
            .trim()
            .parse()
            .expect("ffprobe reports a duration")
    }

    /// The decoded background colour of every encoded frame, in order. A recording that captured
    /// only one moment returns the same triple for every frame.
    async fn frame_colours(path: &Path, frames: usize) -> Vec<Vec<u8>> {
        let args: Vec<String> = [
            "-v",
            "error",
            "-i",
            &path.display().to_string(),
            // `2:2` before the RGB conversion: a 4:2:0 frame cannot crop to an odd size.
            "-vf",
            "crop=2:2:2:2",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-",
        ]
        .into_iter()
        .map(str::to_string)
        .collect();
        let finished = process::run("ffmpeg", &args).await.expect("ffmpeg runs");
        assert!(finished.ok(), "decoding {path:?}: {}", finished.tail());
        assert_eq!(
            finished.stdout.len(),
            frames * 12,
            "every frame is a 2x2 rgb24 block (twelve bytes)"
        );
        finished
            .stdout
            .as_chunks::<12>()
            .0
            .iter()
            .map(|block| block[..3].to_vec())
            .collect()
    }

    #[tokio::test]
    async fn a_two_frame_animation_is_recorded_at_the_requested_cadence() {
        // The only Chromium-dependent test gate: a box without a browser cannot run this, and a
        // silent pass is better than a red test over a missing third-party binary.
        if chrome_binary().is_none() {
            return;
        }
        let config = scratch("animation");
        // 500 ms at 4 fps is exactly two frames; one frame is 250 ms.
        let params = json!({
            "html": TWO_FRAME_ANIMATION,
            "part_id": "anim001",
            "duration_ms": 500,
            "fps": 4,
            "width": 320,
            "height": 240,
        });
        let result = record(&params, &config)
            .await
            .expect("the recording completes");

        assert_eq!(result["frames"], json!(2));
        assert_eq!(result["capture"], json!("virtual_time"));
        assert_eq!(result["width"], json!(320));
        assert_eq!(result["height"], json!(240));
        assert_eq!(result["fps"], json!(4));
        assert_eq!(result["duration_seconds"], json!(0.5));
        assert_eq!(result["output"], json!("anim001.mp4"));
        assert!(result["bytes"].as_u64().expect("bytes is a number") > 0);

        let output = config.out_dir.join("anim001.mp4");
        assert!(output.is_file(), "the mp4 must exist at {output:?}");
        assert_eq!(
            std::fs::read_dir(config.work_dir.join("anim001-frames"))
                .expect("the frame directory exists")
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name().to_string_lossy().ends_with(".png"))
                .count(),
            2,
            "frames must equal what was rendered"
        );

        // ffprobe must report the requested duration within one frame (250 ms at 4 fps).
        let measured = measured_duration(&output).await;
        assert!(
            (measured - 0.5).abs() <= 0.25 + 1e-6,
            "recorded {measured:.3}s, requested 0.500s at 4 fps"
        );

        // The animation advanced: the first frame is the navy page, the second the red one.
        let colours = frame_colours(&output, 2).await;
        assert_ne!(
            colours[0], colours[1],
            "both encoded frames show the same picture"
        );
        assert!(
            colours[1][0] > colours[0][0] + 50,
            "the second frame must be the red stage, got {:?} after {:?}",
            colours[1],
            colours[0]
        );
    }

    #[tokio::test]
    async fn a_local_file_url_is_recorded_without_allow_network() {
        if chrome_binary().is_none() {
            return;
        }
        let config = scratch("file-url");
        std::fs::write(
            config.work_dir.join("page.html"),
            "<!doctype html><body style=\"margin:0;background:#00ff00\"></body>",
        )
        .expect("write the fixture page");
        let params = json!({
            "url": "file://page.html",
            "part_id": "local001",
            "duration_ms": 1000,
            "fps": 1,
            "width": 320,
            "height": 240,
        });
        let result = record(&params, &config)
            .await
            .expect("a local file URL never needs egress");
        assert_eq!(result["frames"], json!(1));
        assert!(config.out_dir.join("local001.mp4").is_file());
    }

    #[tokio::test]
    async fn a_remote_url_without_allow_network_is_refused_by_name() {
        let params = json!({
            "url": "https://example.com/",
            "part_id": "remote001",
            "duration_ms": 1000,
            "fps": 25,
        });
        let error = record(&params, &scratch("egress")).await.unwrap_err();
        assert!(error.to_string().contains("egress_denied"), "{error}");
    }

    #[tokio::test]
    async fn a_request_without_a_page_source_is_refused() {
        let params = json!({"part_id": "empty001", "duration_ms": 1000});
        let error = record(&params, &scratch("no-source")).await.unwrap_err();
        assert!(
            error
                .to_string()
                .contains("one of html, html_content, input or url is required"),
            "{error}"
        );
    }

    #[tokio::test]
    async fn the_scripted_part_shape_is_refused_by_name() {
        // `RecordPartRequest` carries a required `scripts_dir`; the executor is not given one, so
        // a path in the request cannot be mounted.
        let params = json!({
            "part_id": "shot001",
            "scripts_dir": "scripts",
            "duration_ms": 1000,
        });
        let error = record(&params, &scratch("scripts")).await.unwrap_err();
        assert!(
            error.to_string().contains("scripts_dir_not_mounted"),
            "{error}"
        );
    }

    #[tokio::test]
    async fn an_ignored_recording_config_field_is_refused_by_name() {
        // A scrolling session defines a wall-clock timeline the duration-driven port cannot
        // reproduce; accepting the block and ignoring it would be a silent lie.
        let params = json!({
            "html": "<p>synthetic</p>",
            "duration_ms": 1000,
            "scroll_config": {"enabled": true, "speed_px_per_second": 5000},
        });
        let error = record(&params, &scratch("scroll")).await.unwrap_err();
        assert!(
            error.to_string().contains("scroll_not_supported"),
            "{error}"
        );

        let params = json!({
            "html": "<p>synthetic</p>",
            "duration_ms": 1000,
            "recording_config": {"output_format": "webm"},
        });
        let error = record(&params, &scratch("webm")).await.unwrap_err();
        assert!(
            error.to_string().contains("output_format_not_supported"),
            "{error}"
        );
    }
}
