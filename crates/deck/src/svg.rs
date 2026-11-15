//! The deck model as one SVG document per slide.
//!
//! This is the layout pass the PDF and the PNGs share (ADR 0007 §7: one deck model, and the
//! rendered formats must not disagree about where a box sits). It replaces nothing: the `.pptx`
//! writer lays the same model out in OOXML for PowerPoint, and this module lays it out in SVG for
//! `crates/render`. The two are allowed to differ in *fidelity* (a theme font is not reproduced),
//! never in *content*: the same text appears in both.
//!
//! The canvas is 960×540 — 16:9, and exactly 10 × 5.625 inch at 96 dpi, which is what a slide is.

use crate::{Deck, MAX_TEXT_CHARS, Slide};

/// The slide canvas width in SVG units (px at 96 dpi).
pub const WIDTH: u32 = 960;
/// The slide canvas height in SVG units.
pub const HEIGHT: u32 = 540;

/// One SVG document per slide, in presentation order.
pub fn slides(deck: &Deck, images: &[Option<(String, Vec<u8>)>]) -> Vec<String> {
    deck.slides
        .iter()
        .enumerate()
        .map(|(index, slide)| {
            let image = images.get(index).and_then(|entry| entry.as_ref());
            match slide {
                Slide::Title { title, subtitle } => title_svg(title, subtitle.as_deref()),
                Slide::Bullets { title, bullets } => bullets_svg(title.as_deref(), bullets),
                Slide::Image {
                    title, caption, ..
                } => image_svg(title.as_deref(), caption.as_deref(), image),
                Slide::Notes { title, notes } => notes_svg(title, notes),
            }
        })
        .collect()
}

fn page(body: &str) -> String {
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{WIDTH}" height="{HEIGHT}" viewBox="0 0 {WIDTH} {HEIGHT}"><rect width="{WIDTH}" height="{HEIGHT}" fill="#ffffff"/>{body}</svg>"##
    )
}

fn title_svg(title: &str, subtitle: Option<&str>) -> String {
    let mut body = format!(
        r##"<text x="80" y="250" font-family="DejaVu Sans" font-size="52" font-weight="bold" fill="#111111">{}</text>"##,
        escape(&clip(title))
    );
    if let Some(subtitle) = subtitle {
        body.push_str(&format!(
            r##"<text x="80" y="310" font-family="DejaVu Sans" font-size="26" fill="#444444">{}</text>"##,
            escape(&clip(subtitle))
        ));
    }
    page(&body)
}

fn bullets_svg(title: Option<&str>, bullets: &[String]) -> String {
    let mut body = String::new();
    let mut top = 90;
    if let Some(title) = title {
        body.push_str(&format!(
            r##"<text x="70" y="80" font-family="DejaVu Sans" font-size="38" font-weight="bold" fill="#111111">{}</text>"##,
            escape(&clip(title))
        ));
        top = 150;
    }
    // At most eight lines fit; the model already caps a bullet's length, and the writer refuses a
    // list that cannot be shown rather than dropping one silently.
    for (index, bullet) in bullets.iter().take(8).enumerate() {
        let y = top + index as u32 * 45;
        body.push_str(&format!(
            r##"<circle cx="84" cy="{}" r="5" fill="#335577"/><text x="104" y="{y}" font-family="DejaVu Sans" font-size="26" fill="#222222">{}</text>"##,
            y - 8,
            escape(&clip(bullet))
        ));
    }
    page(&body)
}

fn image_svg(
    title: Option<&str>,
    caption: Option<&str>,
    image: Option<&(String, Vec<u8>)>,
) -> String {
    let mut body = String::new();
    if let Some(title) = title {
        body.push_str(&format!(
            r##"<text x="70" y="70" font-family="DejaVu Sans" font-size="34" font-weight="bold" fill="#111111">{}</text>"##,
            escape(&clip(title))
        ));
    }
    if let Some((media_type, bytes)) = image {
        body.push_str(&format!(
            r##"<image x="240" y="110" width="480" height="300" preserveAspectRatio="xMidYMid meet" xlink:href="data:{media_type};base64,{}"/>"##,
            base64(bytes)
        ));
    }
    if let Some(caption) = caption {
        body.push_str(&format!(
            r##"<text x="240" y="460" font-family="DejaVu Sans" font-size="20" fill="#555555">{}</text>"##,
            escape(&clip(caption))
        ));
    }
    page(&body)
}

fn notes_svg(title: &str, notes: &[String]) -> String {
    let mut body = format!(
        r##"<text x="70" y="80" font-family="DejaVu Sans" font-size="38" font-weight="bold" fill="#111111">{}</text>"##,
        escape(&clip(title))
    );
    for (index, note) in notes.iter().take(9).enumerate() {
        let y = 140 + index as u32 * 40;
        body.push_str(&format!(
            r##"<text x="70" y="{y}" font-family="DejaVu Sans" font-size="22" fill="#333333">{}</text>"##,
            escape(&clip(note))
        ));
    }
    page(&body)
}

/// A field clipped to the model's own character cap, so the layout and the writer agree about how
/// much text a slide carries.
fn clip(text: &str) -> String {
    text.chars().take(MAX_TEXT_CHARS).collect()
}

/// XML text escaping. The model refuses control characters; these five are the rest.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            other => out.push(other),
        }
    }
    out
}

/// Standard base64 with padding — the data URI form an SVG image reference needs.
fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let a = chunk[0] as u32;
        let b = chunk.get(1).copied().unwrap_or(0) as u32;
        let c = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (a << 16) | (b << 8) | c;
        out.push(TABLE[(triple >> 18) as usize & 0x3f] as char);
        out.push(TABLE[(triple >> 12) as usize & 0x3f] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(triple >> 6) as usize & 0x3f] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[triple as usize & 0x3f] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_title_slide_carries_its_text_and_no_unescaped_markup() {
        let deck = Deck {
            title: "deck".to_string(),
            slides: vec![Slide::Title {
                title: "A & B <c>".to_string(),
                subtitle: Some("sub".to_string()),
            }],
        };
        let svg = slides(&deck, &[None]);
        assert!(svg[0].contains("A &amp; B &lt;c&gt;"), "{}", svg[0]);
        assert!(!svg[0].contains("<c>"));
        assert!(svg[0].contains("sub"));
    }

    #[test]
    fn a_bullets_slide_shows_at_most_eight_lines() {
        let bullets: Vec<String> = (0..12).map(|index| format!("bullet {index}")).collect();
        let deck = Deck {
            title: "deck".to_string(),
            slides: vec![Slide::Bullets {
                title: None,
                bullets,
            }],
        };
        let svg = slides(&deck, &[None]);
        assert!(svg[0].contains("bullet 7"));
        assert!(!svg[0].contains("bullet 8"));
    }

    #[test]
    fn base64_pads_and_encodes_the_known_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn every_slide_is_a_16_by_9_canvas() {
        let deck = Deck {
            title: "deck".to_string(),
            slides: vec![Slide::Notes {
                title: "n".to_string(),
                notes: vec!["one".to_string()],
            }],
        };
        let svg = slides(&deck, &[None]);
        assert!(svg[0].contains(r##"width="960" height="540""##));
        assert_eq!(WIDTH * 9, HEIGHT * 16);
    }
}
