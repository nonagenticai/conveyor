//! The process state the HTTP handlers see.
//!
//! The pool is optional so a unit test can build the router without a database. The binary always
//! supplies one: the service refuses to start without its database, as the Python service does when
//! its boot migrations fail.

use sqlx::PgPool;

#[derive(Clone)]
pub struct AppState {
    pub pool: Option<PgPool>,
    pub environment: String,
    pub version: String,
}

impl AppState {
    pub fn new(pool: PgPool, environment: String, version: String) -> Self {
        Self {
            pool: Some(pool),
            environment,
            version,
        }
    }
}
