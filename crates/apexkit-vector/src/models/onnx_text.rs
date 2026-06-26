// Generic ONNX Runtime embedder for stand-alone TEXT EMBEDDING models exported via
// onnx-community (e.g. embeddinggemma-300m-ONNX, Qwen3-Embedding-0.6B-ONNX). These
// produce vectors for text-to-text search, in the SAME family of embedding space as the
// candle-native Gemma/Qwen paths in gemma_embed.rs/qwen_embed.rs - this is just an
// alternate (ONNX Runtime) execution backend for the identical model family, chosen when
// the repo id signals an ONNX export rather than raw safetensors.
//
// Do NOT confuse this with onnx_vision_text.rs::OnnxVisionTextEmbedder - that module is
// for CLIP/SigLIP-style paired text towers living in a joint text/image space (used for
// text-image search). This module's output is comparable with other text-to-text
// embeddings produced by this crate (BERT/Gemma/Qwen, candle or ONNX), never with image
// embeddings.
//
// POOLING DIFFERS BY MODEL, same as the candle paths:
//   - EmbeddingGemma (bidirectional): masked mean pool over real (non-padding) tokens.
//   - Qwen3-Embedding (causal): last real (non-padding) token's hidden state.
// Get the wrong one and, same failure mode as everywhere else in this crate: nothing
// crashes, the vectors are just quietly wrong.
//
// PROMPT PREFIXES are NOT applied inside this module - callers (CandleEmbedder) apply
// `apply_prefix` exactly as they do for the candle backends, before calling embed_batch.

use anyhow::{Context, Result, bail};
use ndarray::Array2;
use ort::session::{OutputSelector, RunOptions, Session};
use ort::value::{DynValue, Value};
use std::path::Path;
use std::sync::Mutex;
use tokenizers::Tokenizer;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pooling {
    /// Bidirectional models (EmbeddingGemma): average hidden states over real tokens only.
    MaskedMean,
    /// Causal models (Qwen3-Embedding): take the last non-padding token's hidden state.
    LastToken,
}

#[derive(Clone, Debug)]
pub struct OnnxTextConfig {
    pub input_ids_name: String,
    pub attention_mask_name: String,
    /// Name of the graph output to request. If the export exposes an already-pooled
    /// output (e.g. "sentence_embedding"), set that here and `pooling` is ignored for
    /// 2D outputs. If it only exposes "last_hidden_state" ([batch, seq, hidden]), set
    /// that here and `pooling` is applied manually. CONFIRM the real output name with
    /// Netron / `onnx.load(...).graph.output` per checkpoint - don't trust either
    /// default blindly.
    pub output_name: String,
    pub pooling: Pooling,
    /// Set when the exported graph is decoder-with-cache and needs empty
    /// past_key_values.{i}.key/.value inputs fed on every call (Qwen3-style causal
    /// export). None for graphs with no KV-cache inputs (e.g. EmbeddingGemma, if its
    /// export is a plain encoder graph).
    pub kv_cache: Option<KvCacheShape>,
}

#[derive(Clone, Copy, Debug)]
pub struct KvCacheShape {
    pub num_layers: usize,
    pub num_kv_heads: usize,
    pub head_dim: usize,
}

impl OnnxTextConfig {
    pub fn gemma_embed_onnx() -> Self {
        Self {
            input_ids_name: "input_ids".to_string(),
            attention_mask_name: "attention_mask".to_string(),
            output_name: "sentence_embedding".to_string(), // confirmed real graph output - already pooled
            pooling: Pooling::MaskedMean, // unused for 2D output, harmless to leave
            kv_cache: None,
        }
    }

    /// Reads num_hidden_layers / num_key_value_heads (or num_attention_heads if absent) /
    /// head_dim straight out of config.json, since the ONNX graph's past_key_values
    /// shapes must match the checkpoint exactly - hardcoding these would silently break
    /// on any other Qwen3-Embedding size variant.
    pub fn qwen3_embed_onnx_with_kv_shape(raw_config: &serde_json::Value) -> anyhow::Result<Self> {
        let num_layers = raw_config["num_hidden_layers"]
            .as_u64()
            .context("config.json missing num_hidden_layers")? as usize;
        let hidden_size = raw_config["hidden_size"]
            .as_u64()
            .context("config.json missing hidden_size")? as usize;
        let num_attention_heads = raw_config["num_attention_heads"]
            .as_u64()
            .context("config.json missing num_attention_heads")?
            as usize;
        let num_kv_heads = raw_config["num_key_value_heads"]
            .as_u64()
            .unwrap_or(num_attention_heads as u64) as usize;
        let head_dim = raw_config["head_dim"]
            .as_u64()
            .unwrap_or((hidden_size / num_attention_heads) as u64) as usize;

        Ok(Self {
            input_ids_name: "input_ids".to_string(),
            attention_mask_name: "attention_mask".to_string(),
            output_name: "last_hidden_state".to_string(),
            pooling: Pooling::LastToken,
            kv_cache: Some(KvCacheShape {
                num_layers,
                num_kv_heads,
                head_dim,
            }),
        })
    }
}

pub struct OnnxTextEmbedder {
    session: Mutex<Session>,
    tokenizer: Tokenizer,
    cfg: OnnxTextConfig,
}

impl OnnxTextEmbedder {
    pub fn load(model_path: &Path, tokenizer_path: &Path, cfg: OnnxTextConfig) -> Result<Self> {
        tracing::info!(
            "Apex Vector: Loading ONNX text-embedding model from {:?}",
            model_path
        );
        let session = Session::builder()
            .context("failed to create ONNX Runtime session builder")?
            .commit_from_file(model_path)
            .with_context(|| format!("failed to load ONNX text-embed model at {:?}", model_path))?;

        let tokenizer_bytes = std::fs::read(tokenizer_path)
            .with_context(|| format!("failed to read tokenizer file at {:?}", tokenizer_path))?;
        let tokenizer = Tokenizer::from_bytes(&tokenizer_bytes)
            .map_err(|e| anyhow::anyhow!("text-embed tokenizer parse error: {e}"))?;

        Ok(Self {
            session: Mutex::new(session),
            tokenizer,
            cfg,
        })
    }

    /// Single-text convenience wrapper around `embed_batch`.
    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        Ok(self
            .embed_batch(std::slice::from_ref(&text.to_string()))?
            .remove(0))
    }

    /// Batched embedding: pads to batch-longest, runs once, pools per `cfg.pooling`,
    /// L2-normalizes each result.
    pub fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let encodings = self
            .tokenizer
            .encode_batch(texts.to_vec(), true)
            .map_err(|e| anyhow::anyhow!("tokenizer encode_batch error: {e}"))?;

        let max_len = encodings
            .iter()
            .map(|e| e.get_ids().len())
            .max()
            .unwrap_or(0);
        let batch = encodings.len();

        let mut ids_flat = vec![0i64; batch * max_len];
        let mut mask_flat = vec![0i64; batch * max_len];
        let mut masks_u32: Vec<Vec<u32>> = Vec::with_capacity(batch);

        for (i, enc) in encodings.iter().enumerate() {
            let ids = enc.get_ids();
            let mask = enc.get_attention_mask();
            for (j, &id) in ids.iter().enumerate() {
                ids_flat[i * max_len + j] = id as i64;
                mask_flat[i * max_len + j] = mask[j] as i64;
            }
            masks_u32.push(mask.to_vec());
        }

        let ids_array = Array2::from_shape_vec((batch, max_len), ids_flat)
            .context("failed to reshape input_ids into [batch, max_len]")?;
        let mask_array = Array2::from_shape_vec((batch, max_len), mask_flat)
            .context("failed to reshape attention_mask into [batch, max_len]")?;
        let ids_value = Value::from_array(ids_array)
            .context("failed to wrap input_ids as an ORT Value")?
            .into_dyn();
        let mask_value = Value::from_array(mask_array)
            .context("failed to wrap attention_mask as an ORT Value")?
            .into_dyn();

        let run_options = RunOptions::new()
            .context("failed to create ORT RunOptions")?
            .with_outputs(OutputSelector::no_default().with(self.cfg.output_name.as_str()));

        let mut inputs: Vec<(std::borrow::Cow<'static, str>, DynValue)> = vec![
            (self.cfg.input_ids_name.clone().into(), ids_value),
            (self.cfg.attention_mask_name.clone().into(), mask_value),
        ];

        // Decoder-with-cache exports also want explicit position_ids: [batch, seq_len],
        // values 0..seq_len-1 per row. Simple absolute positions are correct here since
        // there's no actual cached prefix on this call (past_key_values are empty).
        let position_ids: Vec<i64> = (0..max_len as i64).cycle().take(batch * max_len).collect();
        let position_ids_array = Array2::from_shape_vec((batch, max_len), position_ids)
            .context("failed to reshape position_ids into [batch, max_len]")?;
        let position_ids_value = Value::from_array(position_ids_array)
            .context("failed to wrap position_ids as an ORT Value")?
            .into_dyn();
        inputs.push(("position_ids".into(), position_ids_value));

        if let Some(kv) = &self.cfg.kv_cache {
            for layer in 0..kv.num_layers {
                let empty_kv =
                    ndarray::Array4::<f32>::zeros((batch, kv.num_kv_heads, 0, kv.head_dim));
                let key_value = Value::from_array(empty_kv.clone())
                    .context("failed to build empty past_key_values key tensor")?
                    .into_dyn();
                let val_value = Value::from_array(empty_kv)
                    .context("failed to build empty past_key_values value tensor")?
                    .into_dyn();
                inputs.push((format!("past_key_values.{layer}.key").into(), key_value));
                inputs.push((format!("past_key_values.{layer}.value").into(), val_value));
            }
        }

        let mut session_guard = self.session.lock().unwrap();
        let outputs = session_guard
            .run_with_options(inputs, &run_options)
            .context(
                "ONNX text-embed inference failed - check input_ids_name/attention_mask_name/\
         output_name and past_key_values shapes match the model's actual graph",
            )?;

        let first_output = outputs
            .iter()
            .next()
            .context("ONNX text-embed model produced no outputs")?
            .1;
        let (shape, data) = first_output
            .try_extract_tensor::<f32>()
            .context("failed to extract f32 tensor from ONNX text-embed output")?;
        let dims: Vec<usize> = shape.iter().map(|d| *d as usize).collect();
        tracing::info!("Apex Vector: ONNX text-embed output dims = {:?}", dims);

        let mut results = Vec::with_capacity(batch);
        match dims.len() {
            2 => {
                // Already pooled (e.g. a "sentence_embedding" output) - use directly.
                let hidden = dims[1];
                for i in 0..batch {
                    let mut v = data[i * hidden..(i + 1) * hidden].to_vec();
                    l2_normalize(&mut v);
                    results.push(v);
                }
            }
            3 => {
                // [batch, seq, hidden] raw hidden states - pool manually per cfg.pooling.
                let seq = dims[1];
                let hidden = dims[2];
                for i in 0..batch {
                    let mut pooled = vec![0f32; hidden];
                    match self.cfg.pooling {
                        Pooling::MaskedMean => {
                            let mut count = 0f32;
                            for s in 0..seq {
                                if masks_u32[i][s] == 1 {
                                    count += 1.0;
                                    for h in 0..hidden {
                                        pooled[h] += data[i * seq * hidden + s * hidden + h];
                                    }
                                }
                            }
                            let count = count.max(1.0);
                            for h in pooled.iter_mut() {
                                *h /= count;
                            }
                        }
                        Pooling::LastToken => {
                            let last = masks_u32[i].iter().rposition(|&m| m == 1).unwrap_or(0);
                            for h in 0..hidden {
                                pooled[h] = data[i * seq * hidden + last * hidden + h];
                            }
                        }
                    }
                    l2_normalize(&mut pooled);
                    results.push(pooled);
                }
            }
            other => bail!(
                "unexpected ONNX text-embed output rank {other} (dims={:?}) - check output_name \
                 actually points at a [batch, hidden] or [batch, seq, hidden] tensor",
                dims
            ),
        }
        Ok(results)
    }
}

fn l2_normalize(v: &mut [f32]) {
    let mag = v.iter().map(|x| x * x).sum::<f32>().sqrt() + 1e-12;
    for x in v.iter_mut() {
        *x /= mag;
    }
}
