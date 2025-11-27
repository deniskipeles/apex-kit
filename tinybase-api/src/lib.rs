use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tinybase_core::{
    models::{Collection as CollectionModel, Record},
    schema::CollectionSchema,
    validation::{validate_record, ValidationError},
    Db,
};
use utoipa::{OpenApi, ToSchema};
use utoipa_swagger_ui::SwaggerUi;

pub type AppState = Arc<dyn Db>;

#[derive(Serialize, ToSchema)]
pub struct CollectionResponse {
    id: i64,
    name: String,
    schema: Option<CollectionSchema>,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateCollection {
    name: Option<String>,
    schema: Option<CollectionSchema>,
}

#[derive(Serialize, ToSchema)]
pub struct RecordResponse {
    id: i64,
    data: serde_json::Value,
}

#[derive(Serialize, ToSchema)]
struct ProblemDetail {
    error: String,
    message: String,
    details: Option<serde_json::Value>,
    status: u16,
}

pub enum AppError {
    LibsqlError(libsql::Error),
    JsonError(String),
    UnknownError(String),
    NotFound(String),
    Validation(Vec<ValidationError>),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, problem) = match self {
            AppError::LibsqlError(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ProblemDetail {
                    error: "database_error".to_string(),
                    message: "A database error occurred.".to_string(),
                    details: Some(serde_json::json!({ "db_error": e.to_string() })),
                    status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                },
            ),
            AppError::JsonError(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ProblemDetail {
                    error: "serialization_error".to_string(),
                    message: "Failed to serialize data.".to_string(),
                    details: Some(serde_json::json!({ "json_error": e })),
                    status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                },
            ),
            AppError::UnknownError(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ProblemDetail {
                    error: "unknown_error".to_string(),
                    message: "An unknown error occurred.".to_string(),
                    details: Some(serde_json::json!({ "error": e })),
                    status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                },
            ),
            AppError::NotFound(e) => (
                StatusCode::NOT_FOUND,
                ProblemDetail {
                    error: "not_found".to_string(),
                    message: e,
                    details: None,
                    status: StatusCode::NOT_FOUND.as_u16(),
                },
            ),
            AppError::Validation(e) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                ProblemDetail {
                    error: "validation_error".to_string(),
                    message: "Input validation failed.".to_string(),
                    details: Some(serde_json::json!(e)),
                    status: StatusCode::UNPROCESSABLE_ENTITY.as_u16(),
                },
            ),
        };

        (status, Json(problem)).into_response()
    }
}

impl From<libsql::Error> for AppError {
    fn from(e: libsql::Error) -> Self {
        AppError::LibsqlError(e)
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(
        create_collection,
        list_collections,
        get_collection,
        update_collection,
        delete_collection,
        create_record,
        list_records,
        get_record,
        update_record,
        delete_record,
    ),
    components(
        schemas(CollectionResponse, UpdateCollection, RecordResponse, ProblemDetail)
    ),
    tags(
        (name = "Tinybase", description = "Tinybase API")
    )
)]
struct ApiDoc;

pub fn app_router(db: AppState) -> Router {
    Router::new()
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .nest(
            "/api/v1",
            Router::new()
                .route("/collections", post(create_collection).get(list_collections))
                .route(
                    "/collections/:id",
                    get(get_collection)
                        .patch(update_collection)
                        .delete(delete_collection),
                )
                .route(
                    "/collections/:id/records",
                    post(create_record).get(list_records),
                )
                .route(
                    "/collections/:id/records/:record_id",
                    get(get_record)
                        .patch(update_record)
                        .delete(delete_record),
                ),
        )
        .with_state(db)
}

#[utoipa::path(
    post,
    path = "/api/v1/collections",
    request_body = CollectionModel,
    responses(
        (status = 201, description = "Create a new collection", body = CollectionResponse),
        (status = 500, description = "Internal server error", body = ProblemDetail)
    )
)]
async fn create_collection(
    State(db): State<AppState>,
    Json(payload): Json<CollectionModel>,
) -> Result<(StatusCode, Json<CollectionResponse>), AppError> {
    let id = db
        .create_collection(&payload.name, &payload.schema)
        .await
        .map_err(|e| {
            if let Some(e) = e.downcast_ref::<serde_json::Error>() {
                AppError::JsonError(e.to_string())
            } else if let Ok(e) = e.downcast::<libsql::Error>() {
                AppError::LibsqlError(*e)
            } else {
                AppError::UnknownError("An unknown error occurred".to_string())
            }
        })?;
    Ok((
        StatusCode::CREATED,
        Json(CollectionResponse {
            id,
            name: payload.name,
            schema: payload.schema,
        }),
    ))
}

#[utoipa::path(
    get,
    path = "/api/v1/collections",
    responses(
        (status = 200, description = "List all collections", body = Vec<CollectionResponse>),
        (status = 500, description = "Internal server error", body = ProblemDetail)
    )
)]
async fn list_collections(
    State(db): State<AppState>,
) -> Result<Json<Vec<CollectionResponse>>, AppError> {
    let collections = db.list_collections().await.map_err(|e| {
        if let Ok(e) = e.downcast::<libsql::Error>() {
            AppError::LibsqlError(*e)
        } else {
            AppError::UnknownError("An unknown error occurred".to_string())
        }
    })?;
    let collections = collections
        .into_iter()
        .map(|c| CollectionResponse {
            id: c.id,
            name: c.name,
            schema: c.schema,
        })
        .collect();
    Ok(Json(collections))
}

#[utoipa::path(
    get,
    path = "/api/v1/collections/{id}",
    params(
        ("id" = i64, Path, description = "Collection id")
    ),
    responses(
        (status = 200, description = "Get a single collection", body = CollectionResponse),
        (status = 404, description = "Collection not found", body = ProblemDetail),
        (status = 500, description = "Internal server error", body = ProblemDetail)
    )
)]
async fn get_collection(
    State(db): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<CollectionResponse>, AppError> {
    let collection = db.get_collection(id).await.map_err(|e| {
        if let Ok(e) = e.downcast::<libsql::Error>() {
            AppError::LibsqlError(*e)
        } else {
            AppError::UnknownError("An unknown error occurred".to_string())
        }
    })?;
    match collection {
        Some(c) => Ok(Json(CollectionResponse {
            id: c.id,
            name: c.name,
            schema: c.schema,
        })),
        None => Err(AppError::NotFound(format!("Collection {} not found", id))),
    }
}

#[utoipa::path(
    patch,
    path = "/api/v1/collections/{id}",
    params(
        ("id" = i64, Path, description = "Collection id")
    ),
    request_body = UpdateCollection,
    responses(
        (status = 200, description = "Update a collection", body = CollectionResponse),
        (status = 404, description = "Collection not found", body = ProblemDetail),
        (status = 500, description = "Internal server error", body = ProblemDetail)
    )
)]
async fn update_collection(
    State(db): State<AppState>,
    Path(id): Path<i64>,
    Json(payload): Json<UpdateCollection>,
) -> Result<Json<CollectionResponse>, AppError> {
    let collection = db
        .update_collection(id, payload.name, payload.schema)
        .await
        .map_err(|e| {
            if let Ok(e) = e.downcast::<libsql::Error>() {
                AppError::LibsqlError(*e)
            } else {
                AppError::UnknownError("An unknown error occurred".to_string())
            }
        })?;
    Ok(Json(CollectionResponse {
        id: collection.id,
        name: collection.name,
        schema: collection.schema,
    }))
}

#[utoipa::path(
    delete,
    path = "/api/v1/collections/{id}",
    params(
        ("id" = i64, Path, description = "Collection id")
    ),
    responses(
        (status = 204, description = "Delete a collection"),
        (status = 500, description = "Internal server error", body = ProblemDetail)
    )
)]
async fn delete_collection(State(db): State<AppState>, Path(id): Path<i64>) -> Result<StatusCode, AppError> {
    db.delete_collection(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/v1/collections/{id}/records",
    params(
        ("id" = i64, Path, description = "Collection id")
    ),
    request_body = Record,
    responses(
        (status = 201, description = "Create a new record", body = RecordResponse),
        (status = 404, description = "Collection not found", body = ProblemDetail),
        (status = 422, description = "Validation error", body = ProblemDetail),
        (status = 500, description = "Internal server error", body = ProblemDetail)
    )
)]
async fn create_record(
    State(db): State<AppState>,
    Path(id): Path<i64>,
    Json(payload): Json<Record>,
) -> Result<(StatusCode, Json<RecordResponse>), AppError> {
    let collection = db.get_collection(id).await.map_err(|e| {
        if let Ok(e) = e.downcast::<libsql::Error>() {
            AppError::LibsqlError(*e)
        } else {
            AppError::UnknownError("An unknown error occurred".to_string())
        }
    })?;
    if let Some(c) = collection {
        if let Some(schema) = &c.schema {
            validate_record(schema, &payload.data).map_err(AppError::Validation)?;
        }
    } else {
        return Err(AppError::NotFound(format!("Collection {} not found", id)));
    }

    let record_id = db.create_record(id, &payload.data).await.map_err(|e| {
        if let Some(e) = e.downcast_ref::<serde_json::Error>() {
            AppError::JsonError(e.to_string())
        } else if let Ok(e) = e.downcast::<libsql::Error>() {
            AppError::LibsqlError(*e)
        } else {
            AppError::UnknownError("An unknown error occurred".to_string())
        }
    })?;
    Ok((
        StatusCode::CREATED,
        Json(RecordResponse {
            id: record_id,
            data: payload.data,
        }),
    ))
}

#[utoipa::path(
    get,
    path = "/api/v1/collections/{id}/records",
    params(
        ("id" = i64, Path, description = "Collection id")
    ),
    responses(
        (status = 200, description = "List all records in a collection", body = Vec<RecordResponse>),
        (status = 500, description = "Internal server error", body = ProblemDetail)
    )
)]
async fn list_records(
    State(db): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<RecordResponse>>, AppError> {
    let records = db.list_records(id).await.map_err(|e| {
        if let Ok(e) = e.downcast::<libsql::Error>() {
            AppError::LibsqlError(*e)
        } else {
            AppError::UnknownError("An unknown error occurred".to_string())
        }
    })?;
    let records = records
        .into_iter()
        .map(|r| RecordResponse {
            id: r.id,
            data: r.data,
        })
        .collect();
    Ok(Json(records))
}

#[utoipa::path(
    get,
    path = "/api/v1/collections/{id}/records/{record_id}",
    params(
        ("id" = i64, Path, description = "Collection id"),
        ("record_id" = i64, Path, description = "Record id")
    ),
    responses(
        (status = 200, description = "Get a single record", body = RecordResponse),
        (status = 404, description = "Record not found", body = ProblemDetail),
        (status = 500, description = "Internal server error", body = ProblemDetail)
    )
)]
async fn get_record(
    State(db): State<AppState>,
    Path((collection_id, record_id)): Path<(i64, i64)>,
) -> Result<Json<RecordResponse>, AppError> {
    let record = db
        .get_record(collection_id, record_id)
        .await
        .map_err(|e| {
            if let Ok(e) = e.downcast::<libsql::Error>() {
                AppError::LibsqlError(*e)
            } else {
                AppError::UnknownError("An unknown error occurred".to_string())
            }
        })?;
    match record {
        Some(r) => Ok(Json(RecordResponse {
            id: r.id,
            data: r.data,
        })),
        None => Err(AppError::NotFound(format!(
            "Record {} not found in collection {}",
            record_id, collection_id
        ))),
    }
}

#[utoipa::path(
    patch,
    path = "/api/v1/collections/{id}/records/{record_id}",
    params(
        ("id" = i64, Path, description = "Collection id"),
        ("record_id" = i64, Path, description = "Record id")
    ),
    request_body = Record,
    responses(
        (status = 200, description = "Update a record", body = RecordResponse),
        (status = 404, description = "Record not found", body = ProblemDetail),
        (status = 422, description = "Validation error", body = ProblemDetail),
        (status = 500, description = "Internal server error", body = ProblemDetail)
    )
)]
async fn update_record(
    State(db): State<AppState>,
    Path((collection_id, record_id)): Path<(i64, i64)>,
    Json(payload): Json<Record>,
) -> Result<Json<RecordResponse>, AppError> {
    let collection = db.get_collection(collection_id).await.map_err(|e| {
        if let Ok(e) = e.downcast::<libsql::Error>() {
            AppError::LibsqlError(*e)
        } else {
            AppError::UnknownError("An unknown error occurred".to_string())
        }
    })?;
    if let Some(c) = collection {
        if let Some(schema) = &c.schema {
            validate_record(schema, &payload.data).map_err(AppError::Validation)?;
        }
    } else {
        return Err(AppError::NotFound(format!(
            "Collection {} not found",
            collection_id
        )));
    }

    let record = db
        .update_record(collection_id, record_id, &payload.data)
        .await
        .map_err(|e| {
            if let Ok(e) = e.downcast::<libsql::Error>() {
                AppError::LibsqlError(*e)
            } else {
                AppError::UnknownError("An unknown error occurred".to_string())
            }
        })?;
    Ok(Json(RecordResponse {
        id: record.id,
        data: record.data,
    }))
}

#[utoipa::path(
    delete,
    path = "/api/v1/collections/{id}/records/{record_id}",
    params(
        ("id" = i64, Path, description = "Collection id"),
        ("record_id" = i64, Path, description = "Record id")
    ),
    responses(
        (status = 204, description = "Delete a record"),
        (status = 500, description = "Internal server error", body = ProblemDetail)
    )
)]
async fn delete_record(
    State(db): State<AppState>,
    Path((collection_id, record_id)): Path<(i64, i64)>,
) -> Result<StatusCode, AppError> {
    db.delete_record(collection_id, record_id).await?;
    Ok(StatusCode::NO_CONTENT)
}
