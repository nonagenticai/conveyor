//! The process state every handler shares.

use std::sync::Arc;
use std::time::Instant;

use object_store::ObjectStore;

use crate::cache::Cache;
use crate::clients::embeddings::EmbeddingClient;
use crate::clients::inference::InferenceClient;
use crate::clients::lean4::Lean4Client;
use crate::clients::objects;
use crate::config::Config;
use crate::pipeline::PipelineState;

/// The prover is slow by nature — a proof search is the work — so its client gets a longer
/// deadline than the model router's.
const LEAN4_TIMEOUT_SECONDS: u64 = 180;

#[derive(Debug)]
pub struct AppState {
    pub config: Arc<Config>,
    pub cache: Arc<Cache>,
    pub started: Instant,
    pub lean4: Option<Lean4Client>,
    pub inference: Option<InferenceClient>,
    /// The `pytorch-backend` route the two vector metrics call.
    pub embeddings: Option<EmbeddingClient>,
    /// `None` when no bucket is configured (or its client cannot be built), which every read route
    /// treats as "empty" and every write route refuses by name.
    pub objects: Option<Arc<dyn ObjectStore>>,
    pub pipeline: PipelineState,
}

pub type Shared = Arc<AppState>;

impl AppState {
    pub fn new(config: Config) -> Self {
        let cache = Cache::new(config.cache_ttl, config.cache_max_entries);
        let http = reqwest::Client::builder()
            .user_agent(concat!("conveyor-scientific/", env!("CARGO_PKG_VERSION")))
            .build()
            .unwrap_or_default();
        let lean4 = Lean4Client::new(
            config.lean4_prover_url.as_deref(),
            config.downstream_license_token.as_deref(),
            http.clone(),
            LEAN4_TIMEOUT_SECONDS,
        );
        let inference = InferenceClient::new(config.inference_router_url.as_deref(), http.clone());
        let embeddings = EmbeddingClient::new(config.pytorch_backend_url.as_deref(), http.clone());
        let objects = match config.objects.as_ref().map(objects::build) {
            Some(Ok(store)) => Some(store),
            Some(Err(error)) => {
                // Boot does not fail: an unreachable bucket must not take the statistics routes
                // down with it. The write routes say which variables to check.
                tracing::error!(%error, "object store is configured but unusable; proofs and history will be empty");
                None
            }
            None => None,
        };
        Self {
            config: Arc::new(config),
            cache: Arc::new(cache),
            started: Instant::now(),
            lean4,
            inference,
            embeddings,
            objects,
            pipeline: PipelineState::default(),
        }
    }
}
