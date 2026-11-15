//! `media.ocr` — text out of an image, through the image's own tesseract.
//!
//! This kind is **new in conveyor**, not a ported operation. The reference's `extract_ocr` stage
//! runs through the vision provider behind `inference-router` (a licence-gated external binding per
//! ADR 0006 §Consequences), so no reference route and no golden evidence a local OCR; ADR 0007 §1
//! nevertheless puts tesseract in the ffmpeg image, and this is that capability. The parity table
//! records it as published-new for that reason.
//!
//! The contract is deliberately small and closed: an image in, text out. Anything a caller asks for
//! that tesseract cannot honour (diagram structure, bounding boxes per word with confidence) is
//! refused by name rather than approximated.

use serde_json::{Value, json};

use crate::common::{CapabilityError, Config, process};

/// The kind slug this module implements.
pub const KIND: &str = "media.ocr";

/// The languages this build accepts. The image installs `eng` and `osd` only; a caller asking for
/// another language is refused rather than handed a silently empty result.
const LANGS: &[&str] = &["eng"];

/// Read the text an image carries.
pub async fn ocr(params: &Value, config: &Config) -> Result<Value, CapabilityError> {
    let input = params
        .get("input")
        .and_then(Value::as_str)
        .ok_or_else(|| CapabilityError::Invalid("input is required".to_string()))?;

    let lang = params.get("lang").and_then(Value::as_str).unwrap_or("eng");
    if !LANGS.contains(&lang) {
        return Err(CapabilityError::refused(
            "unsupported_language",
            format!(
                "lang={lang:?} is not installed in this image; available: {}",
                LANGS.join(", ")
            ),
        ));
    }

    // Page segmentation mode 3 is tesseract's "fully automatic, no orientation" default. A caller
    // with a single line of text gets mode 7; both are tesseract's own vocabulary, not a new one.
    let psm = match params.get("psm").and_then(Value::as_u64) {
        None => 3,
        Some(mode @ (3 | 6 | 7 | 11 | 12 | 13)) => mode as u32,
        Some(other) => {
            return Err(CapabilityError::refused(
                "unsupported_psm",
                format!("psm={other} is not in this port's vocabulary (3, 6, 7, 11, 12, 13)"),
            ));
        }
    };

    if let Some(field) = ["boxes", "confidence", "hocr"]
        .iter()
        .find(|field| params.get(**field).is_some())
    {
        return Err(CapabilityError::refused(
            "text_only",
            format!("{field} is not produced by this kind; it answers text, one line per element"),
        ));
    }

    // Resolve the file only after every parameter has been judged: a refusal must name the field,
    // never report a missing file for a request that was going to be refused anyway.
    let path = process::resolve_input(&config.work_dir, input)?;

    let args = vec![
        path.display().to_string(),
        "stdout".to_string(),
        "-l".to_string(),
        lang.to_string(),
        "--psm".to_string(),
        psm.to_string(),
    ];
    let finished = process::run("tesseract", &args).await?;
    if !finished.ok() {
        return Err(CapabilityError::Failed(format!(
            "tesseract failed: {}",
            finished.tail()
        )));
    }
    Ok(summarise(&String::from_utf8_lossy(&finished.stdout)))
}

/// The stable subset of tesseract's answer: the text, its lines, and a count.
///
/// Trailing blank lines are dropped and a line is trimmed, because tesseract pads its stdout and a
/// caller comparing two runs of the same image must not see that padding as a difference.
pub fn summarise(text: &str) -> Value {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.trim().is_empty())
        .collect();
    let text = lines.join("\n");
    let words = text.split_whitespace().count();
    json!({
        "text": text,
        "lines": lines,
        "words": words,
    })
}

/// Whether the image carries tesseract at all — the probe behind a `tool_missing` refusal.
pub fn available() -> bool {
    process::available("tesseract")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_padded_answer_is_trimmed_and_counted() {
        let document = summarise("hello world  \n\n  second line\n\n");
        assert_eq!(document["text"], "hello world\n  second line");
        assert_eq!(document["lines"].as_array().unwrap().len(), 2);
        assert_eq!(document["words"], 4);
    }

    #[test]
    fn an_empty_answer_is_empty_not_null() {
        let document = summarise("  \n\n");
        assert_eq!(document["text"], "");
        assert_eq!(document["words"], 0);
        assert!(document["lines"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn an_uninstalled_language_is_refused_by_name() {
        let config = Config::from_env("test");
        let error = ocr(&json!({"input": "nope.png", "lang": "deu"}), &config)
            .await
            .expect_err("an uninstalled language must be refused");
        match error {
            CapabilityError::Refused { code, .. } => assert_eq!(code, "unsupported_language"),
            other => panic!("expected a typed refusal, got {other}"),
        }
    }

    #[tokio::test]
    async fn a_bbox_request_is_refused_by_name() {
        let config = Config::from_env("test");
        let error = ocr(&json!({"input": "nope.png", "boxes": true}), &config)
            .await
            .expect_err("boxes are not produced by this kind");
        match error {
            CapabilityError::Refused { code, .. } => assert_eq!(code, "text_only"),
            other => panic!("expected a typed refusal, got {other}"),
        }
    }

    #[tokio::test]
    async fn a_real_image_yields_its_text() {
        if !available() {
            eprintln!("tesseract is not installed on this host; skipping the end-to-end case");
            return;
        }
        let dir = std::env::temp_dir().join(format!("media-ocr-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        // A white image with black text, rendered by ImageMagick if present; otherwise ffmpeg's
        // drawtext filter, which is the same tesseract input shape.
        let image = dir.join("text.png");
        let mut arguments: Vec<String> = [
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=white:s=640x200:d=1",
            "-vf",
            "drawtext=text='CONVEYOR OCR':fontsize=48:fontcolor=black:x=20:y=80",
            "-frames:v",
            "1",
            "-y",
        ]
        .into_iter()
        .map(str::to_string)
        .collect();
        arguments.push(image.display().to_string());
        let built = process::run("ffmpeg", &arguments).await;
        if !matches!(built, Ok(finished) if finished.ok()) {
            eprintln!("ffmpeg could not build the fixture; skipping the end-to-end case");
            return;
        }
        let config = Config {
            service: "test".to_string(),
            work_dir: dir.clone(),
            out_dir: dir.clone(),
        };
        let document = ocr(&json!({"input": "text.png"}), &config)
            .await
            .expect("a readable image must be read");
        let text = document["text"]
            .as_str()
            .unwrap_or_default()
            .to_ascii_uppercase();
        assert!(
            text.contains("CONVEYOR") || text.contains("OCR"),
            "tesseract read {text:?}"
        );
    }
}
