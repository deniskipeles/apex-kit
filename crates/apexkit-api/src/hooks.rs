use crate::AppError;
use crate::AppState;
use crate::utils::resolve_db_from_scope;
use apexkit_core::models::Collection;
use apexkit_core::realtime::EventScope;
use apexkit_core::{auth::Claims, validation::ValidationError};
use serde_json::{Value, json};
use std::sync::Arc;

// --- HELPER: Void Hooks (Notify/Block) ---
pub async fn trigger_void_hook(
    state: &AppState,
    trigger: &str,
    data: Value,
    auth: Option<&Claims>,
    scope: Option<&EventScope>,
    base_url: Option<String>,
) -> Result<(), AppError> {
    let scripts = state
        .db
        .get_scripts_by_trigger(trigger)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    if scripts.is_empty() {
        return Ok(());
    }

    let ctx = json!({
        "trigger": trigger,
        "data": data,
        "auth": auth.map(|c| json!({ "id": c.uid, "email": c.sub, "role": c.role })),
        "timestamp": chrono::Utc::now().to_rfc3339()
    });

    let context = Arc::new(crate::ScopedScriptContext {
        state: state.clone(),
        scope: scope.cloned().unwrap_or(EventScope::Root),
    });

    for script in scripts {
        state
            .script_engine
            .run_hook(
                &script.code,
                ctx.clone(),
                context.clone(),
                base_url.clone(),
                scope.cloned(),
            )
            .await
            .map_err(|e| {
                AppError::Validation(vec![ValidationError::ConstraintViolation(
                    trigger.into(),
                    e,
                )])
            })?;
    }
    Ok(())
}

// --- HELPER: Filter Hooks (Modify Data) ---
pub async fn trigger_filter_hook(
    state: &AppState,
    trigger: &str,
    data: Value,
    auth: Option<&Claims>,
    scope: Option<&EventScope>,
    base_url: Option<String>,
) -> Result<Value, AppError> {
    let scripts = state
        .db
        .get_scripts_by_trigger(trigger)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    if scripts.is_empty() {
        return Ok(data);
    }

    let mut current_data = data;

    let context = Arc::new(crate::ScopedScriptContext {
        state: state.clone(),
        scope: scope.cloned().unwrap_or(EventScope::Root),
    });

    for script in scripts {
        let ctx = json!({
            "trigger": trigger,
            "data": current_data,
            "auth": auth.map(|c| json!({ "id": c.uid, "email": c.sub, "role": c.role }))
        });

        if let Some(res) = state
            .script_engine
            .run_hook(
                &script.code,
                ctx,
                context.clone(),
                base_url.clone(),
                scope.cloned(),
            )
            .await
            .map_err(|e| {
                AppError::Validation(vec![ValidationError::ConstraintViolation(
                    trigger.into(),
                    e,
                )])
            })?
        {
            current_data = res;
        }
    }
    Ok(current_data)
}

// --- HELPER: Record Hooks (Existing) ---
pub async fn trigger_hooks(
    state: &AppState,
    // [REVERTED] No 'db' parameter here. We resolve it from scope.
    trigger: &str,
    collection: &Collection,
    record_id: Option<i64>,
    data: &serde_json::Value,
    auth: Option<&Claims>,
    base_url: Option<String>,
    scope: Option<&EventScope>,
) -> Result<Option<serde_json::Value>, AppError> {
    let actual_scope = scope.cloned().unwrap_or(EventScope::Root);

    // 1. Resolve DB locally just to fetch the scripts configuration
    let db = resolve_db_from_scope(state, &actual_scope).await?;

    let scripts = db
        .get_scripts_by_trigger(trigger)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    if scripts.is_empty() {
        return Ok(None);
    }

    let mut current_data = data.clone();
    let mut modified = false;

    // 2. Create Context (Lightweight, no DB instance attached)
    // The ScriptEngine will use `resolve_tenant_db` via the trait to get the DB when needed.
    let context = Arc::new(crate::ScopedScriptContext {
        state: state.clone(),
        scope: actual_scope.clone(),
    });

    for script in scripts {
        // Target Collection Filtering
        if let Some(target) = &script.target_collection
            && target != &collection.name
        {
            continue;
        }

        let event_context = serde_json::json!({
            "record": { "id": record_id, "data": current_data },
            "collection": { "id": collection.id, "name": collection.name, "schema": collection.schema },
            "auth": auth.map(|c| serde_json::json!({ "id": c.uid, "email": c.sub, "role": c.role })),
            "trigger": trigger
        });

        // Run Hook
        match state
            .script_engine
            .run_hook(
                &script.code,
                event_context,
                context.clone(),
                base_url.clone(),
                Some(actual_scope.clone()),
            )
            .await
        {
            Ok(Some(new_data)) => {
                current_data = new_data;
                modified = true;
            }
            Ok(None) => {}
            Err(err_msg) => {
                // If a hook fails, we block the operation
                return Err(AppError::Validation(vec![
                    ValidationError::ConstraintViolation("_hook".to_string(), err_msg),
                ]));
            }
        }
    }

    if modified {
        Ok(Some(current_data))
    } else {
        Ok(None)
    }
}
