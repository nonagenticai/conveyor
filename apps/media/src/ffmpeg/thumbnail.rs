//! `media.thumbnail` — poster frames from a video, with safe-area geometry.
//!
//! The captured contract is `ThumbnailRequest`/`ThumbnailResponse` in
//! `contract/video_compiler_service/openapi.lock.json`. The reference extracts inline: it probes
//! the source, samples `count` evenly spaced timestamps from the middle 90% of the clip (never the
//! lead-in or the tail card), extracts one frame per timestamp, retargets the canvas to a
//! deliverable format when one is named, and reports 90% action-safe / 80% title-safe rectangles
//! beside each frame. Sampling and geometry are deterministic: the same duration and count always
//! produce the same timestamps and rectangles.
//!
//! Two request inputs are refused **by name** rather than accepted and ignored: a remote
//! `video_url` (this image has no egress and no object store — every input is already a file in
//! the job's work directory) and a whole-film `grade` (the reference's `renderer._grade_vf` chain
//! is not ported). A disabled or identity grade is the reference's own no-op and stays accepted.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::common::{CapabilityError, Config, process};

/// The kind slug this module implements.
pub const KIND: &str = "media.thumbnail";

/// Deliverable aspect to canvas dimensions — the reference's `_FORMAT_DIMS` map, shared with the
/// compile route so a poster can never ship on a canvas the film does not.
const FORMAT_DIMS: &[(&str, u32, u32)] = &[
    ("16x9", 1920, 1080),
    ("9x16", 1080, 1920),
    ("1x1", 1080, 1080),
    ("4x5", 1080, 1350),
];

/// The middle 90% is the body of the film; the first/last 5% are lead-in and tail cards.
const EDGE_TRIM: f64 = 0.05;
const ACTION_SAFE_FRAC: f64 = 0.90;
const TITLE_SAFE_FRAC: f64 = 0.80;
const DEFAULT_COUNT: u64 = 3;
const MIN_COUNT: u64 = 1;
const MAX_COUNT: u64 = 12;

/// `_SWS_FLAGS` plus the reference's explicit bt709 matrix: every retargeted poster is scaled with
/// the same kernel and colour matrix as the compiler's other `scale=` nodes.
const SCALE_FLAGS: &str =
    "flags=lanczos+accurate_rnd+full_chroma_int:in_color_matrix=bt709:out_color_matrix=bt709";

/// Extract the poster candidates the request asks for into `config.out_dir`.
pub async fn thumbnail(params: &Value, config: &Config) -> Result<Value, CapabilityError> {
    let video_url = params
        .get("video_url")
        .and_then(Value::as_str)
        .ok_or_else(|| CapabilityError::Invalid("video_url: Field required".to_string()))?;

    // Refusals and schema checks precede any filesystem work, so an unservable request is named
    // as such even when its input does not exist.
    validate_grade(params)?;
    validate_tags(params)?;
    let count = candidate_count(params.get("count"))?;
    let requested_format = requested_format(params)?;

    let source = resolve_source(&config.work_dir, video_url)?;
    let probe = probe_media(&source).await?;
    let duration = probe.duration_seconds.ok_or_else(|| {
        CapabilityError::Invalid(format!(
            "cannot probe a positive duration from {video_url:?}"
        ))
    })?;
    let dims = format_dims(requested_format);
    let (width, height) = match dims {
        Some(dims) => dims,
        None => probe.dimensions().ok_or_else(|| {
            CapabilityError::Invalid(format!(
                "cannot probe positive dimensions from {video_url:?}"
            ))
        })?,
    };

    // `tags.run_id` namespaces the reference's uploaded PNGs; with no object store it namespaces
    // the output directory instead, so the same request still lands under the same prefix.
    let namespace = output_namespace(params.get("tags"));
    let out_dir = config.out_dir.join(namespace);
    std::fs::create_dir_all(&out_dir).map_err(|error| {
        CapabilityError::Failed(format!("cannot create {:?}: {error}", out_dir))
    })?;

    let mut candidates = Vec::with_capacity(count as usize);
    for (index, timestamp) in sample_timestamps(duration, count).into_iter().enumerate() {
        let output = out_dir.join(format!("thumb-{index}.png"));
        let mut args = vec![
            "-y".to_string(),
            "-ss".to_string(),
            format!("{timestamp:.3}"),
            "-i".to_string(),
            source.display().to_string(),
            "-frames:v".to_string(),
            "1".to_string(),
        ];
        if dims.is_some() {
            args.push("-vf".to_string());
            args.push(format!("scale={width}:{height}:{SCALE_FLAGS}"));
        }
        args.push(output.display().to_string());
        let finished = process::run("ffmpeg", &args).await?;
        if !finished.ok() {
            return Err(CapabilityError::Failed(format!(
                "ffmpeg could not extract a frame at {timestamp:.3}s: {}",
                finished.tail()
            )));
        }
        candidates.push(candidate_document(&output, timestamp, width, height));
    }

    Ok(json!({ "candidates": candidates }))
}

/// One `ThumbnailCandidate`. The URL is a `file://` handle: this image writes the frame into the
/// job's output directory and conveyor's own routes carry the durable bytes, so there is nothing
/// to presign here (the reference's `url` is a presigned GET and is not a replayable field).
fn candidate_document(output: &Path, timestamp: f64, width: u32, height: u32) -> Value {
    json!({
        "url": format!("file://{}", output.display()),
        "timestamp_seconds": round_millis(timestamp),
        "width": width,
        "height": height,
        "action_safe": safe_rect(width, height, ACTION_SAFE_FRAC),
        "title_safe": safe_rect(width, height, TITLE_SAFE_FRAC),
    })
}

/// A centered rectangle at `frac` of the frame, in pixels (the reference's `_safe_rect`).
fn safe_rect(width: u32, height: u32, frac: f64) -> Value {
    let rect_width = (f64::from(width) * frac).round() as u32;
    let rect_height = (f64::from(height) * frac).round() as u32;
    json!({
        "x": (width - rect_width) / 2,
        "y": (height - rect_height) / 2,
        "width": rect_width,
        "height": rect_height,
    })
}

/// DETERMINISTIC evenly-spaced sample points across the middle 90% of the clip: each point is the
/// centre of its equal slice (the reference's `_timestamps`).
fn sample_timestamps(duration: f64, count: u64) -> Vec<f64> {
    let start = duration * EDGE_TRIM;
    let span = duration * (1.0 - 2.0 * EDGE_TRIM);
    (0..count)
        .map(|index| start + span * (index as f64 + 0.5) / count as f64)
        .collect()
}

/// The count is clamped in the service, not only at the schema (the reference's `_clamp_count`),
/// so a direct caller cannot request 0 or 10_000 frames.
fn candidate_count(value: Option<&Value>) -> Result<u64, CapabilityError> {
    let raw = match value {
        None => DEFAULT_COUNT,
        Some(value) => value.as_u64().ok_or_else(|| {
            CapabilityError::Invalid("count must be a non-negative integer".to_string())
        })?,
    };
    Ok(raw.clamp(MIN_COUNT, MAX_COUNT))
}

/// The optional deliverable aspect. Absent, `null`, an empty string and an unknown aspect all mean
/// "keep the source dimensions", exactly like the reference's `_FORMAT_DIMS.get`.
fn requested_format(params: &Value) -> Result<Option<&str>, CapabilityError> {
    match params.get("format") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(format)) => Ok(Some(format.as_str())),
        Some(_) => Err(CapabilityError::Invalid(
            "format must be a string".to_string(),
        )),
    }
}

fn format_dims(format: Option<&str>) -> Option<(u32, u32)> {
    let key = format?.trim();
    FORMAT_DIMS
        .iter()
        .find(|(name, _, _)| *name == key)
        .map(|(_, width, height)| (*width, *height))
}

/// A whole-film grade is a real request behaviour this image does not implement, so it is refused
/// by name. `enabled=false` (the schema default) is the reference's documented no-op — the whole
/// grade chain emits nothing — and stays accepted. `match_sources` is rejected even when disabled,
/// because the reference rejects it before looking at `enabled`: a single frame cannot reproduce a
/// grade biased toward the whole film's measured signalstats.
fn validate_grade(params: &Value) -> Result<(), CapabilityError> {
    let Some(grade) = params.get("grade").filter(|value| !value.is_null()) else {
        return Ok(());
    };
    let object = grade
        .as_object()
        .ok_or_else(|| CapabilityError::Invalid("grade must be an object".to_string()))?;
    if object.get("match_sources").and_then(Value::as_bool) == Some(true) {
        return Err(CapabilityError::refused(
            "match_sources_not_supported",
            "a match_sources grade cannot be reproduced from a single frame; \
             extract the poster from the already-graded film with grade=null",
        ));
    }
    match object.get("enabled") {
        None | Some(Value::Null) | Some(Value::Bool(false)) => Ok(()),
        Some(Value::Bool(true)) => Err(CapabilityError::refused(
            "grade_not_supported",
            "the whole-film grade filter chain (eq/colorbalance/vignette/noise/unsharp/bloom) \
             is not ported; send grade=null or grade.enabled=false",
        )),
        Some(_) => Err(CapabilityError::Invalid(
            "grade.enabled must be a boolean".to_string(),
        )),
    }
}

fn validate_tags(params: &Value) -> Result<(), CapabilityError> {
    let Some(tags) = params.get("tags").filter(|value| !value.is_null()) else {
        return Ok(());
    };
    let object = tags
        .as_object()
        .ok_or_else(|| CapabilityError::Invalid("tags must be an object".to_string()))?;
    if let Some(run_id) = object.get("run_id").filter(|value| !value.is_null())
        && !run_id.is_string()
    {
        return Err(CapabilityError::Invalid(
            "tags.run_id must be a string".to_string(),
        ));
    }
    Ok(())
}

/// `tags.run_id`, sanitised exactly like the reference's `_safe_run_id`: everything outside
/// `[A-Za-z0-9_-]` becomes `_`, at most 64 characters, and an empty value is `ad-hoc`.
fn output_namespace(tags: Option<&Value>) -> String {
    let raw = tags
        .and_then(Value::as_object)
        .and_then(|tags| tags.get("run_id"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let cleaned: String = raw
        .trim()
        .chars()
        .take(64)
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' || character == '-' {
                character
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "ad-hoc".to_string()
    } else {
        cleaned
    }
}

/// The job's input, or a named refusal for a reference this image cannot fetch. The reference
/// accepts a presigned HTTP(S) URL, `file://` and a bare path; an executor pod is handed files,
/// so only the last two are servable here.
fn resolve_source(root: &Path, reference: &str) -> Result<PathBuf, CapabilityError> {
    if reference.starts_with("http://") || reference.starts_with("https://") {
        return Err(CapabilityError::refused(
            "remote_input_not_supported",
            format!(
                "{reference} is a remote URL; this image has no egress — inputs are files in the \
                 job's work directory"
            ),
        ));
    }
    let name = reference.strip_prefix("file://").unwrap_or(reference);
    process::resolve_input(root, name)
}

/// The stable measurements a poster depends on: a duration the container (or its video stream)
/// states, and the primary video stream's dimensions. Unknown is `None`, never zero — a
/// degenerate rectangle in the response would be a lie about the frame.
struct MediaProbe {
    duration_seconds: Option<f64>,
    width: Option<u32>,
    height: Option<u32>,
}

impl MediaProbe {
    fn dimensions(&self) -> Option<(u32, u32)> {
        match (self.width, self.height) {
            (Some(width), Some(height)) if width > 0 && height > 0 => Some((width, height)),
            _ => None,
        }
    }
}

async fn probe_media(path: &Path) -> Result<MediaProbe, CapabilityError> {
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
    let document: Value = serde_json::from_slice(&finished.stdout).map_err(|error| {
        CapabilityError::Failed(format!("ffprobe emitted unreadable JSON: {error}"))
    })?;

    let video = document
        .get("streams")
        .and_then(Value::as_array)
        .and_then(|streams| {
            streams
                .iter()
                .find(|stream| stream.get("codec_type").and_then(Value::as_str) == Some("video"))
        });
    // The video stream's own duration wins when it states one (the reference's
    // `_ffprobe_duration`); the container is the fallback, and an audio-only input never had a
    // video stream to ask.
    let stream_duration = video
        .and_then(|stream| stream.get("duration"))
        .and_then(positive_seconds);
    let container_duration = document
        .get("format")
        .and_then(|format| format.get("duration"))
        .and_then(positive_seconds);
    Ok(MediaProbe {
        duration_seconds: stream_duration.or(container_duration),
        width: video
            .and_then(|stream| stream.get("width"))
            .and_then(positive_pixels),
        height: video
            .and_then(|stream| stream.get("height"))
            .and_then(positive_pixels),
    })
}

fn positive_seconds(value: &Value) -> Option<f64> {
    value
        .as_str()
        .and_then(|raw| raw.parse::<f64>().ok())
        .filter(|seconds| seconds.is_finite() && *seconds > 0.0)
}

fn positive_pixels(value: &Value) -> Option<u32> {
    value
        .as_u64()
        .and_then(|pixels| u32::try_from(pixels).ok())
        .filter(|pixels| *pixels > 0)
}

fn round_millis(seconds: f64) -> f64 {
    (seconds * 1000.0).round() / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "conveyor-media-thumbnail-{label}-{}",
            uuid::Uuid::now_v7()
        ))
    }

    fn test_config(root: &Path) -> Config {
        Config {
            service: "conveyor-media-test".to_string(),
            work_dir: root.to_path_buf(),
            out_dir: root.join("out"),
        }
    }

    /// A real one-second H.264-free mp4: `mpeg4` is built into every ffmpeg and needs no external
    /// encoder, so the fixture has the same availability as the tool under test.
    async fn generate_video(root: &Path) {
        std::fs::create_dir_all(root).unwrap();
        let args = vec![
            "-y".to_string(),
            "-v".to_string(),
            "error".to_string(),
            "-f".to_string(),
            "lavfi".to_string(),
            "-i".to_string(),
            "testsrc=duration=1:size=320x240:rate=10".to_string(),
            "-c:v".to_string(),
            "mpeg4".to_string(),
            root.join("source.mp4").display().to_string(),
        ];
        let finished = process::run("ffmpeg", &args).await.unwrap();
        assert!(
            finished.ok(),
            "could not generate the fixture: {}",
            finished.tail()
        );
    }

    #[tokio::test]
    async fn extracts_a_frame_from_a_generated_clip() {
        if !(process::available("ffmpeg") && process::available("ffprobe")) {
            eprintln!("ffmpeg/ffprobe not installed; skipping the end-to-end case");
            return;
        }
        let root = scratch("extract");
        generate_video(&root).await;
        let config = test_config(&root);

        let result = thumbnail(
            &json!({
                "video_url": "source.mp4",
                "count": 2,
                "format": "16x9",
                "tags": {"run_id": "run-7"},
            }),
            &config,
        )
        .await
        .unwrap();

        let candidates = result["candidates"].as_array().unwrap();
        assert_eq!(candidates.len(), 2);
        let mut timestamps = Vec::new();
        for (index, candidate) in candidates.iter().enumerate() {
            assert_eq!(candidate["width"].as_u64(), Some(1920));
            assert_eq!(candidate["height"].as_u64(), Some(1080));
            let url = candidate["url"].as_str().unwrap();
            let path = PathBuf::from(url.strip_prefix("file://").expect("a file:// url"));
            // `tags.run_id` namespaces the outputs, as it namespaces the reference's uploads.
            assert!(
                path.starts_with(config.out_dir.join("run-7")),
                "{path:?} must live under the run's namespace"
            );
            assert_eq!(
                path.file_name().and_then(|name| name.to_str()),
                Some(format!("thumb-{index}.png").as_str())
            );
            let bytes = std::fs::read(&path).unwrap();
            assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
            timestamps.push(candidate["timestamp_seconds"].as_f64().unwrap());
        }
        assert!(
            timestamps[0] > 0.05 && timestamps[0] < timestamps[1] && timestamps[1] < 0.95,
            "timestamps {timestamps:?} must be strictly increasing inside the middle 90%"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The captured contract's payload shape (`ThumbnailResponse` → `ThumbnailCandidate`), with
    /// the safe-rect arithmetic the reference documents: 90% of 1920x1080 is 1728x972 at (96,54),
    /// 80% is 1536x864 at (192,108).
    #[test]
    fn the_candidate_document_matches_the_captured_schema() {
        let document = candidate_document(Path::new("/tmp/ad-hoc/thumb-0.png"), 1.5, 1920, 1080);
        let mut keys: Vec<&str> = document
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "action_safe",
                "height",
                "timestamp_seconds",
                "title_safe",
                "url",
                "width"
            ]
        );
        assert_eq!(document["url"], json!("file:///tmp/ad-hoc/thumb-0.png"));
        assert_eq!(document["timestamp_seconds"], json!(1.5));
        assert_eq!(
            document["action_safe"],
            json!({"x": 96, "y": 54, "width": 1728, "height": 972})
        );
        assert_eq!(
            document["title_safe"],
            json!({"x": 192, "y": 108, "width": 1536, "height": 864})
        );
    }

    #[test]
    fn sampling_uses_the_middle_ninety_percent() {
        assert_eq!(sample_timestamps(10.0, 3), vec![2.0, 5.0, 8.0]);
    }

    #[test]
    fn a_disabled_grade_is_the_documented_no_op() {
        assert!(validate_grade(&json!({"grade": {"enabled": false, "contrast": 1.2}})).is_ok());
    }

    #[tokio::test]
    async fn a_whole_film_grade_is_refused_by_name() {
        let root = scratch("grade");
        let config = test_config(&root);
        let error = thumbnail(
            &json!({"video_url": "source.mp4", "grade": {"enabled": true, "contrast": 1.2}}),
            &config,
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("grade_not_supported"), "{error}");
    }

    #[tokio::test]
    async fn a_remote_video_url_is_refused_by_name() {
        let root = scratch("remote");
        let config = test_config(&root);
        let error = thumbnail(
            &json!({"video_url": "https://example.invalid/hero.mp4"}),
            &config,
        )
        .await
        .unwrap_err();
        assert!(
            error.to_string().contains("remote_input_not_supported"),
            "{error}"
        );
    }

    /// The captured 422 names the missing field as `video_url: Field required`; the port's
    /// validation failure must name the same field, or a caller cannot map one to the other.
    #[tokio::test]
    async fn a_missing_video_url_names_the_captured_field() {
        let root = scratch("missing");
        let config = test_config(&root);
        let error = thumbnail(&json!({}), &config).await.unwrap_err();
        assert!(
            error.to_string().contains("video_url: Field required"),
            "{error}"
        );
    }
}
