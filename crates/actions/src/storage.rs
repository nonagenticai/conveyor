#![forbid(unsafe_code)]
//! Job object storage — direct MinIO, tenant-separated (SPEC_V2 §11.10, D57; T13 P5).
//!
//! One adapter, and it is deliberately narrow:
//!
//! * **a mandatory tenant prefix** — every key starts `tenant/<tenant_id>/`, with the project and
//!   run beneath it when the caller has them. The prefix is not a convention a caller may skip:
//!   it is built here, and a name that is not one safe segment is refused;
//! * **a content hash per object** — `put` returns the SHA-256 of the bytes and `get_verified`
//!   refuses a read-back that does not match it;
//! * **a size cap** — one object is at most [`MAX_OBJECT_BYTES`];
//! * **no bucket administration** — there is no list, no bucket create and no delete-bucket on
//!   this type. A job can write, read, head and delete its own keys and nothing else;
//! * **server-side encryption** — [`Objects::from_env_sealed`] asks for SSE-S3 on every write;
//! * **credentials stay here** — the adapter is constructed from the service's own sealed
//!   configuration; a job never sees a key, a secret or an S3 URL (§11.3 step 4, §11.10).
//!
//! The bytes go to the platform's shared MinIO bucket the documents adapter already uses — not a
//! private store — and `artifact_service`/`storage-api` are not in the path.

use bytes::Bytes;
use uuid::Uuid;

use crate::integrations::objects::Objects;

/// The mandatory first path segment of every job object (§11.10).
pub const TENANT_PREFIX: &str = "tenant";

/// One object's cap. A run that needs more writes more objects, so a single answer can never
/// exhaust the store.
pub const MAX_OBJECT_BYTES: usize = 64 * 1024 * 1024;

/// The longest name segment the adapter accepts.
const MAX_NAME_CHARS: usize = 128;

/// Why a storage operation failed.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// `CONVEYOR_S3_*` is not configured; the caller answers `503`, never writes elsewhere.
    #[error("object storage is not configured")]
    Unavailable,
    /// The name is not one safe path segment (empty, too long, a separator or `..`).
    #[error("object name {0:?} is not a safe key segment")]
    InvalidName(String),
    /// The bytes exceed [`MAX_OBJECT_BYTES`].
    #[error("the object is {size} bytes; the adapter caps one object at {max}")]
    TooLarge {
        /// The size the caller offered.
        size: usize,
        /// The cap.
        max: usize,
    },
    /// No object at that key.
    #[error("no object at that key")]
    NotFound,
    /// The read-back hash did not match the expected one.
    #[error("the object's sha256 is {actual}, not the expected {expected}")]
    HashMismatch {
        /// The caller's expected hash.
        expected: String,
        /// The hash the store returned.
        actual: String,
    },
    /// The store refused or failed the call.
    #[error("object storage: {0}")]
    Store(String),
}

/// What a write produced: the key it used, the bytes' hash and their size.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredObject {
    /// The tenant-prefixed key.
    pub key: String,
    /// The SHA-256 of the stored bytes, lowercase hex.
    pub sha256: String,
    /// The number of bytes stored.
    pub bytes: usize,
}

/// The job-facing storage adapter.
#[derive(Clone, Debug)]
pub struct JobStorage {
    objects: Objects,
}

impl JobStorage {
    /// Build from the environment; `None` when storage is not configured.
    pub fn from_env() -> Option<Self> {
        Objects::from_env_sealed().map(|objects| Self { objects })
    }

    /// The key for one object: `tenant/<tenant>[/project/<project>][/run/<run>]/<name>`.
    ///
    /// The tenant segment is written first and is never caller-supplied, which is what makes the
    /// separation enforceable rather than polite.
    pub fn key(
        &self,
        tenant: Uuid,
        project: Option<Uuid>,
        run: Option<Uuid>,
        name: &str,
    ) -> Result<String, StorageError> {
        key_for(tenant, project, run, name)
    }

    /// Store `bytes` under the tenant's prefix and return its key and hash.
    pub async fn put(
        &self,
        tenant: Uuid,
        project: Option<Uuid>,
        run: Option<Uuid>,
        name: &str,
        bytes: Bytes,
    ) -> Result<StoredObject, StorageError> {
        if bytes.len() > MAX_OBJECT_BYTES {
            return Err(StorageError::TooLarge {
                size: bytes.len(),
                max: MAX_OBJECT_BYTES,
            });
        }
        let key = key_for(tenant, project, run, name)?;
        let sha256 = sha256_hex(&bytes);
        let size = bytes.len();
        self.objects.put(&key, bytes).await.map_err(storage_error)?;
        Ok(StoredObject {
            key,
            sha256,
            bytes: size,
        })
    }

    /// Read one object.
    pub async fn get(
        &self,
        tenant: Uuid,
        project: Option<Uuid>,
        run: Option<Uuid>,
        name: &str,
    ) -> Result<Bytes, StorageError> {
        let key = key_for(tenant, project, run, name)?;
        self.objects.get(&key).await.map_err(storage_error)
    }

    /// Read one object and refuse it unless its hash matches the expected value.
    ///
    /// This is the read-back a write tool's `checks[]` uses: the caller compares the hash it
    /// recorded with the hash of the bytes that actually came back.
    pub async fn get_verified(
        &self,
        tenant: Uuid,
        project: Option<Uuid>,
        run: Option<Uuid>,
        name: &str,
        expected_sha256: &str,
    ) -> Result<Bytes, StorageError> {
        let bytes = self.get(tenant, project, run, name).await?;
        let actual = sha256_hex(&bytes);
        if !actual.eq_ignore_ascii_case(expected_sha256) {
            return Err(StorageError::HashMismatch {
                expected: expected_sha256.to_owned(),
                actual,
            });
        }
        Ok(bytes)
    }

    /// Whether an object exists at the key.
    pub async fn head(
        &self,
        tenant: Uuid,
        project: Option<Uuid>,
        run: Option<Uuid>,
        name: &str,
    ) -> Result<bool, StorageError> {
        let key = key_for(tenant, project, run, name)?;
        self.objects.head(&key).await.map_err(storage_error)
    }
}

/// The key scheme, as a free function so a caller can compute it without an adapter.
pub fn key_for(
    tenant: Uuid,
    project: Option<Uuid>,
    run: Option<Uuid>,
    name: &str,
) -> Result<String, StorageError> {
    if !is_safe_segment(name) {
        return Err(StorageError::InvalidName(name.to_owned()));
    }
    let mut key = format!("{TENANT_PREFIX}/{tenant}");
    if let Some(project) = project {
        key.push_str(&format!("/project/{project}"));
    }
    if let Some(run) = run {
        key.push_str(&format!("/run/{run}"));
    }
    key.push('/');
    key.push_str(name);
    Ok(key)
}

/// One safe path segment: non-empty, bounded, no separators and no traversal.
fn is_safe_segment(name: &str) -> bool {
    if name.is_empty() || name.chars().count() > MAX_NAME_CHARS {
        return false;
    }
    if name == "." || name == ".." || name.contains('/') || name.contains('\\') {
        return false;
    }
    name.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// The SHA-256 of the bytes, lowercase hex — the same helper the engine's own hashes use.
pub fn sha256_hex(bytes: &[u8]) -> String {
    crate::spec::sha256_hex(bytes)
}

fn storage_error(error: crate::integrations::objects::ObjectError) -> StorageError {
    use crate::integrations::objects::ObjectError;
    match error {
        ObjectError::Unavailable => StorageError::Unavailable,
        ObjectError::NotFound => StorageError::NotFound,
        ObjectError::Store(detail) => StorageError::Store(detail),
    }
}
