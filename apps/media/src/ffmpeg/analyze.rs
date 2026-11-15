//! `media.analyze_video` and `media.analyze_audio` — the reference's own measurements.
//!
//! The captured `video_analyzer_service` document mixes four kinds of work:
//!
//! * ffprobe/ffmpeg metadata (duration, dimensions, frame rate, stream codecs),
//! * statistics ffmpeg itself produces (mean luma through `signalstats`, scene scores through
//!   `scdet`),
//! * arithmetic the reference does over decoded frames with OpenCV (Laplacian-variance
//!   sharpness, frame abs-difference motion, k-means dominant colours, HSV histogram scene
//!   boundaries),
//! * licence-gated external work (vision-provider frame descriptions, ASR transcription,
//!   imagehash verification).
//!
//! This port serves the first two. The rest is **refused by name**: a request that would need a
//! capability this crate does not have is a typed [`CapabilityError::refused`] with a stable code
//! naming the exact field, never a silently missing answer and never an approximation. Unknown
//! parameters are refused too, because the schema is closed.
//!
//! `media.analyze_audio` is the same discipline applied to the captured beat-grid route. Its
//! `duration_s` is the reference's own definition — decoded samples at 22050 Hz — so it is exact
//! for the same input. The grid itself (`bpm`, `bpm_confidence`, `beats`, `onsets`, `downbeats`,
//! `grid_source`) is librosa's median-aggregated mel onset envelope plus a DP beat tracker; this
//! crate links no audio DSP, so a caller asking for it with `beat_grid: true` is told exactly
//! which fields cannot be produced rather than handed a fabricated grid.

use std::path::Path;

use serde_json::{Value, json};

use crate::common::{CapabilityError, Config, process};

/// The video-analysis kind.
pub const KIND_VIDEO: &str = "media.analyze_video";

/// The audio-analysis kind.
pub const KIND_AUDIO: &str = "media.analyze_audio";

/// The reference's configured frame interval (`PIPELINE_FRAME_INTERVAL_SECONDS`).
const DEFAULT_FRAME_INTERVAL_SECONDS: f64 = 3.0;

/// The reference's configured frame cap (`PIPELINE_MAX_FRAMES`).
const DEFAULT_MAX_FRAMES: i64 = 200;

/// `frame_extractor.MIN_FRAMES_FOR_QUALITY`: a quality mean must not rest on one sample, so a
/// short clip's interval is narrowed until it yields at least this many frames.
const MIN_FRAMES_FOR_QUALITY: f64 = 5.0;

/// `scdet`'s own default threshold, on its 0..100 score scale.
const DEFAULT_SCENE_THRESHOLD: f64 = 10.0;

/// The reference's beat-detector analysis rate (`beat_detector.SAMPLE_RATE`, librosa's default).
const AUDIO_SAMPLE_RATE: u32 = 22_050;

/// `beat_detector.MAX_DURATION_S`: the synchronous route's server-side ceiling.
const MAX_AUDIO_SECONDS: f64 = 600.0;

/// Bytes per decoded f32 sample.
const F32_BYTES: usize = 4;

/// A request field this kind refuses, with the stable code and the exact field(s) it names.
struct Refusal {
    field: &'static str,
    code: &'static str,
    reason: &'static str,
}

/// Reference request fields that need a capability this crate does not link.
///
/// The reason strings are caller-facing: they name the captured arithmetic and, where a served
/// field overlaps, say which one is served instead.
const VIDEO_REFUSALS: &[Refusal] = &[
    Refusal {
        field: "video_url",
        code: "input_must_be_a_file",
        reason: "an executor runs on files placed in its work directory; download the URL into \
                 the work dir and pass `input` instead",
    },
    Refusal {
        field: "compute_quality_metrics",
        code: "requires_opencv",
        reason: "the captured quality block reads decoded pixels: mean_sharpness (Laplacian \
                 variance), motion_score (frame abs-difference), dominant_colors (k-means), \
                 overall_score (derived), motion_timeline and visual_timeline. This crate links \
                 no image/OpenCV code; `brightness` is served separately from ffmpeg signalstats",
    },
    Refusal {
        field: "quality_metrics",
        code: "requires_opencv",
        reason: "same fields as compute_quality_metrics: mean_sharpness, motion_score, \
                 dominant_colors, overall_score, motion_timeline, visual_timeline; only \
                 `brightness` is measurable from ffmpeg's own output",
    },
    Refusal {
        field: "scene_boundaries",
        code: "requires_opencv",
        reason: "the captured scene boundaries are a chi-square distance between HSV frame \
                 histograms computed in OpenCV; ffmpeg's own `scdet` scores are served as \
                 `scene_changes`, a different measurement",
    },
    Refusal {
        field: "extract_frame_descriptions",
        code: "requires_vision_llm",
        reason: "per-frame descriptions are vision-provider calls behind inference-router, a \
                 licence-gated external binding",
    },
    Refusal {
        field: "extract_ocr",
        code: "requires_frame_extraction",
        reason: "the reference OCRs extracted frames; this kind analyses the file and writes no \
                 frames. The separate `media.ocr` kind reads one provided image",
    },
    Refusal {
        field: "generate_ui_inventory",
        code: "requires_vision_llm",
        reason: "the UI inventory is synthesised from per-frame vision descriptions",
    },
    Refusal {
        field: "generate_walkthrough",
        code: "requires_vision_llm",
        reason: "the walkthrough is synthesised from per-frame vision descriptions and the \
                 transcript",
    },
    Refusal {
        field: "generate_frame_index",
        code: "requires_vision_llm",
        reason: "the frame index is built from per-frame vision descriptions and extracted frames",
    },
    Refusal {
        field: "verification",
        code: "requires_imagehash",
        reason: "verification compares reference-image pHashes (imagehash) and transcript \
                 similarity; neither is derivable from ffprobe/ffmpeg",
    },
    Refusal {
        field: "language",
        code: "requires_transcription",
        reason: "language is a hint to the external ASR binding, which this executor does not call",
    },
    Refusal {
        field: "vision_provider",
        code: "requires_vision_llm",
        reason: "no vision provider is bound to a media executor",
    },
    Refusal {
        field: "max_vision_cost_usd",
        code: "requires_vision_llm",
        reason: "a vision spend cap has nothing to cap when no vision call is made",
    },
];

/// Reference audio-request fields that need a capability this crate does not link.
const AUDIO_REFUSALS: &[Refusal] = &[
    Refusal {
        field: "audio_url",
        code: "input_must_be_a_file",
        reason: "an executor runs on files placed in its work directory; download the URL into \
                 the work dir and pass `input` instead",
    },
    Refusal {
        field: "beat_grid",
        code: "requires_librosa",
        reason: "the captured grid is bpm, bpm_confidence, beats, onsets, downbeats and \
                 grid_source from librosa's median-aggregated mel onset envelope and DP beat \
                 tracker (numba); this crate links no audio DSP. duration_s and the stream \
                 metadata are served",
    },
    Refusal {
        field: "meter",
        code: "requires_librosa",
        reason: "meter sets the downbeat phase of the librosa beat grid; `downbeats` cannot be \
                 produced without it, so the knob is refused rather than ignored",
    },
];

/// Analyse a video file named by `params.input`.
pub async fn analyze_video(params: &Value, config: &Config) -> Result<Value, CapabilityError> {
    check_schema(
        params,
        &[
            "input",
            "frame_interval_seconds",
            "max_frames",
            "scene_threshold",
        ],
        VIDEO_REFUSALS,
    )?;
    let path = input_path(params, config)?;
    let probe = probe_document(&path).await?;
    let facts = VideoFacts::read(&probe)?;

    let interval = sampling_interval(facts.duration_seconds, params)?;
    let max_frames = integer_param(params, "max_frames")?.unwrap_or(DEFAULT_MAX_FRAMES);
    let threshold = scene_threshold(params)?;

    let (brightness, scene_changes) =
        measure_frames(&path, interval, max_frames, threshold).await?;
    // The reference defaults `compute_quality_metrics` to true; an explicit false declines the
    // measurement. An explicit true is refused above, because the block also requires OpenCV.
    let brightness = match params.get("compute_quality_metrics") {
        Some(Value::Bool(false)) => Value::Null,
        _ => brightness,
    };

    Ok(json!({
        "duration_seconds": facts.duration_seconds,
        "width": facts.width,
        "height": facts.height,
        "fps": facts.fps,
        "frame_count": facts.frame_count,
        "has_audio": facts.has_audio,
        "format_name": facts.format_name,
        "streams": streams_document(&probe),
        "brightness": brightness,
        "scene_changes": scene_changes,
    }))
}

/// Analyse an audio track named by `params.input`.
pub async fn analyze_audio(params: &Value, config: &Config) -> Result<Value, CapabilityError> {
    check_schema(params, &["input"], AUDIO_REFUSALS)?;
    let path = input_path(params, config)?;
    let probe = probe_document(&path).await?;
    let format = probe.get("format").cloned().unwrap_or(Value::Null);

    if !has_stream(&probe, "audio") {
        return Err(CapabilityError::Failed(
            "input has no audio stream".to_string(),
        ));
    }

    // The reference's pre-flight: the container header is checked before any decoding, so a
    // three-hour file is refused cheaply. An absent or unreadable container duration is a failure,
    // not a zero (the reference's own rule: zero would mean "empty audio reported as success").
    let container_seconds = strict_number(format.get("duration"), "format duration")?
        .filter(|seconds| *seconds > 0.0)
        .ok_or_else(|| CapabilityError::Failed("audio has no readable duration".to_string()))?;
    if container_seconds > MAX_AUDIO_SECONDS {
        return Err(CapabilityError::refused(
            "audio_too_long",
            format!(
                "audio is {container_seconds:.1}s, which exceeds the {MAX_AUDIO_SECONDS:.0}s limit"
            ),
        ));
    }

    let samples = decode_sample_count(&path).await?;
    if samples == 0 {
        return Err(CapabilityError::Failed(
            "decoded audio is empty (no audio stream?)".to_string(),
        ));
    }
    let duration_s = samples as f64 / f64::from(AUDIO_SAMPLE_RATE);

    Ok(json!({
        "duration_s": round6(duration_s),
        "format_name": format.get("format_name").cloned().unwrap_or(Value::Null),
        "has_audio": true,
        "streams": streams_document(&probe),
    }))
}

/// The probe arithmetic the reference records in `VideoMetadata`.
struct VideoFacts {
    width: u64,
    height: u64,
    duration_seconds: Option<f64>,
    fps: Option<f64>,
    frame_count: Option<u64>,
    has_audio: bool,
    format_name: Option<String>,
}

impl VideoFacts {
    fn read(probe: &Value) -> Result<Self, CapabilityError> {
        let streams = streams(probe);
        let format = probe.get("format").cloned().unwrap_or(Value::Null);
        let video = streams
            .iter()
            .find(|stream| stream.get("codec_type").and_then(Value::as_str) == Some("video"))
            .ok_or_else(|| {
                CapabilityError::Failed("ffprobe output contains no video stream".to_string())
            })?;

        let width = video.get("width").and_then(Value::as_u64).ok_or_else(|| {
            CapabilityError::Failed("ffprobe video stream has no width".to_string())
        })?;
        let height = video.get("height").and_then(Value::as_u64).ok_or_else(|| {
            CapabilityError::Failed("ffprobe video stream has no height".to_string())
        })?;

        let duration_seconds = resolve_duration_seconds(video, &format)?;
        let fps = parse_rate(video.get("avg_frame_rate"))
            .or_else(|| parse_rate(video.get("r_frame_rate")));

        // The reference's chain: the stream's own `nb_frames` when it carries one, otherwise
        // `duration * fps`. A value neither source can supply stays absent, never 0.
        let frame_count = lenient_integer(video.get("nb_frames"))
            .map(|frames| frames.max(0) as u64)
            .or_else(|| match (duration_seconds, fps) {
                (Some(seconds), Some(rate)) if seconds > 0.0 && rate > 0.0 => {
                    Some((seconds * rate).round() as u64)
                }
                _ => None,
            });

        Ok(Self {
            width,
            height,
            duration_seconds,
            fps,
            frame_count,
            has_audio: has_stream(probe, "audio"),
            format_name: format
                .get("format_name")
                .and_then(Value::as_str)
                .map(str::to_string),
        })
    }
}

/// The reference's duration fallback chain, in its documented order of authority:
/// the video stream's own duration, then `nb_frames / fps`, then a Matroska/WebM `DURATION` tag,
/// then the container — clamped by the container when both are known, because the container is an
/// upper bound over its streams by construction.
fn resolve_duration_seconds(video: &Value, format: &Value) -> Result<Option<f64>, CapabilityError> {
    let format_duration = strict_number(format.get("duration"), "format duration")?;

    let mut candidate = strict_number(video.get("duration"), "video stream duration")?
        .filter(|seconds| *seconds > 0.0);

    if candidate.is_none() {
        let frames = lenient_number(video.get("nb_frames"));
        let fps = parse_rate(video.get("avg_frame_rate"))
            .or_else(|| parse_rate(video.get("r_frame_rate")));
        if let (Some(frames), Some(rate)) = (frames, fps)
            && frames > 0.0
            && rate > 0.0
        {
            candidate = Some(frames / rate);
        }
    }

    if candidate.is_none() {
        candidate = video
            .get("tags")
            .and_then(|tags| tags.get("DURATION").or_else(|| tags.get("duration")))
            .and_then(Value::as_str)
            .and_then(parse_duration_tag);
    }

    Ok(match (candidate, format_duration) {
        (Some(stream), Some(container)) if container > 0.0 => Some(stream.min(container)),
        (Some(stream), _) => Some(stream),
        (None, container) => container.filter(|seconds| *seconds > 0.0),
    })
}

/// Parse a Matroska/WebM `DURATION` stream tag (`HH:MM:SS.nnnnnnnnn`).
fn parse_duration_tag(value: &str) -> Option<f64> {
    let parts: Vec<&str> = value.trim().split(':').collect();
    if parts.len() != 3 {
        return None;
    }
    let hours = parts[0].parse::<f64>().ok()?;
    let minutes = parts[1].parse::<f64>().ok()?;
    let seconds = parts[2].parse::<f64>().ok()?;
    let total = hours * 3600.0 + minutes * 60.0 + seconds;
    (total.is_finite() && total > 0.0).then_some(total)
}

/// Parse ffprobe's `avg_frame_rate`/`r_frame_rate` (`"30000/1001"` or `"25"`).
/// `"0/0"` (a stream that carries no frame rate) is absent, not zero.
fn parse_rate(value: Option<&Value>) -> Option<f64> {
    let raw = match value? {
        Value::String(raw) => raw.as_str(),
        Value::Number(number) => return number.as_f64().filter(|rate| *rate > 0.0),
        _ => return None,
    };
    let rate = match raw.split_once('/') {
        Some((numerator, denominator)) => {
            let numerator = numerator.trim().parse::<f64>().ok()?;
            let denominator = denominator.trim().parse::<f64>().ok()?;
            if denominator == 0.0 {
                return None;
            }
            numerator / denominator
        }
        None => raw.trim().parse::<f64>().ok()?,
    };
    (rate.is_finite() && rate > 0.0).then_some(rate)
}

/// A numeric field: absent/null means "this container does not carry it"; a present but
/// unparseable value is a corrupt probe and fails the capability (the reference's own
/// strict/lenient split).
fn strict_number(value: Option<&Value>, field: &str) -> Result<Option<f64>, CapabilityError> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let parsed = match value {
        Value::Number(number) => number.as_f64(),
        Value::String(raw) => raw.trim().parse::<f64>().ok(),
        _ => None,
    };
    match parsed {
        Some(number) if number.is_finite() => Ok(Some(number)),
        _ => Err(CapabilityError::Failed(format!(
            "ffprobe {field} is not numeric: {value}"
        ))),
    }
}

/// An advisory numeric field: any shape this port does not understand collapses to absent.
fn lenient_number(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Number(number) => number.as_f64(),
        Value::String(raw) => raw.trim().parse::<f64>().ok(),
        _ => None,
    }
    .filter(|number| number.is_finite())
}

fn lenient_integer(value: Option<&Value>) -> Option<i64> {
    match value? {
        Value::Number(number) => number.as_i64(),
        Value::String(raw) => raw.trim().parse::<i64>().ok(),
        _ => None,
    }
}

fn streams(probe: &Value) -> Vec<Value> {
    probe
        .get("streams")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn has_stream(probe: &Value, codec_type: &str) -> bool {
    streams(probe)
        .iter()
        .any(|stream| stream.get("codec_type").and_then(Value::as_str) == Some(codec_type))
}

/// The per-stream subset a caller chooses on: codec type/name, resolution, frame rate, and the
/// audio fields. An absent field stays `null`; nothing is filled with a zero.
fn streams_document(probe: &Value) -> Value {
    Value::Array(
        streams(probe)
            .iter()
            .map(|stream| {
                let codec_type = stream.get("codec_type").cloned().unwrap_or(Value::Null);
                let is_video = codec_type.as_str() == Some("video");
                json!({
                    "index": stream.get("index").cloned().unwrap_or(Value::Null),
                    "codec_type": codec_type,
                    "codec_name": stream.get("codec_name").cloned().unwrap_or(Value::Null),
                    "width": stream.get("width").cloned().unwrap_or(Value::Null),
                    "height": stream.get("height").cloned().unwrap_or(Value::Null),
                    "fps": if is_video {
                        parse_rate(
                            stream
                                .get("avg_frame_rate")
                                .or_else(|| stream.get("r_frame_rate")),
                        )
                        .map_or(Value::Null, |rate| json!(rate))
                    } else {
                        Value::Null
                    },
                    "sample_rate": stream.get("sample_rate").cloned().unwrap_or(Value::Null),
                    "channels": stream.get("channels").cloned().unwrap_or(Value::Null),
                })
            })
            .collect(),
    )
}

/// Run ffprobe exactly as the reference does: the same four arguments, the same JSON document.
async fn probe_document(path: &Path) -> Result<Value, CapabilityError> {
    let args = vec![
        "-v".to_string(),
        "error".to_string(),
        "-print_format".to_string(),
        "json".to_string(),
        "-show_format".to_string(),
        "-show_streams".to_string(),
        path.display().to_string(),
    ];
    let finished = process::run("ffprobe", &args).await?;
    if !finished.ok() {
        return Err(CapabilityError::Failed(format!(
            "ffprobe failed: {}",
            finished.tail()
        )));
    }
    serde_json::from_slice(&finished.stdout).map_err(|error| {
        CapabilityError::Failed(format!("ffprobe emitted unreadable JSON: {error}"))
    })
}

/// The reference's frame-sampling interval: `frame_interval_seconds` narrowed so a short clip
/// still yields [`MIN_FRAMES_FOR_QUALITY`] samples, but never below the `max_frames` cap. A
/// non-positive `max_frames` means "no cap" and leaves the desired interval untouched, exactly as
/// the reference's `compute_effective_interval` does.
fn sampling_interval(duration: Option<f64>, params: &Value) -> Result<f64, CapabilityError> {
    let desired = positive_number(params, "frame_interval_seconds")?
        .unwrap_or(DEFAULT_FRAME_INTERVAL_SECONDS);
    let max_frames = integer_param(params, "max_frames")?.unwrap_or(DEFAULT_MAX_FRAMES);
    let Some(seconds) = duration.filter(|seconds| *seconds > 0.0) else {
        return Ok(desired);
    };
    if max_frames <= 0 {
        return Ok(desired);
    }
    let from_floor = seconds / MIN_FRAMES_FOR_QUALITY;
    let from_cap = seconds / max_frames as f64;
    Ok(desired.min(from_floor).max(from_cap))
}

fn scene_threshold(params: &Value) -> Result<f64, CapabilityError> {
    match params.get("scene_threshold") {
        None | Some(Value::Null) => Ok(DEFAULT_SCENE_THRESHOLD),
        Some(Value::Number(number)) => {
            let threshold = number.as_f64().unwrap_or(f64::NAN);
            if threshold.is_finite() && (0.0..=100.0).contains(&threshold) {
                Ok(threshold)
            } else {
                Err(CapabilityError::Invalid(
                    "scene_threshold must be a number in 0..=100".to_string(),
                ))
            }
        }
        Some(_) => Err(CapabilityError::Invalid(
            "scene_threshold must be a number in 0..=100".to_string(),
        )),
    }
}

/// One ffmpeg pass over sampled frames, carrying both of ffmpeg's own measurements:
/// `signalstats`' mean luma (`YAVG`, 0..255) and `scdet`'s scene score (0..100). Sampling is
/// `fps=<1/interval>`, the same grid the reference's frame extractor uses.
async fn measure_frames(
    path: &Path,
    interval: f64,
    max_frames: i64,
    threshold: f64,
) -> Result<(Value, Value), CapabilityError> {
    let rate = 1.0 / interval;
    let filters =
        format!("fps={rate:.9},signalstats,scdet=threshold={threshold},metadata=print:file=-");
    let mut args = vec![
        "-hide_banner".to_string(),
        "-loglevel".to_string(),
        "error".to_string(),
        "-nostdin".to_string(),
        "-i".to_string(),
        path.display().to_string(),
        "-an".to_string(),
        "-vf".to_string(),
        filters,
    ];
    if max_frames > 0 {
        args.push("-frames:v".to_string());
        args.push(max_frames.to_string());
    }
    args.push("-f".to_string());
    args.push("null".to_string());
    args.push("-".to_string());

    let finished = process::run("ffmpeg", &args).await?;
    if !finished.ok() {
        return Err(CapabilityError::Failed(format!(
            "ffmpeg frame analysis failed: {}",
            finished.tail()
        )));
    }

    let frames = parse_metadata(&String::from_utf8_lossy(&finished.stdout));
    let brightness_values: Vec<f64> = frames
        .iter()
        .filter_map(|frame| frame.yavg)
        // `YAVG` is already the frame's mean luma on 0..255; the reference's brightness is the
        // same quantity divided by 255.
        .map(|yavg| yavg / 255.0)
        .collect();
    if brightness_values.is_empty() {
        return Err(CapabilityError::Failed(
            "ffmpeg produced no measurable video frames".to_string(),
        ));
    }
    let mean = brightness_values.iter().sum::<f64>() / brightness_values.len() as f64;
    let brightness = json!({
        "mean": round6(mean),
        "samples": brightness_values.len(),
        "interval_seconds": round6(interval),
    });

    let scene_changes: Vec<Value> = frames
        .iter()
        .filter_map(|frame| {
            let t = frame.t?;
            let score = frame.scene_score?;
            // The header's timestamp is the later frame's, i.e. when the change has landed.
            (score >= threshold).then(|| json!({"t": round6(t), "score": score}))
        })
        .collect();

    Ok((brightness, Value::Array(scene_changes)))
}

struct FrameStats {
    t: Option<f64>,
    yavg: Option<f64>,
    scene_score: Option<f64>,
}

/// Read `metadata=print` output: a `frame:N pts:P pts_time:T` header followed by `key=value`
/// lines. Only the two keys this kind measures are kept.
fn parse_metadata(text: &str) -> Vec<FrameStats> {
    let mut frames: Vec<FrameStats> = Vec::new();
    let mut pending: Option<FrameStats> = None;
    for line in text.lines() {
        if let Some(header) = line.strip_prefix("frame:") {
            if let Some(frame) = pending.take() {
                frames.push(frame);
            }
            pending = Some(FrameStats {
                t: header_time(header),
                yavg: None,
                scene_score: None,
            });
            continue;
        }
        let Some(frame) = pending.as_mut() else {
            continue;
        };
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "lavfi.signalstats.YAVG" => frame.yavg = value.trim().parse::<f64>().ok(),
            "lavfi.scd.score" => frame.scene_score = value.trim().parse::<f64>().ok(),
            _ => {}
        }
    }
    if let Some(frame) = pending {
        frames.push(frame);
    }
    frames
}

/// The `pts_time:` token of a metadata-print frame header.
fn header_time(header: &str) -> Option<f64> {
    let rest = header.split_once("pts_time:")?.1;
    rest.split_whitespace().next()?.parse::<f64>().ok()
}

/// Decode to the reference's beat-detector format (mono f32 at 22050 Hz) and count whole samples.
/// This is the reference's own `duration_s` definition — decoded samples divided by the rate — so
/// the number is exact for the same input rather than the container header's approximation.
async fn decode_sample_count(path: &Path) -> Result<usize, CapabilityError> {
    let args = vec![
        "-v".to_string(),
        "error".to_string(),
        "-nostdin".to_string(),
        "-i".to_string(),
        path.display().to_string(),
        // `-vn` keeps an mp3 with embedded cover art from being mapped as video and failing.
        "-vn".to_string(),
        "-f".to_string(),
        "f32le".to_string(),
        "-acodec".to_string(),
        "pcm_f32le".to_string(),
        "-ac".to_string(),
        "1".to_string(),
        "-ar".to_string(),
        AUDIO_SAMPLE_RATE.to_string(),
        "-".to_string(),
    ];
    let finished = process::run("ffmpeg", &args).await?;
    if !finished.ok() {
        return Err(CapabilityError::Failed(format!(
            "could not decode audio: {}",
            finished.tail()
        )));
    }
    Ok(finished.stdout.len() / F32_BYTES)
}

/// The closed parameter schema: supported keys, named refusals, and nothing else.
fn check_schema(
    params: &Value,
    supported: &[&str],
    refusals: &[Refusal],
) -> Result<(), CapabilityError> {
    let fields = params
        .as_object()
        .ok_or_else(|| CapabilityError::Invalid("params must be a JSON object".to_string()))?;

    for refusal in refusals {
        if let Some(value) = fields.get(refusal.field)
            && requested(value)
        {
            return Err(CapabilityError::refused(
                refusal.code,
                format!(
                    "{} is not served by this kind: {}",
                    refusal.field, refusal.reason
                ),
            ));
        }
    }

    for field in fields.keys() {
        if supported.contains(&field.as_str())
            || refusals
                .iter()
                .any(|refusal| refusal.field == field.as_str())
        {
            continue;
        }
        return Err(CapabilityError::refused(
            "unknown_parameter",
            format!("{field} is not a parameter of this kind (the schema is closed)"),
        ));
    }
    Ok(())
}

/// `false` and `null` decline a stage; anything else requests it.
fn requested(value: &Value) -> bool {
    !(value.is_null() || value == &Value::Bool(false))
}

fn input_path(params: &Value, config: &Config) -> Result<std::path::PathBuf, CapabilityError> {
    let name = params
        .get("input")
        .and_then(Value::as_str)
        .ok_or_else(|| CapabilityError::Invalid("input is required".to_string()))?;
    process::resolve_input(&config.work_dir, name)
}

fn positive_number(params: &Value, field: &str) -> Result<Option<f64>, CapabilityError> {
    match params.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(number)) => {
            let value = number.as_f64().unwrap_or(f64::NAN);
            if value.is_finite() && value > 0.0 {
                Ok(Some(value))
            } else {
                Err(CapabilityError::Invalid(format!(
                    "{field} must be a positive number"
                )))
            }
        }
        Some(_) => Err(CapabilityError::Invalid(format!(
            "{field} must be a positive number"
        ))),
    }
}

fn integer_param(params: &Value, field: &str) -> Result<Option<i64>, CapabilityError> {
    match params.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(number)) => number
            .as_i64()
            .map(Some)
            .ok_or_else(|| CapabilityError::Invalid(format!("{field} must be an integer"))),
        Some(_) => Err(CapabilityError::Invalid(format!(
            "{field} must be an integer"
        ))),
    }
}

fn round6(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn arguments(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    struct Fixture {
        dir: PathBuf,
    }

    impl Fixture {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "conveyor-media-analyze-{name}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            std::fs::create_dir_all(&dir).expect("fixture dir");
            Self { dir }
        }

        fn config(&self) -> Config {
            Config {
                service: "test".to_string(),
                work_dir: self.dir.clone(),
                out_dir: self.dir.clone(),
            }
        }

        /// A 1-second 320x240 @ 10 fps mp4 with a 440 Hz sine track, built by the image's own
        /// ffmpeg (the same synthetic shape the capture used).
        async fn video(&self) -> bool {
            let mut arguments = arguments(&[
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=320x240:rate=10:duration=1",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=1",
                "-c:v",
                "mpeg4",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-shortest",
                "-y",
            ]);
            arguments.push(self.dir.join("cap.mp4").display().to_string());
            process::run("ffmpeg", &arguments)
                .await
                .is_ok_and(|finished| finished.ok())
        }

        /// A 1-second 440 Hz sine WAV.
        async fn tone(&self) -> bool {
            let mut arguments = arguments(&[
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=1",
                "-c:a",
                "pcm_s16le",
                "-y",
            ]);
            arguments.push(self.dir.join("tone.wav").display().to_string());
            process::run("ffmpeg", &arguments)
                .await
                .is_ok_and(|finished| finished.ok())
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[tokio::test]
    async fn a_generated_video_is_measured_end_to_end() {
        if !(process::available("ffmpeg") && process::available("ffprobe")) {
            eprintln!("ffmpeg/ffprobe not installed; skipping the end-to-end case");
            return;
        }
        let fixture = Fixture::new("video");
        assert!(fixture.video().await, "could not build the synthetic clip");

        let document = analyze_video(&json!({"input": "cap.mp4"}), &fixture.config())
            .await
            .expect("a readable clip must be measured");

        let duration = document["duration_seconds"].as_f64().unwrap_or_default();
        assert!((duration - 1.0).abs() < 0.1, "duration was {duration}");
        assert_eq!(document["width"], json!(320));
        assert_eq!(document["height"], json!(240));
        assert_eq!(document["has_audio"], json!(true));
        let fps = document["fps"].as_f64().unwrap_or_default();
        assert!((fps - 10.0).abs() < 0.01, "fps was {fps}");

        let brightness = &document["brightness"];
        let mean = brightness["mean"].as_f64().expect("brightness.mean");
        assert!((0.0..=1.0).contains(&mean), "brightness was {mean}");
        // The reference's short-clip floor: one second at the default interval still yields at
        // least MIN_FRAMES_FOR_QUALITY samples.
        assert!(
            brightness["samples"].as_u64().unwrap_or_default() >= 5,
            "brightness was {brightness}"
        );
        assert!(
            document["scene_changes"].is_array(),
            "scene_changes was {}",
            document["scene_changes"]
        );
        assert_eq!(
            document["streams"].as_array().map(Vec::len),
            Some(2_usize),
            "streams was {}",
            document["streams"]
        );
    }

    #[tokio::test]
    async fn a_generated_sine_is_measured_end_to_end() {
        if !(process::available("ffmpeg") && process::available("ffprobe")) {
            eprintln!("ffmpeg/ffprobe not installed; skipping the end-to-end case");
            return;
        }
        let fixture = Fixture::new("audio");
        assert!(fixture.tone().await, "could not build the synthetic tone");

        let document = analyze_audio(&json!({"input": "tone.wav"}), &fixture.config())
            .await
            .expect("a readable tone must be measured");

        // 22050 whole f32 samples at the reference's analysis rate is exactly one second.
        let duration = document["duration_s"].as_f64().unwrap_or_default();
        assert!((duration - 1.0).abs() < 0.001, "duration_s was {duration}");
        assert_eq!(document["has_audio"], json!(true));
        let stream = &document["streams"][0];
        assert_eq!(stream["codec_type"], json!("audio"));
        assert_eq!(stream["codec_name"], json!("pcm_s16le"));
        assert_eq!(stream["sample_rate"], json!("44100"));
        assert_eq!(stream["channels"], json!(1));
    }

    /// Golden-derived shape: the captured result pins these field names, so the video document
    /// carries exactly them (never a renamed stand-in).
    #[test]
    fn the_captured_result_fields_keep_their_shape() {
        let lock: Value = serde_json::from_str(include_str!(
            "../../contract/video_analyzer_service/openapi.lock.json"
        ))
        .expect("the captured OpenAPI document");
        let properties = lock["components"]["schemas"]["JobResultResponse"]["properties"]
            .as_object()
            .expect("JobResultResponse properties");
        for field in ["duration_seconds", "width", "height", "has_audio"] {
            assert!(
                properties.contains_key(field),
                "{field} is not in the capture"
            );
        }
    }

    /// Golden-derived refusal: the captured beat grid names six fields this crate cannot compute;
    /// the refusal must list every one of them and never return an empty grid instead.
    #[tokio::test]
    async fn the_captured_beat_grid_fields_are_refused_by_name() {
        let golden: Value = serde_json::from_str(include_str!(
            "../../contract/video_analyzer_service/golden/audio_beat_grid.body"
        ))
        .expect("the captured beat-grid body");
        let fixture = Fixture::new("grid");
        let error = analyze_audio(
            &json!({"input": "missing.wav", "beat_grid": true}),
            &fixture.config(),
        )
        .await
        .expect_err("a librosa grid must be refused");

        match error {
            CapabilityError::Refused { code, message } => {
                assert_eq!(code, "requires_librosa");
                for field in [
                    "bpm",
                    "bpm_confidence",
                    "beats",
                    "onsets",
                    "downbeats",
                    "grid_source",
                ] {
                    assert!(golden.get(field).is_some(), "golden lost {field}");
                    assert!(
                        message.contains(field),
                        "refusal did not name {field}: {message}"
                    );
                }
            }
            other => panic!("expected a typed refusal, got {other}"),
        }
    }

    /// Refusal test: the captured quality block is OpenCV arithmetic over decoded pixels. Asking
    /// for it is refused with the exact missing fields, never approximated from ffmpeg output.
    #[tokio::test]
    async fn opencv_frame_arithmetic_is_refused_by_name() {
        let fixture = Fixture::new("refusal");
        let error = analyze_video(
            &json!({"input": "missing.mp4", "compute_quality_metrics": true}),
            &fixture.config(),
        )
        .await
        .expect_err("OpenCV arithmetic must be refused");

        match error {
            CapabilityError::Refused { code, message } => {
                assert_eq!(code, "requires_opencv");
                for field in [
                    "mean_sharpness",
                    "motion_score",
                    "dominant_colors",
                    "overall_score",
                    "motion_timeline",
                    "visual_timeline",
                ] {
                    assert!(
                        message.contains(field),
                        "refusal did not name {field}: {message}"
                    );
                }
            }
            other => panic!("expected a typed refusal, got {other}"),
        }
    }

    #[tokio::test]
    async fn an_unknown_parameter_is_refused_because_the_schema_is_closed() {
        let fixture = Fixture::new("closed");
        let error = analyze_video(
            &json!({"input": "missing.mp4", "frame_intervall_seconds": 1.0}),
            &fixture.config(),
        )
        .await
        .expect_err("a misspelt knob must not be silently ignored");
        match error {
            CapabilityError::Refused { code, .. } => assert_eq!(code, "unknown_parameter"),
            other => panic!("expected a typed refusal, got {other}"),
        }
    }

    #[test]
    fn a_container_duration_that_is_present_but_garbage_fails_the_probe() {
        let error = strict_number(Some(&json!("N/A")), "format duration")
            .expect_err("a present unparseable duration is corrupt, not absent");
        assert!(error.to_string().contains("format duration"), "{error}");
    }

    #[test]
    fn the_reference_frame_interval_floor_is_reproduced() {
        // 1 s clip, 3 s desired interval, 200 frame cap -> narrowed to 1/5 s for the floor.
        let params = json!({});
        let interval = sampling_interval(Some(1.0), &params).expect("interval");
        assert!((interval - 0.2).abs() < 1e-9, "interval was {interval}");

        // A long clip keeps the desired interval.
        let interval = sampling_interval(Some(600.0), &params).expect("interval");
        assert!((interval - 3.0).abs() < 1e-9, "interval was {interval}");
    }
}
