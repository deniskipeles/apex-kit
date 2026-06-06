use crate::{AppError, AppState, DatabaseConnection, sandbox_manager::CloneStrategy};
use apexkit_core::{
    ai_models::Plugin, auth::Claims, models::SandboxMetadata, realtime::EventScope,
};
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::Deserialize;

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateSandboxReq {
    pub name: String,
    pub clone_strategy: String,
    pub clone_record_limit: Option<usize>,
    pub model: Option<String>,
    pub initial_prompt: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/sandboxes",
    request_body = CreateSandboxReq,
    responses((status = 200, body = SandboxMetadata))
)]
pub async fn create_sandbox_handler(
    Extension(claims): Extension<Claims>,
    DatabaseConnection(db): DatabaseConnection, // Contextual DB (Root or Tenant)
    State(state): State<AppState>,
    scope: Option<Extension<EventScope>>,
    Json(req): Json<CreateSandboxReq>,
) -> Result<Json<SandboxMetadata>, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    let id = uuid::Uuid::new_v4().to_string();
    let event_scope = scope.map(|s| s.0).unwrap_or(EventScope::Root);
    let tenant_id = crate::get_tenant_id_from_scope(Some(&event_scope));
    let scope_str = if tenant_id.is_some() {
        "tenant"
    } else {
        "root"
    };

    let strategy = match req.clone_strategy.as_str() {
        "schema" => CloneStrategy::SchemaOnly,
        "partial" => CloneStrategy::Partial(req.clone_record_limit.unwrap_or(100)),
        "full" => CloneStrategy::Full,
        _ => CloneStrategy::None,
    };

    // 1. Enforce Quotas
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

    // 2. Clone physically from the contextual DB
    state
        .sandbox_manager
        .create_sandbox(&id, strategy, db.clone())
        .await
        .map_err(|e| AppError::UnknownError(e))?;

    // 3. Register Metadata in Root DB
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

    // 4. If initial prompt is provided, start background task to seed AI Architect
    if let Some(prompt) = req.initial_prompt {
        let state_clone = state.clone();
        let sid = id.clone();
        let model = req.model.unwrap_or("gemini-2.5-flash".to_string());

        tokio::spawn(async move {
            if let Ok(sandbox_db) = state_clone.sandbox_manager.get_sandbox(&sid).await {
                // Initialize default session record
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
                // Run chat generation inside sandbox
                let _ = crate::ai_architect::process_ai_chat(
                    &sid,
                    sandbox_db,
                    state_clone,
                    prompt,
                    model,
                )
                .await;
            }
        });
    }

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
    auth: Option<Extension<Claims>>,
    State(state): State<AppState>,
    scope: Option<Extension<EventScope>>,
) -> Result<Json<Vec<SandboxMetadata>>, AppError> {
    let claims = auth
        .ok_or(AppError::Unauthorized("Login required".into()))?
        .0;
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    let event_scope = scope.map(|s| s.0).unwrap_or(EventScope::Root);
    let tenant_id = crate::get_tenant_id_from_scope(Some(&event_scope));

    // For root scope, tenant_id is None, so it fetches all sandboxes
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
    auth: Option<Extension<Claims>>,
    State(state): State<AppState>,
    scope: Option<Extension<EventScope>>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let claims = auth
        .ok_or(AppError::Unauthorized("Login required".into()))?
        .0;
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    let event_scope = scope.map(|s| s.0).unwrap_or(EventScope::Root);
    let tenant_id = crate::get_tenant_id_from_scope(Some(&event_scope));

    if let Some(tid) = tenant_id {
        let sandboxes = state.db.list_sandboxes(Some(tid)).await.unwrap_or_default();
        if !sandboxes.iter().any(|s| s.id == id) {
            return Err(AppError::Forbidden(
                "You do not have permission to delete this sandbox".into(),
            ));
        }
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
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection, // The Parent DB (Root or Tenant)
    State(state): State<AppState>,
    scope: Option<Extension<EventScope>>,
    Path(id): Path<String>,
) -> Result<Json<Plugin>, AppError> {
    let claims = auth
        .ok_or(AppError::Unauthorized("Login required".into()))?
        .0;
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    let event_scope = scope.map(|s| s.0).unwrap_or(EventScope::Root);
    let tenant_id = crate::get_tenant_id_from_scope(Some(&event_scope));

    // Verify ownership
    if let Some(tid) = tenant_id {
        let sandboxes = state.db.list_sandboxes(Some(tid)).await.unwrap_or_default();
        if !sandboxes.iter().any(|s| s.id == id) {
            return Err(AppError::Forbidden(
                "You do not have permission to publish this sandbox".into(),
            ));
        }
    }

    // 1. Fetch Sandbox DB
    let sandbox_db = state
        .sandbox_manager
        .get_sandbox(&id)
        .await
        .map_err(|_| AppError::NotFound("Sandbox expired or deleted".into()))?;

    // 2. Extract Manifest from Sandbox DB
    let session = sandbox_db
        .get_ai_session("default")
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?
        .ok_or(AppError::NotFound("Sandbox AI Session missing".into()))?;
    let manifest = session
        .current_manifest
        .ok_or_else(|| AppError::Validation(vec![]))?;

    tracing::info!("AI Architect: Committing Sandbox {} to Parent...", id);

    // 3. Deploy to Parent DB
    crate::ai_architect::deploy_manifest(db.clone(), &manifest).await?;

    // 4. Save Plugin Record in Parent DB
    let plugin = Plugin {
        id: uuid::Uuid::new_v4().to_string(),
        name: manifest.app_name.clone(),
        version: "1.0.0".to_string(),
        manifest,
        description: Some(format!("Exported from sandbox: {}", id)),
    };
    db.save_plugin(&plugin)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    // 5. Cleanup Sandbox
    state.db.delete_sandbox_metadata(&id).await.ok();
    state.sandbox_manager.cleanup_sandbox(&id);

    Ok(Json(plugin))
}
