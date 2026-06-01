use axum::{
    body::{Body, to_bytes},
    http::StatusCode,
};
use tower::ServiceExt;

mod common;
use common::{admin_token, setup_test_app, test_request};

#[tokio::test]
async fn test_storage_lifecycle() {
    let app = setup_test_app().await;

    // 1. Upload File
    let boundary = "X-BOUNDARY";
    let body_content = format!(
        "--{boundary}\r\n\
        Content-Disposition: form-data; name=\"file\"; filename=\"test.txt\"\r\n\
        Content-Type: text/plain\r\n\r\n\
        Hello Storage!\r\n\
        --{boundary}--\r\n"
    );

    let response = app
        .clone()
        .oneshot(
            test_request()
                .method("POST")
                .uri("/api/v1/storage/upload")
                .header("authorization", format!("Bearer {}", admin_token()))
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={}", boundary),
                )
                .body(Body::from(body_content))
                .unwrap(),
        )
        .await
        .unwrap();

    let status = response.status();
    let body_bytes = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    assert_eq!(
        status,
        StatusCode::CREATED,
        "Upload failed with body: {:?}",
        String::from_utf8_lossy(&body_bytes)
    );

    let res: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    let filename = res["filename"]
        .as_str()
        .expect("filename should be present");
    let file_id = res["id"].as_i64().expect("id should be present");

    // 2. List Files
    let response = app
        .clone()
        .oneshot(
            test_request()
                .method("GET")
                .uri("/api/v1/storage/files")
                .header("authorization", format!("Bearer {}", admin_token()))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    let res: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(res["items"].as_array().unwrap().len() >= 1);

    // 3. Serve File
    let response = app
        .clone()
        .oneshot(
            test_request()
                .method("GET")
                .uri(format!("/api/v1/storage/file/{}", filename))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    assert_eq!(body, "Hello Storage!");

    // 4. Delete File
    let response = app
        .clone()
        .oneshot(
            test_request()
                .method("DELETE")
                .uri(format!("/api/v1/storage/files/{}", file_id))
                .header("authorization", format!("Bearer {}", admin_token()))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // 5. Verify Deletion
    let response = app
        .clone()
        .oneshot(
            test_request()
                .method("GET")
                .uri("/api/v1/storage/files")
                .header("authorization", format!("Bearer {}", admin_token()))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    let res: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(res["items"].as_array().unwrap().len(), 0);
}
