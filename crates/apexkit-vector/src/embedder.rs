use anyhow::{Context, Error as E, Result};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use hf_hub::{Repo, RepoType, api::sync::ApiBuilder};
use html2text::from_read;
use std::path::PathBuf;
use std::sync::Mutex;
use tokenizers::{PaddingParams, Tokenizer};

use candle_transformers::models::bert::{BertModel, Config as BertConfig};

use crate::models::gemma_embed::{GemmaEmbedConfig, GemmaEmbedModel, dump_tensor_names, masked_mean_pool};
use crate::models::onnx_vision::{OnnxVisionConfig, OnnxVisionEmbedder};
use crate::models::qwen_embed::{QwenEmbedConfig, QwenEmbedModel, last_token_pool};
use crate::models::siglip2::{Siglip2VisionModel, SiglipVisionConfig};

/// Which text-backbone family is currently loaded. Each family has its own pooling
/// strategy and prompt-prefix convention, so call sites branch on this rather than a
/// scattered pile of `is_gemma` / `is_qwen` booleans.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BackendKind {
    Bert,
    Gemma,
    Qwen,
}

/// Which side of a retrieval pair this text is - affects prompt prefixing differently
/// per backbone (see `apply_prefix` below).
#[derive(Clone, Copy, Debug)]
enum TaskKind {
    Document,
    Query,
}

/// EmbeddingGemma uses a prefix on BOTH sides (document and query), just different ones.
/// Qwen3-Embedding only prefixes the QUERY side; documents are embedded raw. BERT-family
/// models (MiniLM, BGE, GTE) get no prefix at all. Getting any of this backwards hurts
/// retrieval quality without ever throwing an error, so it's centralized here instead of
/// left for call sites to remember.
fn apply_prefix(kind: BackendKind, task: TaskKind, text: &str) -> String {
    match (kind, task) {
        (BackendKind::Gemma, TaskKind::Document) => format!("title: none | text: {text}"),
        (BackendKind::Gemma, TaskKind::Query) => format!("task: search result | query: {text}"),
        (BackendKind::Qwen, TaskKind::Document) => text.to_string(),
        (BackendKind::Qwen, TaskKind::Query) => format!(
            "Instruct: Given a query, retrieve relevant documents.\nQuery: {text}"
        ),
        (BackendKind::Bert, _) => text.to_string(),
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
            window_size: 2048,
            overlap: 256,
            ..Default::default()
        }
    }

    /// Qwen3-Embedding-0.6B - causal decoder backbone with last-token pooling. See
    /// `models/qwen_embed.rs` for the full forward-pass implementation and pooling notes.
    pub fn qwen3_embedding_0_6b() -> Self {
        Self {
            repo_id: "Qwen/Qwen3-Embedding-0.6B".to_string(),
            window_size: 8192,
            overlap: 512,
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
    Qwen(Box<QwenEmbedModel>),
}

// ---------------------------------------------------------------------------
// Vision backend selection
// ---------------------------------------------------------------------------
//
// Controlled by the APEXKIT_VISION_MODEL env var, read once at first use (the vision
// model is lazy-loaded on the first `embed_image` call, same as before):
//
//   unset / "siglip2-onnx"   -> DEFAULT. onnx-community/siglip2-base-patch16-384-ONNX,
//                               run through ONNX Runtime. Quantized weights, low RAM.
//   "tinyclip-onnx"          -> TinyCLIP-ViT-4M via ONNX Runtime. Needs
//                               APEXKIT_VISION_MODEL_REPO / APEXKIT_VISION_MODEL_FILE set
//                               to wherever you've hosted a verified ONNX export - see the
//                               caveat in models/onnx_vision.rs.
//   "mobileclip-onnx"        -> MobileCLIP-S0 via ONNX Runtime. Same caveat as above.
//   "candle-siglip2"         -> LEGACY. The original hand-rolled candle SigLIP2 path from
//                               before this change. Heavier (full F32 weights, custom
//                               transformer), kept only for environments that can't use
//                               ONNX Runtime for some reason.
//
// Additional overrides:
//   APEXKIT_VISION_MODEL_REPO  - override the HF repo id for the selected preset
//   APEXKIT_VISION_MODEL_FILE  - override the .onnx filename within that repo
//   APEXKIT_VISION_INPUT_NAME  - override the ONNX input tensor name (default "pixel_values")
enum VisionBackend {
    Onnx(OnnxVisionEmbedder),
    CandleSiglip2(Siglip2VisionModel),
}

struct VisionPreset {
    repo_id: String,
    file_name: String,
    onnx_cfg: OnnxVisionConfig,
}

fn resolve_vision_preset() -> VisionPreset {
    let selection = std::env::var("APEXKIT_VISION_MODEL").unwrap_or_else(|_| "siglip2-onnx".to_string());
    let repo_override = std::env::var("APEXKIT_VISION_MODEL_REPO").ok();
    let file_override = std::env::var("APEXKIT_VISION_MODEL_FILE").ok();
    let input_name_override = std::env::var("APEXKIT_VISION_INPUT_NAME").ok();

    let (default_repo, default_file, mut onnx_cfg) = match selection.as_str() {
        "tinyclip-onnx" => (
            "wkcn/TinyCLIP-ViT-4M-Text-3M".to_string(),
            "onnx/model.onnx".to_string(),
            OnnxVisionConfig::tinyclip_vit_4m(),
        ),
        "mobileclip-onnx" => (
            "apple/MobileCLIP".to_string(),
            "onnx/model.onnx".to_string(),
            OnnxVisionConfig::mobileclip_s0(),
        ),
        // "siglip2-onnx" and anything unrecognized fall back to the default preset.
        _ => (
            "onnx-community/siglip2-base-patch16-384-ONNX".to_string(),
            "onnx/model_quantized.onnx".to_string(),
            OnnxVisionConfig::siglip2_base_patch16_384_onnx(),
        ),
    };

    if let Some(name) = input_name_override {
        onnx_cfg.input_name = name;
    }

    VisionPreset {
        repo_id: repo_override.unwrap_or(default_repo),
        file_name: file_override.unwrap_or(default_file),
        onnx_cfg,
    }
}

pub struct CandleEmbedder {
    backend: Mutex<ModelBackend>,
    backend_kind: BackendKind,
    vision_backend: Mutex<Option<VisionBackend>>,
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

        let actual_config = config.clone();
        let lower_repo = actual_config.repo_id.to_lowercase();

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
        let is_qwen = lower_repo.contains("qwen");

        let (backend, backend_kind) = if is_gemma {
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
            (ModelBackend::Gemma(Box::new(model)), BackendKind::Gemma)
        } else if is_qwen {
            tracing::info!("Apex Vector: Loading Qwen embedding transformer (causal, last-token pooling).");
            let cfg: QwenEmbedConfig = serde_json::from_value(raw_config).with_context(|| {
                "failed to parse Qwen config.json - check field names against QwenEmbedConfig"
            })?;

            let vb = unsafe {
                VarBuilder::from_mmaped_safetensors(
                    std::slice::from_ref(&weights_filename),
                    DType::F32,
                    &device,
                )?
            };

            let model = match QwenEmbedModel::load(vb, cfg, &device) {
                Ok(m) => m,
                Err(e) => {
                    tracing::error!("Qwen load failed ({e}). Dumping tensor names from safetensors for debugging:");
                    if let Ok(raw_tensors) = candle_core::safetensors::load(&weights_filename, &device) {
                        dump_tensor_names(&raw_tensors);
                    }
                    return Err(e);
                }
            };
            (ModelBackend::Qwen(Box::new(model)), BackendKind::Qwen)
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
            (ModelBackend::Bert(Box::new(model)), BackendKind::Bert)
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
            backend_kind,
            vision_backend: Mutex::new(None),
            tokenizer,
            device,
            config: actual_config,
        })
    }

    /// Embed a document/passage for storage and search.
    pub fn embed(&self, html_content: &str) -> Result<Vec<f32>> {
        self.embed_with_task(html_content, TaskKind::Document)
    }

    /// Embed a search query. Use this at query time, not at indexing time - both
    /// EmbeddingGemma and Qwen3-Embedding were trained with different text on the query
    /// side vs the document side, and mixing them up will quietly hurt ranking quality
    /// even though nothing errors.
    pub fn embed_query(&self, query_text: &str) -> Result<Vec<f32>> {
        self.embed_with_task(query_text, TaskKind::Query)
    }

    fn embed_with_task(&self, html_content: &str, task: TaskKind) -> Result<Vec<f32>> {
        let clean_text = from_read(html_content.as_bytes(), usize::MAX);
        let clean_text = apply_prefix(self.backend_kind, task, &clean_text);

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

    /// Single window, no padding (attention mask is all-1s).
    fn run_model_pass(&self, token_ids: &[u32]) -> Result<Vec<f32>> {
        let attention_mask: Vec<u32> = vec![1; token_ids.len()];
        self.run_model_pass_with_mask(token_ids, &attention_mask)
    }

    /// Attention-mask-aware forward pass, branching on backend-specific pooling:
    ///   - BERT / Gemma: bidirectional, masked MEAN pooling over real tokens.
    ///   - Qwen: causal, LAST-real-token pooling (see models/qwen_embed.rs for why).
    fn run_model_pass_with_mask(&self, token_ids: &[u32], attention_mask: &[u32]) -> Result<Vec<f32>> {
        let mut backend_lock = self.backend.lock().unwrap();

        match &mut *backend_lock {
            ModelBackend::Bert(m) => {
                let token_tensor = Tensor::new(token_ids, &self.device)?.unsqueeze(0)?;
                let type_tensor = Tensor::zeros_like(&token_tensor)?;
                let mask_tensor = Tensor::new(attention_mask, &self.device)?
                    .unsqueeze(0)?
                    .to_dtype(DType::F32)?;

                let hidden = m.forward(&token_tensor, &type_tensor, Some(&mask_tensor))?;
                let pooled = masked_mean_pool(&hidden, &mask_tensor)?;
                let normalized = normalize_l2(&pooled)?;
                let vec = normalized.squeeze(0)?.to_vec1::<f32>()?;
                Ok(vec)
            }
            ModelBackend::Gemma(m) => {
                let token_tensor = Tensor::new(token_ids, &self.device)?.unsqueeze(0)?;
                let mask_tensor = Tensor::new(attention_mask, &self.device)?.unsqueeze(0)?;

                let hidden = m.forward(&token_tensor, &mask_tensor)?;
                let mask_f32 = mask_tensor.to_dtype(DType::F32)?;
                let pooled = masked_mean_pool(&hidden, &mask_f32)?;
                let normalized = normalize_l2(&pooled)?;
                let vec = normalized.squeeze(0)?.to_vec1::<f32>()?;
                Ok(vec)
            }
            ModelBackend::Qwen(m) => {
                let token_tensor = Tensor::new(token_ids, &self.device)?.unsqueeze(0)?;
                let mask_tensor = Tensor::new(attention_mask, &self.device)?.unsqueeze(0)?;

                let hidden = m.forward(&token_tensor, &mask_tensor)?;
                // Last-token pooling needs the mask as owned Vec<Vec<u32>> (batch of one).
                let pooled = last_token_pool(&hidden, &[attention_mask.to_vec()])?;
                let normalized = normalize_l2(&pooled)?;
                let vec = normalized.squeeze(0)?.to_vec1::<f32>()?;
                Ok(vec)
            }
        }
    }

    /// Batch-embed multiple documents in one padded batch.
    pub fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let prefixed: Vec<String> = texts
            .iter()
            .map(|t| apply_prefix(self.backend_kind, TaskKind::Document, t))
            .collect();

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
            let preset = resolve_vision_preset();
            let selection = std::env::var("APEXKIT_VISION_MODEL").unwrap_or_else(|_| "siglip2-onnx".to_string());

            if selection == "candle-siglip2" {
                tracing::info!("Apex Vector: Loading LEGACY candle SigLIP2 vision model (APEXKIT_VISION_MODEL=candle-siglip2).");
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
                        tracing::error!("Candle SigLIP2 load failed ({e}). Dumping tensor names for debugging:");
                        if let Ok(raw_tensors) = candle_core::safetensors::load(&weights, &self.device) {
                            dump_tensor_names(&raw_tensors);
                        }
                        return Err(e);
                    }
                };
                *vision_lock = Some(VisionBackend::CandleSiglip2(model));
            } else {
                tracing::info!(
                    "Apex Vector: Loading ONNX vision model '{}' from repo '{}' file '{}' (APEXKIT_VISION_MODEL={})",
                    selection, preset.repo_id, preset.file_name, selection
                );
                let api = ApiBuilder::new().build()?;
                let repo = api.repo(Repo::new(preset.repo_id.clone(), RepoType::Model));
                let model_path: PathBuf = repo.get(&preset.file_name).with_context(|| {
                    format!(
                        "failed to fetch '{}' from repo '{}' - if this preset's default \
                         filename doesn't exist in that repo, set APEXKIT_VISION_MODEL_FILE \
                         to the correct path (check the repo's file listing on HF)",
                        preset.file_name, preset.repo_id
                    )
                })?;

                let embedder = OnnxVisionEmbedder::load(&model_path, preset.onnx_cfg)?;
                *vision_lock = Some(VisionBackend::Onnx(embedder));
            }
        }

        let backend = vision_lock.as_mut().unwrap();

        let b64 = if let Some(idx) = base64_image.find(',') {
            &base64_image[idx + 1..]
        } else {
            base64_image
        };
        let image_bytes = STANDARD.decode(b64).map_err(E::msg)?;
        let img = image::load_from_memory(&image_bytes).map_err(E::msg)?;

        match backend {
            VisionBackend::Onnx(embedder) => {
                let cfg = embedder.config();
                let size = cfg.image_size;
                let resized = img.resize_exact(size as u32, size as u32, image::imageops::FilterType::Triangle);
                let rgb = resized.to_rgb8();

                let mut data = Vec::with_capacity(3 * size * size);
                for c in 0..3 {
                    for y in 0..size as u32 {
                        for x in 0..size as u32 {
                            let pixel = rgb.get_pixel(x, y);
                            let val = pixel[c] as f32 / 255.0;
                            let norm_val = (val - cfg.mean[c]) / cfg.std[c];
                            data.push(norm_val);
                        }
                    }
                }

                let pooled = embedder.embed(data)?;
                let mut vec = pooled;
                l2_normalize_in_place(&mut vec);
                Ok(vec)
            }
            VisionBackend::CandleSiglip2(model) => {
                let cfg = model.config();
                let image_size = cfg.image_size;
                let resized = img.resize_exact(
                    image_size as u32,
                    image_size as u32,
                    image::imageops::FilterType::Triangle,
                );
                let rgb = resized.to_rgb8();

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
