#![forbid(unsafe_code)]
//! SVG → PDF through `svg2pdf`, on the same parsed tree the PNG is rasterised from.
//!
//! There is no second layout engine. `typst` was the original plan and was dropped: it typesets
//! the page again from a different input, which is a class of bug (PDF and PNG disagreeing
//! about where a box sits) that cannot be caught by a test if the two paths never meet. Here
//! one `usvg` tree feeds both encoders.
//!
//! The page size is derived from the SVG's own `width`/`height` at [`DPI`], which is why the
//! canvas is exactly A4 landscape at 96 dpi: 1122.52 × 793.70 px becomes 841.89 × 595.28 pt.
//!
//! Text is embedded (`embed_text`), so the document does not depend on the reader's fonts. The
//! only document-info field `svg2pdf` writes is a constant `producer` — no date, no file id —
//! so two runs of the same input are byte-identical; the `golden` test proves it from two
//! processes.

use crate::{RenderError, font};

/// The page resolution the canvas is defined at. Changing it changes the PDF page size.
pub(crate) const DPI: f32 = 96.0;

/// Convert an SVG document to a single-page PDF.
pub(crate) fn render(svg: &[u8]) -> Result<Vec<u8>, RenderError> {
    let text = std::str::from_utf8(svg).map_err(|error| RenderError::Invalid(error.to_string()))?;
    let tree = usvg::Tree::from_str(text, font::options())?;
    let conversion = svg2pdf::ConversionOptions {
        // A PDF that carries its text as glyph outlines with no font is both unsearchable and
        // larger; embedding the subset is the deterministic option.
        embed_text: true,
        ..svg2pdf::ConversionOptions::default()
    };
    let page = svg2pdf::PageOptions { dpi: DPI };
    svg2pdf::to_pdf(&tree, conversion, page)
        .map_err(|error| RenderError::Invalid(format!("pdf: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_canvas_is_a4_landscape_in_points() {
        // A4 landscape is 297 × 210 mm; points are 1/72 inch.
        let width_pt = crate::CANVAS_WIDTH / f64::from(DPI) * 72.0;
        let height_pt = crate::CANVAS_HEIGHT / f64::from(DPI) * 72.0;
        assert!((width_pt - 841.89).abs() < 0.01, "{width_pt}");
        assert!((height_pt - 595.28).abs() < 0.01, "{height_pt}");
    }
}
