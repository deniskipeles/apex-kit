pub mod auth;
pub mod config;
pub mod database;
pub mod embeddings;
pub mod error;
pub mod query;
pub mod realtime;
pub mod scripting;
pub mod search;
pub mod security;
pub mod storage;
pub mod utils;
pub mod validation;
pub mod workers;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub use config::COMPOSITE_SEPARATOR;
pub use database::{
    batching, cache, models,
    sqlite::connections::ApexKit,
    traits::{Db, VectorProvider},
};
pub use scripting::ScriptContext;
