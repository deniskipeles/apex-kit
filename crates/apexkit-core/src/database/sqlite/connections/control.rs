use super::ApexKit;
use crate::database::traits::ConnectionStore;
use async_trait::async_trait;
use rusqlite::Connection;

#[async_trait]
impl ConnectionStore for ApexKit {
    async fn reload_connections(
        &self,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut core = self.hot_conn_core.lock().await;
        *core = Connection::open(format!("{}/core.db", self.base_path))?;
        super::super::setup::apply_pragmas(&core)?;

        let mut data = self.hot_conn_data.lock().await;
        *data = Connection::open(format!("{}/data.db", self.base_path))?;
        super::super::setup::apply_pragmas(&data)?;

        let mut log = self.hot_conn_log.lock().await;
        *log = Connection::open(format!("{}/logs.db", self.base_path))?;
        super::super::setup::apply_pragmas(&log)?;

        let mut sys = self.hot_conn_sys.lock().await;
        *sys = Connection::open(format!("{}/system.db", self.base_path))?;
        super::super::setup::apply_pragmas(&sys)?;

        let mut vec = self.hot_conn_vec.lock().await;
        *vec = Connection::open(format!("{}/vectors.db", self.base_path))?;
        super::super::setup::apply_pragmas(&vec)?;

        Ok(())
    }
}
