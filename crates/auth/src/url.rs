//! The one rule every configured service address must satisfy.
//!
//! These clients attach a credential to their calls — a bearer minted from a client
//! secret, a Telegram bot token that is itself a path segment — so a cleartext endpoint
//! would put that credential on the wire for anyone on the path to read. Every client
//! therefore refuses a base URL that is not `https` at construction (or, where the base
//! comes from a binding at request time, before the request is built).
//!
//! Two exceptions are cleartext-and-safe, and only these two:
//!
//! 1. A literal loopback address: traffic to it never leaves the host, so there is no
//!    network to observe. That is what lets the integration suites point a stub at
//!    `http://127.0.0.1:<ephemeral>`.
//! 2. An in-cluster service DNS name (`*.svc`, `*.svc.cluster.local`): the sidecars hold
//!    a STRICT istio `PeerAuthentication` in every namespace these clients call
//!    (`conveyor`, `licenses`, `billing`), so the hop is mTLS-encrypted at the proxy even
//!    though the application speaks `http`. Requiring `https` here would add no
//!    confidentiality and would break the live deployment, whose service URLs are
//!    `http://<svc>.<ns>.svc.cluster.local` (see the `licenses` and `license-authz`
//!    deployments). If a namespace's mTLS ever drops to PERMISSIVE, this exception must
//!    be revisited — the safety here is the mesh's, not this function's.

use reqwest::Url;

/// True when `value` is an `https` URL, or an `http` URL whose host is a literal
/// loopback address (`127.0.0.0/8`, `::1`) or an in-cluster service DNS name.
///
/// A hostname that merely resolves to loopback (`localhost`) is deliberately NOT trusted:
/// its meaning depends on resolution, which this check cannot verify.
pub fn is_secure_service_url(value: &str) -> bool {
    let Ok(url) = Url::parse(value) else {
        return false;
    };
    match url.scheme() {
        "https" => true,
        "http" => url
            .host_str()
            .is_some_and(|host| is_loopback_host(host) || is_in_cluster_host(host)),
        _ => false,
    }
}

/// True for `name.namespace.svc` and `name.namespace.svc.cluster.local`, and for the
/// cluster search-domain spellings that end in `.svc.` plus anything the cluster appends.
///
/// Suffix matching is on whole labels — the leading dot is part of the suffix — so
/// `evil-svc.cluster.local` (no `.svc` label) does not match, and neither does a host
/// that merely contains `svc` in a label.
fn is_in_cluster_host(host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    host.ends_with(".svc") || host.ends_with(".svc.cluster.local")
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

    /// The live deployments call `http://<svc>.<ns>.svc.cluster.local`; the hop is
    /// mTLS-encrypted by the istio sidecars (STRICT PeerAuthentication), so these must
    /// keep being accepted or the service stops calling itself.
    #[test]
    fn cleartext_is_allowed_to_an_in_cluster_service_host() {
        assert!(is_secure_service_url(
            "http://billing-service.billing.svc.cluster.local:8157"
        ));
        assert!(is_secure_service_url(
            "http://licenses.licenses.svc.cluster.local:8544"
        ));
        assert!(is_secure_service_url(
            "http://provisioning.provisioning.svc.cluster.local:8546"
        ));
        assert!(is_secure_service_url("http://billing-service.billing.svc"));
        assert!(is_secure_service_url(
            "http://BILLING.billing.svc.cluster.local"
        ));
    }

    /// The in-cluster exception is label-anchored: a host that merely *contains* the
    /// letters `svc`, or an attacker domain that leaves the `.svc` label out, stays
    /// rejected. This is the same class of bug the `*-sanitization` alerts are about.
    #[test]
    fn in_cluster_exception_does_not_leak_to_lookalike_hosts() {
        assert!(!is_secure_service_url("http://evil-svc.cluster.local"));
        assert!(!is_secure_service_url("http://billing.svc.evil.com"));
        assert!(!is_secure_service_url("http://svc.billing.example.com"));
        assert!(!is_secure_service_url("http://notsvc:8157"));
        assert!(!is_secure_service_url(
            "http://billing.billing.svc.cluster.local.evil.com"
        ));
    }

    #[test]
    fn other_shapes_are_not_secure() {
        assert!(!is_secure_service_url("billing.example.com:8157"));
        assert!(!is_secure_service_url("ftp://billing.example.com"));
        assert!(!is_secure_service_url(""));
    }
}
