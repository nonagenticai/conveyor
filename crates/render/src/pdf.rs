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

/// Convert several SVG documents into **one** PDF, one page per document.
///
/// The deck authoring path (`crates/deck`) needs one file for a whole deck, and it must be the
/// same layout pass the PNGs came from. Each page's size comes from its own SVG, so a deck's pages
/// are 16:9 while a document's single page stays A4 landscape; nothing here re-lays-out a page.
///
/// Determinism holds for the same reason the single-page path holds it: the page order is the
/// caller's slice order, the object numbering is derived from it, and `svg2pdf` writes no date and
/// no file id.
///
/// **Text is outlines here, not an embedded subset.** Measured: with `embed_text: true` two runs of
/// the same two-page deck differed — each page gets its own `svg2pdf` context, and a context writes
/// its font objects in a hash-map order, so the subset order inside a page is not stable across
/// processes. Outlines remove the font objects, and the same deck is then byte-identical. The
/// single-page document path keeps its embedded text; a deck's pages are pictures of themselves.
pub(crate) fn render_pages(pages: &[&[u8]]) -> Result<Vec<u8>, RenderError> {
    use std::collections::HashMap;

    use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref};

    if pages.is_empty() {
        return Err(RenderError::Invalid(
            "a pdf needs at least one page".to_string(),
        ));
    }
    let conversion = svg2pdf::ConversionOptions {
        embed_text: false,
        ..svg2pdf::ConversionOptions::default()
    };
    // Each page's graphic is addressed by the same resource name; the name lives in the page's own
    // resource dictionary, so pages cannot collide.
    let svg_name = Name(b"S1");

    let mut alloc = Ref::new(1);
    let catalog_id = alloc.bump();
    let page_tree_id = alloc.bump();
    let page_ids: Vec<Ref> = pages.iter().map(|_| alloc.bump()).collect();

    let mut pdf = Pdf::new();
    pdf.catalog(catalog_id).pages(page_tree_id);
    pdf.pages(page_tree_id)
        .kids(page_ids.iter().copied())
        .count(pages.len() as i32);

    for (index, svg) in pages.iter().enumerate() {
        let text =
            std::str::from_utf8(svg).map_err(|error| RenderError::Invalid(error.to_string()))?;
        let tree = usvg::Tree::from_str(text, font::options())?;
        let size = tree.size();
        let width_pt = f64::from(size.width()) * 72.0 / f64::from(DPI);
        let height_pt = f64::from(size.height()) * 72.0 / f64::from(DPI);

        // The SVG becomes an independent chunk, renumbered into this document's object space.
        let (chunk, svg_id) = svg2pdf::to_chunk(&tree, conversion.clone())
            .map_err(|error| RenderError::Invalid(format!("pdf: {error}")))?;
        let mut map = HashMap::new();
        let chunk = chunk.renumber(|old| *map.entry(old).or_insert_with(|| alloc.bump()));
        let svg_id = *map
            .get(&svg_id)
            .ok_or_else(|| RenderError::Invalid(format!("pdf: page {index} lost its graphic")))?;

        let content_id = alloc.bump();
        {
            let mut page = pdf.page(page_ids[index]);
            page.media_box(Rect::new(0.0, 0.0, width_pt as f32, height_pt as f32));
            page.parent(page_tree_id);
            page.contents(content_id);
            page.resources().x_objects().pair(svg_name, svg_id);
            page.finish();
        }

        // The graphic's own coordinate space is one point square, so the page transform is its
        // page size in points.
        let mut content = Content::new();
        content
            .transform([
                width_pt as f32,
                0.0,
                0.0,
                height_pt as f32,
                0.0,
                0.0,
            ])
            .x_object(svg_name);
        pdf.stream(content_id, &content.finish());
        pdf.extend(&chunk);
    }

    Ok(pdf.finish())
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
