//! What both images share: configuration, the capability error, and the command line.

pub mod config;
pub mod entry;
pub mod error;
pub mod process;

pub use config::Config;
pub use error::CapabilityError;
