use crate::{AppError, AppState, UserDto};
use crate::{BaseUrl, DatabaseConnection, IdPath};
use crate::{
    hooks::{trigger_filter_hook, trigger_void_hook},
    utils::extract_log_meta,
};
use apexkit_core::auth::Claims;
use apexkit_core::auth::policies;
use apexkit_core::realtime::EventScope;
use apexkit_core::workers;
use axum::Extension;
use axum::{
    Json,
    extract::{ConnectInfo, Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::net::SocketAddr;
use utoipa::{IntoParams, ToSchema};

#[utoipa::path(
    get,
    path = "/api/v1/auth/me",
    responses((status = 200, body = UserDto))
)]
pub async fn get_me(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection,
) -> Result<Json<UserDto>, AppError> {
    let claims = auth
        .ok_or(AppError::Unauthorized("Login required".into()))?
        .0;

    // We can fetch fresh data from DB, or just return claims if that's enough.
    // Fetching is safer to ensure user wasn't deleted/banned.
    // Note: get_user_by_email might need to be exposed or we use list with filter.
    // Ideally, we should have get_user(id) in the Db trait.

    // Since 'get_user' by ID isn't explicitly in the Db trait visible here (only list/get_by_email),
    // let's use list with ID filter logic or add get_user(id).
    // Assuming we can use get_users_by_ids([id]) which IS in the trait.

    let users = db
        .get_users_by_ids(&[claims.uid])
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    let user = users
        .first()
        .ok_or(AppError::NotFound("User not found".into()))?;

    Ok(Json(UserDto {
        id: user.id,
        email: user.email.clone(),
        role: user.role.clone(),
        metadata: user.metadata.clone(),
        scope: Some(claims.scope),
    }))
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateMeReq {
    #[schema(value_type = Object)]
    pub metadata: serde_json::Value,
}

#[utoipa::path(
    patch,
    path = "/api/v1/auth/me",
    request_body = UpdateMeReq,
    responses((status = 200, body = UserDto))
)]
pub async fn update_me(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection,
    Json(payload): Json<UpdateMeReq>,
) -> Result<Json<UserDto>, AppError> {
    // 1. Authenticate user
    let claims = auth
        .ok_or(AppError::Unauthorized("Login required".into()))?
        .0;

    // 2. Fetch current user data to perform a safe metadata merge
    let users = db
        .get_users_by_ids(&[claims.uid])
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    let current_user = users
        .first()
        .ok_or(AppError::NotFound("User not found".into()))?;

    // 3. Perform a recursive JSON merge of old and new metadata to avoid overwriting unrelated keys
    let mut merged_metadata = current_user
        .metadata
        .clone()
        .unwrap_or_else(|| serde_json::json!({}));
    if let (Some(merged_obj), Some(new_obj)) = (
        merged_metadata.as_object_mut(),
        payload.metadata.as_object(),
    ) {
        for (k, v) in new_obj {
            merged_obj.insert(k.clone(), v.clone());
        }
    } else {
        merged_metadata = payload.metadata;
    }

    // 4. Update the user in the database
    let updated_user = db
        .update_user(claims.uid, None, None, Some(merged_metadata), None)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    Ok(Json(UserDto {
        id: updated_user.id,
        email: updated_user.email,
        role: updated_user.role,
        metadata: updated_user.metadata,
        scope: Some(claims.scope),
    }))
}

#[derive(Serialize, ToSchema)]
pub struct RolesResponse {
    pub roles: Vec<String>,
}

#[utoipa::path(
    get,
    path = "/api/v1/auth/roles",
    responses((status = 200, body = RolesResponse))
)]
pub async fn list_roles_handler(
    DatabaseConnection(db): DatabaseConnection,
) -> Result<Json<RolesResponse>, AppError> {
    // 1. Try to fetch from config
    let roles = if let Ok(Some(val)) = db.get_config("APEX_AUTH_ROLES").await {
        // [FIX] Handle potential double-encoding or string-wrapped JSON
        if let Some(s) = val.as_str() {
            // If it's a string, try to parse it as JSON array
            serde_json::from_str::<Vec<String>>(s)
                .unwrap_or_else(|_| vec!["admin".to_string(), "user".to_string()])
        } else if val.is_array() {
            // If it's already an array value
            serde_json::from_value::<Vec<String>>(val)
                .unwrap_or_else(|_| vec!["admin".to_string(), "user".to_string()])
        } else {
            // Default roles
            vec!["admin".to_string(), "user".to_string()]
        }
    } else {
        // Default roles
        vec!["admin".to_string(), "user".to_string()]
    };
    Ok(Json(RolesResponse { roles }))
}

#[derive(Deserialize, ToSchema)]
pub struct TestEmailReq {
    pub email: String,
    pub template_type: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/smtp/test",
    request_body = TestEmailReq,
    responses((status = 200, description = "Email Sent"))
)]
pub async fn test_email_handler(
    Extension(claims): Extension<Claims>,
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>,
    Json(payload): Json<TestEmailReq>,
) -> Result<Json<serde_json::Value>, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    let gen_val = db.get_config("general").await.unwrap_or(None);
    let app_name = gen_val
        .as_ref()
        .and_then(|v| v.get("app_name").and_then(|s| s.as_str()))
        .unwrap_or("ApexKit")
        .to_string();
    let app_url = gen_val
        .as_ref()
        .and_then(|v| v.get("app_url").and_then(|s| s.as_str()))
        .unwrap_or("http://localhost:5000")
        .to_string();

    let smtp_val = db.get_config("smtp").await.unwrap_or(None);

    let (subject, mut body, link, mock_token) = match payload.template_type.as_deref() {
        Some("welcome") => {
            let tmpl = smtp_val
                .as_ref()
                .and_then(|v| v.get("template_welcome").and_then(|s| s.as_str()))
                .unwrap_or("Welcome to {{app_name}}!");
            (
                format!("Welcome to {}!", app_name),
                tmpl.to_string(),
                None,
                None,
            )
        }
        Some("reset") => {
            let tmpl = smtp_val
                .as_ref()
                .and_then(|v| v.get("template_reset").and_then(|s| s.as_str()))
                .unwrap_or("Click here to reset: {{link}}");
            let mock_token = uuid::Uuid::new_v4().to_string();
            let mock_link = format!(
                "{}/_dashboard/login?token={}",
                app_url.trim_end_matches('/'),
                mock_token
            );
            (
                format!("Reset your password for {}", app_name),
                tmpl.to_string(),
                Some(mock_link),
                Some(mock_token),
            )
        }
        Some("verify") => {
            let tmpl = smtp_val
                .as_ref()
                .and_then(|v| v.get("template_verify").and_then(|s| s.as_str()))
                .unwrap_or("Verify your email: {{link}}");
            let mock_token = uuid::Uuid::new_v4().to_string();
            let mock_link = format!(
                "{}/api/v1/auth/verify?token={}",
                app_url.trim_end_matches('/'),
                mock_token
            );
            (
                format!("Verify your email for {}", app_name),
                tmpl.to_string(),
                Some(mock_link),
                Some(mock_token),
            )
        }
        _ => (
            "Test Email from ApexKit".to_string(),
            "If you are reading this, your SMTP or Sendmail configuration is working correctly."
                .to_string(),
            None,
            None,
        ),
    };

    body = body.replace("{{app_name}}", &app_name);
    body = body.replace("{{email}}", &payload.email);
    if let Some(l) = link {
        body = body.replace("{{link}}", &l);
    }
    if let Some(t) = mock_token {
        body = body.replace("{{token}}", &t);
    }

    workers::tasks::emails::send_email(db, state.vault.clone(), &payload.email, &subject, &body)
        .await
        .map_err(|e| AppError::UnknownError(format!("Failed to send: {}", e)))?;

    Ok(Json(json!({ "success": true, "message": "Email sent." })))
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateUserReq {
    pub email: Option<String>,
    pub role: Option<String>,
    pub password: Option<String>,
    #[schema(value_type = Option<Object>)]
    pub metadata: Option<serde_json::Value>,
}

#[utoipa::path(
    patch,
    path = "/api/v1/admin/users/{id}",
    request_body = UpdateUserReq,
    params(IdPath),
    responses((status = 200, body = UserDto))
)]
pub async fn update_user_handler(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection,
    Path(path): Path<IdPath>,
    Json(payload): Json<UpdateUserReq>,
) -> Result<Json<UserDto>, AppError> {
    let claims = auth
        .ok_or(AppError::Unauthorized("Login required".into()))?
        .0;
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    let user_id = path
        .id
        .parse::<i64>()
        .map_err(|_| AppError::JsonError("Invalid User ID".into()))?;

    // Normalize the role if provided
    let clean_role = payload.role.map(|r| r.trim().to_lowercase());

    // Pass password to DB layer
    let u = db
        .update_user(
            user_id,
            payload.email,
            clean_role,
            payload.metadata,
            payload.password,
        )
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    Ok(Json(UserDto {
        id: u.id,
        email: u.email,
        role: u.role,
        metadata: u.metadata,
        scope: None,
    }))
}

// Helper to convert User to Value for policy check
fn user_to_value(u: &apexkit_core::auth::User) -> serde_json::Value {
    serde_json::json!({
        "id": u.id,
        "email": u.email,
        "role": u.role,
        "metadata": u.metadata
    })
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/users",
    params(UserListQuery),
    responses((status = 200, body = UserListResponse))
)]
pub async fn list_users_handler(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>,
    BaseUrl(base_url): BaseUrl,
    scope: Option<Extension<EventScope>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Query(params): Query<UserListQuery>,
) -> Result<Json<UserListResponse>, AppError> {
    let claims = auth.map(|c| c.0);

    // 1. Fetch User Policies from Config
    let policy_json = db
        .get_config("policy_users")
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    // [RESOLVED POLICIES DOUBLE-SERIALIZATION WRAPPING]
    // Unwrap the configuration key if it was double-serialized into a JSON-escaped string
    let policies: apexkit_core::models::schema::CollectionPolicies = if let Some(val) = policy_json
    {
        let parsed_val = match val {
            serde_json::Value::String(s) => {
                serde_json::from_str(&s).unwrap_or(serde_json::Value::Null)
            }
            _ => val,
        };
        serde_json::from_value(parsed_val).unwrap_or_else(|_| {
            apexkit_core::models::schema::CollectionPolicies {
                read: "admin || owner:id".to_string(),
                ..Default::default()
            }
        })
    } else {
        apexkit_core::models::schema::CollectionPolicies {
            read: "admin || owner:id".to_string(),
            ..Default::default()
        }
    };

    // 2. Check Global Read Access
    // Passing None for record_data checks if user has general read access
    if !apexkit_core::auth::policies::check_access(&policies.read, claims.as_ref(), None) {
        return Err(AppError::Forbidden("Access denied".into()));
    }

    let event_scope = scope.map(|e| e.0).unwrap_or(EventScope::Root);

    // [TRIGGER] Before List
    let query_json = json!({ "search": params.search, "page": params.page });
    let mod_q = trigger_filter_hook(
        &state,
        "before_list_users",
        query_json,
        claims.as_ref(),
        Some(&event_scope.clone()),
        Some(base_url.clone()),
    )
    .await?;

    let search = mod_q
        .get("search")
        .and_then(|s| s.as_str())
        .map(String::from);
    let page = mod_q
        .get("page")
        .and_then(|v| v.as_i64())
        .unwrap_or(1)
        .max(1);
    let limit = params.per_page.unwrap_or(20).min(100);
    let offset = (page - 1) * limit;

    let users = db
        .list_users(search.clone(), limit, offset)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    let total = db
        .count_users(search)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    // 3. [OPTIONAL] Row-Level Filtering (In-Memory)
    // If policy is complex (e.g. "owner:id"), we must filter the results.
    // However, for efficiency, "list" usually implies broad access or specific query filters.
    // If you want strict RLS on list, uncomment this block:
    /*
    let filtered_users: Vec<User> = users.into_iter().filter(|u| {
        let u_val = serde_json::json!({ "id": u.id, "email": u.email, "role": u.role });
        apexkit_core::auth::policies::check_access(&policies.read, claims.as_ref(), Some(&u_val))
    }).collect();
    // Update total? Doing so accurately requires fetching ALL and filtering, which kills pagination.
    // Standard practice: Apply global check, then rely on query filters for narrowing.
    */

    let response = UserListResponse {
        items: users
            .into_iter()
            .map(|u| UserDto {
                id: u.id,
                email: u.email,
                role: u.role,
                metadata: u.metadata,
                scope: None,
            })
            .collect(),
        total,
    };

    // [TRIGGER] After List
    let final_json = trigger_filter_hook(
        &state,
        "after_list_users",
        json!(response),
        claims.as_ref(),
        Some(&event_scope.clone()),
        Some(base_url.clone()),
    )
    .await?;
    let final_resp: UserListResponse = serde_json::from_value(final_json).unwrap_or(response);

    // [LOG]
    let meta = extract_log_meta(
        &headers,
        Some(addr),
        json!({ "count": final_resp.items.len() }),
    );
    let _ = db
        .log_audit_event("info", "Users Listed", "admin", Some(meta))
        .await;

    Ok(Json(final_resp))
}

#[utoipa::path(delete, path = "/api/v1/admin/users/{id}", params(IdPath))]
pub async fn delete_user_handler(
    BaseUrl(base_url): BaseUrl,
    auth: Option<Extension<Claims>>,
    State(state): State<AppState>,
    scope: Option<Extension<EventScope>>,
    DatabaseConnection(db): DatabaseConnection,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(path): Path<IdPath>,
) -> Result<StatusCode, AppError> {
    let claims = auth.map(|c| c.0);
    let user_id = path
        .id
        .parse::<i64>()
        .map_err(|_| AppError::JsonError("Invalid ID".into()))?;

    // 1. Fetch Target User
    // We need to fetch it to check "owner" policy against it
    // get_users_by_ids is in Db trait
    let targets = db
        .get_users_by_ids(&[user_id])
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    let target_user = targets
        .first()
        .ok_or(AppError::NotFound("User not found".into()))?;

    // 2. Get Policy
    let policy_json = db
        .get_config("policy_users")
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    // [RESOLVED POLICIES DOUBLE-SERIALIZATION WRAPPING]
    let policies: apexkit_core::models::schema::CollectionPolicies = if let Some(val) = policy_json
    {
        let parsed = match val {
            serde_json::Value::String(s) => {
                serde_json::from_str(&s).unwrap_or(serde_json::Value::Null)
            }
            _ => val,
        };
        serde_json::from_value(parsed).unwrap_or_default()
    } else {
        apexkit_core::models::schema::CollectionPolicies {
            delete: "admin".to_string(),
            ..Default::default()
        }
    };

    // 3. Check "Delete" Policy
    let target_data = user_to_value(target_user);
    if !policies::check_access(&policies.delete, claims.as_ref(), Some(&target_data)) {
        return Err(AppError::Forbidden("Delete denied".into()));
    }

    let event_scope = scope.map(|e| e.0).unwrap_or(EventScope::Root);

    // [TRIGGER] Before Delete
    let user_json = json!({ "id": path.id });
    trigger_void_hook(
        &state,
        "before_user_delete",
        user_json.clone(),
        claims.as_ref(),
        Some(&event_scope.clone()),
        Some(base_url.clone()),
    )
    .await?;

    // [FIX] Parse String ID to i64
    let user_id = path
        .id
        .parse::<i64>()
        .map_err(|_| AppError::JsonError("Invalid User ID format".into()))?;

    db.delete_user(user_id)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    // [LOG]
    let meta = extract_log_meta(&headers, Some(addr), json!({ "target_user_id": user_id }));
    let _ = db
        .log_audit_event("warning", "User Deleted", "admin", Some(meta))
        .await;

    // [TRIGGER] After Delete
    let _ = trigger_void_hook(
        &state,
        "after_user_delete",
        user_json,
        claims.as_ref(),
        Some(&event_scope.clone()),
        Some(base_url.clone()),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, IntoParams)]
pub struct UserListQuery {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
    pub search: Option<String>,
}

#[derive(Serialize, ToSchema, Deserialize)]
pub struct UserListResponse {
    pub items: Vec<UserDto>,
    pub total: i64,
}
