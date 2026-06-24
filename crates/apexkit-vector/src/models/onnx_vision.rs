// Generic ONNX Runtime vision embedder, scoped to five supported families:
// SigLIP, SigLIP2, CLIP, OpenCLIP, DINOv2 - plus a "Custom" escape hatch for any ONNX
// vision model that fits the same input/output contract (NCHW float input, pooled or
// per-patch hidden state output). Each family gets its own preprocessing config (image
// size, channel mean/std) because they were trained with different normalization
// conventions; using the wrong one won't error, it'll just quietly produce worse
// embeddings (same failure mode as everything else pooling/preprocessing related in this
// crate - nothing crashes, the numbers are just wrong).
//
// CROSS-MODAL CAPABILITY: SigLIP, SigLIP2, CLIP, and OpenCLIP are trained with a paired
// text tower in the same embedding space, so text-image search is meaningful for them.
// DINOv2 is a vision-only self-supervised model with no text tower at all - there is no
// such thing as "DINOv2 text embedding," so text-image search for DINOv2 is not a missing
// feature to implement later, it's a model limitation. `VisionFamily::supports_text_image`
// reflects this, and `CandleEmbedder::embed_text_for_image_search` uses it to fail loudly
// (`bail!`) instead of silently returning a meaningless vector.
//
// IMPORTANT ORT/NDARRAY VERSION NOTE: ort does not re-export ndarray. Whatever `ndarray`
// version this crate declares in Cargo.toml MUST match the version ort's own Cargo.toml
// pins internally (currently 0.17 for ort 2.0.0-rc.12), or you get the
// "OwnedTensorArrayData is not satisfied... multiple different versions of crate
// `ndarray`" error - two structurally-identical-looking types that the compiler treats as
// unrelated because they come from different crate-version instantiations.

use anyhow::{Context, Result, bail};
use ndarray::Array4;
use ort::session::Session;
use ort::value::Value;
use std::path::Path;
use std::sync::Mutex;

/// The five vision-model families this crate knows how to preprocess and run, plus a
/// `Custom` escape hatch for anything else with a compatible ONNX input/output contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisionFamily {
    Siglip,
    Siglip2,
    Clip,
    OpenClip,
    Dinov2,
    Custom,
}

impl VisionFamily {
    /// Parses the APEXKIT_VISION_MODEL env value. Unrecognized strings fall back to
    /// `Siglip2` (the default) rather than erroring, on the theory that a typo'd env var
    /// shouldn't take down the whole service - but it does log a warning, so the mistake
    /// is visible rather than silently "working" with the wrong model.
    pub fn from_env_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "siglip" => VisionFamily::Siglip,
            "siglip2" => VisionFamily::Siglip2,
            "clip" => VisionFamily::Clip,
            "openclip" => VisionFamily::OpenClip,
            "dinov2" => VisionFamily::Dinov2,
            "custom" => VisionFamily::Custom,
            other => {
                tracing::warn!(
                    "Apex Vector: unrecognized APEXKIT_VISION_MODEL='{other}', falling back to siglip2. \
                     Valid values: siglip, siglip2, clip, openclip, dinov2, custom."
                );
                VisionFamily::Siglip2
            }
        }
    }

    /// DINOv2 is vision-only (no paired text tower exists for it, by design). The other
    /// four families are trained with a joint text/image embedding space and support
    /// text-image search - PROVIDED a text-tower ONNX file is actually configured; this
    /// flag says the model family is capable in principle, not that the current
    /// configuration definitely has a text tower loaded (custom configs may omit one).
    pub fn supports_text_image_in_principle(&self) -> bool {
        !matches!(self, VisionFamily::Dinov2)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            VisionFamily::Siglip => "siglip",
            VisionFamily::Siglip2 => "siglip2",
            VisionFamily::Clip => "clip",
            VisionFamily::OpenClip => "openclip",
            VisionFamily::Dinov2 => "dinov2",
            VisionFamily::Custom => "custom",
        }
    }
}

#[derive(Clone, Debug)]
pub struct OnnxVisionConfig {
    pub image_size: usize,
    pub mean: [f32; 3],
    pub std: [f32; 3],
    pub input_name: String,
}

impl OnnxVisionConfig {
    /// google/siglip-base-patch16-224 - original SigLIP, sigmoid-loss CLIP variant.
    pub fn siglip_base_patch16_224() -> Self {
        Self {
            image_size: 224,
            mean: [0.5, 0.5, 0.5],
            std: [0.5, 0.5, 0.5],
            input_name: "pixel_values".to_string(),
        }
    }

    /// onnx-community/siglip2-base-patch16-384-ONNX - SigLIP2, default vision model.
    /// Confirmed public quantized ONNX export, fits comfortably under 512MB RAM.
    pub fn siglip2_base_patch16_384() -> Self {
        Self {
            image_size: 384,
            mean: [0.5, 0.5, 0.5],
            std: [0.5, 0.5, 0.5],
            input_name: "pixel_values".to_string(),
        }
    }

    /// openai/clip-vit-base-patch32 - original OpenAI CLIP, ImageNet-style normalization.
    pub fn clip_vit_base_patch32() -> Self {
        Self {
            image_size: 224,
            mean: [0.48145466, 0.4578275, 0.40821073],
            std: [0.26862954, 0.26130258, 0.27577711],
            input_name: "pixel_values".to_string(),
        }
    }

    /// laion/CLIP-ViT-B-32-laion2B-s34B-b79K - OpenCLIP, same preprocessing convention as
    /// OpenAI CLIP (ImageNet mean/std), different training data/weights.
    pub fn openclip_vit_b_32() -> Self {
        Self {
            image_size: 224,
            mean: [0.48145466, 0.4578275, 0.40821073],
            std: [0.26862954, 0.26130258, 0.27577711],
            input_name: "pixel_values".to_string(),
        }
    }

    /// facebook/dinov2-base - vision-only self-supervised model, ImageNet normalization,
    /// no text tower (see `VisionFamily::supports_text_image_in_principle`).
    pub fn dinov2_base() -> Self {
        Self {
            image_size: 224,
            mean: [0.485, 0.456, 0.406],
            std: [0.229, 0.224, 0.225],
            input_name: "pixel_values".to_string(),
        }
    }
}

pub struct OnnxVisionEmbedder {
    session: Mutex<Session>,
    cfg: OnnxVisionConfig,
}

impl OnnxVisionEmbedder {
    pub fn load(model_path: &Path, cfg: OnnxVisionConfig) -> Result<Self> {
        tracing::info!(
            "Apex Vector: Loading ONNX vision model from {:?}",
            model_path
        );
        let session = Session::builder()
            .context("failed to create ONNX Runtime session builder")?
            .commit_from_file(model_path)
            .with_context(|| format!("failed to load ONNX model at {:?}", model_path))?;
        Ok(Self {
            session: Mutex::new(session),
            cfg,
        })
    }

    pub fn config(&self) -> &OnnxVisionConfig {
        &self.cfg
    }

    /// `pixel_data` must already be a flat CHW f32 buffer (channels, height, width),
    /// normalized per `self.cfg.mean` / `self.cfg.std`, length == 3 * image_size * image_size.
    pub fn embed(&self, pixel_data: Vec<f32>) -> Result<Vec<f32>> {
        let size = self.cfg.image_size;
        let expected_len = 3 * size * size;
        if pixel_data.len() != expected_len {
            bail!(
                "pixel buffer length {} does not match expected {} (3x{}x{})",
                pixel_data.len(),
                expected_len,
                size,
                size
            );
        }

        let array = Array4::from_shape_vec((1, 3, size, size), pixel_data)
            .context("failed to reshape pixel buffer into NCHW tensor")?;
        // Don't annotate this as `Value` (= Value<DynValueTypeMarker>) - from_array
        // returns the concretely-typed Value<TensorValueType<f32>>; convert explicitly
        // with .into_dyn() since that's what ort::inputs! expects.
        let input_value = Value::from_array(array)
            .context("failed to wrap pixel tensor as an ORT Value")?
            .into_dyn();

        let mut session_guard = self.session.lock().unwrap();
        let outputs = session_guard
            .run(ort::inputs![self.cfg.input_name.as_str() => input_value])
            .context(
                "ONNX Runtime inference failed - check that input_name matches the model's \
                 actual input tensor name (inspect with Netron if unsure)",
            )?;

        let first_output = outputs
            .iter()
            .next()
            .context("ONNX model produced no outputs")?
            .1;
        let (shape, data) = first_output
            .try_extract_tensor::<f32>()
            .context("failed to extract f32 tensor from ONNX output")?;
        let dims: Vec<usize> = shape.iter().map(|d| *d as usize).collect();
        tracing::info!("Apex Vector: ONNX vision output dims = {:?}", dims);

        let pooled: Vec<f32> = match dims.len() {
            1 => data.to_vec(),
            2 => data.to_vec(),
            3 => {
                let seq = dims[1];
                let hidden = dims[2];
                let mut acc = vec![0f32; hidden];
                for s in 0..seq {
                    for h in 0..hidden {
                        acc[h] += data[s * hidden + h];
                    }
                }
                for v in acc.iter_mut() {
                    *v /= seq as f32;
                }
                acc
            }
            other => bail!(
                "unexpected ONNX output rank {other} (dims={:?}) - inspect the model's actual output shape and adjust embed()",
                dims
            ),
        };

        Ok(pooled)
    }
}
