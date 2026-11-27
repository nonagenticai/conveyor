#![forbid(unsafe_code)]
//! The delivery-proof certificate — SPEC_V2 §5.3/§5.4, issue #744.
//!
//! A `proof` row is the re-checkable form of gate evidence: the canonical facts, the specification
//! they answer, the exact Lean source the prover checked, the verdict, and the identity of the
//! checker. It is **append-only**: a re-verification reads the same row, it never rewrites one, so
//! the row is the record and `scripts/grants-runtime.sql` withholds UPDATE and DELETE.
//!
//! The Rust↔Lean binding is checkable without hashing inside Lean, and the three parts of it live
//! here rather than in the action layer so `proofs.record` and `resolve_evidence` cannot drift:
//!
//! * [`Proof::certificate_value`] is the canonical tuple the digest is taken over;
//! * [`Proof::recomputed_digest`] recomputes it from the stored columns;
//! * [`Proof::digest_matches`] is the refusal `resolve_evidence` makes.
//!
//! The facts digest follows from `facts_canonical` alone: `content_hash` is the sha256 of the
//! canonical JSON, so a verifier re-renders the Lean source from the stored text and compares
//! bytes.

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use conveyor_domain::compare::content_hash;
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

/// The subject kinds a certificate can be about (`proof.subject_type`).
pub const SUBJECT_TYPES: [&str; 7] = [
    "card",
    "run",
    "wbs_node",
    "release",
    "programme",
    "package",
    "unit_ledger",
];

/// The reduction a proof closed under. `kernel` is `by decide`; `native` is `native_decide`,
/// which trusts the compiler and is a different claim, so it is recorded and never flattened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProofTrust {
    /// `by decide` — the kernel reduced the proposition.
    Kernel,
    /// `native_decide` — compiled code reduced it; a stronger trust assumption.
    Native,
}

impl ProofTrust {
    /// The column value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Kernel => "kernel",
            Self::Native => "native",
        }
    }

    /// Parse a stored value.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "kernel" => Some(Self::Kernel),
            "native" => Some(Self::Native),
            _ => None,
        }
    }
}

/// What the store can refuse.
#[derive(Debug)]
pub enum ProofStoreError {
    /// The database refused the statement.
    Database(sqlx::Error),
    /// No such proof in this tenant.
    NotFound(Uuid),
}

impl std::fmt::Display for ProofStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Database(error) => write!(f, "proof store: {error}"),
            Self::NotFound(id) => write!(f, "proof store: no proof {id} in this tenant"),
        }
    }
}

impl std::error::Error for ProofStoreError {}

impl From<sqlx::Error> for ProofStoreError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

/// A certificate about to be written.
#[derive(Clone, Debug)]
pub struct NewProof {
    /// The subject kind (`card`, `run`, `wbs_node`, `release`, `programme`, `package`).
    pub subject_type: String,
    /// The subject row.
    pub subject_id: Uuid,
    /// The specification the statement answers.
    pub spec_name: String,
    /// That specification's digest.
    pub spec_sha256: String,
    /// The facts, canonically serialised (`conveyor_domain::compare::canonical_json`).
    pub facts_canonical: String,
    /// The digest of `facts_canonical`.
    pub facts_sha256: String,
    /// The exact Lean source the prover checked.
    pub lean_source: String,
    /// The theorem name the source closes.
    pub theorem: String,
    /// Whether reduction was kernel or native.
    pub trust: ProofTrust,
    /// `true` when the prover answered `valid`.
    pub proven: bool,
    /// The prover's own computation time, when it reported one.
    pub computation_time_ms: Option<f64>,
    /// The prover build the check ran on: toolchain, Mathlib revision, service version.
    pub prover_identity: Value,
    /// The engine's `action_invocation` row for the `lean4.check` call, when known.
    pub invocation_id: Option<Uuid>,
    /// That call's idempotency key — the durable name of the invocation.
    pub idempotency_key: String,
}

/// One stored certificate.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct Proof {
    /// The row id.
    pub id: Uuid,
    /// The tenant.
    pub tenant_id: Uuid,
    /// The subject kind.
    pub subject_type: String,
    /// The subject row.
    pub subject_id: Uuid,
    /// The specification the statement answers.
    pub spec_name: String,
    /// That specification's digest.
    pub spec_sha256: String,
    /// The canonical facts.
    pub facts_canonical: String,
    /// The digest of the canonical facts.
    pub facts_sha256: String,
    /// The exact Lean source.
    pub lean_source: String,
    /// The theorem name.
    pub theorem: String,
    /// `kernel` or `native`.
    pub trust: String,
    /// `proven` or `unproven`.
    pub verdict: String,
    /// The prover's computation time, when reported.
    pub computation_time_ms: Option<f64>,
    /// The prover build identity.
    pub prover_identity: Value,
    /// The engine invocation this certificate records.
    pub invocation_id: Option<Uuid>,
    /// That call's idempotency key.
    pub idempotency_key: String,
    /// The digest of the canonical tuple.
    pub certificate_sha256: String,
    /// When it was written.
    pub created_at: DateTime<Utc>,
}

const COLUMNS: &str = "id, tenant_id, subject_type, subject_id, spec_name, spec_sha256, \
                       facts_canonical, facts_sha256, lean_source, theorem, trust, verdict, \
                       computation_time_ms, prover_identity, invocation_id, idempotency_key, \
                       certificate_sha256, created_at";

impl Proof {
    /// Whether the prover answered `proven`.
    pub fn is_proven(&self) -> bool {
        self.verdict == "proven"
    }

    /// The canonical tuple the certificate digest is taken over.
    ///
    /// Deliberately excludes the row identity (`id`, `created_at`), the invocation
    /// (`invocation_id`, `idempotency_key`) and the digest itself, so the same proof of the same
    /// facts under the same specification reproduces the same certificate — which is what makes a
    /// replay a replay rather than a second claim.
    pub fn certificate_value(&self) -> Value {
        json!({
            "subject_type": self.subject_type,
            "subject_id": self.subject_id,
            "spec_name": self.spec_name,
            "spec_sha256": self.spec_sha256,
            "facts_sha256": self.facts_sha256,
            "lean_source": self.lean_source,
            "theorem": self.theorem,
            "trust": self.trust,
            "verdict": self.verdict,
            "prover_identity": self.prover_identity,
        })
    }

    /// Recompute the digest from the stored columns.
    pub fn recomputed_digest(&self) -> String {
        content_hash(&self.certificate_value())
    }

    /// Whether the stored digest still describes the stored columns.
    pub fn digest_matches(&self) -> bool {
        self.recomputed_digest() == self.certificate_sha256
    }

    /// Whether `facts_sha256` still describes `facts_canonical`.
    ///
    /// `content_hash` canonicalises, so a row written by a pre-canonicalisation writer would fail
    /// here rather than pass on a text comparison.
    pub fn facts_match(&self) -> bool {
        content_hash(&Value::String(self.facts_canonical.clone())) == self.facts_sha256
    }
}

/// The digest a [`NewProof`] will carry once written.
pub fn certificate_digest(proof: &NewProof) -> String {
    content_hash(&json!({
        "subject_type": proof.subject_type,
        "subject_id": proof.subject_id,
        "spec_name": proof.spec_name,
        "spec_sha256": proof.spec_sha256,
        "facts_sha256": proof.facts_sha256,
        "lean_source": proof.lean_source,
        "theorem": proof.theorem,
        "trust": proof.trust.as_str(),
        "verdict": if proof.proven { "proven" } else { "unproven" },
        "prover_identity": proof.prover_identity,
    }))
}

/// Write a certificate, or return the one already stored for this tuple.
///
/// `ON CONFLICT DO NOTHING` with no target covers both unique constraints: a replay of the same
/// certificate and a second record of the same invocation both land on the existing row. The
/// caller gets the stored certificate either way, so a replayed `proofs.record` is idempotent.
pub async fn insert_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    proof: &NewProof,
) -> Result<Proof, ProofStoreError> {
    let digest = certificate_digest(proof);
    let sql = format!(
        "INSERT INTO proof (tenant_id, subject_type, subject_id, spec_name, spec_sha256, \
                            facts_canonical, facts_sha256, lean_source, theorem, trust, verdict, \
                            computation_time_ms, prover_identity, invocation_id, idempotency_key, \
                            certificate_sha256) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16) \
         ON CONFLICT DO NOTHING \
         RETURNING {COLUMNS}"
    );
    let row: Option<Proof> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(&proof.subject_type)
        .bind(proof.subject_id)
        .bind(&proof.spec_name)
        .bind(&proof.spec_sha256)
        .bind(&proof.facts_canonical)
        .bind(&proof.facts_sha256)
        .bind(&proof.lean_source)
        .bind(&proof.theorem)
        .bind(proof.trust.as_str())
        .bind(if proof.proven { "proven" } else { "unproven" })
        .bind(proof.computation_time_ms)
        .bind(&proof.prover_identity)
        .bind(proof.invocation_id)
        .bind(&proof.idempotency_key)
        .bind(&digest)
        .fetch_optional(&mut *conn)
        .await?;
    if let Some(row) = row {
        return Ok(row);
    }
    // The insert hit a unique constraint, so the certificate (or the invocation) is already
    // recorded. Return that row: the replay's answer is the first write, never a second one.
    find_by_certificate_tx(conn, tenant, &digest)
        .await?
        .ok_or(ProofStoreError::NotFound(proof.subject_id))
}

/// Read one certificate by id.
pub async fn get_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<Proof>, ProofStoreError> {
    let sql = format!("SELECT {COLUMNS} FROM proof WHERE tenant_id = $1 AND id = $2");
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?)
}

/// Read the certificate already stored for a canonical tuple.
pub async fn find_by_certificate_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    certificate_sha256: &str,
) -> Result<Option<Proof>, ProofStoreError> {
    let sql =
        format!("SELECT {COLUMNS} FROM proof WHERE tenant_id = $1 AND certificate_sha256 = $2");
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(certificate_sha256)
        .fetch_optional(&mut *conn)
        .await?)
}

/// Read the certificate recorded for an engine invocation.
pub async fn find_by_invocation_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    idempotency_key: &str,
) -> Result<Option<Proof>, ProofStoreError> {
    let sql = format!("SELECT {COLUMNS} FROM proof WHERE tenant_id = $1 AND idempotency_key = $2");
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(idempotency_key)
        .fetch_optional(&mut *conn)
        .await?)
}

/// Every certificate about one subject, newest first.
pub async fn list_for_subject_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    subject_type: &str,
    subject_id: Uuid,
) -> Result<Vec<Proof>, ProofStoreError> {
    let sql = format!(
        "SELECT {COLUMNS} FROM proof \
          WHERE tenant_id = $1 AND subject_type = $2 AND subject_id = $3 \
          ORDER BY created_at DESC, id DESC"
    );
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(subject_type)
        .bind(subject_id)
        .fetch_all(&mut *conn)
        .await?)
}
