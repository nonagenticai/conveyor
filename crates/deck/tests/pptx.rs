//! The package and refusal tests for `crates/deck` (ADR 0007 §7).
//!
//! There is no ZIP or XML dependency here. The archive is read back with a small local-header
//! walker and each `.xml` / `.rels` part is parsed with a tag-level well-formedness check, so
//! what is asserted is the bytes actually emitted rather than the writer's own helpers.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use conveyor_deck::{Deck, Slide, to_pptx, to_pptx_in, validate};

const LOCAL_FILE_HEADER_SIGNATURE: u32 = 0x0403_4b50;
const CENTRAL_DIRECTORY_SIGNATURE: u32 = 0x0201_4b50;
const END_OF_CENTRAL_DIRECTORY_SIGNATURE: u32 = 0x0605_4b50;
/// PNG signature plus a few bytes: the writer reads the magic, and stores the rest verbatim.
const PNG_MAGIC: &[u8] = &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13];

fn four_layout_deck() -> Deck {
    Deck {
        title: "Quarterly review".to_string(),
        slides: vec![
            Slide::Title {
                title: "Opening Title".to_string(),
                subtitle: Some("A subtitle".to_string()),
            },
            Slide::Bullets {
                title: Some("Agenda".to_string()),
                bullets: vec![
                    "First bullet".to_string(),
                    "Second <bullet> & more".to_string(),
                ],
            },
            Slide::Image {
                title: Some("A photo".to_string()),
                image: "photo.png".to_string(),
                caption: Some("A caption".to_string()),
            },
            Slide::Notes {
                title: "The notes title".to_string(),
                notes: vec!["First note".to_string(), "Second note".to_string()],
            },
        ],
    }
}

fn deck_with_image(image: &str) -> Deck {
    Deck {
        title: "Images".to_string(),
        slides: vec![Slide::Image {
            title: Some("One image".to_string()),
            image: image.to_string(),
            caption: None,
        }],
    }
}

#[test]
fn a_package_opens_as_a_zip_and_carries_the_four_layouts() {
    let directory = TempDir::new("four-layouts");
    fs::write(directory.path().join("photo.png"), PNG_MAGIC).expect("write the image");
    let deck = four_layout_deck();
    let package = to_pptx_in(&deck, directory.path()).expect("the four layouts write");
    let archive = entries(&package);

    let mut expected = vec![
        "[Content_Types].xml".to_string(),
        "_rels/.rels".to_string(),
        "ppt/presentation.xml".to_string(),
        "ppt/_rels/presentation.xml.rels".to_string(),
        "ppt/slideMasters/slideMaster1.xml".to_string(),
        "ppt/slideMasters/_rels/slideMaster1.xml.rels".to_string(),
        "ppt/slideLayouts/slideLayout1.xml".to_string(),
        "ppt/slideLayouts/_rels/slideLayout1.xml.rels".to_string(),
        "ppt/theme/theme1.xml".to_string(),
        "ppt/media/image1.png".to_string(),
        "ppt/notesSlides/notesSlide1.xml".to_string(),
        "ppt/notesSlides/_rels/notesSlide1.xml.rels".to_string(),
    ];
    for number in 1..=4 {
        expected.push(format!("ppt/slides/slide{number}.xml"));
        expected.push(format!("ppt/slides/_rels/slide{number}.xml.rels"));
    }
    expected.sort();
    let actual: Vec<String> = archive.keys().cloned().collect();
    assert_eq!(actual, expected, "the part tree changed");

    for (name, entry) in &archive {
        if name.ends_with(".xml") || name.ends_with(".rels") {
            let xml =
                std::str::from_utf8(&entry.data).unwrap_or_else(|_| panic!("{name} is not UTF-8"));
            assert_well_formed(xml);
        }
    }

    assert_eq!(
        archive["ppt/media/image1.png"].data, PNG_MAGIC,
        "media bytes must be stored verbatim"
    );
    assert_contains(&archive, "ppt/slides/slide1.xml", "Opening Title");
    assert_contains(&archive, "ppt/slides/slide1.xml", "A subtitle");
    assert_contains(&archive, "ppt/slides/slide2.xml", "Agenda");
    assert_contains(&archive, "ppt/slides/slide2.xml", "First bullet");
    assert_contains(&archive, "ppt/slides/slide2.xml", "Second <bullet> & more");
    assert_contains(&archive, "ppt/slides/slide3.xml", "A photo");
    assert_contains(&archive, "ppt/slides/slide3.xml", "A caption");
    assert_contains(&archive, "ppt/slides/slide4.xml", "The notes title");
    assert_contains(&archive, "ppt/notesSlides/notesSlide1.xml", "First note");
    assert_contains(&archive, "ppt/notesSlides/notesSlide1.xml", "Second note");

    let picture = std::str::from_utf8(&archive["ppt/slides/slide3.xml"].data).unwrap();
    assert!(
        picture.contains("<p:pic>"),
        "the image slide carries a picture"
    );
    assert!(
        picture.contains("r:embed=\"rId2\""),
        "the picture's r:embed must address its media relationship"
    );
    let picture_rels =
        std::str::from_utf8(&archive["ppt/slides/_rels/slide3.xml.rels"].data).unwrap();
    assert!(picture_rels.contains("relationships/image"));
    assert!(picture_rels.contains("../media/image1.png"));

    let notes_rels =
        std::str::from_utf8(&archive["ppt/slides/_rels/slide4.xml.rels"].data).unwrap();
    assert!(notes_rels.contains("relationships/notesSlide"));
    assert!(notes_rels.contains("../notesSlides/notesSlide1.xml"));

    let root_rels = std::str::from_utf8(&archive["_rels/.rels"].data).unwrap();
    assert!(root_rels.contains("relationships/officeDocument"));
    assert!(root_rels.contains("ppt/presentation.xml"));

    let content_types = std::str::from_utf8(&archive["[Content_Types].xml"].data).unwrap();
    assert!(content_types.contains("notesSlide+xml"));
    assert!(content_types.contains("image/png"));
    assert!(content_types.contains("presentationml.slide+xml"));
}

#[test]
fn two_processes_write_identical_packages() {
    // Two calls in one process are the same experiment as two processes for this writer: it
    // reads no clock, no environment and no hash map. The byte scans below are the proof that no
    // such source slipped in, which is exactly what the cross-process gate checks too.
    let deck = Deck {
        title: "Deterministic".to_string(),
        slides: vec![
            Slide::Title {
                title: "Same bytes".to_string(),
                subtitle: None,
            },
            Slide::Bullets {
                title: None,
                bullets: vec!["one".to_string(), "two".to_string()],
            },
        ],
    };
    let first = to_pptx(&deck).expect("first write");
    let second = to_pptx(&deck).expect("second write");
    assert_eq!(first, second, "the same deck must produce the same bytes");

    let archive = entries(&first);
    for (name, entry) in &archive {
        assert_eq!(entry.date, 0x0021, "{name} carries a non-fixed DOS date");
        assert_eq!(entry.time, 0, "{name} carries a DOS time");
    }

    let year = current_year().to_string();
    assert!(
        !first
            .windows(year.len())
            .any(|window| window == year.as_bytes()),
        "the package contains the current year {year}"
    );
    assert!(
        !first.windows(36).any(looks_like_uuid),
        "the package contains a UUID-shaped id"
    );
}

#[test]
fn a_layout_outside_the_fixed_four_is_refused_by_name() {
    let error = serde_json::from_str::<Slide>(r#"{"layout":"chart","title":"Nope"}"#)
        .expect_err("an unknown layout must not deserialize");
    let message = error.to_string();
    assert!(message.contains("chart"), "{message}");
    for layout in ["title", "bullets", "image", "notes"] {
        assert!(
            message.contains(layout),
            "{message} does not list the supported layout `{layout}`"
        );
    }
}

#[test]
fn an_image_that_escapes_the_base_is_refused_by_name() {
    let directory = TempDir::new("escape");
    for path in ["../outside.png", "/etc/passwd", "a/../../outside.png"] {
        let error = validate(&deck_with_image(path), directory.path())
            .expect_err("an escaping path must be refused");
        assert_eq!(error.code(), "image_escapes_base", "{path}");
        let message = error.to_string();
        assert!(message.contains("slide 1"), "{message}");
        assert!(message.contains("field `image`"), "{message}");
        assert!(message.contains(path), "{message}");
    }
}

#[test]
fn a_missing_image_names_its_slide_and_field() {
    let directory = TempDir::new("missing");
    let error = validate(&deck_with_image("missing.png"), directory.path())
        .expect_err("a missing image must be refused");
    assert_eq!(error.code(), "image_missing");
    let message = error.to_string();
    assert!(message.contains("slide 1"), "{message}");
    assert!(message.contains("field `image`"), "{message}");
    assert!(message.contains("missing.png"), "{message}");
}

#[test]
fn an_empty_deck_is_refused_by_name() {
    let deck = Deck {
        title: "Empty".to_string(),
        slides: Vec::new(),
    };
    let error = validate(&deck, Path::new(".")).expect_err("an empty deck must be refused");
    assert_eq!(error.code(), "empty_deck");
    assert!(error.to_string().contains("no slides"), "{error}");
    assert_eq!(to_pptx(&deck).expect_err("to_pptx refuses too"), error);
}

#[test]
fn an_image_that_is_not_png_or_jpeg_is_refused_by_name() {
    let directory = TempDir::new("gif");
    fs::write(
        directory.path().join("chart.gif"),
        b"GIF89a not a slide image",
    )
    .expect("write the image");
    let error = validate(&deck_with_image("chart.gif"), directory.path())
        .expect_err("a GIF must be refused");
    assert_eq!(error.code(), "image_unsupported");
    let message = error.to_string();
    assert!(message.contains("slide 1"), "{message}");
    assert!(message.contains("field `image`"), "{message}");
    assert!(message.contains("chart.gif"), "{message}");
    assert!(message.contains("GIF"), "{message}");
}

#[test]
fn an_empty_bullet_list_names_its_slide_and_field() {
    let deck = Deck {
        title: "Bullets".to_string(),
        slides: vec![Slide::Bullets {
            title: Some("Agenda".to_string()),
            bullets: Vec::new(),
        }],
    };
    let error = validate(&deck, Path::new(".")).expect_err("empty bullets must be refused");
    assert_eq!(error.code(), "empty_bullets");
    let message = error.to_string();
    assert!(message.contains("slide 1"), "{message}");
    assert!(message.contains("bullets"), "{message}");
}

#[test]
fn a_deck_over_the_slide_limit_is_refused_by_name() {
    let deck = Deck {
        title: "Too many".to_string(),
        slides: (0..=conveyor_deck::MAX_SLIDES)
            .map(|_| Slide::Title {
                title: "Slide".to_string(),
                subtitle: None,
            })
            .collect(),
    };
    let error = validate(&deck, Path::new(".")).expect_err("101 slides must be refused");
    assert_eq!(error.code(), "too_many_slides");
    assert!(error.to_string().contains("101"), "{error}");
}

#[test]
fn text_over_the_limit_names_its_slide_and_field() {
    let deck = Deck {
        title: "Long".to_string(),
        slides: vec![Slide::Bullets {
            title: None,
            bullets: vec!["x".repeat(conveyor_deck::MAX_TEXT_CHARS + 1)],
        }],
    };
    let error = validate(&deck, Path::new(".")).expect_err("1001 characters must be refused");
    assert_eq!(error.code(), "text_too_long");
    let message = error.to_string();
    assert!(message.contains("slide 1"), "{message}");
    assert!(message.contains("bullets[0]"), "{message}");
    assert!(message.contains("1001"), "{message}");
}

// ── the archive reader ───────────────────────────────────────────────────────────────────────

struct Entry {
    data: Vec<u8>,
    date: u16,
    time: u16,
}

fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap())
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

/// Walk the stored entries from the local headers. Every one of this writer's entries is
/// stored, so the local size fields are the data's real bounds and no data descriptor follows.
fn entries(bytes: &[u8]) -> BTreeMap<String, Entry> {
    let mut out = BTreeMap::new();
    let mut at = 0;
    while at + 30 <= bytes.len() && u32_at(bytes, at) == LOCAL_FILE_HEADER_SIGNATURE {
        assert_eq!(u16_at(bytes, at + 8), 0, "entries must be stored");
        let time = u16_at(bytes, at + 10);
        let date = u16_at(bytes, at + 12);
        let compressed = u32_at(bytes, at + 18) as usize;
        let uncompressed = u32_at(bytes, at + 22) as usize;
        let name_len = u16_at(bytes, at + 26) as usize;
        let extra_len = u16_at(bytes, at + 28) as usize;
        let name_start = at + 30;
        let data_start = name_start + name_len + extra_len;
        let data_end = data_start + compressed;
        assert!(data_end <= bytes.len(), "truncated entry");
        assert_eq!(compressed, uncompressed, "stored entries are not shrunk");
        let name = String::from_utf8(bytes[name_start..name_start + name_len].to_vec())
            .expect("entry names are UTF-8");
        out.insert(
            name,
            Entry {
                data: bytes[data_start..data_end].to_vec(),
                date,
                time,
            },
        );
        at = data_end;
    }
    let mut central = at;
    while central + 46 <= bytes.len() && u32_at(bytes, central) == CENTRAL_DIRECTORY_SIGNATURE {
        let name_len = u16_at(bytes, central + 28) as usize;
        let extra_len = u16_at(bytes, central + 30) as usize;
        let comment_len = u16_at(bytes, central + 32) as usize;
        central += 46 + name_len + extra_len + comment_len;
    }
    assert_eq!(
        u32_at(bytes, central),
        END_OF_CENTRAL_DIRECTORY_SIGNATURE,
        "the central directory must end the entries"
    );
    assert_eq!(
        u16_at(bytes, central + 10) as usize,
        out.len(),
        "the EOCD must count every entry"
    );
    assert_eq!(central + 22, bytes.len(), "no archive comment is written");
    out
}

// ── a tag-level XML reader ──────────────────────────────────────────────────────────────────

/// Assert that `xml` is well-formed enough for this package: one root, balanced tags, quoted
/// attributes and only known entities. Comments and CDATA are deliberately not accepted — the
/// writer emits neither, and a test that tolerated them would be testing a different format.
fn assert_well_formed(xml: &str) {
    assert!(
        xml.starts_with("<?xml"),
        "no XML declaration: {:?}",
        &xml[..xml.len().min(80)]
    );
    let mut stack: Vec<String> = Vec::new();
    let mut rest = xml;
    while let Some(position) = rest.find('<') {
        check_text(&rest[..position]);
        rest = &rest[position..];
        let end = tag_end(rest);
        let raw = &rest[1..end];
        rest = &rest[end + 1..];
        if raw.starts_with('?') {
            assert!(raw.ends_with('?'), "malformed processing instruction");
        } else if let Some(name) = raw.strip_prefix('/') {
            let name = name.trim();
            let open = stack.pop().expect("a closing tag without an open tag");
            assert_eq!(open, name, "mismatched closing tag");
        } else {
            assert!(
                !raw.starts_with('!'),
                "comments and declarations other than <?xml?> are not emitted"
            );
            let self_closing = raw.ends_with('/');
            let body = if self_closing {
                raw[..raw.len() - 1].trim_end()
            } else {
                raw
            };
            let name = body
                .split_whitespace()
                .next()
                .expect("every tag has a name");
            assert!(
                name.chars()
                    .all(|character| character.is_ascii_alphanumeric()
                        || matches!(character, ':' | '_' | '-')),
                "invalid element name {name:?}"
            );
            if !self_closing {
                stack.push(name.to_string());
            }
        }
    }
    check_text(rest);
    assert!(stack.is_empty(), "unclosed elements: {stack:?}");
}

fn tag_end(xml: &str) -> usize {
    let mut quote: Option<char> = None;
    for (index, character) in xml.char_indices().skip(1) {
        match quote {
            Some(open) => {
                if character == open {
                    quote = None;
                }
            }
            None => match character {
                '"' | '\'' => quote = Some(character),
                '>' => return index,
                _ => {}
            },
        }
    }
    panic!("unterminated tag in {:?}", &xml[..xml.len().min(80)]);
}

fn check_text(text: &str) {
    let mut rest = text;
    while let Some(position) = rest.find('&') {
        rest = &rest[position + 1..];
        let end = rest
            .find(';')
            .unwrap_or_else(|| panic!("unterminated entity in {text:?}"));
        let entity = &rest[..end];
        assert!(
            ["amp", "lt", "gt", "quot", "apos"].contains(&entity) || entity.starts_with('#'),
            "unknown entity &{entity};"
        );
        rest = &rest[end + 1..];
    }
}

/// The text nodes of `xml`, with entities decoded.
fn texts(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(position) = rest.find('<') {
        if position > 0 {
            out.push(unescape(&rest[..position]));
        }
        rest = &rest[position..];
        let end = tag_end(rest);
        rest = &rest[end + 1..];
    }
    if !rest.is_empty() {
        out.push(unescape(rest));
    }
    out
}

fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn assert_contains(archive: &BTreeMap<String, Entry>, part: &str, expected: &str) {
    let xml =
        std::str::from_utf8(&archive[part].data).unwrap_or_else(|_| panic!("{part} is not UTF-8"));
    let found = texts(xml);
    assert!(
        found.iter().any(|text| text == expected),
        "{part} does not carry {expected:?}; its text nodes are {found:?}"
    );
}

// ── determinism helpers ────────────────────────────────────────────────────────────────────

/// The current UTC year, from the Unix clock, so the test does not hard-code a year that goes
/// stale. This is Howard Hinnant's `civil_from_days`.
fn current_year() -> i64 {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the clock is after 1970")
        .as_secs() as i64;
    let days = seconds.div_euclid(86_400);
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_part = (5 * day_of_year + 2) / 153;
    let month = if month_part < 10 {
        month_part + 3
    } else {
        month_part - 9
    };
    if month <= 2 { year + 1 } else { year }
}

fn looks_like_uuid(window: &[u8]) -> bool {
    window.iter().enumerate().all(|(index, byte)| match index {
        8 | 13 | 18 | 23 => *byte == b'-',
        _ => byte.is_ascii_hexdigit(),
    })
}

// ── a temp directory without a dev-dependency ──────────────────────────────────────────────

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "conveyor-deck-{}-{tag}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).expect("create the temp directory");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
