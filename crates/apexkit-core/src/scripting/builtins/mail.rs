use super::super::{context::ACTIVE_CONTEXT, return_json_promise};
use boa_engine::{
    Context, JsArgs, JsString, NativeFunction, object::ObjectInitializer, property::Attribute,
};

pub fn register_mail(ctx: &mut Context) -> Result<(), String> {
    let send = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let to = args
            .get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();
        let subj = args
            .get_or_undefined(1)
            .to_string(ctx)?
            .to_std_string_escaped();
        let body = args
            .get_or_undefined(2)
            .to_string(ctx)?
            .to_std_string_escaped();

        let res = ACTIVE_CONTEXT.with(|c| {
            if let Some((app, handle, _, _, _)) = &*c.borrow() {
                handle.block_on(async {
                    let db = super::db::resolve_db(None, app.clone()).await?;
                    crate::workers::tasks::emails::send_email(
                        db,
                        app.get_vault(),
                        &to,
                        &subj,
                        &body,
                    )
                    .await
                })
            } else {
                Err("Context lost".into())
            }
        });
        return_json_promise(ctx, res.map(|_| serde_json::Value::Bool(true)))
    });
    let obj = ObjectInitializer::new(ctx)
        .function(send, JsString::from("send"), 3)
        .build();
    ctx.register_global_property(JsString::from("$mail"), obj, Attribute::all())
        .map_err(|e| e.to_string())
}
