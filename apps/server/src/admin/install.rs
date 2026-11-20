//! `conveyor admin install` — a fresh tenant, installed by one command (S14 P5).
//!
//! The platform wizard creates tenants through the HTTP provisioning route, authenticated as a
//! super-admin. This command is the operator's repeatable path: it creates (or converges) the
//! same local rows through `store::platform::create`, imports the catalogue and the default
//! process catalogue, and installs the tenant-scoped bindings and schedules named by a
//! manifest — through the **same action engine** a caller gets, never raw SQL for content.
//!
//! Idempotent: the tenant id is resolved by slug first, `platform::create` converges on the id,
//! `seed_catalogue`/`process_seed::apply_all` create nothing on a second run, and a schedule is
//! upserted by kind. A second `install` is therefore a no-op that reports what exists.

use std::path::{Path, PathBuf};

use conveyor_domain::TenantId;
use conveyor_store as store;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

use super::action::{ActionCaller, CallError};

/// The default manifest, relative to the working directory (the repository root in CI and on
/// the pod).
pub const DEFAULT_MANIFEST: &str = "seed/tenant-install.json";

/// What the command needs.
#[derive(Clone, Debug)]
pub struct InstallArgs<'a> {
    /// The canonical tenant slug.
    pub slug: &'a str,
    /// The tenant's display name.
    pub name: &'a str,
    /// The first admin's verified email — the identity the person signs in with.
    pub admin_email: &'a str,
    /// The first admin's display name.
    pub admin_name: &'a str,
    /// Adopt this tenant id instead of resolving by slug or minting one.
    pub tenant_id: Option<Uuid>,
    /// The install manifest; [`DEFAULT_MANIFEST`] when omitted.
    pub manifest: Option<PathBuf>,
}

/// What one run did.
#[derive(Debug)]
pub struct InstallReport {
    /// The tenant's id.
    pub tenant_id: Uuid,
    /// The slug, as installed.
    pub slug: String,
    /// The first admin's `app_user.id`.
    pub admin_user_id: Uuid,
    /// Whether the admin person row was created by this run.
    pub created_admin: bool,
    /// Card types created.
    pub card_types: usize,
    /// Capability agents created.
    pub agents: usize,
    /// Procedure (SOP) rows created.
    pub sops: usize,
    /// Processes created.
    pub processes: usize,
    /// Process maps created.
    pub maps: usize,
    /// Capabilities whose knowledge binding the manifest installed (or converged).
    pub knowledge: Vec<String>,
    /// Schedule kinds upserted.
    pub schedules: Vec<String>,
}

impl InstallReport {
    /// The CLI's machine-readable line.
    pub fn json(&self) -> Value {
        json!({
            "tenant_id": self.tenant_id,
            "slug": self.slug,
            "admin_user_id": self.admin_user_id,
            "created_admin": self.created_admin,
            "seed": {
                "card_types": self.card_types,
                "agents": self.agents,
                "sops": self.sops,
                "processes": self.processes,
                "maps": self.maps,
            },
            "knowledge": self.knowledge,
            "schedules": self.schedules,
        })
    }
}

/// The failures this command distinguishes.
#[derive(Debug, Error)]
pub enum InstallError {
    /// The database refused a read or a write.
    #[error("database: {0}")]
    Database(#[from] sqlx::Error),
    /// The platform create path refused (a slug held by another id, a non-human email).
    #[error("platform: {0}")]
    Platform(#[from] store::platform::PlatformError),
    /// The catalogue/process seed refused.
    #[error("seed: {0}")]
    Seed(#[from] crate::seed::SeedError),
    /// The action path refused.
    #[error(transparent)]
    Call(#[from] CallError),
    /// The manifest could not be read or parsed.
    #[error("manifest {path}: {detail}")]
    Manifest {
        /// The manifest path.
        path: String,
        /// The parse error.
        detail: String,
    },
    /// The manifest names a capability this tenant does not have.
    #[error("manifest: capability {code:?} does not exist in the installed tenant")]
    UnknownCapability {
        /// The code the manifest named.
        code: String,
    },
    /// The tenant id is not a valid non-nil id.
    #[error("tenant id: {0}")]
    Tenant(String),
}

/// The manifest: what a fresh tenant is born with beyond the catalogue and the process
/// catalogue. Both arrays may be empty — a tenant with no knowledge packs exists.
#[derive(Debug, Default, Deserialize)]
pub struct InstallManifest {
    /// Capability knowledge bindings to install.
    #[serde(default)]
    pub knowledge: Vec<KnowledgeBinding>,
    /// Schedules to arm.
    #[serde(default)]
    pub schedules: Vec<ScheduleSpec>,
}

/// One `capabilities.bind_knowledge` call.
#[derive(Debug, Deserialize)]
pub struct KnowledgeBinding {
    /// The capability's stable code (`architecture`, `pm`, …).
    pub capability: String,
    /// The knowledge pack's key.
    pub pack_key: String,
    /// The pack version (positive).
    pub pack_version: i32,
    /// Whether a run must have the pack before it starts.
    #[serde(default)]
    pub required: bool,
    /// The minimum retrieval score, when the pack is scored.
    #[serde(default)]
    pub min_score: Option<f64>,
}

/// One `schedules.upsert` call.
#[derive(Debug, Deserialize)]
pub struct ScheduleSpec {
    /// The job kind (unique per tenant).
    pub kind: String,
    /// A five-field cron expression.
    pub cron: String,
    /// Whether to arm it.
    #[serde(default = "armed")]
    pub enabled: bool,
    /// The payload every enqueued job carries.
    #[serde(default)]
    pub payload: Value,
}

fn armed() -> bool {
    true
}

/// Install (or converge) one tenant.
pub async fn run(
    pool: &PgPool,
    args: &InstallArgs<'_>,
    app_key: &str,
) -> Result<InstallReport, InstallError> {
    let slug = args.slug.trim();
    // Resolve by slug first: a second run adopts the id the first run created, so the command
    // converges instead of colliding on the unique slug.
    let tenant_id = match args.tenant_id {
        Some(id) => id,
        None => store::tenants::id_by_slug(pool, slug)
            .await?
            .unwrap_or_else(Uuid::now_v7),
    };
    let tenant =
        TenantId::from_uuid(tenant_id).map_err(|error| InstallError::Tenant(error.to_string()))?;

    // The audit row of a CLI-created tenant names a real person: the first admin's row is
    // converged first and used as the actor.
    let mut tx = store::begin_scoped(pool, tenant).await?;
    let (admin_user_id, created_admin) =
        store::platform::ensure_human(&mut tx, args.admin_email, args.admin_name).await?;
    tx.commit().await?;

    store::platform::create(
        pool,
        store::platform::NewTenant {
            tenant,
            slug,
            name: args.name,
            email: args.admin_email,
            display_name: args.admin_name,
            actor: admin_user_id,
            request_id: Some("admin.install"),
        },
    )
    .await?;

    // Catalogue + the default process catalogue: card types, capabilities, agents, SOPs,
    // processes and maps. Idempotent on a second run (`apply_all` creates nothing).
    let counts = crate::seed::seed_process_catalogue(pool, Some(slug))
        .await?
        .into_iter()
        .next()
        .map(|(_, counts)| counts)
        .ok_or_else(|| {
            InstallError::Seed(crate::seed::SeedError::Missing(
                "tenant",
                "the installed tenant after the process catalogue",
            ))
        })?;

    let manifest = read_manifest(
        args.manifest
            .as_deref()
            .unwrap_or(Path::new(DEFAULT_MANIFEST)),
    )?;
    let caller = ActionCaller::new(pool, tenant, app_key).await?;

    let mut knowledge = Vec::new();
    for binding in &manifest.knowledge {
        let capability_id = capability_id_by_code(&caller, &binding.capability).await?;
        let mut input = json!({
            "capability_id": capability_id,
            "pack_key": binding.pack_key,
            "pack_version": binding.pack_version,
            "required": binding.required,
        });
        if let Some(score) = binding.min_score {
            input["min_score"] = json!(score);
        }
        caller
            .call("capabilities.bind_knowledge", input, None)
            .await?;
        knowledge.push(binding.capability.clone());
    }

    let mut schedules = Vec::new();
    for schedule in &manifest.schedules {
        caller
            .call(
                "schedules.upsert",
                json!({
                    "kind": schedule.kind,
                    "cron": schedule.cron,
                    "enabled": schedule.enabled,
                    "payload": schedule.payload,
                }),
                None,
            )
            .await?;
        schedules.push(schedule.kind.clone());
    }

    Ok(InstallReport {
        tenant_id,
        slug: slug.to_owned(),
        admin_user_id,
        created_admin,
        card_types: counts.card_types,
        agents: counts.agents,
        sops: counts.sops,
        processes: counts.processes,
        maps: counts.maps,
        knowledge,
        schedules,
    })
}

fn read_manifest(path: &Path) -> Result<InstallManifest, InstallError> {
    let text = std::fs::read_to_string(path).map_err(|error| InstallError::Manifest {
        path: path.display().to_string(),
        detail: error.to_string(),
    })?;
    serde_json::from_str(&text).map_err(|error| InstallError::Manifest {
        path: path.display().to_string(),
        detail: error.to_string(),
    })
}

/// Resolve a capability's stable code to its row id through the registered read action.
async fn capability_id_by_code(
    caller: &ActionCaller<'_>,
    code: &str,
) -> Result<Uuid, InstallError> {
    let detail = caller
        .call("capabilities.get", json!({"code": code}), None)
        .await?;
    detail
        .get("id")
        .and_then(Value::as_str)
        .and_then(|id| Uuid::parse_str(id).ok())
        .ok_or_else(|| InstallError::UnknownCapability {
            code: code.to_owned(),
        })
}
