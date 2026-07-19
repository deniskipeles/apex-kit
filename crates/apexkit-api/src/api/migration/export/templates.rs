use crate::AppError;
use crate::DatabaseConnection;
use crate::api::migration::export::ExportQuery;
use apexkit_core::auth::Claims;
use apexkit_core::realtime::EventScope;
use axum::{Extension, extract::Query, http::header, response::Response};

// Handler: Export Templates
#[utoipa::path(
    get,
    path = "/api/v1/admin/export-templates",
    params(ExportQuery),
    responses((status = 200, description = "Templates JSON or TXT"))
)]
pub async fn export_templates_handler(
    Extension(claims): Extension<Claims>,
    scope: Option<Extension<EventScope>>,
    DatabaseConnection(db): DatabaseConnection,
    Query(params): Query<ExportQuery>,
) -> Result<Response, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    let event_scope = scope.map(|s| s.0).unwrap_or(EventScope::Root);
    let (scope_type, scope_id) = match event_scope {
        EventScope::Root => ("root", "root".to_string()),
        EventScope::Tenant(id) => ("tenant", id),
        EventScope::Sandbox(id) => ("sandbox", id),
        _ => ("unknown", "unknown".to_string()),
    };

    let date_str = chrono::Utc::now().format("%Y%m%d").to_string();
    let format_ext = if params.format.to_lowercase() == "txt" {
        "txt"
    } else {
        "json"
    };
    let filename = format!(
        "apexkit-template-{}-{}-{}.{}",
        scope_type, scope_id, date_str, format_ext
    );

    let templates = db
        .list_templates()
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    let (content_type, body_bytes) = if format_ext == "txt" {
        let mut output = String::new();
        for tmpl in &templates {
            let mut meta = serde_json::to_value(tmpl).unwrap();
            if let Some(obj) = meta.as_object_mut() {
                obj.remove("content");
            }
            output.push_str("<!-- ====================start-metadata==================== -->\n");
            output.push_str(&format!(
                "<!-- {} -->\n",
                serde_json::to_string(&meta).unwrap()
            ));
            output.push_str("<!-- ====================end-metadata==================== -->\n");
            output.push_str("<!-- ====================start-code==================== -->\n");
            output.push_str(&tmpl.content);
            output.push_str("\n<!-- ====================end-code==================== -->\n\n");
        }
        ("text/plain; charset=utf-8", output.into_bytes())
    } else {
        let json_bytes = serde_json::to_vec_pretty(&templates)
            .map_err(|e| AppError::UnknownError(format!("Serialization Error: {}", e)))?;
        ("application/json; charset=utf-8", json_bytes)
    };

    Response::builder()
        .header(header::CONTENT_TYPE, content_type)
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", filename),
        )
        .body(body_bytes.into())
        .map_err(|e| AppError::UnknownError(format!("Response build failed: {}", e)))
}
