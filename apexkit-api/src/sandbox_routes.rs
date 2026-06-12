use crate::{AppError, AppState, sandbox_manager::CloneStrategy};
use apexkit_core::{
    ai_models::Plugin, auth::Claims, models::SandboxMetadata, realtime::EventScope,
};
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateSandboxReq {
    pub name: String,
    pub clone_strategy: String,
    pub clone_record_limit: Option<usize>,
    pub model: Option<String>,
    pub initial_prompt: Option<String>,
    pub collections: Option<Vec<String>>,
    pub scripts: Option<Vec<String>>,
    pub templates: Option<Vec<String>>,
}

#[derive(Deserialize, Serialize, utoipa::ToSchema, utoipa::IntoParams)]
pub struct SandboxPathParams {
    pub session_id: Option<String>,
    pub id: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/sandboxes",
    request_body = CreateSandboxReq,
    responses((status = 200, body = SandboxMetadata))
)]
pub async fn create_sandbox_handler(
    Extension(claims): Extension<Claims>,
    State(state): State<AppState>,
    scope: Option<Extension<EventScope>>,
    Json(req): Json<CreateSandboxReq>,
) -> Result<Json<SandboxMetadata>, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    let event_scope = scope.map(|s| s.0).unwrap_or(EventScope::Root);
    let tenant_id = crate::get_tenant_id_from_scope(Some(&event_scope));
    let scope_str = if tenant_id.is_some() {
        "tenant"
    } else {
        "root"
    };

    let parent_db = if let Some(tid) = &tenant_id {
        state
            .tenant_manager
            .get_tenant(tid.clone())
            .await
            .map_err(|_| AppError::UnknownError("Tenant missing".into()))?
    } else {
        state.db.clone()
    };

    let id = uuid::Uuid::new_v4().to_string();

    let strategy = match req.clone_strategy.as_str() {
        "schema" => CloneStrategy::SchemaOnly,
        "partial" => CloneStrategy::Partial(req.clone_record_limit.unwrap_or(100)),
        "full" => CloneStrategy::Full,
        "selected" => CloneStrategy::Selected {
            collections: req.collections.unwrap_or_default(),
            scripts: req.scripts.unwrap_or_default(),
            templates: req.templates.unwrap_or_default(),
            record_limit: req.clone_record_limit,
        },
        _ => CloneStrategy::None,
    };

    let existing = state
        .db
        .list_sandboxes(tenant_id.clone())
        .await
        .unwrap_or_default();

    let max_sandboxes = 3;
    if existing.len() >= max_sandboxes && scope_str != "root" {
        return Err(AppError::Forbidden(format!(
            "Sandbox limit ({}) reached.",
            max_sandboxes
        )));
    }

    state
        .sandbox_manager
        .create_sandbox(&id, strategy, parent_db, event_scope.clone())
        .await
        .map_err(|e| AppError::UnknownError(e))?;

    let expires_at = chrono::Utc::now()
        .checked_add_signed(chrono::Duration::hours(24))
        .map(|d| d.to_rfc3339());

    state
        .db
        .register_sandbox(
            &id,
            Some(claims.uid),
            Some(req.name.clone()),
            expires_at.clone(),
            scope_str,
            tenant_id.clone(),
        )
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    let state_clone = state.clone();
    let sid = id.clone();
    let prompt_opt = req.initial_prompt;
    let model = req.model.unwrap_or("gemini-2.5-flash".to_string());

    tokio::spawn(async move {
        if let Ok(sandbox_db) = state_clone.sandbox_manager.get_sandbox(&sid).await {
            let session = apexkit_core::ai_models::AiSession {
                id: "default".into(),
                name: "Architect".into(),
                messages: vec![],
                current_manifest: None,
                pending_manifest: None,
                diff_summary: None,
                last_error: None,
                created_at: chrono::Utc::now().to_rfc3339(),
            };
            let _ = sandbox_db.create_ai_session(&session).await;

            if let Some(prompt) = prompt_opt {
                let _ = crate::ai_architect::process_ai_chat(
                    &sid,
                    sandbox_db,
                    state_clone,
                    prompt,
                    model,
                )
                .await;
            }
        }
    });

    Ok(Json(SandboxMetadata {
        id,
        name: Some(req.name),
        status: "active".into(),
        expires_at,
        scope: scope_str.into(),
        tenant_id,
        current_storage_mb: 0.0,
        max_storage_mb: 100,
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/sandboxes",
    responses((status = 200, body = Vec<SandboxMetadata>))
)]
pub async fn list_sandboxes_handler(
    Extension(claims): Extension<Claims>,
    State(state): State<AppState>,
    scope: Option<Extension<EventScope>>, // [RESTORED] Use active workspace context
) -> Result<Json<Vec<SandboxMetadata>>, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    // [FIXED] List sandboxes belonging to the active Tenant/Root workspace being viewed
    let event_scope = scope.map(|s| s.0).unwrap_or(EventScope::Root);
    let tenant_id = crate::get_tenant_id_from_scope(Some(&event_scope));

    let sandboxes = state
        .db
        .list_sandboxes(tenant_id)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    Ok(Json(sandboxes))
}

#[utoipa::path(
    delete,
    path = "/api/v1/admin/sandboxes/{id}",
    responses((status = 204, description = "Deleted successfully"))
)]
pub async fn delete_sandbox_handler(
    Extension(claims): Extension<Claims>,
    State(state): State<AppState>,
    Path(params): Path<SandboxPathParams>,
) -> Result<StatusCode, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    let id = params.id;

    // Securely resolve user's true tenant boundary from JWT
    let tenant_id = if claims.scope.starts_with("tenant:") {
        Some(claims.scope.strip_prefix("tenant:").unwrap().to_string())
    } else {
        None
    };

    // Verify Ownership
    let sandboxes = state.db.list_sandboxes(tenant_id).await.unwrap_or_default();
    if !sandboxes.iter().any(|s| s.id == id) {
        return Err(AppError::Forbidden(
            "You do not have permission to delete this sandbox".into(),
        ));
    }

    state
        .db
        .delete_sandbox_metadata(&id)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    state.sandbox_manager.cleanup_sandbox(&id);

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/sandboxes/{id}/publish",
    responses((status = 200, body = Plugin))
)]
pub async fn publish_sandbox_handler(
    Extension(claims): Extension<Claims>,
    State(state): State<AppState>,
    Path(params): Path<SandboxPathParams>,
) -> Result<Json<Plugin>, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    let id = params.id;

    // 1. Securely resolve user's true tenant boundary from JWT
    let tenant_id = if claims.scope.starts_with("tenant:") {
        Some(claims.scope.strip_prefix("tenant:").unwrap().to_string())
    } else {
        None
    };

    // 2. Verify Ownership strictly against their owned list
    let sandboxes = state
        .db
        .list_sandboxes(tenant_id.clone())
        .await
        .unwrap_or_default();
    if !sandboxes.iter().any(|s| s.id == id) {
        return Err(AppError::Forbidden(
            "You do not have permission to publish this sandbox".into(),
        ));
    }

    let sandbox_db = state
        .sandbox_manager
        .get_sandbox(&id)
        .await
        .map_err(|_| AppError::NotFound("Sandbox expired or deleted".into()))?;

    // 3. Extract Manifest from the LIVE Sandbox Database
    let collections = sandbox_db.list_collections().await.unwrap_or_default();
    let scripts = sandbox_db.list_scripts().await.unwrap_or_default();
    let templates = sandbox_db.list_templates().await.unwrap_or_default();

    let manifest_cols = collections
        .into_iter()
        .map(|c| apexkit_core::models::ManifestCollection {
            name: c.name,
            schema: c.schema.unwrap_or_default(),
        })
        .collect();

    let manifest_scripts: Vec<apexkit_core::models::ManifestScript> = scripts
        .iter()
        .map(|s| apexkit_core::models::ManifestScript {
            name: s.name.clone(),
            trigger_type: s.trigger_type.clone(),
            code: s.code.clone(),
        })
        .collect();

    let manifest_templates = templates
        .into_iter()
        .map(|t| {
            let mut loader_script = None;
            if let Some(sid) = t.script_id {
                if let Some(s) = scripts.iter().find(|sc| sc.id == sid) {
                    loader_script = Some(s.name.clone());
                }
            }
            apexkit_core::models::ManifestTemplate {
                slug: t.slug,
                content: t.content,
                loader_script,
            }
        })
        .collect();

    let manifest = apexkit_core::models::AppManifest {
        app_name: format!("Sandbox Export {}", id),
        collections: manifest_cols,
        scripts: manifest_scripts,
        templates: manifest_templates,
    };

    // 4. Resolve Parent DB to push changes to (Tenant vs Root)
    let parent_db = if let Some(tid) = &tenant_id {
        state
            .tenant_manager
            .get_tenant(tid.clone())
            .await
            .map_err(|_| AppError::UnknownError("Tenant missing".into()))?
    } else {
        state.db.clone()
    };

    tracing::info!("AI Architect: Committing Sandbox {} to Parent DB...", id);

    // 5. Deploy to Production
    crate::ai_architect::deploy_manifest(parent_db.clone(), &manifest).await?;

    let plugin = Plugin {
        id: uuid::Uuid::new_v4().to_string(),
        name: manifest.app_name.clone(),
        version: "1.0.0".to_string(),
        manifest,
        description: Some(format!("Exported from sandbox: {}", id)),
    };
    parent_db
        .save_plugin(&plugin)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    // 6. Cleanup Sandbox
    state.db.delete_sandbox_metadata(&id).await.ok();
    state.sandbox_manager.cleanup_sandbox(&id);

    Ok(Json(plugin))
}
