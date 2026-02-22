use async_trait::async_trait;
use std::path::PathBuf;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use aws_sdk_s3::primitives::ByteStream;
use aws_config::BehaviorVersion; 
use aws_credential_types::Credentials; 

// [FIX] Correct imports for Custom HTTP Client and Response Building
use aws_smithy_runtime_api::client::http::{HttpConnector, HttpConnectorFuture, HttpClient, SharedHttpClient, SharedHttpConnector};
use aws_smithy_runtime_api::client::orchestrator::{HttpRequest, HttpResponse};
use aws_smithy_types::body::SdkBody;
use http::{Response, HeaderName, HeaderValue}; // Standard http crate response
use reqwest::Method;
use std::str::FromStr;

// --- Trait Definition ---
#[async_trait]
pub trait StorageBackend: Send + Sync {
    async fn save(&self, filename: &str, data: &[u8], content_type: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>>;
    async fn get(&self, filename: &str) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>>;
    async fn delete(&self, filename: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn list_prefix(&self, prefix: &str) -> Result<Vec<(String, u64, String)>, Box<dyn std::error::Error + Send + Sync>>; 
    fn get_public_url_base(&self) -> String;
    async fn get_signed_url(&self, filename: &str, expires_in_secs: u64) -> Result<String, Box<dyn std::error::Error + Send + Sync>>;
}

// --- Custom HTTP Client Adapter (Reqwest -> AWS Smithy) ---
#[derive(Clone, Debug)]
struct ReqwestHttpClient {
    client: reqwest::Client,
}

impl ReqwestHttpClient {
    pub fn new() -> Self {
        Self { client: reqwest::Client::new() }
    }
}

// [FIX] Implement HttpConnector (Low-level)
impl HttpConnector for ReqwestHttpClient {
    fn call(&self, request: HttpRequest) -> HttpConnectorFuture {
        let client = self.client.clone();
        
        let uri = request.uri().to_string();
        let method_str = request.method().to_string();
        
        let mut headers_vec = Vec::new();
        for (k, v) in request.headers() {
            headers_vec.push((k.to_string(), v.to_string()));
        }

        let body_bytes = match request.body().bytes() {
            Some(b) => Some(b.to_vec()),
            None => None,
        };

        HttpConnectorFuture::new(async move {
            use aws_smithy_runtime_api::client::result::ConnectorError;

            let method = Method::from_bytes(method_str.as_bytes())
                .map_err(|e| ConnectorError::user(Box::new(e)))?;
            
            let mut reqwest_request = client.request(method, &uri);
            
            for (k, v) in headers_vec {
                reqwest_request = reqwest_request.header(k, v);
            }
            
            if let Some(bytes) = body_bytes {
                reqwest_request = reqwest_request.body(bytes);
            }

            let response = reqwest_request.send().await
                .map_err(|e| ConnectorError::io(Box::new(e)))?;
            
            // [FIX] Convert status to u16
            let status_code = response.status().as_u16();

            let mut response_headers = Vec::new();
            for (k, v) in response.headers() {
                if let Ok(s) = v.to_str() {
                    response_headers.push((k.to_string(), s.to_string()));
                }
            }

            let bytes = response.bytes().await
                .map_err(|e| ConnectorError::io(Box::new(e)))?;
                
            let mut builder = Response::builder().status(status_code);
            
            for (k, v) in response_headers {
                if let Some(headers) = builder.headers_mut() {
                     // [FIX] Explicitly parse header name and value
                     if let Ok(name) = HeaderName::from_str(&k) {
                         if let Ok(val) = HeaderValue::from_str(&v) {
                             headers.insert(name, val);
                         }
                     }
                }
            }
            
            let sdk_body = SdkBody::from(bytes);
            
            let response = builder.body(sdk_body)
                .map_err(|e| ConnectorError::other(Box::new(e), None))?;
            
            Ok(HttpResponse::try_from(response).map_err(|e| ConnectorError::other(Box::new(e), None))?)
        })
    }
}

// [FIX] Implement HttpClient (High-level Marker Trait)
impl HttpClient for ReqwestHttpClient {
    fn http_connector(&self, _: &aws_smithy_runtime_api::client::http::HttpConnectorSettings, _: &aws_smithy_runtime_api::client::runtime_components::RuntimeComponents) -> SharedHttpConnector {
        SharedHttpConnector::new(self.clone())
    }
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

    async fn list_prefix(&self, _prefix: &str) -> Result<Vec<(String, u64, String)>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(vec![])
    }

    fn get_public_url_base(&self) -> String {
        self.base_url.clone()
    }

    async fn get_signed_url(&self, filename: &str, _expires_in_secs: u64) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Ok(format!("{}{}", self.base_url, filename))
    }
}

// --- AWS S3 / R2 Implementation ---

pub struct S3Storage {
    client: aws_sdk_s3::Client,
    bucket: String,
    public_url: String,
}

impl S3Storage {
    pub async fn new_with_creds(
        bucket: &str, 
        region: &str, 
        endpoint: &str, 
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

        let reg = if region.is_empty() { "auto".to_string() } else { region.to_string() };

        // [FIX] Use SharedHttpClient wrapper around our Reqwest adapter which now implements HttpClient
        let http_client = SharedHttpClient::new(ReqwestHttpClient::new());

        let shared_config = aws_config::defaults(BehaviorVersion::latest())
            .region(aws_config::Region::new(reg))
            .credentials_provider(creds)
            .http_client(http_client) // Now satisfied
            .load()
            .await;
            
        let mut s3_config_builder = aws_sdk_s3::config::Builder::from(&shared_config);
        
        if !endpoint.is_empty() {
            let final_endpoint = if !endpoint.contains("://") {
                format!("https://{}", endpoint)
            } else {
                endpoint.to_string()
            };

            s3_config_builder = s3_config_builder
                .endpoint_url(final_endpoint)
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
            .content_type(content_type)
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

    async fn list_prefix(&self, prefix: &str) -> Result<Vec<(String, u64, String)>, Box<dyn std::error::Error + Send + Sync>> {
        let resp = self.client.list_objects_v2()
            .bucket(&self.bucket)
            .prefix(prefix)
            .send()
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
            
        let mut results = Vec::new();
        for obj in resp.contents.unwrap_or_default() {
            let key = obj.key.unwrap_or_default();
            let size = obj.size.unwrap_or(0) as u64;
            let time = obj.last_modified.map(|t| t.to_string()).unwrap_or_default();
            results.push((key, size, time));
        }
        Ok(results)
    }

    fn get_public_url_base(&self) -> String {
        self.public_url.clone()
    }

    async fn get_signed_url(&self, filename: &str, expires_in_secs: u64) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let expiry = std::time::Duration::from_secs(expires_in_secs);
        
        let presigning_config = aws_sdk_s3::presigning::PresigningConfig::expires_in(expiry)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        let presigned_req = self.client.get_object()
            .bucket(&self.bucket)
            .key(filename)
            .presigned(presigning_config)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        Ok(presigned_req.uri().to_string())
    }
}