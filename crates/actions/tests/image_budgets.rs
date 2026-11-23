//! #571 — a budget that waits on a shared host must outlast that host.
//!
//! The mini fleet serves the estate's image, speech and transcription work, and it serves ONE job
//! at a time. So a caller's budget is not the work: it is the work plus whatever is queued ahead of
//! it. Measured 2026-11-22: a 50-step 512² render took **647 s** on the image host and the host
//! answered `200`, but the action's 600 s budget had already expired, so the engine abandoned a
//! render that had been performed and metered. The host's own ceiling is 900 s, which is why the
//! property below is `budget > ceiling` rather than a remembered number.
//!
//! The second test pins the budgets that deliberately do NOT move. Several of them are bounded by
//! another schema (the tool version's `1..=300` and the agent row's `timeout_secs 1..=300`) or by
//! S09 §5.5's read/write rule; raising those here would make an action's budget unreachable and
//! contradict the contract that caps the caller. A later "double everything" pass has to argue with
//! this test instead of silently crossing those caps.

use conveyor_actions::registry::registry;

/// The image host's own worst case, from the serving configuration (a 900 s request ceiling).
const IMAGE_HOST_CEILING_MS: u32 = 900_000;

fn budget(name: &str) -> u32 {
    registry()
        .get(name)
        .unwrap_or_else(|| panic!("{name} is not registered"))
        .timeout_ms
}

/// P1–P3: the three hosts the estate shares, and the judgement call that runs on one of them.
#[test]
fn every_shared_host_budget_outlasts_its_host() {
    assert_eq!(
        budget("media.generate_image"),
        1_200_000,
        "a render is minutes of work plus the queue ahead of it"
    );
    assert!(
        budget("media.generate_image") > IMAGE_HOST_CEILING_MS,
        "the render budget must exceed the host's own {}ms ceiling, or the engine gives up on a \
         render the host will still complete",
        IMAGE_HOST_CEILING_MS
    );
    assert_eq!(
        budget("produce.speech"),
        1_200_000,
        "narration shares the render's host and its queue"
    );
    assert_eq!(
        budget("media.transcribe"),
        600_000,
        "transcription shares the same queue"
    );
    assert_eq!(
        budget("laya.judge"),
        60_000,
        "a decision is an inference call, not a local read"
    );
}

/// P4: the budgets a schema or S09 §5.5 bounds stay where their bound puts them.
#[test]
fn a_budget_never_exceeds_the_cap_its_own_callers_enforce() {
    // The tool version's own timeout is 1..=300 s: a larger action budget could never be reached,
    // because the destination gives up first.
    assert_eq!(budget("tools.call"), 300_000);
    // S10 §5.5's accepted bound: the copilot's model call is 60 s.
    assert_eq!(budget("completions.create"), 60_000);
    // The agent row's `timeout_secs` is 1..=300; the workflow loop's `inference.chat` stays at
    // that cap (#270), so its caller's own timeout remains reachable.
    assert_eq!(budget("inference.chat"), 300_000);
    // S09 §5.5: an external read is 5 s, a dispatch is 30 s.
    assert_eq!(budget("retrieval.query"), 5_000);
    assert_eq!(budget("coding_jobs.get"), 5_000);
    assert_eq!(budget("coding_jobs.create"), 30_000);
}
