//! What the three binaries share: the error envelope, the health surface, config and the tenant.

pub mod config;
pub mod entry;
pub mod error;
pub mod health;
pub mod tenant;

pub use config::Config;
pub use error::ApiError;
