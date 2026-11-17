#![forbid(unsafe_code)]
//! The deck model and its deterministic OOXML writer (ADR 0007 §7).
//!
//! One [`Deck`] is the authoring truth. This crate writes `.pptx` from it in pure Rust — no
//! Python and no LibreOffice — and the fixed v1 template set is exactly four layouts: title,
//! bullets, image and notes. A layout outside that set is not a silent downgrade; the model
//! refuses to deserialize it.
//!
//! # Determinism
//!
//! [`to_pptx`] writes a stored ZIP with a fixed 1980-01-01 DOS timestamp, no extra fields, no
//! random ids and no hash-map iteration, so the same deck and the same image bytes always
//! produce the same package. There is no clock and no environment read in the writer.
//!
//! # Images and the base directory
//!
//! An [`Slide::Image`] names a **relative** path. [`to_pptx`] resolves it against the process
//! working directory; [`to_pptx_in`] resolves it against an explicit base and is what a caller
//! holding a project directory should use; [`validate`] runs every refusal without writing
//! anything. A path that is absolute, has a drive prefix, or contains a `..` segment is refused
//! as escaping the base rather than resolved. The check is lexical: the caller owns the base
//! directory and must not point it at a directory whose symlinks escape it.
//!
//! # The JSON shape
//!
//! The model is internally tagged by layout:
//!
//! ```json
//! {"title":"Quarterly review","slides":[
//!   {"layout":"title","title":"Opening","subtitle":"Q3"},
//!   {"layout":"bullets","title":"Agenda","bullets":["One","Two"]},
//!   {"layout":"image","title":null,"image":"chart.png","caption":"Revenue"},
//!   {"layout":"notes","title":"Speaker notes","notes":["Say this"]}
//! ]}
//! ```

mod ooxml;
mod zip;

use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The most slides one deck may carry.
pub const MAX_SLIDES: usize = 100;

/// The most characters any one text field may carry.
pub const MAX_TEXT_CHARS: usize = 1000;

pub mod svg;

/// One deck: a title and its ordered slides.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Deck {
    /// The deck's title. Carried by the model; the v1 slides each carry their own title.
    pub title: String,
    /// The slides, in presentation order.
    pub slides: Vec<Slide>,
}

/// One slide. The `layout` tag is the discriminant; the four variants are the whole v1 set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "layout", rename_all = "snake_case")]
pub enum Slide {
    /// A centred title and an optional subtitle.
    Title {
        /// The slide title.
        title: String,
        /// The subtitle, when there is one.
        subtitle: Option<String>,
    },
    /// An optional title and a non-empty list of bullets.
    Bullets {
        /// The slide title, when there is one.
        title: Option<String>,
        /// At least one bullet; an empty list is refused.
        bullets: Vec<String>,
    },
    /// An optional title, one PNG or JPEG image, and an optional caption.
    Image {
        /// The slide title, when there is one.
        title: Option<String>,
        /// A PNG or JPEG path relative to the base directory.
        image: String,
        /// The caption, when there is one.
        caption: Option<String>,
    },
    /// A title whose notes are written to the slide's notes part.
    Notes {
        /// The slide title.
        title: String,
        /// The speaker notes, one paragraph each. Empty notes are allowed.
        notes: Vec<String>,
    },
}

/// Why a deck was refused. Every variant has a stable [`DeckError::code`] for logs and rows.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DeckError {
    /// The deck has no slides.
    #[error("the deck is empty: it has no slides")]
    EmptyDeck,
    /// The deck has more slides than the fixed v1 template set allows.
    #[error("the deck has {count} slides; the limit is {MAX_SLIDES}")]
    TooManySlides {
        /// How many slides the deck carries.
        count: usize,
    },
    /// A bullets layout with no bullets.
    #[error("slide {slide} field `{field}`: the bullet list is empty")]
    EmptyBullets {
        /// The 1-based slide number.
        slide: usize,
        /// The offending field.
        field: String,
    },
    /// A text field over [`MAX_TEXT_CHARS`] characters.
    #[error("{location}: {chars} characters; the limit is {MAX_TEXT_CHARS} per field")]
    TextTooLong {
        /// The slide number and field, or `deck field `title``.
        location: String,
        /// How many characters the value carries.
        chars: usize,
    },
    /// An image path that is absolute or contains `..`.
    #[error(
        "slide {slide} field `{field}`: image path `{path}` is absolute or contains `..`, so it \
         escapes the base directory"
    )]
    ImageEscapesBase {
        /// The 1-based slide number.
        slide: usize,
        /// The offending field.
        field: String,
        /// The path as the deck wrote it.
        path: String,
    },
    /// An image that does not exist under the base directory.
    #[error(
        "slide {slide} field `{field}`: image `{path}` does not exist under the base directory"
    )]
    ImageMissing {
        /// The 1-based slide number.
        slide: usize,
        /// The offending field.
        field: String,
        /// The path as the deck wrote it.
        path: String,
    },
    /// An image whose magic bytes are neither PNG nor JPEG.
    #[error(
        "slide {slide} field `{field}`: image `{path}` is {detected}, but only PNG and JPEG are \
         supported"
    )]
    ImageUnsupported {
        /// The 1-based slide number.
        slide: usize,
        /// The offending field.
        field: String,
        /// The path as the deck wrote it.
        path: String,
        /// What the magic bytes say the file is.
        detected: &'static str,
    },
    /// An image that exists but could not be read.
    #[error("slide {slide} field `{field}`: image `{path}` could not be read: {reason}")]
    ImageUnreadable {
        /// The 1-based slide number.
        slide: usize,
        /// The offending field.
        field: String,
        /// The path as the deck wrote it.
        path: String,
        /// The operating-system error.
        reason: String,
    },
    /// The renderer refused or could not complete the page layout.
    #[error("the deck could not be rendered: {reason}")]
    Render {
        /// The renderer's own message.
        reason: String,
    },
    /// A package part that does not fit ZIP32's 4 GiB limit.
    #[error("part `{part}` exceeds the 4 GiB limit of the ZIP32 format")]
    PackageTooLarge {
        /// The part that did not fit.
        part: String,
    },
}

impl DeckError {
    /// The stable slug for logs and rows.
    pub fn code(&self) -> &'static str {
        match self {
            Self::EmptyDeck => "empty_deck",
            Self::TooManySlides { .. } => "too_many_slides",
            Self::EmptyBullets { .. } => "empty_bullets",
            Self::TextTooLong { .. } => "text_too_long",
            Self::ImageEscapesBase { .. } => "image_escapes_base",
            Self::ImageMissing { .. } => "image_missing",
            Self::ImageUnsupported { .. } => "image_unsupported",
            Self::ImageUnreadable { .. } => "image_unreadable",
            Self::Render { .. } => "render_failed",
            Self::PackageTooLarge { .. } => "package_too_large",
        }
    }
}

/// One image loaded and classified, ready to be stored as a media part.
pub(crate) struct PreparedImage {
    /// The file's bytes, stored verbatim.
    pub(crate) bytes: Vec<u8>,
    /// `png` or `jpeg`, from the magic bytes, never from the file name.
    pub(crate) extension: &'static str,
}

/// Write `deck` as an OOXML `.pptx`, resolving image paths against `base`.
///
/// This is the base-aware entry point. [`to_pptx`] is the same writer with the process working
/// directory as the base.
pub fn to_pptx_in(deck: &Deck, base: &Path) -> Result<Vec<u8>, DeckError> {
    let images = prepare(deck, base)?;
    ooxml::package(deck, &images).map_err(|part| DeckError::PackageTooLarge { part })
}

/// Write `deck` as an OOXML `.pptx`, resolving image paths against the process working
/// directory.
pub fn to_pptx(deck: &Deck) -> Result<Vec<u8>, DeckError> {
    to_pptx_in(deck, Path::new("."))
}

/// Render `deck` as one PDF, one page per slide, through `crates/render`.
///
/// The PDF and the PNGs come from the same SVG layout pass, which is why they cannot disagree
/// about the text (ADR 0007 §7). Image paths resolve against `base`, exactly as in
/// [`to_pptx_in`].
pub fn to_pdf(deck: &Deck, base: &Path) -> Result<Vec<u8>, DeckError> {
    let prepared = prepare(deck, base)?;
    let images = media(&prepared);
    let pages = svg::slides(deck, &images);
    let refs: Vec<&[u8]> = pages.iter().map(|page| page.as_bytes()).collect();
    conveyor_render::pdf_from_svgs(&refs).map_err(|error| DeckError::Render {
        reason: error.to_string(),
    })
}

/// Render `deck` as one PNG per slide, in presentation order.
pub fn to_pngs(deck: &Deck, base: &Path) -> Result<Vec<Vec<u8>>, DeckError> {
    let prepared = prepare(deck, base)?;
    let images = media(&prepared);
    svg::slides(deck, &images)
        .into_iter()
        .map(|page| {
            conveyor_render::svg_to_png(page.as_bytes()).map_err(|error| DeckError::Render {
                reason: error.to_string(),
            })
        })
        .collect()
}

/// Write `parts` — names and stored bytes, in the given order — as one deterministic ZIP.
///
/// This is the same stored-ZIP writer [`to_pptx_in`] emits, exposed so a caller that has several
/// artefacts of one deck (the PNG-per-slide form) packages them without a second writer and
/// without a second determinism contract. A part that does not fit ZIP32 is refused by name.
pub fn zip_parts(parts: &[(&str, &[u8])]) -> Result<Vec<u8>, DeckError> {
    zip::archive(parts).map_err(|part| DeckError::PackageTooLarge { part })
}

/// The prepared images as `(media type, bytes)` — the shape the SVG layout embeds as a data URI.
fn media(prepared: &[Option<PreparedImage>]) -> Vec<Option<(String, Vec<u8>)>> {
    prepared
        .iter()
        .map(|image| {
            image
                .as_ref()
                .map(|image| (format!("image/{}", image.extension), image.bytes.clone()))
        })
        .collect()
}

/// Check every refusal without writing a package.
///
/// `base` is the directory relative image paths are resolved against, exactly as in
/// [`to_pptx_in`].
pub fn validate(deck: &Deck, base: &Path) -> Result<(), DeckError> {
    prepare(deck, base).map(|_| ())
}

/// Validate the whole deck and load every image once, so [`to_pptx_in`] neither reads a file
/// twice nor can disagree with [`validate`] about what is acceptable.
fn prepare(deck: &Deck, base: &Path) -> Result<Vec<Option<PreparedImage>>, DeckError> {
    if deck.slides.is_empty() {
        return Err(DeckError::EmptyDeck);
    }
    if deck.slides.len() > MAX_SLIDES {
        return Err(DeckError::TooManySlides {
            count: deck.slides.len(),
        });
    }
    check_text("deck field `title`".to_string(), &deck.title)?;

    let mut images = Vec::with_capacity(deck.slides.len());
    for (index, slide) in deck.slides.iter().enumerate() {
        let number = index + 1;
        let mut image = None;
        match slide {
            Slide::Title { title, subtitle } => {
                check_slide_text(number, "title", title)?;
                if let Some(subtitle) = subtitle {
                    check_slide_text(number, "subtitle", subtitle)?;
                }
            }
            Slide::Bullets { title, bullets } => {
                if let Some(title) = title {
                    check_slide_text(number, "title", title)?;
                }
                if bullets.is_empty() {
                    return Err(DeckError::EmptyBullets {
                        slide: number,
                        field: "bullets".to_string(),
                    });
                }
                for (position, bullet) in bullets.iter().enumerate() {
                    check_slide_text(number, &format!("bullets[{position}]"), bullet)?;
                }
            }
            Slide::Image {
                title,
                image: path,
                caption,
            } => {
                if let Some(title) = title {
                    check_slide_text(number, "title", title)?;
                }
                if let Some(caption) = caption {
                    check_slide_text(number, "caption", caption)?;
                }
                check_slide_text(number, "image", path)?;
                image = Some(load_image(number, "image", path, base)?);
            }
            Slide::Notes { title, notes } => {
                check_slide_text(number, "title", title)?;
                for (position, note) in notes.iter().enumerate() {
                    check_slide_text(number, &format!("notes[{position}]"), note)?;
                }
            }
        }
        images.push(image);
    }
    Ok(images)
}

/// Refuse a text value over [`MAX_TEXT_CHARS`] characters.
fn check_text(location: String, value: &str) -> Result<(), DeckError> {
    let chars = value.chars().count();
    if chars > MAX_TEXT_CHARS {
        return Err(DeckError::TextTooLong { location, chars });
    }
    Ok(())
}

/// [`check_text`] with the slide number and field already formatted.
fn check_slide_text(slide: usize, field: &str, value: &str) -> Result<(), DeckError> {
    check_text(format!("slide {slide} field `{field}`"), value)
}

/// Resolve and read one image, refusing what the v1 template set cannot carry.
fn load_image(
    slide: usize,
    field: &str,
    path: &str,
    base: &Path,
) -> Result<PreparedImage, DeckError> {
    if path.trim().is_empty() {
        return Err(DeckError::ImageMissing {
            slide,
            field: field.to_string(),
            path: path.to_string(),
        });
    }
    if escapes(path) {
        return Err(DeckError::ImageEscapesBase {
            slide,
            field: field.to_string(),
            path: path.to_string(),
        });
    }
    let bytes = match fs::read(base.join(path)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Err(DeckError::ImageMissing {
                slide,
                field: field.to_string(),
                path: path.to_string(),
            });
        }
        Err(error) => {
            return Err(DeckError::ImageUnreadable {
                slide,
                field: field.to_string(),
                path: path.to_string(),
                reason: error.to_string(),
            });
        }
    };
    match image_extension(&bytes) {
        Some(extension) => Ok(PreparedImage { bytes, extension }),
        None => Err(DeckError::ImageUnsupported {
            slide,
            field: field.to_string(),
            path: path.to_string(),
            detected: detected_format(&bytes),
        }),
    }
}

/// The lexical `../`-and-absolute rule. Both separators are checked: a path is refused if it
/// would escape on any platform this service may run on, not only on the one reading it now.
fn escapes(path: &str) -> bool {
    if path.starts_with('/') || path.starts_with('\\') {
        return true;
    }
    let bytes = path.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return true; // a Windows drive prefix
    }
    path.split(['/', '\\']).any(|segment| segment == "..")
}

/// The media extension for bytes the template set accepts, sniffed from the file's magic.
fn image_extension(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]) {
        Some("png")
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("jpeg")
    } else {
        None
    }
}

/// A human-readable label for a refused file, so the message names what it actually found.
fn detected_format(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        "a GIF"
    } else if bytes.starts_with(b"BM") {
        "a BMP"
    } else if bytes.starts_with(b"%PDF") {
        "a PDF"
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        "a WebP"
    } else if bytes
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .is_some_and(|start| {
            bytes[start..].starts_with(b"<svg") || bytes[start..].starts_with(b"<?xml")
        })
    {
        "an SVG"
    } else {
        "an unrecognised format"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn title_slide() -> Slide {
        Slide::Title {
            title: "Title".to_string(),
            subtitle: None,
        }
    }

    #[test]
    fn the_four_layouts_round_trip_through_json() {
        let json = r#"{"title":"D","slides":[
            {"layout":"title","title":"T","subtitle":"S"},
            {"layout":"bullets","title":null,"bullets":["b"]},
            {"layout":"image","title":null,"image":"i.png","caption":null},
            {"layout":"notes","title":"N","notes":["n"]}
        ]}"#;
        let deck: Deck = serde_json::from_str(json).expect("the documented shape deserializes");
        assert_eq!(deck.slides.len(), 4);
    }

    #[test]
    fn a_bullet_list_of_one_empty_string_is_allowed() {
        // "Empty" means no bullets at all; a deliberately blank bullet is text the author chose.
        let deck = Deck {
            title: "D".to_string(),
            slides: vec![Slide::Bullets {
                title: None,
                bullets: vec![String::new()],
            }],
        };
        assert!(validate(&deck, Path::new(".")).is_ok());
    }

    #[test]
    fn the_limit_is_one_hundred_and_one_thousand() {
        let too_many = Deck {
            title: "D".to_string(),
            slides: (0..=MAX_SLIDES).map(|_| title_slide()).collect(),
        };
        assert_eq!(
            validate(&too_many, Path::new(".")).unwrap_err().code(),
            "too_many_slides"
        );

        let exact = Deck {
            title: "T".repeat(MAX_TEXT_CHARS),
            slides: vec![title_slide()],
        };
        assert!(validate(&exact, Path::new(".")).is_ok());
    }

    #[test]
    fn escape_detection_covers_absolute_and_parent_segments() {
        for path in [
            "../x.png",
            "a/../../x.png",
            "/etc/passwd",
            "\\x.png",
            "C:/x.png",
        ] {
            assert!(escapes(path), "{path} must be refused");
        }
        for path in ["x.png", "media/x.png", "./x.png", "a..b/x.png"] {
            assert!(!escapes(path), "{path} must be allowed");
        }
    }

    #[test]
    fn image_magic_is_read_rather_than_the_extension() {
        assert_eq!(
            image_extension(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0]),
            Some("png")
        );
        assert_eq!(image_extension(&[0xff, 0xd8, 0xff, 0xe0]), Some("jpeg"));
        assert_eq!(image_extension(b"GIF89a...."), None);
        assert_eq!(detected_format(b"GIF89a...."), "a GIF");
    }
}
