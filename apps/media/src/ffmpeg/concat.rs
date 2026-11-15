//! `media.concat_audio` — several audio inputs joined into one track, in the order given.
//!
//! The captured contract is `AudioConcatRequest`/`AudioConcatResponse` in
//! `contract/video_compiler_service/openapi.lock.json`. The reference stitches with ffmpeg: a
//! uniform WAV with no inter-clip gap takes the stream-copy demuxer path; MP3, m4b or a non-zero
//! `gap_ms` re-encode through the concat filter, with `anullsrc` silence between clips. The
//! output's duration is always re-probed — a clip's `seconds` is a billing input (the reference's
//! `_declared_clip_seconds`), never a rendering one, so it is validated and carried but never read
//! to size the track.
//!
//! `acx_master` (the ACX program-loudness master) and non-empty `chapters` on an m4b (the
//! ffmetadata remux) are refused **by name**: both are real request behaviours whose omission
//! would silently change the delivered audio. `chapters` on wav/mp3 is the captured contract's own
//! no-op ("only honoured for output_format=m4b"), as is `acx_master.enabled=false`.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::common::{CapabilityError, Config, process};

/// The kind slug this module implements.
pub const KIND: &str = "media.concat_audio";

/// The captured request's bounds.
const MAX_CLIPS: usize = 10_000;
const MAX_GAP_MS: u64 = 5_000;

/// Output codec arguments per `output_format` — the reference's `_AUDIOBOOK_OUTPUT_CODECS`,
/// unchanged: WAV stays PCM (lossless), MP3 is VBR q2, m4b is 128k AAC in the `ipod` container.
fn codec_args(output_format: &str) -> Option<&'static [&'static str]> {
    match output_format {
        "wav" => Some(&["-c:a", "pcm_s16le"]),
        "mp3" => Some(&["-c:a", "libmp3lame", "-q:a", "2"]),
        "m4b" => Some(&["-c:a", "aac", "-b:a", "128k", "-f", "ipod"]),
        _ => None,
    }
}

/// One named input in the request. `seconds` is deliberately absent: it is advisory billing
/// metadata the reference's renderer never reads, so there is nothing here to implement.
struct Clip {
    id: String,
    audio_url: String,
}

/// Concatenate the audio inputs the request names into `config.out_dir`.
pub async fn concat_audio(params: &Value, config: &Config) -> Result<Value, CapabilityError> {
    let clips = parse_clips(params)?;
    let output_format = parse_output_format(params)?;
    let gap_ms = parse_gap_ms(params)?;

    // Refusals and schema checks precede any filesystem work, so an unservable request is named
    // as such even when its inputs do not exist.
    refuse_unimplemented_fields(params, output_format)?;
    validate_tags(params)?;

    let mut paths = Vec::with_capacity(clips.len());
    for clip in &clips {
        paths.push(resolve_audio(&config.work_dir, &clip.audio_url)?);
    }
    // Both concat paths require uniform input: the copy demuxer stream-copies, and the filter
    // path sizes its `anullsrc` gaps from the first clip. The reference refuses the rest rather
    // than let ffmpeg guess (`_assert_uniform_audio`).
    let (sample_rate, channels) = audio_layout(&clips, &paths).await?;

    let namespace = output_namespace(params.get("tags"));
    let out_dir = config.out_dir.join(&namespace);
    std::fs::create_dir_all(&out_dir).map_err(|error| {
        CapabilityError::Failed(format!("cannot create {:?}: {error}", out_dir))
    })?;
    let output = out_dir.join(format!("audiobook.{output_format}"));

    if output_format == "wav" && gap_ms == 0 {
        concat_copy(&paths, &output).await?;
    } else {
        concat_filter(
            &paths,
            &output,
            output_format,
            gap_ms,
            sample_rate,
            channels,
        )
        .await?;
    }

    let duration_seconds = probe_duration(&output).await?;
    Ok(result_document(
        &output,
        &namespace,
        output_format,
        duration_seconds,
    ))
}

/// The captured `AudioConcatResponse` shape. `artifact_url` is a `file://` handle on the local
/// output; `bucket` is null because this image has no object store (conveyor owns the store and
/// the job's outputs). `acx_report` is null because `acx_master` is refused above, and null is
/// exactly what the reference reports when the master was not requested.
fn result_document(
    output: &Path,
    namespace: &str,
    output_format: &str,
    duration_seconds: f64,
) -> Value {
    json!({
        "artifact_url": format!("file://{}", output.display()),
        "duration_seconds": round_millis(duration_seconds),
        "object_key": format!("{namespace}/audiobook.{output_format}"),
        "bucket": null,
        "acx_report": null,
    })
}

/// The request's clips, validated against the captured schema (required `id` and `audio_url`, the
/// id charset, the entry cap). `seconds` is checked for its captured type and then left alone.
fn parse_clips(params: &Value) -> Result<Vec<Clip>, CapabilityError> {
    let clips = params
        .get("clips")
        .and_then(Value::as_array)
        .ok_or_else(|| CapabilityError::Invalid("clips: Field required".to_string()))?;
    if clips.is_empty() {
        return Err(CapabilityError::Invalid(
            "clips must contain at least one clip".to_string(),
        ));
    }
    if clips.len() > MAX_CLIPS {
        return Err(CapabilityError::Invalid(format!(
            "clips must contain at most {MAX_CLIPS} entries"
        )));
    }

    let mut parsed = Vec::with_capacity(clips.len());
    for (index, clip) in clips.iter().enumerate() {
        let object = clip
            .as_object()
            .ok_or_else(|| CapabilityError::Invalid(format!("clips[{index}] must be an object")))?;
        let id = object.get("id").and_then(Value::as_str).ok_or_else(|| {
            CapabilityError::Invalid(format!("clips[{index}].id: Field required"))
        })?;
        if id.is_empty()
            || id.len() > 128
            || !id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        {
            return Err(CapabilityError::Invalid(format!(
                "clips[{index}].id must match ^[A-Za-z0-9_.-]+$ and be at most 128 characters"
            )));
        }
        let audio_url = object
            .get("audio_url")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                CapabilityError::Invalid(format!("clips[{index}].audio_url: Field required"))
            })?;
        if let Some(seconds) = object.get("seconds").filter(|value| !value.is_null())
            && !seconds.is_number()
        {
            return Err(CapabilityError::Invalid(format!(
                "clips[{index}].seconds must be a number or null"
            )));
        }
        parsed.push(Clip {
            id: id.to_string(),
            audio_url: audio_url.to_string(),
        });
    }
    Ok(parsed)
}

fn parse_output_format(params: &Value) -> Result<&str, CapabilityError> {
    let format = match params.get("output_format") {
        None => "wav",
        Some(Value::String(format)) => format.as_str(),
        Some(_) => {
            return Err(CapabilityError::Invalid(
                "output_format must be \"wav\", \"mp3\" or \"m4b\"".to_string(),
            ));
        }
    };
    if codec_args(format).is_none() {
        return Err(CapabilityError::Invalid(format!(
            "output_format must be \"wav\", \"mp3\" or \"m4b\", not {format:?}"
        )));
    }
    Ok(format)
}

fn parse_gap_ms(params: &Value) -> Result<u64, CapabilityError> {
    let gap_ms = match params.get("gap_ms") {
        None | Some(Value::Null) => 0,
        Some(value) => value.as_u64().ok_or_else(|| {
            CapabilityError::Invalid("gap_ms must be a non-negative integer".to_string())
        })?,
    };
    if gap_ms > MAX_GAP_MS {
        return Err(CapabilityError::Invalid(format!(
            "gap_ms must be at most {MAX_GAP_MS}"
        )));
    }
    Ok(gap_ms)
}

/// Refuse the request behaviours this image does not implement, by name.
fn refuse_unimplemented_fields(params: &Value, output_format: &str) -> Result<(), CapabilityError> {
    if let Some(acx_master) = params.get("acx_master").filter(|value| !value.is_null()) {
        let object = acx_master
            .as_object()
            .ok_or_else(|| CapabilityError::Invalid("acx_master must be an object".to_string()))?;
        // Presence is opt-in in the captured contract, but `enabled=false` turns the master off —
        // the reference passes `bool(acx and acx.enabled)` — and that is its documented no-op.
        match object.get("enabled") {
            None | Some(Value::Null) | Some(Value::Bool(true)) => {
                return Err(CapabilityError::refused(
                    "acx_master_not_supported",
                    "the ACX two-pass loudnorm master and its compliance report are not ported; \
                     send acx_master=null or acx_master.enabled=false",
                ));
            }
            Some(Value::Bool(false)) => {}
            Some(_) => {
                return Err(CapabilityError::Invalid(
                    "acx_master.enabled must be a boolean".to_string(),
                ));
            }
        }
    }

    if let Some(chapters) = params.get("chapters").filter(|value| !value.is_null()) {
        let list = chapters
            .as_array()
            .ok_or_else(|| CapabilityError::Invalid("chapters must be an array".to_string()))?;
        // Chapters are only honoured for m4b in the captured contract; on the other containers
        // the reference ignores them, which is the contract's behaviour rather than a gap.
        if output_format == "m4b" && !list.is_empty() {
            return Err(CapabilityError::refused(
                "chapters_not_supported",
                "m4b chapter markers (the ffmetadata remux) are not ported; \
                 send chapters=null or chapters=[]",
            ));
        }
    }
    Ok(())
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

/// `tags.run_id`, sanitised exactly like the reference's `_safe_run_id` (everything outside
/// `[A-Za-z0-9_-]` becomes `_`, at most 64 characters, empty is `ad-hoc`). It prefixes the
/// reference's object key; here it prefixes the local output so the document keeps that shape.
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
fn resolve_audio(root: &Path, reference: &str) -> Result<PathBuf, CapabilityError> {
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

/// The first audio stream's `(sample_rate, channels)`, or `None` when the clip has no audio.
async fn probe_audio_params(path: &Path) -> Result<Option<(u32, u32)>, CapabilityError> {
    let args = vec![
        "-v".to_string(),
        "error".to_string(),
        "-select_streams".to_string(),
        "a:0".to_string(),
        "-show_entries".to_string(),
        "stream=sample_rate,channels".to_string(),
        "-of".to_string(),
        "csv=s=x:p=0".to_string(),
        path.display().to_string(),
    ];
    let finished = process::run("ffprobe", &args).await?;
    if !finished.ok() {
        return Err(CapabilityError::Failed(format!(
            "ffprobe failed for {:?}: {}",
            path.display().to_string(),
            finished.tail()
        )));
    }
    let text = String::from_utf8_lossy(&finished.stdout);
    let line = text.trim();
    if line.is_empty() {
        return Ok(None);
    }
    let unreadable = || {
        CapabilityError::Failed(format!(
            "ffprobe returned unreadable audio parameters for {:?}",
            path.display().to_string()
        ))
    };
    let (rate, channels) = line.split_once('x').ok_or_else(unreadable)?;
    let rate = rate.trim().parse::<u32>().map_err(|_| unreadable())?;
    let channels = channels.trim().parse::<u32>().map_err(|_| unreadable())?;
    Ok(Some((rate, channels)))
}

/// Refuse heterogeneous input by name (clip id, rate and channel count), rather than let the
/// concat filter silently reinterpret it.
async fn audio_layout(clips: &[Clip], paths: &[PathBuf]) -> Result<(u32, u32), CapabilityError> {
    let (sample_rate, channels) = probe_audio_params(&paths[0]).await?.ok_or_else(|| {
        CapabilityError::Failed(format!("clip {:?} carries no audio stream", clips[0].id))
    })?;
    for (clip, path) in clips.iter().zip(paths).skip(1) {
        match probe_audio_params(path).await? {
            None => {
                return Err(CapabilityError::Failed(format!(
                    "clip {:?} carries no audio stream",
                    clip.id
                )));
            }
            Some(params) if params != (sample_rate, channels) => {
                return Err(CapabilityError::Failed(format!(
                    "heterogeneous audio: clip {:?} is {}Hz/{}ch, expected {sample_rate}Hz/{channels}ch",
                    clip.id, params.0, params.1
                )));
            }
            Some(_) => {}
        }
    }
    Ok((sample_rate, channels))
}

/// Fast path: stream-copy uniform WAV clips through the concat demuxer, so no re-encode happens
/// on the reference's byte-identical legacy path.
async fn concat_copy(paths: &[PathBuf], output: &Path) -> Result<(), CapabilityError> {
    let list_path = output.with_extension("concat.txt");
    let mut list = String::new();
    for path in paths {
        list.push_str("file ");
        list.push_str(&quote_for_concat(path));
        list.push('\n');
    }
    std::fs::write(&list_path, list).map_err(|error| {
        CapabilityError::Failed(format!("cannot write {:?}: {error}", list_path))
    })?;
    let args = vec![
        "-y".to_string(),
        "-f".to_string(),
        "concat".to_string(),
        "-safe".to_string(),
        "0".to_string(),
        "-i".to_string(),
        list_path.display().to_string(),
        "-c".to_string(),
        "copy".to_string(),
        output.display().to_string(),
    ];
    let finished = process::run("ffmpeg", &args).await;
    let _ = std::fs::remove_file(&list_path);
    let finished = finished?;
    if !finished.ok() {
        return Err(CapabilityError::Failed(format!(
            "ffmpeg concat failed: {}",
            finished.tail()
        )));
    }
    Ok(())
}

/// A concat-demuxer path in its own quoting rules: wrap in single quotes, and spell an embedded
/// single quote as `'\''`.
fn quote_for_concat(path: &Path) -> String {
    let raw = path.to_string_lossy();
    format!("'{}'", raw.replace('\'', "'\\''"))
}

/// Re-encode path: one `concat` filtergraph over every clip, with an `anullsrc` input sized from
/// the first clip's rate and layout inserted between clips when `gap_ms` is non-zero.
async fn concat_filter(
    paths: &[PathBuf],
    output: &Path,
    output_format: &str,
    gap_ms: u64,
    sample_rate: u32,
    channels: u32,
) -> Result<(), CapabilityError> {
    let mut args = vec!["-y".to_string()];
    let mut labels = String::new();
    let mut input_count = 0u32;
    let layout = if channels == 1 { "mono" } else { "stereo" };
    let gap_seconds = gap_ms as f64 / 1000.0;

    for (index, path) in paths.iter().enumerate() {
        args.push("-i".to_string());
        args.push(path.display().to_string());
        labels.push_str(&format!("[{input_count}:a]"));
        input_count += 1;
        if gap_ms > 0 && index + 1 < paths.len() {
            args.extend([
                "-f".to_string(),
                "lavfi".to_string(),
                "-t".to_string(),
                format!("{gap_seconds}"),
                "-i".to_string(),
                format!("anullsrc=r={sample_rate}:cl={layout}"),
            ]);
            labels.push_str(&format!("[{input_count}:a]"));
            input_count += 1;
        }
    }

    let filtergraph = format!("{labels}concat=n={input_count}:v=0:a=1[out]");
    args.extend([
        "-filter_complex".to_string(),
        filtergraph,
        "-map".to_string(),
        "[out]".to_string(),
    ]);
    if let Some(codec) = codec_args(output_format) {
        args.extend(codec.iter().map(|token| (*token).to_string()));
    }
    args.push(output.display().to_string());

    let finished = process::run("ffmpeg", &args).await?;
    if !finished.ok() {
        return Err(CapabilityError::Failed(format!(
            "ffmpeg concat failed: {}",
            finished.tail()
        )));
    }
    Ok(())
}

/// The stitched output's duration, re-probed rather than trusted from the request's advisory
/// per-clip seconds (the captured contract's own rule).
async fn probe_duration(path: &Path) -> Result<f64, CapabilityError> {
    let args = vec![
        "-v".to_string(),
        "error".to_string(),
        "-show_entries".to_string(),
        "format=duration".to_string(),
        "-of".to_string(),
        "json".to_string(),
        path.display().to_string(),
    ];
    let finished = process::run("ffprobe", &args).await?;
    if !finished.ok() {
        return Err(CapabilityError::Failed(format!(
            "ffprobe failed on the stitched output: {}",
            finished.tail()
        )));
    }
    let document: Value = serde_json::from_slice(&finished.stdout).map_err(|error| {
        CapabilityError::Failed(format!("ffprobe emitted unreadable JSON: {error}"))
    })?;
    document
        .get("format")
        .and_then(|format| format.get("duration"))
        .and_then(Value::as_str)
        .and_then(|raw| raw.parse::<f64>().ok())
        .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)
        .ok_or_else(|| {
            CapabilityError::Failed(format!(
                "the stitched output {:?} reports no duration",
                path.display().to_string()
            ))
        })
}

fn round_millis(seconds: f64) -> f64 {
    (seconds * 1000.0).round() / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "conveyor-media-concat-{label}-{}",
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

    /// A real one-second mono 24 kHz tone: the reference's own TTS shape (24 kHz mono PCM WAV).
    async fn generate_tone(root: &Path, name: &str, frequency: u32) {
        std::fs::create_dir_all(root).unwrap();
        let args = vec![
            "-y".to_string(),
            "-v".to_string(),
            "error".to_string(),
            "-f".to_string(),
            "lavfi".to_string(),
            "-i".to_string(),
            format!("sine=frequency={frequency}:duration=1"),
            "-ar".to_string(),
            "24000".to_string(),
            "-ac".to_string(),
            "1".to_string(),
            root.join(name).display().to_string(),
        ];
        let finished = process::run("ffmpeg", &args).await.unwrap();
        assert!(
            finished.ok(),
            "could not generate the fixture: {}",
            finished.tail()
        );
    }

    #[tokio::test]
    async fn concatenates_two_generated_clips_in_order() {
        let root = scratch("stitch");
        generate_tone(&root, "clip-0.wav", 440).await;
        generate_tone(&root, "clip-1.wav", 660).await;
        let config = test_config(&root);

        let result = concat_audio(
            &json!({"clips": [
                {"id": "clip-0", "audio_url": "clip-0.wav", "seconds": 1.0},
                {"id": "clip-1", "audio_url": "clip-1.wav"},
            ]}),
            &config,
        )
        .await
        .unwrap();

        // Two one-second clips, in the order given, make a two-second track.
        let duration = result["duration_seconds"].as_f64().unwrap();
        assert!(
            (duration - 2.0).abs() < 0.05,
            "expected a two-second stitch, got {duration}"
        );
        let url = result["artifact_url"].as_str().unwrap();
        let path = PathBuf::from(url.strip_prefix("file://").unwrap());
        assert!(path.is_file(), "{path:?} must exist");
        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("audiobook.wav")
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The captured `AudioConcatResponse` payload shape, with this image's store-free values: the
    /// artifact is a `file://` handle, `bucket` and `acx_report` are null.
    #[test]
    fn the_document_matches_the_captured_audio_concat_response_shape() {
        let document = result_document(Path::new("/tmp/run-1/audiobook.wav"), "run-1", "wav", 2.0);
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
                "acx_report",
                "artifact_url",
                "bucket",
                "duration_seconds",
                "object_key"
            ]
        );
        assert_eq!(
            document["artifact_url"],
            json!("file:///tmp/run-1/audiobook.wav")
        );
        assert_eq!(document["duration_seconds"], json!(2.0));
        assert_eq!(document["object_key"], json!("run-1/audiobook.wav"));
        assert_eq!(document["bucket"], Value::Null);
        assert_eq!(document["acx_report"], Value::Null);
    }

    #[test]
    fn every_captured_output_format_has_its_codec_arguments() {
        assert_eq!(codec_args("wav"), Some(&["-c:a", "pcm_s16le"] as &[&str]));
        assert!(codec_args("mp3").is_some());
        assert!(codec_args("m4b").is_some());
        assert_eq!(codec_args("flac"), None);
    }

    #[tokio::test]
    async fn an_acx_master_request_is_refused_by_name() {
        let root = scratch("acx");
        let config = test_config(&root);
        let error = concat_audio(
            &json!({
                "clips": [{"id": "clip-0", "audio_url": "clip-0.wav"}],
                "acx_master": {"enabled": true, "lufs_target": -20},
            }),
            &config,
        )
        .await
        .unwrap_err();
        assert!(
            error.to_string().contains("acx_master_not_supported"),
            "{error}"
        );
    }

    #[tokio::test]
    async fn m4b_chapters_are_refused_by_name() {
        let root = scratch("chapters");
        let config = test_config(&root);
        let error = concat_audio(
            &json!({
                "clips": [{"id": "clip-0", "audio_url": "clip-0.wav"}],
                "output_format": "m4b",
                "chapters": [{"title": "One", "start_ms": 0}],
            }),
            &config,
        )
        .await
        .unwrap_err();
        assert!(
            error.to_string().contains("chapters_not_supported"),
            "{error}"
        );
    }

    #[tokio::test]
    async fn a_remote_audio_url_is_refused_by_name() {
        let root = scratch("remote");
        let config = test_config(&root);
        let error = concat_audio(
            &json!({"clips": [{"id": "clip-0", "audio_url": "https://example.invalid/clip.wav"}]}),
            &config,
        )
        .await
        .unwrap_err();
        assert!(
            error.to_string().contains("remote_input_not_supported"),
            "{error}"
        );
    }

    /// The captured 422 names the missing field as `clips: Field required`; the port's validation
    /// failure must name the same field.
    #[tokio::test]
    async fn a_missing_clips_field_names_the_captured_field() {
        let root = scratch("missing");
        let config = test_config(&root);
        let error = concat_audio(&json!({}), &config).await.unwrap_err();
        assert!(
            error.to_string().contains("clips: Field required"),
            "{error}"
        );
    }
}
