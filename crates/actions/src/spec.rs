#![forbid(unsafe_code)]
//! `ActionSpec` — the one declaration every surface is projected from (S04 §5.1).
//!
//! A projection that reads a second source is the bug this module exists to prevent. The
//! manifest, the OpenAPI operation, the MCP tool and the invocation route are all derived
//! from the same `ActionSpec` value, and [`spec_hash`] is the same hash of the same
//! canonical projection in each of them. `parity_http_mcp.rs` re-computes it from the
//! `$ref`-resolved operation and from the MCP tool, so the claim is checkable and not a
//! comment.

use std::collections::BTreeMap;

use conveyor_auth::{Role, Session};

use crate::invoke::{ActionError, HandlerCtx};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub use schemars::Schema;

/// A schema projection: `schema_for!` in the `#[action]` expansion.
pub type SchemaFn = fn() -> Schema;

// The compiled machine-channel defaults and the floor live in `crate::channel` (S06d).

/// CONTRACT §6's closed vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ActionKind {
    /// A handler compiled into this binary.
    Internal,
    /// A service operation ingested from its `openapi.json`.
    External,
    /// The S13 workflow engine; declared now, executed later (S4-D7).
    Workflow,
}

impl ActionKind {
    /// The spelling used in the manifest, the OpenAPI extension and `action_registry.kind`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Internal => "internal",
            Self::External => "external",
            Self::Workflow => "workflow",
        }
    }
}

/// What an invocation does to the tenant's state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Reads only; no `action_invocation` mutation beyond the log row itself.
    Read,
    /// Creates or updates.
    Write,
    /// Deletes or irreversibly transitions.
    Destructive,
}

impl Effect {
    /// The spelling used in the manifest and OpenAPI.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Destructive => "destructive",
        }
    }
}

/// How a channel asks the human before executing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Confirm {
    /// No confirmation.
    None,
    /// A card is posted and the action waits for a decision.
    Card,
    /// A card that requires a typed confirmation phrase.
    CardTyped,
}

impl Confirm {
    /// The spelling used in the manifest.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Card => "card",
            Self::CardTyped => "card-typed",
        }
    }
}

/// The scope a caller's tenant resolution is confined to.
///
/// `Self` is spelled `Self_` because it is a Rust keyword; the spelling on the wire is
/// `self`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tenancy {
    /// Only the caller's own principal.
    Self_,
    /// The caller's tenant.
    Tenant,
    /// One project inside the tenant.
    Project,
    /// Above every tenant.
    Platform,
}

impl Tenancy {
    /// The spelling used in the manifest.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Self_ => "self",
            Self::Tenant => "tenant",
            Self::Project => "project",
            Self::Platform => "platform",
        }
    }
}

/// The scope vocabulary of this slice. A scope outside this list is a registry error, not
/// an action nobody can call.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Scope {
    ActionsRead,
    ActionsWrite,
    MeRead,
    TenantsRead,
    TenantsWrite,
    MembersRead,
    MembersWrite,
    DepartmentsRead,
    DepartmentsWrite,
    AuditRead,
    JobsRead,
    JobsWrite,
    SchedulesRead,
    SchedulesWrite,
    EventsRead,
    LicencesRead,
    ProjectsRead,
    ProjectsWrite,
    BoardsRead,
    BoardsWrite,
    ListsRead,
    ListsWrite,
    CardsRead,
    CardsWrite,
    // S06 — the control plane. Nine resource:verb pairs across the slice's six action
    // families; `cards.claim`/`.renew`/`.release` reuse `CardsWrite`.
    RunsRead,
    RunsWrite,
    ApprovalsRead,
    ApprovalsWrite,
    AndonWrite,
    MessagesWrite,
    NotificationsRead,
    NotificationsWrite,
    ChannelsWrite,
    // S07 — the requirements lifecycle (`governance:*`), the read-only catalogue vocabulary
    // (`catalogues:*`), the CP→conveyor intake bridge (`requests:*`) and ranked search.
    /// Read requirements, principles, decisions and their revisions.
    GovernanceRead,
    /// Create or change a requirement, principle or decision.
    GovernanceWrite,
    /// Read catalogue entries.
    CataloguesRead,
    /// Import catalogue entries — platform administration.
    CataloguesWrite,
    /// Read inbound project requests.
    RequestsRead,
    /// Create, decide and promote inbound project requests.
    RequestsWrite,
    /// Run a ranked search over assets, cards and catalogue entries.
    SearchRead,
    // S08 — the portfolio surface that survives: stakeholders and the programmes over S03's
    // `programme`. **Both share this pair.** They deliberately do NOT reuse `ProjectsRead` /
    // `ProjectsWrite`: `ProjectsWrite` is admin-only, and a member owns stakeholders.
    // ⚠️ 2026-11-08: capabilities, deliverables, registers and TOCs were dropped by migration
    // 0011 (S00 day 2), and their `portfolio:read`/`portfolio:write` users went with them.
    /// Read programmes and stakeholders.
    PortfolioRead,
    /// Create, change or delete a programme or a stakeholder.
    PortfolioWrite,
    /// Read models, their elements, relationships and views, and the architecture tools.
    ModelRead,
    /// Create, change or delete a model, element, relationship, view, object or connection,
    /// and publish or validate a model.
    ModelWrite,
    // S09 — documents (bytes in the object store) and secrets (write-only, sealed at rest). Secrets do NOT
    // reuse `DocumentsRead`: a tenant may let a member read documents without letting them enumerate or
    // rotate credentials, and a single pair would make that impossible to express.
    /// Read documents and download their bytes.
    DocumentsRead,
    /// Upload, change and delete documents.
    DocumentsWrite,
    /// Read secret metadata (never a value — there is no reveal action).
    SecretsRead,
    /// Create, rotate and delete secrets.
    SecretsWrite,
    /// Call an external service's read surface (workers, artifacts, billing, retrieval, coding job status).
    IntegrationsRead,
    /// Dispatch work to an external service (`coding_jobs.create`).
    IntegrationsWrite,
    // S11 — executable processes: ordered registry actions on one card.
    /// Read process templates, their steps and their published revisions.
    ProcessesRead,
    /// Create, change, publish and retire a process template.
    ProcessesWrite,
    // S17 — the benchmark harness. `benchmarks:read` covers suites, runs, cases and compare;
    // `benchmarks:write` starts, cancels and promotes runs. No benchmark action is in
    // `AGENT_TOOLS`: a model-driven principal cannot start a run or read an answer (CONTRACT §19).
    /// Read suites, runs, cases and comparisons.
    BenchmarksRead,
    /// Start, cancel and promote benchmark runs.
    BenchmarksWrite,
    // S16 — the inference ledger and the prompt audit.
    /// Read the inference ledger, the audits and their findings.
    InferenceRead,
    /// Start an audit and decide a finding.
    InferenceWrite,
    // S19 — the eval instrument. Three scopes, deliberately not two: **building is not
    // signing**, and the held-out trace and the run cost hang off the sign-off.
    /// Read evals, their versions, cases and metrics.
    ///
    /// Held by `member` and above rather than by every role: an eval's case inputs and its
    /// held-out split are the instrument, not general project content.
    EvalsRead,
    /// Create, change and retire an eval, its cases and its metrics.
    EvalsWrite,
    /// Sign off an eval's inputs and grading — a human act, never a machine's.
    ///
    /// ⛔ `Owner | Admin` **explicitly**, never the `admin` predicate: that predicate includes
    /// `Role::KataExecutor`, which is held only by a machine principal, and a scope a machine
    /// holds cannot be a sign-off (CONTRACT §20, trap 1).
    EvalsApprove,
    // S13 — the workflow engine's templates. One document per version, versioned by key; the
    // engine executes published versions only, so `workflows:write` is an authoring permission.
    /// Read workflow definitions, their versions and their published revisions.
    WorkflowsRead,
    /// Create, change, publish and retire a workflow definition.
    WorkflowsWrite,
    // S15 — the capability aggregate: its configured executor, knowledge bindings, relations and
    // work-package memory. A read is open; a write is `member`-and-above, intersected with the
    // caller's own scopes at readiness (CONTRACT §17), so a member cannot configure an executor
    // to act beyond what they may do themselves.
    /// Read capabilities, their agents, their knowledge bindings and their relations.
    CapabilitiesRead,
    /// Configure an executor, bind a knowledge pack and write work-package memory.
    CapabilitiesWrite,
    // T0 — the organisation spine (SPEC_V2 §4): industries, the job catalogue, releases and
    // bindings. A read is open; a write is platform administration, like `departments:write`.
    /// Read industries, the job catalogue, releases and bindings.
    OrganisationRead,
    /// Create and change industries, jobs, releases and bindings.
    OrganisationWrite,
}

impl Scope {
    /// Every scope this build knows.
    pub const ALL: [Scope; 65] = [
        Scope::ActionsRead,
        Scope::ActionsWrite,
        Scope::MeRead,
        Scope::TenantsRead,
        Scope::TenantsWrite,
        Scope::MembersRead,
        Scope::MembersWrite,
        Scope::DepartmentsRead,
        Scope::DepartmentsWrite,
        Scope::AuditRead,
        Scope::JobsRead,
        Scope::JobsWrite,
        Scope::SchedulesRead,
        Scope::SchedulesWrite,
        Scope::EventsRead,
        Scope::LicencesRead,
        Scope::ProjectsRead,
        Scope::ProjectsWrite,
        Scope::BoardsRead,
        Scope::BoardsWrite,
        Scope::ListsRead,
        Scope::ListsWrite,
        Scope::CardsRead,
        Scope::CardsWrite,
        Scope::RunsRead,
        Scope::RunsWrite,
        Scope::ApprovalsRead,
        Scope::ApprovalsWrite,
        Scope::AndonWrite,
        Scope::MessagesWrite,
        Scope::NotificationsRead,
        Scope::NotificationsWrite,
        Scope::ChannelsWrite,
        Scope::GovernanceRead,
        Scope::GovernanceWrite,
        Scope::CataloguesRead,
        Scope::CataloguesWrite,
        Scope::RequestsRead,
        Scope::RequestsWrite,
        Scope::SearchRead,
        Scope::PortfolioRead,
        Scope::PortfolioWrite,
        Scope::ModelRead,
        Scope::ModelWrite,
        Scope::DocumentsRead,
        Scope::DocumentsWrite,
        Scope::SecretsRead,
        Scope::SecretsWrite,
        Scope::IntegrationsRead,
        Scope::IntegrationsWrite,
        Scope::ProcessesRead,
        Scope::ProcessesWrite,
        Scope::BenchmarksRead,
        Scope::BenchmarksWrite,
        Scope::InferenceRead,
        Scope::InferenceWrite,
        Scope::EvalsRead,
        Scope::EvalsWrite,
        Scope::EvalsApprove,
        Scope::WorkflowsRead,
        Scope::WorkflowsWrite,
        Scope::CapabilitiesRead,
        Scope::CapabilitiesWrite,
        Scope::OrganisationRead,
        Scope::OrganisationWrite,
    ];

    /// The wire spelling (`<resource>:<verb>`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ActionsRead => "actions:read",
            Self::ActionsWrite => "actions:write",
            Self::MeRead => "me:read",
            Self::TenantsRead => "tenants:read",
            Self::TenantsWrite => "tenants:write",
            Self::MembersRead => "members:read",
            Self::MembersWrite => "members:write",
            Self::DepartmentsRead => "departments:read",
            Self::DepartmentsWrite => "departments:write",
            Self::AuditRead => "audit:read",
            Self::JobsRead => "jobs:read",
            Self::JobsWrite => "jobs:write",
            Self::SchedulesRead => "schedules:read",
            Self::SchedulesWrite => "schedules:write",
            Self::EventsRead => "events:read",
            Self::LicencesRead => "licences:read",
            Self::ProjectsRead => "projects:read",
            Self::ProjectsWrite => "projects:write",
            Self::BoardsRead => "boards:read",
            Self::BoardsWrite => "boards:write",
            Self::ListsRead => "lists:read",
            Self::ListsWrite => "lists:write",
            Self::CardsRead => "cards:read",
            Self::CardsWrite => "cards:write",
            Self::RunsRead => "runs:read",
            Self::RunsWrite => "runs:write",
            Self::ApprovalsRead => "approvals:read",
            Self::ApprovalsWrite => "approvals:write",
            Self::AndonWrite => "andon:write",
            Self::MessagesWrite => "messages:write",
            Self::NotificationsRead => "notifications:read",
            Self::NotificationsWrite => "notifications:write",
            Self::ChannelsWrite => "channels:write",
            Self::GovernanceRead => "governance:read",
            Self::GovernanceWrite => "governance:write",
            Self::CataloguesRead => "catalogues:read",
            Self::CataloguesWrite => "catalogues:write",
            Self::RequestsRead => "requests:read",
            Self::RequestsWrite => "requests:write",
            Self::SearchRead => "search:read",
            Self::PortfolioRead => "portfolio:read",
            Self::PortfolioWrite => "portfolio:write",
            Self::ModelRead => "model:read",
            Self::ModelWrite => "model:write",
            Self::DocumentsRead => "documents:read",
            Self::DocumentsWrite => "documents:write",
            Self::SecretsRead => "secrets:read",
            Self::SecretsWrite => "secrets:write",
            Self::IntegrationsRead => "integrations:read",
            Self::IntegrationsWrite => "integrations:write",
            Self::ProcessesRead => "processes:read",
            Self::ProcessesWrite => "processes:write",
            Self::BenchmarksRead => "benchmarks:read",
            Self::BenchmarksWrite => "benchmarks:write",
            Self::InferenceRead => "inference:read",
            Self::InferenceWrite => "inference:write",
            Self::EvalsRead => "evals:read",
            Self::EvalsWrite => "evals:write",
            Self::EvalsApprove => "evals:approve",
            Self::WorkflowsRead => "workflows:read",
            Self::WorkflowsWrite => "workflows:write",
            Self::CapabilitiesRead => "capabilities:read",
            Self::CapabilitiesWrite => "capabilities:write",
            Self::OrganisationRead => "organisation:read",
            Self::OrganisationWrite => "organisation:write",
        }
    }

    /// Whether a role holds this scope. Reads are open to every member; a write on the work
    /// itself (a run, a gate, the cord, a message, a notification preference, a requirement, a
    /// project request, a stakeholder, a deliverable) is `member`/`admin`/`owner`; a write on
    /// the platform's configuration is `admin`/`owner` only — `channels:write` binds a route to
    /// a tenant and `catalogues:write` imports the shared vocabulary, both platform
    /// administration.
    /// The mapping lives here, once, so HTTP and MCP cannot disagree about who may call what.
    ///
    /// S19's three scopes are the deliberate exception on both sides: `evals:read` and
    /// `evals:write` are `member`-and-above rather than open reads (an eval's case inputs and
    /// its held-out split are the instrument, not general project content), and
    /// `evals:approve` is `Admin` literally, because the `admin` predicate would hand a
    /// sign-off to the machine role `kata_executor`.
    pub fn granted_to(self, role: Role) -> bool {
        // `kata_executor` is the synthetic tenant-admin (George 2026-11-09): it holds every
        // tenant scope an admin does, so it appears in both predicates. It is granted only to
        // a machine principal (`members.set_role` refuses it for a human), and the MCP agent
        // allowlist still bounds what an agent principal may call.
        let admin = matches!(role, Role::Admin | Role::KataExecutor);
        let member = matches!(role, Role::Admin | Role::Member | Role::KataExecutor);
        match self {
            Self::ActionsRead
            | Self::MeRead
            | Self::TenantsRead
            | Self::MembersRead
            | Self::DepartmentsRead
            | Self::AuditRead
            | Self::JobsRead
            | Self::SchedulesRead
            | Self::EventsRead
            | Self::LicencesRead
            | Self::ProjectsRead
            | Self::BoardsRead
            | Self::ListsRead
            | Self::CardsRead
            | Self::RunsRead
            | Self::ApprovalsRead
            | Self::NotificationsRead
            | Self::GovernanceRead
            | Self::CataloguesRead
            | Self::RequestsRead
            | Self::SearchRead
            | Self::PortfolioRead
            | Self::ModelRead
            | Self::DocumentsRead
            | Self::SecretsRead
            | Self::ProcessesRead
            | Self::BenchmarksRead
            | Self::InferenceRead
            | Self::IntegrationsRead
            | Self::WorkflowsRead
            | Self::CapabilitiesRead
            | Self::OrganisationRead => true,
            Self::RunsWrite
            | Self::ApprovalsWrite
            | Self::AndonWrite
            | Self::MessagesWrite
            | Self::NotificationsWrite
            | Self::GovernanceWrite
            | Self::RequestsWrite
            | Self::PortfolioWrite
            | Self::ModelWrite
            | Self::DocumentsWrite
            | Self::SecretsWrite
            | Self::ProcessesWrite
            | Self::BenchmarksWrite
            | Self::InferenceWrite
            | Self::IntegrationsWrite
            | Self::EvalsRead
            | Self::EvalsWrite
            | Self::WorkflowsWrite
            | Self::CapabilitiesWrite => member,
            Self::ChannelsWrite
            | Self::CataloguesWrite
            | Self::ActionsWrite
            | Self::TenantsWrite
            | Self::MembersWrite
            | Self::DepartmentsWrite
            | Self::JobsWrite
            | Self::SchedulesWrite
            | Self::ProjectsWrite
            | Self::BoardsWrite
            | Self::ListsWrite
            | Self::CardsWrite
            | Self::OrganisationWrite => admin,
            // ⛔ NOT the `admin` predicate: that includes `Role::KataExecutor`, a machine
            // role, and a scope a machine holds cannot be a sign-off (CONTRACT §20 trap 1).
            Self::EvalsApprove => matches!(role, Role::Admin),
        }
    }
}

/// What a compiled handler returns: its output document plus the success status and any
/// response header the action's contract declares.
///
/// The default is `200` with no header — every S04 action returns `HandlerOutput::ok`, which
/// the `#[action]` macro wraps automatically. An action whose contract says otherwise
/// (`cards.create` is `201 + ETag`, `cards.archive` is `204`) declares it by returning
/// `HandlerOutput<T>` directly; the macro reads the inner `T` as the output schema, so the
/// OpenAPI operation and the MCP tool still describe the action's own document and not this
/// transport envelope.
#[derive(Clone, Debug)]
pub struct HandlerOutput<T = Value> {
    /// The action's output document.
    pub value: T,
    /// The HTTP status a successful invocation returns.
    pub status: u16,
    /// An `ETag` header value, when the contract declares one.
    pub etag: Option<String>,
}

impl<T> HandlerOutput<T> {
    /// `200` with no header — the ordinary success.
    pub fn ok(value: T) -> Self {
        Self {
            value,
            status: 200,
            etag: None,
        }
    }

    /// `201 Created`.
    pub fn created(value: T) -> Self {
        Self {
            value,
            status: 201,
            etag: None,
        }
    }

    /// `204 No Content`. The output document is discarded.
    pub fn no_content(value: T) -> Self {
        Self {
            value,
            status: 204,
            etag: None,
        }
    }

    /// Attach an `ETag` value (the representation's `version`).
    pub fn with_etag(mut self, etag: impl Into<String>) -> Self {
        self.etag = Some(etag.into());
        self
    }
}

/// The handler future a compiled action returns. The transaction is borrowed, so the
/// handler cannot outlive it (a detached write would escape the invocation's rollback).
pub type HandlerFuture<'a> = std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<HandlerOutput, ActionError>> + Send + 'a>,
>;

/// An internal handler: `(transaction, context, validated input) -> output`.
pub type HandlerFn = for<'a> fn(&'a mut sqlx::PgConnection, HandlerCtx, Value) -> HandlerFuture<'a>;

/// Where the work happens.
#[derive(Clone, Copy)]
pub enum HandlerRef {
    /// A compiled handler in this binary.
    Internal(HandlerFn),
    /// A service operation. `path` is the operation's path in the service's own document;
    /// the base URL is configuration (`<SERVICE>_URL`), never compiled in.
    External {
        /// The service key (`fixtures`).
        service: &'static str,
        /// The HTTP method the operation uses.
        method: &'static str,
        /// The operation path.
        path: &'static str,
        /// The name of a per-tenant sealed secret whose value is sent as the bearer. `None`
        /// for an unauthenticated service. The engine opens it under the caller's tenant; a
        /// missing secret is a typed `503`, never an unauthenticated call — unless
        /// `auth_optional` is set.
        auth: Option<&'static str>,
        /// When true, an absent secret sends **no** bearer instead of refusing with a `503`.
        /// It is for services with a tenant-BYO credential: the tenant may have registered one
        /// (`secrets.create`/`rotate`), and the service itself is the one that decides whether
        /// an anonymous call is acceptable. The engine never substitutes a platform token.
        auth_optional: bool,
    },
    /// No handler yet: S13's engine.
    Workflow,
}

impl std::fmt::Debug for HandlerRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Internal(_) => write!(f, "HandlerRef::Internal"),
            Self::External {
                service,
                method,
                path,
                ..
            } => write!(f, "HandlerRef::External({service} {method} {path})"),
            Self::Workflow => write!(f, "HandlerRef::Workflow"),
        }
    }
}

/// The copilot's tier for an action (S10 §5.2; fr's `PROJECT_AGENT_CORE` / `PROJECT_AGENT_SEARCHABLE`).
///
/// A hint, not a filter: the surface is still the scope-and-kind projection, so an action with no
/// tier is reachable — it is only absent from the model's short list. Tier lists that lived outside
/// `ActionSpec` were a second registry (S10 trap 1), which is why this lives here.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AgentTier {
    /// No copilot hint declared.
    #[default]
    None,
    /// Always offered to the copilot's model.
    Core,
    /// Offered through search rather than in the short list.
    Searchable,
}

impl AgentTier {
    /// The spelling used in the manifest's `agent` extension.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Core => "core",
            Self::Searchable => "searchable",
        }
    }

    /// Parse the manifest spelling; `None` for anything else.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "none" => Some(Self::None),
            "core" => Some(Self::Core),
            "searchable" => Some(Self::Searchable),
            _ => None,
        }
    }
}

/// How the copilot exposes an action (S10-D1: a defaulted `ActionSpec` field, never a second table).
///
/// Deliberately **not** part of [`ActionSpec::canonical`]: the hash is the contract every transport
/// reproduces, and a display hint is not part of the contract. Keeping it out also means the 187
/// existing action hashes do not move when a hint is declared.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AgentHints {
    /// The fr surface tier.
    pub tier: AgentTier,
    /// Whether a copilot tool call must become a proposal even when the effect alone would not
    /// (S10 §5.3). `effect = destructive` and `confirm != none` propose regardless.
    pub proposal_required: bool,
}

/// One invocable operation. Exactly §5.1's fields.
#[derive(Clone, Debug)]
pub struct ActionSpec {
    /// `<plural resource>.<verb>`, exactly one dot (CONTRACT §6).
    pub name: &'static str,
    /// The integer version; not part of the name.
    pub version: u32,
    /// Internal, external or workflow.
    pub kind: ActionKind,
    /// The request schema (schemars).
    pub input: SchemaFn,
    /// The response schema (schemars).
    pub output: SchemaFn,
    /// The scopes a caller must hold (all of them).
    pub scopes: &'static [Scope],
    /// Whether replaying the same key is safe.
    pub idempotent: bool,
    /// The caller's deadline, in milliseconds.
    pub timeout_ms: u32,
    /// Read, write or destructive.
    pub effect: Effect,
    /// Confirmation mode for channels.
    pub confirm: Confirm,
    /// The resolution scope.
    pub tenancy: Tenancy,
    /// The copilot's hints; defaulted, and never part of [`ActionSpec::canonical`].
    pub agent: AgentHints,
    /// Whether `/mcp` may run this action as a task (`tasks/*`, S06c). The action itself is the
    /// task's *start* operation; the task id is the run it created. Defaulted by the derive
    /// (`task = true`); like [`ActionSpec::agent`], it is **not** part of [`ActionSpec::canonical`],
    /// so declaring it does not move an operation's hash.
    pub task_capable: bool,
    /// Whether the engine writes an `inference_call` ledger row for this action's successful
    /// external call (S16). Defaulted by the derive; like [`ActionSpec::agent`] and
    /// [`ActionSpec::task_capable`], it is **not** part of [`ActionSpec::canonical`], so declaring
    /// it does not move an operation's hash.
    pub captures_usage: bool,
    /// Where the work happens.
    pub handler: HandlerRef,
}

impl ActionSpec {
    /// The input schema as JSON (the same value the OpenAPI operation and the MCP tool
    /// carry).
    pub fn input_schema(&self) -> Value {
        schema_json((self.input)())
    }

    /// The output schema as JSON.
    pub fn output_schema(&self) -> Value {
        schema_json((self.output)())
    }

    /// `[<scope>, …]` in declaration order.
    pub fn scope_names(&self) -> Vec<&'static str> {
        self.scopes.iter().map(|s| s.as_str()).collect()
    }

    /// The MCP tool name: `.` → `_` (CONTRACT §6). Not injective; the registry asserts it.
    pub fn mcp_name(&self) -> String {
        self.name.replace('.', "_")
    }

    /// The one canonical contract projection [`spec_hash`] hashes.
    pub fn canonical(&self) -> Value {
        json!({
            "name": self.name,
            "version": self.version,
            "kind": self.kind.as_str(),
            "scopes": self.scope_names(),
            "idempotent": self.idempotent,
            "timeout_ms": self.timeout_ms,
            "effect": self.effect.as_str(),
            "confirm": self.confirm.as_str(),
            "tenancy": self.tenancy.as_str(),
            "input": self.input_schema(),
            "output": self.output_schema(),
        })
    }

    /// The contract hash: sha256 over [`ActionSpec::canonical`].
    pub fn spec_hash(&self) -> String {
        sha256_hex(&serde_json::to_vec(&self.canonical()).unwrap_or_default())
    }

    /// Whether a caller holding `role` may invoke it (all declared scopes held).
    pub fn permits(&self, role: Role) -> bool {
        self.scopes.iter().all(|scope| scope.granted_to(role))
    }

    /// Whether a role holds every scope, with the tenant's stored overrides applied.
    pub fn permits_with(&self, role: Role, policy: &conveyor_auth::ScopePolicy) -> bool {
        self.scopes
            .iter()
            .all(|scope| policy.allows(scope.as_str(), scope.granted_to(role)))
    }

    /// Whether the caller's **session** holds every scope this action needs.
    ///
    /// The compiled table is the default; a tenant's stored `tenant_role_scope` row overrides it.
    /// This is the predicate HTTP, `/mcp`, the agent surface, the copilot proposal and process
    /// publish should call — `permits(role)` exists for tests and for a caller with no session.
    pub fn permits_session(&self, session: &Session) -> bool {
        self.permits_with(session.role, &session.policy)
    }

    /// Whether a machine connector authority may invoke it (S06d).
    ///
    /// The grant (tenant override or compiled default) **and** the floor must both pass. The
    /// floor is computed from this spec's own declaration, never from the role table, so a
    /// channel's `kata_executor` membership cannot widen a grant.
    pub fn permits_channel(&self, authority: &conveyor_auth::ChannelAuthority) -> bool {
        !self.machine_forbidden() && crate::channel::allows(authority, self.name)
    }

    /// Whether this action is closed to **every** machine channel, whatever a tenant stores.
    ///
    /// Three independent reasons, all structural rather than name-list-only where possible:
    /// the `evals:approve` scope (a scope a machine holds cannot be a sign-off, CONTRACT §20),
    /// an administrative resource (including `channels`, which would let a grant rewrite
    /// grants), and the explicit held-out reads that declare an ordinary read scope.
    pub fn machine_forbidden(&self) -> bool {
        self.scopes
            .iter()
            .any(|scope| scope.as_str() == crate::channel::MACHINE_DENIED_SCOPE)
            || crate::channel::is_denied_action(self.name)
            || crate::channel::is_denied_resource(crate::channel::resource_of(self.name))
    }

    /// The literal route path `POST /api/actions/<name>/invocations`.
    pub fn route_path(&self) -> String {
        format!("/api/actions/{}/invocations", self.name)
    }

    /// The OpenAPI operation id (also the MCP-adjacent identifier).
    pub fn operation_id(&self) -> String {
        format!("{}_invoke", self.name.replace('.', "_"))
    }
}

/// A schemars `Schema` as the JSON value the projections embed.
pub fn schema_json(schema: Schema) -> Value {
    serde_json::to_value(schema).unwrap_or(Value::Null)
}

/// sha256, lowercase hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// `<plural>.<verb>`, exactly one dot, both halves lower_snake_case (CONTRACT §6).
pub fn validate_name(name: &str) -> Result<(), String> {
    let Some((resource, verb)) = name.split_once('.') else {
        return Err("must be exactly <resource>.<verb>".into());
    };
    if verb.contains('.') {
        return Err("must contain exactly one dot".into());
    }
    let word = |s: &str| {
        !s.is_empty()
            && s.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    };
    if !word(resource) {
        return Err(format!("resource {resource:?} is not lower_snake_case"));
    }
    if !word(verb) {
        return Err(format!("verb {verb:?} is not lower_snake_case"));
    }
    Ok(())
}

/// The MCP tool-name grammar: `^[a-z0-9_]{1,64}$`.
pub fn validate_mcp_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > 64 {
        return Err(format!(
            "MCP tool name {name:?} must be 1–64 characters, got {}",
            name.len()
        ));
    }
    if !name
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    {
        return Err(format!(
            "MCP tool name {name:?} is outside ^[a-z0-9_]{{1,64}}$"
        ));
    }
    Ok(())
}

/// A parsed `{"input": …}` invocation body.
pub fn envelope_input(body: &Value) -> Option<&Value> {
    body.get("input")
}

/// Scope → whether the role holds every one. Kept as a free function so the MCP filter and
/// the HTTP check call the same code.
pub fn scopes_permitted(scopes: &[Scope], role: Role) -> bool {
    scopes.iter().all(|scope| scope.granted_to(role))
}

/// A map from MCP tool name → action name, for boot-time collision detection.
pub fn mcp_collisions<'a>(
    names: impl IntoIterator<Item = &'a str>,
) -> BTreeMap<String, Vec<&'a str>> {
    let mut seen: BTreeMap<String, Vec<&'a str>> = BTreeMap::new();
    for name in names {
        seen.entry(name.replace('.', "_")).or_default().push(name);
    }
    seen.retain(|_, actions| actions.len() > 1);
    seen
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vocabularies_are_the_contract_spelling() {
        assert_eq!(ActionKind::External.as_str(), "external");
        assert_eq!(Effect::Destructive.as_str(), "destructive");
        assert_eq!(Confirm::CardTyped.as_str(), "card-typed");
        assert_eq!(Tenancy::Self_.as_str(), "self");
        assert_eq!(Scope::MembersWrite.as_str(), "members:write");
        assert_eq!(Scope::CardsWrite.as_str(), "cards:write");
        assert_eq!(Scope::RunsRead.as_str(), "runs:read");
        assert_eq!(Scope::RunsWrite.as_str(), "runs:write");
        assert_eq!(Scope::ApprovalsRead.as_str(), "approvals:read");
        assert_eq!(Scope::ApprovalsWrite.as_str(), "approvals:write");
        assert_eq!(Scope::AndonWrite.as_str(), "andon:write");
        assert_eq!(Scope::MessagesWrite.as_str(), "messages:write");
        assert_eq!(Scope::NotificationsRead.as_str(), "notifications:read");
        assert_eq!(Scope::NotificationsWrite.as_str(), "notifications:write");
        assert_eq!(Scope::ChannelsWrite.as_str(), "channels:write");
        assert_eq!(Scope::GovernanceRead.as_str(), "governance:read");
        assert_eq!(Scope::GovernanceWrite.as_str(), "governance:write");
        assert_eq!(Scope::CataloguesRead.as_str(), "catalogues:read");
        assert_eq!(Scope::CataloguesWrite.as_str(), "catalogues:write");
        assert_eq!(Scope::RequestsRead.as_str(), "requests:read");
        assert_eq!(Scope::RequestsWrite.as_str(), "requests:write");
        assert_eq!(Scope::SearchRead.as_str(), "search:read");
        assert_eq!(Scope::PortfolioRead.as_str(), "portfolio:read");
        assert_eq!(Scope::PortfolioWrite.as_str(), "portfolio:write");
        assert_eq!(Scope::ModelRead.as_str(), "model:read");
        assert_eq!(Scope::ModelWrite.as_str(), "model:write");
        assert_eq!(Scope::DocumentsRead.as_str(), "documents:read");
        assert_eq!(Scope::DocumentsWrite.as_str(), "documents:write");
        assert_eq!(Scope::SecretsRead.as_str(), "secrets:read");
        assert_eq!(Scope::SecretsWrite.as_str(), "secrets:write");
        // 24 from S02–S05, S06's nine control-plane scopes, S07's seven, S08 Part A's two,
        // S08 Part B's two, S09's six, S11's two, S16's two, S19's three and S13's two.
        assert_eq!(Scope::IntegrationsRead.as_str(), "integrations:read");
        assert_eq!(Scope::IntegrationsWrite.as_str(), "integrations:write");
        assert_eq!(Scope::ProcessesRead.as_str(), "processes:read");
        assert_eq!(Scope::ProcessesWrite.as_str(), "processes:write");
        assert_eq!(Scope::InferenceRead.as_str(), "inference:read");
        assert_eq!(Scope::InferenceWrite.as_str(), "inference:write");
        assert_eq!(Scope::EvalsRead.as_str(), "evals:read");
        assert_eq!(Scope::EvalsWrite.as_str(), "evals:write");
        assert_eq!(Scope::EvalsApprove.as_str(), "evals:approve");
        assert_eq!(Scope::WorkflowsRead.as_str(), "workflows:read");
        assert_eq!(Scope::WorkflowsWrite.as_str(), "workflows:write");
        assert_eq!(Scope::CapabilitiesRead.as_str(), "capabilities:read");
        assert_eq!(Scope::CapabilitiesWrite.as_str(), "capabilities:write");
        assert_eq!(Scope::OrganisationRead.as_str(), "organisation:read");
        assert_eq!(Scope::OrganisationWrite.as_str(), "organisation:write");
        assert_eq!(Scope::ALL.len(), 65);
    }

    #[test]
    fn name_grammar_is_one_dot() {
        for ok in ["actions.list", "members.set_role", "departments.create"] {
            assert!(validate_name(ok).is_ok(), "{ok}");
        }
        for bad in [
            "actions",
            "Actions.list",
            "actions.List",
            "actions.list.now",
            "",
            ".list",
            "actions.",
            "actions list",
        ] {
            assert!(validate_name(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn the_mcp_grammar_is_narrower_and_the_collision_is_detectable() {
        assert!(validate_mcp_name("members_set_role").is_ok());
        assert!(validate_mcp_name("cards.move").is_err());
        assert!(validate_mcp_name(&"a".repeat(65)).is_err());
        let collisions = mcp_collisions(["cards.move", "cards_move", "me.get"]);
        assert_eq!(
            collisions.get("cards_move"),
            Some(&vec!["cards.move", "cards_move"])
        );
        assert!(!collisions.contains_key("me_get"));
    }

    #[test]
    fn a_role_holds_reads_but_only_admins_hold_writes() {
        assert!(Scope::AuditRead.granted_to(Role::Viewer));
        assert!(!Scope::MembersWrite.granted_to(Role::Member));
        assert!(Scope::MembersWrite.granted_to(Role::Admin));
    }

    /// S11's authoring policy (George 2026-11-09, amended): super-admin, tenant-admin,
    /// tenant-member and the `kata_executor` synthetic tenant-admin. The last is a machine
    /// principal that holds every tenant scope an `admin` does (it creates and runs shared
    /// assets and can start projects); a viewer may read but never author.
    #[test]
    fn processes_write_is_held_by_the_human_roles_and_the_synthetic_admin() {
        for role in [Role::Admin, Role::Member, Role::KataExecutor] {
            assert!(Scope::ProcessesWrite.granted_to(role), "{role:?}");
        }
        assert!(!Scope::ProcessesWrite.granted_to(Role::Viewer));
        assert!(Scope::ProcessesRead.granted_to(Role::Viewer));
        assert!(Scope::ProcessesRead.granted_to(Role::KataExecutor));
    }

    /// The ruling that `kata_executor` is a synthetic *tenant-admin*, not a narrow executor:
    /// it must reach the same tenant-level writes an admin reaches — projects, boards, cards,
    /// governance — because its job is to create/update shared assets and run project work.
    #[test]
    fn kata_executor_holds_the_admin_scopes_and_is_not_ambient_for_a_viewer() {
        for scope in [
            Scope::ProjectsWrite,
            Scope::BoardsWrite,
            Scope::ListsWrite,
            Scope::CardsWrite,
            Scope::GovernanceWrite,
            Scope::MembersWrite,
            Scope::AuditRead,
        ] {
            assert!(scope.granted_to(Role::KataExecutor), "{scope:?}");
        }
        assert!(!Scope::CardsWrite.granted_to(Role::Viewer));
        assert!(!Scope::MembersWrite.granted_to(Role::Member));
    }

    #[test]
    fn a_member_holds_work_writes_but_not_channel_administration() {
        for scope in [
            Scope::RunsWrite,
            Scope::ApprovalsWrite,
            Scope::AndonWrite,
            Scope::MessagesWrite,
            Scope::NotificationsWrite,
        ] {
            assert!(scope.granted_to(Role::Member), "{scope:?}");
            assert!(scope.granted_to(Role::Admin), "{scope:?}");
            assert!(!scope.granted_to(Role::Viewer), "{scope:?}");
        }
        for scope in [
            Scope::RunsRead,
            Scope::ApprovalsRead,
            Scope::NotificationsRead,
        ] {
            assert!(scope.granted_to(Role::Viewer), "{scope:?}");
            assert!(scope.granted_to(Role::Member), "{scope:?}");
        }
        assert!(!Scope::ChannelsWrite.granted_to(Role::Member));
        assert!(!Scope::ChannelsWrite.granted_to(Role::Viewer));
        assert!(Scope::ChannelsWrite.granted_to(Role::Admin));
    }

    #[test]
    fn s07_reads_are_open_and_only_platform_admins_hold_the_catalogue_write() {
        for scope in [
            Scope::GovernanceRead,
            Scope::CataloguesRead,
            Scope::RequestsRead,
            Scope::SearchRead,
        ] {
            for role in Role::ALL {
                assert!(
                    scope.granted_to(role),
                    "{scope:?} must be readable by {role:?}"
                );
            }
        }
        for scope in [Scope::GovernanceWrite, Scope::RequestsWrite] {
            assert!(scope.granted_to(Role::Member), "{scope:?}");
            assert!(scope.granted_to(Role::Admin), "{scope:?}");
            assert!(!scope.granted_to(Role::Viewer), "{scope:?}");
        }
        // An agent or service principal carries member-level access: it reads the catalogue
        // vocabulary but never imports it — that is platform administration.
        assert!(!Scope::CataloguesWrite.granted_to(Role::Member));
        assert!(!Scope::CataloguesWrite.granted_to(Role::Viewer));
        assert!(Scope::CataloguesWrite.granted_to(Role::Admin));
    }

    /// S19 trap 1: `evals:approve` must never be reachable through the `admin` predicate,
    /// because that predicate includes `Role::KataExecutor` — a role held only by a machine
    /// principal. A scope a machine holds cannot be a sign-off (CONTRACT §20).
    #[test]
    fn evals_approve_is_held_by_people_only_never_by_the_machine_admin() {
        assert!(Scope::EvalsApprove.granted_to(Role::Admin));
        assert!(
            !Scope::EvalsApprove.granted_to(Role::KataExecutor),
            "kata_executor is a machine role; a machine cannot sign off"
        );
        assert!(!Scope::EvalsApprove.granted_to(Role::Member));
        assert!(!Scope::EvalsApprove.granted_to(Role::Viewer));

        // The build scopes are the member predicate, the synthetic admin included: building
        // is not signing.
        for role in [Role::Admin, Role::Member, Role::KataExecutor] {
            assert!(Scope::EvalsRead.granted_to(role), "{role:?}");
            assert!(Scope::EvalsWrite.granted_to(role), "{role:?}");
        }
        assert!(!Scope::EvalsRead.granted_to(Role::Viewer));
        assert!(!Scope::EvalsWrite.granted_to(Role::Viewer));
    }
}

// ── input validation ────────────────────────────────────────────────────────────────────

/// One field-level validation failure (the same type the problem document carries, and the same
/// type `capabilities.get` returns as `blocking[]`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FieldError {
    /// The input member that failed.
    pub field: String,
    /// A stable slug (`required`, `invalid_type`, `unknown_field`, `invalid_value`).
    pub code: String,
    /// What was wrong.
    pub detail: String,
}

/// Validate `value` against a schemars schema.
///
/// The validator reads the SAME `Schema` value the OpenAPI operation and MCP tool embed,
/// so a refusal here and the published contract cannot disagree. It deliberately covers
/// the JSON-Schema subset schemars emits for our documents: `$ref` into `$defs`,
/// `allOf` (a flattened shared envelope), `anyOf`/`oneOf` (an `Option<T>`),
/// `required`, `properties`, `additionalProperties`, `type` and `enum`. Anything outside
/// that subset is accepted rather than guessed — the typed deserialization after this is
/// the second belt.
pub fn validate_input(schema: &Value, value: &Value) -> Vec<FieldError> {
    let mut errors = Vec::new();
    validate_at(schema, schema, value, "", &mut errors);
    errors.truncate(32);
    errors
}

fn resolve<'a>(root: &'a Value, schema: &'a Value) -> &'a Value {
    let Some(reference) = schema.get("$ref").and_then(Value::as_str) else {
        return schema;
    };
    let Some(path) = reference.strip_prefix("#/") else {
        return schema;
    };
    let mut current = root;
    for segment in path.split('/') {
        let segment = segment.replace("~1", "/").replace("~0", "~");
        match current.get(&segment) {
            Some(next) => current = next,
            None => return schema,
        }
    }
    current
}

/// Merge `allOf` members into one object view (the flattened-envelope case).
fn merged(root: &Value, schema: &Value) -> Value {
    let schema = resolve(root, schema);
    let mut out = schema.clone();
    let Some(members) = schema.get("allOf").and_then(Value::as_array) else {
        return out;
    };
    let Some(target) = out.as_object_mut() else {
        return out;
    };
    target.remove("allOf");
    for member in members {
        let member = resolve(root, member);
        if let Some(object) = member.as_object() {
            for (key, value) in object {
                match key.as_str() {
                    "required" => {
                        if let (Some(existing), Some(extra)) = (
                            target.get_mut("required").and_then(Value::as_array_mut),
                            value.as_array(),
                        ) {
                            for item in extra {
                                if !existing.contains(item) {
                                    existing.push(item.clone());
                                }
                            }
                        } else {
                            target.insert("required".into(), value.clone());
                        }
                    }
                    "properties" => {
                        if let (Some(existing), Some(extra)) = (
                            target.get_mut("properties").and_then(Value::as_object_mut),
                            value.as_object(),
                        ) {
                            for (name, schema) in extra {
                                existing
                                    .entry(name.clone())
                                    .or_insert_with(|| schema.clone());
                            }
                        } else {
                            target.insert("properties".into(), value.clone());
                        }
                    }
                    _ => {
                        target.entry(key.clone()).or_insert_with(|| value.clone());
                    }
                }
            }
        }
    }
    out
}

fn join(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_owned()
    } else {
        format!("{prefix}.{name}")
    }
}

fn validate_at(
    root: &Value,
    schema: &Value,
    value: &Value,
    path: &str,
    errors: &mut Vec<FieldError>,
) {
    let schema = merged(root, schema);

    // `anyOf` / `oneOf`: accept when one branch accepts (an Option<T> is the common case).
    for keyword in ["anyOf", "oneOf"] {
        if let Some(branches) = schema.get(keyword).and_then(Value::as_array) {
            let mut any_ok = false;
            for branch in branches {
                let mut branch_errors = Vec::new();
                validate_at(root, branch, value, path, &mut branch_errors);
                if branch_errors.is_empty() {
                    any_ok = true;
                    break;
                }
            }
            if !any_ok {
                errors.push(FieldError {
                    field: if path.is_empty() {
                        "body".into()
                    } else {
                        path.to_owned()
                    },
                    code: "invalid_value".into(),
                    detail: "does not match any accepted shape".into(),
                });
            }
            return;
        }
    }

    if let Some(types) = schema.get("type") {
        let allowed: Vec<&str> = match types {
            Value::String(single) => vec![single.as_str()],
            Value::Array(items) => items.iter().filter_map(Value::as_str).collect(),
            _ => Vec::new(),
        };
        if !allowed.is_empty() && !allowed.iter().any(|allowed| type_matches(allowed, value)) {
            errors.push(FieldError {
                field: if path.is_empty() {
                    "body".into()
                } else {
                    path.to_owned()
                },
                code: "invalid_type".into(),
                detail: format!(
                    "expected {}, got {}",
                    allowed.join(" or "),
                    type_name(value)
                ),
            });
            return;
        }
    }

    if let Some(choices) = schema.get("enum").and_then(Value::as_array)
        && !choices.contains(value)
    {
        errors.push(FieldError {
            field: if path.is_empty() {
                "body".into()
            } else {
                path.to_owned()
            },
            code: "invalid_value".into(),
            detail: format!("must be one of {choices:?}"),
        });
        return;
    }

    if let Some(object) = value.as_object() {
        if let Some(required) = schema.get("required").and_then(Value::as_array) {
            for name in required.iter().filter_map(Value::as_str) {
                if !object.contains_key(name) {
                    errors.push(FieldError {
                        field: join(path, name),
                        code: "required".into(),
                        detail: "is required".into(),
                    });
                }
            }
        }
        if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
            for (name, member_schema) in properties {
                if let Some(member) = object.get(name) {
                    validate_at(root, member_schema, member, &join(path, name), errors);
                }
            }
            if schema
                .get("additionalProperties")
                .and_then(Value::as_bool)
                .is_some_and(|allowed| !allowed)
            {
                for name in object.keys() {
                    if !properties.contains_key(name) {
                        errors.push(FieldError {
                            field: join(path, name),
                            code: "unknown_field".into(),
                            detail: "is not a member of this action's input".into(),
                        });
                    }
                }
            }
        }
    }

    if let (Some(items), Some(array)) = (schema.get("items"), value.as_array()) {
        for (index, item) in array.iter().enumerate() {
            validate_at(root, items, item, &join(path, &index.to_string()), errors);
        }
    }
}

fn type_matches(expected: &str, value: &Value) -> bool {
    match expected {
        "null" => value.is_null(),
        "boolean" => value.is_boolean(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "number" => value.is_number(),
        "string" => value.is_string(),
        "array" => value.is_array(),
        "object" => value.is_object(),
        _ => true,
    }
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}
