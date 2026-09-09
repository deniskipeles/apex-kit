use super::traits::StorageBackend;
use async_trait::async_trait;
use std::path::PathBuf;
use tokio::fs;
use tokio::io::AsyncWriteExt;

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
    async fn save(
        &self,
        filename: &str,
        data: &[u8],
        _content_type: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let file_path = self.base_path.join(filename);
        let mut file = fs::File::create(file_path).await?;
        file.write_all(data).await?;
        Ok(filename.to_string())
    }
    async fn get(
        &self,
        filename: &str,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(fs::read(self.base_path.join(filename)).await?)
    }
    async fn delete(&self, filename: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let file_path = self.base_path.join(filename);
        if file_path.exists() {
            fs::remove_file(file_path).await?;
        }
        Ok(())
    }
    async fn list_prefix(
        &self,
        prefix: &str,
    ) -> Result<Vec<(String, u64, String)>, Box<dyn std::error::Error + Send + Sync>> {
        let base_path = self.base_path.clone();
        let target_dir = base_path.join(prefix);

        let res = tokio::task::spawn_blocking(move || {
            let mut results = Vec::new();
            if !target_dir.exists() {
                return results;
            }

            for entry in walkdir::WalkDir::new(&target_dir).into_iter().flatten() {
                if entry.file_type().is_file() {
                    let path = entry.path();
                    if let Ok(rel_path) = path.strip_prefix(&base_path) {
                        let name = rel_path.to_string_lossy().replace("\\", "/");
                        let meta = entry.metadata().unwrap();
                        let size = meta.len();
                        let time = meta
                            .modified()
                            .ok()
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| d.as_secs().to_string())
                            .unwrap_or_default();
                        results.push((name, size, time));
                    }
                }
            }
            results
        })
        .await?;

        Ok(res)
    }
    fn get_public_url_base(&self) -> String {
        self.base_url.clone()
    }
    async fn get_signed_url(
        &self,
        filename: &str,
        _expires_in_secs: u64,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Ok(format!("{}{}", self.base_url, filename))
    }
}
