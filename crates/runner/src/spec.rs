//! The Job spec: a `runner/v1` instruction becomes a ConfigMap, a Secret and a `batch/v1` Job.
//!
//! The spec is a pure function of the instruction and the dispatch configuration, so it is
//! unit-tested without a cluster. Two invariants are structural rather than documented:
//! `automountServiceAccountToken` is always `false` and no `serviceAccountName` is emitted; the
//! pod's CPU, memory, wall clock and root filesystem come from the instruction's limits.

use conveyor_domain::runner::Instruction;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::cluster::default_labels;
use crate::{CONFIGMAP_LIMIT, NodeSelector, RunnerError, Toleration};

/// Per-kind dispatch configuration that is not part of the instruction.
#[derive(Clone, Debug)]
pub struct Dispatch {
    /// The namespace the three objects are created in.
    pub namespace: String,
    /// The executor image, pinned by digest by the deployment.
    pub image: String,
    /// The pull secret present in the runner namespace.
    pub pull_secret: String,
    /// An optional node selector for this capability.
    pub node_selector: Option<NodeSelector>,
    /// An optional toleration matching that node's taint.
    pub toleration: Option<Toleration>,
}

/// The three objects one dispatch creates, plus the values a test wants to assert on.
#[derive(Clone, Debug)]
pub struct Manifests {
    /// The deterministic object name, shared by all three objects.
    pub name: String,
    /// The instruction as it appears in the ConfigMap.
    pub instruction_json: String,
    /// The job token the Secret carries (never logged).
    pub token: String,
    pub config_map: Value,
    pub secret: Value,
    pub job: Value,
}

/// The deterministic name for `(job, attempt)`: a repeated dispatch finds the Job it started.
pub fn job_name(job_id: Uuid, attempt: i32) -> String {
    let digest = Sha256::digest(job_id.as_bytes());
    let run12: String = digest.iter().take(6).map(|b| format!("{b:02x}")).collect();
    let name = format!("run-{run12}-{attempt}");
    debug_assert!(name.len() <= 63, "a runner name must fit the label limit");
    name
}

/// Build the ConfigMap, Secret and Job for one instruction.
pub fn manifests(
    instruction: &Instruction,
    dispatch: &Dispatch,
    token: &str,
) -> Result<Manifests, RunnerError> {
    instruction
        .validate()
        .map_err(|err| RunnerError::Manifest {
            detail: err.to_string(),
        })?;
    if token.trim().is_empty() {
        return Err(RunnerError::Manifest {
            detail: "the job token must not be empty".to_owned(),
        });
    }

    let instruction_json =
        serde_json::to_string(instruction).map_err(|err| RunnerError::Manifest {
            detail: format!("instruction: {err}"),
        })?;
    if instruction_json.len() > CONFIGMAP_LIMIT {
        return Err(RunnerError::ConfigMapTooLarge {
            size: instruction_json.len(),
            limit: CONFIGMAP_LIMIT,
        });
    }

    let name = job_name(instruction.job_id, instruction.attempt);
    let labels = default_labels(instruction.job_id, instruction.attempt, &instruction.kind);
    let limits = instruction.limits;

    // The pod clock is the instruction's wall clock plus a grace window for the result POST: a
    // pod that dies at the deadline without reporting is an infrastructure failure, not a
    // capability failure.
    let deadline = limits.wall_secs.saturating_add(60);

    let mut pod_spec = json!({
        "restartPolicy": "Never",
        // Invariant 1: a pod can never acquire a cluster credential.
        "automountServiceAccountToken": false,
        "enableServiceLinks": false,
        "imagePullSecrets": [{ "name": dispatch.pull_secret }],
        "containers": [{
            "name": "executor",
            "image": dispatch.image,
            "imagePullPolicy": "IfNotPresent",
            "env": [
                { "name": "RUNNER_INSTRUCTION", "value": "/etc/conveyor/instruction.json" },
                { "name": "RUNNER_BASE_URL", "value": instruction.conveyor_base_url.clone() },
                { "name": "RUNNER_TOKEN", "valueFrom": { "secretKeyRef": { "name": name, "key": "token" } } },
                { "name": "HOME", "value": "/tmp" },
                { "name": "TMPDIR", "value": "/tmp" }
            ],
            "resources": {
                "requests": {
                    "cpu": format!("{}m", limits.cpu_millis),
                    "memory": limits.memory_bytes.to_string(),
                    "ephemeral-storage": "1Gi"
                },
                "limits": {
                    "cpu": format!("{}m", limits.cpu_millis),
                    "memory": limits.memory_bytes.to_string(),
                    "ephemeral-storage": "4Gi"
                }
            },
            "securityContext": {
                "allowPrivilegeEscalation": false,
                "readOnlyRootFilesystem": true,
                "capabilities": { "drop": ["ALL"] }
            },
            "volumeMounts": [
                { "name": "instruction", "mountPath": "/etc/conveyor", "readOnly": true },
                { "name": "token", "mountPath": "/etc/conveyor-token", "readOnly": true },
                { "name": "tmp", "mountPath": "/tmp" }
            ]
        }],
        "volumes": [
            { "name": "instruction", "configMap": { "name": name, "defaultMode": 292 } },
            { "name": "token", "secret": { "secretName": name, "defaultMode": 256 } },
            { "name": "tmp", "emptyDir": {} }
        ]
    });

    if let Some(selector) = &dispatch.node_selector {
        let mut node_selector = Map::new();
        node_selector.insert(selector.key.clone(), Value::String(selector.value.clone()));
        pod_spec["nodeSelector"] = Value::Object(node_selector);
    }
    if let Some(toleration) = &dispatch.toleration {
        pod_spec["tolerations"] = json!([{
            "key": toleration.key,
            "operator": "Equal",
            "value": toleration.value,
            "effect": toleration.effect,
        }]);
    }

    let config_map = json!({
        "apiVersion": "v1",
        "kind": "ConfigMap",
        "metadata": { "name": name, "namespace": dispatch.namespace, "labels": labels.clone() },
        "data": { "instruction.json": instruction_json },
    });

    let secret = json!({
        "apiVersion": "v1",
        "kind": "Secret",
        "metadata": { "name": name, "namespace": dispatch.namespace, "labels": labels.clone() },
        "type": "Opaque",
        "stringData": { "token": token },
    });

    let job = json!({
        "apiVersion": "batch/v1",
        "kind": "Job",
        "metadata": {
            "name": name,
            "namespace": dispatch.namespace,
            "labels": labels.clone(),
            "annotations": {
                "conveyor/kind": instruction.kind.clone(),
                "conveyor/attempt": instruction.attempt.to_string(),
                "conveyor/correlation-id": instruction.correlation_id.to_string(),
                "conveyor/trace-id": instruction.trace_id.clone(),
            },
        },
        "spec": {
            "backoffLimit": 0,
            "activeDeadlineSeconds": deadline,
            "ttlSecondsAfterFinished": 3600,
            "template": {
                "metadata": { "labels": labels },
                "spec": pod_spec,
            },
        },
    });

    Ok(Manifests {
        name,
        instruction_json,
        token: token.to_owned(),
        config_map,
        secret,
        job,
    })
}
