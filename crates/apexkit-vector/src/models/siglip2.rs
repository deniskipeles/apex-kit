// Self-contained SigLIP2 vision tower for image embeddings (replaces the old CLIP path).
//
// Architecture (standard ViT, pre-norm):
//   patch_embeds = Conv2d(patch_size stride) -> flatten -> + position_embeddings
//   for each layer:
//     x = x + Attn(LayerNorm1(x))        (bidirectional, no mask - vision has no padding here
//                                          since every image is resized to a fixed grid)
//     x = x + MLP(LayerNorm2(x))         (GELU, not SwiGLU - this is a ViT, not a Gemma block)
//   x = LayerNorm_post(x)
//   pooled = mean over patch tokens          <-- see caveat below
//
// CAVEAT: the official SigLIP/SigLIP2 pooling head is an attention-pooling ("MAP") head,
// not a plain mean. We use mean pooling here as a practical, easy-to-verify approximation.
// If you need bit-for-bit parity with the reference implementation's pooled output, you'll
// need to add the attention-pooling head (a learned query vector + single MHA layer over
// the patch tokens) using the "vision_model.head.*" tensors from the checkpoint.
//
// NOTE ON SAFETENSORS KEY NAMES: written against the standard HF SigLIP naming convention:
//   vision_model.embeddings.patch_embedding.{weight,bias}
//   vision_model.embeddings.position_embedding.weight
//   vision_model.encoder.layers.{i}.layer_norm1.{weight,bias}
//   vision_model.encoder.layers.{i}.self_attn.{q,k,v,out}_proj.{weight,bias}
//   vision_model.encoder.layers.{i}.layer_norm2.{weight,bias}
//   vision_model.encoder.layers.{i}.mlp.fc1.{weight,bias}
//   vision_model.encoder.layers.{i}.mlp.fc2.{weight,bias}
//   vision_model.post_layernorm.{weight,bias}
// Call `dump_tensor_names` (in gemma_embed.rs, reused here) against your actual checkpoint
// if loading fails - SigLIP2 variants (so400m vs base, -i18n, etc) can rename things.

use anyhow::{Context, Result};
use candle_core::{D, Tensor};
use candle_nn::{Conv2d, Conv2dConfig, LayerNorm, Linear, Module, VarBuilder};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct SiglipVisionConfig {
    pub hidden_size: usize,
    pub intermediate_size: usize,
    pub num_hidden_layers: usize,
    pub num_attention_heads: usize,
    pub num_channels: usize,
    pub image_size: usize,
    pub patch_size: usize,
    #[serde(default = "default_ln_eps")]
    pub layer_norm_eps: f64,
}

fn default_ln_eps() -> f64 {
    1e-6
}

impl SiglipVisionConfig {
    pub fn siglip2_base_patch16_224() -> Self {
        Self {
            hidden_size: 768,
            intermediate_size: 3072,
            num_hidden_layers: 12,
            num_attention_heads: 12,
            num_channels: 3,
            image_size: 224,
            patch_size: 16,
            layer_norm_eps: 1e-6,
        }
    }

    fn num_patches(&self) -> usize {
        (self.image_size / self.patch_size).pow(2)
    }
}

struct VisAttention {
    q_proj: Linear,
    k_proj: Linear,
    v_proj: Linear,
    out_proj: Linear,
    n_heads: usize,
    head_dim: usize,
}

impl VisAttention {
    fn load(vb: &VarBuilder, prefix: &str, cfg: &SiglipVisionConfig) -> Result<Self> {
        let h = cfg.hidden_size;
        let head_dim = h / cfg.num_attention_heads;
        let lin = |name: &str| -> Result<Linear> {
            let w = vb.get((h, h), &format!("{prefix}.{name}.weight")).context(format!("missing {prefix}.{name}.weight"))?;
            let b = vb.get(h, &format!("{prefix}.{name}.bias")).context(format!("missing {prefix}.{name}.bias"))?;
            Ok(Linear::new(w, Some(b)))
        };
        Ok(Self {
            q_proj: lin("q_proj")?,
            k_proj: lin("k_proj")?,
            v_proj: lin("v_proj")?,
            out_proj: lin("out_proj")?,
            n_heads: cfg.num_attention_heads,
            head_dim,
        })
    }

    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        let (b, seq, _) = x.dims3()?;
        let q = self.q_proj.forward(x)?.reshape((b, seq, self.n_heads, self.head_dim))?.transpose(1, 2)?.contiguous()?;
        let k = self.k_proj.forward(x)?.reshape((b, seq, self.n_heads, self.head_dim))?.transpose(1, 2)?.contiguous()?;
        let v = self.v_proj.forward(x)?.reshape((b, seq, self.n_heads, self.head_dim))?.transpose(1, 2)?.contiguous()?;

        let scale = 1f64 / (self.head_dim as f64).sqrt();
        let scores = (q.matmul(&k.transpose(D::Minus2, D::Minus1)?.contiguous()?)? * scale)?;
        let probs = candle_nn::ops::softmax_last_dim(&scores)?;
        let out = probs.matmul(&v)?;
        let out = out.transpose(1, 2)?.reshape((b, seq, self.n_heads * self.head_dim))?;
        Ok(self.out_proj.forward(&out)?)
    }
}

struct VisMlp {
    fc1: Linear,
    fc2: Linear,
}

impl VisMlp {
    fn load(vb: &VarBuilder, prefix: &str, cfg: &SiglipVisionConfig) -> Result<Self> {
        let h = cfg.hidden_size;
        let i = cfg.intermediate_size;
        let fc1_w = vb.get((i, h), &format!("{prefix}.fc1.weight")).context("missing mlp.fc1.weight")?;
        let fc1_b = vb.get(i, &format!("{prefix}.fc1.bias")).context("missing mlp.fc1.bias")?;
        let fc2_w = vb.get((h, i), &format!("{prefix}.fc2.weight")).context("missing mlp.fc2.weight")?;
        let fc2_b = vb.get(h, &format!("{prefix}.fc2.bias")).context("missing mlp.fc2.bias")?;
        Ok(Self {
            fc1: Linear::new(fc1_w, Some(fc1_b)),
            fc2: Linear::new(fc2_w, Some(fc2_b)),
        })
    }

    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        let h = self.fc1.forward(x)?.gelu_erf()?;
        Ok(self.fc2.forward(&h)?)
    }
}

fn load_layer_norm(vb: &VarBuilder, prefix: &str, size: usize, eps: f64) -> Result<LayerNorm> {
    let w = vb.get(size, &format!("{prefix}.weight")).context(format!("missing {prefix}.weight"))?;
    let b = vb.get(size, &format!("{prefix}.bias")).context(format!("missing {prefix}.bias"))?;
    Ok(LayerNorm::new(w, b, eps))
}

struct VisLayer {
    ln1: LayerNorm,
    attn: VisAttention,
    ln2: LayerNorm,
    mlp: VisMlp,
}

impl VisLayer {
    fn load(vb: &VarBuilder, prefix: &str, cfg: &SiglipVisionConfig) -> Result<Self> {
        Ok(Self {
            ln1: load_layer_norm(vb, &format!("{prefix}.layer_norm1"), cfg.hidden_size, cfg.layer_norm_eps)?,
            attn: VisAttention::load(vb, &format!("{prefix}.self_attn"), cfg)?,
            ln2: load_layer_norm(vb, &format!("{prefix}.layer_norm2"), cfg.hidden_size, cfg.layer_norm_eps)?,
            mlp: VisMlp::load(vb, &format!("{prefix}.mlp"), cfg)?,
        })
    }

    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        let residual = x;
        let h = self.ln1.forward(x)?;
        let h = self.attn.forward(&h)?;
        let x = (residual + h)?;

        let residual = &x;
        let h = self.ln2.forward(&x)?;
        let h = self.mlp.forward(&h)?;
        Ok((residual + h)?)
    }
}

pub struct Siglip2VisionModel {
    patch_embed: Conv2d,
    position_embedding: Tensor, // [num_patches, hidden]
    layers: Vec<VisLayer>,
    post_layernorm: LayerNorm,
    cfg: SiglipVisionConfig,
}

impl Siglip2VisionModel {
    pub fn load(vb: VarBuilder, cfg: SiglipVisionConfig) -> Result<Self> {
        let conv_cfg = Conv2dConfig {
            stride: cfg.patch_size,
            padding: 0,
            dilation: 1,
            groups: 1,
            cudnn_fwd_algo: None,
        };
        let patch_w = vb.get(
            (cfg.hidden_size, cfg.num_channels, cfg.patch_size, cfg.patch_size),
            "vision_model.embeddings.patch_embedding.weight",
        ).context("missing patch_embedding.weight")?;
        let patch_b = vb.get(cfg.hidden_size, "vision_model.embeddings.patch_embedding.bias")
            .context("missing patch_embedding.bias")?;
        let patch_embed = Conv2d::new(patch_w, Some(patch_b), conv_cfg);

        let position_embedding = vb.get(
            (cfg.num_patches(), cfg.hidden_size),
            "vision_model.embeddings.position_embedding.weight",
        ).context("missing position_embedding.weight")?;

        let mut layers = Vec::with_capacity(cfg.num_hidden_layers);
        for i in 0..cfg.num_hidden_layers {
            let prefix = format!("vision_model.encoder.layers.{i}");
            layers.push(VisLayer::load(&vb, &prefix, &cfg)?);
        }

        let post_layernorm = load_layer_norm(&vb, "vision_model.post_layernorm", cfg.hidden_size, cfg.layer_norm_eps)?;

        Ok(Self { patch_embed, position_embedding, layers, post_layernorm, cfg })
    }

    /// pixel_values: [batch, channels, image_size, image_size], already normalized.
    /// Returns a pooled embedding: [batch, hidden] (mean pooled over patch tokens).
    pub fn forward(&self, pixel_values: &Tensor) -> Result<Tensor> {
        let patches = self.patch_embed.forward(pixel_values)?; // [b, hidden, gh, gw]
        let (b, hidden, gh, gw) = patches.dims4()?;
        let x = patches.reshape((b, hidden, gh * gw))?.transpose(1, 2)?; // [b, num_patches, hidden]
        let mut x = x.broadcast_add(&self.position_embedding.unsqueeze(0)?)?;

        for layer in &self.layers {
            x = layer.forward(&x)?;
        }
        let x = self.post_layernorm.forward(&x)?;

        // Mean pool over patch tokens. See module-level caveat re: official MAP head.
        let pooled = x.mean(1)?; // [b, hidden]
        Ok(pooled)
    }

    pub fn config(&self) -> &SiglipVisionConfig {
        &self.cfg
    }
}
