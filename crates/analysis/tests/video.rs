//! P2.1/P2.2 — `analyse.video` scores a supplied measurement bundle and is a pure function of it.
//!
//! The evidence is what the media executors produce (`media.analyze_video`, `media.analyze_audio`,
//! `media.ocr`); no process is spawned and nothing is re-measured here.

use conveyor_analysis::model::{Category, Severity};
use conveyor_analysis::video::{
    Audio, Container, CriterionResult, OcrText, Scene, Verification, VideoEvidence, analyse,
    group_of,
};

fn clip() -> VideoEvidence {
    VideoEvidence {
        url: "https://media.test/promo.mp4".to_owned(),
        container: Container {
            duration_s: 42.0,
            width: 1920,
            height: 1080,
            fps: 30.0,
            video_bitrate_kbps: Some(6000.0),
        },
        scenes: vec![
            Scene {
                start_s: 0.0,
                end_s: 20.0,
                mean_motion: Some(1.8),
            },
            Scene {
                start_s: 20.0,
                end_s: 42.0,
                mean_motion: Some(0.9),
            },
        ],
        ocr: vec![OcrText {
            at_s: 3.0,
            text: "SUMMER SALE".to_owned(),
        }],
        audio: Some(Audio {
            loudness_lufs: Some(-16.0),
            beat_count: Some(84),
        }),
        verification: None,
    }
}

#[test]
fn scoring_from_supplied_evidence() {
    let mut evidence = clip();
    evidence.container.height = 360; // below SD → a quality finding
    evidence.audio = None; // no track → an audio finding
    evidence.verification = Some(Verification {
        passed: false,
        criteria: vec![CriterionResult {
            name: "logo_visible".to_owned(),
            passed: false,
        }],
    });

    let report = analyse(&evidence);

    // Every finding belongs to the video category and carries the vid- namespace.
    for finding in &report.findings {
        assert_eq!(finding.category, Category::Video);
        assert!(
            finding.check_id().starts_with("vid-"),
            "{}",
            finding.check_id()
        );
    }
    // Findings are grouped by rubric group, and each present group is scored.
    let groups: std::collections::BTreeSet<String> = report
        .findings
        .iter()
        .map(|finding| group_of(finding.check_id()))
        .collect();
    assert!(groups.contains("quality"));
    assert!(groups.contains("audio"));
    assert!(groups.contains("verification"));
    for group in &groups {
        assert!(
            report.scorecard.buckets.contains_key(group),
            "the {group} bucket exists"
        );
    }
    let verification = report
        .findings
        .iter()
        .find(|finding| finding.check_id() == "vid-verification-failed")
        .expect("the failed criterion is recorded");
    assert_eq!(verification.severity, Severity::Critical);
    assert!(report.scorecard.overall_score < 100.0);
}

#[test]
fn deterministic_output() {
    let first = analyse(&clip());
    let second = analyse(&clip());
    assert_eq!(
        serde_json::to_string(&first).expect("the report serialises"),
        serde_json::to_string(&second).expect("the report serialises"),
        "identical evidence must produce byte-identical reports"
    );
}

#[test]
fn a_clean_clip_is_clean() {
    let report = analyse(&clip());
    assert!(report.findings.is_empty(), "{:?}", report.findings);
    assert_eq!(report.scorecard.overall_score, 100.0);
}
