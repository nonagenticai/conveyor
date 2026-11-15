//! `conveyor-research`: the captured `webscraper` contract, implemented as a
//! static-page scraper. Browser-only capabilities are refused by name.

pub mod contract;
pub mod extract;
pub mod fetch;
pub mod links;
pub mod media;
pub mod models;
pub mod router;
pub mod schema_extract;

pub use router::{AppState, Config, ROUTES, build_app, openapi};
