//! The development principal (`CONVEYOR_DEV_AUTH=1`).
//!
//! Two transports, one resolution:
//! - `Authorization: Bearer dev:<login>` — the CLI/gate transport.
//! - the `conveyor_dev=<login>` cookie — the browser transport (CONTRACT §10: a browser
//!   cannot attach an Authorization header to a navigation, so the app accepts a cookie
//!   with the SAME resolution and the SAME refusals).
//!
//! The subject is stored literally as `dev:<login>` in `app_user.oidc_subject`, so
//! `dev:alice` IS the `dev-a` principal seeded by `migrate --dev-seed`. There is no
//! login column and no header-supplied tenant: the tenant comes from `membership`.

use axum::http::{HeaderMap, header};

/// The dev cookie name.
pub const DEV_COOKIE: &str = "conveyor_dev";

/// The subject prefix a dev login is stored under.
pub const DEV_SUBJECT_PREFIX: &str = "dev:";

/// A login is 1–128 chars of `[A-Za-z0-9._-]`. Anything else cannot be a seeded subject and
/// is refused rather than normalised.
pub fn valid_login(login: &str) -> bool {
    !login.is_empty()
        && login.len() <= 128
        && login
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// The dev subject for this request, if it presents a valid dev credential.
///
/// A present `Authorization` header is authoritative: if it is not a valid
/// `Bearer dev:<login>`, the cookie is NOT consulted. Falling through would let a stray
/// header silently authenticate a browser session.
pub fn subject_from(headers: &HeaderMap) -> Option<String> {
    let mut values = headers.get_all(header::AUTHORIZATION).iter();
    if let Some(value) = values.next() {
        if values.next().is_some() {
            return None;
        }
        let raw = value.to_str().ok()?;
        let (scheme, token) = raw.split_once(' ')?;
        if !scheme.eq_ignore_ascii_case("bearer") {
            return None;
        }
        let login = token.trim().strip_prefix(DEV_SUBJECT_PREFIX)?;
        return valid_login(login).then(|| format!("{DEV_SUBJECT_PREFIX}{login}"));
    }

    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    for part in raw.split(';') {
        let part = part.trim();
        if let Some(login) = part
            .strip_prefix(DEV_COOKIE)
            .and_then(|p| p.strip_prefix('='))
        {
            return valid_login(login).then(|| format!("{DEV_SUBJECT_PREFIX}{login}"));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.append(*k, HeaderValue::from_str(v).unwrap());
        }
        h
    }

    #[test]
    fn bearer_dev_token_resolves_to_the_stored_subject() {
        assert_eq!(
            subject_from(&headers(&[("authorization", "Bearer dev:alice")])).as_deref(),
            Some("dev:alice")
        );
        assert_eq!(
            subject_from(&headers(&[("authorization", "bearer dev:bob")])).as_deref(),
            Some("dev:bob")
        );
    }

    #[test]
    fn the_cookie_is_the_browser_transport() {
        assert_eq!(
            subject_from(&headers(&[(
                "cookie",
                "theme=dark; conveyor_dev=alice; x=1"
            )]))
            .as_deref(),
            Some("dev:alice")
        );
    }

    #[test]
    fn a_present_authorization_header_is_authoritative_over_the_cookie() {
        assert_eq!(
            subject_from(&headers(&[
                ("authorization", "Bearer not-a-dev-token"),
                ("cookie", "conveyor_dev=alice"),
            ])),
            None,
            "a stray header must not silently fall through to a browser session"
        );
        assert_eq!(
            subject_from(&headers(&[
                ("authorization", "Bearer devnope"),
                ("cookie", "conveyor_dev=alice"),
            ])),
            None
        );
    }

    #[test]
    fn malformed_and_duplicate_credentials_are_refused() {
        for pairs in [
            vec![],
            vec![("authorization", "Basic dXNlcjpwYXNz")],
            vec![("authorization", "Bearer ")],
            vec![("authorization", "Bearer dev:")],
            vec![("authorization", "Bearer dev:al ice")],
            vec![
                ("authorization", "Bearer dev:alice"),
                ("authorization", "Bearer dev:bob"),
            ],
            vec![("cookie", "conveyor_dev=")],
            vec![("cookie", "conveyor_devx=alice")],
            vec![("cookie", "conveyor_dev=alice/../bob")],
        ] {
            assert_eq!(subject_from(&headers(&pairs)), None, "{pairs:?}");
        }
    }

    #[test]
    fn login_alphabet_is_bounded() {
        assert!(valid_login("alice"));
        assert!(valid_login("dev.user-1_x"));
        assert!(!valid_login(""));
        assert!(!valid_login(&"a".repeat(129)));
        assert!(!valid_login("alice bob"));
        assert!(!valid_login("alice\n"));
    }
}
