use anyhow::{Context, Error as E, Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use hf_hub::{Repo, RepoType, api::sync::ApiBuilder};
use html2text::from_read;
use std::path::PathBuf;
use std::sync::Mutex;
use tokenizers::{PaddingParams, Tokenizer};

use candle_transformers::models::bert::{BertModel, Config as BertConfig};

use crate::models::gemma_embed::{
    GemmaEmbedConfig, GemmaEmbedModel, dump_tensor_names, masked_mean_pool,
};
// imports: swap onnx_text -> onnx_vision_text for the cross-modal tower,
// add new onnx_text for the stand-alone text-embed models
#[cfg(feature = "onnx")]
use crate::models::onnx_text::{OnnxTextConfig, OnnxTextEmbedder};
#[cfg(feature = "onnx")]
use crate::models::onnx_vision::{OnnxVisionConfig, OnnxVisionEmbedder, VisionFamily};
#[cfg(feature = "onnx")]
use crate::models::onnx_vision_text::{OnnxVisionTextConfig, OnnxVisionTextEmbedder};
use crate::models::qwen_embed::{QwenEmbedConfig, QwenEmbedModel, last_token_pool};
use crate::models::siglip2::{Siglip2VisionModel, SiglipVisionConfig};

// =====================================================================================
// TEXT BACKBONE SELECTION (BERT / Gemma / Qwen) - unchanged from prior revisions
// =====================================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BackendKind {
    Bert,
    Gemma,
    Qwen,
}

#[derive(Clone, Copy, Debug)]
enum TaskKind {
    Document,
    Query,
}

fn apply_prefix(kind: BackendKind, task: TaskKind, text: &str) -> String {
    match (kind, task) {
        (BackendKind::Gemma, TaskKind::Document) => format!("title: none | text: {text}"),
        (BackendKind::Gemma, TaskKind::Query) => format!("task: search result | query: {text}"),
        (BackendKind::Qwen, TaskKind::Document) => text.to_string(),
        (BackendKind::Qwen, TaskKind::Query) => {
            format!("Instruct: Given a query, retrieve relevant documents.\nQuery: {text}")
        }
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

impl EmbeddingModelConfig {
    pub fn gemma_300m_onnx() -> Self {
        Self {
            repo_id: "onnx-community/embeddinggemma-300m-ONNX".to_string(),
            window_size: 2048,
            overlap: 256,
            ..Default::default()
        }
    }
    pub fn qwen3_embedding_0_6b_onnx() -> Self {
        Self {
            repo_id: "onnx-community/Qwen3-Embedding-0.6B-ONNX".to_string(),
            window_size: 8192,
            overlap: 512,
            ..Default::default()
        }
    }
}

enum ModelBackend {
    Bert(Box<BertModel>),
    Gemma(Box<GemmaEmbedModel>),
    Qwen(Box<QwenEmbedModel>),
    #[cfg(feature = "onnx")]
    OnnxText(Box<OnnxTextEmbedder>),
}

// =====================================================================================
// VISION BACKEND SELECTION - siglip / siglip2 / clip / openclip / dinov2 / custom
// =====================================================================================
//
// Controlled by:
//   APEXKIT_VISION_MODEL = siglip | siglip2 (default) | clip | openclip | dinov2 | custom
//                          | candle-siglip2 (legacy hand-rolled candle path, vision-only)
//
//   APEXKIT_VISION_MODEL_REPO        - override the HF repo id for the selected preset's
//                                       IMAGE tower (defaults to a known-good repo per family)
//   APEXKIT_VISION_MODEL_FILE        - override the image tower .onnx filename
//   APEXKIT_VISION_INPUT_NAME        - override the image tower's ONNX input tensor name
//
//   APEXKIT_VISION_TEXT_REPO         - HF repo id for the paired TEXT tower (cross-modal
//                                       text-image search). Defaults to APEXKIT_VISION_MODEL_REPO
//                                       if unset, since most CLIP-family ONNX exports ship
//                                       both towers in the same repo.
//   APEXKIT_VISION_TEXT_FILE         - text tower .onnx filename (default: "onnx/textual.onnx";
//                                       override per repo layout)
//   APEXKIT_VISION_TEXT_TOKENIZER    - tokenizer.json filename for the text tower
//                                       (default: "tokenizer.json")
//   APEXKIT_VISION_TEXT_DISABLE      - set to "1"/"true" to force-disable the text tower
//                                       even for a family that supports it (e.g. if your
//                                       repo only ships the vision half)
//
// Custom (APEXKIT_VISION_MODEL=custom) ALSO requires:
//   APEXKIT_VISION_CUSTOM_IMAGE_SIZE - integer, e.g. "224"
//   APEXKIT_VISION_CUSTOM_MEAN       - comma-separated 3 floats, e.g. "0.5,0.5,0.5"
//   APEXKIT_VISION_CUSTOM_STD        - comma-separated 3 floats, e.g. "0.5,0.5,0.5"
//   (APEXKIT_VISION_MODEL_REPO/_FILE are required too, same as any other preset)
//
// DINOv2 has no text tower by design (see VisionFamily::supports_text_image_in_principle)
// - APEXKIT_VISION_TEXT_* env vars are ignored when APEXKIT_VISION_MODEL=dinov2.

enum VisionBackend {
    #[cfg(feature = "onnx")]
    Onnx {
        family: VisionFamily,
        vision: OnnxVisionEmbedder,
        text: Option<OnnxVisionTextEmbedder>,
        vision_repo: String,
        vision_file: String,
    },
    /// Legacy hand-rolled candle SigLIP2 path from before the ONNX migration. Vision-only
    /// (no text tower) - kept only for environments that can't use ONNX Runtime.
    CandleSiglip2 {
        model: Siglip2VisionModel,
        repo: String,
    },
}

#[cfg(feature = "onnx")]
#[derive(Clone, Debug)]
struct ResolvedVisionConfig {
    family: VisionFamily,
    vision_repo: String,
    vision_file: String,
    onnx_cfg: OnnxVisionConfig,
    text_repo: Option<String>,
    text_file: Option<String>,
    text_tokenizer_file: Option<String>,
    text_cfg: OnnxVisionTextConfig,
}

#[cfg(feature = "onnx")]
fn env_bool(name: &str) -> bool {
    std::env::var(name)
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

#[cfg(feature = "onnx")]
fn parse_csv_f32_3(s: &str) -> Result<[f32; 3]> {
    let parts: Vec<f32> = s
        .split(',')
        .map(|p| p.trim().parse::<f32>())
        .collect::<Result<_, _>>()
        .map_err(|e| anyhow::anyhow!("failed to parse '{s}' as 3 comma-separated floats: {e}"))?;
    if parts.len() != 3 {
        bail!(
            "expected exactly 3 comma-separated floats, got {} in '{}'",
            parts.len(),
            s
        );
    }
    Ok([parts[0], parts[1], parts[2]])
}

#[cfg(feature = "onnx")]
fn resolve_vision_config() -> Result<ResolvedVisionConfig> {
    let selection_str =
        std::env::var("APEXKIT_VISION_MODEL").unwrap_or_else(|_| "siglip2".to_string());
    let family = VisionFamily::from_env_str(&selection_str);

    let repo_override = std::env::var("APEXKIT_VISION_MODEL_REPO").ok();
    let file_override = std::env::var("APEXKIT_VISION_MODEL_FILE").ok();
    let input_name_override = std::env::var("APEXKIT_VISION_INPUT_NAME").ok();
    let output_name_override = std::env::var("APEXKIT_VISION_OUTPUT_NAME").ok();
    let text_output_name_override = std::env::var("APEXKIT_VISION_TEXT_OUTPUT_NAME").ok();

    let (default_repo, default_file, mut onnx_cfg, default_text_cfg) = match family {
        VisionFamily::Siglip => (
            "google/siglip-base-patch16-224".to_string(),
            "onnx/model.onnx".to_string(),
            OnnxVisionConfig::siglip_base_patch16_224(),
            OnnxVisionTextConfig::siglip_style(),
        ),
        VisionFamily::Siglip2 => (
            "onnx-community/siglip2-base-patch16-384-ONNX".to_string(),
            "onnx/vision_model_quantized.onnx".to_string(),
            {
                let mut cfg = OnnxVisionConfig::siglip2_base_patch16_384();
                cfg.output_name = "pooler_output".to_string();
                cfg
            },
            {
                let mut cfg = OnnxVisionTextConfig::siglip_style();
                cfg.output_name = "pooler_output".to_string();
                cfg
            },
        ),
        VisionFamily::Clip => (
            "openai/clip-vit-base-patch32".to_string(),
            "onnx/visual.onnx".to_string(),
            OnnxVisionConfig::clip_vit_base_patch32(),
            OnnxVisionTextConfig::clip_style(),
        ),
        VisionFamily::OpenClip => (
            "laion/CLIP-ViT-B-32-laion2B-s34B-b79K".to_string(),
            "onnx/visual.onnx".to_string(),
            OnnxVisionConfig::openclip_vit_b_32(),
            OnnxVisionTextConfig::clip_style(),
        ),
        VisionFamily::Dinov2 => (
            "facebook/dinov2-base".to_string(),
            "onnx/model.onnx".to_string(),
            OnnxVisionConfig::dinov2_base(),
            OnnxVisionTextConfig::default(), // unused - DINOv2 has no text tower
        ),
        VisionFamily::Custom => {
            let image_size: usize = std::env::var("APEXKIT_VISION_CUSTOM_IMAGE_SIZE")
                .context("APEXKIT_VISION_MODEL=custom requires APEXKIT_VISION_CUSTOM_IMAGE_SIZE")?
                .parse()
                .context("APEXKIT_VISION_CUSTOM_IMAGE_SIZE must be a positive integer")?;
            let mean = parse_csv_f32_3(
                &std::env::var("APEXKIT_VISION_CUSTOM_MEAN")
                    .context("APEXKIT_VISION_MODEL=custom requires APEXKIT_VISION_CUSTOM_MEAN")?,
            )?;
            let std_dev = parse_csv_f32_3(
                &std::env::var("APEXKIT_VISION_CUSTOM_STD")
                    .context("APEXKIT_VISION_MODEL=custom requires APEXKIT_VISION_CUSTOM_STD")?,
            )?;
            let repo = repo_override
                .clone()
                .context("APEXKIT_VISION_MODEL=custom requires APEXKIT_VISION_MODEL_REPO")?;
            let file = file_override
                .clone()
                .context("APEXKIT_VISION_MODEL=custom requires APEXKIT_VISION_MODEL_FILE")?;
            (
                repo,
                file,
                OnnxVisionConfig {
                    image_size,
                    mean,
                    std: std_dev,
                    input_name: "pixel_values".to_string(),
                    output_name: "image_embeds".to_string(), // overridden below via APEXKIT_VISION_OUTPUT_NAME if set
                },
                OnnxVisionTextConfig::default(),
            )
        }
    };

    if let Some(name) = input_name_override {
        onnx_cfg.input_name = name;
    }
    if let Some(name) = output_name_override {
        onnx_cfg.output_name = name;
    }

    let mut default_text_cfg = default_text_cfg;
    if let Some(name) = text_output_name_override {
        default_text_cfg.output_name = name;
    }

    let vision_repo = repo_override.unwrap_or(default_repo);
    let vision_file = file_override.unwrap_or(default_file);

    let text_disabled = env_bool("APEXKIT_VISION_TEXT_DISABLE");
    let (text_repo, text_file, text_tokenizer_file) =
        if family.supports_text_image_in_principle() && !text_disabled {
            let text_repo =
                std::env::var("APEXKIT_VISION_TEXT_REPO").unwrap_or_else(|_| vision_repo.clone());
            let text_file = std::env::var("APEXKIT_VISION_TEXT_FILE")
                .unwrap_or_else(|_| "onnx/text_model_quantized.onnx".to_string());
            let text_tokenizer_file = std::env::var("APEXKIT_VISION_TEXT_TOKENIZER")
                .unwrap_or_else(|_| "tokenizer.json".to_string());
            (Some(text_repo), Some(text_file), Some(text_tokenizer_file))
        } else {
            (None, None, None)
        };

    Ok(ResolvedVisionConfig {
        family,
        vision_repo,
        vision_file,
        onnx_cfg,
        text_repo,
        text_file,
        text_tokenizer_file,
        text_cfg: default_text_cfg,
    })
}

// =====================================================================================
// CandleEmbedder
// =====================================================================================

pub struct CandleEmbedder {
    backend: Mutex<ModelBackend>,
    backend_kind: BackendKind,
    text_config: EmbeddingModelConfig,
    vision_backend: Mutex<Option<VisionBackend>>,
    tokenizer: Tokenizer,
    device: Device,
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

        let is_gemma = lower_repo.contains("gemma");
        let is_qwen = lower_repo.contains("qwen");
        let is_onnx_text = lower_repo.contains("-onnx") || lower_repo.contains("_onnx");

        let (backend, backend_kind) = if is_onnx_text && (is_gemma || is_qwen) {
            #[cfg(feature = "onnx")]
            {
                tracing::info!("Apex Vector: Loading ONNX text-embedding model (ort backend).");
                let onnx_file = std::env::var("APEXKIT_TEXT_MODEL_FILE")
                    .unwrap_or_else(|_| "onnx/model.onnx".to_string());
                let onnx_path = repo.get(&onnx_file).with_context(|| {
                    format!(
                        "failed to fetch text-embed onnx file '{onnx_file}' - if the default path doesn't \
                         exist in this repo, set APEXKIT_TEXT_MODEL_FILE (e.g. onnx/model_quantized.onnx)"
                    )
                })?;

                let onnx_data_file = format!("{onnx_file}_data");
                match repo.get(&onnx_data_file) {
                    Ok(_) => {}
                    Err(e) => {
                        tracing::debug!(
                            "Apex Vector: no external-data sidecar '{onnx_data_file}' found ({e}) - \
                             assuming this export is self-contained (small enough to not need one)."
                        );
                    }
                }

                let config_filename = repo.get(&actual_config.config_file)?;
                let config_str = std::fs::read_to_string(&config_filename)?;
                let raw_config: serde_json::Value = serde_json::from_str(&config_str)?;

                let cfg = if is_gemma {
                    OnnxTextConfig::gemma_embed_onnx()
                } else {
                    OnnxTextConfig::qwen3_embed_onnx_with_kv_shape(&raw_config)?
                };
                let embedder = OnnxTextEmbedder::load(&onnx_path, &tokenizer_filename, cfg)?;
                let kind = if is_gemma {
                    BackendKind::Gemma
                } else {
                    BackendKind::Qwen
                };
                (ModelBackend::OnnxText(Box::new(embedder)), kind)
            }
            #[cfg(not(feature = "onnx"))]
            {
                bail!("ONNX support is disabled in this build. Cannot load an ONNX model.");
            }
        } else {
            // Only candle (safetensors) backends need weights_file/config_file - fetch
            // them here, not unconditionally up top, since ONNX repos don't ship a
            // model.safetensors and would 404 before ever reaching this branch.
            let weights_filename = repo.get(&actual_config.weights_file)?;
            let config_filename = repo.get(&actual_config.config_file)?;
            let config_str = std::fs::read_to_string(config_filename)?;
            let raw_config: serde_json::Value = serde_json::from_str(&config_str)?;

            if is_gemma {
                tracing::info!(
                    "Apex Vector: Loading EmbeddingGemma transformer (full forward pass)."
                );
                let cfg: GemmaEmbedConfig = serde_json::from_value(raw_config).context(
                    "failed to parse Gemma config.json - check field names against GemmaEmbedConfig",
                )?;
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
                        tracing::error!(
                            "Gemma load failed ({e}). Dumping tensor names for debugging:"
                        );
                        if let Ok(raw_tensors) =
                            candle_core::safetensors::load(&weights_filename, &device)
                        {
                            dump_tensor_names(&raw_tensors);
                        }
                        return Err(e);
                    }
                };
                (ModelBackend::Gemma(Box::new(model)), BackendKind::Gemma)
            } else if is_qwen {
                tracing::info!(
                    "Apex Vector: Loading Qwen embedding transformer (causal, last-token pooling)."
                );
                let cfg: QwenEmbedConfig = serde_json::from_value(raw_config).context(
                    "failed to parse Qwen config.json - check field names against QwenEmbedConfig",
                )?;
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
                        tracing::error!(
                            "Qwen load failed ({e}). Dumping tensor names for debugging:"
                        );
                        if let Ok(raw_tensors) =
                            candle_core::safetensors::load(&weights_filename, &device)
                        {
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
            }
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
            text_config: actual_config,
            vision_backend: Mutex::new(None),
            tokenizer,
            device,
        })
    }

    // ------------------------------------------------------------------
    // Text embedding (document/query, text-to-text search)
    // ------------------------------------------------------------------

    pub fn embed(&self, html_content: &str) -> Result<Vec<f32>> {
        self.embed_with_task(html_content, TaskKind::Document)
    }

    pub fn embed_query(&self, query_text: &str) -> Result<Vec<f32>> {
        self.embed_with_task(query_text, TaskKind::Query)
    }

    fn embed_with_task(&self, html_content: &str, task: TaskKind) -> Result<Vec<f32>> {
        let clean_text = from_read(html_content.as_bytes(), usize::MAX);
        let clean_text = apply_prefix(self.backend_kind, task, &clean_text);

        #[cfg(feature = "onnx")]
        {
            let backend_lock = self.backend.lock().unwrap();
            if let ModelBackend::OnnxText(onnx) = &*backend_lock {
                // Route to the new chunk-aware ONNX method
                return onnx.embed_windowed(
                    &clean_text,
                    self.text_config.window_size,
                    self.text_config.overlap,
                );
            }
        }

        let encoding = self.tokenizer.encode(clean_text, true).map_err(E::msg)?;
        let token_ids = encoding.get_ids();

        let window_size = self.text_config.window_size;
        let overlap = self.text_config.overlap;
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

    fn run_model_pass(&self, token_ids: &[u32]) -> Result<Vec<f32>> {
        let attention_mask: Vec<u32> = vec![1; token_ids.len()];
        self.run_model_pass_with_mask(token_ids, &attention_mask)
    }

    fn run_model_pass_with_mask(
        &self,
        token_ids: &[u32],
        attention_mask: &[u32],
    ) -> Result<Vec<f32>> {
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
                Ok(normalized.squeeze(0)?.to_vec1::<f32>()?)
            }
            ModelBackend::Gemma(m) => {
                let token_tensor = Tensor::new(token_ids, &self.device)?.unsqueeze(0)?;
                let mask_tensor = Tensor::new(attention_mask, &self.device)?.unsqueeze(0)?;
                let hidden = m.forward(&token_tensor, &mask_tensor)?;
                let mask_f32 = mask_tensor.to_dtype(DType::F32)?;
                let pooled = masked_mean_pool(&hidden, &mask_f32)?;
                let normalized = normalize_l2(&pooled)?;
                Ok(normalized.squeeze(0)?.to_vec1::<f32>()?)
            }
            ModelBackend::Qwen(m) => {
                let token_tensor = Tensor::new(token_ids, &self.device)?.unsqueeze(0)?;
                let mask_tensor = Tensor::new(attention_mask, &self.device)?.unsqueeze(0)?;
                let hidden = m.forward(&token_tensor, &mask_tensor)?;
                let pooled = last_token_pool(&hidden, &[attention_mask.to_vec()])?;
                let normalized = normalize_l2(&pooled)?;
                Ok(normalized.squeeze(0)?.to_vec1::<f32>()?)
            }
            #[cfg(feature = "onnx")]
            ModelBackend::OnnxText(_) => {
                bail!(
                    "internal error: OnnxText backend should never reach run_model_pass_with_mask"
                )
            }
        }
    }

    pub fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let prefixed: Vec<String> = texts
            .iter()
            .map(|t| apply_prefix(self.backend_kind, TaskKind::Document, t))
            .collect();

        #[cfg(feature = "onnx")]
        {
            let backend_lock = self.backend.lock().unwrap();
            if let ModelBackend::OnnxText(onnx) = &*backend_lock {
                return onnx.embed_batch(&prefixed);
            }
        }

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

    // ------------------------------------------------------------------
    // Vision backend loading (lazy, shared by image-image and text-image paths)
    // ------------------------------------------------------------------

    fn ensure_vision_loaded(&self) -> Result<()> {
        let mut vision_lock = self.vision_backend.lock().unwrap();
        if vision_lock.is_some() {
            return Ok(());
        }

        let default_model = if cfg!(feature = "onnx") { "siglip2" } else { "candle-siglip2" };
        let selection = std::env::var("APEXKIT_VISION_MODEL").unwrap_or_else(|_| default_model.to_string());

        if selection == "candle-siglip2" {
            tracing::info!(
                "Apex Vector: Loading LEGACY candle SigLIP2 vision model (APEXKIT_VISION_MODEL=candle-siglip2)."
            );
            let api = ApiBuilder::new().build()?;
            let repo_id = "google/siglip2-base-patch16-224".to_string();
            let repo = api.repo(Repo::new(repo_id.clone(), RepoType::Model));
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
                    tracing::error!(
                        "Candle SigLIP2 load failed ({e}). Dumping tensor names for debugging:"
                    );
                    if let Ok(raw_tensors) = candle_core::safetensors::load(&weights, &self.device)
                    {
                        dump_tensor_names(&raw_tensors);
                    }
                    return Err(e);
                }
            };
            *vision_lock = Some(VisionBackend::CandleSiglip2 {
                model,
                repo: repo_id,
            });
            return Ok(());
        }

        #[cfg(feature = "onnx")]
        {
            let resolved = resolve_vision_config()?;
            tracing::info!(
                "Apex Vector: Loading ONNX vision family '{}' - vision repo='{}' file='{}', text repo={:?} file={:?}",
                resolved.family.as_str(),
                resolved.vision_repo,
                resolved.vision_file,
                resolved.text_repo,
                resolved.text_file
            );

            let api = ApiBuilder::new().build()?;
            let vision_repo = api.repo(Repo::new(resolved.vision_repo.clone(), RepoType::Model));
            let vision_model_path: PathBuf =
                vision_repo.get(&resolved.vision_file).with_context(|| {
                    format!(
                        "failed to fetch vision tower '{}' from repo '{}' - if this preset's default \
                     filename doesn't exist in that repo, set APEXKIT_VISION_MODEL_FILE to the \
                     correct path (check the repo's file listing on HF)",
                        resolved.vision_file, resolved.vision_repo
                    )
                })?;
            let vision_embedder = OnnxVisionEmbedder::load(&vision_model_path, resolved.onnx_cfg)?;

            let text_embedder = match (
                &resolved.text_repo,
                &resolved.text_file,
                &resolved.text_tokenizer_file,
            ) {
                (Some(text_repo_id), Some(text_file), Some(text_tok_file)) => {
                    let text_repo = api.repo(Repo::new(text_repo_id.clone(), RepoType::Model));
                    match (text_repo.get(text_file), text_repo.get(text_tok_file)) {
                        (Ok(text_model_path), Ok(text_tok_path)) => {
                            match OnnxVisionTextEmbedder::load(
                                &text_model_path,
                                &text_tok_path,
                                resolved.text_cfg.clone(),
                            ) {
                                Ok(t) => Some(t),
                                Err(e) => {
                                    tracing::warn!(
                                        "Apex Vector: text tower failed to load ({e}) - text-image search will be \
                                         unavailable for this session. Image-image search is unaffected."
                                    );
                                    None
                                }
                            }
                        }
                        (Err(e), _) | (_, Err(e)) => {
                            tracing::warn!(
                                "Apex Vector: text tower files not found in repo '{}' ({e}) - text-image search \
                                 will be unavailable. Set APEXKIT_VISION_TEXT_REPO/_FILE/_TOKENIZER if the text \
                                 tower lives elsewhere, or APEXKIT_VISION_TEXT_DISABLE=1 to silence this warning.",
                                text_repo_id
                            );
                            None
                        }
                    }
                }
                _ => None,
            };

            *vision_lock = Some(VisionBackend::Onnx {
                family: resolved.family,
                vision: vision_embedder,
                text: text_embedder,
                vision_repo: resolved.vision_repo,
                vision_file: resolved.vision_file,
            });
            return Ok(());
        }
        #[cfg(not(feature = "onnx"))]
        {
            bail!("ONNX support is disabled. Set APEXKIT_VISION_MODEL=candle-siglip2 to use pure Rust vision engine.");
        }
    }

    // ------------------------------------------------------------------
    // Image-image search: embed any image for comparison against other images.
    // ------------------------------------------------------------------

    pub fn embed_image(&self, base64_image: &str) -> Result<Vec<f32>> {
        self.ensure_vision_loaded()?;
        let vision_lock = self.vision_backend.lock().unwrap();
        let backend = vision_lock.as_ref().unwrap();

        let b64 = if let Some(idx) = base64_image.find(',') {
            &base64_image[idx + 1..]
        } else {
            base64_image
        };
        let image_bytes = STANDARD.decode(b64).map_err(E::msg)?;
        let img = image::load_from_memory(&image_bytes).map_err(E::msg)?;

        match backend {
            #[cfg(feature = "onnx")]
            VisionBackend::Onnx { vision, .. } => {
                let cfg = vision.config();
                let size = cfg.image_size;
                let resized = img.resize_exact(
                    size as u32,
                    size as u32,
                    image::imageops::FilterType::Triangle,
                );
                let rgb = resized.to_rgb8();

                let mut data = Vec::with_capacity(3 * size * size);
                for c in 0..3 {
                    for y in 0..size as u32 {
                        for x in 0..size as u32 {
                            let pixel = rgb.get_pixel(x, y);
                            let val = pixel[c] as f32 / 255.0;
                            data.push((val - cfg.mean[c]) / cfg.std[c]);
                        }
                    }
                }

                let mut pooled = vision.embed(data)?;
                l2_normalize_in_place(&mut pooled);
                Ok(pooled)
            }
            VisionBackend::CandleSiglip2 { model, .. } => {
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
                            data.push((val - 0.5) / 0.5);
                        }
                    }
                }

                let tensor = Tensor::from_vec(data, (3, image_size, image_size), &self.device)?
                    .unsqueeze(0)?;
                let pooled = model.forward(&tensor)?;
                let normalized = normalize_l2(&pooled)?;
                Ok(normalized.squeeze(0)?.to_vec1::<f32>()?)
            }
        }
    }

    // ------------------------------------------------------------------
    // Text-image search: embed query text into the SAME joint space as embed_image's
    // output, using the active vision model's paired text tower. Returns an error for
    // model families/configs with no text tower (DINOv2 always; others if the text
    // tower wasn't found/configured) instead of silently returning a meaningless vector.
    // ------------------------------------------------------------------

    pub fn embed_text_for_image_search(&self, text: &str) -> Result<Vec<f32>> {
        self.ensure_vision_loaded()?;
        let vision_lock = self.vision_backend.lock().unwrap();
        let backend = vision_lock.as_ref().unwrap();

        match backend {
            #[cfg(feature = "onnx")]
            VisionBackend::Onnx {
                text: Some(text_embedder),
                ..
            } => {
                let mut pooled = text_embedder.embed(text)?;
                l2_normalize_in_place(&mut pooled);
                Ok(pooled)
            }
            #[cfg(feature = "onnx")]
            VisionBackend::Onnx {
                family, text: None, ..
            } => {
                if family.supports_text_image_in_principle() {
                    bail!(
                        "text-image search is unsupported right now: the '{}' family supports it in \
                         principle, but no text tower was loaded for the current configuration. Set \
                         APEXKIT_VISION_TEXT_REPO/_FILE/_TOKENIZER to point at one.",
                        family.as_str()
                    )
                } else {
                    bail!(
                        "text-image search is unsupported: '{}' is a vision-only model with no paired \
                         text tower - this is a model limitation, not a missing feature.",
                        family.as_str()
                    )
                }
            }
            VisionBackend::CandleSiglip2 { .. } => {
                bail!(
                    "text-image search is unsupported: the legacy candle-siglip2 path is vision-only. \
                     Switch APEXKIT_VISION_MODEL to siglip2 (the ONNX default) to get a paired text tower."
                )
            }
        }
    }

    // ------------------------------------------------------------------
    // Stable model identity, for tagging stored vectors with which model produced them.
    // ------------------------------------------------------------------

    pub fn current_text_model_id(&self) -> String {
        format!("{}@{}", self.text_config.repo_id, self.text_config.revision)
    }

    pub fn current_vision_model_id(&self) -> Result<String> {
        self.ensure_vision_loaded()?;
        let vision_lock = self.vision_backend.lock().unwrap();
        let backend = vision_lock.as_ref().unwrap();
        Ok(match backend {
            #[cfg(feature = "onnx")]
            VisionBackend::Onnx {
                family,
                vision_repo,
                vision_file,
                ..
            } => {
                format!("{}:{}:{}", family.as_str(), vision_repo, vision_file)
            }
            VisionBackend::CandleSiglip2 { repo, .. } => {
                format!("candle-siglip2:{}:model.safetensors", repo)
            }
        })
    }
}

// =====================================================================================
// Free-function identity helpers, exposed via lib.rs - read directly from env so callers
// can compute an identity string WITHOUT needing a loaded CandleEmbedder instance handy
// (e.g. at startup, before deciding whether to re-index due to a model change).
// =====================================================================================

/// Mirrors the active_model_name -> EmbeddingModelConfig mapping applications typically
/// build around this crate (bge-small / bge-base / gte-small / gemma-300m / qwen3-embedding
/// / custom), driven by APEX_VECTOR_TEXT_MODEL + the APEX_VECTOR_CUSTOM_* env vars, so the
/// identity string can be computed independently of whatever model instance is currently
/// loaded in memory.
pub fn get_current_text_model() -> String {
    let active_model_name =
        std::env::var("APEX_VECTOR_TEXT_MODEL").unwrap_or_else(|_| "default".to_string());

    let cfg = match active_model_name.as_str() {
        "bge-small" => EmbeddingModelConfig::bge_small_en_v1_5(),
        "bge-base" => EmbeddingModelConfig::bge_base_en_v1_5(),
        "gte-small" => EmbeddingModelConfig::gte_small(),
        "gemma-300m" => EmbeddingModelConfig::gemma_300m(),
        "qwen3-embedding" => EmbeddingModelConfig::qwen3_embedding_0_6b(),
        "gemma-300m-onnx" => EmbeddingModelConfig::gemma_300m_onnx(),
        "qwen3-embedding-onnx" => EmbeddingModelConfig::qwen3_embedding_0_6b_onnx(),
        "custom" => EmbeddingModelConfig::custom(
            std::env::var("APEX_VECTOR_CUSTOM_REPO")
                .unwrap_or_else(|_| "sentence-transformers/all-MiniLM-L6-v2".to_string()),
            std::env::var("APEX_VECTOR_CUSTOM_REV").unwrap_or_else(|_| "main".to_string()),
            std::env::var("APEX_VECTOR_CUSTOM_CONFIG")
                .unwrap_or_else(|_| "config.json".to_string()),
            std::env::var("APEX_VECTOR_CUSTOM_TOKENIZER")
                .unwrap_or_else(|_| "tokenizer.json".to_string()),
            std::env::var("APEX_VECTOR_CUSTOM_WEIGHTS")
                .unwrap_or_else(|_| "model.safetensors".to_string()),
            std::env::var("APEX_VECTOR_CUSTOM_WINDOW")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(512),
            std::env::var("APEX_VECTOR_CUSTOM_OVERLAP")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(128),
        ),
        _ => EmbeddingModelConfig::default(),
    };

    format!("{}@{}", cfg.repo_id, cfg.revision)
}

/// Same idea as `get_current_text_model`, but for the vision side. Reads
/// APEXKIT_VISION_MODEL plus its repo/file overrides directly, without requiring a loaded
/// vision backend - this is deliberately a pure env read, not a method on CandleEmbedder,
/// so callers can compute it cheaply (e.g. to decide whether stored vectors need
/// re-embedding) without paying the cost of actually loading the ONNX session.
pub fn get_current_vision_model() -> String {
    let default_model = if cfg!(feature = "onnx") { "siglip2" } else { "candle-siglip2" };
    let selection = std::env::var("APEXKIT_VISION_MODEL").unwrap_or_else(|_| default_model.to_string());

    if selection == "candle-siglip2" {
        return "candle-siglip2:google/siglip2-base-patch16-224:model.safetensors".to_string();
    }

    #[cfg(feature = "onnx")]
    match resolve_vision_config() {
        Ok(resolved) => format!(
            "{}:{}:{}",
            resolved.family.as_str(),
            resolved.vision_repo,
            resolved.vision_file
        ),
        Err(e) => format!("unresolved:{selection}:error={e}"),
    }
    #[cfg(not(feature = "onnx"))]
    format!("unresolved:onnx_disabled_for_{selection}")
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