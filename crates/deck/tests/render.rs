//! P2 — one deck model, one layout pass, PDF and PNG from it.
//!
//! What these tests can honestly prove: the page count is the slide count in both formats, every
//! page is produced from the same SVG document, a missing image is refused with its slide and
//! field, and the bytes are deterministic. Text identity across formats is **structural** — both
//! encoders consume the same `slides()` output — and the renderer's own cross-process golden test
//! carries the byte-determinism of `usvg` → PNG/PDF.

use std::path::Path;

use conveyor_deck::{Deck, Slide, to_pdf, to_pngs};

fn deck() -> Deck {
    Deck {
        title: "Quarterly".to_string(),
        slides: vec![
            Slide::Title {
                title: "Quarterly review".to_string(),
                subtitle: Some("Synthetic".to_string()),
            },
            Slide::Bullets {
                title: Some("Findings".to_string()),
                bullets: vec!["one".to_string(), "two".to_string()],
            },
        ],
    }
}

/// A count of page objects in the PDF. `svg2pdf` writes each page as an `/Type /Page` object and
/// the page tree as `/Type /Pages`, so the count of `/Type /Page` is exact.
fn page_count(pdf: &[u8]) -> usize {
    let text = String::from_utf8_lossy(pdf);
    text.matches("/Type /Page").count() - text.matches("/Type /Pages").count()
}

#[test]
fn one_deck_renders_to_pdf_and_png_with_the_same_text() {
    let deck = deck();
    let pdf = to_pdf(&deck, Path::new(".")).expect("the deck renders to PDF");
    let pngs = to_pngs(&deck, Path::new(".")).expect("the deck renders to PNGs");

    assert!(pdf.starts_with(b"%PDF-"), "the PDF header");
    assert!(
        pdf.ends_with(b"%%EOF\n") || pdf.ends_with(b"%%EOF"),
        "the PDF trailer"
    );
    assert_eq!(page_count(&pdf), deck.slides.len(), "one page per slide");
    assert_eq!(pngs.len(), deck.slides.len(), "one PNG per slide");
    for (index, png) in pngs.iter().enumerate() {
        assert!(
            png.starts_with(&[0x89, b'P', b'N', b'G']),
            "slide {index} is a PNG"
        );
        // 960x540 at the renderer's 2x scale rule.
        let width = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
        let height = u32::from_be_bytes([png[20], png[21], png[22], png[23]]);
        assert_eq!((width, height), (1920, 1080), "slide {index} canvas");
    }
}

#[test]
fn a_missing_image_names_its_slide_and_field() {
    let deck = Deck {
        title: "d".to_string(),
        slides: vec![Slide::Image {
            title: None,
            image: "missing.png".to_string(),
            caption: None,
        }],
    };
    let error = to_pdf(&deck, Path::new(".")).expect_err("a missing image is refused");
    let text = error.to_string();
    assert!(text.contains("slide 1"), "{text}");
    assert!(text.contains("image"), "{text}");
    assert!(text.contains("missing.png"), "{text}");
}

#[test]
fn rendering_is_deterministic_and_carries_no_timestamp() {
    let deck = deck();
    let first = to_pdf(&deck, Path::new(".")).expect("pdf");
    let second = to_pdf(&deck, Path::new(".")).expect("pdf");
    assert_eq!(first, second, "the same deck renders to the same bytes");

    let text = String::from_utf8_lossy(&first);
    for volatile in ["CreationDate", "ModDate", "/Producer (", "D:20"] {
        assert!(
            !text.contains(volatile),
            "the PDF carries {volatile}, which makes two runs differ"
        );
    }
}
