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
    /// The client the tenant gate introspects licences with (same one the downstream clients use).
    pub http: reqwest::Client,
    /// The evidence-review database. `None` when `DATABASE_URL` is absent; every review route
    /// refuses by name rather than reading an unbound session (an unbound tenant makes every RLS
    /// policy match nothing, which is indistinguishable from "no data").
    pub pool: Option<sqlx::PgPool>,
    /// Verifies `X-Actor-Token`. `None` when the actor realm is unconfigured, and every actor
    /// token is then refused — never accepted unchecked.
    pub actor_verifier: Option<crate::actor_jwt::ActorVerifier>,
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
        let pool = config.database_url.as_deref().map(crate::db::connect_lazy);
        let actor_verifier = match crate::actor_jwt::ActorVerifier::from_config(&config) {
            Ok(verifier) => verifier,
            Err(error) => {
                // Fail closed and loudly: a realm is configured but its JWKS client cannot be
                // built, so no actor token will verify. Refusing every human is correct here.
                tracing::error!(%error, "actor verifier is misconfigured; actor tokens will be refused");
                None
            }
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
            http,
            pool,
            actor_verifier,
        }
    }

    /// State for tests: a database and an actor verifier supplied by the harness.
    pub fn for_tests(
        config: Config,
        pool: Option<sqlx::PgPool>,
        actor_verifier: Option<crate::actor_jwt::ActorVerifier>,
    ) -> Self {
        let mut state = Self::new(config);
        state.pool = pool;
        state.actor_verifier = actor_verifier;
        state
    }
}
