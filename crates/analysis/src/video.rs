//! `analyse.video` — a rubric-scored read over a video's **measured** properties.
//!
//! The measurements are the executor's (`media.analyze_video`, `media.analyze_audio`,
//! `media.ocr`); this module never spawns ffmpeg, reads a file or touches the network. Given the
//! same evidence it returns the same report, byte for byte.
//!
//! Findings carry the source's `vid-<group>-<check>` namespace and are scored per group by the
//! shared decay kernel, so a video verdict and a page verdict cannot drift apart.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::model::{Category, Finding, Report, Scorecard, Severity};
use crate::rubric::{Rubric, dedupe, grade, round_1dp};

/// The video's container facts, as the executor measured them.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct Container {
    pub duration_s: f64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    #[serde(default)]
    pub video_bitrate_kbps: Option<f64>,
}

/// One detected scene (shot), with its mean frame-to-frame motion when measured.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct Scene {
    pub start_s: f64,
    pub end_s: f64,
    #[serde(default)]
    pub mean_motion: Option<f64>,
}

/// One OCR result at a timestamp.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct OcrText {
    pub at_s: f64,
    pub text: String,
}

/// The audio facts the executor extracted; absent when the file has no audio stream.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct Audio {
    #[serde(default)]
    pub loudness_lufs: Option<f64>,
    #[serde(default)]
    pub beat_count: Option<u32>,
}

/// One verification criterion's outcome.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct CriterionResult {
    pub name: String,
    pub passed: bool,
}

/// The reference-matching verdict, when a verification was requested.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct Verification {
    pub passed: bool,
    #[serde(default)]
    pub criteria: Vec<CriterionResult>,
}

/// Everything `analyse.video` scores. The caller supplies what the executors measured.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct VideoEvidence {
    #[serde(default)]
    pub url: String,
    pub container: Container,
    #[serde(default)]
    pub scenes: Vec<Scene>,
    #[serde(default)]
    pub ocr: Vec<OcrText>,
    #[serde(default)]
    pub audio: Option<Audio>,
    #[serde(default)]
    pub verification: Option<Verification>,
}

fn finding(
    check_id: &str,
    severity: Severity,
    title: impl Into<String>,
    description: impl Into<String>,
    url: &str,
) -> Finding {
    Finding {
        check_id: Some(check_id.to_owned()),
        category: Category::Video,
        severity,
        title: title.into(),
        description: description.into(),
        url: url.to_owned(),
        source_tool: Some("video_analysis".to_owned()),
        scanner_count: 1,
        ..Finding::default()
    }
}

/// Analyse one video from its measured evidence.
pub fn analyse(evidence: &VideoEvidence) -> Report {
    let url = evidence.url.as_str();
    let mut findings = Vec::new();
    let container = &evidence.container;

    // Quality: the delivery floor a viewer actually receives.
    if container.height > 0 && container.height < 480 {
        findings.push(finding(
            "vid-quality-low-resolution",
            Severity::High,
            "Video is below SD resolution",
            format!(
                "The video is {}×{} — below 480p, which reads as broken at any normal viewing size.",
                container.width, container.height
            ),
            url,
        ));
    } else if container.height > 0 && container.height < 720 {
        findings.push(finding(
            "vid-quality-below-hd",
            Severity::Medium,
            "Video is below 720p",
            format!(
                "The video is {}×{} — below the 720p floor for a published clip.",
                container.width, container.height
            ),
            url,
        ));
    }
    if container.fps > 0.0 && container.fps < 24.0 {
        findings.push(finding(
            "vid-quality-low-fps",
            Severity::Medium,
            "Frame rate is below 24 fps",
            format!(
                "The video runs at {:.2} fps; below 24 fps, motion reads as stutter rather than movement.",
                container.fps
            ),
            url,
        ));
    }
    if let Some(bitrate) = container.video_bitrate_kbps {
        let floor = if container.height >= 1080 {
            2500.0
        } else {
            1000.0
        };
        if bitrate < floor {
            findings.push(finding(
                "vid-quality-low-bitrate",
                Severity::Low,
                "Video bitrate is low for its resolution",
                format!(
                    "{bitrate:.0} kbps at {}p is below the {floor:.0} kbps floor; compression artefacts will show in motion.",
                    container.height
                ),
                url,
            ));
        }
    }

    // Duration: a clip too short to say anything, or too long to hold attention.
    if container.duration_s > 0.0 && container.duration_s < 5.0 {
        findings.push(finding(
            "vid-duration-short",
            Severity::Medium,
            "Video is shorter than 5 seconds",
            format!(
                "The video runs {:.1}s — too short for a hook and a payoff.",
                container.duration_s
            ),
            url,
        ));
    } else if container.duration_s > 180.0 {
        findings.push(finding(
            "vid-duration-long",
            Severity::Low,
            "Video is longer than three minutes",
            format!(
                "The video runs {:.1}s; long-form needs a reason to be this long.",
                container.duration_s
            ),
            url,
        ));
    }

    // Motion: a single static frame is a slide, not a video.
    if evidence.scenes.is_empty() {
        findings.push(finding(
            "vid-motion-no-scenes",
            Severity::Info,
            "No scene changes were detected",
            "The measurement found no shot boundary; either the clip is one continuous take or the detector saw a static frame.",
            url,
        ));
    } else {
        let motions: Vec<f64> = evidence
            .scenes
            .iter()
            .filter_map(|scene| scene.mean_motion)
            .collect();
        if !motions.is_empty() && motions.iter().all(|motion| *motion < 0.5) {
            findings.push(finding(
                "vid-motion-static",
                Severity::Low,
                "Every scene is nearly static",
                format!(
                    "All {} measured scenes average under 0.5 motion; the clip reads as a slide deck.",
                    motions.len()
                ),
                url,
            ));
        }
    }

    // Audio: a missing track on a published clip is a defect; loudness far off target hurts.
    match &evidence.audio {
        None => findings.push(finding(
            "vid-audio-missing",
            Severity::Medium,
            "Video has no audio track",
            "The file carries no audio stream; a published clip is expected to.",
            url,
        )),
        Some(audio) => {
            if let Some(loudness) = audio
                .loudness_lufs
                .filter(|loudness| !(-24.0..=-14.0).contains(loudness))
            {
                findings.push(finding(
                        "vid-audio-loudness-out-of-range",
                        Severity::Low,
                        "Audio loudness is outside −24…−14 LUFS",
                        format!(
                            "Integrated loudness is {loudness:.1} LUFS; outside −24…−14 LUFS the clip is too quiet or clips on playback."
                        ),
                        url,
                    ));
            }
        }
    }

    // Text: dense burned-in text is fine, but a wall of it is unreadable at speed.
    let text_chars: usize = evidence
        .ocr
        .iter()
        .map(|item| item.text.chars().count())
        .sum();
    if text_chars > 2000 {
        findings.push(finding(
            "vid-text-dense",
            Severity::Info,
            "Dense on-screen text",
            format!(
                "OCR read {text_chars} characters; at this density the text is on screen too briefly to read."
            ),
            url,
        ));
    }

    // Verification: a failed criterion is the strongest signal the family returns.
    if let Some(verification) = &evidence.verification {
        let failed: Vec<&str> = verification
            .criteria
            .iter()
            .filter(|criterion| !criterion.passed)
            .map(|criterion| criterion.name.as_str())
            .collect();
        if !failed.is_empty() || !verification.passed {
            findings.push(finding(
                "vid-verification-failed",
                Severity::Critical,
                "Video verification failed",
                if failed.is_empty() {
                    "The verifier reported a failure without naming a criterion.".to_owned()
                } else {
                    format!("Failed criteria: {}.", failed.join(", "))
                },
                url,
            ));
        }
    }

    let findings = dedupe(findings);
    let scorecard = scorecard(&findings);
    Report {
        url: evidence.url.clone(),
        findings,
        scorecard,
    }
}

/// The rubric group a video finding belongs to: the second segment of `vid-<group>-<check>`.
pub fn group_of(check_id: &str) -> String {
    let mut parts = check_id.split('-');
    match (parts.next(), parts.next()) {
        (Some("vid"), Some(group)) => group.to_owned(),
        _ => "other".to_owned(),
    }
}

/// Score each present group by the decay law and take the equal-weighted mean; a clean, complete
/// scan is 100 (`overall_score_for_complete_scan`).
fn scorecard(findings: &[Finding]) -> Scorecard {
    let rubric = Rubric::default();
    let groups: BTreeSet<String> = findings
        .iter()
        .filter(|finding| !finding.check_id().is_empty())
        .map(|finding| group_of(finding.check_id()))
        .collect();
    let mut buckets = BTreeMap::new();
    let mut total = 0.0;
    for group in &groups {
        let penalty: i32 = findings
            .iter()
            .filter(|finding| group_of(finding.check_id()) == *group)
            .map(|finding| rubric.severity.weight(finding.severity))
            .sum();
        let score = rubric.score_for_penalty(penalty);
        buckets.insert(group.clone(), score);
        total += score;
    }
    let overall_score = if groups.is_empty() {
        100.0
    } else {
        round_1dp(total / groups.len() as f64)
    };
    Scorecard {
        overall_score,
        grade: grade(overall_score),
        buckets,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Grade;

    fn healthy() -> VideoEvidence {
        VideoEvidence {
            url: "https://media.test/clip.mp4".to_owned(),
            container: Container {
                duration_s: 30.0,
                width: 1920,
                height: 1080,
                fps: 30.0,
                video_bitrate_kbps: Some(5000.0),
            },
            scenes: vec![Scene {
                start_s: 0.0,
                end_s: 30.0,
                mean_motion: Some(2.0),
            }],
            ocr: Vec::new(),
            audio: Some(Audio {
                loudness_lufs: Some(-16.0),
                beat_count: Some(60),
            }),
            verification: None,
        }
    }

    #[test]
    fn a_healthy_clip_produces_no_finding_and_scores_one_hundred() {
        let report = analyse(&healthy());
        assert!(report.findings.is_empty(), "{:?}", report.findings);
        assert_eq!(report.scorecard.overall_score, 100.0);
        assert_eq!(report.scorecard.grade, Grade::APlus);
    }

    #[test]
    fn each_group_reports_its_own_finding() {
        let mut evidence = healthy();
        evidence.container.height = 360;
        evidence.audio = None;
        let report = analyse(&evidence);
        let ids: Vec<&str> = report.findings.iter().map(Finding::check_id).collect();
        assert!(ids.contains(&"vid-quality-low-resolution"));
        assert!(ids.contains(&"vid-audio-missing"));
        assert!(report.scorecard.buckets.contains_key("quality"));
        assert!(report.scorecard.buckets.contains_key("audio"));
    }

    #[test]
    fn a_failed_verification_is_critical_and_names_the_criteria() {
        let mut evidence = healthy();
        evidence.verification = Some(Verification {
            passed: false,
            criteria: vec![CriterionResult {
                name: "product_visible".to_owned(),
                passed: false,
            }],
        });
        let report = analyse(&evidence);
        let finding = report
            .findings
            .iter()
            .find(|finding| finding.check_id() == "vid-verification-failed")
            .expect("the failed verification produces a finding");
        assert_eq!(finding.severity, Severity::Critical);
        assert!(finding.description.contains("product_visible"));
    }
}
