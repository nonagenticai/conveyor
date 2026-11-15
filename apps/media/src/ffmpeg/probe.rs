//! `media.probe_video` — what a file is, before anything is decided about it.
//!
//! `ffprobe -print_format json` is the same call the reference makes; the point of the port is not
//! to re-derive the container metadata in Rust but to keep the answer's **shape** stable and the
//! arithmetic that callers rely on honest: a duration is either a number the container states, or
//! it is absent — it is never `0.0` standing in for "unknown" (the failure the reference records
//! for unmeasured audio, and the one that silently collapses a timeline).

use std::path::Path;

use serde_json::{Value, json};

use crate::common::{CapabilityError, Config};
use crate::ffmpeg::process;

/// The kind slug this module implements.
pub const KIND: &str = "media.probe_video";

/// Probe the file named by `params.input` inside the job's work directory.
pub async fn probe(params: &Value, config: &Config) -> Result<Value, CapabilityError> {
    let input = params
        .get("input")
        .and_then(Value::as_str)
        .ok_or_else(|| CapabilityError::Invalid("input is required".to_string()))?;
    let path = process::resolve_input(&config.work_dir, input)?;

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
    let document: Value = serde_json::from_slice(&finished.stdout)
        .map_err(|error| CapabilityError::Failed(format!("ffprobe emitted unreadable JSON: {error}")))?;
    Ok(summarise(&document))
}

/// The stable subset of an ffprobe document: what a caller chooses on, nothing else.
pub fn summarise(probe: &Value) -> Value {
    let streams: Vec<Value> = probe
        .get("streams")
        .and_then(Value::as_array)
        .map(|streams| {
            streams
                .iter()
                .map(|stream| {
                    json!({
                        "index": stream.get("index").cloned().unwrap_or(Value::Null),
                        "codec_type": stream.get("codec_type").cloned().unwrap_or(Value::Null),
                        "codec_name": stream.get("codec_name").cloned().unwrap_or(Value::Null),
                        "width": stream.get("width").cloned().unwrap_or(Value::Null),
                        "height": stream.get("height").cloned().unwrap_or(Value::Null),
                        "sample_rate": stream.get("sample_rate").cloned().unwrap_or(Value::Null),
                        "channels": stream.get("channels").cloned().unwrap_or(Value::Null),
                        "pix_fmt": stream.get("pix_fmt").cloned().unwrap_or(Value::Null),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let format = probe.get("format").cloned().unwrap_or(Value::Null);
    json!({
        "duration_seconds": parse_seconds(format.get("duration")),
        "size_bytes": parse_integer(format.get("size")),
        "bit_rate": parse_integer(format.get("bit_rate")),
        "format_name": format.get("format_name").cloned().unwrap_or(Value::Null),
        "streams": streams,
    })
}

/// A container's `duration` is a string in ffprobe's JSON. Absent or unparseable stays `null`:
/// an unknown length is not zero, and a caller that treats it as zero builds a broken timeline.
fn parse_seconds(value: Option<&Value>) -> Value {
    value
        .and_then(Value::as_str)
        .and_then(|raw| raw.parse::<f64>().ok())
        .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)
        .map(|seconds| json!(seconds))
        .unwrap_or(Value::Null)
}

fn parse_integer(value: Option<&Value>) -> Value {
    value
        .and_then(Value::as_str)
        .and_then(|raw| raw.parse::<i64>().ok())
        .map(|n| json!(n))
        .unwrap_or(Value::Null)
}

/// Whether a probed file has a video stream — the check a renderer makes before drawing on it.
pub fn has_video_stream(summary: &Value) -> bool {
    summary
        .get("streams")
        .and_then(Value::as_array)
        .map(|streams| {
            streams
                .iter()
                .any(|s| s.get("codec_type").and_then(Value::as_str) == Some("video"))
        })
        .unwrap_or(false)
}

/// Whether `path` exists — used by the capability tests to assert an output was really written.
pub fn exists(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_absent_duration_stays_null_never_zero() {
        let summarised = summarise(&json!({"format": {"format_name": "wav"}, "streams": []}));
        assert_eq!(summarised["duration_seconds"], Value::Null);
    }

    #[test]
    fn a_zero_duration_is_kept_as_zero() {
        let summarised = summarise(&json!({"format": {"duration": "0.000000"}}));
        assert_eq!(summarised["duration_seconds"], json!(0.0));
    }

    #[test]
    fn a_video_stream_is_detected() {
        let summarised = summarise(&json!({
            "format": {"duration": "2.0"},
            "streams": [{"codec_type": "video", "codec_name": "h264"}]
        }));
        assert!(has_video_stream(&summarised));
    }
}
