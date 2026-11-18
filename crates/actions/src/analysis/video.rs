//! `analyse.video` — the video rubric over the executor's **measured** evidence.
//!
//! The measurements arrive in the input: `media.analyze_video`, `media.analyze_audio` and
//! `media.ocr` produced them in a Job, and the caller supplies them here. Nothing is spawned, no
//! document is read, no row is written: a replay of the same evidence returns the same report.

use conveyor_analysis::model::Report;
use conveyor_analysis::video::{VideoEvidence, analyse};
use sqlx::PgConnection;

use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;

/// Score a video from supplied executor evidence.
#[crate::action(
    name = "analyse.video",
    version = 1,
    kind = "internal",
    scopes = ["documents:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn analyse_video(
    _tx: &mut PgConnection,
    _ctx: HandlerCtx,
    input: VideoEvidence,
) -> Result<HandlerOutput<Report>, ActionError> {
    Ok(HandlerOutput::ok(analyse(&input)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use conveyor_analysis::video::{Audio, Container, Scene};

    #[test]
    fn the_evidence_is_the_only_input() {
        let evidence = VideoEvidence {
            url: "https://example.test/x.mp4".to_owned(),
            container: Container {
                duration_s: 12.0,
                width: 1280,
                height: 720,
                fps: 30.0,
                video_bitrate_kbps: Some(2500.0),
            },
            scenes: vec![Scene {
                start_s: 0.0,
                end_s: 12.0,
                mean_motion: Some(1.1),
            }],
            ocr: Vec::new(),
            audio: Some(Audio {
                loudness_lufs: Some(-16.0),
                beat_count: Some(20),
            }),
            verification: None,
        };
        let report = analyse(&evidence);
        assert!(report.findings.is_empty(), "{:?}", report.findings);
        assert_eq!(report.scorecard.overall_score, 100.0);
    }
}
