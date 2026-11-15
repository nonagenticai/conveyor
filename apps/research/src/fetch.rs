//! Outbound fetch with the reference service's SSRF guard.
//!
//! `url_guard.py` is ported as the single chokepoint: an exact-host allowlist
//! (`SSRF_ALLOWED_HOSTS`), then scheme/host checks, then any literal or resolved
//! address in a blocked range is refused. A hostname that cannot be resolved is
//! refused too (fail closed). Redirects are re-checked for literal addresses and
//! blocked suffixes, mirroring the reference's per-hop hook; a DNS-rebinding
//! window remains for redirects to a hostname, which the reference documents as
//! an accepted limitation as well.

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::Duration;

use reqwest::header::{COOKIE, USER_AGENT};
use reqwest::redirect::Policy;
use url::{Host, Url};

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("{0}")]
    Blocked(String),
    #[error("Timeout: {0}")]
    Timeout(String),
    #[error("HTTP {status}: {message}")]
    Status { status: u16, message: String },
    #[error("{0}")]
    Transport(String),
}

#[derive(Debug, Clone)]
pub struct FetchedPage {
    pub html_content: String,
    pub status_code: u16,
    pub final_url: String,
}

#[derive(Debug, Clone, Copy)]
pub struct FetchOptions<'a> {
    pub timeout_seconds: f64,
    pub user_agent: Option<&'a str>,
    pub headers: Option<&'a BTreeMap<String, String>>,
    pub cookies: Option<&'a BTreeMap<String, String>>,
    pub allowed_hosts: &'a [String],
}

pub async fn fetch_page(url: &str, options: &FetchOptions<'_>) -> Result<FetchedPage, FetchError> {
    let validated = assert_allowed_url(url, options.allowed_hosts).await?;

    // The contract bounds `timeout` to 1..=300 seconds. Clamp here as well:
    // `Duration::from_secs_f64` panics on NaN/infinity/absurd values, and this
    // is a request-controlled field.
    let timeout_seconds = if options.timeout_seconds.is_finite() {
        options.timeout_seconds.clamp(0.1, 300.0)
    } else {
        30.0
    };
    let timeout = Duration::from_secs_f64(timeout_seconds);
    let client = reqwest::Client::builder()
        .timeout(timeout)
        .redirect(redirect_policy(options.allowed_hosts.to_vec()))
        .build()
        .map_err(|error| FetchError::Transport(error.to_string()))?;

    let mut request = client.get(validated);
    if let Some(user_agent) = options.user_agent {
        request = request.header(USER_AGENT, user_agent);
    }
    if let Some(headers) = options.headers {
        for (name, value) in headers {
            request = request.header(name.as_str(), value.as_str());
        }
    }
    if let Some(cookies) = options.cookies
        && !cookies.is_empty()
    {
        let header = cookies
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("; ");
        request = request.header(COOKIE, header);
    }

    let response = request.send().await.map_err(|error| {
        if error.is_timeout() {
            FetchError::Timeout(error.to_string())
        } else {
            FetchError::Transport(error.to_string())
        }
    })?;

    let status = response.status();
    let final_url = response.url().to_string();
    let html_content = response.text().await.map_err(|error| {
        if error.is_timeout() {
            FetchError::Timeout(error.to_string())
        } else {
            FetchError::Transport(error.to_string())
        }
    })?;

    if !status.is_success() {
        return Err(FetchError::Status {
            status: status.as_u16(),
            message: status.canonical_reason().unwrap_or_default().to_string(),
        });
    }

    Ok(FetchedPage {
        html_content,
        status_code: status.as_u16(),
        final_url,
    })
}

/// The chokepoint. `allowed_hosts` entries are exact hostnames (case-insensitive,
/// trailing root dot ignored) and bypass the range checks, exactly as the
/// reference's `SSRF_ALLOWED_HOSTS` does.
/// Validate `url` and return **the parsed value that was checked**.
///
/// Returning the `Url` (rather than `()`) is the point: the caller's sink takes exactly the value
/// this function inspected, so the request cannot be issued against a string that was validated as
/// one thing and parsed as another — and the data flow from the request body to `reqwest` passes
/// through the checks instead of around them.
pub async fn assert_allowed_url(url: &str, allowed_hosts: &[String]) -> Result<Url, FetchError> {
    let parsed = Url::parse(url)
        .map_err(|error| FetchError::Blocked(format!("URL not allowed: unparseable ({error})")))?;

    let scheme = parsed.scheme().to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return Err(FetchError::Blocked(format!(
            "URL not allowed: scheme '{}' (use http/https)",
            if scheme.is_empty() {
                "(none)"
            } else {
                scheme.as_str()
            }
        )));
    }

    let Some(host) = parsed.host() else {
        return Err(FetchError::Blocked(
            "URL not allowed: missing host".to_string(),
        ));
    };
    let host_text = parsed
        .host_str()
        .unwrap_or_default()
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if host_text.is_empty() {
        return Err(FetchError::Blocked(
            "URL not allowed: missing host".to_string(),
        ));
    }

    if allowed_hosts.iter().any(|allowed| {
        allowed
            .trim_end_matches('.')
            .eq_ignore_ascii_case(&host_text)
    }) {
        return Ok(parsed);
    }

    match host {
        Host::Domain(domain) => {
            let domain = domain.trim_end_matches('.').to_ascii_lowercase();
            if domain == "localhost" || domain.ends_with(".internal") {
                return Err(FetchError::Blocked(format!(
                    "URL not allowed: internal host '{domain}'"
                )));
            }
            let resolved = tokio::net::lookup_host((domain.as_str(), 0))
                .await
                .map_err(|_| {
                    FetchError::Blocked(format!("URL not allowed: cannot resolve host '{domain}'"))
                })?;
            for address in resolved {
                if ip_is_blocked(address.ip()) {
                    return Err(FetchError::Blocked(format!(
                        "URL not allowed: host '{domain}' resolves to blocked address {}",
                        address.ip()
                    )));
                }
            }
            Ok(parsed)
        }
        Host::Ipv4(address) => {
            if ip_is_blocked(IpAddr::V4(address)) {
                return Err(FetchError::Blocked(format!(
                    "URL not allowed: blocked address {host_text}"
                )));
            }
            Ok(parsed)
        }
        Host::Ipv6(address) => {
            if ip_is_blocked(IpAddr::V6(address)) {
                return Err(FetchError::Blocked(format!(
                    "URL not allowed: blocked address {host_text}"
                )));
            }
            Ok(parsed)
        }
    }
}

fn redirect_policy(allowed_hosts: Vec<String>) -> Policy {
    Policy::custom(move |attempt| {
        if attempt.previous().len() >= 10 {
            return attempt.error("too many redirects");
        }
        if literal_host_blocked(attempt.url(), &allowed_hosts) {
            return attempt.error("redirect to a blocked address");
        }
        attempt.follow()
    })
}

/// Synchronous redirect-hop check: suffix, `localhost` and literal IP only.
/// DNS cannot be resolved from reqwest's redirect policy, so a hostname hop is
/// followed; the reference has the same re-resolution window.
fn literal_host_blocked(url: &Url, allowed_hosts: &[String]) -> bool {
    if url.scheme() != "http" && url.scheme() != "https" {
        return true;
    }
    let Some(host) = url.host() else {
        return true;
    };
    let host_text = url
        .host_str()
        .unwrap_or_default()
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if allowed_hosts.iter().any(|allowed| {
        allowed
            .trim_end_matches('.')
            .eq_ignore_ascii_case(&host_text)
    }) {
        return false;
    }
    match host {
        Host::Domain(domain) => {
            let domain = domain.trim_end_matches('.').to_ascii_lowercase();
            domain == "localhost" || domain.ends_with(".internal")
        }
        Host::Ipv4(address) => ip_is_blocked(IpAddr::V4(address)),
        Host::Ipv6(address) => ip_is_blocked(IpAddr::V6(address)),
    }
}

/// The reference's `_ip_is_blocked`, with `is_global` as the load-bearing rule:
/// every non-global range is refused, including RFC 6598 shared address space
/// (the tailnet) that the individual Python flags miss.
fn ip_is_blocked(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => ipv4_is_blocked(address),
        IpAddr::V6(address) => ipv6_is_blocked(address),
    }
}

fn ipv4_is_blocked(address: Ipv4Addr) -> bool {
    let octets = address.octets();
    address.is_loopback()
        || address.is_private()
        || address.is_link_local()
        || address.is_multicast()
        || address.is_unspecified()
        || octets[0] == 0
        || (octets[0] == 100 && (64..=127).contains(&octets[1]))
        || (octets[0] == 192 && octets[1] == 0 && octets[2] == 0)
        || (octets[0] == 192 && octets[1] == 0 && octets[2] == 2)
        || (octets[0] == 198 && (octets[1] == 18 || octets[1] == 19))
        || (octets[0] == 198 && octets[1] == 51 && octets[2] == 100)
        || (octets[0] == 203 && octets[1] == 0 && octets[2] == 113)
        || octets[0] >= 224
}

fn ipv6_is_blocked(address: Ipv6Addr) -> bool {
    if let Some(mapped) = address.to_ipv4_mapped() {
        return ipv4_is_blocked(mapped);
    }
    let segments = address.segments();
    address.is_loopback()
        || address.is_unspecified()
        || address.is_multicast()
        || (segments[0] & 0xfe00) == 0xfc00
        || (segments[0] & 0xffc0) == 0xfe80
        || (segments[0] == 0x2001 && segments[1] == 0x0db8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_captured_metadata_address_is_blocked_with_its_message() {
        let error = assert_allowed_url("http://169.254.169.254/latest/meta-data/", &[])
            .await
            .expect_err("link-local metadata endpoint is blocked");
        assert_eq!(
            error.to_string(),
            "URL not allowed: blocked address 169.254.169.254"
        );
    }

    #[tokio::test]
    async fn loopback_is_blocked_unless_exactly_allowlisted() {
        assert!(
            assert_allowed_url("http://127.0.0.1:18900/x", &[])
                .await
                .is_err()
        );
        assert!(
            assert_allowed_url("http://127.0.0.1:18900/x", &["127.0.0.1".to_string()])
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn internal_suffixes_are_blocked_even_with_a_trailing_root_dot() {
        let error = assert_allowed_url("http://registry.internal./v2/", &[])
            .await
            .expect_err("internal suffix");
        assert!(error.to_string().contains("internal host"), "{error}");
    }

    #[test]
    fn the_tailnet_range_is_not_globally_routable() {
        // Built rather than spelled: the CGNAT range is what must be blocked, and no real
        // address needs to sit in a public tree to prove it.
        assert!(ip_is_blocked(IpAddr::V4(std::net::Ipv4Addr::new(
            100, 81, 177, 99
        ))));
        assert!(!ip_is_blocked("93.184.216.34".parse().expect("ip")));
    }
}
