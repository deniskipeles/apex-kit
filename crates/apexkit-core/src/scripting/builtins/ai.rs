use super::super::{context::ACTIVE_CONTEXT, return_json_promise};
use boa_engine::{
    Context, JsArgs, JsString, NativeFunction, object::ObjectInitializer, property::Attribute,
};

pub fn register_ai(ctx: &mut Context) -> Result<(), String> {
    let embed = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let text = args
            .get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();
        let res = ACTIVE_CONTEXT.with(|c| {
            if let Some((app, handle, _, _, _)) = &*c.borrow() {
                handle.block_on(async { app.get_vector_provider().embed(&text).await })
            } else {
                Err("Context lost".into())
            }
        });
        return_json_promise(ctx, res.map(|v| serde_json::to_value(v).unwrap()))
    });
    let obj = ObjectInitializer::new(ctx)
        .function(embed, JsString::from("embed"), 1)
        .build();
    ctx.register_global_property(JsString::from("$ai"), obj, Attribute::all())
        .map_err(|e| e.to_string())
}
