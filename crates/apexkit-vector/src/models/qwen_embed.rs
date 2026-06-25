// Qwen3-Embedding-style text encoder.
//
// Unlike Gemma's embedding variant (bidirectional), Qwen embedding models are CAUSAL
// decoders - same architecture family as the chat models, just used differently at
// inference time: instead of generating tokens, you run a forward pass and take the
// hidden state of the LAST real (non-padding) token as the embedding. That works because
// causal attention means the last token's hidden state has already "seen" every token
// before it - it's the only position with full-sequence context in a causal model.
//
// This means two pooling-related things are different from the Gemma/BERT paths in this
// crate, and getting them wrong silently produces garbage:
//   1. Pooling is LAST-TOKEN, not mean-pooling. Averaging a causal model's hidden states
//      (like you would for BERT/Gemma) mixes in early-position states that only saw a
//      few tokens of context - it dilutes the embedding with half-formed representations.
//   2. "Last real token" must account for padding. If a batch right-pads shorter
//      sequences, the last real token of a short sequence is NOT at the final tensor
//      index - it's wherever attention_mask transitions from 1 to 0. We find that
//      per-sequence rather than assuming index `seq_len - 1`.
//
// Architecture per layer (Qwen2/Qwen3 decoder block):
//   x = x + Attn(RMSNorm(x))     - causal self-attention, GQA, RoPE, optional QK-norm
//   x = x + MLP(RMSNorm(x))      - SwiGLU: down(silu(gate(x)) * up(x))
//
// Qwen-specific details implemented here:
//   - Standard RMSNorm (plain `weight` scaling - NOT Gemma's `1 + weight` quirk).
//   - No embedding-scale-by-sqrt(hidden_size) (that's a Gemma-only thing).
//   - Optional per-head QK-RMSNorm (Qwen3 applies RMSNorm to Q and K projections before
//     RoPE; Qwen2 does not). Controlled by `cfg.use_qk_norm`, auto-detected at load time
//     by checking whether the relevant tensors exist.
//   - Causal mask combined with the padding mask (both as one additive bias).
//   - Attention bias terms use Q/K/V *with* bias (Qwen2/2.5 have qkv bias; Qwen3 generally
//     doesn't). We load bias tensors if present, otherwise fall back to bias-free.
//
// PROMPT FORMAT: Qwen3-Embedding was trained with an instruction-style prefix, distinct
// for queries vs documents:
//   query:    "Instruct: Given a query, retrieve relevant documents.\nQuery: {text}"
//   document: "{text}"   (no prefix - documents are embedded raw)
// This is different from EmbeddingGemma's symmetric prefixing on both sides, so don't
// copy-paste the Gemma prefix logic here; see `QwenTaskPrefix` in embedder.rs.
//
// NOTE ON SAFETENSORS KEY NAMES: standard HF Qwen2/Qwen3 naming:
//   model.embed_tokens.weight
//   model.layers.{i}.input_layernorm.weight
//   model.layers.{i}.self_attn.{q,k,v,o}_proj.{weight,bias?}
//   model.layers.{i}.self_attn.{q,k}_norm.weight       (Qwen3 only, optional)
//   model.layers.{i}.post_attention_layernorm.weight
//   model.layers.{i}.mlp.{gate,up,down}_proj.weight
//   model.norm.weight
// Use `dump_tensor_names` (in gemma_embed.rs) against your checkpoint if loading fails.

use anyhow::{Context, Result, bail};
use candle_core::{D, DType, Device, IndexOp, Tensor};
use candle_nn::{Linear, Module, VarBuilder};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct QwenEmbedConfig {
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
    /// If true, apply RMSNorm to Q/K projections per-head before RoPE (Qwen3).
    /// Auto-overridden at load time based on tensor presence - the config.json default
    /// here is just a fallback if detection is inconclusive.
    #[serde(default)]
    pub use_qk_norm: bool,
    /// Whether qkv/o projections have bias terms (Qwen2.x: yes, Qwen3: usually no).
    /// Also auto-detected at load time.
    #[serde(default)]
    pub attention_bias: bool,
}

fn default_rms_eps() -> f64 {
    1e-6
}
fn default_rope_theta() -> f64 {
    1000000.0
}
fn default_max_pos() -> usize {
    32768
}

impl QwenEmbedConfig {
    fn n_kv_heads(&self) -> usize {
        self.num_key_value_heads.unwrap_or(self.num_attention_heads)
    }
    fn head_dim(&self) -> usize {
        self.head_dim
            .unwrap_or(self.hidden_size / self.num_attention_heads)
    }
}

// ---------------------------------------------------------------------------
// Standard RMSNorm (plain weight scaling, unlike Gemma's 1+weight)
// ---------------------------------------------------------------------------
struct RmsNorm {
    weight: Tensor,
    eps: f64,
}

impl RmsNorm {
    fn load(vb: &VarBuilder, name: &str, size: usize, eps: f64) -> Result<Self> {
        let weight = vb
            .get(size, name)
            .context(format!("missing tensor {name}"))?;
        Ok(Self { weight, eps })
    }

    fn try_load_optional(vb: &VarBuilder, name: &str, size: usize, eps: f64) -> Option<Self> {
        vb.get(size, name).ok().map(|weight| Self { weight, eps })
    }

    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        let dtype = x.dtype();
        let xf = x.to_dtype(DType::F32)?;
        let variance = xf.sqr()?.mean_keepdim(D::Minus1)?;
        let normed = xf.broadcast_div(&(variance + self.eps)?.sqrt()?)?;
        let normed = normed.to_dtype(dtype)?;
        Ok(normed.broadcast_mul(&self.weight.to_dtype(dtype)?)?)
    }
}

// ---------------------------------------------------------------------------
// Rotary embeddings (same construction as Gemma's; standard RoPE)
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
        let freqs = positions
            .reshape((max_pos, 1))?
            .broadcast_mul(&inv_freq.reshape((1, half))?)?;
        Ok(Self {
            cos: freqs.cos()?,
            sin: freqs.sin()?,
        })
    }

    fn apply(&self, x: &Tensor, seq_len: usize) -> Result<Tensor> {
        let head_dim = x.dim(D::Minus1)?;
        let half = head_dim / 2;
        let cos = self
            .cos
            .i((..seq_len, ..))?
            .reshape((1, 1, seq_len, half))?;
        let sin = self
            .sin
            .i((..seq_len, ..))?
            .reshape((1, 1, seq_len, half))?;

        let x1 = x.narrow(D::Minus1, 0, half)?;
        let x2 = x.narrow(D::Minus1, half, half)?;

        let rx1 = (x1.broadcast_mul(&cos)? - x2.broadcast_mul(&sin)?)?;
        let rx2 = (x2.broadcast_mul(&cos)? + x1.broadcast_mul(&sin)?)?;
        Ok(Tensor::cat(&[rx1, rx2], D::Minus1)?)
    }
}

fn linear(
    vb: &VarBuilder,
    prefix: &str,
    in_dim: usize,
    out_dim: usize,
    with_bias: bool,
) -> Result<Linear> {
    let w = vb
        .get((out_dim, in_dim), &format!("{prefix}.weight"))
        .context(format!("missing {prefix}.weight"))?;
    let b = if with_bias {
        vb.get(out_dim, &format!("{prefix}.bias")).ok()
    } else {
        None
    };
    Ok(Linear::new(w, b))
}

// ---------------------------------------------------------------------------
// Attention: causal, GQA, optional per-head QK-RMSNorm (Qwen3)
// ---------------------------------------------------------------------------
struct Attention {
    q_proj: Linear,
    k_proj: Linear,
    v_proj: Linear,
    o_proj: Linear,
    q_norm: Option<RmsNorm>,
    k_norm: Option<RmsNorm>,
    n_heads: usize,
    n_kv_heads: usize,
    head_dim: usize,
}

impl Attention {
    fn load(vb: &VarBuilder, prefix: &str, cfg: &QwenEmbedConfig) -> Result<Self> {
        let h = cfg.hidden_size;
        let head_dim = cfg.head_dim();
        let n_heads = cfg.num_attention_heads;
        let n_kv_heads = cfg.n_kv_heads();
        let bias = cfg.attention_bias;

        let q_proj = linear(vb, &format!("{prefix}.q_proj"), h, n_heads * head_dim, bias)?;
        let k_proj = linear(
            vb,
            &format!("{prefix}.k_proj"),
            h,
            n_kv_heads * head_dim,
            bias,
        )?;
        let v_proj = linear(
            vb,
            &format!("{prefix}.v_proj"),
            h,
            n_kv_heads * head_dim,
            bias,
        )?;
        let o_proj = linear(vb, &format!("{prefix}.o_proj"), n_heads * head_dim, h, bias)?;

        // Qwen3 QK-norm tensors are per-head-dim sized, not per-hidden-size.
        let q_norm = RmsNorm::try_load_optional(
            vb,
            &format!("{prefix}.q_norm.weight"),
            head_dim,
            cfg.rms_norm_eps,
        );
        let k_norm = RmsNorm::try_load_optional(
            vb,
            &format!("{prefix}.k_norm.weight"),
            head_dim,
            cfg.rms_norm_eps,
        );

        Ok(Self {
            q_proj,
            k_proj,
            v_proj,
            o_proj,
            q_norm,
            k_norm,
            n_heads,
            n_kv_heads,
            head_dim,
        })
    }

    fn forward(&self, x: &Tensor, rope: &RotaryEmbedding, attn_bias: &Tensor) -> Result<Tensor> {
        let (b, seq, _) = x.dims3()?;

        let mut q = self
            .q_proj
            .forward(x)?
            .reshape((b, seq, self.n_heads, self.head_dim))?;
        let mut k = self
            .k_proj
            .forward(x)?
            .reshape((b, seq, self.n_kv_heads, self.head_dim))?;
        let v = self
            .v_proj
            .forward(x)?
            .reshape((b, seq, self.n_kv_heads, self.head_dim))?
            .transpose(1, 2)?
            .contiguous()?;

        // QK-norm (Qwen3) is applied per-head, on the last dim, before transpose/RoPE.
        if let Some(qn) = &self.q_norm {
            q = qn.forward(&q)?;
        }
        if let Some(kn) = &self.k_norm {
            k = kn.forward(&k)?;
        }

        let q = q.transpose(1, 2)?.contiguous()?; // [b, h, seq, d]
        let k = k.transpose(1, 2)?.contiguous()?;

        let q = rope.apply(&q, seq)?;
        let k = rope.apply(&k, seq)?;

        let rep = self.n_heads / self.n_kv_heads;
        let (k, v) = if rep > 1 {
            (repeat_kv(&k, rep)?, repeat_kv(&v, rep)?)
        } else {
            (k, v)
        };

        let scale = 1f64 / (self.head_dim as f64).sqrt();
        let scores = (q.matmul(&k.transpose(D::Minus2, D::Minus1)?.contiguous()?)? * scale)?;
        let scores = scores.broadcast_add(attn_bias)?;
        let probs = candle_nn::ops::softmax_last_dim(&scores)?;

        let out = probs.matmul(&v)?;
        let out = out
            .transpose(1, 2)?
            .reshape((b, seq, self.n_heads * self.head_dim))?;
        Ok(self.o_proj.forward(&out)?)
    }
}

fn repeat_kv(x: &Tensor, rep: usize) -> Result<Tensor> {
    let (b, n_kv, seq, d) = x.dims4()?;
    Ok(x.unsqueeze(2)?
        .expand((b, n_kv, rep, seq, d))?
        .reshape((b, n_kv * rep, seq, d))?)
}

struct Mlp {
    gate_proj: Linear,
    up_proj: Linear,
    down_proj: Linear,
}

impl Mlp {
    fn load(vb: &VarBuilder, prefix: &str, cfg: &QwenEmbedConfig) -> Result<Self> {
        let h = cfg.hidden_size;
        let i = cfg.intermediate_size;
        Ok(Self {
            gate_proj: linear(vb, &format!("{prefix}.gate_proj"), h, i, false)?,
            up_proj: linear(vb, &format!("{prefix}.up_proj"), h, i, false)?,
            down_proj: linear(vb, &format!("{prefix}.down_proj"), i, h, false)?,
        })
    }

    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        let gate = candle_nn::ops::silu(&self.gate_proj.forward(x)?)?;
        let up = self.up_proj.forward(x)?;
        Ok(self.down_proj.forward(&(gate * up)?)?)
    }
}

struct Layer {
    input_norm: RmsNorm,
    attn: Attention,
    post_attn_norm: RmsNorm,
    mlp: Mlp,
}

impl Layer {
    fn load(vb: &VarBuilder, prefix: &str, cfg: &QwenEmbedConfig) -> Result<Self> {
        Ok(Self {
            input_norm: RmsNorm::load(
                vb,
                &format!("{prefix}.input_layernorm.weight"),
                cfg.hidden_size,
                cfg.rms_norm_eps,
            )?,
            attn: Attention::load(vb, &format!("{prefix}.self_attn"), cfg)?,
            post_attn_norm: RmsNorm::load(
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

pub struct QwenEmbedModel {
    embed_tokens: Tensor,
    layers: Vec<Layer>,
    final_norm: RmsNorm,
    rope: RotaryEmbedding,
    cfg: QwenEmbedConfig,
    device: Device,
}

impl QwenEmbedModel {
    pub fn load(vb: VarBuilder, mut cfg: QwenEmbedConfig, device: &Device) -> Result<Self> {
        let embed_tokens = vb
            .get(
                (cfg.vocab_size, cfg.hidden_size),
                "embed_tokens.weight",
            )
            .context("missing embed_tokens.weight")?;

        // Auto-detect QK-norm / attention-bias presence from layer 0 rather than trusting
        // config.json, since some Qwen config exports omit these fields entirely.
        let head_dim = cfg.head_dim();
        cfg.use_qk_norm = vb
            .get(head_dim, "layers.0.self_attn.q_norm.weight")
            .is_ok();
        cfg.attention_bias = vb
            .get(
                cfg.num_attention_heads * head_dim,
                "layers.0.self_attn.q_proj.bias",
            )
            .is_ok();

        let mut layers = Vec::with_capacity(cfg.num_hidden_layers);
        for i in 0..cfg.num_hidden_layers {
            let prefix = format!("layers.{i}");
            layers.push(Layer::load(&vb, &prefix, &cfg)?);
        }

        let final_norm =
            RmsNorm::load(&vb, "norm.weight", cfg.hidden_size, cfg.rms_norm_eps)?;
        let rope = RotaryEmbedding::new(
            head_dim,
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

    /// Returns last-hidden-state: [batch, seq, hidden].
    /// `attention_mask` is [batch, seq], 1 = real token, 0 = padding (RIGHT padding assumed -
    /// last-token pooling below depends on this).
    pub fn forward(&self, input_ids: &Tensor, attention_mask: &Tensor) -> Result<Tensor> {
        let (b, seq) = input_ids.dims2()?;
        if seq > self.cfg.max_position_embeddings {
            bail!(
                "sequence length {seq} exceeds max_position_embeddings {}",
                self.cfg.max_position_embeddings
            );
        }

        let flat_ids = input_ids.flatten_all()?;
        let embeds = self.embed_tokens.index_select(&flat_ids, 0)?;
        let hidden = self.cfg.hidden_size;
        let mut x = embeds.reshape((b, seq, hidden))?;

        // Combined causal + padding additive mask: [b, 1, seq, seq].
        // causal_bias[i,j] = 0 if j <= i else -inf  (can't attend to future positions)
        // padding additionally zeroes out (sets -inf for) any key position j that's padding.
        let device = x.device();
        let mut causal_vals = vec![0f32; seq * seq];
        for i in 0..seq {
            for j in (i + 1)..seq {
                causal_vals[i * seq + j] = f32::MIN / 2.0;
            }
        }
        let causal = Tensor::from_vec(causal_vals, (1, 1, seq, seq), device)?;

        let mask_f32 = attention_mask.to_dtype(DType::F32)?; // [b, seq], 1=keep
        let pad_bias = ((mask_f32.ones_like()? - &mask_f32)? * f64::from(f32::MIN / 2.0))?
            .reshape((b, 1, 1, seq))?; // broadcast over query positions, mask key positions

        let attn_bias = causal.broadcast_add(&pad_bias)?;

        for layer in &self.layers {
            x = layer.forward(&x, &self.rope, &attn_bias)?;
        }
        self.final_norm.forward(&x)
    }

    pub fn device(&self) -> &Device {
        &self.device
    }
}

/// Last-real-token pooling. For each sequence in the batch, finds the index of the last
/// position where attention_mask == 1 (handles right-padding correctly) and extracts that
/// position's hidden state as the embedding.
pub fn last_token_pool(hidden: &Tensor, attention_mask: &[Vec<u32>]) -> Result<Tensor> {
    let (b, _seq, h) = hidden.dims3()?;
    let mut rows = Vec::with_capacity(b);
    for (batch_idx, mask_row) in attention_mask.iter().enumerate().take(b) {
        let last_real_idx = mask_row.iter().rposition(|&m| m == 1).unwrap_or(0);
        let row = hidden.i((batch_idx, last_real_idx, ..))?.reshape((1, h))?;
        rows.push(row);
    }
    Ok(Tensor::cat(&rows, 0)?)
}
