#![forbid(unsafe_code)]
//! The **default process catalogue** (`seed/processes/**`, SPEC D29/D30): parse, validate, apply.
//!
//! The catalogue is *content*, not behaviour: the 280-process default (70 card types × the four
//! list stages) plus the named `cap.*`/`ask_fr`/`kata.open` processes. A tenant may override or
//! design its own (D29), so the loader never revisits a key that is already published — it only
//! fills the gaps a fresh tenant has.
//!
//! # The documents
//!
//! - `types/<card_type>.yaml` names its `card_type`, its `capability`, its one process `agent`,
//!   its `sops` and **exactly four** `processes` (one per list stage).
//! - `named.yaml` (when present) defines the `cap.*` processes with the same step schema but no
//!   card type and no list stage.
//! - `maps.yaml` (when present) defines the `process_map` documents: a `{schema, nodes, edges}`
//!   phase DAG per project type, validated against the published process keys, the tenant's
//!   capabilities, that project type's `meta.phases` and the `milestone_kind` codes.
//!
//! All are embedded at compile time with [`include_dir`], so a deployment carries the catalogue
//! even when `seed/` is absent from the image — the same rule `seed/catalogue.json` and
//! `seed/capability.json` already follow.
//!
//! # Validation (`parse_*`)
//!
//! A malformed file is refused **before any write**, with an error naming the `file`, the
//! `process` and (for a step) the `step` position:
//!
//! 1. a type file has exactly one process per stage, `ready | doing | qa | done`, no stage twice
//!    and no process key twice;
//! 2. every step names an SOP defined in the same file (named.yaml's own SOPs for a `cap.*`
//!    process), and every `process_agent` names an agent the same file defines;
//! 3. the step positions are contiguous `1..=n`;
//! 4. an SOP, agent or process key appears in **one** file across the whole catalogue — keys are
//!    tenant-unique, so a second definition would be silently skipped at apply.
//!
//! # Apply (`apply_all`)
//!
//! One transaction per tenant, and every write is a look-up-first so a second run inserts
//! nothing and creates no version 2:
//!
//! 1. the file's `capability:` resolves to a `capability` row (absent → the typed error names the
//!    code; the tenant has not had `migrate --seed` yet);
//! 2. each agent is found by `(tenant_id, key)` or inserted `status = 'active'`,
//!    `role = 'process_agent'`, `process_key = key` and `prompt_ref = <recipe>@1`;
//! 3. the card type is found by `(tenant_id, code)` or created with the label
//!    `seed/catalogue.json` gives it (falling back to the code);
//! 4. each SOP is skipped when published, else created as a draft and published;
//! 5. each process is skipped when published, else created as a draft, given its steps (the SOP,
//!    agent and capability keys resolved to uuids) and published.

use std::collections::{HashMap, HashSet};

use conveyor_domain::process::StepInput;
use conveyor_domain::process_map::{MapDoc, MapError, MapFacts};
use conveyor_domain::{CatalogueKind, Code, CodeError, TenantId};
use include_dir::{Dir, include_dir};
use serde::Deserialize;
use serde_json::Value;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::begin_scoped;
use crate::capability::{self, CapabilityStoreError};
use crate::catalogue::{self, CatalogueStoreError};
use crate::procedure::{self, NewSop, SopStoreError};
use crate::process::{self, NewProcess, ProcessStoreError};
use crate::process_map::{self, MapStoreError, NewMap};
use crate::seed::{SeedError, catalogue_export};

/// `seed/processes/` itself — embedded at compile time (the loader is the only reader).
static CATALOGUE_DIR: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/../../seed/processes");

/// The four list stages a type file must cover, in ladder order.
pub const LIST_STAGES: [&str; 4] = ["ready", "doing", "qa", "done"];

// ── the documents ─────────────────────────────────────────────────────────────────────────────

/// One `capability_agent` a file defines.
#[derive(Clone, Debug, Deserialize)]
pub struct AgentSpec {
    /// The tenant-unique agent key (`process_agent:` steps name it).
    pub key: String,
    /// `process_agent | tool_agent`; the file's own agents are process agents.
    #[serde(default = "process_agent_role")]
    pub role: String,
    /// The process key the agent drives; defaults to [`AgentSpec::key`] (the convention).
    #[serde(default)]
    pub process_key: Option<String>,
    /// The capability code the agent hangs off; defaults to the file's `capability:`.
    #[serde(default)]
    pub capability: Option<String>,
    /// The prompt recipe (`agent`); [`AgentSpec::prompt_ref`] adds the `@1` version.
    #[serde(default)]
    pub prompt: Option<String>,
    /// The model registry reference.
    pub model: String,
    /// The toolset the executor may use (`text[]`; empty today).
    #[serde(default)]
    pub toolset: Vec<String>,
    /// The steps the executor may invoke.
    #[serde(default)]
    pub allowed_actions: Vec<String>,
    /// The loop's iteration ceiling (the database allows `1..=20`).
    #[serde(default = "default_max_iterations")]
    pub max_iterations: i32,
    /// The loop's wall-clock ceiling (the database allows `1..=300`).
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: i32,
}

impl AgentSpec {
    /// The `process_key` the row carries: the spec's own, else the agent key.
    pub fn process_key(&self) -> &str {
        self.process_key.as_deref().unwrap_or(&self.key)
    }

    /// The prompt catalogue reference: `<recipe>@1` when the recipe names no version.
    pub fn prompt_ref(&self) -> Option<String> {
        self.prompt.as_ref().map(|recipe| {
            if recipe.contains('@') {
                recipe.clone()
            } else {
                format!("{recipe}@1")
            }
        })
    }
}

/// One SOP (`procedure`) a file defines — the judge's contract for the steps that name it.
#[derive(Clone, Debug, Deserialize)]
pub struct SopSpec {
    /// The tenant-unique SOP key (`sop:` steps name it).
    pub key: String,
    /// What a person reads.
    pub title: String,
    /// Free text.
    #[serde(default)]
    pub description: String,
    /// The definition of ready (`jsonb`).
    #[serde(default = "empty_object")]
    pub definition_of_ready: Value,
    /// The definition of done (`jsonb`).
    #[serde(default = "empty_object")]
    pub definition_of_done: Value,
    /// The acceptance criteria the judge reads (`jsonb`).
    #[serde(default = "empty_array")]
    pub acceptance_criteria: Value,
    /// `{kind: laya|eval|checker, ...}` (`jsonb`).
    #[serde(default = "empty_object")]
    pub inspection: Value,
    /// The declared user-facing outputs (`jsonb`).
    #[serde(default = "empty_array")]
    pub outputs: Value,
}

/// One step of one process.
#[derive(Clone, Debug, Deserialize)]
pub struct StepSpec {
    /// 1-based order; must be contiguous `1..=n` across the process.
    pub position: i32,
    /// `agent | computed | gate | call`; absent = inferred by the store.
    #[serde(default)]
    pub kind: Option<String>,
    /// `process_agent | tool_agent | system | human`; absent = the kind's default.
    #[serde(default)]
    pub actor: Option<String>,
    /// The registry action a `computed` step invokes; `agent.run` on an `agent` step; the loader
    /// fills the agent primitive when it is omitted, exactly as the API path does.
    #[serde(default)]
    pub action: String,
    /// The agent key that runs an `agent` step (must be defined in the same file).
    #[serde(default)]
    pub process_agent: Option<String>,
    /// The SOP key this step is judged against (must be defined in the same file).
    pub sop: String,
    /// The catalogue gate code a `gate` step parks on.
    #[serde(default)]
    pub gate_kind: Option<String>,
    /// The published process or workflow a `call` step starts.
    #[serde(default)]
    pub child_process_key: Option<String>,
    /// `fail | continue | retry`.
    #[serde(default)]
    pub on_result: Option<String>,
    /// The retry cap for this step.
    #[serde(default)]
    pub max_attempts: Option<i32>,
    /// The input template (`jsonb`).
    #[serde(default = "empty_object")]
    pub input_template: Value,
    /// The dot-paths into this step's output later steps may reference.
    #[serde(default)]
    pub outputs: Vec<String>,
}

/// One process: four per type file, one per list stage.
#[derive(Clone, Debug, Deserialize)]
pub struct ProcessSpec {
    /// The tenant-unique process key (the version family).
    pub key: String,
    /// The catalogue stage this process drives (`ready | doing | qa | done`); `null` for a
    /// named process.
    #[serde(default)]
    pub list: Option<String>,
    /// What a person reads.
    pub title: String,
    /// Free text.
    #[serde(default)]
    pub description: String,
    /// A capability override; defaults to the file's `capability:`.
    #[serde(default)]
    pub capability: Option<String>,
    /// The user-facing deliverables; the file's `outputs:` is the fallback.
    #[serde(default)]
    pub outputs: Vec<String>,
    /// The ordered steps.
    #[serde(default)]
    pub steps: Vec<StepSpec>,
}

/// The raw YAML document of one type file or `named.yaml`.
#[derive(Clone, Debug, Default, Deserialize)]
struct Document {
    #[serde(default)]
    card_type: Option<String>,
    #[serde(default)]
    capability: Option<String>,
    #[serde(default)]
    outputs: Vec<String>,
    #[serde(default)]
    agent: Option<AgentSpec>,
    #[serde(default)]
    agents: Vec<AgentSpec>,
    #[serde(default)]
    sops: Vec<SopSpec>,
    #[serde(default)]
    processes: Vec<ProcessSpec>,
}

/// One parsed and validated `types/<card_type>.yaml`.
#[derive(Clone, Debug)]
pub struct TypeFile {
    /// The embedded path (`types/<card_type>.yaml`), as the error messages name it.
    pub file: String,
    /// The catalogue card-type code the four processes belong to.
    pub card_type: String,
    /// The capability code the file's agents and steps hang off.
    pub capability: String,
    /// The type's declared outputs; a process that declares none inherits these.
    pub outputs: Vec<String>,
    /// The agents the file defines.
    pub agents: Vec<AgentSpec>,
    /// The SOPs the file defines.
    pub sops: Vec<SopSpec>,
    /// Exactly four processes, one per list stage.
    pub processes: Vec<ProcessSpec>,
}

/// One parsed and validated `named.yaml`.
#[derive(Clone, Debug)]
pub struct NamedFile {
    /// The embedded path (`named.yaml`).
    pub file: String,
    /// A default capability for the file's agents and processes.
    pub capability: Option<String>,
    /// The agents the file defines.
    pub agents: Vec<AgentSpec>,
    /// The SOPs the file defines.
    pub sops: Vec<SopSpec>,
    /// The named processes (`cap.*`, `ask_fr`, `kata.open`).
    pub processes: Vec<ProcessSpec>,
}

/// One parsed `maps.yaml` entry: a `process_map` document bound to a project type.
#[derive(Clone, Debug, Deserialize)]
pub struct MapSpec {
    /// The tenant-unique map key (the version family).
    pub key: String,
    /// What a person reads.
    #[serde(default)]
    pub title: String,
    /// The `catalogue_entry(kind='project_type')` code the map is drawn for.
    pub project_type: String,
    /// The `{schema, nodes, edges}` document.
    pub definition: MapDoc,
}

/// The raw `maps.yaml` document.
#[derive(Clone, Debug, Deserialize)]
struct MapsDocument {
    version: u32,
    maps: Vec<MapSpec>,
}

/// The whole embedded catalogue, parsed and validated.
#[derive(Clone, Debug, Default)]
pub struct ProcessCatalogue {
    /// The `types/*.yaml` documents, in path order.
    pub types: Vec<TypeFile>,
    /// `named.yaml`, when the directory carries one.
    pub named: Option<NamedFile>,
    /// `maps.yaml`, when the directory carries one.
    pub maps: Vec<MapSpec>,
}

// ── errors ────────────────────────────────────────────────────────────────────────────────────

/// Why the default process catalogue was refused.
///
/// Every validation error names the `file` it came from and, where the violation is inside a
/// process, the `process` key and the `step` position — a message that says only "bad YAML" sends
/// its reader back to a 280-file haystack.
#[derive(Debug, thiserror::Error)]
pub enum ProcessSeedError {
    /// The file is not valid YAML, or does not match the documented schema.
    #[error("seed file {file}: {source}")]
    Parse {
        /// The embedded path.
        file: String,
        /// The YAML error.
        #[source]
        source: serde_yaml::Error,
    },

    /// An embedded file is not UTF-8 (a build-time constant, so this is a packaging error).
    #[error("seed file {file}: the embedded file is not valid UTF-8")]
    NotUtf8 {
        /// The embedded path.
        file: String,
    },

    /// A type file does not declare the card type its processes belong to.
    #[error("seed file {file}: a type file must name its card_type")]
    MissingCardTypeName {
        /// The embedded path.
        file: String,
    },

    /// A caller asked for a card type no embedded type file declares.
    #[error("no embedded seed file declares card type {card_type:?}")]
    UnknownCardType {
        /// The requested code.
        card_type: String,
    },

    /// `maps.yaml` declares a schema version this loader does not implement.
    #[error("seed file {file}: maps version {version} is not supported (expected 1)")]
    UnsupportedMapsVersion {
        /// The embedded path.
        file: String,
        /// The declared version.
        version: u32,
    },

    /// A step's kind and its action/reference fields disagree — the shape only the executor
    /// knows how to run. The same rule `validate_publish` applies, caught at parse time.
    #[error("seed file {file}: process {process:?} step {step}: {detail}")]
    BadStepShape {
        /// The embedded path.
        file: String,
        /// The process key.
        process: String,
        /// The 1-based position.
        step: i32,
        /// What a step of that kind must and must not name.
        detail: String,
    },

    /// A `call` step names a process or workflow this tenant has not published (during apply, that
    /// usually means the target appears LATER in the file: a call target must be published first).
    #[error(
        "seed file {file}: process {process:?} step {step}: {key:?} is neither a published \
         process nor a published workflow"
    )]
    UnknownCallTarget {
        /// The embedded path.
        file: String,
        /// The process key.
        process: String,
        /// The 1-based position.
        step: i32,
        /// The target the step named.
        key: String,
    },

    /// Two maps in `maps.yaml` share a key — keys are tenant-unique.
    #[error("seed file {file}: map key {map:?} is defined twice")]
    DuplicateMap {
        /// The embedded path.
        file: String,
        /// The repeated key.
        map: String,
    },

    /// A map names a `project_type` the catalogue code type refuses.
    #[error("seed file {file}: process map {map:?}: project type {project_type:?}: {source}")]
    InvalidMapProjectType {
        /// The embedded path.
        file: String,
        /// The map key.
        map: String,
        /// The code as written.
        project_type: String,
        /// The code parser's refusal.
        #[source]
        source: CodeError,
    },

    /// A map names a project type the tenant's catalogue does not have.
    #[error(
        "seed file {file}: process map {map:?}: project type {project_type:?} does not exist in \
         this tenant (run `migrate --seed`)"
    )]
    MissingProjectType {
        /// The embedded path.
        file: String,
        /// The map key.
        map: String,
        /// The missing code.
        project_type: String,
    },

    /// A map document fails the domain validator — never relaxed, never partially applied.
    #[error("seed file {file}: process map {map:?} does not validate: {errors:?}")]
    InvalidMap {
        /// The embedded path.
        file: String,
        /// The map key.
        map: String,
        /// Every refusal the validator reported.
        errors: Vec<MapError>,
    },

    /// The map document could not be rendered to the stored `jsonb`.
    #[error("seed file {file}: process map {map:?}: {source}")]
    MapDefinition {
        /// The embedded path.
        file: String,
        /// The map key.
        map: String,
        /// The serializer's refusal.
        #[source]
        source: serde_json::Error,
    },

    /// The process-map store refused a read or write.
    #[error("seed file {file}: process map {map:?}: {source}")]
    ProcessMap {
        /// The embedded path.
        file: String,
        /// The map key.
        map: String,
        /// The store's refusal.
        #[source]
        source: MapStoreError,
    },

    /// The catalogue store refused a lookup.
    #[error(transparent)]
    CatalogueStore(#[from] CatalogueStoreError),

    /// A process (or the file itself) names no capability to hang the agent off.
    #[error("seed file {file}: {owner} names no capability and the file declares none")]
    MissingCapabilityName {
        /// The embedded path.
        file: String,
        /// What is missing the capability (`the type file`, `process "x"`, `agent "y"`).
        owner: String,
    },

    /// A type file does not cover one of the four list stages with exactly one process.
    #[error(
        "seed file {file}: a type file must define exactly four processes (ready, doing, qa, done); \
         missing {missing:?}"
    )]
    MissingListStage {
        /// The embedded path.
        file: String,
        /// The stages with no process.
        missing: Vec<String>,
    },

    /// A process names a `list` outside the four catalogue stages.
    #[error("seed file {file}: process {process:?} names unknown list stage {list:?}")]
    UnknownListStage {
        /// The embedded path.
        file: String,
        /// The process key.
        process: String,
        /// The offending stage.
        list: String,
    },

    /// Two processes in one file drive the same list stage.
    #[error("seed file {file}: process {process:?} repeats list stage {list:?}")]
    DuplicateList {
        /// The embedded path.
        file: String,
        /// The second process key for the stage.
        process: String,
        /// The repeated stage.
        list: String,
    },

    /// Two processes in one file share a key.
    #[error("seed file {file}: process key {process:?} is defined twice")]
    DuplicateProcess {
        /// The embedded path.
        file: String,
        /// The repeated key.
        process: String,
    },

    /// Two SOPs in one file share a key — the second would be silently skipped at apply.
    #[error("seed file {file}: SOP key {sop:?} is defined twice")]
    DuplicateSop {
        /// The embedded path.
        file: String,
        /// The repeated key.
        sop: String,
    },

    /// Two agents in one file share a key — the second would be silently skipped at apply.
    #[error("seed file {file}: agent key {agent:?} is defined twice")]
    DuplicateAgent {
        /// The embedded path.
        file: String,
        /// The repeated key.
        agent: String,
    },

    /// A step references an SOP the file does not define.
    #[error(
        "seed file {file}: process {process:?} step {step}: SOP {sop:?} is not defined in this file"
    )]
    UnknownSop {
        /// The embedded path.
        file: String,
        /// The process key.
        process: String,
        /// The step position.
        step: i32,
        /// The missing SOP key.
        sop: String,
    },

    /// A step names a process agent the file does not define.
    #[error(
        "seed file {file}: process {process:?} step {step}: process agent {agent:?} is not defined \
         in this file"
    )]
    UnknownAgent {
        /// The embedded path.
        file: String,
        /// The process key.
        process: String,
        /// The step position.
        step: i32,
        /// The missing agent key.
        agent: String,
    },

    /// A process's step positions are not contiguous `1..=n`.
    #[error(
        "seed file {file}: process {process:?}: step positions must be contiguous 1..={expected}; \
         found {actual:?}"
    )]
    PositionsNotContiguous {
        /// The embedded path.
        file: String,
        /// The process key.
        process: String,
        /// How many steps the process has.
        expected: i32,
        /// The positions as written, sorted.
        actual: Vec<i32>,
    },

    /// The tenant has no `capability` row with the code a file names.
    #[error(
        "seed file {file}: capability {code:?} does not exist in this tenant (run `migrate --seed`)"
    )]
    MissingCapability {
        /// The embedded path.
        file: String,
        /// The capability code.
        code: String,
    },

    /// A step names an SOP that is not published in the tenant when the process is written.
    #[error("seed file {file}: process {process:?} step {step}: SOP {sop:?} is not published")]
    MissingSop {
        /// The embedded path.
        file: String,
        /// The process key.
        process: String,
        /// The step position.
        step: i32,
        /// The SOP key.
        sop: String,
    },

    /// A step names a process agent that is not in the tenant when the process is written.
    #[error(
        "seed file {file}: process {process:?} step {step}: process agent {agent:?} is missing"
    )]
    MissingProcessAgent {
        /// The embedded path.
        file: String,
        /// The process key.
        process: String,
        /// The step position.
        step: i32,
        /// The agent key.
        agent: String,
    },

    /// The procedure store refused an SOP write.
    #[error("seed file {file}: SOP {sop:?}: {source}")]
    Procedure {
        /// The embedded path.
        file: String,
        /// The SOP key.
        sop: String,
        /// The store's refusal.
        #[source]
        source: SopStoreError,
    },

    /// The process store refused a process write.
    #[error("seed file {file}: process {process:?}: {source}")]
    Process {
        /// The embedded path.
        file: String,
        /// The process key.
        process: String,
        /// The store's refusal.
        #[source]
        source: ProcessStoreError,
    },

    /// The capability lookup itself failed.
    #[error(transparent)]
    Capability(#[from] CapabilityStoreError),

    /// The embedded `seed/catalogue.json` (the card-type labels) could not be read.
    #[error("seed/processes needs the catalogue export for its card-type labels: {0}")]
    Catalogue(#[from] SeedError),

    /// The database refused a statement the loader issues itself.
    #[error("seed/processes: {0}")]
    Database(#[from] sqlx::Error),
}

/// What one [`apply_all`] created. Every field counts **inserted** rows, so a second run returns
/// the all-zero default — that is the idempotency assertion.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SeedCounts {
    /// `card_type` rows created.
    pub card_types: usize,
    /// `capability_agent` rows created.
    pub agents: usize,
    /// `procedure` (SOP) rows created and published.
    pub sops: usize,
    /// `process` rows created and published.
    pub processes: usize,
    /// `process_map` rows created and published.
    pub maps: usize,
}

// ── parsing and validation ───────────────────────────────────────────────────────────────────

fn process_agent_role() -> String {
    "process_agent".to_owned()
}

fn default_max_iterations() -> i32 {
    12
}

fn default_timeout_secs() -> i32 {
    120
}

fn empty_object() -> Value {
    Value::Object(serde_json::Map::new())
}

fn empty_array() -> Value {
    Value::Array(Vec::new())
}

/// The agents a document defines: the singular `agent:` plus any `agents:` entries.
fn agents_of(document: &Document) -> Vec<AgentSpec> {
    let mut agents = Vec::new();
    if let Some(agent) = &document.agent {
        agents.push(agent.clone());
    }
    agents.extend(document.agents.iter().cloned());
    agents
}

fn check_duplicate_sops(file: &str, sops: &[SopSpec]) -> Result<(), ProcessSeedError> {
    let mut seen = HashSet::new();
    for sop in sops {
        if !seen.insert(sop.key.as_str()) {
            return Err(ProcessSeedError::DuplicateSop {
                file: file.to_owned(),
                sop: sop.key.clone(),
            });
        }
    }
    Ok(())
}

fn check_duplicate_agents(file: &str, agents: &[AgentSpec]) -> Result<(), ProcessSeedError> {
    let mut seen = HashSet::new();
    for agent in agents {
        if !seen.insert(agent.key.as_str()) {
            return Err(ProcessSeedError::DuplicateAgent {
                file: file.to_owned(),
                agent: agent.key.clone(),
            });
        }
    }
    Ok(())
}

/// The rules every file shares, plus the per-process rules (steps, references, positions).
///
/// `four_stages` is what makes a type file a type file: exactly one process per list stage.
fn validate_processes(
    file: &str,
    document: &Document,
    sops: &[SopSpec],
    agents: &[AgentSpec],
    four_stages: bool,
) -> Result<(), ProcessSeedError> {
    let mut keys = HashSet::new();
    for process in &document.processes {
        if !keys.insert(process.key.as_str()) {
            return Err(ProcessSeedError::DuplicateProcess {
                file: file.to_owned(),
                process: process.key.clone(),
            });
        }
        if process
            .capability
            .as_deref()
            .or(document.capability.as_deref())
            .is_none()
        {
            return Err(ProcessSeedError::MissingCapabilityName {
                file: file.to_owned(),
                owner: format!("process {:?}", process.key),
            });
        }
    }

    let mut stages: HashMap<&str, &str> = HashMap::new();
    for process in &document.processes {
        let Some(list) = process.list.as_deref() else {
            continue;
        };
        if !LIST_STAGES.contains(&list) {
            return Err(ProcessSeedError::UnknownListStage {
                file: file.to_owned(),
                process: process.key.clone(),
                list: list.to_owned(),
            });
        }
        if stages.insert(list, process.key.as_str()).is_some() {
            return Err(ProcessSeedError::DuplicateList {
                file: file.to_owned(),
                process: process.key.clone(),
                list: list.to_owned(),
            });
        }
    }
    if four_stages {
        let missing: Vec<String> = LIST_STAGES
            .iter()
            .filter(|stage| !stages.contains_key(**stage))
            .map(|stage| (*stage).to_owned())
            .collect();
        if !missing.is_empty() {
            return Err(ProcessSeedError::MissingListStage {
                file: file.to_owned(),
                missing,
            });
        }
    }

    let sop_keys: HashSet<&str> = sops.iter().map(|sop| sop.key.as_str()).collect();
    let agent_keys: HashSet<&str> = agents.iter().map(|agent| agent.key.as_str()).collect();
    for process in &document.processes {
        let mut positions: Vec<i32> = process.steps.iter().map(|step| step.position).collect();
        positions.sort_unstable();
        let contiguous = positions
            .iter()
            .enumerate()
            .all(|(index, position)| *position == i32::try_from(index + 1).unwrap_or(i32::MAX));
        if !contiguous {
            return Err(ProcessSeedError::PositionsNotContiguous {
                file: file.to_owned(),
                process: process.key.clone(),
                expected: i32::try_from(positions.len()).unwrap_or(i32::MAX),
                actual: positions,
            });
        }
        for step in &process.steps {
            if !sop_keys.contains(step.sop.as_str()) {
                return Err(ProcessSeedError::UnknownSop {
                    file: file.to_owned(),
                    process: process.key.clone(),
                    step: step.position,
                    sop: step.sop.clone(),
                });
            }
            if let Some(agent) = step.process_agent.as_deref()
                && !agent_keys.contains(agent)
            {
                return Err(ProcessSeedError::UnknownAgent {
                    file: file.to_owned(),
                    process: process.key.clone(),
                    step: step.position,
                    agent: agent.to_owned(),
                });
            }
            let detail = step_shape_detail(step);
            if let Some(detail) = detail {
                return Err(ProcessSeedError::BadStepShape {
                    file: file.to_owned(),
                    process: process.key.clone(),
                    step: step.position,
                    detail,
                });
            }
        }
    }
    Ok(())
}

/// One step's kind and the fields it may name, mirroring `StepInput::kind()`'s inference.
///
/// This is the loader's half of the one step contract: the registry-dependent refusals
/// (`validate_publish`'s unknown action, gate code and call target) cannot run here — the store
/// has no registry — but the shape the executor dispatches on can, and it is the shape the
/// catalogue silently got wrong before 2026-11-10.
fn step_shape_detail(step: &StepSpec) -> Option<String> {
    let kind = match step.kind.as_deref() {
        Some(kind) => kind,
        // `StepInput::kind()`: a gate is inferred from its code, everything else is computed.
        None if step.gate_kind.is_some() => "gate",
        None => "computed",
    };
    let action = step.action.trim();
    match kind {
        "agent" => {
            if !action.is_empty() && action != conveyor_domain::process::AGENT_STEP_ACTION {
                return Some(format!(
                    "an agent step runs `{}`, not `{action}`",
                    conveyor_domain::process::AGENT_STEP_ACTION
                ));
            }
            if step.process_agent.is_none() {
                return Some("an agent step must name its process_agent".to_owned());
            }
            if step.gate_kind.is_some() || step.child_process_key.is_some() {
                return Some(
                    "an agent step names no gate_kind and no child_process_key".to_owned(),
                );
            }
        }
        "computed" => {
            if action.is_empty() {
                return Some("a computed step must name its action".to_owned());
            }
            if action == conveyor_domain::process::AGENT_STEP_ACTION {
                return Some(format!(
                    "`{}` is the agent step's primitive; a computed step cannot name it",
                    conveyor_domain::process::AGENT_STEP_ACTION
                ));
            }
            if step.gate_kind.is_some() || step.child_process_key.is_some() {
                return Some(
                    "a computed step names no gate_kind and no child_process_key".to_owned(),
                );
            }
        }
        "gate" => {
            if step.gate_kind.is_none() {
                return Some("a gate step must name its gate_kind".to_owned());
            }
            if !action.is_empty() || step.child_process_key.is_some() {
                return Some("a gate step names no action and no child_process_key".to_owned());
            }
        }
        "call" => {
            if step.child_process_key.is_none() {
                return Some("a call step must name its child_process_key".to_owned());
            }
            if !action.is_empty() || step.gate_kind.is_some() {
                return Some("a call step names no action and no gate_kind".to_owned());
            }
        }
        other => return Some(format!("unknown step kind {other:?}")),
    }
    None
}

/// Parse and validate one `types/*.yaml` document.
///
/// Public as the malformed-file seam: the catalogue itself is a build-time constant, so a test
/// proves the refusal with a document it supplies rather than by writing into `seed/`.
pub fn parse_type_file(file: &str, yaml: &str) -> Result<TypeFile, ProcessSeedError> {
    let document: Document =
        serde_yaml::from_str(yaml).map_err(|source| ProcessSeedError::Parse {
            file: file.to_owned(),
            source,
        })?;
    let card_type =
        document
            .card_type
            .clone()
            .ok_or_else(|| ProcessSeedError::MissingCardTypeName {
                file: file.to_owned(),
            })?;
    let capability =
        document
            .capability
            .clone()
            .ok_or_else(|| ProcessSeedError::MissingCapabilityName {
                file: file.to_owned(),
                owner: "the type file".to_owned(),
            })?;
    let agents = agents_of(&document);
    check_duplicate_agents(file, &agents)?;
    check_duplicate_sops(file, &document.sops)?;
    validate_processes(file, &document, &document.sops, &agents, true)?;
    Ok(TypeFile {
        file: file.to_owned(),
        card_type,
        capability,
        outputs: document.outputs.clone(),
        agents,
        sops: document.sops.clone(),
        processes: document.processes.clone(),
    })
}

/// Parse and validate `named.yaml` (the `cap.*` processes).
///
/// Same schema as a type file, minus the card type and the four-stage rule: `list` may be `null`
/// and a process may name its own `capability`.
pub fn parse_named_file(file: &str, yaml: &str) -> Result<NamedFile, ProcessSeedError> {
    let document: Document =
        serde_yaml::from_str(yaml).map_err(|source| ProcessSeedError::Parse {
            file: file.to_owned(),
            source,
        })?;
    let agents = agents_of(&document);
    for agent in &agents {
        if agent.capability.is_none() && document.capability.is_none() {
            return Err(ProcessSeedError::MissingCapabilityName {
                file: file.to_owned(),
                owner: format!("agent {:?}", agent.key),
            });
        }
    }
    check_duplicate_agents(file, &agents)?;
    check_duplicate_sops(file, &document.sops)?;
    validate_processes(file, &document, &document.sops, &agents, false)?;
    Ok(NamedFile {
        file: file.to_owned(),
        capability: document.capability.clone(),
        agents,
        sops: document.sops.clone(),
        processes: document.processes.clone(),
    })
}

/// Parse `maps.yaml`: `version: 1` and the tenant-unique map keys.
///
/// The documents themselves are validated at apply time, when the project type's phases and the
/// tenant's published processes and capabilities are known — a parse-time validator would have no
/// facts to resolve against.
pub fn parse_maps_file(file: &str, yaml: &str) -> Result<Vec<MapSpec>, ProcessSeedError> {
    let document: MapsDocument =
        serde_yaml::from_str(yaml).map_err(|source| ProcessSeedError::Parse {
            file: file.to_owned(),
            source,
        })?;
    if document.version != 1 {
        return Err(ProcessSeedError::UnsupportedMapsVersion {
            file: file.to_owned(),
            version: document.version,
        });
    }
    let mut keys = HashSet::new();
    for map in &document.maps {
        if !keys.insert(map.key.as_str()) {
            return Err(ProcessSeedError::DuplicateMap {
                file: file.to_owned(),
                map: map.key.clone(),
            });
        }
    }
    Ok(document.maps)
}

/// The embedded catalogue, parsed and validated. An empty catalogue is not an error — a tenant
/// with no `seed/processes` content simply has nothing to apply.
pub fn catalogue() -> Result<ProcessCatalogue, ProcessSeedError> {
    let mut types: Vec<_> = match CATALOGUE_DIR.get_dir("types") {
        Some(directory) => directory.files().collect(),
        None => Vec::new(),
    };
    types.sort_by(|left, right| left.path().cmp(right.path()));

    let mut parsed = Vec::with_capacity(types.len());
    for file in types {
        let path = path_of(file.path());
        let yaml = file
            .contents_utf8()
            .ok_or_else(|| ProcessSeedError::NotUtf8 { file: path.clone() })?;
        parsed.push(parse_type_file(&path, yaml)?);
    }

    let named = match CATALOGUE_DIR.get_file("named.yaml") {
        Some(file) => {
            let path = path_of(file.path());
            let yaml = file
                .contents_utf8()
                .ok_or_else(|| ProcessSeedError::NotUtf8 { file: path.clone() })?;
            Some(parse_named_file(&path, yaml)?)
        }
        None => None,
    };

    let maps = match CATALOGUE_DIR.get_file("maps.yaml") {
        Some(file) => {
            let path = path_of(file.path());
            let yaml = file
                .contents_utf8()
                .ok_or_else(|| ProcessSeedError::NotUtf8 { file: path.clone() })?;
            parse_maps_file(&path, yaml)?
        }
        None => Vec::new(),
    };

    let catalogue = ProcessCatalogue {
        types: parsed,
        named,
        maps,
    };
    check_catalogue_uniqueness(&catalogue)?;
    Ok(catalogue)
}

/// SOP, agent and process keys are tenant-unique, so a key defined by two files would be applied
/// once and silently skipped for the second — the second file's content would never reach the
/// database. Refuse it up front, naming the second file.
fn check_catalogue_uniqueness(catalogue: &ProcessCatalogue) -> Result<(), ProcessSeedError> {
    let mut sops = HashSet::new();
    let mut agents = HashSet::new();
    let mut processes = HashSet::new();
    for file in &catalogue.types {
        record_sops(&file.file, &file.sops, &mut sops)?;
        record_agents(&file.file, &file.agents, &mut agents)?;
        record_processes(&file.file, &file.processes, &mut processes)?;
    }
    if let Some(named) = &catalogue.named {
        record_sops(&named.file, &named.sops, &mut sops)?;
        record_agents(&named.file, &named.agents, &mut agents)?;
        record_processes(&named.file, &named.processes, &mut processes)?;
    }
    Ok(())
}

fn record_sops(
    file: &str,
    sops: &[SopSpec],
    seen: &mut HashSet<String>,
) -> Result<(), ProcessSeedError> {
    for sop in sops {
        if !seen.insert(sop.key.clone()) {
            return Err(ProcessSeedError::DuplicateSop {
                file: file.to_owned(),
                sop: sop.key.clone(),
            });
        }
    }
    Ok(())
}

fn record_agents(
    file: &str,
    agents: &[AgentSpec],
    seen: &mut HashSet<String>,
) -> Result<(), ProcessSeedError> {
    for agent in agents {
        if !seen.insert(agent.key.clone()) {
            return Err(ProcessSeedError::DuplicateAgent {
                file: file.to_owned(),
                agent: agent.key.clone(),
            });
        }
    }
    Ok(())
}

fn record_processes(
    file: &str,
    processes: &[ProcessSpec],
    seen: &mut HashSet<String>,
) -> Result<(), ProcessSeedError> {
    for process in processes {
        if !seen.insert(process.key.clone()) {
            return Err(ProcessSeedError::DuplicateProcess {
                file: file.to_owned(),
                process: process.key.clone(),
            });
        }
    }
    Ok(())
}

fn path_of(path: &std::path::Path) -> String {
    path.to_string_lossy().into_owned()
}

// ── apply ────────────────────────────────────────────────────────────────────────────────────

/// The rows resolved during one [`apply_all`], cached so a 280-process catalogue does not issue a
/// look-up per step.
struct State {
    /// The card-type labels from `seed/catalogue.json`.
    labels: HashMap<String, String>,
    /// Capability code → id.
    capabilities: HashMap<String, Uuid>,
    /// Agent key → id.
    agents: HashMap<String, Uuid>,
    /// SOP key → published row id.
    sops: HashMap<String, Uuid>,
    /// What has been created so far.
    counts: SeedCounts,
}

/// What one file's processes hang off: the file path (for errors), the default capability and the
/// card type / outputs a type file supplies.
struct FileContext<'a> {
    file: &'a str,
    capability: Option<&'a str>,
    card_type_id: Option<Uuid>,
    outputs: &'a [String],
}

impl FileContext<'_> {
    fn capability_code(&self, process: &ProcessSpec) -> Result<String, ProcessSeedError> {
        process
            .capability
            .clone()
            .or_else(|| self.capability.map(str::to_owned))
            .ok_or_else(|| ProcessSeedError::MissingCapabilityName {
                file: self.file.to_owned(),
                owner: format!("process {:?}", process.key),
            })
    }
}

/// Whether the tenant has any capability at all.
///
/// The loader's `capability:` reference is strict — a missing one is a refusal naming the code.
/// A caller that runs earlier than `migrate --seed` (the dev seed runs before the catalogue
/// import) uses this to skip a tenant that has not been catalogue-seeded yet instead of failing.
pub async fn has_capability(pool: &PgPool, tenant: TenantId) -> Result<bool, sqlx::Error> {
    let mut tx = begin_scoped(pool, tenant).await?;
    let present: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM capability WHERE tenant_id = $1)")
            .bind(tenant.as_uuid())
            .fetch_one(&mut *tx)
            .await?;
    tx.commit().await?;
    Ok(present)
}

/// Apply the embedded catalogue to one tenant: one transaction, look-up-first, published keys
/// untouched. A second run returns the all-zero [`SeedCounts`] and creates no version 2.
pub async fn apply_all(pool: &PgPool, tenant: TenantId) -> Result<SeedCounts, ProcessSeedError> {
    let catalogue = catalogue()?;
    let mut state = State {
        labels: card_type_labels()?,
        capabilities: HashMap::new(),
        agents: HashMap::new(),
        sops: HashMap::new(),
        counts: SeedCounts::default(),
    };
    let mut tx = begin_scoped(pool, tenant).await?;
    for file in &catalogue.types {
        apply_type_file(&mut tx, tenant, file, &mut state).await?;
    }
    if let Some(named) = &catalogue.named {
        apply_named_file(&mut tx, tenant, named, &mut state).await?;
    }
    apply_maps(&mut tx, tenant, "maps.yaml", &catalogue.maps, &mut state).await?;
    tx.commit().await?;
    Ok(state.counts)
}

/// Apply ONE embedded type file to one tenant.
///
/// The shared dev fixture needs the start card's process without importing the whole 280;
/// `seed::catalogue` (this crate) calls this for `project_start_request_analysis`. The file is
/// found by its declared `card_type`, so a caller never names a path.
pub async fn apply_type(
    pool: &PgPool,
    tenant: TenantId,
    card_type: &str,
) -> Result<SeedCounts, ProcessSeedError> {
    let catalogue = catalogue()?;
    let file = catalogue
        .types
        .iter()
        .find(|file| file.card_type == card_type)
        .ok_or_else(|| ProcessSeedError::UnknownCardType {
            card_type: card_type.to_owned(),
        })?;
    let mut state = State {
        labels: card_type_labels()?,
        capabilities: HashMap::new(),
        agents: HashMap::new(),
        sops: HashMap::new(),
        counts: SeedCounts::default(),
    };
    let mut tx = begin_scoped(pool, tenant).await?;
    apply_type_file(&mut tx, tenant, file, &mut state).await?;
    tx.commit().await?;
    Ok(state.counts)
}

fn card_type_labels() -> Result<HashMap<String, String>, ProcessSeedError> {
    Ok(catalogue_export()?
        .card_types
        .into_iter()
        .map(|card_type| (card_type.code, card_type.label))
        .collect())
}

async fn apply_type_file(
    conn: &mut PgConnection,
    tenant: TenantId,
    file: &TypeFile,
    state: &mut State,
) -> Result<(), ProcessSeedError> {
    let context = FileContext {
        file: &file.file,
        capability: Some(&file.capability),
        card_type_id: Some(ensure_card_type(conn, tenant, &file.card_type, state).await?),
        outputs: &file.outputs,
    };
    ensure_agents(conn, tenant, &context, &file.agents, state).await?;
    ensure_sops(conn, tenant, &file.file, &file.sops, state).await?;
    for process in &file.processes {
        apply_process(conn, tenant, &context, process, state).await?;
    }
    Ok(())
}

async fn apply_named_file(
    conn: &mut PgConnection,
    tenant: TenantId,
    named: &NamedFile,
    state: &mut State,
) -> Result<(), ProcessSeedError> {
    let context = FileContext {
        file: &named.file,
        capability: named.capability.as_deref(),
        card_type_id: None,
        outputs: &[],
    };
    ensure_agents(conn, tenant, &context, &named.agents, state).await?;
    ensure_sops(conn, tenant, &named.file, &named.sops, state).await?;
    for process in &named.processes {
        apply_process(conn, tenant, &context, process, state).await?;
    }
    Ok(())
}

/// The capability id for `code`, cached per apply.
async fn capability_id(
    conn: &mut PgConnection,
    tenant: TenantId,
    file: &str,
    code: &str,
    state: &mut State,
) -> Result<Uuid, ProcessSeedError> {
    if let Some(id) = state.capabilities.get(code) {
        return Ok(*id);
    }
    let id = capability::get_capability_by_code_tx(conn, tenant, code)
        .await?
        .ok_or_else(|| ProcessSeedError::MissingCapability {
            file: file.to_owned(),
            code: code.to_owned(),
        })?
        .id;
    state.capabilities.insert(code.to_owned(), id);
    Ok(id)
}

/// Find each agent by `(tenant_id, key)` or insert it active; cache the ids.
async fn ensure_agents(
    conn: &mut PgConnection,
    tenant: TenantId,
    context: &FileContext<'_>,
    agents: &[AgentSpec],
    state: &mut State,
) -> Result<(), ProcessSeedError> {
    for agent in agents {
        if state.agents.contains_key(&agent.key) {
            continue;
        }
        let existing: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM capability_agent WHERE tenant_id = $1 AND key = $2")
                .bind(tenant.as_uuid())
                .bind(&agent.key)
                .fetch_optional(&mut *conn)
                .await?;
        let id = match existing {
            Some(id) => id,
            None => {
                let capability = agent
                    .capability
                    .as_deref()
                    .or(context.capability)
                    .ok_or_else(|| ProcessSeedError::MissingCapabilityName {
                        file: context.file.to_owned(),
                        owner: format!("agent {:?}", agent.key),
                    })?;
                let capability_id =
                    capability_id(conn, tenant, context.file, capability, state).await?;
                // Raw SQL, not `capability::create_agent_tx`: the seed writes `role`,
                // `process_key` and `toolset` from the file, which that constructor does not
                // take. The parent capability is resolved in scope above (CONTRACT §2).
                let id: Uuid = sqlx::query_scalar(
                    "INSERT INTO capability_agent
                         (tenant_id, capability_id, key, status, model_ref, prompt_ref, params,
                          allowed_actions, max_iterations, timeout_secs, role, process_key,
                          toolset)
                     VALUES ($1, $2, $3, 'active', $4, $5, '{}'::jsonb, $6, $7, $8, $9, $10, $11)
                     RETURNING id",
                )
                .bind(tenant.as_uuid())
                .bind(capability_id)
                .bind(&agent.key)
                .bind(&agent.model)
                .bind(agent.prompt_ref())
                .bind(&agent.allowed_actions)
                .bind(agent.max_iterations)
                .bind(agent.timeout_secs)
                .bind(&agent.role)
                .bind(agent.process_key())
                .bind(&agent.toolset)
                .fetch_one(&mut *conn)
                .await?;
                state.counts.agents += 1;
                id
            }
        };
        state.agents.insert(agent.key.clone(), id);
    }
    Ok(())
}

/// Find the card type by `(tenant_id, code)` or create it with the catalogue's label.
async fn ensure_card_type(
    conn: &mut PgConnection,
    tenant: TenantId,
    code: &str,
    state: &mut State,
) -> Result<Uuid, ProcessSeedError> {
    let existing: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM card_type WHERE tenant_id = $1 AND code = $2")
            .bind(tenant.as_uuid())
            .bind(code)
            .fetch_optional(&mut *conn)
            .await?;
    if let Some(id) = existing {
        return Ok(id);
    }
    let label = state
        .labels
        .get(code)
        .cloned()
        .unwrap_or_else(|| code.to_owned());
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO card_type (tenant_id, code, label) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(code)
    .bind(&label)
    .fetch_one(&mut *conn)
    .await?;
    state.counts.card_types += 1;
    Ok(id)
}

/// Skip a published SOP, else create the draft and publish it; cache the published id.
async fn ensure_sops(
    conn: &mut PgConnection,
    tenant: TenantId,
    file: &str,
    sops: &[SopSpec],
    state: &mut State,
) -> Result<(), ProcessSeedError> {
    for sop in sops {
        if state.sops.contains_key(&sop.key) {
            continue;
        }
        let published = procedure::published_by_key_tx(conn, tenant, &sop.key)
            .await
            .map_err(|source| ProcessSeedError::Procedure {
                file: file.to_owned(),
                sop: sop.key.clone(),
                source,
            })?;
        let id = match published {
            Some(record) => record.id,
            None => {
                let draft = procedure::create_draft_tx(
                    conn,
                    tenant,
                    &NewSop {
                        key: sop.key.clone(),
                        title: sop.title.clone(),
                        description: sop.description.clone(),
                        definition_of_ready: sop.definition_of_ready.clone(),
                        definition_of_done: sop.definition_of_done.clone(),
                        acceptance_criteria: sop.acceptance_criteria.clone(),
                        inspection: sop.inspection.clone(),
                        outputs: sop.outputs.clone(),
                    },
                )
                .await
                .map_err(|source| ProcessSeedError::Procedure {
                    file: file.to_owned(),
                    sop: sop.key.clone(),
                    source,
                })?;
                let record = procedure::publish_tx(conn, tenant, draft.id)
                    .await
                    .map_err(|source| ProcessSeedError::Procedure {
                        file: file.to_owned(),
                        sop: sop.key.clone(),
                        source,
                    })?;
                state.counts.sops += 1;
                record.id
            }
        };
        state.sops.insert(sop.key.clone(), id);
    }
    Ok(())
}

/// Skip a published process, else create the draft, insert the resolved steps and publish it.
async fn apply_process(
    conn: &mut PgConnection,
    tenant: TenantId,
    context: &FileContext<'_>,
    process: &ProcessSpec,
    state: &mut State,
) -> Result<(), ProcessSeedError> {
    let published = process::published_by_key_tx(conn, tenant, &process.key)
        .await
        .map_err(|source| ProcessSeedError::Process {
            file: context.file.to_owned(),
            process: process.key.clone(),
            source,
        })?;
    if published.is_some() {
        return Ok(());
    }

    let capability = context.capability_code(process)?;
    let capability_id = capability_id(conn, tenant, context.file, &capability, state).await?;
    let outputs = if process.outputs.is_empty() {
        context.outputs
    } else {
        process.outputs.as_slice()
    };

    let mut steps = process.steps.clone();
    steps.sort_by_key(|step| step.position);
    let mut inputs = Vec::with_capacity(steps.len());
    for step in &steps {
        let procedure_id =
            *state
                .sops
                .get(&step.sop)
                .ok_or_else(|| ProcessSeedError::MissingSop {
                    file: context.file.to_owned(),
                    process: process.key.clone(),
                    step: step.position,
                    sop: step.sop.clone(),
                })?;
        let process_agent_id = match step.process_agent.as_deref() {
            Some(agent) => Some(*state.agents.get(agent).ok_or_else(|| {
                ProcessSeedError::MissingProcessAgent {
                    file: context.file.to_owned(),
                    process: process.key.clone(),
                    step: step.position,
                    agent: agent.to_owned(),
                }
            })?),
            None => None,
        };
        inputs.push(StepInput {
            // One spelling across the seed and the API: an `agent` step stores the primitive it
            // runs, so publish validation and the executor read the same value.
            action: match step.kind.as_deref() {
                Some("agent") if step.action.trim().is_empty() => {
                    conveyor_domain::process::AGENT_STEP_ACTION.to_owned()
                }
                Some("agent") => step.action.trim().to_owned(),
                _ => step.action.trim().to_owned(),
            },
            kind: step.kind.clone(),
            actor: step.actor.clone(),
            capability_id: Some(capability_id),
            process_agent_id,
            procedure_id: Some(procedure_id),
            child_process_key: step.child_process_key.clone(),
            input_template: step.input_template.clone(),
            gate_kind: step.gate_kind.clone(),
            on_result: step.on_result.clone(),
            max_attempts: step.max_attempts,
            outputs: step.outputs.clone(),
        });
    }

    // A `call` step starts a published process or workflow, so its target must already be
    // published in this tenant when the file is applied. `validate_publish` enforces the same rule
    // through the API; the loader cannot reach the registry, but it can reach this.
    let mut call_targets = process::published_keys_tx(conn, tenant)
        .await
        .map_err(|source| ProcessSeedError::Process {
            file: context.file.to_owned(),
            process: process.key.clone(),
            source: process::ProcessStoreError::Database(source),
        })?;
    call_targets.extend(
        crate::workflow::workflow::published_keys_tx(conn, tenant)
            .await
            .map_err(|source| ProcessSeedError::Process {
                file: context.file.to_owned(),
                process: process.key.clone(),
                source: process::ProcessStoreError::Database(source),
            })?,
    );
    for (index, step) in inputs.iter().enumerate() {
        if step.kind.as_deref() != Some("call") {
            continue;
        }
        let key = step.child_process_key.as_deref().unwrap_or_default();
        let position = i32::try_from(index + 1).unwrap_or(i32::MAX);
        if key == process.key {
            return Err(ProcessSeedError::BadStepShape {
                file: context.file.to_owned(),
                process: process.key.clone(),
                step: position,
                detail: "a call step may not start its own process".to_owned(),
            });
        }
        if !call_targets.iter().any(|target| target == key) {
            return Err(ProcessSeedError::UnknownCallTarget {
                file: context.file.to_owned(),
                process: process.key.clone(),
                step: position,
                key: key.to_owned(),
            });
        }
    }

    let draft = process::create_draft_tx(
        conn,
        tenant,
        &NewProcess {
            key: process.key.clone(),
            title: process.title.clone(),
            description: process.description.clone(),
            card_type_id: context.card_type_id,
            list_kind: process.list.clone(),
            project_type_id: None,
            outputs: serde_json::json!(outputs),
            capability_id: Some(capability_id),
            owner_user_id: None,
        },
    )
    .await
    .map_err(|source| ProcessSeedError::Process {
        file: context.file.to_owned(),
        process: process.key.clone(),
        source,
    })?;
    process::insert_steps_tx(conn, tenant, draft.id, &inputs)
        .await
        .map_err(|source| ProcessSeedError::Process {
            file: context.file.to_owned(),
            process: process.key.clone(),
            source,
        })?;
    process::publish_tx(conn, tenant, draft.id, None)
        .await
        .map_err(|source| ProcessSeedError::Process {
            file: context.file.to_owned(),
            process: process.key.clone(),
            source,
        })?;
    state.counts.processes += 1;
    Ok(())
}

/// Apply `maps.yaml`: resolve each map's project type, skip a key already published for it, build
/// the facts the domain validator needs, then create and publish version 1.
///
/// A map is validated against **this project type's** phases (`meta.phases`) and the tenant's
/// published processes, capability codes and `milestone_kind` codes. The validator is the domain's
/// own; the seed never loosens it. Validation runs before the draft is written, so a refusal leaves
/// the transaction with no map row at all.
async fn apply_maps(
    conn: &mut PgConnection,
    tenant: TenantId,
    file: &str,
    maps: &[MapSpec],
    state: &mut State,
) -> Result<(), ProcessSeedError> {
    for map in maps {
        let code = Code::parse(&map.project_type).map_err(|source| {
            ProcessSeedError::InvalidMapProjectType {
                file: file.to_owned(),
                map: map.key.clone(),
                project_type: map.project_type.clone(),
                source,
            }
        })?;
        let entry =
            catalogue::get_catalogue_entry_tx(conn, tenant, CatalogueKind::ProjectType, &code)
                .await?
                .ok_or_else(|| ProcessSeedError::MissingProjectType {
                    file: file.to_owned(),
                    map: map.key.clone(),
                    project_type: map.project_type.clone(),
                })?;

        // `published_by_project_type_tx` is the existing read for this binding; a key the tenant
        // already published for the same project type is never overwritten.
        let published = process_map::published_by_project_type_tx(conn, tenant, entry.id)
            .await
            .map_err(|source| ProcessSeedError::ProcessMap {
                file: file.to_owned(),
                map: map.key.clone(),
                source,
            })?;
        if published.iter().any(|record| record.key == map.key) {
            continue;
        }

        let capabilities: Vec<String> =
            sqlx::query_scalar("SELECT code FROM capability WHERE tenant_id = $1")
                .bind(tenant.as_uuid())
                .fetch_all(&mut *conn)
                .await?;
        let milestones: Vec<String> = sqlx::query_scalar(
            "SELECT code FROM catalogue_entry WHERE tenant_id = $1 AND kind = 'milestone_kind'",
        )
        .bind(tenant.as_uuid())
        .fetch_all(&mut *conn)
        .await?;
        let facts = MapFacts {
            processes: process::published_keys_tx(conn, tenant).await?,
            capabilities: capabilities.into_iter().collect(),
            // The project type's OWN phases — there are no `project_type_phase` rows.
            phases: entry
                .meta
                .get("phases")
                .and_then(Value::as_array)
                .map(|phases| {
                    phases
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default(),
            milestones: milestones.into_iter().collect(),
        };

        conveyor_domain::process_map::validate(&map.definition, &facts).map_err(|errors| {
            ProcessSeedError::InvalidMap {
                file: file.to_owned(),
                map: map.key.clone(),
                errors,
            }
        })?;

        let definition = serde_json::to_value(&map.definition).map_err(|source| {
            ProcessSeedError::MapDefinition {
                file: file.to_owned(),
                map: map.key.clone(),
                source,
            }
        })?;
        let draft = process_map::create_draft_tx(
            conn,
            tenant,
            &NewMap {
                key: map.key.clone(),
                title: map.title.clone(),
                project_type_id: entry.id,
                definition,
            },
        )
        .await
        .map_err(|source| ProcessSeedError::ProcessMap {
            file: file.to_owned(),
            map: map.key.clone(),
            source,
        })?;
        process_map::publish_tx(conn, tenant, draft.id)
            .await
            .map_err(|source| ProcessSeedError::ProcessMap {
                file: file.to_owned(),
                map: map.key.clone(),
                source,
            })?;
        state.counts.maps += 1;
    }
    Ok(())
}
