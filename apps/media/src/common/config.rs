//! Per-image configuration. A media image keeps **no state** (ADR 0007 §2: one stateful plane —
//! conveyor holds the database, the queue and the store). The only thing an image is configured
//! with is where it may write its working bytes, and every durable byte crosses conveyor's own job
//! routes.

use std::path::PathBuf;

/// The image's name and its scratch directory.
#[derive(Debug, Clone)]
pub struct Config {
    /// The image name this process reports in its logs and its `--list` output.
    pub service: String,
    /// Scratch space. The pod's `emptyDir`; never a mounted store, because no store credential
    /// reaches a media pod (ADR 0007 §4).
    pub work_dir: PathBuf,
}

impl Config {
    pub fn from_env(service: &str) -> Self {
        let work_dir = std::env::var("MEDIA_WORK_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| std::env::temp_dir().join("conveyor-media"));
        Self {
            service: service.to_string(),
            work_dir,
        }
    }
}
