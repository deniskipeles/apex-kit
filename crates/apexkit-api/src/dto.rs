use apexkit_core::{models::schema::CollectionSchema, validation::ValidationError};
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use std::fmt;
use utoipa::{IntoParams, ToSchema};
use validator::Validate;

// --- DTOs ---

// --- Path Structs for Nested Routes ---
#[derive(Deserialize, IntoParams)]
pub struct IdPath {
    pub id: String, // Can be "1" (ID) or "posts" (Name)
}

#[derive(Deserialize, IntoParams)]
pub struct RecordPath {
    pub id: String,     // Collection ID or Name
    pub record_id: i64, // Maps to {record_id}
}

#[derive(Serialize, ToSchema, Deserialize)]
pub struct CollectionResponse {
    pub id: i64,
    pub name: String,
    pub schema: Option<CollectionSchema>,
    pub index: Option<String>,
}
#[derive(Deserialize, ToSchema, Validate, Serialize)]
pub struct UpdateCollection {
    #[validate(length(min = 1, max = 50))]
    pub name: Option<String>,
    pub schema: Option<CollectionSchema>,
}
#[derive(Deserialize, ToSchema, Validate)]
pub struct CreateCollectionReq {
    #[validate(length(min = 1, max = 50))]
    pub name: String,
    pub schema: Option<CollectionSchema>,
    pub index: Option<String>,
}
#[derive(Serialize, ToSchema, Deserialize)]
pub struct RecordResponse {
    pub id: i64,
    pub data: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expand: Option<serde_json::Value>,
    pub created: String,
    pub updated: String,
}
#[derive(Deserialize, ToSchema, Validate)]
pub struct AuthRequest {
    #[validate(email)]
    pub email: String,
    #[validate(length(min = 6))]
    pub password: String,
    // Optional Role (defaults to "user" if not provided or restricted)
    pub role: Option<String>,
    // Optional Metadata
    #[schema(value_type = Option<Object>)]
    pub metadata: Option<serde_json::Value>,
}
#[derive(Serialize, ToSchema)]
pub struct AuthResponse {
    pub token: String,
    pub user: UserDto,
}
#[derive(Serialize, ToSchema, Deserialize)]
pub struct UserDto {
    pub id: i64,
    pub email: String,
    pub role: String,
    pub metadata: Option<serde_json::Value>,
    // Authoritative scope from the current session token
    pub scope: Option<String>,
}
#[derive(Serialize, ToSchema)]
pub struct ProblemDetail {
    pub error: String,
    pub message: String,
    pub details: Option<serde_json::Value>,
    pub status: u16,
}
#[derive(Deserialize, ToSchema)]
pub struct RelationRequest {
    pub target_collection_id: i64,
    pub target_record_id: i64,
    pub relation_name: String,
}
#[derive(Deserialize, ToSchema, IntoParams)]
pub struct SearchQuery {
    pub q: String,
    pub limit: Option<usize>,
    pub expand: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}
#[derive(Serialize, ToSchema, Deserialize)]
pub struct RecordListResponse {
    pub items: Vec<RecordResponse>,
    pub total: i64,
}

#[derive(Debug)]
pub enum AppError {
    RusqliteError(rusqlite::Error),
    JsonError(String),
    UnknownError(String),
    NotFound(String),
    Validation(Vec<ValidationError>),
    InputValidation(validator::ValidationErrors),
    Unauthorized(String),
    Forbidden(String),
    // --- NEW: DETAILED TEMPLATE RENDER ERROR ---
    RenderError {
        template: String,
        error: String,
        details: serde_json::Value,
    },
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, msg, details) = match self {
            AppError::NotFound(m) => (StatusCode::NOT_FOUND, m, None),
            AppError::Forbidden(m) => (StatusCode::FORBIDDEN, m, None),
            AppError::Unauthorized(m) => (StatusCode::UNAUTHORIZED, m, None),
            AppError::Validation(v) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "Schema Validation Failed".into(),
                Some(serde_json::json!(v)),
            ),
            AppError::InputValidation(v) => (
                StatusCode::BAD_REQUEST,
                "Input Validation Failed".into(),
                Some(serde_json::json!(v)),
            ),
            AppError::JsonError(m) => (StatusCode::BAD_REQUEST, m, None),
            AppError::RusqliteError(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Database Error: {}", e),
                None,
            ),
            // Map the detailed RenderError to internal server error with JSON details
            AppError::RenderError {
                template,
                error,
                details,
            } => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Template Compilation Error on '{}': {}", template, error),
                Some(details),
            ),
            AppError::UnknownError(m) => (StatusCode::INTERNAL_SERVER_ERROR, m, None),
        };

        let body = Json(ProblemDetail {
            error: status.canonical_reason().unwrap_or("error").to_string(),
            message: msg,
            details,
            status: status.as_u16(),
        });

        (status, body).into_response()
    }
}
