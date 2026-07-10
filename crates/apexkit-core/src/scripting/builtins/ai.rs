use super::super::{context::ACTIVE_CONTEXT, return_json_promise};
use boa_engine::{
    Context, JsArgs, JsNativeError, JsString, JsValue, NativeFunction, object::ObjectInitializer,
    property::Attribute,
};

pub fn register_ai(ctx: &mut Context) -> Result<(), String> {
    // 1. $ai.embed(text)
    let embed = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let text = args
            .get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();

        let res = ACTIVE_CONTEXT.with(|c| {
            if let Some((app, handle, _, _, _)) = &*c.borrow() {
                handle.block_on(async {
                    let provider = app.get_scoped_vector_provider().await;
                    provider.embed(&text).await
                })
            } else {
                Err("Context lost".into())
            }
        });

        return_json_promise(ctx, res.map(|v| serde_json::to_value(v).unwrap()))
    });

    // 2. $ai.meanVector([v1, v2, ...]) -> averages and L2 normalizes a list of vectors
    let mean_vector = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let val = args.get_or_undefined(0);
        let json_val = val.to_json(ctx)?.unwrap_or(serde_json::Value::Null);

        let vectors: Vec<Vec<f32>> = serde_json::from_value(json_val).map_err(|e| {
            JsNativeError::typ().with_message(format!("Expected an array of float arrays: {e}"))
        })?;

        if vectors.is_empty() {
            return Err(JsNativeError::range()
                .with_message("Cannot average an empty array of vectors")
                .into());
        }

        let dim = vectors[0].len();
        if dim == 0 {
            return Err(JsNativeError::range()
                .with_message("Vector dimension cannot be 0")
                .into());
        }

        for (i, v) in vectors.iter().enumerate() {
            if v.len() != dim {
                return Err(JsNativeError::range()
                    .with_message(format!(
                        "Vector dimension mismatch at index {}. Expected {}, got {}",
                        i,
                        dim,
                        v.len()
                    ))
                    .into());
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

        // L2 Normalize (so it can be safely used in vector search)
        let sum_sq: f32 = summed.iter().map(|x| x * x).sum();
        let mag = sum_sq.sqrt() + 1e-12; // epsilon to prevent div by zero
        for val in &mut summed {
            *val /= mag;
        }

        let js_arr = serde_json::to_value(summed).unwrap();
        Ok(JsValue::from_json(&js_arr, ctx).unwrap())
    });

    // 3. $ai.cosineSimilarity(v1, v2) -> computes similarity score between -1 and 1
    let cosine_sim = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let v1_val = args
            .get_or_undefined(0)
            .to_json(ctx)?
            .unwrap_or(serde_json::Value::Null);
        let v2_val = args
            .get_or_undefined(1)
            .to_json(ctx)?
            .unwrap_or(serde_json::Value::Null);

        let v1: Vec<f32> = serde_json::from_value(v1_val).map_err(|e| {
            JsNativeError::typ()
                .with_message(format!("Argument 1 must be an array of numbers: {e}"))
        })?;
        let v2: Vec<f32> = serde_json::from_value(v2_val).map_err(|e| {
            JsNativeError::typ()
                .with_message(format!("Argument 2 must be an array of numbers: {e}"))
        })?;

        if v1.len() != v2.len() || v1.is_empty() {
            return Err(JsNativeError::range()
                .with_message("Vectors must have the same non-zero dimensions")
                .into());
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

        Ok(JsValue::from(similarity as f64))
    });

    let obj = ObjectInitializer::new(ctx)
        .function(embed, JsString::from("embed"), 1)
        .function(mean_vector, JsString::from("meanVector"), 1)
        .function(cosine_sim, JsString::from("cosineSimilarity"), 2)
        .build();

    ctx.register_global_property(JsString::from("$ai"), obj, Attribute::all())
        .map_err(|e| e.to_string())
}
