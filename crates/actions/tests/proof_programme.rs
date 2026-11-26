//! #746 — programme certificates: one leaf per built slice, one bundle over the set.
//!
//! The issue's P4 scenarios. They are pure: the live check that the bundle is accepted by the real
//! prover is recorded in `docs/evidence/proof-of-delivery.md`, and the model itself is checked
//! live by `lean_corpus`.
//!
//! `rbuild -- test -p conveyor-actions --test proof_programme`.

use conveyor_actions::proof::programme::{self, Leaf};
use conveyor_actions::proof::render::{self, RenderError};

/// A distinct, well-formed digest per leaf.
fn digest(index: usize) -> String {
    format!("{:02x}", index + 1).repeat(32)
}

fn leaves(expected: &[String]) -> Vec<Leaf> {
    expected
        .iter()
        .enumerate()
        .map(|(index, slice)| Leaf::new(slice, digest(index)))
        .collect()
}

/// The spec pin must be the digest of the spec in this tree, or every certificate answers a
/// document nobody can find.
#[test]
fn the_pinned_spec_digest_matches_the_spec() {
    let actual = render::sha256_hex(include_bytes!("../../../docs/SPEC_V2.md"));
    assert_eq!(
        actual,
        programme::SPEC_V2_SHA256,
        "docs/SPEC_V2.md changed; re-read README.md § Model drift before updating the pin"
    );
}

/// The expected set is the §12 slice table, minus the slices still proposed (`T18`). Derived from
/// the spec text rather than trusted, so adding a slice to §12 without a certificate fails here.
#[test]
fn the_expected_set_is_the_slice_table() {
    let spec = include_str!("../../../docs/SPEC_V2.md");
    let mut inside = false;
    let mut slices: Vec<String> = Vec::new();
    for line in spec.lines() {
        if line.starts_with("## 12. Delivery plan") {
            inside = true;
            continue;
        }
        if inside && line.starts_with("## 13.") {
            break;
        }
        if !inside {
            continue;
        }
        if let Some(rest) = line.strip_prefix("| **") {
            if let Some(end) = rest.find("**") {
                slices.push(rest[..end].to_owned());
            }
        }
    }

    slices.retain(|slice| slice != "T18");
    slices.sort();
    let mut expected = programme::expected();
    expected.sort();
    assert_eq!(
        expected, slices,
        "the bundle's expected set and §12 disagree"
    );
}

/// P4.1 — every built slice has a leaf, and the bundle renders with all of them.
#[test]
fn each_built_slice_has_a_certificate() {
    let expected = programme::expected();
    assert_eq!(expected.len(), 20, "the V2 slices, per SPEC_V2 §12");
    let facts = programme::facts(&expected, &leaves(&expected));
    let source = render::render(&facts, "programme_bundle").expect("a complete bundle renders");

    for slice in &expected {
        assert!(
            source.contains(&format!("(\"{slice}\", ")),
            "the bundle names {slice}: {source}"
        );
    }
    assert!(source.contains("theorem programme_bundle : checkProgramme facts = true"));
}

/// P4.2 — a bundle with a slice removed is refused, and the refusal names the slice.
#[test]
fn the_bundle_fails_when_a_slice_is_dropped() {
    let expected = programme::expected();
    let leaves: Vec<Leaf> = leaves(&expected)
        .into_iter()
        .filter(|leaf| leaf.slice != "T5")
        .collect();
    let facts = programme::facts(&expected, &leaves);

    let error = render::render(&facts, "programme_bundle").expect_err("a dropped slice");
    assert!(matches!(error, RenderError::Incomplete(_)), "{error:?}");
    assert!(
        format!("{error:?}").contains("T5"),
        "the item is named: {error:?}"
    );
}

/// A leaf that reuses another slice's certificate is refused: it would claim a slice it never
/// proved.
#[test]
fn a_shared_certificate_is_refused() {
    let expected = programme::expected();
    let mut leaves = leaves(&expected);
    leaves[1].certificate_sha256 = leaves[0].certificate_sha256.clone();
    let facts = programme::facts(&expected, &leaves);

    let error = render::render(&facts, "programme_bundle").expect_err("a shared certificate");
    assert!(
        format!("{error:?}").contains("backs more than one leaf"),
        "{error:?}"
    );
}
