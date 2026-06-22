use anyhow::{Context, Error as E, Result};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use hf_hub::{Repo, RepoType, api::sync::ApiBuilder};
use html2text::from_read;
use std::sync::Mutex;
use tokenizers::{PaddingParams, Tokenizer};

use candle_transformers::models::bert::{BertModel, Config as BertConfig};

use crate::models::gemma_embed::{GemmaEmbedConfig, GemmaEmbedModel, dump_tensor_names, masked_mean_pool};
use crate::models::siglip2::{Siglip2VisionModel, SiglipVisionConfig};

/// EmbeddingGemma uses task-specific prompt prefixes rather than a bare string.
/// Document/passage embeddings (what you store and search against) use one prefix;
/// search queries use another. Picking the wrong one measurably hurts retrieval quality,
/// so this is exposed as an explicit choice rather than baked into `embed()` silently.
#[derive(Clone, Copy, Debug)]
pub enum GemmaTaskPrefix {
    /// For text you are indexing / storing as a retrievable document.
    Document,
    /// For the incoming search query at retrieval time.
    Query,
    /// No prefix - use only if you know your checkpoint wasn't tuned with prefixes.
    None,
}

impl GemmaTaskPrefix {
    fn apply(&self, text: &str) -> String {
        match self {
            GemmaTaskPrefix::Document => format!("title: none | text: {text}"),
            GemmaTaskPrefix::Query => format!("task: search result | query: {text}"),
            GemmaTaskPrefix::None => text.to_string(),
        }
    }
}

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
        Self {
            repo_id: "BAAI/bge-small-en-v1.5".to_string(),
            window_size: 512,
            overlap: 128,
            ..Default::default()
        }
    }
    pub fn bge_base_en_v1_5() -> Self {
        Self {
            repo_id: "BAAI/bge-base-en-v1.5".to_string(),
            window_size: 512,
            overlap: 128,
            ..Default::default()
        }
    }
    pub fn gte_small() -> Self {
        Self {
            repo_id: "thenlper/gte-small".to_string(),
            window_size: 512,
            overlap: 128,
            ..Default::default()
        }
    }

    pub fn gemma_300m() -> Self {
        Self {
            repo_id: "google/embeddinggemma-300m".to_string(),
            // EmbeddingGemma's real context window; lower this if you hit memory limits.
            window_size: 2048,
            overlap: 256,
            ..Default::default()
        }
    }

    pub fn custom(
        repo_id: String,
        revision: String,
        config_file: String,
        tokenizer_file: String,
        weights_file: String,
        window_size: usize,
        overlap: usize,
    ) -> Self {
        Self {
            repo_id,
            revision,
            config_file,
            tokenizer_file,
            weights_file,
            window_size,
            overlap,
        }
    }
}

enum ModelBackend {
    Bert(Box<BertModel>),
    Gemma(Box<GemmaEmbedModel>),
}

pub struct CandleEmbedder {
    backend: Mutex<ModelBackend>,
    vision_backend: Mutex<Option<Siglip2VisionModel>>,
    tokenizer: Tokenizer,
    device: Device,
    config: EmbeddingModelConfig,
    is_gemma: bool,
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

        if lower_repo.contains("qwen") {
            tracing::warn!("Qwen models are not implemented yet! Defaulting to all-MiniLM-L6-v2.");
            actual_config = EmbeddingModelConfig::default();
        }

        let api_builder = ApiBuilder::new();
        let api = if let Ok(token) = std::env::var("HF_TOKEN") {
            api_builder.with_token(Some(token)).build()?
        } else {
            api_builder.build()?
        };

        let repo = api.repo(Repo::new(actual_config.repo_id.clone(), RepoType::Model));

        let tokenizer_filename = repo.get(&actual_config.tokenizer_file)?;
        let weights_filename = repo.get(&actual_config.weights_file)?;
        let config_filename = repo.get(&actual_config.config_file)?;
        let config_str = std::fs::read_to_string(config_filename)?;
        let raw_config: serde_json::Value = serde_json::from_str(&config_str)?;

        let is_gemma = lower_repo.contains("gemma");

        let backend = if is_gemma {
            tracing::info!("Apex Vector: Loading EmbeddingGemma transformer (full forward pass).");
            let cfg: GemmaEmbedConfig = serde_json::from_value(raw_config).with_context(|| {
                "failed to parse Gemma config.json - check field names against GemmaEmbedConfig"
            })?;

            let vb = unsafe {
                VarBuilder::from_mmaped_safetensors(
                    std::slice::from_ref(&weights_filename),
                    DType::F32,
                    &device,
                )?
            };

            let model = match GemmaEmbedModel::load(vb, cfg, &device) {
                Ok(m) => m,
                Err(e) => {
                    tracing::error!("Gemma load failed ({e}). Dumping tensor names from safetensors for debugging:");
                    if let Ok(raw_tensors) = candle_core::safetensors::load(&weights_filename, &device) {
                        dump_tensor_names(&raw_tensors);
                    }
                    return Err(e);
                }
            };
            ModelBackend::Gemma(Box::new(model))
        } else {
            tracing::info!("Apex Vector: Loading BERT-style text model");
            let vb = unsafe {
                VarBuilder::from_mmaped_safetensors(
                    std::slice::from_ref(&weights_filename),
                    DType::F32,
                    &device,
                )?
            };
            let cfg: BertConfig = serde_json::from_value(raw_config)?;
            let model = BertModel::load(vb, &cfg)?;
            ModelBackend::Bert(Box::new(model))
        };

        let tokenizer_bytes = std::fs::read(&tokenizer_filename)?;
        let mut tokenizer = Tokenizer::from_bytes(&tokenizer_bytes)
            .map_err(|e| anyhow::anyhow!("Tokenizer Parse Error: {}", e))?;

        let pp = PaddingParams {
            strategy: tokenizers::PaddingStrategy::BatchLongest,
            ..Default::default()
        };
        tokenizer.with_padding(Some(pp));

        Ok(Self {
            backend: Mutex::new(backend),
            vision_backend: Mutex::new(None),
            tokenizer,
            device,
            config: actual_config,
            is_gemma,
        })
    }

    /// Embed a document/passage for storage and search. Equivalent to the old `embed()`
    /// signature but now routes through correct masked pooling and (for Gemma) the
    /// document task-prefix automatically.
    pub fn embed(&self, html_content: &str) -> Result<Vec<f32>> {
        self.embed_with_task(html_content, GemmaTaskPrefix::Document)
    }

    /// Embed a search query. Use this at query time, not at indexing time - EmbeddingGemma
    /// was trained with different prefixes for queries vs documents and mixing them up
    /// will quietly hurt ranking quality even though nothing errors.
    pub fn embed_query(&self, query_text: &str) -> Result<Vec<f32>> {
        self.embed_with_task(query_text, GemmaTaskPrefix::Query)
    }

    fn embed_with_task(&self, html_content: &str, task: GemmaTaskPrefix) -> Result<Vec<f32>> {
        let clean_text = from_read(html_content.as_bytes(), usize::MAX);
        let clean_text = if self.is_gemma {
            task.apply(&clean_text)
        } else {
            clean_text
        };

        let encoding = self.tokenizer.encode(clean_text, true).map_err(E::msg)?;
        let token_ids = encoding.get_ids();

        let window_size = self.config.window_size;
        let overlap = self.config.overlap;
        let stride = if window_size > overlap {
            window_size - overlap
        } else {
            window_size / 2
        };
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
                for (i, val) in embedding.iter().enumerate() {
                    acc[i] += val;
                }
            } else {
                accum_vector = Some(embedding);
            }
            window_count += 1;
            if end_idx == total_tokens {
                break;
            }
            start_idx += stride;
        }

        let mut final_vector = accum_vector.unwrap();
        let count_f32 = window_count as f32;
        for val in &mut final_vector {
            *val /= count_f32;
        }

        l2_normalize_in_place(&mut final_vector);
        Ok(final_vector)
    }

    /// Runs a single forward pass for a window of token ids that's already <= window_size.
    /// NOTE: this path builds its own attention mask of all-1s because a single window
    /// (pre-batching) has no padding. The real padding fix lives in
    /// `run_model_pass_with_mask`, used when you batch multiple texts together - see below.
    fn run_model_pass(&self, token_ids: &[u32]) -> Result<Vec<f32>> {
        let attention_mask: Vec<u32> = vec![1; token_ids.len()];
        self.run_model_pass_with_mask(token_ids, &attention_mask)
    }

    /// Attention-mask-aware forward pass. This is the fix for the original bug: padding
    /// tokens are excluded from both the model's attention computation (for BERT, via the
    /// mask passed into `forward`) and from the pooling average (for both backends, via
    /// `masked_mean_pool` / manual masked sum-then-divide). Previously padding tokens were
    /// silently included in both, diluting every embedding produced from a padded batch.
    fn run_model_pass_with_mask(&self, token_ids: &[u32], attention_mask: &[u32]) -> Result<Vec<f32>> {
        let mut backend_lock = self.backend.lock().unwrap();

        match &mut *backend_lock {
            ModelBackend::Bert(m) => {
                let token_tensor = Tensor::new(token_ids, &self.device)?.unsqueeze(0)?;
                let type_tensor = Tensor::zeros_like(&token_tensor)?;
                let mask_tensor = Tensor::new(attention_mask, &self.device)?
                    .unsqueeze(0)?
                    .to_dtype(DType::F32)?;

                // THE FIX: pass the real attention mask instead of None, so padding tokens
                // don't get attended to.
                let hidden = m.forward(&token_tensor, &type_tensor, Some(&mask_tensor))?;

                // THE FIX: masked mean pool - divide by the count of REAL tokens, not the
                // padded sequence length.
                let pooled = masked_mean_pool(&hidden, &mask_tensor)?;
                let normalized = normalize_l2(&pooled)?;
                let vec = normalized.squeeze(0)?.to_vec1::<f32>()?;
                Ok(vec)
            }
            ModelBackend::Gemma(m) => {
                let token_tensor = Tensor::new(token_ids, &self.device)?.unsqueeze(0)?;
                let mask_tensor = Tensor::new(attention_mask, &self.device)?.unsqueeze(0)?;

                // Real Gemma transformer forward pass (full attention + MLP stack),
                // not a raw embedding-table lookup.
                let hidden = m.forward(&token_tensor, &mask_tensor)?;
                let mask_f32 = mask_tensor.to_dtype(DType::F32)?;
                let pooled = masked_mean_pool(&hidden, &mask_f32)?;
                let normalized = normalize_l2(&pooled)?;
                let vec = normalized.squeeze(0)?.to_vec1::<f32>()?;
                Ok(vec)
            }
        }
    }

    /// Batch-embed multiple documents in one padded batch. This is where the old attention
    /// mask bug would have silently corrupted results the most, since `BatchLongest`
    /// padding means shorter texts in the batch get diluted by pad tokens unless the mask
    /// is correctly threaded through both attention and pooling.
    pub fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let prefixed: Vec<String> = if self.is_gemma {
            texts.iter().map(|t| GemmaTaskPrefix::Document.apply(t)).collect()
        } else {
            texts.to_vec()
        };

        let encodings = self
            .tokenizer
            .encode_batch(prefixed, true)
            .map_err(E::msg)?;

        let mut out = Vec::with_capacity(encodings.len());
        for enc in &encodings {
            let ids = enc.get_ids();
            let mask = enc.get_attention_mask();
            out.push(self.run_model_pass_with_mask(ids, mask)?);
        }
        Ok(out)
    }

    pub fn embed_image(&self, base64_image: &str) -> Result<Vec<f32>> {
        let mut vision_lock = self.vision_backend.lock().unwrap();

        if vision_lock.is_none() {
            tracing::info!("Apex Vector: Lazy-loading SigLIP2 vision model for image vectorization...");
            let api = ApiBuilder::new().build()?;
            let repo = api.repo(Repo::new(
                "google/siglip2-base-patch16-224".to_string(),
                RepoType::Model,
            ));

            let weights = repo.get("model.safetensors")?;
            let vision_config = SiglipVisionConfig::siglip2_base_patch16_224();

            let vb = unsafe {
                VarBuilder::from_mmaped_safetensors(
                    std::slice::from_ref(&weights),
                    DType::F32,
                    &self.device,
                )?
            };

            let model = match Siglip2VisionModel::load(vb, vision_config) {
                Ok(m) => m,
                Err(e) => {
                    tracing::error!("SigLIP2 load failed ({e}). Dumping tensor names for debugging:");
                    if let Ok(raw_tensors) = candle_core::safetensors::load(&weights, &self.device) {
                        dump_tensor_names(&raw_tensors);
                    }
                    return Err(e);
                }
            };
            *vision_lock = Some(model);
        }

        let model = vision_lock.as_ref().unwrap();
        let cfg = model.config();
        let image_size = cfg.image_size;

        let b64 = if let Some(idx) = base64_image.find(',') {
            &base64_image[idx + 1..]
        } else {
            base64_image
        };

        let image_bytes = STANDARD.decode(b64).map_err(E::msg)?;
        let img = image::load_from_memory(&image_bytes).map_err(E::msg)?;

        let resized = img.resize_exact(
            image_size as u32,
            image_size as u32,
            image::imageops::FilterType::Triangle,
        );
        let rgb = resized.to_rgb8();

        // SigLIP preprocessing: scale to [-1, 1] (mean=0.5, std=0.5 per channel), not the
        // ImageNet/CLIP mean-std normalization used previously.
        let mut data = Vec::with_capacity(3 * image_size * image_size);
        for c in 0..3 {
            for y in 0..image_size as u32 {
                for x in 0..image_size as u32 {
                    let pixel = rgb.get_pixel(x, y);
                    let val = pixel[c] as f32 / 255.0;
                    let norm_val = (val - 0.5) / 0.5;
                    data.push(norm_val);
                }
            }
        }

        let tensor = Tensor::from_vec(data, (3, image_size, image_size), &self.device)?.unsqueeze(0)?;
        let pooled = model.forward(&tensor)?;

        let normalized = normalize_l2(&pooled)?;
        let vec = normalized.squeeze(0)?.to_vec1::<f32>()?;

        Ok(vec)
    }
}

fn l2_normalize_in_place(v: &mut [f32]) {
    let sum_squares: f32 = v.iter().map(|x| x * x).sum();
    let magnitude = sum_squares.sqrt() + 1e-12;
    for val in v.iter_mut() {
        *val /= magnitude;
    }
}

fn normalize_l2(v: &Tensor) -> Result<Tensor> {
    let sum_squares = v.sqr()?.sum_keepdim(candle_core::D::Minus1)?;
    let norm = sum_squares.sqrt()?;
    let eps = Tensor::new(1e-12f32, v.device())?;
    let norm_eps = norm.broadcast_add(&eps)?;
    let inv_norm = norm_eps.recip()?;
    Ok(v.broadcast_mul(&inv_norm)?)
}
