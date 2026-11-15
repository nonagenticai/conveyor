//! The runner's job token: mint, verify, and every refusal the byte routes depend on.
//!
//! `cargo test -p conveyor-auth --test job_token`. The row fence — "the job finished", "the attempt
//! was superseded", "the tenant does not match" — is NOT tested here: it lives where the job row is
//! read, in `crates/api/tests/job_routes.rs`. This file tests the credential primitive only.

use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
use conveyor_auth::job_token::{
    self, JobClaims, JobTokenError, JobTokenKey, JOB_TOKEN_ENV, MAX_TTL,
};
use conveyor_domain::TenantId;
use uuid::Uuid;

fn key() -> JobTokenKey {
    JobTokenKey::parse(&"ab".repeat(32)).expect("64 hex characters is 32 bytes")
}

fn other_key() -> JobTokenKey {
    JobTokenKey::parse(&"cd".repeat(32)).expect("a second key")
}

fn claims_at(expires_at: chrono::DateTime<Utc>) -> JobClaims {
    JobClaims {
        job_id: Uuid::now_v7(),
        tenant: TenantId::new(),
        attempt: 1,
        expires_at,
    }
}

#[test]
fn a_minted_token_round_trips() {
    let now = Utc::now();
    let claims = claims_at(job_token::expires_at(now, Duration::from_secs(60)));
    let token = job_token::mint(&key(), &claims);
    let verified = job_token::verify(&key(), &token, now).expect("the token it minted verifies");
    assert_eq!(verified, claims);
    println!("token: {token}");
}

#[test]
fn a_token_from_another_key_is_refused() {
    let now = Utc::now();
    let token = job_token::mint(&other_key(), &claims_at(now + ChronoDuration::minutes(5)));
    assert_eq!(
        job_token::verify(&key(), &token, now),
        Err(JobTokenError::BadMac),
        "a token signed elsewhere never verifies here"
    );
}

#[test]
fn a_tampered_payload_is_refused() {
    let now = Utc::now();
    let token = job_token::mint(&key(), &claims_at(now + ChronoDuration::minutes(5)));
    let mut parts: Vec<&str> = token.split('.').collect();
    let forged_payload = base64::Engine::encode(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
        br#"{"j":"00000000-0000-0000-0000-000000000000","t":"00000000-0000-0000-0000-000000000000","a":1,"e":9999999999}"#,
    );
    parts[1] = &forged_payload;
    let forged = parts.join(".");
    assert_eq!(
        job_token::verify(&key(), &forged, now),
        Err(JobTokenError::BadMac),
        "changing the claims without the key fails the MAC"
    );
}

#[test]
fn an_expired_token_is_refused() {
    let now = Utc::now();
    let claims = claims_at(now - ChronoDuration::seconds(1));
    let token = job_token::mint(&key(), &claims);
    assert_eq!(
        job_token::verify(&key(), &token, now),
        Err(JobTokenError::Expired)
    );
}

#[test]
fn a_foreign_version_is_refused() {
    let now = Utc::now();
    let token = job_token::mint(&key(), &claims_at(now + ChronoDuration::minutes(5)));
    let (_, rest) = token.split_once('.').expect("three parts");
    let relabelled = format!("v2.{rest}");
    assert_eq!(
        job_token::verify(&key(), &relabelled, now),
        Err(JobTokenError::UnsupportedVersion),
        "the version is inside the MAC, and an unknown one is named as such"
    );
}

#[test]
fn malformed_tokens_are_refused_without_panicking() {
    let now = Utc::now();
    for token in ["", "v1", "v1.", "v1.a.b.c", "not-a-token", "v1.????.????"] {
        let result = job_token::verify(&key(), token, now);
        assert!(
            matches!(
                result,
                Err(JobTokenError::Malformed | JobTokenError::BadMac)
            ),
            "{token:?} produced {result:?}"
        );
    }
}

#[test]
fn the_key_accepts_hex_base64_and_raw_32_bytes() {
    let hex = "ab".repeat(32);
    let raw = "x".repeat(32);
    let b64 = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        [7u8; 32].as_slice(),
    );
    for candidate in [hex, raw, b64] {
        assert!(
            JobTokenKey::parse(&candidate).is_ok(),
            "{candidate:?} should parse as a 32-byte key"
        );
    }
    for bad in ["", "short", &"y".repeat(31), &"z".repeat(33)] {
        assert_eq!(
            JobTokenKey::parse(bad).err(),
            Some(JobTokenError::KeyMalformed),
            "{bad:?} is not a key"
        );
    }
}

#[test]
fn the_expiry_is_clamped_to_a_day() {
    let now = Utc::now();
    let clamped = job_token::expires_at(now, Duration::from_secs(365 * 24 * 60 * 60));
    let expected = now.timestamp() + MAX_TTL.as_secs() as i64;
    assert_eq!(
        clamped.timestamp(),
        expected,
        "a year-long request is clamped to the day this code will honour"
    );
}

#[test]
fn an_unconfigured_key_is_none_and_never_a_default() {
    // The variable is process-wide, so this assertion only speaks when the environment really is
    // clean; a configured key is exercised by `the_key_accepts_hex_base64_and_raw_32_bytes`.
    if std::env::var(JOB_TOKEN_ENV).is_ok() {
        return;
    }
    assert!(
        JobTokenKey::from_env()
            .expect("reading an absent variable is not an error")
            .is_none(),
        "an unset {} must refuse every token, not fall back to a zero key",
        JOB_TOKEN_ENV
    );
}
