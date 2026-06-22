use async_trait::async_trait;

#[async_trait]
pub trait StorageBackend: Send + Sync {
    async fn save(
        &self,
        filename: &str,
        data: &[u8],
        content_type: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>>;
    async fn get(
        &self,
        filename: &str,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>>;
    async fn delete(&self, filename: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn list_prefix(
        &self,
        prefix: &str,
    ) -> Result<Vec<(String, u64, String)>, Box<dyn std::error::Error + Send + Sync>>;
    fn get_public_url_base(&self) -> String;
    async fn get_signed_url(
        &self,
        filename: &str,
        expires_in_secs: u64,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>>;
}
