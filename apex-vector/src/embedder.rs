use anyhow::{Error as E, Result};
use candle_core::{Device, Tensor, DType};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config};
use hf_hub::{api::sync::Api, Repo, RepoType};
use tokenizers::{PaddingParams, Tokenizer};
use html2text::from_read;

#[derive(Clone, Debug)]
pub struct EmbeddingModelConfig {
    pub repo_id: String,
    pub revision: String,
    pub config_file: String,
    pub tokenizer_file: String,
    pub weights_file: String,
}

impl Default for EmbeddingModelConfig {
    fn default() -> Self {
        Self {
            repo_id: "sentence-transformers/all-MiniLM-L6-v2".to_string(),
            revision: "main".to_string(),
            config_file: "config.json".to_string(),
            tokenizer_file: "tokenizer.json".to_string(),
            weights_file: "model.safetensors".to_string(),
        }
    }
}

impl EmbeddingModelConfig {
    pub fn bge_small_en_v1_5() -> Self {
        Self {
            repo_id: "BAAI/bge-small-en-v1.5".to_string(),
            ..Default::default()
        }
    }

    pub fn bge_base_en_v1_5() -> Self {
        Self {
            repo_id: "BAAI/bge-base-en-v1.5".to_string(),
            ..Default::default()
        }
    }

    pub fn gte_small() -> Self {
        Self {
            repo_id: "thenlper/gte-small".to_string(),
            ..Default::default()
        }
    }
}

pub struct CandleEmbedder {
    model: BertModel,
    tokenizer: Tokenizer,
    device: Device,
}

impl CandleEmbedder {
    pub fn new(config: EmbeddingModelConfig) -> Result<Self> {
        let device = Device::Cpu; // Use CUDA if available

        let api = Api::new()?;
        let repo = api.repo(Repo::new(config.repo_id, RepoType::Model));

        let config_filename = repo.get(&config.config_file)?;
        let tokenizer_filename = repo.get(&config.tokenizer_file)?;
        let weights_filename = repo.get(&config.weights_file)?;

        let config_str = std::fs::read_to_string(config_filename)?;
        let config: Config = serde_json::from_str(&config_str)?;

        let mut tokenizer = Tokenizer::from_file(tokenizer_filename).map_err(E::msg)?;
        
        // Use BatchLongest padding. 
        // IMPORTANT: Do NOT set global truncation here, we handle it manually in the loop.
        let pp = PaddingParams {
            strategy: tokenizers::PaddingStrategy::BatchLongest,
            ..Default::default()
        };
        tokenizer.with_padding(Some(pp));

        let vb = unsafe { 
            VarBuilder::from_mmaped_safetensors(&[weights_filename], DType::F32, &device)? 
        };
        
        let model = BertModel::load(vb, &config)?;

        Ok(Self {
            model,
            tokenizer,
            device,
        })
    }

    pub fn embed(&self, html_content: &str) -> Result<Vec<f32>> {
        // 1. CLEAN HTML (Smartly)
        // Convert HTML to Text, preserving formatting (tables, code blocks) as Markdown-ish text.
        // We use usize::MAX width to prevent arbitrary line wrapping which hurts semantic meaning.
        let clean_text = from_read(html_content.as_bytes(), usize::MAX);

        // 2. TOKENIZE EVERYTHING
        // Encode the whole string. We disable truncation for this call to get ALL tokens.
        let tokens = self.tokenizer.encode(clean_text, true).map_err(E::msg)?;
        let token_ids = tokens.get_ids();
        let type_ids = tokens.get_type_ids(); // BERT needs token types too

        // 3. SLIDING WINDOW CONFIG
        let window_size = 512;
        let overlap = 128; // 25% overlap to keep context at boundaries
        let stride = window_size - overlap;
        
        let total_tokens = token_ids.len();
        
        // If short enough, just run once
        if total_tokens <= window_size {
            return self.run_model_pass(token_ids, type_ids);
        }

        // 4. PROCESS WINDOWS
        let mut accum_vector: Option<Vec<f32>> = None;
        let mut window_count = 0;
        let mut start_idx = 0;

        while start_idx < total_tokens {
            let end_idx = std::cmp::min(start_idx + window_size, total_tokens);
            
            // Extract window slices
            let window_token_ids = &token_ids[start_idx..end_idx];
            let window_type_ids = &type_ids[start_idx..end_idx];

            // Run Model
            let embedding = self.run_model_pass(window_token_ids, window_type_ids)?;

            // Accumulate (Summing vectors)
            if let Some(ref mut acc) = accum_vector {
                for (i, val) in embedding.iter().enumerate() {
                    acc[i] += val;
                }
            } else {
                accum_vector = Some(embedding);
            }
            
            window_count += 1;

            // Move sliding window
            if end_idx == total_tokens { break; } // Done
            start_idx += stride;
        }

        // 5. MEAN POOLING & NORMALIZE
        let mut final_vector = accum_vector.unwrap();
        
        // Divide by count (Average)
        let count_f32 = window_count as f32;
        for val in &mut final_vector {
            *val /= count_f32;
        }

        // Normalize (L2 Norm) to make it ready for Cosine Similarity
        let sum_squares: f32 = final_vector.iter().map(|v| v * v).sum();
        let magnitude = sum_squares.sqrt() + 1e-12; // Avoid divide by zero
        
        for val in &mut final_vector {
            *val /= magnitude;
        }

        Ok(final_vector)
    }

    // Helper: Runs a single forward pass on specific token IDs
    fn run_model_pass(&self, token_ids: &[u32], type_ids: &[u32]) -> Result<Vec<f32>> {
        let token_tensor = Tensor::new(token_ids, &self.device)?.unsqueeze(0)?;
        let type_tensor = Tensor::new(type_ids, &self.device)?.unsqueeze(0)?;

        let embeddings = self.model.forward(&token_tensor, &type_tensor)?;
        
        // Mean pooling logic inside Candle
        let (_n_sentence, n_tokens, _hidden_size) = embeddings.dims3()?;
        let pooled = (embeddings.sum(1)? / (n_tokens as f64))?;
        
        // L2 Normalize individual pass
        let normalized = normalize_l2(&pooled)?;
        
        let vec = normalized.squeeze(0)?.to_vec1::<f32>()?;
        Ok(vec)
    }
}

fn normalize_l2(v: &Tensor) -> Result<Tensor> {
    let sum_squares = v.sqr()?.sum_all()?.to_scalar::<f32>()?;
    let inv_norm = 1.0 / (sum_squares.sqrt() + 1e-12);
    Ok((v * (inv_norm as f64))?)
}