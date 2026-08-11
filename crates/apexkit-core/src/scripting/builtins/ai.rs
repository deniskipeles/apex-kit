// =========================== apex-kit/crates/apexkit-core/src/scripting/builtins/ai.rs start here ===========================
use std::sync::Arc;
use rquickjs::prelude::Async;
use rquickjs::{Ctx, Function, Object, Value};
use rquickjs_serde::from_value;
use super::super::context::ScriptContext;

pub fn register_ai<'js>(ctx: &Ctx<'js>, app_ctx: Arc<dyn ScriptContext>) -> Result<(), String> {
    let globals = ctx.globals();
    let ai_obj = Object::new(ctx.clone()).map_err(|e| e.to_string())?;

    // 1. $ai.embed(text) -> Promise<Vec<f32>>
    let app_embed = app_ctx.clone();
    let embed_fn = Function::new(ctx.clone(), Async(move |text: String| {
    let app = app_embed.clone();
    async move {
        app.check_quota("ai").await.map_err(|_| rquickjs::Error::Exception)?;
        let provider = app.get_scoped_vector_provider().await;
        let vec = provider.embed(&text).await.map_err(|_| rquickjs::Error::Exception)?;
        
        Ok::<Vec<f32>, rquickjs::Error>(vec) // <-- Type explicitly at the end!
    }
}))
    .map_err(|e| e.to_string())?;

    // 2. $ai.meanVector([v1, v2, ...]) -> Vec<f32>
    let mean_vector_fn = Function::new(
        ctx.clone(),
        move |val: Value<'js>| -> rquickjs::Result<Vec<f32>> {
            let vectors: Vec<Vec<f32>> =
                from_value(val).map_err(|_| rquickjs::Error::Exception)?;

            if vectors.is_empty() {
                return Err(rquickjs::Error::Exception);
            }

            let dim = vectors[0].len();
            if dim == 0 {
                return Err(rquickjs::Error::Exception);
            }

            for v in &vectors {
                if v.len() != dim {
                    return Err(rquickjs::Error::Exception);
                }
            }

            // Sum
            let mut summed = vec![0.0_f32; dim];
            for v in &vectors {
                for (i, &val) in v.iter().enumerate() {
                    summed[i] += val;
                }
            }

            // Mean
            let count = vectors.len() as f32;
            for val in &mut summed {
                *val /= count;
            }

            // L2 Normalize
            let sum_sq: f32 = summed.iter().map(|x| x * x).sum();
            let mag = sum_sq.sqrt() + 1e-12;
            for val in &mut summed {
                *val /= mag;
            }

            Ok(summed)
        },
    )
    .map_err(|e| e.to_string())?;

    // 3. $ai.cosineSimilarity(v1, v2) -> f64
    let cosine_sim_fn = Function::new(
        ctx.clone(),
        move |v1_val: Value<'js>, v2_val: Value<'js>| -> rquickjs::Result<f64> {
            let v1: Vec<f32> = from_value(v1_val).map_err(|_| rquickjs::Error::Exception)?;
            let v2: Vec<f32> = from_value(v2_val).map_err(|_| rquickjs::Error::Exception)?;

            if v1.len() != v2.len() || v1.is_empty() {
                return Err(rquickjs::Error::Exception);
            }

            let mut dot_product = 0.0;
            let mut norm_a = 0.0;
            let mut norm_b = 0.0;

            for i in 0..v1.len() {
                dot_product += v1[i] * v2[i];
                norm_a += v1[i] * v1[i];
                norm_b += v2[i] * v2[i];
            }

            let denominator = norm_a.sqrt() * norm_b.sqrt();
            let similarity = if denominator == 0.0 {
                0.0
            } else {
                dot_product / denominator
            };

            Ok(similarity as f64)
        },
    )
    .map_err(|e| e.to_string())?;

    ai_obj.set("embed", embed_fn).map_err(|e| e.to_string())?;
    ai_obj.set("meanVector", mean_vector_fn).map_err(|e| e.to_string())?;
    ai_obj.set("cosineSimilarity", cosine_sim_fn).map_err(|e| e.to_string())?;

    globals.set("$ai", ai_obj).map_err(|e| e.to_string())?;
    Ok(())
}
// =========================== apex-kit/crates/apexkit-core/src/scripting/builtins/ai.rs ends here ===========================