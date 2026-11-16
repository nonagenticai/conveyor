//! The captured contracts, and the check that an image still covers them.
//!
//! Each reference service's OpenAPI document is kept verbatim as **evidence** — it is what the
//! capture produced and what the drop table is argued against. What a media image is checked on is
//! different in kind: an executor does not serve routes, it implements **capability kinds**
//! (`media.render_html`, `media.compile_video`, …). So each captured service also carries
//! `capabilities.txt`, the kinds it owns *after* the drop table, and the check compares that list
//! with the kinds an image declares it implements.
//!
//! The direction is load-bearing — a list regenerated from the implementation cannot fail for the
//! reason the check exists.

use std::collections::BTreeSet;

use thiserror::Error;

/// One captured reference.
#[derive(Debug, Clone, Copy)]
pub struct Capture {
    /// The reference's name, as the contract directory spells it.
    pub service: &'static str,
    /// The captured OpenAPI 3.1 document (evidence; not compared).
    pub lock_json: &'static str,
    /// `kind` slugs this service owns after the drop table, one per line, `#` comments allowed.
    pub capabilities: &'static str,
}

#[derive(Debug, Error)]
pub enum LockError {
    /// An image with no embedded capture cannot claim coverage. Without this arm two empty lists
    /// would compare equal and report green — the one result that must never be free.
    #[error("no captured capability is embedded: this image cannot claim coverage yet")]
    Uncaptured,
    #[error(
        "the image drifted from the capture: {missing} captured kind(s) are not implemented, \
         {extra} kind(s) are implemented but appear in no capture\n  missing: {missing_list}\n  \
         extra: {extra_list}"
    )]
    Drifted {
        missing: usize,
        extra: usize,
        missing_list: String,
        extra_list: String,
    },
}

/// What a green check reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Summary {
    pub services: usize,
    pub kinds: usize,
}

/// The `kind` slugs a capture declares, comments and blanks dropped.
pub fn captured_kinds(capture: &Capture) -> BTreeSet<String> {
    capture
        .capabilities
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect()
}

/// The union of every capture's kinds.
pub fn captured_union(captures: &[Capture]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for capture in captures {
        out.extend(captured_kinds(capture));
    }
    out
}

/// `"METHOD path"` for every operation in a captured document — the evidence view.
pub fn operations(document: &serde_json::Value) -> BTreeSet<String> {
    const METHODS: [&str; 7] = ["get", "put", "post", "delete", "options", "head", "patch"];
    let mut out = BTreeSet::new();
    let Some(paths) = document.get("paths").and_then(serde_json::Value::as_object) else {
        return out;
    };
    for (path, item) in paths {
        let Some(item) = item.as_object() else {
            continue;
        };
        for method in METHODS {
            if item.contains_key(method) {
                out.insert(format!("{} {path}", method.to_ascii_uppercase()));
            }
        }
    }
    out
}

/// Check an image's kinds against its captures, in both directions.
pub fn check(captures: &[Capture], kinds: &[&str]) -> Result<Summary, LockError> {
    if captures.is_empty() || captures.iter().all(|c| captured_kinds(c).is_empty()) {
        return Err(LockError::Uncaptured);
    }
    let want = captured_union(captures);
    let have: BTreeSet<String> = kinds.iter().map(|k| (*k).to_string()).collect();

    let missing: Vec<&String> = want.difference(&have).collect();
    let extra: Vec<&String> = have.difference(&want).collect();
    if !missing.is_empty() || !extra.is_empty() {
        return Err(LockError::Drifted {
            missing: missing.len(),
            extra: extra.len(),
            missing_list: missing
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            extra_list: extra
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", "),
        });
    }
    Ok(Summary {
        services: captures.len(),
        kinds: want.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAPTURE: Capture = Capture {
        service: "x",
        lock_json: r#"{"paths":{"/a":{"get":{}}}}"#,
        capabilities: "# comment\nmedia.render_html\n\nmedia.compile_video\n",
    };

    #[test]
    fn comments_and_blanks_are_dropped() {
        let kinds = captured_kinds(&CAPTURE);
        assert_eq!(kinds.len(), 2);
        assert!(kinds.contains("media.render_html"));
    }

    #[test]
    fn a_missing_kind_is_drift() {
        let err = check(&[CAPTURE], &["media.render_html"]).unwrap_err();
        assert!(err.to_string().contains("1 captured kind"), "{err}");
    }

    #[test]
    fn an_extra_kind_is_drift() {
        let err = check(
            &[CAPTURE],
            &["media.render_html", "media.compile_video", "media.invented"],
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("1 kind(s) are implemented"),
            "{err}"
        );
    }

    #[test]
    fn a_complete_image_is_green() {
        let summary = check(&[CAPTURE], &["media.render_html", "media.compile_video"]).unwrap();
        assert_eq!(
            summary,
            Summary {
                services: 1,
                kinds: 2
            }
        );
    }

    #[test]
    fn an_empty_capture_never_reports_green() {
        let empty = Capture {
            service: "y",
            lock_json: "{}",
            capabilities: "",
        };
        assert!(matches!(check(&[empty], &[]), Err(LockError::Uncaptured)));
    }
}
