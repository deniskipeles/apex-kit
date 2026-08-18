use apexkit_core::models::StoredFile;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

#[derive(Serialize, ToSchema)]
pub struct FileResponse {
    pub id: i64,
    pub url: String,
    pub filename: String,
}

#[derive(Deserialize, ToSchema, IntoParams)]
pub struct FileListQuery {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[derive(Deserialize, IntoParams)]
pub struct FileParams {
    pub thumb: Option<String>,
    pub format: Option<String>,
    pub quality: Option<u8>,
    pub blur: Option<f32>,
}

#[derive(Serialize, ToSchema)]
pub struct FileListResponse {
    pub items: Vec<StoredFile>,
    pub total: i64,
}

#[derive(ToSchema)]
#[allow(dead_code)]
pub struct FileUploadRequest {
    #[schema(value_type = String, format = Binary)]
    pub file: Vec<u8>,
}

#[derive(Deserialize, ToSchema)]
pub struct TestS3ConfigReq {
    pub bucket: Option<String>,
    pub region: Option<String>,
    pub endpoint: Option<String>,
    pub access_key: Option<String>,
    pub secret_key: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct MigrateStorageReq {
    pub source: String,
    pub destination: String,
}

#[derive(Serialize, ToSchema)]
pub struct MigrationResult {
    pub success: bool,
    pub processed: usize,
    pub errors: usize,
    pub message: String,
}

#[derive(Deserialize, IntoParams)]
pub struct FilenamePath {
    pub filename: String,
}

#[derive(Deserialize, IntoParams)]
pub struct FileIdPath {
    pub id: i64,
}

#[derive(Deserialize, IntoParams)]
pub struct GetFileQuery {
    pub expires_in: Option<u64>,
}

#[derive(Serialize, ToSchema)]
pub struct StoredFileWithSignedUrl {
    pub id: i64,
    pub filename: String,
    pub original_name: String,
    pub mime_type: String,
    pub size: i64,
    pub created_at: String,
    pub signed_url: String,
}

#[derive(Deserialize)]
pub struct GetFileParams {
    pub id: String,
}

#[derive(Deserialize, ToSchema, IntoParams)]
pub struct OpenGraphQuery {
    pub template: String,
    pub format: Option<String>,
    pub quality: Option<u8>,
    /// URL-encoded JSON string array: [{"type": "text|image", "value": "...", "target": "..."}]
    pub data: String,
}

#[derive(Deserialize)]
pub struct OgItem {
    pub r#type: String,
    pub value: String,
    pub target: String,
}
