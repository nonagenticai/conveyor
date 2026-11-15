//! The deck toolkit: one action that turns the deck model into an artefact (issue #161 P3).
//!
//! A project reaches a deck through the broker, never through a library call. `decks.render`
//! takes the [`conveyor_deck::Deck`] model and renders it to OOXML (`.pptx`), PDF, or a ZIP of
//! one PNG per slide, and returns the artefact inline as base64 with its hash so a caller can
//! verify what it received.
//!
//! # Why the artefact rides inside the JSON document
//!
//! An action's output is JSON (CONTRACT §6), and the same declaration is the OpenAPI operation
//! and the MCP tool. A `.pptx` is not JSON, so it travels as `base64` beside its `media_type`,
//! `bytes` and `sha256` — the shape `models.render` and `documents.download` already use. The
//! inline form is deliberately capped at [`MAX_INLINE_BYTES`]: past that the right route is the
//! documents surface, which owns object storage and a URL, not an invocation body.
//!
//! # The base directory
//!
//! `to_pptx_in`/`to_pdf`/`to_pngs` resolve an image slide's relative path against a base
//! directory. That is correct here — the handler runs inside the server process, which owns the
//! job's work directory — so image layouts are served, not refused. What a caller may not do is
//! aim that read at the host filesystem: `base` must be `.` or start with `./`, or be an
//! absolute directory inside the server's working directory. `/etc` and any `..` segment are
//! refused on the `base` field before the deck is touched.
//!
//! # Refusals
//!
//! Every [`conveyor_deck::DeckError`] becomes `invalid_input` on the `deck` field: the model's
//! own [`conveyor_deck::DeckError::code`] is the field code and its message (which names the
//! slide and field for a layout refusal) is the detail. Nothing is written before the model
//! validates, so a refused deck leaves no row, no job and no artefact.

use std::path::{Path, PathBuf};

use conveyor_deck::{Deck, DeckError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;

use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::{FieldError, sha256_hex};

/// The action's name, as the broker and the registry spell it.
pub const DECKS_RENDER: &str = "decks.render";

/// The largest artefact this action will carry inline, in bytes.
///
/// Past it the caller is pointed at the documents route: an invocation body is not an object
/// store, and a 4 MiB deck encoded as base64 is a ~5.4 MiB request/response cycle nobody planned
/// for.
pub const MAX_INLINE_BYTES: usize = 256 * 1024;

/// The `.pptx` MIME type.
const PPTX_MEDIA_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.presentationml.presentation";
/// A PDF document.
const PDF_MEDIA_TYPE: &str = "application/pdf";
/// A ZIP archive — the shape a `png` request returns, one entry per slide.
const ZIP_MEDIA_TYPE: &str = "application/zip";

/// The three artefacts the deck model renders to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum DeckFormat {
    /// A PowerPoint OOXML package, written by `conveyor-deck` itself.
    Pptx,
    /// One PDF page per slide.
    Pdf,
    /// A ZIP of one PNG per slide.
    Png,
}

impl DeckFormat {
    /// The wire name, exactly as the input carried it.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pptx => "pptx",
            Self::Pdf => "pdf",
            Self::Png => "png",
        }
    }

    /// The MIME type of the artefact this format produces.
    pub fn media_type(self) -> &'static str {
        match self {
            Self::Pptx => PPTX_MEDIA_TYPE,
            Self::Pdf => PDF_MEDIA_TYPE,
            Self::Png => ZIP_MEDIA_TYPE,
        }
    }
}

/// The input: one deck and the format to render it to.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeckRenderIn {
    /// The deck model, exactly as `conveyor-deck` deserializes it.
    pub deck: Deck,
    /// The artefact to produce.
    pub format: DeckFormat,
    /// The directory an image slide's relative path resolves against.
    #[serde(default = "default_base")]
    pub base: String,
}

/// `.` — the process working directory, which is the job's work directory in the server.
fn default_base() -> String {
    ".".to_owned()
}

/// The rendered artefact: its bytes inline, with the identity a caller can verify them by.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DeckRenderOut {
    /// `pptx`, `pdf` or `png`, exactly as requested.
    pub format: String,
    /// The artefact's MIME type. A `png` request is a ZIP of PNGs.
    pub media_type: String,
    /// The artefact's length before base64, in bytes.
    pub bytes: u64,
    /// SHA-256 of the artefact, lowercase hex.
    pub sha256: String,
    /// The number of slides rendered — the PDF's page count and the PNG ZIP's entry count.
    pub pages: u64,
    /// The artefact itself, standard base64 (padded).
    pub base64: String,
}

/// Render one deck to the requested artefact.
#[crate::action(
    name = "decks.render",
    version = 1,
    kind = "internal",
    scopes = ["capabilities:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn decks_render(
    _tx: &mut PgConnection,
    _ctx: HandlerCtx,
    input: DeckRenderIn,
) -> Result<DeckRenderOut, ActionError> {
    render_deck(&input)
}

/// The handler's whole body, split out so the tests can render without a database connection.
fn render_deck(input: &DeckRenderIn) -> Result<DeckRenderOut, ActionError> {
    use base64::Engine as _;

    let base = resolve_base(&input.base)?;
    let artefact = match input.format {
        DeckFormat::Pptx => conveyor_deck::to_pptx_in(&input.deck, &base),
        DeckFormat::Pdf => conveyor_deck::to_pdf(&input.deck, &base),
        DeckFormat::Png => png_archive(&input.deck, &base),
    }
    .map_err(deck_error)?;

    if artefact.len() > MAX_INLINE_BYTES {
        return Err(ActionError::invalid_input(vec![FieldError {
            field: "deck".to_owned(),
            code: "too_large".to_owned(),
            detail: format!(
                "the rendered deck is {} bytes; the inline cap is {MAX_INLINE_BYTES} — post it to \
                 the documents route instead",
                artefact.len()
            ),
        }]));
    }

    Ok(DeckRenderOut {
        format: input.format.as_str().to_owned(),
        media_type: input.format.media_type().to_owned(),
        bytes: artefact.len() as u64,
        sha256: sha256_hex(&artefact),
        pages: input.deck.slides.len() as u64,
        base64: base64::engine::general_purpose::STANDARD.encode(&artefact),
    })
}

/// Package the deck's PNGs with `conveyor-deck`'s own stored-ZIP writer.
///
/// Entry names are `slide-1.png`, `slide-2.png`, … in presentation order. Using the deck crate's
/// writer rather than a zip dependency keeps one deterministic archive implementation in the
/// workspace.
fn png_archive(deck: &Deck, base: &Path) -> Result<Vec<u8>, DeckError> {
    let pages = conveyor_deck::to_pngs(deck, base)?;
    let names: Vec<String> = (1..=pages.len())
        .map(|number| format!("slide-{number}.png"))
        .collect();
    let parts: Vec<(&str, &[u8])> = names
        .iter()
        .zip(&pages)
        .map(|(name, png)| (name.as_str(), png.as_slice()))
        .collect();
    conveyor_deck::zip_parts(&parts)
}

/// Map the deck model's typed refusal onto the action's field vocabulary.
///
/// The model's own slug becomes the field code, so `empty_bullets` in a problem document is
/// directly traceable to `DeckError::EmptyBullets`; its message already names the slide and the
/// field.
fn deck_error(error: DeckError) -> ActionError {
    ActionError::invalid_input(vec![FieldError {
        field: "deck".to_owned(),
        code: error.code().to_owned(),
        detail: error.to_string(),
    }])
}

/// The `base` refusal: a shape check plus confinement to the server's working directory.
///
/// The base exists so a project's image files can be read from the job's own work directory.
/// The rule keeps that intent while refusing the two ways a caller could aim it at the host:
///
/// - any `..` segment, checked for both separators so the refusal does not depend on the
///   platform reading it;
/// - an absolute path outside the process working directory (`/etc`), and a relative path that
///   is not `.`-anchored (`work`), because neither is a directory the server handed this job.
fn resolve_base(raw: &str) -> Result<PathBuf, ActionError> {
    let raw = raw.trim();
    let refuse = |detail: &str| {
        ActionError::invalid_input(vec![FieldError {
            field: "base".to_owned(),
            code: "invalid_value".to_owned(),
            detail: detail.to_owned(),
        }])
    };
    if raw.is_empty() {
        return Err(refuse(
            "must not be empty; use `.` for the job's work directory",
        ));
    }
    if raw.split(['/', '\\']).any(|segment| segment == "..") {
        return Err(refuse(
            "must not contain a `..` segment; image paths resolve inside the base",
        ));
    }

    let path = Path::new(raw);
    if path.is_absolute() {
        let working_directory = std::env::current_dir().map_err(|_| ActionError::internal())?;
        if !path.starts_with(&working_directory) {
            return Err(refuse(
                "an absolute base must be inside the server's working directory; use `.` or a \
                 `./…` path for the job's work directory",
            ));
        }
        return Ok(path.to_path_buf());
    }
    if raw == "." || raw.starts_with("./") || raw.starts_with(".\\") {
        return Ok(path.to_path_buf());
    }
    Err(refuse(
        "must be `.`, a `./…` path, or an absolute directory inside the server's working \
         directory",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::{ActionKind, Effect};
    use base64::Engine as _;
    use conveyor_deck::Slide;

    fn deck() -> Deck {
        Deck {
            title: "Quarterly".to_owned(),
            slides: vec![
                Slide::Title {
                    title: "Opening".to_owned(),
                    subtitle: Some("Synthetic".to_owned()),
                },
                Slide::Bullets {
                    title: Some("Findings".to_owned()),
                    bullets: vec!["one".to_owned(), "two".to_owned()],
                },
            ],
        }
    }

    fn request(deck: Deck, format: DeckFormat) -> DeckRenderIn {
        DeckRenderIn {
            deck,
            format,
            base: ".".to_owned(),
        }
    }

    /// Decode the inline artefact, checking it is exactly the bytes the report describes.
    fn artefact(out: &DeckRenderOut) -> Vec<u8> {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&out.base64)
            .expect("the artefact is base64");
        assert_eq!(bytes.len() as u64, out.bytes, "the reported length");
        assert_eq!(sha256_hex(&bytes), out.sha256, "the reported hash");
        bytes
    }

    /// Walk an archive's local headers: the stored entries, in order.
    fn entries(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
        const LOCAL_FILE_HEADER_SIGNATURE: u32 = 0x0403_4b50;
        let u16_at = |at: usize| u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap());
        let u32_at = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
        let mut out = Vec::new();
        let mut at = 0;
        while at + 30 <= bytes.len() && u32_at(at) == LOCAL_FILE_HEADER_SIGNATURE {
            let size = u32_at(at + 18) as usize;
            let name_len = u16_at(at + 26) as usize;
            let extra = u16_at(at + 28) as usize;
            let name_at = at + 30;
            let data_at = name_at + name_len + extra;
            let name = std::str::from_utf8(&bytes[name_at..name_at + name_len])
                .expect("entry names are UTF-8")
                .to_owned();
            let data = bytes[data_at..data_at + size].to_vec();
            out.push((name, data));
            at = data_at + size;
        }
        out
    }

    /// A PDF page count: `svg2pdf` writes one `/Type /Page` per page and one `/Type /Pages`
    /// dictionary for the tree.
    fn pdf_pages(pdf: &[u8]) -> usize {
        let text = String::from_utf8_lossy(pdf);
        text.matches("/Type /Page").count() - text.matches("/Type /Pages").count()
    }

    #[test]
    fn a_two_slide_deck_renders_to_pptx_with_its_part_tree() {
        let out = render_deck(&request(deck(), DeckFormat::Pptx)).expect("the deck writes");
        assert_eq!(out.format, "pptx");
        assert_eq!(out.media_type, PPTX_MEDIA_TYPE);
        assert_eq!(out.pages, 2);

        let package = artefact(&out);
        assert!(
            package.starts_with(&[0x50, 0x4b, 0x03, 0x04]),
            "the .pptx is a ZIP"
        );
        let names: Vec<String> = entries(&package)
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        for part in [
            "[Content_Types].xml",
            "ppt/presentation.xml",
            "ppt/slides/slide1.xml",
            "ppt/slides/slide2.xml",
        ] {
            assert!(names.iter().any(|name| name == part), "missing {part}");
        }
    }

    #[test]
    fn a_two_slide_deck_renders_to_pdf_with_one_page_per_slide() {
        let out = render_deck(&request(deck(), DeckFormat::Pdf)).expect("the deck renders");
        assert_eq!(out.format, "pdf");
        assert_eq!(out.media_type, PDF_MEDIA_TYPE);
        assert_eq!(out.pages, 2);

        let pdf = artefact(&out);
        assert!(pdf.starts_with(b"%PDF-"), "the PDF header");
        assert_eq!(pdf_pages(&pdf), 2, "one page per slide");
    }

    #[test]
    fn a_two_slide_deck_renders_to_a_zip_with_one_png_per_slide() {
        let out = render_deck(&request(deck(), DeckFormat::Png)).expect("the deck renders");
        assert_eq!(out.format, "png");
        assert_eq!(out.media_type, ZIP_MEDIA_TYPE);
        assert_eq!(out.pages, 2);

        let archive = artefact(&out);
        assert!(
            archive.starts_with(&[0x50, 0x4b, 0x03, 0x04]),
            "the PNGs are a ZIP"
        );
        let entries = entries(&archive);
        let names: Vec<&str> = entries.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["slide-1.png", "slide-2.png"], "one entry per slide");
        for (name, png) in &entries {
            assert!(
                png.starts_with(&[0x89, b'P', b'N', b'G']),
                "{name} is a PNG"
            );
        }
    }

    #[test]
    fn a_refusal_names_the_slide_and_the_decks_own_code() {
        let broken = Deck {
            title: "Quarterly".to_owned(),
            slides: vec![
                Slide::Title {
                    title: "Opening".to_owned(),
                    subtitle: None,
                },
                Slide::Bullets {
                    title: None,
                    bullets: Vec::new(),
                },
            ],
        };
        let error = render_deck(&request(broken, DeckFormat::Pptx))
            .expect_err("an empty bullet list is refused");
        assert_eq!(error.status, 422);
        let field = &error.errors[0];
        assert_eq!(field.field, "deck");
        assert_eq!(field.code, "empty_bullets", "the model's own slug");
        assert!(field.detail.contains("slide 2"), "{}", field.detail);
        assert!(field.detail.contains("bullets"), "{}", field.detail);
    }

    #[test]
    fn an_artefact_over_the_inline_cap_is_refused_by_size() {
        // One hundred slides of eight maximum-length bullets: comfortably past 256 KiB once the
        // OOXML parts are written, and every field stays inside the model's own limits.
        let large = Deck {
            title: "Large".to_owned(),
            slides: (0..100)
                .map(|_| Slide::Bullets {
                    title: Some("T".repeat(conveyor_deck::MAX_TEXT_CHARS)),
                    bullets: (0..8)
                        .map(|_| "B".repeat(conveyor_deck::MAX_TEXT_CHARS))
                        .collect(),
                })
                .collect(),
        };
        let error = render_deck(&request(large, DeckFormat::Pptx))
            .expect_err("an artefact over the cap is refused");
        assert_eq!(error.status, 422);
        let field = &error.errors[0];
        assert_eq!(field.field, "deck");
        assert_eq!(field.code, "too_large");
        assert!(
            field.detail.starts_with("the rendered deck is "),
            "{}",
            field.detail
        );
        assert!(field.detail.contains("262144"), "{}", field.detail);
        assert!(
            field
                .detail
                .contains("post it to the documents route instead"),
            "{}",
            field.detail
        );
    }

    #[test]
    fn a_base_that_escapes_is_refused_before_the_deck_is_read() {
        for base in ["/etc", "..", "a/../b"] {
            let mut input = request(deck(), DeckFormat::Pptx);
            input.base = base.to_owned();
            let error = render_deck(&input).expect_err("the base is refused");
            assert_eq!(error.status, 422, "{base}");
            assert_eq!(error.errors[0].field, "base", "{base}");
        }
    }

    #[test]
    fn the_action_is_registered_as_an_internal_write() {
        let spec = crate::registry::registry()
            .get(DECKS_RENDER)
            .expect("decks.render is registered");
        assert_eq!(spec.kind, ActionKind::Internal);
        assert_eq!(spec.effect, Effect::Write);
        assert!(spec.idempotent, "a replay of the same deck writes once");
        assert_eq!(spec.scope_names(), vec!["capabilities:write"]);
    }
}
