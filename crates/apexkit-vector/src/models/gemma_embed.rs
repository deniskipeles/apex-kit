// A self-contained, bidirectional (non-causal) Gemma-architecture transformer.
//
// EmbeddingGemma is NOT the causal decoder you'd use for text generation - it's a
// bidirectional encoder built on the Gemma3 architecture (full self-attention, no causal
// mask) with mean pooling over the sequence to produce a single embedding vector.
// That's why we implement it here directly rather than reusing a causal Gemma decoder.
//
// Architecture per layer (standard Gemma block):
//   x = x + Attn(RMSNorm(x))
//   x = x + MLP(RMSNorm(x))     where MLP is SwiGLU: down(silu(gate(x)) * up(x))
//
// Gemma-specific details implemented here:
//   - Token embeddings are scaled by sqrt(hidden_size) after lookup.
//   - RMSNorm uses (1 + weight) scaling, not just `weight` (a real Gemma quirk -
//     get it wrong and every layer's output is silently off).
//   - Rotary position embeddings (RoPE) applied per attention head.
//   - Grouped-query attention (num_key_value_heads can be < num_attention_heads).
//   - Attention is BIDIRECTIONAL: we only mask out padding tokens, never future tokens.
//
// NOTE ON SAFETENSORS KEY NAMES: these match the standard HF Gemma/Gemma2/Gemma3 naming
// convention ("model.embed_tokens.weight", "model.layers.N.self_attn.q_proj.weight", etc).
// If loading fails with a "tensor not found" error, call `dump_tensor_names()` on the raw
// safetensors map and diff it against the key strings below - checkpoint authors
// occasionally deviate (e.g. no "model." prefix, or fused qkv instead of split q/k/v).

use anyhow::{Context, Result, bail};
use candle_core::{D, DType, Device, IndexOp, Tensor};
use candle_nn::{Linear, Module, VarBuilder};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize)]
pub struct GemmaEmbedConfig {
    pub vocab_size: usize,
    pub hidden_size: usize,
    pub intermediate_size: usize,
    pub num_hidden_layers: usize,
    pub num_attention_heads: usize,
    #[serde(default)]
    pub num_key_value_heads: Option<usize>,
    #[serde(default)]
    pub head_dim: Option<usize>,
    #[serde(default = "default_rms_eps")]
    pub rms_norm_eps: f64,
    #[serde(default = "default_rope_theta")]
    pub rope_theta: f64,
    #[serde(default = "default_max_pos")]
    pub max_position_embeddings: usize,
}

fn default_rms_eps() -> f64 {
    1e-6
}
fn default_rope_theta() -> f64 {
    10000.0
}
fn default_max_pos() -> usize {
    8192
}

impl GemmaEmbedConfig {
    fn n_kv_heads(&self) -> usize {
        self.num_key_value_heads.unwrap_or(self.num_attention_heads)
    }
    fn head_dim(&self) -> usize {
        self.head_dim
            .unwrap_or(self.hidden_size / self.num_attention_heads)
    }
}

/// Dump tensor names + shapes for debugging key-name mismatches against a checkpoint.
pub fn dump_tensor_names(tensors: &HashMap<String, Tensor>) {
    let mut names: Vec<_> = tensors.keys().cloned().collect();
    names.sort();
    for n in names {
        if let Some(t) = tensors.get(&n) {
            tracing::info!("tensor: {n}  shape={:?}", t.dims());
        }
    }
}

// ---------------------------------------------------------------------------
// RMSNorm with Gemma's (1 + weight) scaling
// ---------------------------------------------------------------------------
struct GemmaRmsNorm {
    weight: Tensor,
    eps: f64,
}

impl GemmaRmsNorm {
    fn load(vb: &VarBuilder, name: &str, size: usize, eps: f64) -> Result<Self> {
        let weight = vb
            .get(size, name)
            .context(format!("missing tensor {name}"))?;
        Ok(Self { weight, eps })
    }

    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        let dtype = x.dtype();
        let x = x.to_dtype(DType::F32)?;
        let variance = x.sqr()?.mean_keepdim(D::Minus1)?;
        let normed = x.broadcast_div(&(variance + self.eps)?.sqrt()?)?;
        let normed = normed.to_dtype(dtype)?;
        // Gemma quirk: scale by (1 + weight), not weight directly.
        let one_plus_w = (self.weight.to_dtype(dtype)? + 1.0)?;
        Ok(normed.broadcast_mul(&one_plus_w)?)
    }
}

// ---------------------------------------------------------------------------
// Rotary position embeddings
// ---------------------------------------------------------------------------
struct RotaryEmbedding {
    cos: Tensor,
    sin: Tensor,
}

impl RotaryEmbedding {
    fn new(head_dim: usize, max_pos: usize, theta: f64, device: &Device) -> Result<Self> {
        let half = head_dim / 2;
        let inv_freq: Vec<f32> = (0..half)
            .map(|i| 1f32 / (theta as f32).powf((2 * i) as f32 / head_dim as f32))
            .collect();
        let inv_freq = Tensor::from_vec(inv_freq, half, device)?;
        let positions: Vec<f32> = (0..max_pos).map(|p| p as f32).collect();
        let positions = Tensor::from_vec(positions, max_pos, device)?;
        // [max_pos, half]
        let freqs = positions
            .reshape((max_pos, 1))?
            .broadcast_mul(&inv_freq.reshape((1, half))?)?;
        let cos = freqs.cos()?;
        let sin = freqs.sin()?;
        Ok(Self { cos, sin })
    }

    fn apply(&self, x: &Tensor, seq_len: usize) -> Result<Tensor> {
        // x: [batch, n_heads, seq, head_dim]
        let head_dim = x.dim(D::Minus1)?;
        let half = head_dim / 2;
        let cos = self.cos.i((..seq_len, ..))?; // [seq, half]
        let sin = self.sin.i((..seq_len, ..))?;
        let cos = cos.reshape((1, 1, seq_len, half))?;
        let sin = sin.reshape((1, 1, seq_len, half))?;

        let x1 = x.narrow(D::Minus1, 0, half)?;
        let x2 = x.narrow(D::Minus1, half, half)?;

        // standard "rotate half" RoPE
        let rx1 = (x1.broadcast_mul(&cos)? - x2.broadcast_mul(&sin)?)?;
        let rx2 = (x2.broadcast_mul(&cos)? + x1.broadcast_mul(&sin)?)?;
        Ok(Tensor::cat(&[rx1, rx2], D::Minus1)?)
    }
}

// ---------------------------------------------------------------------------
// Attention (grouped-query, bidirectional - no causal mask)
// ---------------------------------------------------------------------------
struct Attention {
    q_proj: Linear,
    k_proj: Linear,
    v_proj: Linear,
    o_proj: Linear,
    n_heads: usize,
    n_kv_heads: usize,
    head_dim: usize,
}

impl Attention {
    fn load(vb: &VarBuilder, prefix: &str, cfg: &GemmaEmbedConfig) -> Result<Self> {
        let h = cfg.hidden_size;
        let head_dim = cfg.head_dim();
        let n_heads = cfg.num_attention_heads;
        let n_kv_heads = cfg.n_kv_heads();

        let q_proj = linear_no_bias(
            vb,
            &format!("{prefix}.q_proj.weight"),
            h,
            n_heads * head_dim,
        )?;
        let k_proj = linear_no_bias(
            vb,
            &format!("{prefix}.k_proj.weight"),
            h,
            n_kv_heads * head_dim,
        )?;
        let v_proj = linear_no_bias(
            vb,
            &format!("{prefix}.v_proj.weight"),
            h,
            n_kv_heads * head_dim,
        )?;
        let o_proj = linear_no_bias(
            vb,
            &format!("{prefix}.o_proj.weight"),
            n_heads * head_dim,
            h,
        )?;

        Ok(Self {
            q_proj,
            k_proj,
            v_proj,
            o_proj,
            n_heads,
            n_kv_heads,
            head_dim,
        })
    }

    fn forward(&self, x: &Tensor, rope: &RotaryEmbedding, attn_bias: &Tensor) -> Result<Tensor> {
        let (b, seq, _) = x.dims3()?;

        let q = self
            .q_proj
            .forward(x)?
            .reshape((b, seq, self.n_heads, self.head_dim))?
            .transpose(1, 2)?; // [b, h, seq, d]
        let k = self
            .k_proj
            .forward(x)?
            .reshape((b, seq, self.n_kv_heads, self.head_dim))?
            .transpose(1, 2)?;
        let v = self
            .v_proj
            .forward(x)?
            .reshape((b, seq, self.n_kv_heads, self.head_dim))?
            .transpose(1, 2)?;

        let q = rope.apply(&q.contiguous()?, seq)?;
        let k = rope.apply(&k.contiguous()?, seq)?;

        // expand kv heads for GQA if needed
        let rep = self.n_heads / self.n_kv_heads;
        let (k, v) = if rep > 1 {
            (repeat_kv(&k, rep)?, repeat_kv(&v, rep)?)
        } else {
            (k, v)
        };

        let scale = 1f64 / (self.head_dim as f64).sqrt();
        let attn_scores = (q.matmul(&k.transpose(D::Minus2, D::Minus1)?.contiguous()?)? * scale)?;
        // attn_bias: [b, 1, 1, seq] additive padding mask (0 for keep, -inf for pad)
        let attn_scores = attn_scores.broadcast_add(attn_bias)?;
        let attn_probs = candle_nn::ops::softmax_last_dim(&attn_scores)?;

        let out = attn_probs.matmul(&v.contiguous()?)?; // [b, h, seq, d]
        let out = out
            .transpose(1, 2)?
            .reshape((b, seq, self.n_heads * self.head_dim))?;
        Ok(self.o_proj.forward(&out)?)
    }
}

fn repeat_kv(x: &Tensor, rep: usize) -> Result<Tensor> {
    let (b, n_kv, seq, d) = x.dims4()?;
    let x = x
        .unsqueeze(2)?
        .expand((b, n_kv, rep, seq, d))?
        .reshape((b, n_kv * rep, seq, d))?;
    Ok(x)
}

fn linear_no_bias(vb: &VarBuilder, name: &str, in_dim: usize, out_dim: usize) -> Result<Linear> {
    let w = vb
        .get((out_dim, in_dim), name)
        .context(format!("missing tensor {name}"))?;
    Ok(Linear::new(w, None))
}

// ---------------------------------------------------------------------------
// SwiGLU MLP
// ---------------------------------------------------------------------------
struct Mlp {
    gate_proj: Linear,
    up_proj: Linear,
    down_proj: Linear,
}

impl Mlp {
    fn load(vb: &VarBuilder, prefix: &str, cfg: &GemmaEmbedConfig) -> Result<Self> {
        let h = cfg.hidden_size;
        let i = cfg.intermediate_size;
        Ok(Self {
            gate_proj: linear_no_bias(vb, &format!("{prefix}.gate_proj.weight"), h, i)?,
            up_proj: linear_no_bias(vb, &format!("{prefix}.up_proj.weight"), h, i)?,
            down_proj: linear_no_bias(vb, &format!("{prefix}.down_proj.weight"), i, h)?,
        })
    }

    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        let gate = candle_nn::ops::silu(&self.gate_proj.forward(x)?)?;
        let up = self.up_proj.forward(x)?;
        Ok(self.down_proj.forward(&(gate * up)?)?)
    }
}

// ---------------------------------------------------------------------------
// Decoder (encoder, here) layer
// ---------------------------------------------------------------------------
struct Layer {
    input_norm: GemmaRmsNorm,
    attn: Attention,
    post_attn_norm: GemmaRmsNorm,
    mlp: Mlp,
}

impl Layer {
    fn load(vb: &VarBuilder, prefix: &str, cfg: &GemmaEmbedConfig) -> Result<Self> {
        Ok(Self {
            input_norm: GemmaRmsNorm::load(
                vb,
                &format!("{prefix}.input_layernorm.weight"),
                cfg.hidden_size,
                cfg.rms_norm_eps,
            )?,
            attn: Attention::load(vb, &format!("{prefix}.self_attn"), cfg)?,
            post_attn_norm: GemmaRmsNorm::load(
                vb,
                &format!("{prefix}.post_attention_layernorm.weight"),
                cfg.hidden_size,
                cfg.rms_norm_eps,
            )?,
            mlp: Mlp::load(vb, &format!("{prefix}.mlp"), cfg)?,
        })
    }

    fn forward(&self, x: &Tensor, rope: &RotaryEmbedding, attn_bias: &Tensor) -> Result<Tensor> {
        let residual = x;
        let h = self.input_norm.forward(x)?;
        let h = self.attn.forward(&h, rope, attn_bias)?;
        let x = (residual + h)?;

        let residual = &x;
        let h = self.post_attn_norm.forward(&x)?;
        let h = self.mlp.forward(&h)?;
        Ok((residual + h)?)
    }
}

// ---------------------------------------------------------------------------
// Full model
// ---------------------------------------------------------------------------
pub struct GemmaEmbedModel {
    embed_tokens: Tensor, // [vocab, hidden] - kept raw so we can index_select directly
    layers: Vec<Layer>,
    final_norm: GemmaRmsNorm,
    rope: RotaryEmbedding,
    cfg: GemmaEmbedConfig,
    device: Device,
}

impl GemmaEmbedModel {
    pub fn load(vb: VarBuilder, cfg: GemmaEmbedConfig, device: &Device) -> Result<Self> {
        let embed_tokens = vb
            .get(
                (cfg.vocab_size, cfg.hidden_size),
                "model.embed_tokens.weight",
            )
            .context("missing model.embed_tokens.weight")?;

        let mut layers = Vec::with_capacity(cfg.num_hidden_layers);
        for i in 0..cfg.num_hidden_layers {
            let prefix = format!("model.layers.{i}");
            layers.push(Layer::load(&vb, &prefix, &cfg)?);
        }

        let final_norm =
            GemmaRmsNorm::load(&vb, "model.norm.weight", cfg.hidden_size, cfg.rms_norm_eps)?;
        let rope = RotaryEmbedding::new(
            cfg.head_dim(),
            cfg.max_position_embeddings,
            cfg.rope_theta,
            device,
        )?;

        Ok(Self {
            embed_tokens,
            layers,
            final_norm,
            rope,
            cfg,
            device: device.clone(),
        })
    }

    /// Returns last-hidden-state: [batch, seq, hidden]. Caller does pooling.
    /// `attention_mask` is [batch, seq] with 1 = real token, 0 = padding.
    pub fn forward(&self, input_ids: &Tensor, attention_mask: &Tensor) -> Result<Tensor> {
        let (b, seq) = input_ids.dims2()?;
        if seq > self.cfg.max_position_embeddings {
            bail!(
                "sequence length {seq} exceeds max_position_embeddings {}",
                self.cfg.max_position_embeddings
            );
        }

        // Embedding lookup via index_select, then Gemma's sqrt(hidden_size) scaling.
        let flat_ids = input_ids.flatten_all()?;
        let embeds = self.embed_tokens.index_select(&flat_ids, 0)?;
        let hidden = self.cfg.hidden_size;
        let mut x = embeds.reshape((b, seq, hidden))?;
        let scale = (hidden as f64).sqrt();
        x = (x * scale)?;

        // Build additive bidirectional padding mask: [b, 1, 1, seq], 0 for keep, large-neg for pad.
        let mask_f32 = attention_mask.to_dtype(DType::F32)?;
        let neg_inf = ((mask_f32.ones_like()? - &mask_f32)? * f64::from(f32::MIN / 2.0))?;
        let attn_bias = neg_inf.reshape((b, 1, 1, seq))?;

        for layer in &self.layers {
            x = layer.forward(&x, &self.rope, &attn_bias)?;
        }
        self.final_norm.forward(&x)
    }

    pub fn device(&self) -> &Device {
        &self.device
    }
}

/// Masked mean pooling: average only over real (non-padding) tokens.
pub fn masked_mean_pool(hidden: &Tensor, attention_mask: &Tensor) -> Result<Tensor> {
    let mask = attention_mask.to_dtype(hidden.dtype())?; // [b, seq]
    let (b, seq) = mask.dims2()?;
    let mask_3d = mask.reshape((b, seq, 1))?;
    let masked_hidden = hidden.broadcast_mul(&mask_3d)?;
    let summed = masked_hidden.sum(1)?; // [b, hidden]
    let counts = mask.sum(1)?.reshape((b, 1))?; // [b, 1]
    let counts = (counts + 1e-9)?;
    Ok(summed.broadcast_div(&counts)?)
}
