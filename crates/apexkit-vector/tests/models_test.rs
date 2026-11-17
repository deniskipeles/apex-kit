//! Module-level tests for the vector model layer: model-selection configs,
//! model-identity helpers, and the pure tensor pooling helpers.
//!
//! These tests do NOT download or execute any model. They validate the
//! deterministic configuration surface (repo ids, window/overlap values,
//! pooling math) that silently controls embedding quality elsewhere.

use apexkit_vector::{
    EmbeddingModelConfig, get_current_text_model, get_current_vision_model, models::gemma_embed,
    models::qwen_embed,
};
use candle_core::{DType, Device, Tensor};

// ---------------------------------------------------------------------------
// EmbeddingModelConfig presets
// ---------------------------------------------------------------------------

#[test]
fn default_config_uses_minilm() {
    let cfg = EmbeddingModelConfig::default();
    assert_eq!(cfg.repo_id, "sentence-transformers/all-MiniLM-L6-v2");
    assert_eq!(cfg.revision, "main");
    assert_eq!(cfg.config_file, "config.json");
    assert_eq!(cfg.tokenizer_file, "tokenizer.json");
    assert_eq!(cfg.weights_file, "model.safetensors");
    assert_eq!(cfg.window_size, 512);
    assert_eq!(cfg.overlap, 128);
}

#[test]
fn named_presets_are_distinct_and_ordered() {
    let bge_small = EmbeddingModelConfig::bge_small_en_v1_5();
    let bge_base = EmbeddingModelConfig::bge_base_en_v1_5();
    let gte = EmbeddingModelConfig::gte_small();
    let gemma = EmbeddingModelConfig::gemma_300m();
    let qwen = EmbeddingModelConfig::qwen3_embedding_0_6b();

    assert_eq!(bge_small.repo_id, "BAAI/bge-small-en-v1.5");
    assert_eq!(bge_base.repo_id, "BAAI/bge-base-en-v1.5");
    assert_eq!(gte.repo_id, "thenlper/gte-small");
    assert_eq!(gemma.repo_id, "google/embeddinggemma-300m");
    assert_eq!(qwen.repo_id, "Qwen/Qwen3-Embedding-0.6B");

    // Larger context windows for the bigger models.
    assert!(gemma.window_size > bge_small.window_size);
    assert!(qwen.window_size > gemma.window_size);

    // Overlap must always be smaller than the window (stride stays positive).
    for cfg in [&bge_small, &bge_base, &gte, &gemma, &qwen] {
        assert!(cfg.overlap < cfg.window_size, "overlap must be < window");
    }
}

#[test]
fn onnx_presets_point_at_onnx_community_repos() {
    let gemma = EmbeddingModelConfig::gemma_300m_onnx();
    let qwen = EmbeddingModelConfig::qwen3_embedding_0_6b_onnx();
    assert_eq!(gemma.repo_id, "onnx-community/embeddinggemma-300m-ONNX");
    assert_eq!(qwen.repo_id, "onnx-community/Qwen3-Embedding-0.6B-ONNX");
}

#[test]
fn custom_config_round_trips_all_fields() {
    let cfg = EmbeddingModelConfig::custom(
        "my/repo".to_string(),
        "rev1".to_string(),
        "cfg.json".to_string(),
        "tok.json".to_string(),
        "w.safetensors".to_string(),
        1024,
        64,
    );
    assert_eq!(cfg.repo_id, "my/repo");
    assert_eq!(cfg.revision, "rev1");
    assert_eq!(cfg.config_file, "cfg.json");
    assert_eq!(cfg.tokenizer_file, "tok.json");
    assert_eq!(cfg.weights_file, "w.safetensors");
    assert_eq!(cfg.window_size, 1024);
    assert_eq!(cfg.overlap, 64);
}

// ---------------------------------------------------------------------------
// Model-identity helpers (pure env reads)
// ---------------------------------------------------------------------------

#[test]
fn text_model_identity_defaults_to_minilm() {
    // SAFETY: single-threaded test process; no other thread reads this env here.
    unsafe { std::env::remove_var("APEXKIT_VECTOR_TEXT_MODEL") };
    let id = get_current_text_model();
    assert_eq!(id, "sentence-transformers/all-MiniLM-L6-v2@main");
}

#[test]
fn text_model_identity_honours_env_selection() {
    unsafe { std::env::set_var("APEXKIT_VECTOR_TEXT_MODEL", "bge-small") };
    assert_eq!(get_current_text_model(), "BAAI/bge-small-en-v1.5@main");

    unsafe { std::env::set_var("APEXKIT_VECTOR_TEXT_MODEL", "gemma-300m") };
    assert_eq!(get_current_text_model(), "google/embeddinggemma-300m@main");

    unsafe { std::env::set_var("APEXKIT_VECTOR_TEXT_MODEL", "qwen3-embedding-onnx") };
    assert_eq!(
        get_current_text_model(),
        "onnx-community/Qwen3-Embedding-0.6B-ONNX@main"
    );

    unsafe { std::env::remove_var("APEXKIT_VECTOR_TEXT_MODEL") };
}

#[test]
fn text_model_identity_custom_reads_custom_env() {
    unsafe { std::env::set_var("APEXKIT_VECTOR_TEXT_MODEL", "custom") };
    unsafe { std::env::set_var("APEXKIT_VECTOR_CUSTOM_REPO", "acme/embed") };
    unsafe { std::env::set_var("APEXKIT_VECTOR_CUSTOM_REV", "v2") };
    unsafe { std::env::set_var("APEXKIT_VECTOR_CUSTOM_WINDOW", "2048") };

    assert_eq!(get_current_text_model(), "acme/embed@v2");

    unsafe { std::env::remove_var("APEXKIT_VECTOR_TEXT_MODEL") };
    unsafe { std::env::remove_var("APEXKIT_VECTOR_CUSTOM_REPO") };
    unsafe { std::env::remove_var("APEXKIT_VECTOR_CUSTOM_REV") };
    unsafe { std::env::remove_var("APEXKIT_VECTOR_CUSTOM_WINDOW") };
}

#[test]
fn vision_model_identity_has_three_segments() {
    unsafe { std::env::remove_var("APEXKIT_VISION_MODEL") };
    let id = get_current_vision_model();
    let parts: Vec<&str> = id.split(':').collect();
    assert_eq!(
        parts.len(),
        3,
        "vision identity must be family:repo:file, got {id}"
    );
    assert!(!parts[0].is_empty());
    assert!(!parts[1].is_empty());
    assert!(!parts[2].is_empty());
}

#[test]
fn vision_model_identity_candle_siglip2_legacy() {
    unsafe { std::env::set_var("APEXKIT_VISION_MODEL", "candle-siglip2") };
    let id = get_current_vision_model();
    assert!(id.starts_with("candle-siglip2:"), "got {id}");
    unsafe { std::env::remove_var("APEXKIT_VISION_MODEL") };
}

// ---------------------------------------------------------------------------
// Pooling helpers (pure tensor math)
// ---------------------------------------------------------------------------

#[test]
fn masked_mean_pool_averages_only_unmasked_tokens() {
    let dev = Device::Cpu;
    // batch=1, seq=3, hidden=2. Token 2 is padding.
    let hidden = Tensor::new(&[[[1.0f32, 1.0], [3.0, 3.0], [100.0, 100.0]]], &dev).unwrap();
    let mask = Tensor::new(&[[1u32, 1, 0]], &dev).unwrap();

    let pooled = gemma_embed::masked_mean_pool(&hidden, &mask).unwrap();
    let out = pooled.flatten_all().unwrap().to_vec1::<f32>().unwrap();

    // Mean of the two real tokens: (1+3)/2 = 2 for both dims.
    assert!((out[0] - 2.0).abs() < 1e-5, "got {out:?}");
    assert!((out[1] - 2.0).abs() < 1e-5, "got {out:?}");
}

#[test]
fn masked_mean_pool_is_stable_when_all_masked() {
    let dev = Device::Cpu;
    let hidden = Tensor::new(&[[[5.0f32, 5.0]]], &dev).unwrap();
    let mask = Tensor::new(&[[0u32]], &dev).unwrap();

    // Division guard (counts + 1e-9) must prevent NaN/Inf.
    let pooled = gemma_embed::masked_mean_pool(&hidden, &mask).unwrap();
    let out = pooled.flatten_all().unwrap().to_vec1::<f32>().unwrap();
    assert!(out[0].is_finite(), "all-masked pooling must stay finite");
}

#[test]
fn last_token_pool_picks_last_real_token_per_row() {
    let dev = Device::Cpu;
    // batch=2, seq=3, hidden=1.
    let hidden = Tensor::new(&[[[1.0f32], [2.0], [3.0]], [[4.0], [5.0], [6.0]]], &dev).unwrap();
    // Row 0: last real token is index 1. Row 1: last real token is index 2.
    let mask = vec![vec![1u32, 1, 0], vec![1, 1, 1]];

    let pooled = qwen_embed::last_token_pool(&hidden, &mask).unwrap();
    let out = pooled.flatten_all().unwrap().to_vec1::<f32>().unwrap();
    assert_eq!(out, vec![2.0, 6.0]);
}

#[test]
fn last_token_pool_falls_back_to_first_token_when_mask_empty() {
    let dev = Device::Cpu;
    let hidden = Tensor::new(&[[[9.0f32], [8.0]]], &dev).unwrap();
    let mask = vec![vec![0u32, 0]];

    let pooled = qwen_embed::last_token_pool(&hidden, &mask).unwrap();
    let out = pooled.flatten_all().unwrap().to_vec1::<f32>().unwrap();
    assert_eq!(out, vec![9.0]);
}

#[test]
fn dump_tensor_names_does_not_panic_on_empty_map() {
    let tensors = std::collections::HashMap::new();
    gemma_embed::dump_tensor_names(&tensors);
}

#[test]
fn candle_tensor_roundtrip_dtype() {
    // Guards the dev-dependency wiring: the test crate's candle version must be
    // able to build tensors the model helpers accept.
    let dev = Device::Cpu;
    let t = Tensor::zeros((2, 2), DType::F32, &dev).unwrap();
    assert_eq!(t.dims(), &[2, 2]);
}
