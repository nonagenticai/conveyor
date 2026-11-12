//! The object store: MinIO behind one bucket, one key scheme (S09 §5.2).
//!
//! **No presigned URLs, ever.** A presigned URL is a time-bombed authorization decision taken by a service
//! that cannot see the caller — the conveyor is the sole authorizer of every byte it serves, so bytes leave
//! through `GET /api/documents/{id}/content` (and the `documents.download` action) after the same tenant check
//! as every other read. The store credential never reaches a client.
//!
//! The key is `t/<tenant>/<document>/<content-hash-prefix>`: tenant-scoped so a bucket listing is not a data
//! leak, and content-addressed enough that two identical uploads in one tenant share a key only if they share
//! every byte (the row still gets its own id and its own key suffix when the hash repeats — see `key_for`).
//!
//! Configuration is environment-only, in the same spirit as `service_url`:
//!
//! | variable | meaning |
//! |---|---|
//! | `CONVEYOR_S3_ENDPOINT` | e.g. `http://minio.example.com:8389`; absent ⇒ object storage is unavailable |
//! | `CONVEYOR_S3_ACCESS_KEY` / `CONVEYOR_S3_SECRET_KEY` | service credentials |
//! | `CONVEYOR_S3_BUCKET` | default `conveyor-documents` |
//! | `CONVEYOR_S3_REGION` | default `us-east-1` (MinIO ignores it; the S3 signer does not) |
//!
//! A missing configuration is [`ObjectError::Unavailable`] — a document action answers 503, never a silent
//! write to somewhere else. There is no local-disk fallback: bytes that live on a pod die with the pod.

use bytes::Bytes;
use object_store::ObjectStore;
// object_store 0.14 moved the high-level put/get/head/delete methods onto
// `ObjectStoreExt` (the `ObjectStore` trait keeps the low-level transport methods
// and is what the dyn store is typed as). Both must be in scope.
use object_store::ObjectStoreExt;
use object_store::aws::AmazonS3Builder;
use object_store::path::Path as ObjectPath;
use std::sync::Arc;
use std::sync::OnceLock;
use uuid::Uuid;

/// The default bucket.
pub const DEFAULT_BUCKET: &str = "conveyor-documents";

/// Why an object operation failed.
#[derive(Debug, thiserror::Error)]
pub enum ObjectError {
    /// `CONVEYOR_S3_ENDPOINT`/credentials are not configured.
    #[error("object storage is not configured")]
    Unavailable,
    /// No object at that key.
    #[error("the object does not exist")]
    NotFound,
    /// The store refused or failed the call.
    #[error("object storage: {0}")]
    Store(String),
}

/// A handle to the one bucket this service uses.
#[derive(Clone)]
pub struct Objects {
    store: Arc<dyn ObjectStore>,
    bucket: String,
}

impl std::fmt::Debug for Objects {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Objects")
            .field("bucket", &self.bucket)
            .finish()
    }
}

impl Objects {
    /// Build from the environment; `None` when the endpoint is absent.
    ///
    /// ⚠️ `CONVEYOR_S3_BACKEND=memory` selects `object_store`'s own in-memory backend. It exists for the test
    /// suite: the MinIO image cannot be pulled on every dev box (`minio/minio` is not reachable from this
    /// one), and a hand-written fake would prove nothing about the crate's semantics. It is the same
    /// `ObjectStore` trait behind the same key scheme, so put/get/delete/head — the contract this module
    /// owns — are real. What it does NOT prove is the S3 wire: endpoint, path-style addressing and SigV4.
    /// That leg is marked UNVERIFIED in `docs/plans/slices/S09-documents-integrations.md` until it runs
    /// against a real MinIO in the cluster.
    pub fn from_env() -> Option<Self> {
        if non_empty("CONVEYOR_S3_BACKEND").as_deref() == Some("memory") {
            // ONE shared instance per process: `from_env` is called per action, and a fresh in-memory store
            // each time would make an upload invisible to the download that follows it.
            static MEMORY: OnceLock<Arc<dyn ObjectStore>> = OnceLock::new();
            let store = MEMORY
                .get_or_init(|| Arc::new(object_store::memory::InMemory::new()))
                .clone();
            return Some(Self {
                store,
                bucket: non_empty("CONVEYOR_S3_BUCKET")
                    .unwrap_or_else(|| DEFAULT_BUCKET.to_owned()),
            });
        }
        let endpoint = non_empty("CONVEYOR_S3_ENDPOINT")?;
        let access = non_empty("CONVEYOR_S3_ACCESS_KEY")?;
        let secret = non_empty("CONVEYOR_S3_SECRET_KEY")?;
        let bucket = non_empty("CONVEYOR_S3_BUCKET").unwrap_or_else(|| DEFAULT_BUCKET.to_owned());
        let region = non_empty("CONVEYOR_S3_REGION").unwrap_or_else(|| "us-east-1".to_owned());
        let s3 = AmazonS3Builder::new()
            .with_endpoint(endpoint)
            .with_bucket_name(&bucket)
            .with_access_key_id(access)
            .with_secret_access_key(secret)
            .with_region(region)
            // MinIO is plain HTTP inside the mesh, and path-style addressing is what it serves.
            .with_allow_http(true)
            .with_virtual_hosted_style_request(false)
            .build()
            .ok()?;
        Some(Self {
            store: Arc::new(s3),
            bucket,
        })
    }

    /// The bucket name (tests assert the key prefix against it).
    pub fn bucket(&self) -> &str {
        &self.bucket
    }

    /// Store `bytes` at `key`, replacing any object already there.
    pub async fn put(&self, key: &str, bytes: Bytes) -> Result<(), ObjectError> {
        let path = ObjectPath::from(key);
        self.store
            .put(&path, bytes.into())
            .await
            .map(|_| ())
            .map_err(map_error)
    }

    /// Read the object at `key`.
    pub async fn get(&self, key: &str) -> Result<Bytes, ObjectError> {
        let path = ObjectPath::from(key);
        let result = self.store.get(&path).await.map_err(map_error)?;
        result.bytes().await.map_err(map_error)
    }

    /// Delete the object at `key`. A missing object is success: the caller is deleting a tombstone.
    pub async fn delete(&self, key: &str) -> Result<(), ObjectError> {
        let path = ObjectPath::from(key);
        match self.store.delete(&path).await {
            Ok(()) => Ok(()),
            Err(object_store::Error::NotFound { .. }) => Ok(()),
            Err(error) => Err(map_error(error)),
        }
    }

    /// True when an object exists at `key`.
    pub async fn head(&self, key: &str) -> Result<bool, ObjectError> {
        let path = ObjectPath::from(key);
        match self.store.head(&path).await {
            Ok(_) => Ok(true),
            Err(object_store::Error::NotFound { .. }) => Ok(false),
            Err(error) => Err(map_error(error)),
        }
    }
}

/// The key for one document: `t/<tenant>/<document>/<hash-prefix>`.
///
/// The document id makes the key unique even when two uploads are byte-identical (a re-upload after a delete
/// must not resurrect the deleted object), and the hash prefix makes an operator's listing legible.
pub fn key_for(tenant: Uuid, document: Uuid, content_hash: &str) -> String {
    let prefix: String = content_hash.chars().take(16).collect();
    format!("t/{tenant}/{document}/{prefix}")
}

fn map_error(error: object_store::Error) -> ObjectError {
    match error {
        object_store::Error::NotFound { .. } => ObjectError::NotFound,
        object_store::Error::NotSupported { .. } | object_store::Error::NotImplemented { .. } => {
            ObjectError::Unavailable
        }
        other => ObjectError::Store(other.to_string()),
    }
}

fn non_empty(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}
