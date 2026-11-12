#![forbid(unsafe_code)]
//! Pure model → SVG / PNG / PDF / XMI (S08 Part B2).
//!
//! It depends on `domain` and on pure libraries — rendering never opens a connection, reads a
//! clock, touches the filesystem or looks at the environment. Given the same
//! [`conveyor_domain::model::ModelSnapshot`] and view it produces the same bytes, on any
//! machine, in any process order.
//!
//! # The determinism contract
//!
//! `(revision.content_hash, view.id, format, RENDERER_VERSION)` ⇒ **byte-identical output**.
//! That is not an aspiration; it is what [`Format`] plus [`RENDERER_VERSION`] mean, and the
//! `golden` test proves it by comparing checked-in bytes and by rendering twice in two
//! processes.
//!
//! Three rules make it true, and each is structural rather than a convention:
//!
//! 1. **One parsed document serves all three formats.** [`svg::render`] emits SVG; the PNG and
//!    PDF encoders parse *that* SVG with `usvg` — the same tree `resvg` rasterises and
//!    `svg2pdf` converts. The page cannot be laid out twice, so the PNG and the PDF cannot
//!    disagree, and the goldens test the artefact that is actually shipped.
//! 2. **Nothing in the document is volatile.** No timestamp, hostname, path, counter, hash-map
//!    iteration or random id. Element, relationship, view, object and connection ids are
//!    `e-`/`r-`/`v-`/`o-`/`c-` prefixed UUIDs; every collection is ordered.
//! 3. **The faces are embedded.** What `include_bytes!` compiles in is what renders — the
//!    workspace pins `usvg` and `resvg` with `default-features = false`, so
//!    `load_system_fonts()` does not exist to be called, and a base-image change cannot move
//!    a golden byte.
//!
//! # The page
//!
//! One view is one A4 landscape page — [`CANVAS_WIDTH`] × [`CANVAS_HEIGHT`] at 96 dpi. The
//! diagram is scaled uniformly into a [`PAGE_MARGIN`] margin and centred, and because the PDF
//! page size is derived from the SVG's own width and height, the PDF is A4 to the point without
//! a second layout pass.

mod font;
mod ids;
mod pdf;
mod png;
mod svg;
pub mod xmi;

pub use xmi::{
    CONVEYOR_XMI_NS, ExportedXmi, ImportedConnection, ImportedElement, ImportedModel,
    ImportedObject, ImportedRelationship, ImportedView, MAX_DEPTH, MAX_DOCUMENT_BYTES, Unsupported,
    export_xmi, import_xmi,
};

use conveyor_domain::model::ModelSnapshot;
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// The version of the rendering rules. Part of the cache key, and it must move whenever a
/// change would alter output bytes — a golden fixture makes forgetting it fail loudly.
pub const RENDERER_VERSION: &str = "conveyor-render/2";

/// A4 landscape at 96 dpi, in CSS pixels: 297 mm ÷ 25.4 × 96.
///
/// It is carried to two decimals on purpose. Rounding to `1123` gives a PDF page of
/// 842.25 × 595.5 pt instead of A4's 841.89 × 595.28 pt, which is 0.13 % oversize — small
/// enough to look right on screen and large enough that a printer rescales the page.
pub const CANVAS_WIDTH: f64 = 1122.52;

/// A4 landscape height at 96 dpi: 210 mm ÷ 25.4 × 96.
pub const CANVAS_HEIGHT: f64 = 793.70;

/// The margin the diagram is fitted inside, in canvas units.
pub const PAGE_MARGIN: f64 = 24.0;

/// Refuse an output larger than this rather than returning a megabyte document.
///
/// The largest legitimate diagram (a few hundred elements at 2× PNG) is well under it; the cap
/// exists so a pathological model cannot make an action response unbounded.
pub const MAX_RENDER_BYTES: usize = 8 * 1024 * 1024;

/// The output format of one render.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// Scalable vector graphics — the artefact the other two are encoded from.
    Svg,
    /// A raster of the same SVG at 2× the page size.
    Png,
    /// A PDF whose single page is A4 landscape.
    Pdf,
}

impl Format {
    /// The wire spelling used by `models.render` and the byte route.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Svg => "svg",
            Self::Png => "png",
            Self::Pdf => "pdf",
        }
    }

    /// Parse the wire spelling. Unknown values are an error at the door, never a default.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "svg" => Some(Self::Svg),
            "png" => Some(Self::Png),
            "pdf" => Some(Self::Pdf),
            _ => None,
        }
    }

    /// The `Content-Type` of the encoded bytes.
    pub fn media_type(self) -> &'static str {
        match self {
            Self::Svg => "image/svg+xml",
            Self::Png => "image/png",
            Self::Pdf => "application/pdf",
        }
    }
}

/// One rendered artefact: its bytes, what they are, and the digest that names them.
#[derive(Clone, Debug)]
pub struct Rendered {
    /// The encoded document.
    pub bytes: Vec<u8>,
    /// `format.media_type()`.
    pub media_type: &'static str,
    /// Lowercase hex SHA-256 of `bytes`. The ETag and the determinism key.
    pub sha256: String,
    /// [`RENDERER_VERSION`] at the time of rendering.
    pub renderer_version: &'static str,
}

/// Why a model could not be rendered.
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    /// `view_id` names no view of this model.
    #[error("the model has no view {0}")]
    ViewNotFound(Uuid),
    /// The model's language/version pair is not one this build knows.
    #[error("no profile for {language} {version}")]
    UnknownProfile {
        /// `model.language`.
        language: String,
        /// `model.language_version`.
        version: String,
    },
    /// A text run contains a character the embedded face cannot draw.
    ///
    /// This is a refusal, not a fallback: a substituted glyph is how a diagram silently
    /// starts lying about a name.
    #[error("{font} has no glyph for {character:?}")]
    MissingGlyph {
        /// The face that was searched.
        font: &'static str,
        /// The offending character.
        character: char,
    },
    /// The output exceeded [`MAX_RENDER_BYTES`].
    #[error("{format} output is {bytes} bytes, over the {cap}-byte cap")]
    TooLarge {
        /// The format that was over the cap.
        format: &'static str,
        /// How large the output turned out to be.
        bytes: usize,
        /// [`MAX_RENDER_BYTES`].
        cap: usize,
    },
    /// The SVG produced by [`svg::render`] did not parse — a bug in this crate, never input.
    #[error("the generated document is not valid: {0}")]
    Invalid(String),
    /// The XMI document could not be read.
    #[error("the XMI document is not valid: {0}")]
    Xmi(String),
}

impl From<usvg::Error> for RenderError {
    fn from(error: usvg::Error) -> Self {
        Self::Invalid(error.to_string())
    }
}

/// Render one view of `model` in `format`.
///
/// `view_id` names the view; `None` renders a synthetic view built from
/// [`conveyor_domain::model::layout`] over every element, which is what lets a freshly
/// imported (view-less) model be drawn at all.
pub fn render(
    model: &ModelSnapshot,
    view_id: Option<Uuid>,
    format: Format,
) -> Result<Rendered, RenderError> {
    let svg = svg::render(model, view_id)?;
    let bytes = match format {
        Format::Svg => svg,
        Format::Png => png::render(&svg)?,
        Format::Pdf => pdf::render(&svg)?,
    };
    if bytes.len() > MAX_RENDER_BYTES {
        return Err(RenderError::TooLarge {
            format: format.as_str(),
            bytes: bytes.len(),
            cap: MAX_RENDER_BYTES,
        });
    }
    Ok(Rendered {
        sha256: sha256_hex(&bytes),
        bytes,
        media_type: format.media_type(),
        renderer_version: RENDERER_VERSION,
    })
}

/// Render SVG only — the artefact the PNG and PDF are both encoded from.
pub fn render_svg(model: &ModelSnapshot, view_id: Option<Uuid>) -> Result<Vec<u8>, RenderError> {
    svg::render(model, view_id)
}

/// Lowercase hex SHA-256, the digest every artefact is named by.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}
