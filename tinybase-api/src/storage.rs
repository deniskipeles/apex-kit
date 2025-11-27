// =========================== /teamspace/studios/this_studio/tinybase/tinybase/tinybase-api/src/storage.rs start here ===========================
use axum::{
    extract::{Multipart, State, Path, Query},
    response::Response, // Removed IntoResponse
    http::{StatusCode, header},
    Json, Extension,
    body::Body,
};
use serde::{Serialize, Deserialize};
use tinybase_core::auth::Claims;
use crate::{AppState, AppError};
use utoipa::{ToSchema, IntoParams};

use tinybase_core::models::StoredFile;

#[derive(Serialize, ToSchema)]
pub struct FileResponse {
    id: i64,
    url: String,
    filename: String,
}

#[derive(Deserialize, ToSchema, IntoParams)] 
pub struct FileListQuery {
    pub page: Option<i64>,      // It is good practice to make these pub
    pub per_page: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub struct FileListResponse {
    items: Vec<StoredFile>,
    total: i64, // We might fake this or fetch count if needed
}

// Dummy struct for Swagger UI documentation
#[derive(ToSchema)]
#[allow(dead_code)] // Fix warning: field `file` is never read
pub struct FileUploadRequest {
    #[schema(value_type = String, format = Binary)]
    file: Vec<u8>,
}

#[utoipa::path(
    post,
    path = "/api/v1/storage/upload",
    request_body(content = FileUploadRequest, content_type = "multipart/form-data"),
    responses((status = 201, body = FileResponse))
)]
pub async fn upload_file(
    auth: Option<Extension<Claims>>,
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<FileResponse>, AppError> {
    let claims = auth.map(|Extension(c)| c);
    let user_id = claims.map(|c| c.uid);

    while let Some(field) = multipart.next_field().await.map_err(|_| AppError::InputValidation(validator::ValidationErrors::new()))? {
        let original_name = field.file_name().unwrap_or("unknown.bin").to_string();
        let content_type = field.content_type().unwrap_or("application/octet-stream").to_string();
        
        let data = field.bytes().await.map_err(|_| AppError::UnknownError("Failed to read bytes".into()))?;
        
        let size = data.len() as i64;
        let ext = std::path::Path::new(&original_name)
            .extension()
            .and_then(std::ffi::OsStr::to_str)
            .unwrap_or("bin");
        
        let filename = format!("{}.{}", uuid::Uuid::new_v4(), ext);

        state.storage.save(&filename, &data).await
            .map_err(|e| AppError::UnknownError(e.to_string()))?;

        let id = state.db.create_file_metadata(&filename, &original_name, &content_type, size, user_id).await
            .map_err(|e| AppError::UnknownError(e.to_string()))?;

        let url = format!("{}{}", state.storage.get_public_url_base(), filename);

        return Ok(Json(FileResponse {
            id,
            url,
            filename,
        }));
    }

    Err(AppError::InputValidation(validator::ValidationErrors::new()))
}

#[utoipa::path(
    get,
    path = "/api/v1/storage/file/{filename}",
    responses((status = 200, description = "File Content"))
)]
pub async fn serve_file(
    State(state): State<AppState>,
    Path(filename): Path<String>,
) -> Result<Response, AppError> {
    let data = state.storage.get(&filename).await
        .map_err(|_| AppError::NotFound("File not found".into()))?;

    let mime_type = mime_guess::from_path(&filename).first_or_octet_stream();

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime_type.as_ref())
        .body(Body::from(data))
        .unwrap())
}

#[utoipa::path(
    get,
    path = "/api/v1/storage/files",
    params(FileListQuery),
    responses((status = 200, body = FileListResponse))
)]
pub async fn list_files(
    State(state): State<AppState>,
    Query(params): Query<FileListQuery>,
) -> Result<Json<FileListResponse>, AppError> {
    let page = params.page.unwrap_or(1).max(1);
    let limit = params.per_page.unwrap_or(20).min(100);
    let offset = (page - 1) * limit;

    let files = state.db.list_files(limit, offset).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    // Simplified total for now (or implement count_files in core)
    let total = files.len() as i64; 

    Ok(Json(FileListResponse { items: files, total }))
}

#[utoipa::path(
    delete,
    path = "/api/v1/storage/files/{id}",
    responses((status = 204, description = "File deleted"))
)]
pub async fn delete_file(
    auth: Option<Extension<Claims>>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, AppError> {
    let claims = auth.map(|Extension(c)| c);
    if let Some(claims) = claims {
        if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }
    } else {
        return Err(AppError::Unauthorized("Login required".into()));
    }

    // 1. Get Metadata to find filename
    let file = state.db.get_file_metadata(id).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?
        .ok_or(AppError::NotFound("File not found".into()))?;

    // 2. Delete from Storage (Disk/S3)
    state.storage.delete(&file.filename).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    // 3. Delete from DB
    state.db.delete_file_metadata(id).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}
// =========================== /teamspace/studios/this_studio/tinybase/tinybase/tinybase-api/src/storage.rs ends here ===========================