use async_trait::async_trait;
use std::path::PathBuf;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use aws_sdk_s3::primitives::ByteStream;
use aws_config::BehaviorVersion; 
use aws_credential_types::Credentials; 

#[async_trait]
pub trait StorageBackend: Send + Sync {
    async fn save(&self, filename: &str, data: &[u8], content_type: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>>;
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
    async fn save(&self, filename: &str, data: &[u8], _content_type: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
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

// --- AWS S3 / R2 Implementation ---

pub struct S3Storage {
    client: aws_sdk_s3::Client,
    bucket: String,
    public_url: String,
}

impl S3Storage {
    // UPDATED: Now accepts 'endpoint' and applies it to the config
    pub async fn new_with_creds(
        bucket: &str, 
        region: &str, 
        endpoint: &str, // <--- Make sure this is passed correctly
        public_url_base: &str, 
        access_key: &str, 
        secret_key: &str
    ) -> Self {
        
        let creds = Credentials::new(
            access_key.to_string(),
            secret_key.to_string(),
            None,
            None,
            "apexkit"
        );

        let shared_config = aws_config::defaults(BehaviorVersion::latest())
            .region(aws_config::Region::new(region.to_string()))
            .credentials_provider(creds)
            .load()
            .await;
            
        // --- CRITICAL FIX FOR R2/MINIO ---
        // We must build a specific S3 config that overrides the endpoint URL.
        // Without this, the SDK defaults to aws.amazon.com and fails auth/connection.
        let mut s3_config_builder = aws_sdk_s3::config::Builder::from(&shared_config);
        
        if !endpoint.is_empty() {
            s3_config_builder = s3_config_builder
                .endpoint_url(endpoint)
                // Force Path Style is often required for R2/MinIO/Localstack
                // i.e. https://endpoint/bucket/file instead of https://bucket.endpoint/file
                .force_path_style(true); 
        }

        let client = aws_sdk_s3::Client::from_conf(s3_config_builder.build());
        
        Self {
            client,
            bucket: bucket.to_string(),
            public_url: public_url_base.to_string(),
        }
    }
}

#[async_trait]
impl StorageBackend for S3Storage {
    async fn save(&self, filename: &str, data: &[u8], content_type: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        self.client.put_object()
            .bucket(&self.bucket)
            .key(filename)
            .body(ByteStream::from(data.to_vec()))
            .content_type(content_type) // <--- CRITICAL FIX
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