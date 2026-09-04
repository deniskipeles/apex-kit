use super::super::context::ScriptContext;
use rquickjs::prelude::Async;
use rquickjs::{Ctx, Exception, Function, Object, Value};
use rquickjs_serde::from_value;
use std::sync::Arc;

fn throw_err<'js, T>(ctx: &Ctx<'js>, msg: &str) -> rquickjs::Result<T> {
    let err = Exception::from_message(ctx.clone(), msg).unwrap();
    Err(ctx.throw(err.into()))
}

pub fn register_ai<'js>(ctx: &Ctx<'js>, app_ctx: Arc<dyn ScriptContext>) -> Result<(), String> {
    let globals = ctx.globals();
    let ai_obj = Object::new(ctx.clone()).map_err(|e| e.to_string())?;

    // $ai.embed(text)
    let app_embed = app_ctx.clone();
    let embed_fn = Function::new(
        ctx.clone(),
        Async(move |js_ctx: Ctx<'js>, text: String| {
            let app = app_embed.clone();
            async move {
                if let Err(e) = app.check_quota("ai").await {
                    return throw_err(&js_ctx, &format!("AI quota exceeded: {}", e));
                }
                let provider = app.get_scoped_vector_provider().await;
                match provider.embed(&text).await {
                    Ok(vec) => Ok::<Vec<f32>, rquickjs::Error>(vec),
                    Err(e) => throw_err(&js_ctx, &format!("Embedding generation failed: {}", e)),
                }
            }
        }),
    )
    .map_err(|e| e.to_string())?;

    // $ai.meanVector(vectors)
    let mean_vector_fn = Function::new(
        ctx.clone(),
        move |js_ctx: Ctx<'js>, val: Value<'js>| -> rquickjs::Result<Vec<f32>> {
            let vectors: Vec<Vec<f32>> = match from_value(val) {
                Ok(v) => v,
                Err(_) => return throw_err(&js_ctx, "Expected an array of float vectors"),
            };

            if vectors.is_empty() {
                return throw_err(&js_ctx, "Vectors array cannot be empty");
            }

            let dim = vectors[0].len();
            if dim == 0 {
                return throw_err(&js_ctx, "Vector dimension must be greater than 0");
            }

            for (idx, v) in vectors.iter().enumerate() {
                if v.len() != dim {
                    return throw_err(
                        &js_ctx,
                        &format!(
                            "Vector dimension mismatch at index {}: expected {}, got {}",
                            idx,
                            dim,
                            v.len()
                        ),
                    );
                }
            }

            let mut summed = vec![0.0_f32; dim];
            for v in &vectors {
                for (i, &val) in v.iter().enumerate() {
                    summed[i] += val;
                }
            }

            let count = vectors.len() as f32;
            for val in &mut summed {
                *val /= count;
            }

            let sum_sq: f32 = summed.iter().map(|x| x * x).sum();
            let mag = sum_sq.sqrt() + 1e-12;
            for val in &mut summed {
                *val /= mag;
            }

            Ok(summed)
        },
    )
    .map_err(|e| e.to_string())?;

    // $ai.cosineSimilarity(v1, v2)
    let cosine_sim_fn = Function::new(
        ctx.clone(),
        move |js_ctx: Ctx<'js>, v1_val: Value<'js>, v2_val: Value<'js>| -> rquickjs::Result<f64> {
            let v1: Vec<f32> = match from_value(v1_val) {
                Ok(v) => v,
                Err(_) => return throw_err(&js_ctx, "Invalid first vector argument"),
            };
            let v2: Vec<f32> = match from_value(v2_val) {
                Ok(v) => v,
                Err(_) => return throw_err(&js_ctx, "Invalid second vector argument"),
            };

            if v1.len() != v2.len() || v1.is_empty() {
                return throw_err(
                    &js_ctx,
                    &format!(
                        "Dimension mismatch: vector 1 has {} items, vector 2 has {}",
                        v1.len(),
                        v2.len()
                    ),
                );
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
    ai_obj
        .set("meanVector", mean_vector_fn)
        .map_err(|e| e.to_string())?;
    ai_obj
        .set("cosineSimilarity", cosine_sim_fn)
        .map_err(|e| e.to_string())?;

    globals.set("$ai", ai_obj).map_err(|e| e.to_string())?;
    Ok(())
}
