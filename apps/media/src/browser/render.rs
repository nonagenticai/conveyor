//! `media.render_html` — an HTML document to a PDF or a PNG.
//!
//! This is the browser image's first capability and the one every deck, report and slide artefact
//! ultimately rides on. It drives the image's own Chromium in headless mode:
//!
//! * PDF is the browser's own print path (`--print-to-pdf`), the same engine
//!   `html_render_service` used through Playwright's `page.pdf()`.
//! * PNG is a viewport screenshot (`--screenshot`), sized by the caller.
//! * `--virtual-time-budget` is how a document is given time to settle (fonts, images, script)
//!   without a Playwright `wait_until` vocabulary. A caller that asks for one of those
//!   Playwright-specific wait modes is **refused by name** rather than silently ignored — the
//!   `apps/research` precedent, and the difference between a declared gap and a lie.
//!
//! No network egress is needed for a self-contained document, and the HTML is written into the
//! job's own work directory, so a `file://` load cannot reach a path the job was not given.

use std::path::PathBuf;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::common::{CapabilityError, Config, process};

/// The kind slug this module implements.
pub const KIND: &str = "media.render_html";

/// The browser binary. The image installs Chromium at `/usr/bin/chromium`; a developer box points
/// this at whatever Chrome it has.
const CHROME_ENV: &str = "CHROME_BIN";
const CHROME_DEFAULT: &str = "/usr/bin/chromium";

/// Render `params` into `config.out_dir`.
pub async fn render(params: &Value, config: &Config) -> Result<Value, CapabilityError> {
    let format = params
        .get("format")
        .and_then(Value::as_str)
        .unwrap_or("pdf")
        .to_ascii_lowercase();
    if format != "pdf" && format != "png" {
        return Err(CapabilityError::Invalid(format!(
            "format must be \"pdf\" or \"png\", not {format:?}"
        )));
    }

    // Playwright's wait vocabulary has no faithful equivalent in the headless CLI. Refusing it by
    // name is the difference between a declared gap and a silent one.
    if let Some(wait_until) = params.get("wait_until").and_then(Value::as_str) {
        return Err(CapabilityError::refused(
            "wait_until_not_supported",
            format!(
                "wait_until={wait_until:?} is a Playwright runner mode with no headless equivalent; \
                 use wait_ms instead"
            ),
        ));
    }

    // The pod has no egress (ADR 0007 §3: a media job reaches nothing but conveyor's own routes).
    // A caller naming a URL is refused by name rather than left to fail at Chromium with a DNS
    // error that reads like a transient fault.
    if params.get("url").is_some() {
        return Err(CapabilityError::refused(
            "url_not_supported",
            "a media job has no network egress; pass the document as html or as an input file",
        ));
    }

    let width = params.get("width").and_then(Value::as_u64).unwrap_or(1280);
    let height = params.get("height").and_then(Value::as_u64).unwrap_or(720);
    let wait_ms = params.get("wait_ms").and_then(Value::as_u64).unwrap_or(0);
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("render");

    std::fs::create_dir_all(&config.out_dir).map_err(|error| {
        CapabilityError::Failed(format!("cannot create {:?}: {error}", config.out_dir))
    })?;
    let output: PathBuf = config.out_dir.join(format!("{name}.{format}"));

    // The document: caller-supplied HTML (written into the work dir) or a file the job was given.
    let document = match (
        params.get("html").and_then(Value::as_str),
        params.get("input").and_then(Value::as_str),
    ) {
        (Some(html), None) => {
            // The captured reference refuses a document of zero characters (`html: String should
            // have at least 1 characters`, 422). An empty file renders a blank page, which would
            // turn that refusal into a silent success and a PDF nobody asked for.
            if html.is_empty() {
                return Err(CapabilityError::Invalid(
                    "html must have at least 1 character".to_string(),
                ));
            }
            let path = config.work_dir.join(format!("{name}.html"));
            std::fs::create_dir_all(&config.work_dir).map_err(|error| {
                CapabilityError::Failed(format!("cannot create {:?}: {error}", config.work_dir))
            })?;
            std::fs::write(&path, html).map_err(|error| {
                CapabilityError::Failed(format!("cannot write {:?}: {error}", path))
            })?;
            path
        }
        (None, Some(input)) => process::resolve_input(&config.work_dir, input)?,
        (Some(_), Some(_)) => {
            return Err(CapabilityError::Invalid(
                "pass exactly one of html or input".to_string(),
            ));
        }
        (None, None) => {
            return Err(CapabilityError::Invalid(
                "one of html or input is required".to_string(),
            ));
        }
    };

    let chrome = std::env::var(CHROME_ENV).unwrap_or_else(|_| CHROME_DEFAULT.to_string());
    // `--headless=new` is load-bearing: the legacy `--headless` mode hangs when another Chrome
    // owns the default profile, and a per-job `--user-data-dir` is what makes two concurrent
    // renders independent (measured on this box 2026-11-15: legacy mode did not exit at all).
    let profile = config.work_dir.join(format!("{name}.profile"));
    let mut args: Vec<String> = vec![
        "--headless=new".to_string(),
        "--disable-gpu".to_string(),
        "--no-sandbox".to_string(),
        "--no-first-run".to_string(),
        "--disable-extensions".to_string(),
        "--hide-scrollbars".to_string(),
        "--disable-dev-shm-usage".to_string(),
        format!("--user-data-dir={}", profile.display()),
    ];
    // Only when the caller asked to wait: `--virtual-time-budget=0` makes Chrome wait for a budget
    // that never advances, which is a hang rather than an immediate render (measured 2026-11-15).
    if wait_ms > 0 {
        args.push(format!("--virtual-time-budget={wait_ms}"));
    }
    if format == "pdf" {
        args.push(format!("--print-to-pdf={}", output.display()));
    } else {
        args.push(format!("--window-size={width},{height}"));
        args.push(format!("--screenshot={}", output.display()));
    }
    args.push(format!("file://{}", document.display()));

    let finished = process::run(&chrome, &args).await?;
    if !finished.ok() {
        return Err(CapabilityError::Failed(format!(
            "the browser failed: {}",
            finished.tail()
        )));
    }
    let bytes = std::fs::read(&output).map_err(|error| {
        CapabilityError::Failed(format!(
            "the browser reported success but wrote no {format} at {:?}: {error}",
            output
        ))
    })?;
    if bytes.is_empty() {
        return Err(CapabilityError::Failed(format!(
            "the browser wrote an empty {format}"
        )));
    }

    Ok(json!({
        "output": output.file_name().and_then(|n| n.to_str()).unwrap_or_default(),
        "format": format,
        "bytes": bytes.len(),
        "sha256": hex(&Sha256::digest(&bytes)),
        "width": width,
        "height": height,
    }))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_playwright_wait_mode_is_refused_by_name() {
        let params = json!({"html": "<p>x</p>", "wait_until": "networkidle"});
        let config = Config {
            service: "test".into(),
            work_dir: std::env::temp_dir(),
            out_dir: std::env::temp_dir(),
        };
        let error = render(&params, &config).await.unwrap_err();
        assert!(
            error.to_string().contains("wait_until_not_supported"),
            "{error}"
        );
    }

    #[tokio::test]
    async fn an_empty_document_is_refused_like_the_captured_reference() {
        // The capture pins `html: String should have at least 1 characters` (422). A blank page
        // rendered successfully would be the silent-success version of the same request.
        let params = json!({"html": ""});
        let config = Config {
            service: "test".into(),
            work_dir: std::env::temp_dir(),
            out_dir: std::env::temp_dir(),
        };
        let error = render(&params, &config).await.unwrap_err();
        assert!(
            error.to_string().contains("at least 1 character"),
            "{error}"
        );
    }

    #[tokio::test]
    async fn an_unknown_format_is_invalid() {
        let params = json!({"html": "<p>x</p>", "format": "docx"});
        let config = Config {
            service: "test".into(),
            work_dir: std::env::temp_dir(),
            out_dir: std::env::temp_dir(),
        };
        let error = render(&params, &config).await.unwrap_err();
        assert!(error.to_string().contains("format must be"), "{error}");
    }
}
