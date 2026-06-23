# apexkit-vector Guide

`apexkit-vector` turns text and images into fixed-length numeric vectors (embeddings) and
indexes them for fast nearest-neighbor search. This guide covers how to use it, how it's
put together internally, and *why* it's built the way it is - especially the parts that
are easy to get subtly wrong (pooling, padding, prompt prefixes, score direction).

---

## 1. What this crate actually does

Two jobs, glued together by `VectorEngine`:

1. **Embed** - turn HTML/text or a base64 image into a `Vec<f32>` vector, via `CandleEmbedder`.
2. **Index & search** - store vectors in an in-memory HNSW index per `(collection, field)`,
   and find nearest neighbors to a query vector, via `VectorIndex`.

```rust
let engine = VectorEngine::new(Some(EmbeddingModelConfig::bge_small_en_v1_5())).await?;

let vec = engine.embedder.embed("some document text")?;
engine.index.insert(collection_id, record_id, "content", &vec);

let query_vec = engine.embedder.embed_query("search query")?;
let matches = engine.index.search(collection_id, "content", &query_vec, 10);
// matches: Vec<(record_id, l2_distance)>, distance ascending = best match first
```

---

## 2. Text embedding: three backbone families

The crate auto-detects which transformer family you're using from the HF repo id
(case-insensitive substring match on `"gemma"` / `"qwen"`, else falls back to BERT-style),
and routes to a completely different forward pass and pooling strategy for each. This
matters because **using the wrong pooling strategy produces vectors that "look" valid
(right shape, right magnitude after normalization) but are semantically broken** - nothing
errors, your search just quietly performs badly.

| Family | Example models | Attention | Pooling | Prompt prefix |
|---|---|---|---|---|
| BERT | `all-MiniLM-L6-v2`, `bge-*`, `gte-small` | bidirectional | masked mean | none |
| Gemma | `embeddinggemma-300m` | bidirectional | masked mean | both sides, different text |
| Qwen | `Qwen3-Embedding-0.6B` | **causal** | **last real token** | query side only |

### Why pooling differs

- **BERT/Gemma (bidirectional):** every token's hidden state has seen the *whole* sequence
  (no causal mask), so averaging all of them is a reasonable summary. We only average over
  *real* tokens - padding tokens are masked out of both the attention computation and the
  final average (`masked_mean_pool`).
- **Qwen (causal):** each token can only see *itself and earlier* tokens. The first token
  has seen almost nothing; only the *last* token has seen the entire input. Mean-pooling a
  causal model dilutes the embedding with half-formed early-position states. So instead we
  take the hidden state at the last **non-padding** position per sequence
  (`last_token_pool`) - this requires knowing exactly where each sequence's real content
  ends in a right-padded batch, not just assuming the final tensor index.

### Why prompt prefixes differ

Both Gemma and Qwen embedding checkpoints were *trained* with specific wrapper text around
the input, not raw strings. Skipping this, or using the wrong side's prefix, doesn't error
- it just produces vectors that don't match how the model was actually trained to compare
queries against documents, which shows up as worse ranking with no error message to find.

- **EmbeddingGemma:** documents get wrapped as `"title: none | text: {text}"`; queries get
  `"task: search result | query: {text}"`.
- **Qwen3-Embedding:** documents are embedded raw (no wrapper); queries get
  `"Instruct: Given a query, retrieve relevant documents.\nQuery: {text}"`.

This is why the crate exposes **two methods, not one**:

```rust
embedder.embed(text)        // for documents you're indexing
embedder.embed_query(text)  // for the incoming search query at retrieval time
```

Always use `embed` when storing/indexing content, and `embed_query` when embedding what
the user typed into a search box. Using `embed` for both will still "work" (no crash) but
silently hurts ranking quality for Gemma/Qwen backbones.

### Long documents: windowing

If tokenized input exceeds `EmbeddingModelConfig.window_size`, the text is split into
overlapping windows (`window_size` tokens per window, `overlap` tokens shared between
consecutive windows), each window is embedded and pooled independently, and the resulting
window vectors are averaged together, then L2-normalized. The overlap exists so a concept
that straddles a window boundary still gets seen in full by at least one window.

### Batches and padding

`embed_batch(&[String])` tokenizes everything together with `PaddingStrategy::BatchLongest`
- every sequence in the batch is padded up to the length of the longest one. This is where
getting the attention mask wrong is most damaging: a 10-token sentence padded out to 200
tokens (because some other sentence in the batch is long) will have its real content
diluted across 190 pad positions unless the mask correctly excludes them from both
attention and pooling. This crate threads the real per-sequence attention mask through
`run_model_pass_with_mask` for exactly this reason.

---

## 3. Image embedding: ONNX by default, candle as a fallback

`embed_image(base64_str)` lazy-loads a vision model on first call and reuses it afterward.
Which model and runtime gets loaded is controlled entirely by environment variables - no
code changes needed to switch.

```bash
APEXKIT_VISION_MODEL=siglip2-onnx      # default if unset
APEXKIT_VISION_MODEL=tinyclip-onnx     # needs APEXKIT_VISION_MODEL_REPO/_FILE set
APEXKIT_VISION_MODEL=mobileclip-onnx   # needs APEXKIT_VISION_MODEL_REPO/_FILE set
APEXKIT_VISION_MODEL=candle-siglip2    # legacy in-process candle transformer

APEXKIT_VISION_MODEL_REPO=...     # override the HF repo id for the selected preset
APEXKIT_VISION_MODEL_FILE=...     # override the .onnx filename inside that repo
APEXKIT_VISION_INPUT_NAME=...     # override the ONNX graph's input tensor name
```

### Why ONNX is the default now

The original/legacy path (`candle-siglip2`) loads full F32 weights and runs a hand-written
transformer forward pass entirely in this crate. That's straightforward to reason about,
but heavy: full-precision weights plus an unoptimized custom implementation don't fit
comfortably in constrained memory (the ask that motivated this change was running under
512MB RAM). Routing through **ONNX Runtime** instead lets you load **quantized** (int8/int4)
`.onnx` exports and get an optimized inference engine for free, instead of reimplementing
quantized inference by hand.

The default preset is `onnx-community/siglip2-base-patch16-384-ONNX`
(`onnx/model_quantized.onnx`) - a SigLIP2 model with a confirmed public quantized ONNX
export, run via the `ort` crate.

### TinyCLIP / MobileCLIP: bring your own export

`tinyclip-onnx` and `mobileclip-onnx` are wired up as presets (correct preprocessing -
image size, channel mean/std - is already set per model), but **the default repo strings
are placeholders, not verified-to-exist public ONNX files.** You will likely need to:

1. Export the model yourself (e.g. `optimum-cli export onnx`), optionally quantize it
   (`optimum-cli export onnx --quantize` or `onnxruntime.quantization`).
2. Host the `.onnx` file somewhere reachable (your own HF repo, or any path
   `hf_hub`/local file access can resolve).
3. Point `APEXKIT_VISION_MODEL_REPO` and `APEXKIT_VISION_MODEL_FILE` at it.

If you find a confirmed public quantized ONNX export for either model, you can update the
default repo string in `models/onnx_vision.rs::OnnxVisionConfig::tinyclip_vit_4m()` /
`::mobileclip_s0()` and it becomes a true zero-config default.

### Output shape handling

Different ONNX exports disagree about whether they hand you an already-pooled embedding
(`[1, hidden]`) or a raw per-patch sequence (`[1, seq, hidden]`) and expect you to pool it
yourself. `OnnxVisionEmbedder::embed` handles both: 2D output is used directly, 3D output
is mean-pooled over the sequence dimension. It also logs the actual output dims at
`tracing::info!` level on every call - check your logs once after wiring up a new model to
confirm it's taking the path you expect.

### Preprocessing

Images are decoded, resized to the model's expected square input size with triangle
filtering, converted to CHW `f32`, and normalized per-channel using the active config's
`mean`/`std` (SigLIP-family models use `[0.5,0.5,0.5]`/`[0.5,0.5,0.5]`; CLIP-family models
typically use ImageNet stats - already set correctly per preset).

---

## 4. Indexing and search: distance, not similarity

`VectorIndex` wraps `hnsw_rs::Hnsw<f32, DistL2>` - **squared L2 distance**, where smaller
means more similar and `0.0` means identical. This is the opposite of "similarity score"
intuition (where bigger is usually better), and got this crate's search results inverted
once already (sorting descending on a distance value puts the *worst* match first). Two
things to remember if you touch this code:

- **Sort ascending** on whatever `search()` returns - lowest distance first.
- If aggregating across multiple vectorized fields for one record (e.g. searching both a
  `title` and a `content` field), keep the **minimum** distance per record across fields,
  not the maximum - you want the record's *best* match, not its worst.

If you want similarity in the `[-1, 1]`/`[0,1]`-style sense instead of raw distance for
display purposes: every embedding produced by this crate is L2-normalized, so for unit
vectors, `cosine_similarity = 1 - (squared_l2_distance / 2)`. Convert at the point you build
any user-facing `_score` field, not inside the index itself, so the index stays a neutral
distance store.

```rust
let cosine_sim = 1.0 - (l2_distance / 2.0); // valid only because vectors are unit-normalized
```

---

## 5. Picking a text model

```rust
EmbeddingModelConfig::default()              // all-MiniLM-L6-v2, small/fast/general
EmbeddingModelConfig::bge_small_en_v1_5()    // BAAI BGE small, English, strong general retrieval
EmbeddingModelConfig::bge_base_en_v1_5()     // BGE base, bigger/slower/better
EmbeddingModelConfig::gte_small()            // GTE small, alternative general-purpose
EmbeddingModelConfig::gemma_300m()           // EmbeddingGemma, bidirectional, 2048-token window
EmbeddingModelConfig::qwen3_embedding_0_6b() // Qwen3-Embedding, causal, 8192-token window
EmbeddingModelConfig::custom(...)            // anything else on the HF Hub with a config.json,
                                              // tokenizer.json, and *.safetensors
```

For custom BERT-family models, just point `custom()` at the repo. For custom Gemma/Qwen
checkpoints, the repo id needs to contain `"gemma"` or `"qwen"` (case-insensitive) for
auto-detection to route correctly - or you can fork the `is_gemma`/`is_qwen` detection in
`embedder.rs::CandleEmbedder::new` if you need an exact-match override instead of a
substring check.

---

## 6. Known caveats worth knowing before you ship this

- **EmbeddingGemma's official pooling head** includes learned dense projection layers after
  mean-pooling in the reference implementation; this crate does masked mean-pooling only,
  without that extra projection. Close, not necessarily bit-identical to the reference.
- **SigLIP/SigLIP2's official pooling** uses a learned attention-pooling ("MAP") head, not a
  plain mean. The candle SigLIP2 path (`candle-siglip2`) here uses plain mean pooling as a
  practical approximation.
- **TinyCLIP/MobileCLIP ONNX presets** need a verified export - see Section 3.
- This crate's Gemma/Qwen implementations are hand-written directly against candle
  primitives (not `candle_transformers::models::gemma`/`qwen`), because the embedding
  variants need bidirectional attention (Gemma) or last-token pooling (Qwen) that differ
  from the causal-generation-oriented implementations in `candle_transformers`. Exact
  safetensors tensor key names are assumed to follow standard HF naming conventions; if a
  checkpoint deviates, loading will fail with a clear "missing tensor" error, and
  `dump_tensor_names()` (in `models/gemma_embed.rs`, reused elsewhere) will print every key
  actually present in the checkpoint so you can diff and adjust.
- No automated tests ship with this guide's code changes. Before trusting any of this in
  production, run a basic sanity check: embed two semantically similar texts/images and two
  unrelated ones, and confirm distances separate them the way you'd expect.