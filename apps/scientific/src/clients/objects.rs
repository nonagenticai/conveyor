//! The object-store boundary: proofs and verification history.
//!
//! The bucket is shared plumbing, not a second database: the same `object_store` crate the rest of
//! the workspace uses speaks S3, and MinIO is S3. Two rules come from the old service and are kept
//! because they are what make the routes usable before a cutover:
//!
//! - An unconfigured store is not an error. Reads answer empty; a write says so.
//! - The key layout is the old one (`proofs/{claim}/…`, `history/{tenant}/{entity}/…`), because a
//!   different layout would silently orphan every object the old service already wrote.

use bytes::Bytes;
use object_store::aws::AmazonS3Builder;
use object_store::path::Path;
use object_store::{Attribute, Attributes, ObjectStore, ObjectStoreExt, PutOptions, PutPayload};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;

use crate::config::ObjectsConfig;

#[derive(Debug, thiserror::Error)]
pub enum ObjectError {
    #[error("the object store is not configured")]
    Unconfigured,
    #[error("object store call failed: {0}")]
    Upstream(#[from] object_store::Error),
}

/// Build the S3 client for a configured bucket.
///
/// Path-style addressing and plain HTTP are explicit: MinIO is addressed by `host:port` behind the
/// mesh, and a virtual-hosted-style rewrite of that endpoint resolves nowhere.
pub fn build(config: &ObjectsConfig) -> Result<Arc<dyn ObjectStore>, ObjectError> {
    let store = AmazonS3Builder::new()
        .with_bucket_name(&config.bucket)
        .with_endpoint(config.url())
        .with_access_key_id(&config.access_key)
        .with_secret_access_key(&config.secret_key)
        .with_region("us-east-1")
        .with_allow_http(config.url().starts_with("http://"))
        .with_virtual_hosted_style_request(false)
        .build()?;
    Ok(Arc::new(store))
}

/// Write one object, replacing any existing one at the same key.
pub async fn put(
    store: Option<&Arc<dyn ObjectStore>>,
    key: &str,
    body: &[u8],
    content_type: &str,
) -> Result<(), ObjectError> {
    let store = store.ok_or(ObjectError::Unconfigured)?;
    // The content type rides as an attribute: `PutOptions` has no field for it, and an object
    // written without one is served as `application/octet-stream` by a browser but read correctly
    // by every caller here. Keeping it costs one line.
    let options = PutOptions {
        attributes: Attributes::from_iter([(Attribute::ContentType, content_type.to_owned())]),
        ..PutOptions::default()
    };
    store
        .put_opts(
            &Path::from(key),
            PutPayload::from(Bytes::copy_from_slice(body)),
            options,
        )
        .await?;
    Ok(())
}

/// Read one object.
pub async fn get(store: Option<&Arc<dyn ObjectStore>>, key: &str) -> Result<Bytes, ObjectError> {
    let store = store.ok_or(ObjectError::Unconfigured)?;
    Ok(store.get(&Path::from(key)).await?.bytes().await?)
}

/// Every key under a prefix, sorted so a listing is deterministic.
pub async fn list(
    store: Option<&Arc<dyn ObjectStore>>,
    prefix: &str,
) -> Result<Vec<String>, ObjectError> {
    use futures_util::StreamExt;

    let Some(store) = store else {
        return Ok(Vec::new());
    };
    let mut stream = store.list(Some(&Path::from(prefix.trim_end_matches('/'))));
    let mut keys = Vec::new();
    while let Some(item) = stream.next().await {
        keys.push(item?.location.to_string());
    }
    keys.sort();
    Ok(keys)
}

/// A JSON object, serialised the way the old service wrote it (no padding, one trailing newline for
/// the history records, none for a proof).
pub async fn put_json(
    store: Option<&Arc<dyn ObjectStore>>,
    key: &str,
    value: &Value,
    newline: bool,
) -> Result<(), ObjectError> {
    let mut body = serde_json::to_vec(value).map_err(|error| {
        ObjectError::Upstream(object_store::Error::Generic {
            store: "json",
            source: Box::new(error),
        })
    })?;
    if newline {
        body.push(b'\n');
    }
    put(store, key, &body, "application/json").await
}

/// The timestamp a key carries, from the object's own last-modified time. Used only for evidence
/// that a listing preserved it.
pub fn age(modified: chrono::DateTime<chrono::Utc>) -> Duration {
    let now = chrono::Utc::now();
    (now - modified).to_std().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use object_store::memory::InMemory;

    fn memory() -> Arc<dyn ObjectStore> {
        Arc::new(InMemory::new())
    }

    #[tokio::test]
    async fn a_write_is_readable_under_the_key_the_old_service_used() {
        let store = memory();
        let summary = serde_json::json!({"hello": "world"});
        put_json(
            Some(&store),
            "proofs/claim-1/20260101T000000Z.json",
            &summary,
            false,
        )
        .await
        .expect("written");
        let body = get(Some(&store), "proofs/claim-1/20260101T000000Z.json")
            .await
            .expect("read");
        assert_eq!(body.as_ref(), br#"{"hello":"world"}"#);
    }

    #[tokio::test]
    async fn a_listing_is_sorted_and_prefixed() {
        let store = memory();
        for key in [
            "history/t/e/2.jsonl",
            "history/t/e/1.jsonl",
            "history/t/other/1.jsonl",
        ] {
            put(Some(&store), key, b"{}", "application/json")
                .await
                .expect("written");
        }
        let keys = list(Some(&store), "history/t/e/").await.expect("list");
        assert_eq!(keys, vec!["history/t/e/1.jsonl", "history/t/e/2.jsonl"]);
    }

    #[tokio::test]
    async fn an_unconfigured_store_reads_empty_and_refuses_a_write() {
        assert!(list(None, "history/").await.expect("empty").is_empty());
        let error = put(None, "k", b"{}", "application/json")
            .await
            .expect_err("refused");
        assert!(matches!(error, ObjectError::Unconfigured));
    }
}
