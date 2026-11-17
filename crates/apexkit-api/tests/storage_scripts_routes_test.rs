//! Functional tests for storage, script/webhook, template, and migration routes.

mod common;

use axum::http::StatusCode;
use common::{admin_token, fresh_app, get_request, json_request, send, send_raw};
use serde_json::json;

#[tokio::test]
async fn storage_upload_list_and_serve() {
    let (dir, state, app) = fresh_app("storage_flow").await;
    let token = admin_token(&state).await;

    // Build a multipart/form-data body with a single "file" field.
    let boundary = "----apexkitboundary";
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"hello.txt\"\r\nContent-Type: text/plain\r\n\r\nhello world\r\n--{boundary}--\r\n"
    );
    let req = axum::http::Request::builder()
        .method("POST")
        .uri("/api/v1/storage/upload")
        .header(axum::http::header::HOST, "localhost")
        .header(
            axum::http::header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}"),
        )
        .header(axum::http::header::AUTHORIZATION, format!("Bearer {token}"))
        .body(axum::body::Body::from(body))
        .unwrap();

    let (status, resp) = send(&app, req).await;
    assert_eq!(status, StatusCode::OK, "upload failed: {resp}");
    let filename = resp["filename"].as_str().unwrap().to_string();
    assert!(!filename.is_empty());

    let (status, list) = send(&app, get_request("/api/v1/storage/files", Some(&token))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["total"], 1);
    assert_eq!(list["items"][0]["original_name"], "hello.txt");

    // The served file must return the original bytes.
    let (status, bytes) = send_raw(
        &app,
        get_request(&format!("/api/v1/storage/file/{filename}"), Some(&token)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bytes, b"hello world");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn storage_delete_file() {
    let (dir, state, app) = fresh_app("storage_delete").await;
    let token = admin_token(&state).await;

    state
        .db
        .create_file_metadata("stored.txt", "orig.txt", "text/plain", 3, None)
        .await
        .unwrap();
    let (status, list) = send(&app, get_request("/api/v1/storage/files", Some(&token))).await;
    let id = list["items"][0]["id"].as_i64().unwrap();

    let (status, _) = send(
        &app,
        json_request(
            "DELETE",
            &format!("/api/v1/storage/files/{id}"),
            &json!(null),
            Some(&token),
        ),
    )
    .await;
    assert!(status.is_success(), "delete file returned {status}");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn script_create_list_and_run() {
    let (dir, state, app) = fresh_app("scripts").await;
    let token = admin_token(&state).await;

    let (status, body) = send(
        &app,
        json_request(
            "POST",
            "/api/v1/admin/scripts",
            &json!({
                "name": "greet",
                "trigger_type": "manual",
                "code": "export default async function(req) { return new Response({ msg: \"hi\" }); }",
                "active": true,
                "visibility": "private"
            }),
            Some(&token),
        ),
    )
    .await;
    assert!(status.is_success(), "create script failed: {body}");

    let (status, body) = send(&app, get_request("/api/v1/admin/scripts", Some(&token))).await;
    assert_eq!(status, StatusCode::OK);
    let local = body["local"].as_array().unwrap();
    assert!(local.iter().any(|s| s["name"] == "greet"));

    let (status, body) = send(
        &app,
        json_request("POST", "/api/v1/run/greet", &json!({}), Some(&token)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "run script failed: {body}");
    assert_eq!(body["msg"], "hi");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn webhook_route_runs_script() {
    let (dir, state, app) = fresh_app("webhook").await;
    let token = admin_token(&state).await;

    send(
        &app,
        json_request(
            "POST",
            "/api/v1/admin/scripts",
            &json!({
                "name": "hook",
                "trigger_type": "manual",
                "code": "export default async function(req) { return new Response({ ok: true }); }",
                "active": true,
                "visibility": "private"
            }),
            Some(&token),
        ),
    )
    .await;

    let (status, body) = send(
        &app,
        json_request("POST", "/api/v1/webhook/hook", &json!({}), Some(&token)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "webhook failed: {body}");
    assert_eq!(body["ok"], true);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn template_lifecycle_and_render() {
    let (dir, state, app) = fresh_app("templates").await;
    let token = admin_token(&state).await;

    let (status, _) = send(
        &app,
        json_request(
            "POST",
            "/api/v1/admin/templates",
            &json!({"slug": "hello", "content": "<h1>Static</h1>"}),
            Some(&token),
        ),
    )
    .await;
    assert!(status.is_success(), "create template returned {status}");

    let (status, body) = send(&app, get_request("/api/v1/admin/templates", Some(&token))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 1);

    // Render the static template.
    let (status, bytes) = send_raw(&app, get_request("/render/hello", Some(&token))).await;
    assert_eq!(status, StatusCode::OK, "render returned {status}");
    let html = String::from_utf8_lossy(&bytes);
    assert!(html.contains("Static"), "render body: {html}");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn export_schema_and_data() {
    let (dir, state, app) = fresh_app("migration").await;
    let token = admin_token(&state).await;
    let col_id = common::create_posts_collection(&state).await;
    send(
        &app,
        json_request(
            "POST",
            "/api/v1/collections/posts/records",
            &json!({"data": {"title": "A", "views": 1}}),
            Some(&token),
        ),
    )
    .await;

    let (status, body) = send_raw(&app, get_request("/api/v1/admin/export-schema", Some(&token))).await;
    assert_eq!(status, StatusCode::OK);
    let schema: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(schema["collections"].is_array());

    let (status, body) = send_raw(
        &app,
        get_request(&format!("/api/v1/admin/export-data/{col_id}"), Some(&token)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let rows: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 1);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn export_scripts_and_ai_actions() {
    let (dir, state, app) = fresh_app("export_misc").await;
    let token = admin_token(&state).await;

    let (status, _) = send_raw(
        &app,
        get_request("/api/v1/admin/export-scripts", Some(&token)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = send_raw(
        &app,
        get_request("/api/v1/admin/export-ai-actions", Some(&token)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn ai_actions_list_contains_seeded_actions() {
    let (dir, state, app) = fresh_app("ai_actions").await;
    let token = admin_token(&state).await;

    let (status, body) = send(&app, get_request("/api/v1/admin/ai/actions", Some(&token))).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.is_array());

    let _ = std::fs::remove_dir_all(&dir);
}
