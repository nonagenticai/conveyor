//! P5.2 — two brand snapshots compared: the changed attributes are named with both values, and the
//! comparison's own numbers (similarity, colour distance) move in the direction the values moved.

use conveyor_analysis::brand::{BrandSnapshot, FontInfo, PageFacts, SiteFacts, compare, extract};

fn font(family: &str) -> FontInfo {
    FontInfo {
        family: family.to_owned(),
    }
}

fn snapshot(name: &str, primary: &str, fonts: &[&str]) -> BrandSnapshot {
    BrandSnapshot {
        name: name.to_owned(),
        domain: format!("{name}.example"),
        primary_color: primary.to_owned(),
        has_logo: true,
        fonts: fonts.iter().map(|family| font(family)).collect(),
        ..BrandSnapshot::default()
    }
}

#[test]
fn changed_attributes_are_named_with_their_direction() {
    let before = snapshot("acme", "#2563EB", &["Inter"]);
    let after = snapshot("globex", "#FF0000", &["Inter"]);
    let comparison = compare(&before, &after);

    let changed = comparison.changed_fields();
    assert!(changed.contains(&"name"));
    assert!(changed.contains(&"primary_color"));

    let name = comparison
        .field_comparisons
        .iter()
        .find(|row| row.field == "name")
        .expect("the comparison names every compared field");
    assert_eq!(name.value_a, "acme", "the direction starts at value_a");
    assert_eq!(name.value_b, "globex", "and ends at value_b");
    assert!(!name.matched);

    // The unchanged font set must not be reported as changed.
    let fonts = comparison
        .field_comparisons
        .iter()
        .find(|row| row.field == "fonts")
        .expect("fonts are compared");
    assert!(fonts.matched, "an identical font set matches");
    assert!(!changed.contains(&"fonts"));
}

#[test]
fn identical_snapshots_are_fully_similar_and_have_no_changes() {
    let brand = snapshot("acme", "#2563EB", &["Inter", "Source Sans"]);
    let comparison = compare(&brand, &brand);
    assert_eq!(comparison.overall_similarity, 1.0);
    assert_eq!(comparison.color_distance, 0.0);
    assert!(comparison.changed_fields().is_empty());
}

#[test]
fn a_far_colour_moves_similarity_down_and_distance_up() {
    let blue = snapshot("acme", "#0000FF", &["Inter"]);
    let red = snapshot("acme", "#FF0000", &["Inter"]);
    let same = compare(&blue, &blue);
    let different = compare(&blue, &red);
    assert!(different.color_distance > same.color_distance);
    assert!(different.overall_similarity < same.overall_similarity);
    // Euclidean distance between pure blue and pure red is 255 * sqrt(2).
    let expected = 255.0_f64 * 2.0_f64.sqrt();
    assert_eq!(
        different.color_distance,
        format!("{expected:.4}").parse::<f64>().unwrap()
    );
}

#[test]
fn extraction_is_a_pure_function_of_the_supplied_facts() {
    let pages = [PageFacts {
        url: "https://www.acme.example/pricing".to_owned(),
        title: "Acme | Home".to_owned(),
        logo_candidates: vec!["https://www.acme.example/logo.svg".to_owned()],
        og_image: Some("https://www.acme.example/card.png".to_owned()),
    }];
    let site = SiteFacts {
        description: String::new(),
        primary_color: Some("#112233".to_owned()),
    };
    let first = extract(&pages, Some(&site));
    let second = extract(&pages, Some(&site));
    assert_eq!(first, second);
    assert_eq!(first.domain, "acme.example");
    assert_eq!(first.name, "Acme"); // the "| Home" suffix is not the brand
    assert_eq!(first.logo_url, "https://www.acme.example/logo.svg");
    assert_eq!(first.primary_color, "#112233");
    assert_eq!(first.secondary_color, "#0d1b28");
}
