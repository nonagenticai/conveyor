//! The `/api/internal/*` guard.
//!
//! Three rules, in this order, because the order is the contract:
//!
//! 1. A request whose `Host` / `X-Forwarded-Host` names the public ingress answers **404**. The
//!    public VirtualService routes `api.nonagentic.ai/inference-router/*` to this pod with the
//!    prefix stripped, so without this rule every tenant's correlation ids, tenant ids and
//!    licence ids would be readable through the front door. 404 (not 403) so the route is not
//!    advertised.
//! 2. `INFERENCE_ROUTER_INTERNAL_SECRET` unset, blank or whitespace answers **503**: a
//!    misconfigured pod serves nothing rather than everything.
//! 3. A missing or wrong `X-Internal-Secret` answers **401**, compared in constant time.

use axum::http::{HeaderMap, StatusCode};

use crate::config::Config;
use crate::routing::Refusal;

/// The reference's body for a request through the public ingress.
pub const NOT_FOUND: &str = "Not Found";
/// The reference's body for an unconfigured pod.
pub const SECRET_UNCONFIGURED: &str =
    "internal routes are disabled: INFERENCE_ROUTER_INTERNAL_SECRET is not set";
/// The reference's body for a missing or wrong header.
pub const SECRET_INVALID: &str = "invalid X-Internal-Secret";

/// Refuse the request, or return `Ok(())` for the six handlers to proceed.
pub fn require(headers: &HeaderMap, config: &Config) -> Result<(), Refusal> {
    if request_is_public(headers, &config.public_host_suffixes) {
        return Err(Refusal::detail(
            StatusCode::NOT_FOUND,
            NOT_FOUND,
            "internal_public_host",
        ));
    }
    let expected = config.private_api_secret.as_deref().unwrap_or("").trim();
    if expected.is_empty() {
        return Err(Refusal::detail(
            StatusCode::SERVICE_UNAVAILABLE,
            SECRET_UNCONFIGURED,
            "private_api_secret_unconfigured",
        ));
    }
    let presented = headers
        .get("x-internal-secret")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .trim();
    if presented.is_empty() || !constant_time_eq(presented.as_bytes(), expected.as_bytes()) {
        return Err(Refusal::detail(
            StatusCode::UNAUTHORIZED,
            SECRET_INVALID,
            "private_api_secret_invalid",
        ));
    }
    Ok(())
}

/// True when `Host` or any `X-Forwarded-Host` value ends with a public suffix.
///
/// Shared with `/api/admin/*`: the reference refuses a public ingress host there for the same
/// reason, and a second copy of this rule is how the two surfaces drift apart.
pub(crate) fn request_is_public(headers: &HeaderMap, suffixes: &[String]) -> bool {
    for name in ["host", "x-forwarded-host"] {
        let Some(raw) = headers.get(name).and_then(|value| value.to_str().ok()) else {
            continue;
        };
        for value in raw.split(',') {
            let value = value.trim();
            if value.is_empty() {
                continue;
            }
            let host = value
                .rsplit_once(':')
                .map(|(host, _)| host)
                .unwrap_or(value)
                .trim()
                .to_ascii_lowercase();
            if host.is_empty() {
                continue;
            }
            for suffix in suffixes {
                let wanted = suffix.trim().trim_start_matches('.').to_ascii_lowercase();
                if !wanted.is_empty() && (host == wanted || host.ends_with(&format!(".{wanted}"))) {
                    return true;
                }
            }
        }
    }
    false
}

/// Constant-time equality for equal-length secrets. A length mismatch returns immediately —
/// the length is not the secret, and `hmac.compare_digest` behaves the same way.
fn constant_time_eq(presented: &[u8], expected: &[u8]) -> bool {
    if presented.len() != expected.len() {
        return false;
    }
    let mut difference = 0_u8;
    for (left, right) in presented.iter().zip(expected) {
        difference |= left ^ right;
    }
    difference == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn config(secret: Option<&str>) -> Config {
        let secret = secret.map(str::to_owned);
        Config::from_lookup(move |key| match key {
            "INFERENCE_ROUTER_INTERNAL_SECRET" => secret.clone(),
            _ => None,
        })
        .expect("config")
    }

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for (name, value) in pairs {
            headers.insert(
                axum::http::HeaderName::from_bytes(name.as_bytes()).expect("header name"),
                HeaderValue::from_str(value).expect("header value"),
            );
        }
        headers
    }

    #[test]
    fn the_right_secret_passes() {
        let config = config(Some("stub-internal-secret"));
        assert!(
            require(
                &headers(&[("x-internal-secret", "stub-internal-secret")]),
                &config
            )
            .is_ok()
        );
        // The reference trims the presented value.
        assert!(
            require(
                &headers(&[("x-internal-secret", "  stub-internal-secret ")]),
                &config
            )
            .is_ok()
        );
    }

    #[test]
    fn a_missing_wrong_or_blank_secret_is_unauthorized() {
        let config = config(Some("stub-internal-secret"));
        for presented in [None, Some(""), Some("   "), Some("wrong")] {
            let request = match presented {
                Some(value) => headers(&[("x-internal-secret", value)]),
                None => HeaderMap::new(),
            };
            let refusal = require(&request, &config).expect_err("refused");
            assert_eq!(refusal.status, StatusCode::UNAUTHORIZED);
            assert_eq!(refusal.body["detail"], SECRET_INVALID);
        }
    }

    #[test]
    fn an_unset_or_blank_configured_secret_fails_closed() {
        for secret in [None, Some(""), Some("   ")] {
            let config = config(secret);
            let refusal = require(&headers(&[("x-internal-secret", "anything")]), &config)
                .expect_err("refused");
            assert_eq!(refusal.status, StatusCode::SERVICE_UNAVAILABLE);
            assert_eq!(refusal.body["detail"], SECRET_UNCONFIGURED);
        }
    }

    #[test]
    fn a_public_host_is_not_found_even_with_the_right_secret() {
        let config = config(Some("stub-internal-secret"));
        for host in [
            "api.nonagentic.ai",
            "API.NonAgentic.AI:443",
            "x.nonagentic.ai",
        ] {
            let refusal = require(
                &headers(&[
                    ("host", host),
                    ("x-internal-secret", "stub-internal-secret"),
                ]),
                &config,
            )
            .expect_err("refused");
            assert_eq!(refusal.status, StatusCode::NOT_FOUND, "{host}");
            assert_eq!(refusal.body["detail"], NOT_FOUND);
        }
        // X-Forwarded-Host is checked too, including a comma-separated chain.
        let refusal = require(
            &headers(&[
                ("host", "inference-router.svc.cluster.local"),
                (
                    "x-forwarded-host",
                    "edge.svc.cluster.local, api.nonagentic.ai",
                ),
                ("x-internal-secret", "stub-internal-secret"),
            ]),
            &config,
        )
        .expect_err("refused");
        assert_eq!(refusal.status, StatusCode::NOT_FOUND);
    }

    #[test]
    fn the_public_host_rule_wins_over_an_unconfigured_secret() {
        let config = config(None);
        let refusal =
            require(&headers(&[("host", "api.nonagentic.ai")]), &config).expect_err("refused");
        assert_eq!(refusal.status, StatusCode::NOT_FOUND);
    }

    #[test]
    fn in_cluster_hosts_are_not_public() {
        let config = config(Some("stub-internal-secret"));
        for host in [
            "inference-router.svc.cluster.local",
            "inference-router.default.svc.cluster.local",
            "127.0.0.1:8150",
        ] {
            assert!(
                require(
                    &headers(&[
                        ("host", host),
                        ("x-internal-secret", "stub-internal-secret")
                    ]),
                    &config
                )
                .is_ok(),
                "{host} must not read as public"
            );
        }
    }
}
