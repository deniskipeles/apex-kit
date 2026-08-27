use crate::AppError;
use axum::extract::Multipart;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

pub mod ai_actions;
pub mod data;
pub mod schema;
pub mod scripts;
pub mod templates;

// --- DTOs ---

use apexkit_core::models::Collection;
// [NEW] DTO for Schema Import
#[derive(Deserialize, ToSchema)]
pub struct ImportSchemaRequestDto {
    #[serde(default)]
    pub apexkit_version: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    pub collections: Vec<Collection>, // Array of full collection objects
    #[serde(default)]
    pub strategy: String, // "skip", "overwrite", "error"
}

#[derive(Serialize, ToSchema)]
pub struct ImportSchemaResponseDto {
    pub created: usize,
    pub updated: usize,
    pub skipped: usize,
    pub errors: Vec<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct ImportDataRequestDto {
    #[schema(example = "products")]
    pub collection_name: String,
}

#[derive(Serialize, ToSchema)]
pub struct ImportDataResponseDto {
    pub collection_id: i64,
    pub records_imported: usize,
    pub collection_created: bool,
    pub schema_updated: bool,
    pub time_taken_to_insert_all: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ImportResult {
    pub created: usize,
    pub updated: usize,
    pub errors: Vec<String>,
}

// Helper for multipart file reading
pub async fn read_file_from_multipart(mut multipart: Multipart) -> Result<Vec<u8>, AppError> {
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| AppError::UnknownError("Multipart error".into()))?
    {
        if field.name() == Some("file") {
            return field
                .bytes()
                .await
                .map_err(|_| AppError::UnknownError("Failed to read bytes".into()))
                .map(|b| b.to_vec());
        }
    }
    Err(AppError::UnknownError("No file uploaded".into()))
}
