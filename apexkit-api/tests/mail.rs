mod common;
use common::*;
use axum::http::StatusCode;
use serde_json::json;
use tower::ServiceExt;
use tempfile::tempdir;
use apexkit_api::app_router;

#[tokio::test]
async fn test_admin_smtp_test_endpoint_file() {
    let ctx = setup_test_context().await;
    let app = app_router(ctx.state.clone());

    let mail_dir = tempdir().unwrap();
    let mail_path = mail_dir.path().to_str().unwrap();

    // 1. Configure File Transport in DB
    let smtp_config = json!({
        "enabled": true,
        "host": "localhost",
        "port": 1025,
        "from_email": "admin@apexkit.io",
        "file_path": mail_path
    });
    ctx.state.db.set_config("smtp", &smtp_config, false).await.unwrap();

    // 2. Call the test email endpoint
    let response = app
        .oneshot(
            base_request()
                .method("POST")
                .uri("/api/v1/admin/smtp/test")
                .body(json!({ "email": "narydjin@gmail.com" }).to_string())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // 3. Verify file exists in mail_path
    let entries = std::fs::read_dir(mail_path).unwrap();
    let files: Vec<_> = entries.map(|e| e.unwrap().path()).collect();
    assert!(!files.is_empty(), "No email file generated");

    let content = std::fs::read_to_string(&files[0]).unwrap();
    assert!(content.contains("To: narydjin@gmail.com"));
}

#[tokio::test]
async fn test_welcome_email_job_file() {
    let ctx = setup_test_context().await;

    let mail_dir = tempdir().unwrap();
    let mail_path = mail_dir.path().to_str().unwrap();

    // Configure File Transport
    let smtp_config = json!({
        "enabled": true,
        "host": "localhost",
        "port": 1025,
        "from_email": "welcome@apexkit.io",
        "file_path": mail_path
    });
    ctx.state.db.set_config("smtp", &smtp_config, false).await.unwrap();

    // Trigger welcome email job manually
    use apexkit_core::jobs::Job;
    ctx.state.queue.enqueue(Job::SendWelcomeEmail {
        email: "narydjin@gmail.com".to_string(),
        user_id: 1
    }).await;

    // Polling for file creation
    let mut found = false;
    for _ in 0..20 {
        let entries = std::fs::read_dir(mail_path).unwrap();
        if entries.count() > 0 {
            found = true;
            break;
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    assert!(found, "Email file was not created by background job");

    // Verify content
    let entries = std::fs::read_dir(mail_path).unwrap();
    let files: Vec<_> = entries.map(|e| e.unwrap().path()).collect();
    let content = std::fs::read_to_string(&files[0]).unwrap();
    assert!(content.contains("To: narydjin@gmail.com"));
    assert!(content.contains("Welcome to ApexKit!"));
}

#[tokio::test]
async fn test_smtp_with_password() {
    let ctx = setup_test_context().await;
    let app = app_router(ctx.state.clone());

    let mail_dir = tempdir().unwrap();
    let mail_path = mail_dir.path().to_str().unwrap();

    // 1. Encrypt a password
    let password = "test-password";
    let encrypted = ctx.state.vault.encrypt(password).unwrap();
    let encrypted_json = serde_json::to_string(&encrypted).unwrap();

    // 2. Configure SMTP with encrypted password AND file_path
    let smtp_config = json!({
        "enabled": true,
        "host": "localhost",
        "port": 1025,
        "from_email": "admin@apexkit.io",
        "username": "test-user",
        "password": encrypted_json,
        "file_path": mail_path
    });
    ctx.state.db.set_config("smtp", &smtp_config, false).await.unwrap();

    // 3. Call the test email endpoint
    let response = app
        .oneshot(
            base_request()
                .method("POST")
                .uri("/api/v1/admin/smtp/test")
                .body(json!({ "email": "narydjin@gmail.com" }).to_string())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // 4. Verify in file
    let entries = std::fs::read_dir(mail_path).unwrap();
    let files: Vec<_> = entries.map(|e| e.unwrap().path()).collect();
    let content = std::fs::read_to_string(&files[0]).unwrap();
    assert!(content.contains("To: narydjin@gmail.com"));
}

#[tokio::test]
async fn test_invalid_email_error() {
    let ctx = setup_test_context().await;
    let app = app_router(ctx.state.clone());

    // Call the test email endpoint with invalid email
    let response = app
        .oneshot(
            base_request()
                .method("POST")
                .uri("/api/v1/admin/smtp/test")
                .body(json!({ "email": "invalid-email" }).to_string())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn test_sendmail_fallback_logic() {
    let ctx = setup_test_context().await;
    let app = app_router(ctx.state.clone());

    // Ensure SMTP is disabled and NO file_path is set
    let smtp_config = json!({
        "enabled": false,
        "from_email": "sendmail@apexkit.io"
    });
    ctx.state.db.set_config("smtp", &smtp_config, false).await.unwrap();

    // Call the test email endpoint
    // It should try to use sendmail and fail because it's not installed in the environment
    let response = app
        .oneshot(
            base_request()
                .method("POST")
                .uri("/api/v1/admin/smtp/test")
                .body(json!({ "email": "narydjin@gmail.com" }).to_string())
                .unwrap(),
        )
        .await
        .unwrap();

    // lettre SendmailTransport::new().send(&email) returns an error if sendmail is not found
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}
