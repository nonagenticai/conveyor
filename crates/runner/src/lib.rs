#![forbid(unsafe_code)]
//! Dispatch to a stateless executor: the `batch/v1` client and the Job built from a `runner/v1`
//! instruction (ADR 0007 §Decision 3–4).
//!
//! The conveyor is the only stateful plane. An executor pod has **no service-account token**, no
//! database, no queue and no store credential: it receives one instruction in a ConfigMap, one
//! short-lived job token in a Secret, and writes its bytes through conveyor's own job routes. This
//! crate builds those objects and talks to the Kubernetes API over `reqwest` — the same recorded
//! choice as `apps/server/src/sandbox.rs`, which this generalises: the surface is five REST calls
//! and `kube`/`k8s-openapi` would add a dependency tree for nothing.
//!
//! Two properties are enforced by the types, not by documentation:
//!
//! 1. **A pod cannot acquire a cluster credential.** `automountServiceAccountToken` is always
//!    `false` and no `serviceAccountName` is ever emitted.
//! 2. **The instruction's caps become the pod's caps.** CPU, memory, the wall clock and the
//!    output ceiling come from [`conveyor_domain::runner::Limits`], never from a per-kind default.

pub mod cluster;
pub mod spec;

pub use cluster::{Cluster, ClusterConfig, JobState, NodeSelector, RunnerError, Toleration};
pub use spec::{Dispatch, Manifests, job_name, manifests};

/// The ConfigMap object cap; refuse before create rather than after a truncated apply.
pub const CONFIGMAP_LIMIT: usize = 1_000_000;

/// The estate's registry pull secret, present in every consuming namespace.
pub const DEFAULT_PULL_SECRET: &str = "registry-pull-secret";

/// The in-cluster service-account directory.
pub const SA_DIR: &str = "/var/run/secrets/kubernetes.io/serviceaccount";
