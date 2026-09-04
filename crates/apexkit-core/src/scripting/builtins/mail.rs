use super::super::context::ScriptContext;
use super::db::resolve_db;
use rquickjs::function::Async;
use rquickjs::{Ctx, Exception, Function, Object};
use std::sync::Arc;

pub fn register_mail<'js>(ctx: &Ctx<'js>, app_ctx: Arc<dyn ScriptContext>) -> Result<(), String> {
    let globals = ctx.globals();
    let mail_obj = Object::new(ctx.clone()).map_err(|e| e.to_string())?;

    let app_send = app_ctx.clone();
    let send_fn = Function::new(
        ctx.clone(),
        Async(
            move |js_ctx: Ctx<'js>, to: String, subj: String, body: String| {
                let app = app_send.clone();
                async move {
                    let db = match resolve_db(None, app.clone()).await {
                        Ok(d) => d,
                        Err(e) => {
                            let err = Exception::from_message(js_ctx.clone(), &e).unwrap();
                            return Err(js_ctx.throw(err.into()));
                        }
                    };

                    match crate::workers::tasks::emails::send_email(
                        db,
                        app.get_vault(),
                        &to,
                        &subj,
                        &body,
                    )
                    .await
                    {
                        Ok(_) => Ok::<bool, rquickjs::Error>(true),
                        Err(e) => {
                            let err = Exception::from_message(
                                js_ctx.clone(),
                                &format!("Failed to send email: {}", e),
                            )
                            .unwrap();
                            Err(js_ctx.throw(err.into()))
                        }
                    }
                }
            },
        ),
    )
    .map_err(|e| e.to_string())?;

    mail_obj.set("send", send_fn).map_err(|e| e.to_string())?;
    globals.set("$mail", mail_obj).map_err(|e| e.to_string())?;
    Ok(())
}
