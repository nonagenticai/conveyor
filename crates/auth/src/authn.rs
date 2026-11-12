//! Turning a request's credential into a [`Session`].
//!
//! Two modes, never mixed:
//! - `CONVEYOR_DEV_AUTH=1`: a `dev:<login>` bearer (or the `conveyor_dev` cookie). A JWT
//!   presented in this mode is not a dev credential and is a hard 401 — it is never
//!   retried as anything else.
//! - otherwise: `Authorization: Bearer <token>`, and the token's **shape** picks the verifier.
//!   A JWT-shaped token goes to [`Verifier`] and a failure there is final. Anything else is a
//!   licence bearer and goes to [`MachineAuthenticator`] (CONTRACT §9). The shape test is what
//!   stops a revoked licence being retried as a JWT, and a broken JWT as a licence.
//!
//! In every mode the verified subject is resolved through `membership` to one `(app_user, tenant)`
//! pair for **this request**. A person may hold several memberships (one row per tenant); the
//! request selects one with `X-Active-Tenant`, matched only against the caller's own rows, and a
//! caller holding more than one that names none is refused — the conveyor never guesses a tenant,
//! because an audit row attributed to a guessed tenant is not a proof. A licence bearer carries
//! its tenant — the tenant comes from the credential, never from a header — and must resolve to a
//! **machine** principal: a credential that lands on a `human` row is refused rather than believed.

use axum::http::{HeaderMap, header};
use conveyor_domain::Id;
use conveyor_store::{MembershipRow, TenantId};
use sqlx::PgPool;

use crate::config::AuthConfig;
use crate::dev;
use crate::error::AuthError;
use crate::jwks::Verifier;
use crate::machine::{self, MachineAuthenticator};
use crate::principal::{
    ChannelAuthority, Membership, Principal, PrincipalKind, Role, ScopePolicy, Session,
};

/// What the credential proves before any database is touched.
struct Credential {
    subject: String,
    /// A licence bearer's tenant: the credential's, never a header.
    licence_tenant: Option<TenantId>,
    /// The verified email, only when the issuer said `email_verified: true`.
    email: Option<String>,
    display_name: Option<String>,
}

/// The `Authorization: Bearer` token, if the header is well formed and single-valued.
pub fn bearer(headers: &HeaderMap) -> Option<&str> {
    let mut values = headers.get_all(header::AUTHORIZATION).iter();
    let value = values.next()?;
    if values.next().is_some() {
        return None;
    }
    let raw = value.to_str().ok()?;
    let (scheme, token) = raw.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let token = token.trim();
    (!token.is_empty()).then_some(token)
}

/// Authenticate a request and resolve the tenant it acts in.
///
/// `machine` is `None` when no licences service is configured; a licence bearer is then
/// simply unauthenticated rather than silently admitted.
pub async fn authenticate(
    cfg: &AuthConfig,
    verifier: Option<&Verifier>,
    machine: Option<&MachineAuthenticator>,
    pool: &PgPool,
    headers: &HeaderMap,
) -> Result<Session, AuthError> {
    let Identified {
        identity,
        kind,
        licence_tenant,
    } = identify(cfg, verifier, machine, pool, headers).await?;

    let requested = active_tenant_header(headers)?;
    let row = select_membership(&identity.memberships, licence_tenant, requested)?;
    let role = Role::parse(&row.role).ok_or_else(|| AuthError::UnknownRole(row.role.clone()))?;
    if row.tenant_status != "active" {
        return Err(AuthError::TenantNotActive {
            status: row.tenant_status.clone(),
        });
    }

    // Both reads are once per request and need no invalidation: a policy change or a grant
    // applies to the next request.
    let policy = conveyor_store::rbac::policy_for(pool, row.tenant_id, role.as_str())
        .await
        .map_err(AuthError::Store)?;
    let is_super_admin = conveyor_store::rbac::is_platform_admin(pool, identity.user_id.as_uuid())
        .await
        .map_err(AuthError::Store)?;
    // A machine credential is a connector, not a role holder (S06d). Its invitation to act is
    // the tenant's grant set for the `mcp` kind; the `kata_executor` membership remains only
    // so intra-action checks see an admin-equivalent caller. A human session has no channel.
    let channel = if kind.is_machine() {
        let overrides = conveyor_store::rbac::channel_policy_for(
            pool,
            row.tenant_id,
            conveyor_domain::channel::MCP,
        )
        .await
        .map_err(AuthError::Store)?;
        Some(ChannelAuthority::new(
            conveyor_domain::channel::MCP,
            None,
            overrides,
        ))
    } else {
        None
    };

    let membership = Membership {
        tenant_id: row.tenant_id,
        slug: row.tenant_slug.clone(),
        name: row.tenant_name.clone(),
        role,
    };
    // Every membership this principal may act in. Only `active` tenants are offered, and a row
    // whose role this build does not know is left out rather than shown under another row's role;
    // the *selected* row was parsed strictly above.
    let memberships: Vec<Membership> = identity
        .memberships
        .iter()
        .filter(|row| row.tenant_status == "active")
        .filter_map(|row| {
            Some(Membership {
                tenant_id: row.tenant_id,
                slug: row.tenant_slug.clone(),
                name: row.tenant_name.clone(),
                role: Role::parse(&row.role)?,
            })
        })
        .collect();
    Ok(Session {
        principal: Principal {
            user_id: identity.user_id,
            subject: identity.subject,
            display_name: identity.display_name,
            email: identity.email,
            kind,
        },
        tenant: membership.tenant_id,
        tenant_slug: membership.slug.clone(),
        tenant_name: membership.name.clone(),
        role,
        memberships,
        is_super_admin,
        policy: ScopePolicy::new(policy),
        channel,
    })
}

/// What the credential proves after the person is resolved, before a tenant is chosen.
struct Identified {
    identity: conveyor_store::identity::Identity,
    kind: PrincipalKind,
    licence_tenant: Option<TenantId>,
}

/// Credential → verified subject → `app_user` row. Chooses no tenant.
async fn identify(
    cfg: &AuthConfig,
    verifier: Option<&Verifier>,
    machine: Option<&MachineAuthenticator>,
    pool: &PgPool,
    headers: &HeaderMap,
) -> Result<Identified, AuthError> {
    let credential = if cfg.dev_auth {
        Credential {
            subject: dev::subject_from(headers).ok_or(AuthError::Unauthenticated)?,
            licence_tenant: None,
            // A dev principal is seeded, never bound by email.
            email: None,
            display_name: None,
        }
    } else {
        let token = bearer(headers).ok_or(AuthError::Unauthenticated)?;
        if machine::is_jwt_shaped(token) {
            // A JWT that fails here is final: it is never retried as a licence token.
            let verifier = verifier.ok_or(AuthError::Unauthenticated)?;
            let verified = verifier.verify(token).await?;
            // The email half of identity is admitted ONLY on `email_verified: true`; an absent
            // or false claim can neither bind an existing row nor redeem an invite.
            let email = (verified.email_verified == Some(true))
                .then_some(verified.email)
                .flatten();
            Credential {
                subject: verified.sub,
                licence_tenant: None,
                email,
                display_name: verified.preferred_username,
            }
        } else {
            let machine = machine.ok_or(AuthError::Unauthenticated)?;
            let identity = machine.identify(token).await?;
            Credential {
                subject: identity.subject,
                licence_tenant: Some(identity.tenant_id),
                email: None,
                display_name: None,
            }
        }
    };

    let identity = resolve_identity(pool, &credential).await?;
    let kind = PrincipalKind::parse(&identity.kind)
        .ok_or_else(|| AuthError::UnknownPrincipalKind(identity.kind.clone()))?;
    // A licence bearer proves a machine. If it resolves to a person's row, the provisioned
    // identity and the credential disagree — refuse rather than act as that person.
    if credential.licence_tenant.is_some() && !kind.is_machine() {
        return Err(AuthError::InvalidToken);
    }
    Ok(Identified {
        identity,
        kind,
        licence_tenant: credential.licence_tenant,
    })
}

/// The principal and every membership, WITHOUT selecting a tenant.
///
/// `/api/me` is the one caller: the shell must render the tenant switcher before a tenant has
/// been chosen. A caller holding several memberships and naming none therefore gets a 200 here,
/// while every other endpoint still refuses with `active_tenant_required`.
#[derive(Clone, Debug)]
pub struct IdentitySession {
    /// The authenticated principal.
    pub principal: Principal,
    /// Every active membership, ordered by tenant slug.
    pub memberships: Vec<Membership>,
    /// `app_user.id` holds a `platform_admin` row.
    pub is_super_admin: bool,
}

impl From<&Session> for IdentitySession {
    /// The same principal and memberships a fully selected session carries, with no tenant
    /// choice made — `/api/me` uses this when a tenant WAS selected, so the answer keeps its
    /// `active_tenant`.
    fn from(session: &Session) -> Self {
        Self {
            principal: session.principal.clone(),
            memberships: session.memberships.clone(),
            is_super_admin: session.is_super_admin,
        }
    }
}

/// Resolve the caller without choosing a tenant.
pub async fn identity_session(
    cfg: &AuthConfig,
    verifier: Option<&Verifier>,
    machine: Option<&MachineAuthenticator>,
    pool: &PgPool,
    headers: &HeaderMap,
) -> Result<IdentitySession, AuthError> {
    let Identified { identity, kind, .. } = identify(cfg, verifier, machine, pool, headers).await?;
    let is_super_admin = conveyor_store::rbac::is_platform_admin(pool, identity.user_id.as_uuid())
        .await
        .map_err(AuthError::Store)?;
    let memberships = active_memberships(&identity);
    Ok(IdentitySession {
        principal: Principal {
            user_id: identity.user_id,
            subject: identity.subject,
            display_name: identity.display_name,
            email: identity.email,
            kind,
        },
        memberships,
        is_super_admin,
    })
}

/// Every membership the principal may act in: active tenants only, known roles only.
fn active_memberships(identity: &conveyor_store::identity::Identity) -> Vec<Membership> {
    identity
        .memberships
        .iter()
        .filter(|row| row.tenant_status == "active")
        .filter_map(|row| {
            Some(Membership {
                tenant_id: row.tenant_id,
                slug: row.tenant_slug.clone(),
                name: row.tenant_name.clone(),
                role: Role::parse(&row.role)?,
            })
        })
        .collect()
}

/// Resolve the credential to an `app_user` row, in the order the owner rulings fix.
///
/// 1. **By subject.** The ordinary path: the account that signed in is the row we hold.
/// 2. **By verified email, then rebind.** A Keycloak account may be deleted and recreated; the
///    new `sub` must land on the same row or the person's memberships and history fork. The
///    rebind is recorded on the row (`version + 1`) and logged by user id — never by email.
/// 3. **By pending invite.** No row, but an admin invited this address: create the row and
///    accept every invite naming it. This is the only sign-up path.
///
/// Anything else is unauthenticated. A dev principal has no email and therefore can never take
/// arms 2 or 3 — it must have been seeded.
async fn resolve_identity(
    pool: &PgPool,
    credential: &Credential,
) -> Result<conveyor_store::identity::Identity, AuthError> {
    if let Some(identity) = conveyor_store::resolve_identity(pool, &credential.subject)
        .await
        .map_err(AuthError::Store)?
    {
        return Ok(identity);
    }
    let Some(email) = credential.email.as_deref() else {
        return Err(AuthError::Unauthenticated);
    };

    if let Some(mut identity) = conveyor_store::identity::resolve_identity_by_email(pool, email)
        .await
        .map_err(AuthError::Store)?
    {
        let changed = conveyor_store::identity::rebind_subject(
            pool,
            identity.user_id.as_uuid(),
            &credential.subject,
        )
        .await
        .map_err(AuthError::Store)?;
        tracing::info!(
            user_id = %identity.user_id,
            changed = changed > 0,
            "login rebound an existing user to a new OIDC subject (verified email)"
        );
        identity.subject = credential.subject.clone();
        return Ok(identity);
    }

    let pending = conveyor_store::invite::pending_for_email(pool, email)
        .await
        .map_err(AuthError::Store)?;
    if pending.is_empty() {
        // No row, no invite: refusing here is what makes an uninvited sign-in impossible.
        return Err(AuthError::Unauthenticated);
    }
    let display_name = credential
        .display_name
        .clone()
        .unwrap_or_else(|| email.to_owned());
    let user_id: Id = conveyor_store::identity::create_invited_user(
        pool,
        &credential.subject,
        email,
        &display_name,
    )
    .await
    .map_err(AuthError::Store)?;
    conveyor_store::invite::accept_all_for(pool, user_id.as_uuid(), email)
        .await
        .map_err(AuthError::Store)?;
    conveyor_store::resolve_identity(pool, &credential.subject)
        .await
        .map_err(AuthError::Store)?
        .ok_or(AuthError::Unauthenticated)
}

/// The header that selects one of the caller's memberships (the per-tenant role switch).
pub const ACTIVE_TENANT_HEADER: &str = "x-active-tenant";

/// The value of `X-Active-Tenant`, when the request carries exactly one non-empty value.
///
/// The header is a selector, not a preference list: two values are malformed, not "the first
/// one wins", because a request must not be able to mean two tenants at once.
fn active_tenant_header(headers: &HeaderMap) -> Result<Option<&str>, AuthError> {
    let mut values = headers.get_all(ACTIVE_TENANT_HEADER).iter();
    let Some(value) = values.next() else {
        return Ok(None);
    };
    if values.next().is_some() {
        return Err(AuthError::ActiveTenantInvalid(
            "the header appears more than once",
        ));
    }
    let raw = value
        .to_str()
        .map_err(|_| AuthError::ActiveTenantInvalid("the header is not ASCII"))?
        .trim();
    if raw.is_empty() {
        return Err(AuthError::ActiveTenantInvalid("the header is empty"));
    }
    Ok(Some(raw))
}

/// Resolve the one membership this request acts in.
///
/// - a licence bearer's tenant is the credential's: a header may repeat it, never contradict it;
/// - otherwise the header selects among the caller's **own** rows, and a missing header is only
///   unambiguous when the caller holds exactly one — a person with several is refused rather than
///   guessed at.
fn select_membership<'a>(
    memberships: &'a [MembershipRow],
    licence_tenant: Option<TenantId>,
    requested: Option<&str>,
) -> Result<&'a MembershipRow, AuthError> {
    match licence_tenant {
        Some(tenant) => {
            // The credential's tenant must be one of the caller's rows before a header is judged:
            // a bearer with no membership at all is "not provisioned", not "forbidden".
            let row = memberships
                .iter()
                .find(|row| row.tenant_id == tenant)
                .ok_or(AuthError::NotProvisioned {
                    count: memberships.len(),
                })?;
            if let Some(requested) = requested
                && find_membership(memberships, requested).map(|row| row.tenant_id) != Some(tenant)
            {
                return Err(AuthError::ActiveTenantNotPermitted);
            }
            Ok(row)
        }
        None => match requested {
            Some(requested) => {
                find_membership(memberships, requested).ok_or(AuthError::ActiveTenantNotPermitted)
            }
            None => match memberships {
                [] => Err(AuthError::NotProvisioned { count: 0 }),
                [one] => Ok(one),
                _ => Err(AuthError::ActiveTenantRequired {
                    count: memberships.len(),
                }),
            },
        },
    }
}

/// Match the header against the caller's own rows: exact tenant id first, then exact slug.
///
/// Only the caller's rows are searched, so a tenant this principal does not belong to is
/// indistinguishable from one that does not exist.
fn find_membership<'a>(memberships: &'a [MembershipRow], raw: &str) -> Option<&'a MembershipRow> {
    memberships
        .iter()
        .find(|row| row.tenant_id.as_uuid().to_string() == raw)
        .or_else(|| memberships.iter().find(|row| row.tenant_slug == raw))
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
    fn bearer_is_single_valued_and_case_insensitive() {
        assert_eq!(
            bearer(&headers(&[("authorization", "Bearer abc.def.ghi")])),
            Some("abc.def.ghi")
        );
        assert_eq!(
            bearer(&headers(&[("authorization", "bEaReR  spaced ")])),
            Some("spaced")
        );
        assert_eq!(bearer(&headers(&[])), None);
        assert_eq!(bearer(&headers(&[("authorization", "Basic abc")])), None);
        assert_eq!(bearer(&headers(&[("authorization", "Bearer    ")])), None);
        assert_eq!(
            bearer(&headers(&[
                ("authorization", "Bearer a"),
                ("authorization", "Bearer b"),
            ])),
            None
        );
    }
}
