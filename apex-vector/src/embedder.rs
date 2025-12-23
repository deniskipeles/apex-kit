use anyhow::{Error as E, Result};
use candle_core::{Device, Tensor, DType};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config};
use hf_hub::{api::sync::Api, Repo, RepoType};
use tokenizers::{PaddingParams, Tokenizer};

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
        let device = Device::Cpu;

        let api = Api::new()?;
        let repo = api.repo(Repo::new(
            config.repo_id,
            RepoType::Model,
        ));

        let config_filename = repo.get(&config.config_file)?;
        let tokenizer_filename = repo.get(&config.tokenizer_file)?;
        let weights_filename = repo.get(&config.weights_file)?;

        let config_str = std::fs::read_to_string(config_filename)?;
        let config: Config = serde_json::from_str(&config_str)?;

        let mut tokenizer = Tokenizer::from_file(tokenizer_filename).map_err(E::msg)?;
        let pp = PaddingParams {
            strategy: tokenizers::PaddingStrategy::BatchLongest,
            ..Default::default()
        };
        tokenizer.with_padding(Some(pp));

        let vb = unsafe { 
            VarBuilder::from_mmaped_safetensors(
                &[weights_filename], 
                DType::F32, 
                &device
            )? 
        };
        
        // FIX: Use load instead of new
        let model = BertModel::load(vb, &config)?;

        Ok(Self {
            model,
            tokenizer,
            device,
        })
    }

    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let tokens = self.tokenizer.encode(text, true).map_err(E::msg)?;
        
        let token_ids = Tensor::new(tokens.get_ids(), &self.device)?.unsqueeze(0)?;
        let token_type_ids = Tensor::new(tokens.get_type_ids(), &self.device)?.unsqueeze(0)?;

        let embeddings = self.model.forward(&token_ids, &token_type_ids)?;
        
        let (_n_sentence, n_tokens, _hidden_size) = embeddings.dims3()?;
        let embeddings = (embeddings.sum(1)? / (n_tokens as f64))?;
        
        let embeddings = normalize_l2(&embeddings)?;
        
        let vec = embeddings.squeeze(0)?.to_vec1::<f32>()?;
        Ok(vec)
    }
}

fn normalize_l2(v: &Tensor) -> Result<Tensor> {
    let _elem_count = v.elem_count();
    let sum_squares = v.sqr()?.sum_all()?.to_scalar::<f32>()?;
    let inv_norm = 1.0 / (sum_squares.sqrt() + 1e-12);
    Ok((v * (inv_norm as f64))?)
}