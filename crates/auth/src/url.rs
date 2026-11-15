//! The one rule every configured service address must satisfy.
//!
//! These clients attach a credential to their calls — a bearer minted from a client
//! secret, a Telegram bot token that is itself a path segment — so a cleartext endpoint
//! would put that credential on the wire for anyone on the path to read. Every client
//! therefore refuses a base URL that is not `https` at construction (or, where the base
//! comes from a binding at request time, before the request is built).
//!
//! The single exception is a literal loopback address: traffic to it never leaves the
//! host, so there is no network to observe. That is what lets the integration suites
//! point a stub at `http://127.0.0.1:<ephemeral>` without a deployed client ever
//! accepting a cleartext service.

use reqwest::Url;

/// True when `value` is an `https` URL, or an `http` URL whose host is a literal
/// loopback address (`127.0.0.0/8`, `::1`).
///
/// A hostname that merely resolves to loopback (`localhost`) is deliberately NOT trusted:
/// its meaning depends on resolution, which this check cannot verify.
pub fn is_secure_service_url(value: &str) -> bool {
    let Ok(url) = Url::parse(value) else {
        return false;
    };
    match url.scheme() {
        "https" => true,
        "http" => url.host_str().is_some_and(is_loopback_host),
        _ => false,
    }
}

fn is_loopback_host(host: &str) -> bool {
    // `Url::host_str` keeps an IPv6 literal's brackets.
    let bare = host
        .strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
        .unwrap_or(host);
    bare.parse::<std::net::IpAddr>()
        .is_ok_and(|address| address.is_loopback())
}

#[cfg(test)]
mod tests {
    use super::is_secure_service_url;

    #[test]
    fn https_is_secure() {
        assert!(is_secure_service_url("https://billing.example.com:8157"));
        assert!(is_secure_service_url("https://127.0.0.1:8157"));
    }

    #[test]
    fn cleartext_is_allowed_only_to_a_literal_loopback_host() {
        assert!(is_secure_service_url("http://127.0.0.1:8157"));
        assert!(is_secure_service_url("http://127.4.3.2:8157"));
        assert!(is_secure_service_url("http://[::1]:8157"));
        assert!(!is_secure_service_url("http://billing.example.com:8157"));
        assert!(!is_secure_service_url("http://localhost:8157"));
        assert!(!is_secure_service_url("http://[2001:db8::1]:8157"));
    }

    #[test]
    fn other_shapes_are_not_secure() {
        assert!(!is_secure_service_url("billing.example.com:8157"));
        assert!(!is_secure_service_url("ftp://billing.example.com"));
        assert!(!is_secure_service_url(""));
    }
}
