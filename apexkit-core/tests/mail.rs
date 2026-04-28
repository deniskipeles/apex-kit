use std::sync::Arc;
use apexkit_core::jobs;
use apexkit_core::security::{MasterKey, Vault};
use apexkit_core::VectorProvider;
use apexkit_core::Db;
use tempfile::tempdir;

struct MockVectorProvider;
#[async_trait::async_trait]
impl VectorProvider for MockVectorProvider {
    async fn embed(&self, _text: &str) -> Result<Vec<f32>, String> { Ok(vec![0.0; 384]) }
    async fn embed_image(&self, _b64: &str) -> Result<Vec<f32>, String> { Ok(vec![0.0; 384]) }
    async fn search(&self, _c: i64, _f: &str, _v: &[f32], _l: usize) -> Result<Vec<(i64, f32)>, String> { Ok(vec![]) }
    async fn index(&self, _c: i64, _r: i64, _f: &str, _v: &[f32]) -> Result<(), String> { Ok(()) }
}

#[tokio::test]
async fn test_send_email_file_transport() {
    let base_dir = tempdir().unwrap();
    let base_path = base_dir.path().to_str().unwrap();
    let mail_dir = tempdir().unwrap();
    let mail_path = mail_dir.path().to_str().unwrap();

    let vector_provider: Arc<dyn VectorProvider> = Arc::new(MockVectorProvider);
    let db = Arc::new(apexkit_core::ApexKit::init_filesystem(base_path, vector_provider.clone(), None, None, "root".to_string()).await.unwrap());

    // Configure File Transport
    let smtp_config = serde_json::json!({
        "enabled": true,
        "host": "localhost",
        "port": 1025,
        "from_email": "test@apexkit.io",
        "file_path": mail_path
    });
    db.set_config("smtp", &smtp_config, false).await.unwrap();

    let master_key = MasterKey::from_string("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".to_string()).unwrap();
    let vault = Arc::new(Vault::new(&master_key));

    let result = jobs::send_email(db, vault, "recipient@example.com", "Test Subject", "Test Body").await;
    assert!(result.is_ok(), "Failed to send email: {:?}", result.err());

    // Verify file exists in mail_path
    let entries = std::fs::read_dir(mail_path).unwrap();
    let files: Vec<_> = entries.map(|e| e.unwrap().path()).collect();
    assert!(!files.is_empty(), "No email file generated");

    let content = std::fs::read_to_string(&files[0]).unwrap();
    assert!(content.contains("To: recipient@example.com"));
    assert!(content.contains("Subject: Test Subject"));
    assert!(content.contains("Test Body"));
}

#[tokio::test]
async fn test_send_email_invalid_recipient() {
    let base_dir = tempdir().unwrap();
    let base_path = base_dir.path().to_str().unwrap();

    let vector_provider: Arc<dyn VectorProvider> = Arc::new(MockVectorProvider);
    let db = Arc::new(apexkit_core::ApexKit::init_filesystem(base_path, vector_provider.clone(), None, None, "root".to_string()).await.unwrap());

    let master_key = MasterKey::from_string("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".to_string()).unwrap();
    let vault = Arc::new(Vault::new(&master_key));

    let result = jobs::send_email(db, vault, "invalid-email", "Subject", "Body").await;
    assert!(result.is_err());
}
