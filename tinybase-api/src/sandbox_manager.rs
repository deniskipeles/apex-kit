// =========================== /teamspace/studios/this_studio/tinybase/tinybase/tinybase-api/src/sandbox_manager.rs ===========================
use std::path::Path;
use std::fs;
use std::sync::Arc;
use tinybase_core::{Db, TinyBase, VectorProvider}; // Added VectorProvider trait
use libsql::Builder;
use tracing::{info};

pub struct SandboxManager;

// Placeholder provider for Sandbox environments
// In a real scenario, you might want to inject the real provider or a specific sandbox one.
struct SandboxVectorProvider;

#[async_trait::async_trait]
impl VectorProvider for SandboxVectorProvider {
    async fn embed(&self, _text: &str) -> Result<Vec<f32>, String> {
        Err("Vector embedding not available in sandbox".to_string())
    }
    async fn search(&self, _col_id: i64, _field: &str, _vec: &[f32], _limit: usize) -> Result<Vec<(i64, f32)>, String> {
        Ok(vec![])
    }
    async fn index(&self, _col_id: i64, _rec_id: i64, _field: &str, _vec: &[f32]) -> Result<(), String> {
        Ok(())
    }
}

impl SandboxManager {
    /// Ensures the sandboxes directory exists
    pub fn init() {
        let _ = fs::create_dir_all("sandboxes");
    }

    /// Creates a fresh sandbox by copying the main DB
    /// Returns the connection to the new sandbox
    pub async fn create_sandbox(session_id: &str) -> Result<Arc<dyn Db>, String> {
        let dbs = vec!["core", "data", "logs", "system", "vectors"];
        let sandbox_dir = format!("sandboxes/session_{}", session_id);
        fs::create_dir_all(&sandbox_dir).map_err(|e| e.to_string())?;

        for db_name in &dbs {
            let prod_path = format!("{}.db", db_name);
            let target_path = format!("{}/{}.db", sandbox_dir, db_name);

            // Copy DB
            if Path::new(&prod_path).exists() {
                fs::copy(&prod_path, &target_path).map_err(|e| format!("Failed to clone {}: {}", db_name, e))?;
                // Copy WAL/SHM if they exist
                let _ = fs::copy(format!("{}-wal", prod_path), format!("{}-wal", target_path));
                let _ = fs::copy(format!("{}-shm", prod_path), format!("{}-shm", target_path));
            }
        }

        info!("Sandbox created at {}", sandbox_dir);

        // Connect to Sandbox DBs
        let core = Builder::new_local(&format!("{}/core.db", sandbox_dir)).build().await.map_err(|e| e.to_string())?;
        let data = Builder::new_local(&format!("{}/data.db", sandbox_dir)).build().await.map_err(|e| e.to_string())?;
        let log = Builder::new_local(&format!("{}/logs.db", sandbox_dir)).build().await.map_err(|e| e.to_string())?;
        let sys = Builder::new_local(&format!("{}/system.db", sandbox_dir)).build().await.map_err(|e| e.to_string())?;
        let vec = Builder::new_local(&format!("{}/vectors.db", sandbox_dir)).build().await.map_err(|e| e.to_string())?;

        // FIX: Provide the 6th argument (VectorProvider)
        Ok(Arc::new(TinyBase::new(
            Arc::new(core), 
            Arc::new(data), 
            Arc::new(log), 
            Arc::new(sys),
            Arc::new(vec),
            Arc::new(SandboxVectorProvider)
        )))
    }

    /// Connects to an existing sandbox
    pub async fn get_sandbox(session_id: &str) -> Result<Arc<dyn Db>, String> {
        let sandbox_dir = format!("sandboxes/session_{}", session_id);
        
        if !Path::new(&sandbox_dir).exists() {
            return Err("Sandbox expired or not found".into());
        }

        let core = Builder::new_local(&format!("{}/core.db", sandbox_dir)).build().await.map_err(|e| e.to_string())?;
        let data = Builder::new_local(&format!("{}/data.db", sandbox_dir)).build().await.map_err(|e| e.to_string())?;
        let log = Builder::new_local(&format!("{}/logs.db", sandbox_dir)).build().await.map_err(|e| e.to_string())?;
        let sys = Builder::new_local(&format!("{}/system.db", sandbox_dir)).build().await.map_err(|e| e.to_string())?;
        let vec = Builder::new_local(&format!("{}/vectors.db", sandbox_dir)).build().await.map_err(|e| e.to_string())?;

        // FIX: Provide the 6th argument (VectorProvider)
        Ok(Arc::new(TinyBase::new(
            Arc::new(core), 
            Arc::new(data), 
            Arc::new(log), 
            Arc::new(sys),
            Arc::new(vec),
            Arc::new(SandboxVectorProvider)
        )))
    }

    /// Deletes the sandbox files
    pub fn cleanup_sandbox(session_id: &str) {
        let sandbox_dir = format!("sandboxes/session_{}", session_id);
        let _ = fs::remove_dir_all(&sandbox_dir);
        info!("Sandbox {} deleted", session_id);
    }
}