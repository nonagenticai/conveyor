//! Realm `conveyor` user administration: find-or-create the first tenant admin, and send
//! the ONE set-password email.
//!
//! `/api/provision` refuses without a `keycloak_user_id` and creates no Keycloak user — the
//! caller owns the identity in its own realm. This module is that owner: it mints the user
//! in realm `conveyor` as the `conveyor-backend` service account, which holds EXACTLY
//! realm-management `{manage-users, view-users, query-users, view-realm}`.
//!
//! The sequence is fr's, proven over years:
//!
//! 1. **find by email** (`?email=&exact=true`) — a retry must not create a second user;
//! 2. **create** `enabled: true`, `emailVerified: true` (the address is the identity the
//!    super-admin typed; there is no self-signup to verify against) with `signup_app=conveyor`;
//! 3. **send `UPDATE_PASSWORD`** once (`execute-actions-email`, 12 h lifespan).
//!
//! `emailVerified: true` is deliberate: it is what lets an invited person's first sign-in
//! bind to the `app_user` row by verified email (CONTRACT §9, email-keyed identity).

use conveyor_domain::Uuid;
use serde_json::{Value, json};

use crate::oauth::ClientCredentials;
use crate::service::{ServiceError, ServiceHttp};

/// The execute-actions-email link lifetime (12 hours, the estate default).
pub const EMAIL_LIFESPAN_SECS: u32 = 43_200;

/// A user in realm `conveyor`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeycloakUser {
    /// The Keycloak user id — the licence subject and the id `/api/provision` needs.
    pub id: Uuid,
    /// The login name (the email).
    pub username: String,
    /// True when this call created the user.
    pub created: bool,
}

/// The admin client for one realm.
#[derive(Clone)]
pub struct KeycloakAdmin {
    http: ServiceHttp,
    realm: String,
}

impl std::fmt::Debug for KeycloakAdmin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeycloakAdmin")
            .field("http", &self.http)
            .field("realm", &self.realm)
            .finish()
    }
}

impl KeycloakAdmin {
    /// Build for `https://auth.nonagentic.ai` + realm `conveyor`, as `conveyor-backend`.
    pub fn new(
        base_url: Option<&str>,
        realm: &str,
        creds: Option<ClientCredentials>,
    ) -> Result<Self, ServiceError> {
        let realm = realm.trim();
        if realm.is_empty() {
            return Err(ServiceError::Unconfigured("KEYCLOAK_ADMIN_REALM"));
        }
        if creds.is_none() {
            return Err(ServiceError::Unconfigured("KEYCLOAK_ADMIN_CLIENT_SECRET"));
        }
        let http = ServiceHttp::new("keycloak", base_url, creds)?;
        Ok(Self {
            http,
            realm: realm.to_owned(),
        })
    }

    /// Find the user by email, or create it. Idempotent: a second call with the same email
    /// returns the same id and `created: false`.
    pub async fn ensure_user(
        &self,
        email: &str,
        display_name: &str,
    ) -> Result<KeycloakUser, ServiceError> {
        let email = email.trim();
        if let Some(user) = self.find_by_email(email).await? {
            return Ok(user);
        }
        let (first, last) = split_name(display_name, email);
        let body = json!({
            "username": email,
            "email": email,
            "emailVerified": true,
            "firstName": first,
            "lastName": last,
            "enabled": true,
            "attributes": {"signup_app": ["conveyor"]},
        });
        match self
            .http
            .post(&format!("/admin/realms/{}/users", self.realm), &body)
            .await
        {
            Ok(_) => {}
            // Keycloak answers 409 when the username exists; the lookup below resolves it,
            // which covers a concurrent creator and a case-only difference.
            Err(ServiceError::Refused { status: 409, .. }) => {}
            Err(error) => return Err(error),
        }
        match self.find_by_email(email).await? {
            Some(mut user) => {
                user.created = true;
                Ok(user)
            }
            None => Err(ServiceError::Body {
                service: "keycloak",
                error: format!("user {email:?} not found after create"),
            }),
        }
    }

    /// Send ONE `UPDATE_PASSWORD` email. Keycloak's own mailer; the conveyor has none.
    pub async fn send_update_password_email(&self, user_id: Uuid) -> Result<(), ServiceError> {
        let path = format!(
            "/admin/realms/{}/users/{}/execute-actions-email?lifespan={}",
            self.realm, user_id, EMAIL_LIFESPAN_SECS
        );
        self.http.put(&path, &json!(["UPDATE_PASSWORD"])).await?;
        Ok(())
    }

    async fn find_by_email(&self, email: &str) -> Result<Option<KeycloakUser>, ServiceError> {
        let path = format!(
            "/admin/realms/{}/users?email={}&exact=true",
            self.realm,
            encode_query(email)
        );
        let value = self.http.get(&path, None).await?;
        let users = value.as_array().cloned().unwrap_or_default();
        let wanted = email.trim().to_ascii_lowercase();
        let mut matches: Vec<KeycloakUser> = users
            .iter()
            .filter(|user| {
                user.get("email")
                    .and_then(Value::as_str)
                    .map(|candidate| candidate.trim().to_ascii_lowercase() == wanted)
                    .unwrap_or(false)
            })
            .filter_map(|user| {
                let id = user.get("id")?.as_str()?;
                Some(KeycloakUser {
                    id: Uuid::parse_str(id).ok()?,
                    username: user
                        .get("username")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                    created: false,
                })
            })
            .collect();
        match matches.len() {
            0 => Ok(None),
            1 => Ok(matches.pop()),
            // Two users on one address is a state a human must resolve; guessing would
            // attach the tenant to the wrong identity.
            _ => Err(ServiceError::Body {
                service: "keycloak",
                error: format!("{} users hold the email {email:?}", matches.len()),
            }),
        }
    }
}

/// fr's name rule: one word is BOTH names; empty uses the email local part for both.
/// (Applies to this realm's user profile, which asks for first/last.)
pub fn split_name(display_name: &str, email: &str) -> (String, String) {
    let trimmed = display_name.trim();
    if trimmed.is_empty() {
        let local = email.split('@').next().unwrap_or(email).trim();
        let local = if local.is_empty() { "User" } else { local };
        return (local.to_owned(), local.to_owned());
    }
    match trimmed.split_once(char::is_whitespace) {
        Some((first, rest)) if !rest.trim().is_empty() => {
            (first.trim().to_owned(), rest.trim().to_owned())
        }
        _ => (trimmed.to_owned(), trimmed.to_owned()),
    }
}

/// `application/x-www-form-urlencoded`-safe query value (RFC 3986 unreserved only).
pub fn encode_query(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_name_rule_matches_fr() {
        assert_eq!(
            split_name("Alice Example", "a@b.co"),
            ("Alice".into(), "Example".into())
        );
        assert_eq!(split_name("Cher", "c@b.co"), ("Cher".into(), "Cher".into()));
        assert_eq!(
            split_name("  ", "alice@example.com"),
            ("alice".into(), "alice".into())
        );
        assert_eq!(
            split_name("Alice  van der Berg", "a@b.co"),
            ("Alice".into(), "van der Berg".into())
        );
    }

    #[test]
    fn the_query_encoder_escapes_at_and_plus() {
        assert_eq!(encode_query("a+b@c.d"), "a%2Bb%40c.d");
        assert_eq!(encode_query("plain-email_1.z~"), "plain-email_1.z~");
    }
}
