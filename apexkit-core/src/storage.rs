use async_trait::async_trait;
use std::path::PathBuf;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use aws_sdk_s3::primitives::ByteStream;
use aws_config::BehaviorVersion; 
use aws_credential_types::Credentials; 

#[async_trait]
pub trait StorageBackend: Send + Sync {
    async fn save(&self, filename: &str, data: &[u8]) -> Result<String, Box<dyn std::error::Error + Send + Sync>>;
    async fn get(&self, filename: &str) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>>;
    async fn delete(&self, filename: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    fn get_public_url_base(&self) -> String;
}

// --- Local Storage Implementation ---

pub struct LocalStorage {
    pub base_path: PathBuf,
    pub base_url: String,
}

impl LocalStorage {
    pub async fn new(path: &str, base_url: &str) -> Self {
        if let Err(e) = fs::create_dir_all(path).await {
            eprintln!("Warning: Could not create upload directory: {}", e);
        }
        Self {
            base_path: PathBuf::from(path),
            base_url: base_url.to_string(),
        }
    }
}

#[async_trait]
impl StorageBackend for LocalStorage {
    async fn save(&self, filename: &str, data: &[u8]) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let file_path = self.base_path.join(filename);
        let mut file = fs::File::create(file_path).await?;
        file.write_all(data).await?;
        Ok(filename.to_string())
    }

    async fn get(&self, filename: &str) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        let file_path = self.base_path.join(filename);
        let data = fs::read(file_path).await?;
        Ok(data)
    }

    async fn delete(&self, filename: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let file_path = self.base_path.join(filename);
        if file_path.exists() {
            fs::remove_file(file_path).await?;
        }
        Ok(())
    }

    fn get_public_url_base(&self) -> String {
        self.base_url.clone()
    }
}

// --- AWS S3 Implementation ---

pub struct S3Storage {
    client: aws_sdk_s3::Client,
    bucket: String,
    public_url: String,
}

impl S3Storage {
    // UPDATED CONSTRUCTOR to take explicit credentials
    pub async fn new_with_creds(bucket: &str, region: &str, public_url_base: &str, access_key: &str, secret_key: &str) -> Self {
        
        let creds = Credentials::new(
            access_key.to_string(),
            secret_key.to_string(),
            None,
            None,
            "apexkit"
        );

        let config = aws_config::defaults(BehaviorVersion::latest())
            .region(aws_config::Region::new(region.to_string()))
            .credentials_provider(creds)
            .load()
            .await;
            
        // If endpoint is custom (e.g. MinIO), we need to adjust config, 
        // but for simplicity with standard aws_config:
        let client = aws_sdk_s3::Client::new(&config);
        
        // Note: For MinIO/DigitalOcean, you might need to build config manually using aws_sdk_s3::Config::builder().endpoint_url(...)
        // But let's stick to standard AWS behavior for now or assume public_url_base implies endpoint logic if expanded later.
        
        Self {
            client,
            bucket: bucket.to_string(),
            public_url: public_url_base.to_string(),
        }
    }
}

#[async_trait]
impl StorageBackend for S3Storage {
    async fn save(&self, filename: &str, data: &[u8]) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        self.client.put_object()
            .bucket(&self.bucket)
            .key(filename)
            .body(ByteStream::from(data.to_vec()))
            .send()
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
            
        Ok(filename.to_string())
    }

    async fn get(&self, filename: &str) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        let resp = self.client.get_object()
            .bucket(&self.bucket)
            .key(filename)
            .send()
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        let data = resp.body.collect().await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?
            .into_bytes();
            
        Ok(data.to_vec())
    }

    async fn delete(&self, filename: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.client.delete_object()
            .bucket(&self.bucket)
            .key(filename)
            .send()
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
        Ok(())
    }

    fn get_public_url_base(&self) -> String {
        self.public_url.clone()
    }
}