//! Golden bytes — the S8 gate's "golden SVG/PNG/PDF fixtures".
//!
//! Three fixtures × three formats, compared against **checked-in bytes** and a SHA-256
//! manifest. There are no `insta` snapshots here: a binary document in an `.snap` file is
//! unreadable in a diff, and `cargo insta accept` makes it too easy to bless a change nobody
//! looked at. The manifest makes the contract explicit — a byte moved, and the diff says which.
//!
//! ```text
//! GOLDEN_UPDATE=1 cargo test -p conveyor-render --test golden   # rewrite the fixtures
//! ```
//!
//! `GOLDEN_DIR` overrides where the artefacts are written, which is how the gate proves
//! determinism **across processes**: two `cargo test` invocations are two processes, and their
//! two output trees must be identical.

mod fixtures;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use conveyor_render::{Format, render, sha256_hex};
use serde::{Deserialize, Serialize};

/// One artefact's expected bytes, by digest.
#[derive(Debug, Deserialize, Serialize)]
struct Entry {
    sha256: String,
    byte_len: usize,
}

fn golden_dir() -> PathBuf {
    match std::env::var_os("GOLDEN_DIR") {
        Some(dir) => PathBuf::from(dir),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden"),
    }
}

fn updating() -> bool {
    std::env::var("GOLDEN_UPDATE").as_deref() == Ok("1")
}

/// Every fixture rendered in every format, keyed by its path inside the golden directory.
fn artefacts() -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    for (name, model) in fixtures::all() {
        let view = fixtures::view_of(&model);
        for format in [Format::Svg, Format::Png, Format::Pdf] {
            let rendered = render(&model, view, format).unwrap_or_else(|error| {
                panic!("{name} as {} did not render: {error}", format.as_str())
            });
            out.insert(
                format!("{name}/diagram.{}", format.as_str()),
                rendered.bytes,
            );
        }
    }
    out
}

#[test]
fn every_fixture_matches_its_golden_bytes() {
    let dir = golden_dir();
    let rendered = artefacts();
    let manifest_path = dir.join("manifest.json");

    if updating() {
        std::fs::create_dir_all(&dir).expect("the golden directory is writable");
        let mut manifest: BTreeMap<String, Entry> = BTreeMap::new();
        for (path, bytes) in &rendered {
            let target = dir.join(path);
            std::fs::create_dir_all(target.parent().expect("a parent")).expect("mkdir");
            std::fs::write(&target, bytes).expect("write the artefact");
            manifest.insert(
                path.clone(),
                Entry {
                    sha256: sha256_hex(bytes),
                    byte_len: bytes.len(),
                },
            );
        }
        let json = serde_json::to_string_pretty(&manifest).expect("the manifest serialises");
        std::fs::write(&manifest_path, format!("{json}\n")).expect("write the manifest");
        eprintln!(
            "GOLDEN_UPDATE: wrote {} artefacts to {}",
            manifest.len(),
            dir.display()
        );
        return;
    }

    let raw = std::fs::read_to_string(&manifest_path).unwrap_or_else(|error| {
        panic!(
            "{} is unreadable ({error}); run with GOLDEN_UPDATE=1 to create it",
            manifest_path.display()
        )
    });
    let manifest: BTreeMap<String, Entry> =
        serde_json::from_str(&raw).expect("the manifest parses");

    // The set must match exactly: a format added without regenerating is a failure, not a
    // silent omission. This is the assertion that stops the test from passing vacuously.
    let expected: Vec<&String> = manifest.keys().collect();
    let actual: Vec<&String> = rendered.keys().collect();
    assert_eq!(
        expected, actual,
        "the manifest and the renderer disagree on which artefacts exist"
    );

    for (path, entry) in &manifest {
        let bytes = &rendered[path];
        assert_eq!(
            entry.byte_len,
            bytes.len(),
            "{path} changed length ({} vs {})",
            entry.byte_len,
            bytes.len()
        );
        assert_eq!(
            entry.sha256,
            sha256_hex(bytes),
            "{path} changed bytes — if the change is intended, regenerate with GOLDEN_UPDATE=1 \
             and move RENDERER_VERSION"
        );
    }
}

#[test]
fn rendering_twice_produces_the_same_bytes() {
    for (name, model) in fixtures::all() {
        let view = fixtures::view_of(&model);
        for format in [Format::Svg, Format::Png, Format::Pdf] {
            let first = render(&model, view, format).expect("first render");
            let second = render(&model, view, format).expect("second render");
            assert_eq!(
                first.sha256,
                second.sha256,
                "{name} as {} is not deterministic within one process",
                format.as_str()
            );
        }
    }
}

#[test]
fn each_format_is_actually_that_format() {
    // A golden comparison proves "unchanged", not "valid". These check the artefact is what it
    // claims to be, so a renderer that emitted an empty file consistently would still fail.
    let model = fixtures::model_a();
    let view = fixtures::view_of(&model);

    let svg = render(&model, view, Format::Svg).expect("svg");
    let text = std::str::from_utf8(&svg.bytes).expect("utf-8");
    assert!(text.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg "));
    assert!(
        text.contains("viewBox=\"0 0 1122.52 793.7\""),
        "A4 landscape canvas"
    );
    assert!(
        text.contains("font-family=\"DejaVu Sans\""),
        "the embedded face"
    );
    assert!(text.contains("id=\"e-"), "element ids are e-<uuid>");
    assert!(text.contains("DejaVu"), "the face is named");
    // The document must parse: it is the PNG and PDF entry point, so an unparseable SVG would
    // fail both encoders for a reason the golden comparison alone would not explain. Font
    // resolution is not exercised here — `default()` has no faces — but parsing is.
    usvg::Tree::from_str(text, &usvg::Options::default()).expect("the SVG parses");

    let png = render(&model, view, Format::Png).expect("png");
    assert_eq!(&png.bytes[..8], b"\x89PNG\r\n\x1a\n", "PNG magic");
    assert!(png.bytes.len() > 4_000, "a blank page would be far smaller");

    let pdf = render(&model, view, Format::Pdf).expect("pdf");
    assert_eq!(&pdf.bytes[..5], b"%PDF-", "PDF magic");
    assert!(pdf.bytes.len() > 2_000, "a blank page would be far smaller");
    // The page must be A4 landscape **exactly**, and the only thing that makes it so is the
    // 1122.52 × 793.70 canvas at 96 dpi. Rounding the canvas to whole pixels gives
    // 842.25 × 595.5 pt — near enough to look right and wrong enough that a printer rescales.
    let (width, height) = media_box(&pdf.bytes);
    assert!(
        (width - 841.89).abs() < 0.05,
        "A4 width in points, got {width}"
    );
    assert!(
        (height - 595.28).abs() < 0.05,
        "A4 height in points, got {height}"
    );
}

/// The first `/MediaBox [x0 y0 x1 y1]` in a PDF, as its width and height in points.
fn media_box(pdf: &[u8]) -> (f64, f64) {
    let text = String::from_utf8_lossy(pdf);
    let start = text.find("/MediaBox").expect("a PDF page has a MediaBox");
    let rest = &text[start..];
    let open = rest.find('[').expect("the MediaBox is an array");
    let close = rest[open..].find(']').expect("the array closes");
    let numbers: Vec<f64> = rest[open + 1..open + close]
        .split_whitespace()
        .filter_map(|value| value.parse().ok())
        .collect();
    assert_eq!(numbers.len(), 4, "a MediaBox has four numbers");
    (numbers[2] - numbers[0], numbers[3] - numbers[1])
}

#[test]
fn the_insertion_order_of_the_graph_does_not_move_a_byte() {
    // S08 §7(h). The renderer sorts every collection it draws and `layout` bands by
    // `(kind, id)`, so shuffling the snapshot must be invisible. If it is not, some ordering
    // is coming from the database's row order and the golden is one query plan away from
    // churning.
    let mut forward = fixtures::model_a();
    let mut backward = fixtures::model_a();
    backward.elements.reverse();
    backward.relationships.reverse();
    backward.views.reverse();
    for view in &mut backward.views {
        view.objects.reverse();
        view.connections.reverse();
    }
    forward.normalize();
    backward.normalize();

    let view = fixtures::view_of(&forward);
    for format in [Format::Svg, Format::Png, Format::Pdf] {
        let left = render(&forward, view, format).expect("render forward");
        let right = render(&backward, view, format).expect("render backward");
        assert_eq!(
            left.sha256,
            right.sha256,
            "{} changed when the snapshot was reversed",
            format.as_str()
        );
    }

    // And the synthetic path, which has no stored view to sort by.
    let synthetic = render(&forward, None, Format::Svg).expect("synthetic forward");
    let synthetic_reversed = render(&backward, None, Format::Svg).expect("synthetic backward");
    assert_eq!(
        synthetic.sha256, synthetic_reversed.sha256,
        "the auto-laid-out view changed when the snapshot was reversed"
    );
}
