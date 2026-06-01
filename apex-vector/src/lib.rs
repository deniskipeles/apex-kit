use anyhow::Result;
use std::sync::Arc;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod embedder;
pub mod index;

// Re-export common types
pub use embedder::{CandleEmbedder, EmbeddingModelConfig};
pub use index::VectorIndex;

// A struct to hold both capabilities
#[derive(Clone)]
pub struct VectorEngine {
    pub embedder: Arc<CandleEmbedder>,
    pub index: Arc<VectorIndex>,
}

impl VectorEngine {
    pub async fn new(config: Option<EmbeddingModelConfig>) -> Result<Self> {
        let cfg = config.unwrap_or_default();
        let embedder = Arc::new(CandleEmbedder::new(cfg)?);
        let index = Arc::new(VectorIndex::new());

        Ok(Self { embedder, index })
    }
}
