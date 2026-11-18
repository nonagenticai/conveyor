//! The acting principal, and the one place `Actor` is constructed from a request.
//!
//! Ported from the reference's `src/actor.py` + `src/api/dependencies.py`. Two rules, both from
//! the specification rather than from convenience:
//!
//! * **A decision needs a human.** A licence token proves a tenant; it says nothing about whether
//!   a person or a machine holds it. The reference measured this: a Keycloak client-credentials
//!   token carries a perfectly good `sub`, so "no subject ⇒ machine" refused nothing under real
//!   auth. The rule that holds is *a licence bearer is a machine* unless a verified user token
//!   names the human (`X-Actor-Token`), which is the third rung of the reference's own gate.
//! * **No body field names the reviewer.** The decision body has no reviewer field at all, so
//!   attribution is a property of there being no other constructor.
//!
//! The tenant/actor GUCs are bound per transaction in [`crate::db::begin`], never per session:
//! every connection reaches Postgres through PgBouncer, which reuses server connections across
//! clients.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use uuid::Uuid;

use crate::problem::Problem;
use crate::state::Shared;

/// What the caller may see, not what they may do.
///
/// `Lead` is the only role that reads across reviewers, and it exists because somebody has to
/// adjudicate and export. It is not an admin flag: a lead is still refused as adjudicator on a
/// conflict they took part in (separation of duty is a database trigger).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Reviewer,
    Lead,
}

impl Role {
    /// The value the RLS helpers read from `app.actor_role`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Reviewer => "reviewer",
            Self::Lead => "lead",
        }
    }
}

/// An authenticated principal acting inside one tenant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Actor {
    pub tenant: Uuid,
    /// The attribution everything evidentiary hangs off.
    pub subject: Option<String>,
    pub role: Role,
    /// The credential identified a MACHINE, even when it carries a subject.
    pub is_service_principal: bool,
    /// What to call this principal, when a verified token said. Never an identifier.
    pub display_name: Option<String>,
}

impl Actor {
    /// The licence rung: a proven tenant and a subject that proves nothing about humanity.
    pub fn machine(tenant: Uuid, subject: Option<String>) -> Self {
        Self {
            tenant,
            subject,
            role: Role::Reviewer,
            is_service_principal: true,
            display_name: None,
        }
    }

    /// A verified user token.
    pub fn human(subject: String, role: Role, display_name: Option<String>) -> Self {
        Self {
            tenant: Uuid::nil(), // the licence supplies the tenant; see `with_tenant`
            subject: Some(subject),
            role,
            is_service_principal: false,
            display_name,
        }
    }

    pub fn with_tenant(mut self, tenant: Uuid) -> Self {
        self.tenant = tenant;
        self
    }

    /// A person, not a machine wearing a subject.
    ///
    /// Two conditions because either alone is insufficient: a subject must be present, AND the
    /// credential must not have identified itself as a service account.
    pub fn require_human(&self) -> Result<&str, Problem> {
        let subject = self.subject.as_deref().map(str::trim).unwrap_or_default();
        if subject.is_empty() || self.is_service_principal {
            return Err(Problem::machine_cannot_decide());
        }
        Ok(subject)
    }

    pub fn is_lead(&self) -> bool {
        self.role == Role::Lead
    }

    /// Lead-only routes. The reference grants on the review's creator as well; that is a review
    /// authorisation, evaluated per row, not a role check — see `review_is_visible`.
    pub fn require_lead(&self) -> Result<(), Problem> {
        if self.is_lead() {
            Ok(())
        } else {
            Err(Problem::forbidden(
                "this operation is restricted to a review lead",
            ))
        }
    }

    /// The subject RLS reads for the blinded decision policy. A machine has none.
    pub fn actor_subject(&self) -> &str {
        self.subject.as_deref().unwrap_or_default()
    }
}

impl FromRequestParts<Shared> for Actor {
    type Rejection = Problem;

    async fn from_request_parts(parts: &mut Parts, _state: &Shared) -> Result<Self, Problem> {
        parts.extensions.get::<Actor>().cloned().ok_or_else(|| {
            Problem::new(
                axum::http::StatusCode::UNAUTHORIZED,
                "authentication_required",
                "Authentication Required",
                "the tenant gate did not resolve an actor for this request",
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tenant() -> Uuid {
        Uuid::parse_str("11111111-1111-1111-1111-111111111111").expect("uuid")
    }

    #[test]
    fn a_licence_subject_without_a_user_token_is_a_machine() {
        let actor = Actor::machine(tenant(), Some("subject-a".to_owned()));
        let refusal = actor.require_human().expect_err("must refuse");
        assert_eq!(refusal.code, "machine_cannot_decide");
    }

    #[test]
    fn a_machine_with_no_subject_is_refused_too() {
        let actor = Actor::machine(tenant(), None);
        assert!(actor.require_human().is_err());
    }

    #[test]
    fn a_verified_human_can_decide() {
        let actor =
            Actor::human("subject-a".to_owned(), Role::Reviewer, None).with_tenant(tenant());
        assert_eq!(actor.require_human().expect("human"), "subject-a");
        assert!(!actor.is_lead());
        assert_eq!(actor.actor_subject(), "subject-a");
    }

    #[test]
    fn the_lead_role_is_what_lead_routes_check() {
        let reviewer = Actor::human("a".to_owned(), Role::Reviewer, None).with_tenant(tenant());
        assert!(reviewer.require_lead().is_err());
        let lead = Actor::human("b".to_owned(), Role::Lead, None).with_tenant(tenant());
        assert!(lead.require_lead().is_ok());
    }
}
