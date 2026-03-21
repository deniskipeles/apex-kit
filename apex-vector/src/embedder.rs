// ============================================================================
// FILE: apex-vector/src/embedder.rs
// ============================================================================

use anyhow::{Error as E, Result};
use candle_core::{Device, Tensor, DType, Module};
use candle_nn::VarBuilder;
use hf_hub::{api::sync::ApiBuilder, Repo, RepoType};
use tokenizers::{PaddingParams, Tokenizer};
use html2text::from_read;
use std::sync::Mutex;
use base64::{Engine as _, engine::general_purpose::STANDARD};

use candle_transformers::models::bert::{BertModel, Config as BertConfig};
// Import the Activation enum specifically from the clip models
use candle_transformers::models::clip::text_model::Activation;
use candle_transformers::models::clip::vision_model::{ClipVisionTransformer, ClipVisionConfig};

#[derive(Clone, Debug)]
pub struct EmbeddingModelConfig {
    pub repo_id: String,
    pub revision: String,
    pub config_file: String,
    pub tokenizer_file: String,
    pub weights_file: String,
    pub window_size: usize,
    pub overlap: usize,
}

impl Default for EmbeddingModelConfig {
    fn default() -> Self {
        Self {
            repo_id: "sentence-transformers/all-MiniLM-L6-v2".to_string(),
            revision: "main".to_string(),
            config_file: "config.json".to_string(),
            tokenizer_file: "tokenizer.json".to_string(),
            weights_file: "model.safetensors".to_string(),
            window_size: 512,
            overlap: 128,
        }
    }
}

impl EmbeddingModelConfig {
    pub fn bge_small_en_v1_5() -> Self { 
        Self { repo_id: "BAAI/bge-small-en-v1.5".to_string(), window_size: 512, overlap: 128, ..Default::default() } 
    }
    pub fn bge_base_en_v1_5() -> Self { 
        Self { repo_id: "BAAI/bge-base-en-v1.5".to_string(), window_size: 512, overlap: 128, ..Default::default() } 
    }
    pub fn gte_small() -> Self { 
        Self { repo_id: "thenlper/gte-small".to_string(), window_size: 512, overlap: 128, ..Default::default() } 
    }
    
    pub fn custom(
        repo_id: String, revision: String, config_file: String, 
        tokenizer_file: String, weights_file: String, 
        window_size: usize, overlap: usize
    ) -> Self {
        Self { repo_id, revision, config_file, tokenizer_file, weights_file, window_size, overlap }
    }
}

enum ModelBackend {
    Bert(BertModel),
}

pub struct CandleEmbedder {
    backend: Mutex<ModelBackend>,
    clip_backend: Mutex<Option<ClipVisionTransformer>>, 
    tokenizer: Tokenizer,
    device: Device,
    config: EmbeddingModelConfig,
}

fn get_best_device() -> Result<Device> {
    if candle_core::utils::cuda_is_available() {
        tracing::info!("Apex Vector: NVIDIA CUDA detected. Offloading to GPU...");
        return Ok(Device::new_cuda(0)?);
    } 
    if candle_core::utils::metal_is_available() {
        tracing::info!("Apex Vector: Apple Silicon (Metal) detected. Offloading to GPU...");
        return Ok(Device::new_metal(0)?);
    }
    tracing::info!("Apex Vector: No compatible GPU found. Running on CPU.");
    Ok(Device::Cpu)
}

impl CandleEmbedder {
    pub fn new(config: EmbeddingModelConfig) -> Result<Self> {
        let device = get_best_device().unwrap_or(Device::Cpu);
        
        let mut actual_config = config.clone();
        let lower_repo = actual_config.repo_id.to_lowercase();
        
        if lower_repo.contains("qwen") || lower_repo.contains("gemma") {
            tracing::warn!("Qwen and Gemma models are not implemented yet! Defaulting to all-MiniLM-L6-v2.");
            actual_config = EmbeddingModelConfig::default();
        }
        
        let api_builder = ApiBuilder::new();
        let api = if let Ok(token) = std::env::var("HF_TOKEN") {
            api_builder.with_token(Some(token)).build()?
        } else {
            api_builder.build()?
        };
        
        let repo = api.repo(Repo::new(actual_config.repo_id.clone(), RepoType::Model));

        let config_filename = repo.get(&actual_config.config_file)?;
        let tokenizer_filename = repo.get(&actual_config.tokenizer_file)?;
        let weights_filename = repo.get(&actual_config.weights_file)?;

        let config_str = std::fs::read_to_string(config_filename)?;
        let raw_config: serde_json::Value = serde_json::from_str(&config_str)?;

        let vb = unsafe { VarBuilder::from_mmaped_safetensors(&[weights_filename.clone()], DType::F32, &device)? };

        tracing::info!("Apex Vector: Loading BERT-style text model");
        let cfg: BertConfig = serde_json::from_value(raw_config)?;
        let model = BertModel::load(vb, &cfg)?;
        let backend = ModelBackend::Bert(model);

        let tokenizer_bytes = std::fs::read(&tokenizer_filename)?;
        let mut tokenizer = Tokenizer::from_bytes(&tokenizer_bytes)
            .map_err(|e| anyhow::anyhow!("Tokenizer Parse Error: {}", e))?;
            
        let pp = PaddingParams { strategy: tokenizers::PaddingStrategy::BatchLongest, ..Default::default() };
        tokenizer.with_padding(Some(pp));

        Ok(Self {
            backend: Mutex::new(backend),
            clip_backend: Mutex::new(None),
            tokenizer,
            device,
            config: actual_config,
        })
    }

    pub fn embed(&self, html_content: &str) -> Result<Vec<f32>> {
        let clean_text = from_read(html_content.as_bytes(), usize::MAX);
        let tokens = self.tokenizer.encode(clean_text, true).map_err(E::msg)?;
        let token_ids = tokens.get_ids();

        let window_size = self.config.window_size;
        let overlap = self.config.overlap;
        let stride = if window_size > overlap { window_size - overlap } else { window_size / 2 };
        let total_tokens = token_ids.len();
        
        if total_tokens <= window_size {
            return self.run_model_pass(token_ids);
        }

        let mut accum_vector: Option<Vec<f32>> = None;
        let mut window_count = 0;
        let mut start_idx = 0;

        while start_idx < total_tokens {
            let end_idx = std::cmp::min(start_idx + window_size, total_tokens);
            let window_token_ids = &token_ids[start_idx..end_idx];
            let embedding = self.run_model_pass(window_token_ids)?;

            if let Some(ref mut acc) = accum_vector {
                for (i, val) in embedding.iter().enumerate() { acc[i] += val; }
            } else {
                accum_vector = Some(embedding);
            }
            window_count += 1;
            if end_idx == total_tokens { break; } 
            start_idx += stride;
        }

        let mut final_vector = accum_vector.unwrap();
        let count_f32 = window_count as f32;
        for val in &mut final_vector { *val /= count_f32; }
        let sum_squares: f32 = final_vector.iter().map(|v| v * v).sum();
        let magnitude = sum_squares.sqrt() + 1e-12; 
        for val in &mut final_vector { *val /= magnitude; }
        Ok(final_vector)
    }

    fn run_model_pass(&self, token_ids: &[u32]) -> Result<Vec<f32>> {
        let token_tensor = Tensor::new(token_ids, &self.device)?.unsqueeze(0)?;
        let mut backend_lock = self.backend.lock().unwrap();

        let embeddings = match &mut *backend_lock {
            ModelBackend::Bert(m) => {
                let type_tensor = Tensor::zeros_like(&token_tensor)?;
                m.forward(&token_tensor, &type_tensor, None)?
            }
        };
        
        let (_b, n_tokens, _h) = embeddings.dims3()?;
        let divisor = Tensor::new(n_tokens as f32, embeddings.device())?;
        let pooled = embeddings.sum(1)?.broadcast_div(&divisor)?;
        
        let normalized = normalize_l2(&pooled)?;
        let vec = normalized.squeeze(0)?.to_vec1::<f32>()?;
        Ok(vec)
    }

    pub fn embed_image(&self, base64_image: &str) -> Result<Vec<f32>> {
        let mut clip_lock = self.clip_backend.lock().unwrap();
        
        if clip_lock.is_none() {
            tracing::info!("Apex Vector: Lazy-loading CLIP model for image vectorization...");
            let api = ApiBuilder::new().build()?;
            let repo = api.repo(Repo::new("openai/clip-vit-base-patch32".to_string(), RepoType::Model));
            
            let weights = repo.get("model.safetensors")?;
            
            // Standard ClipVisionConfig values for Vit-base-patch32
            let vision_config = ClipVisionConfig {
                embed_dim: 768,
                intermediate_size: 3072,
                num_hidden_layers: 12,
                num_attention_heads: 12,
                num_channels: 3,
                image_size: 224,
                patch_size: 32,
                activation: Activation::QuickGelu,
                projection_dim: 512,
            };
            
            let vb = unsafe { VarBuilder::from_mmaped_safetensors(&[weights], DType::F32, &self.device)? };
            let vision_vb = vb.pp("vision_model");
            let model = ClipVisionTransformer::new(vision_vb, &vision_config)?;
            *clip_lock = Some(model);
        }
        
        let model = clip_lock.as_ref().unwrap();
        
        let b64 = if let Some(idx) = base64_image.find(',') {
            &base64_image[idx + 1..]
        } else {
            base64_image
        };
        
        let image_bytes = STANDARD.decode(b64).map_err(E::msg)?;
        let img = image::load_from_memory(&image_bytes).map_err(E::msg)?;
        
        let resized = img.resize_exact(224, 224, image::imageops::FilterType::Triangle);
        let rgb = resized.to_rgb8();
        
        let mut data = Vec::with_capacity(3 * 224 * 224);
        let mean = [0.48145466f32, 0.4578275, 0.40821073];
        let std = [0.26862954f32, 0.26130258, 0.27577711];
        
        for c in 0..3 {
            for y in 0..224 {
                for x in 0..224 {
                    let pixel = rgb.get_pixel(x, y);
                    let val = pixel[c] as f32 / 255.0;
                    let norm_val = (val - mean[c]) / std[c];
                    data.push(norm_val);
                }
            }
        }
        
        let tensor = Tensor::from_vec(data, (3, 224, 224), &self.device)?.unsqueeze(0)?;
        let embeddings = model.forward(&tensor)?;
        
        let normalized = normalize_l2(&embeddings)?;
        let vec = normalized.squeeze(0)?.to_vec1::<f32>()?;
        
        Ok(vec)
    }
}

fn normalize_l2(v: &Tensor) -> Result<Tensor> {
    let sum_squares = v.sqr()?.sum_all()?;
    let norm = sum_squares.sqrt()?;
    let eps = Tensor::new(1e-12f32, v.device())?;
    let norm_eps = (norm + eps)?;
    let inv_norm = norm_eps.recip()?;
    Ok(v.broadcast_mul(&inv_norm)?)
}