// Generic ONNX Runtime vision embedder, intended for small/quantized vision-language
// models (SigLIP2-ONNX, TinyCLIP, MobileCLIP) that need to run comfortably under ~512MB
// RAM - the candle SigLIP2 path in `siglip2.rs` loads full F32 weights and a full
// hand-rolled transformer, which is fine on a server but heavy for constrained
// environments. This module instead defers the whole forward pass to `ort`
// (ONNX Runtime's Rust bindings), which can load int8/int4-quantized `.onnx` files and
// run them with a much smaller memory footprint than an F32 candle model.
//
// IMPORTANT CAVEATS (read before trusting this in production):
//   1. `ort`'s API has changed across major versions (1.x vs 2.x have different builder
//      and tensor-extraction signatures). This is written against the 2.x API shape as I
//      understand it, but I have no compiler in this environment to verify it - check
//      against whatever `ort` version actually resolves in your Cargo.lock and adjust the
//      `Session::builder()` / `try_extract_raw_tensor` calls if they don't match.
//   2. Output handling is intentionally defensive (see `embed`) because different ONNX
//      exports name/shape their pooled output differently: some give you a ready-made
//      [1, hidden] pooled embedding, others give [1, seq, hidden] last-hidden-state and
//      expect you to pool yourself. We handle both, mean-pooling the 3D case, but we
//      can't know which one your specific export uses until you run it once and check
//      `dims` via the `tracing::info!` log this prints on first run.
//   3. Input tensor name is configurable (`input_name`, default "pixel_values") because
//      exports disagree about this too - if `session.run` errors with an "unknown input"
//      style message, inspect the model's actual input name (e.g. with Netron or
//      `python -c "import onnx; print(onnx.load(path).graph.input)"`) and set it via
//      `OnnxVisionConfig.input_name` / the `APEXKIT_VISION_INPUT_NAME` env var.

use anyhow::{Context, Result, bail};
use ndarray::Array4;
use ort::session::Session;
use ort::value::Value;
use std::path::Path;

#[derive(Clone, Debug)]
pub struct OnnxVisionConfig {
    pub image_size: usize,
    pub mean: [f32; 3],
    pub std: [f32; 3],
    pub input_name: String,
}

impl Default for OnnxVisionConfig {
    fn default() -> Self {
        Self {
            image_size: 384,
            mean: [0.5, 0.5, 0.5],
            std: [0.5, 0.5, 0.5],
            input_name: "pixel_values".to_string(),
        }
    }
}

impl OnnxVisionConfig {
    /// onnx-community/siglip2-base-patch16-384-ONNX - SigLIP2 base, 384px, exported to ONNX
    /// with quantized variants available. This is the new default vision model: small
    /// enough (quantized weights run well under 512MB RAM) without writing a custom
    /// transformer forward pass by hand, since ONNX Runtime does that for us.
    pub fn siglip2_base_patch16_384_onnx() -> Self {
        Self {
            image_size: 384,
            mean: [0.5, 0.5, 0.5],
            std: [0.5, 0.5, 0.5],
            input_name: "pixel_values".to_string(),
        }
    }

    /// TinyCLIP-ViT-4M-Text-3M - much smaller ViT (~4M vision params), good fit for very
    /// constrained RAM. CAVEAT: I don't have a confirmed public ONNX export path for this
    /// checkpoint at the time of writing - you'll likely need to export it yourself
    /// (optimum-cli export onnx) or point APEXKIT_VISION_MODEL_REPO / _FILE at wherever
    /// you've hosted the .onnx file. Image size matches OpenCLIP's standard 224px.
    pub fn tinyclip_vit_4m() -> Self {
        Self {
            image_size: 224,
            mean: [0.48145466, 0.4578275, 0.40821073],
            std: [0.26862954, 0.26130258, 0.27577711],
            input_name: "pixel_values".to_string(),
        }
    }

    /// Apple MobileCLIP (e.g. MobileCLIP-S0). CAVEAT: same as TinyCLIP above - Apple's
    /// official release is CoreML/PyTorch; a quantized ONNX export is not something I can
    /// confirm is publicly hosted right now. Treat the repo/file as something you supply
    /// via env vars after exporting/quantizing it yourself, or after finding a community
    /// ONNX mirror you've verified.
    pub fn mobileclip_s0() -> Self {
        Self {
            image_size: 256,
            mean: [0.0, 0.0, 0.0],
            std: [1.0, 1.0, 1.0],
            input_name: "pixel_values".to_string(),
        }
    }
}

pub struct OnnxVisionEmbedder {
    session: Session,
    cfg: OnnxVisionConfig,
}

impl OnnxVisionEmbedder {
    pub fn load(model_path: &Path, cfg: OnnxVisionConfig) -> Result<Self> {
        tracing::info!("Apex Vector: Loading ONNX vision model from {:?}", model_path);
        let session = Session::builder()
            .context("failed to create ONNX Runtime session builder")?
            .commit_from_file(model_path)
            .with_context(|| format!("failed to load ONNX model at {:?}", model_path))?;
        Ok(Self { session, cfg })
    }

    pub fn config(&self) -> &OnnxVisionConfig {
        &self.cfg
    }

    /// `pixel_data` must already be a flat CHW f32 buffer (channels, height, width),
    /// normalized per `self.cfg.mean` / `self.cfg.std`, length == 3 * image_size * image_size.
    pub fn embed(&mut self, pixel_data: Vec<f32>) -> Result<Vec<f32>> {
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
        // `Value::from_array` returns `Value<TensorValueType<f32>>`, not the `Value`
        // alias (`Value<DynValueTypeMarker>`) - annotating the binding as `Value` forces
        // an implicit coercion that doesn't exist, hence the E0308. `.into_dyn()` does
        // the conversion explicitly instead.
        let input_value = Value::from_array(array)
            .context("failed to wrap pixel tensor as an ORT Value")?
            .into_dyn();

        let outputs = self
            .session
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
            // Already pooled: [1, hidden] or just [hidden]
            1 => data.to_vec(),
            2 => data.to_vec(),
            // Last-hidden-state: [1, seq, hidden] - mean-pool over the sequence/patch dim.
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
            other => bail!("unexpected ONNX output rank {other} (dims={:?}) - inspect the model's actual output shape and adjust embed()", dims),
        };

        Ok(pooled)
    }
}
