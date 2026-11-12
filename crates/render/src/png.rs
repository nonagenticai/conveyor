#![forbid(unsafe_code)]
//! SVG → PNG. The SVG is parsed with the same `usvg` options the document was produced under
//! (the embedded faces), then rasterised at 2× so text stays legible when the image is
//! embedded at page width.
//!
//! The raster is capped at [`MAX_DIMENSION`] on its longest side: a deterministic scale rule
//! matters more than resolution, because a scale that depended on available memory would make
//! the bytes depend on the host.

use crate::{RenderError, font};

/// The largest PNG edge. 2× an A4 landscape page is 2246 px, comfortably inside it.
const MAX_DIMENSION: f64 = 4096.0;

/// The preferred raster scale.
const SCALE: f64 = 2.0;

/// Rasterise an SVG document.
pub(crate) fn render(svg: &[u8]) -> Result<Vec<u8>, RenderError> {
    let text = std::str::from_utf8(svg).map_err(|error| RenderError::Invalid(error.to_string()))?;
    let tree = usvg::Tree::from_str(text, font::options())?;
    let size = tree.size();
    let (width, height) = dimensions(f64::from(size.width()), f64::from(size.height()));
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height).ok_or_else(|| {
        RenderError::Invalid(format!("a {width}×{height} pixmap could not be allocated"))
    })?;
    let scale = width as f64 / f64::from(size.width());
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale as f32, scale as f32),
        &mut pixmap.as_mut(),
    );
    pixmap
        .encode_png()
        .map_err(|error| RenderError::Invalid(format!("png: {error}")))
}

/// The pixel dimensions for a page, scaled up to [`SCALE`] and down to [`MAX_DIMENSION`].
fn dimensions(width: f64, height: f64) -> (u32, u32) {
    let width = width.max(1.0);
    let height = height.max(1.0);
    let fit = (MAX_DIMENSION / width.max(height)).min(SCALE);
    let scale = fit.max(f64::MIN_POSITIVE);
    (
        (width * scale).round().max(1.0) as u32,
        (height * scale).round().max(1.0) as u32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_a4_page_rasterises_at_two_times() {
        let (width, height) = dimensions(1122.52, 793.7);
        assert_eq!((width, height), (2245, 1587));
    }

    #[test]
    fn an_oversize_page_is_scaled_down_not_refused() {
        let (width, height) = dimensions(10_000.0, 5_000.0);
        assert!(width <= MAX_DIMENSION as u32, "{width}");
        assert!(height <= MAX_DIMENSION as u32, "{height}");
        assert_eq!(width, 4096);
        assert_eq!(height, 2048);
    }
}
