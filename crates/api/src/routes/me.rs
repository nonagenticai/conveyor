//! `GET /api/me` — the read model declared for S12's `shell/login` (CONTRACT §11): the
//! principal, every membership, and (when one was selected) the active tenant.
//!
//! It takes [`CallerIdentity`], which chooses NO tenant: a caller holding several memberships
//! and naming none gets a 200 with the list, because this answer is what draws the switcher.
//!
//! It is outside the licence gate by construction, not by an exemption: `RequireLicence` is
//! an extractor, so only a route that asks for it evaluates a licence. This handler takes no
//! licence, and there is no exempt-list anywhere in the API. That is deliberate — a blocked
//! client must still be able to learn who it is.

use axum::Json;
use serde::Serialize;
use utoipa::ToSchema;

use crate::error::ProblemDetail;
use crate::extract::CallerIdentity;

/// The authenticated principal.
#[derive(Debug, Serialize, ToSchema)]
pub struct PrincipalView {
    /// `app_user.id`.
    pub id: String,
    /// `app_user.oidc_subject` (`dev:<login>` for a dev principal).
    pub subject: String,
    pub display_name: String,
    pub email: Option<String>,
}

/// One tenant the principal belongs to.
#[derive(Debug, Serialize, ToSchema)]
pub struct MembershipView {
    pub tenant_id: String,
    pub slug: String,
    pub name: String,
    /// `owner | admin | member | viewer`.
    pub role: String,
}

/// The `/api/me` document.
#[derive(Debug, Serialize, ToSchema)]
pub struct MeResponse {
    pub principal: PrincipalView,
    /// The tenant this request acts in, when one was selected. `None` before a tenant is
    /// chosen — the shell renders the switcher from `memberships` and the next request names one.
    pub active_tenant: Option<MembershipView>,
    pub memberships: Vec<MembershipView>,
    /// `app_user.id` holds a `platform_admin` row: the provisioning surface is visible.
    pub is_super_admin: bool,
}

fn membership_view(m: &conveyor_auth::Membership) -> MembershipView {
    MembershipView {
        tenant_id: m.tenant_id.to_string(),
        slug: m.slug.clone(),
        name: m.name.clone(),
        role: m.role.as_str().to_owned(),
    }
}

impl
    From<(
        &conveyor_auth::IdentitySession,
        Option<&conveyor_auth::Session>,
    )> for MeResponse
{
    fn from(
        (session, active): (
            &conveyor_auth::IdentitySession,
            Option<&conveyor_auth::Session>,
        ),
    ) -> Self {
        Self {
            principal: PrincipalView {
                id: session.principal.user_id.to_string(),
                subject: session.principal.subject.clone(),
                display_name: session.principal.display_name.clone(),
                email: session.principal.email.clone(),
            },
            // `Some` exactly when a tenant was selected: a single membership, or a licence
            // bearer's own tenant, or an explicit `X-Active-Tenant`. `None` is the
            // multi-membership bootstrap the switcher needs, not a missing answer.
            active_tenant: active.map(|active| MembershipView {
                tenant_id: active.tenant.to_string(),
                slug: active.tenant_slug.clone(),
                name: active.tenant_name.clone(),
                role: active.role.as_str().to_owned(),
            }),
            memberships: session.memberships.iter().map(membership_view).collect(),
            is_super_admin: session.is_super_admin,
        }
    }
}

/// Who the caller is and which tenants it may act in.
#[utoipa::path(
    get,
    path = "/api/me",
    tag = "identity",
    responses(
        (status = 200, description = "The caller and its memberships", body = MeResponse),
        (status = 401, description = "No credential, or one that is refused", body = ProblemDetail)
    )
)]
pub async fn me(caller: CallerIdentity) -> Json<MeResponse> {
    Json(MeResponse::from((&caller.0, caller.1.as_ref())))
}
