#![forbid(unsafe_code)]
//! The one font database, built from bytes compiled into the binary.
//!
//! The faces are DejaVu 2.37, vendored in `crates/render/fonts/` with their licence and a
//! [`SHA256SUMS`](../fonts/SHA256SUMS) file the gate verifies. They are registered into
//! `usvg`'s `fontdb` here and **nowhere else**.
//!
//! # Why host fonts cannot leak in
//!
//! Two guards, because the first one alone is not enough:
//!
//! 1. The workspace pins `usvg` and `resvg` with `default-features = false`, so usvg's
//!    `system-fonts` feature is off and the parser never scans the system by itself.
//! 2. `fontdb`'s `fs` feature is nevertheless **on**, because `svg2pdf` depends on `fontdb`
//!    with its defaults — so `load_system_fonts()` exists in this build and a future edit
//!    could call it. [`tests::the_renderer_never_loads_system_fonts`] scans this crate's own
//!    sources for the call, and [`tests::the_embedded_faces_are_registered_under_the_family_name`]
//!    asserts the database holds exactly the four vendored faces.
//!
//! Text is never allowed to fall back. [`ensure_covered`] walks the cmap before the document
//! is emitted and refuses a character the face cannot draw, because a substituted glyph is how
//! a diagram starts silently lying about a name.

use std::sync::OnceLock;

use crate::RenderError;

/// The family name the SVG declares and `fontdb` resolves.
pub(crate) const FAMILY: &str = "DejaVu Sans";

const REGULAR: &[u8] = include_bytes!("../fonts/DejaVuSans.ttf");
const BOLD: &[u8] = include_bytes!("../fonts/DejaVuSans-Bold.ttf");
const OBLIQUE: &[u8] = include_bytes!("../fonts/DejaVuSans-Oblique.ttf");
const BOLD_OBLIQUE: &[u8] = include_bytes!("../fonts/DejaVuSans-BoldOblique.ttf");

/// Every embedded face, in a fixed order.
const FACES: [&[u8]; 4] = [REGULAR, BOLD, OBLIQUE, BOLD_OBLIQUE];

/// The base text size, in canvas units.
pub(crate) const BASE_SIZE: f64 = 12.0;

static OPTIONS: OnceLock<usvg::Options> = OnceLock::new();
static FACE: OnceLock<ttf_parser::Face<'static>> = OnceLock::new();

/// `usvg` options with the vendored faces loaded and nothing else.
///
/// Built once per process. `usvg::Options` is shared by every render — the SVG parser, the
/// PNG rasteriser and the PDF converter all resolve families through it, so the three formats
/// use one set of metrics.
pub(crate) fn options() -> &'static usvg::Options<'static> {
    OPTIONS.get_or_init(|| {
        let mut options = usvg::Options {
            font_family: FAMILY.to_owned(),
            font_size: BASE_SIZE as f32,
            ..usvg::Options::default()
        };
        for face in FACES {
            options.fontdb_mut().load_font_data(face.to_vec());
        }
        options
    })
}

/// The regular face, parsed once — used for coverage checks and advance measurement.
fn face() -> &'static ttf_parser::Face<'static> {
    FACE.get_or_init(|| {
        ttf_parser::Face::parse(REGULAR, 0).expect("the vendored DejaVuSans.ttf parses")
    })
}

/// Refuse a string containing a character the regular face cannot draw.
pub(crate) fn ensure_covered(text: &str) -> Result<(), RenderError> {
    let face = face();
    for character in text.chars() {
        if matches!(character, '\n' | '\r' | '\t') {
            continue;
        }
        if face.glyph_index(character).is_none() {
            return Err(RenderError::MissingGlyph {
                font: FAMILY,
                character,
            });
        }
    }
    Ok(())
}

/// The advance width of `text` at `size`, in canvas units.
///
/// Summing real per-glyph advances rather than assuming an average character width is what
/// makes wrapping stable: the same string always measures the same, and a name that fits does
/// not overflow its box by a character or two.
pub(crate) fn advance(text: &str, size: f64) -> f64 {
    let face = face();
    let units = f64::from(face.units_per_em());
    if units <= 0.0 {
        return 0.0;
    }
    let mut total = 0.0;
    for character in text.chars() {
        if let Some(id) = face.glyph_index(character)
            && let Some(advance) = face.glyph_hor_advance(id)
        {
            total += f64::from(advance) / units * size;
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The production half of a source file: everything before `#[cfg(test)]`, with comment
    /// lines removed. A doc comment that *names* the call is not a call.
    fn production_code(text: &str) -> String {
        let body = match text.find("#[cfg(test)]") {
            Some(index) => &text[..index],
            None => text,
        };
        body.lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn the_renderer_never_loads_system_fonts() {
        // Slice §5.6 asks for a grep; this one runs in the gate rather than in someone's shell
        // history. `svg2pdf` turns `fontdb`'s `fs` feature on, so the function exists in this
        // build and nothing but this guard stops it being called.
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut offenders = Vec::new();
        for entry in std::fs::read_dir(&dir).expect("the src directory is readable") {
            let path = entry.expect("a directory entry").path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("a source file is readable");
            if production_code(&text).contains("load_system_fonts") {
                offenders.push(path.display().to_string());
            }
        }
        offenders.sort();
        assert!(
            offenders.is_empty(),
            "system fonts must never be loaded: {offenders:?}"
        );
    }

    #[test]
    fn the_embedded_faces_are_registered_under_the_family_name() {
        let options = options();
        let mut families = options.fontdb.faces().flat_map(|face| {
            face.families
                .iter()
                .map(|(family, _)| family.clone())
                .collect::<Vec<_>>()
        });
        assert!(
            families.any(|family| family == FAMILY),
            "the vendored faces must be registered as {FAMILY}"
        );
        // Four faces, and nothing from the host: the count is exact because `system-fonts` is
        // compiled out.
        assert_eq!(options.fontdb.len(), 4, "exactly the four vendored faces");
    }

    #[test]
    fn a_missing_glyph_is_refused_rather_than_drawn_as_tofu() {
        // Measured against the vendored 2.37 face: it carries é, €, →, … and — and even
        // U+1F600, but no CJK and no private-use plane 16. (The first version of this test
        // asserted U+1F600 was absent and was wrong; the face covers it.)
        for present in ["Architecture", "Café", "€ → … — 😀"] {
            assert!(ensure_covered(present).is_ok(), "{present:?} is covered");
        }
        for missing in ['\u{6F22}', '\u{10FFFD}'] {
            match ensure_covered(&format!("name {missing}")) {
                Err(RenderError::MissingGlyph { character, .. }) => assert_eq!(character, missing),
                other => panic!("expected MissingGlyph for {missing:?}, got {other:?}"),
            }
        }
    }

    #[test]
    fn advances_are_positive_and_monotone() {
        let narrow = advance("i", 12.0);
        let wide = advance("W", 12.0);
        assert!(narrow > 0.0 && wide > 0.0);
        assert!(
            wide > narrow,
            "a W is wider than an i in a proportional face"
        );
        assert!(advance("", 12.0) == 0.0);
        assert!(advance("WW", 12.0) > advance("W", 12.0));
    }
}
