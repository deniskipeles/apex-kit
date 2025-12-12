pub mod embedder;
pub mod index;

use std::sync::Arc;
use anyhow::Result;

// Re-export common types
pub use embedder::CandleEmbedder;
pub use index::VectorIndex;

// A struct to hold both capabilities
#[derive(Clone)]
pub struct VectorEngine {
    pub embedder: Arc<CandleEmbedder>,
    pub index: Arc<VectorIndex>,
}

impl VectorEngine {
    pub async fn new() -> Result<Self> {
        let embedder = Arc::new(CandleEmbedder::new()?);
        let index = Arc::new(VectorIndex::new());
        
        Ok(Self {
            embedder,
            index,
        })
    }
}