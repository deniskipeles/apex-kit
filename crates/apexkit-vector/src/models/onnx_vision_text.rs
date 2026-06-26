// Generic ONNX Runtime TEXT embedder for the text side of CLIP-family joint embedding
// spaces (SigLIP, SigLIP2, CLIP, OpenCLIP). This is deliberately separate from
// `OnnxTextEmbedder` in onnx_text.rs - that module is for stand-alone text-embedding
// models (EmbeddingGemma-ONNX, Qwen3-Embedding-ONNX) producing vectors for text-to-text
// search. THIS module produces vectors in a model's joint text/image space, for
// text-image search. You cannot mix the two: a BGE text embedding and a SigLIP image
// embedding are not comparable, even though both are "just vectors of floats" - it'll
// just be meaningless. This module exists specifically so text-image search uses the
// SAME model's text tower as whatever produced the image embeddings it's being compared
// against (see onnx_vision.rs).
//
// DINOv2 has no text tower - there is no OnnxVisionTextEmbedder preset for it, and
// `CandleEmbedder::embed_text_for_image_search` checks `VisionFamily::supports_text_image_in_principle`
// before ever trying to load one.
//
// NOTE ON TOKENIZATION: CLIP/OpenCLIP traditionally use a BPE tokenizer with a 77-token
// fixed context length and start/end-of-text special tokens baked into the original
// implementation; SigLIP/SigLIP2 use a SentencePiece-based tokenizer with longer context.
// Rather than hand-implement either, this loads a standard HF `tokenizer.json` via the
// `tokenizers` crate already used elsewhere in this crate - correctness here depends on
// the checkpoint's `tokenizer.json` actually matching what the ONNX text tower expects.

use anyhow::{Context, Result, bail};
use ndarray::Array2;
use ort::session::{OutputSelector, RunOptions, Session};
use ort::value::Value;
use std::path::Path;
use std::sync::Mutex;
use tokenizers::Tokenizer;

#[derive(Clone, Debug)]
pub struct OnnxVisionTextConfig {
    pub input_ids_name: String,
    pub attention_mask_name: Option<String>,
    /// CLIP's original tokenizer pads/truncates to a fixed 77-token context length.
    /// SigLIP/OpenCLIP ONNX exports vary - set this to `None` to use the tokenizer's
    /// natural output length with no fixed truncation/padding.
    pub fixed_seq_len: Option<usize>,
    /// Name of the graph output to request. For a fused vision+text checkpoint (the
    /// default SigLIP2-ONNX export ships both towers in one file), an unfiltered
    /// `session.run()` would also try to compute the vision branch's output and fail
    /// because we never supply `pixel_values` here. Naming the text-side output (commonly
    /// "text_embeds") via `OutputSelector` keeps ORT from touching the vision branch at
    /// all. Confirm the real name with `onnx.load(...).graph.output` for your checkpoint.
    pub output_name: String,
}

impl Default for OnnxVisionTextConfig {
    fn default() -> Self {
        Self {
            input_ids_name: "input_ids".to_string(),
            attention_mask_name: Some("attention_mask".to_string()),
            fixed_seq_len: None,
            output_name: "text_embeds".to_string(),
        }
    }
}

impl OnnxVisionTextConfig {
    pub fn clip_style() -> Self {
        Self {
            input_ids_name: "input_ids".to_string(),
            attention_mask_name: Some("attention_mask".to_string()),
            fixed_seq_len: Some(77),
            output_name: "text_embeds".to_string(),
        }
    }

    pub fn siglip_style() -> Self {
        Self {
            input_ids_name: "input_ids".to_string(),
            attention_mask_name: None, // SigLIP's text tower doesn't take one
            fixed_seq_len: Some(64),
            output_name: "pooler_output".to_string(),
        }
    }
}

pub struct OnnxVisionTextEmbedder {
    session: Mutex<Session>,
    tokenizer: Tokenizer,
    cfg: OnnxVisionTextConfig,
}

impl OnnxVisionTextEmbedder {
    pub fn load(
        model_path: &Path,
        tokenizer_path: &Path,
        cfg: OnnxVisionTextConfig,
    ) -> Result<Self> {
        tracing::info!(
            "Apex Vector: Loading ONNX vision-text tower from {:?}",
            model_path
        );
        let session = Session::builder()
            .context("failed to create ONNX Runtime session builder")?
            .commit_from_file(model_path)
            .with_context(|| format!("failed to load ONNX text model at {:?}", model_path))?;

        let tokenizer_bytes = std::fs::read(tokenizer_path)
            .with_context(|| format!("failed to read tokenizer file at {:?}", tokenizer_path))?;
        let tokenizer = Tokenizer::from_bytes(&tokenizer_bytes)
            .map_err(|e| anyhow::anyhow!("vision-text tokenizer parse error: {e}"))?;

        Ok(Self {
            session: Mutex::new(session),
            tokenizer,
            cfg,
        })
    }

    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| anyhow::anyhow!("vision-text tokenizer encode error: {e}"))?;

        let mut ids: Vec<i64> = encoding.get_ids().iter().map(|&x| x as i64).collect();
        let mut mask: Vec<i64> = encoding
            .get_attention_mask()
            .iter()
            .map(|&x| x as i64)
            .collect();

        if let Some(fixed_len) = self.cfg.fixed_seq_len {
            if ids.len() > fixed_len {
                ids.truncate(fixed_len);
                mask.truncate(fixed_len);
            } else {
                while ids.len() < fixed_len {
                    ids.push(0);
                    mask.push(0);
                }
            }
        }

        let seq_len = ids.len();
        let ids_array = Array2::from_shape_vec((1, seq_len), ids)
            .context("failed to reshape input_ids into [1, seq_len]")?;
        let ids_value = Value::from_array(ids_array)
            .context("failed to wrap input_ids as an ORT Value")?
            .into_dyn();

        let run_options = RunOptions::new()
            .context("failed to create ORT RunOptions")?
            .with_outputs(OutputSelector::no_default().with(self.cfg.output_name.as_str()));

        let mut session_guard = self.session.lock().unwrap();
        let outputs = if let Some(mask_name) = &self.cfg.attention_mask_name {
            let mask_array = Array2::from_shape_vec((1, seq_len), mask)
                .context("failed to reshape attention_mask into [1, seq_len]")?;
            let mask_value = Value::from_array(mask_array)
                .context("failed to wrap attention_mask as an ORT Value")?
                .into_dyn();
            session_guard.run_with_options(
                ort::inputs![
                    self.cfg.input_ids_name.as_str() => ids_value,
                    mask_name.as_str() => mask_value,
                ],
                &run_options,
            )
        } else {
            session_guard.run_with_options(
                ort::inputs![self.cfg.input_ids_name.as_str() => ids_value],
                &run_options,
            )
        }
        .context(
            "ONNX vision-text inference failed - check input_ids_name/attention_mask_name \
             match the model's actual graph inputs (inspect with Netron if unsure)",
        )?;

        let first_output = outputs
            .iter()
            .next()
            .context("ONNX vision-text model produced no outputs")?
            .1;
        let (shape, data) = first_output
            .try_extract_tensor::<f32>()
            .context("failed to extract f32 tensor from ONNX vision-text output")?;
        let dims: Vec<usize> = shape.iter().map(|d| *d as usize).collect();
        tracing::info!("Apex Vector: ONNX vision-text output dims = {:?}", dims);

        let pooled: Vec<f32> = match dims.len() {
            1 => data.to_vec(),
            2 => data.to_vec(),
            3 => {
                // [1, seq, hidden] last-hidden-state - mean pool. Note CLIP's reference
                // text tower actually pools at the EOS token position, not the mean; if
                // your export gives you a raw sequence here, mean pooling is an
                // approximation, same caveat as the vision side.
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
                "unexpected ONNX vision-text output rank {other} (dims={:?})",
                dims
            ),
        };

        Ok(pooled)
    }
}
