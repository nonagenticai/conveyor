//! P6.2 — a retried provider submission carries the same `(job_id, attempt)` key as the first
//! attempt, so a retry cannot double-spend; a new attempt carries a new key.

use conveyor_analysis::providers::{IdempotencyKey, Provider, Submission, build};

const PROVIDERS: [Provider; 7] = [
    Provider::Kling,
    Provider::Runway,
    Provider::Veo,
    Provider::Wan,
    Provider::Seedance,
    Provider::Sora,
    Provider::OpenAi,
];

fn submission(provider: Provider, attempt: u32) -> Submission {
    Submission {
        provider,
        prompt: "A slate-blue heron over a still canal at dawn".to_owned(),
        model: "default".to_owned(),
        duration_s: 5,
        negative_prompt: Some("text, watermark".to_owned()),
        image_url: None,
        idempotency: IdempotencyKey::new("job-01a1e491", attempt),
    }
}

fn idempotency_header(request: &conveyor_analysis::providers::ProviderRequest) -> Option<&str> {
    request
        .headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("idempotency-key"))
        .map(|(_, value)| value.as_str())
}

#[test]
fn a_retried_attempt_carries_the_same_key() {
    let first = build(&submission(Provider::Kling, 1));
    let retry = build(&submission(Provider::Kling, 1));
    assert_eq!(
        idempotency_header(&first),
        idempotency_header(&retry),
        "the retry of attempt 1 must not mint a new key"
    );
    assert_eq!(
        idempotency_header(&first),
        Some("job-01a1e491:1"),
        "the key is the (job_id, attempt) pair, verbatim"
    );
}

#[test]
fn a_new_attempt_carries_a_new_key() {
    let first = build(&submission(Provider::Kling, 1));
    let second = build(&submission(Provider::Kling, 2));
    assert_ne!(idempotency_header(&first), idempotency_header(&second));
}

#[test]
fn every_provider_builds_a_submission_with_the_key() {
    for provider in PROVIDERS {
        let request = build(&submission(provider, 1));
        match provider {
            Provider::Wan => {
                // Wan is a local pipeline, not an endpoint: the source's own `LOCAL` shape.
                assert_eq!(request.method, "LOCAL");
                assert!(request.path.starts_with("local://"));
            }
            _ => {
                assert_eq!(request.method, "POST", "a submission is a POST");
                assert!(
                    request.path.starts_with('/'),
                    "the path is joined to the provider base URL: {}",
                    request.path
                );
            }
        }
        assert!(
            !request.path.is_empty(),
            "{} needs a destination",
            provider.name()
        );
        assert!(
            idempotency_header(&request).is_some(),
            "{} must carry the retry key",
            provider.name()
        );
        let rendered = serde_json::to_string(&request.body).expect("the body serialises");
        assert!(
            rendered.contains("heron"),
            "{} must carry the prompt; body was {rendered}",
            provider.name()
        );
    }
}
