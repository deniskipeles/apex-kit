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
//
// SLIDING-WINDOW CHUNKING (single-text path only):
// The candle backends (BERT/Gemma/Qwen) already split long single-text inputs into
// overlapping token windows in `embedder.rs::embed_with_task` before this module
// existed - but that windowing happened OUTSIDE the backend match, so it silently never
// applied to the ONNX text path (which short-circuits with its own `onnx.embed(...)`
// call before the windowing loop is ever reached). That means any input longer than the
// model's effective context window was previously truncated/garbled rather than chunked.
// `embed_windowed` below mirrors the exact windowing strategy used for the candle
// backends (same window_size/overlap/stride math, same "L2-normalize each window, sum,
// average, re-normalize" combination), just operating on pre-tokenized ids fed through
// `forward_batch` instead of through a candle forward pass.

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
    /// [NEW] Whether the ONNX graph expects an explicit `position_ids` input tensor
    pub requires_position_ids: bool,
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
            requires_position_ids: false, // [NEW] Gemma ONNX graph does not take position_ids
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
            requires_position_ids: true, // [NEW] Qwen3 decoder graph requires position_ids
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

    /// Single-text convenience wrapper around `embed_batch`. NOTE: does NOT apply
    /// sliding-window chunking - this re-tokenizes `text` as a single unwindowed
    /// sequence, same as before. Callers that need windowing for long inputs (e.g.
    /// CandleEmbedder::embed_with_task) should call `embed_windowed` instead.
    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        Ok(self
            .embed_batch(std::slice::from_ref(&text.to_string()))?
            .remove(0))
    }

    /// Sliding-window version of `embed`, for a single long text. Mirrors the windowing
    /// strategy already used for the candle backends in
    /// `embedder.rs::CandleEmbedder::embed_with_task`:
    ///   - Tokenize once.
    ///   - If the token count fits in `window_size`, just embed it directly (one window).
    ///   - Otherwise, slide a `window_size`-token window across the tokens with stride
    ///     `window_size - overlap` (or `window_size / 2` if `overlap >= window_size`),
    ///     embedding+normalizing each window independently, then average the per-window
    ///     vectors and L2-normalize the result once more.
    ///
    /// Each window is run through the model as its own batch-of-1 sequence (via
    /// `forward_batch`), so windows never bleed into each other's padding/attention.
    pub fn embed_windowed(
        &self,
        text: &str,
        window_size: usize,
        overlap: usize,
    ) -> Result<Vec<f32>> {
        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| anyhow::anyhow!("tokenizer encode error: {e}"))?;
        let token_ids: Vec<u32> = encoding.get_ids().to_vec();
        let total_tokens = token_ids.len();

        // Degenerate/short-input case: no windowing needed, just one pass.
        if window_size == 0 || total_tokens <= window_size {
            let vec = self.forward_batch(&[token_ids])?.remove(0);
            return Ok(vec);
        }

        // Same stride derivation as the candle windowing path: prefer window_size -
        // overlap, but fall back to window_size / 2 if overlap is degenerate (>= window_size).
        let stride = if window_size > overlap {
            window_size - overlap
        } else {
            window_size / 2
        };
        let stride = stride.max(1); // guard against an infinite loop if window_size == 1

        let mut accum: Option<Vec<f32>> = None;
        let mut window_count: usize = 0;
        let mut start_idx = 0usize;

        while start_idx < total_tokens {
            let end_idx = std::cmp::min(start_idx + window_size, total_tokens);
            let window_ids = token_ids[start_idx..end_idx].to_vec();

            // Each window is embedded (and L2-normalized) independently via forward_batch.
            let window_vec = self.forward_batch(&[window_ids])?.remove(0);

            if let Some(ref mut acc) = accum {
                for (i, val) in window_vec.iter().enumerate() {
                    acc[i] += val;
                }
            } else {
                accum = Some(window_vec);
            }
            window_count += 1;

            if end_idx == total_tokens {
                break;
            }
            start_idx += stride;
        }

        let mut final_vec = accum
            .context("internal error: no windows produced despite total_tokens > window_size")?;
        let count_f32 = window_count as f32;
        for v in final_vec.iter_mut() {
            *v /= count_f32;
        }
        l2_normalize(&mut final_vec);
        Ok(final_vec)
    }

    /// Batched embedding: pads to batch-longest, runs once, pools per `cfg.pooling`,
    /// L2-normalizes each result. Tokenizes each text independently (no windowing) -
    /// this path is for document batches where each text is expected to already fit
    /// the model's window.
    pub fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let encodings = self
            .tokenizer
            .encode_batch(texts.to_vec(), true)
            .map_err(|e| anyhow::anyhow!("tokenizer encode_batch error: {e}"))?;

        let ids_batches: Vec<Vec<u32>> =
            encodings.iter().map(|enc| enc.get_ids().to_vec()).collect();

        self.forward_batch(&ids_batches)
    }

    /// Core inference routine, factored out of `embed_batch` so that `embed_windowed`
    /// can run single-window "batches of 1" through the exact same padding/inference/
    /// pooling/normalization logic, without re-tokenizing already-sliced token windows.
    ///
    /// `ids_batches` are raw (unpadded) per-sequence token id vectors. This method pads
    /// every sequence up to the batch's longest sequence (building its own attention
    /// mask from the original per-sequence lengths, since these ids carry no padding/
    /// mask info of their own), runs one ONNX inference call for the whole batch, pools
    /// according to `self.cfg.pooling` (for 3D raw hidden-state outputs) or uses the
    /// output directly (for already-pooled 2D outputs), and L2-normalizes each result.
    fn forward_batch(&self, ids_batches: &[Vec<u32>]) -> Result<Vec<Vec<f32>>> {
        let batch = ids_batches.len();
        if batch == 0 {
            return Ok(vec![]);
        }

        let max_len = ids_batches.iter().map(|ids| ids.len()).max().unwrap_or(0);

        let mut ids_flat = vec![0i64; batch * max_len];
        let mut mask_flat = vec![0i64; batch * max_len];
        let mut masks_u32: Vec<Vec<u32>> = Vec::with_capacity(batch);

        for (i, ids) in ids_batches.iter().enumerate() {
            let mut mask_row = vec![0u32; max_len];
            for (j, &id) in ids.iter().enumerate() {
                ids_flat[i * max_len + j] = id as i64;
                mask_flat[i * max_len + j] = 1;
                mask_row[j] = 1;
            }
            masks_u32.push(mask_row);
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
        // there's no actual cached prefix on this call (past_key_values are empty), and
        // each window/batch row is treated as its own independent sequence starting at
        // position 0 - this matches how the candle windowing path also restarts position
        // indices at 0 for each window rather than carrying global offsets across windows.
        if self.cfg.requires_position_ids {
            let position_ids: Vec<i64> =
                (0..max_len as i64).cycle().take(batch * max_len).collect();
            let position_ids_array = Array2::from_shape_vec((batch, max_len), position_ids)
                .context("failed to reshape position_ids into [batch, max_len]")?;
            let position_ids_value = Value::from_array(position_ids_array)
                .context("failed to wrap position_ids as an ORT Value")?
                .into_dyn();
            inputs.push(("position_ids".into(), position_ids_value));
        }

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
