use crate::schema::CollectionSchema;
use async_trait::async_trait;
use libsql::{params, Builder, Connection, Database, Result, Row};
use serde_json::Value;
use tokio::sync::Mutex;

pub mod models;
pub mod schema;
pub mod validation;

#[derive(Debug)]
pub struct Collection {
    pub id: i64,
    pub name: String,
    pub schema: Option<CollectionSchema>,
}

#[derive(Debug)]
pub struct Record {
    pub id: i64,
    pub data: Value,
}

#[async_trait]
pub trait Db: Send + Sync {
    async fn create_collection(
        &self,
        name: &str,
        schema: &Option<CollectionSchema>,
    ) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>>;
    async fn get_collection(
        &self,
        id: i64,
    ) -> std::result::Result<Option<Collection>, Box<dyn std::error::Error + Send + Sync>>;
    async fn list_collections(
        &self,
    ) -> std::result::Result<Vec<Collection>, Box<dyn std::error::Error + Send + Sync>>;
    async fn update_collection(
        &self,
        id: i64,
        name: Option<String>,
        schema: Option<CollectionSchema>,
    ) -> std::result::Result<Collection, Box<dyn std::error::Error + Send + Sync>>;
    async fn delete_collection(&self, id: i64) -> Result<()>;
    async fn create_record(
        &self,
        collection_id: i64,
        data: &Value,
    ) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>>;
    async fn list_records(
        &self,
        collection_id: i64,
    ) -> std::result::Result<Vec<Record>, Box<dyn std::error::Error + Send + Sync>>;
    async fn get_record(
        &self,
        collection_id: i64,
        record_id: i64,
    ) -> std::result::Result<Option<Record>, Box<dyn std::error::Error + Send + Sync>>;
    async fn update_record(
        &self,
        collection_id: i64,
        record_id: i64,
        data: &Value,
    ) -> std::result::Result<Record, Box<dyn std::error::Error + Send + Sync>>;
    async fn delete_record(&self, collection_id: i64, record_id: i64) -> Result<()>;
}

fn row_to_collection(
    row: &Row,
) -> std::result::Result<Collection, Box<dyn std::error::Error + Send + Sync>> {
    let schema_str: Option<String> = row.get(2)?;
    let schema = match schema_str {
        Some(s) => serde_json::from_str(&s)?,
        None => None,
    };
    Ok(Collection {
        id: row.get(0)?,
        name: row.get(1)?,
        schema,
    })
}

fn row_to_record(
    row: &Row,
) -> std::result::Result<Record, Box<dyn std::error::Error + Send + Sync>> {
    let data_str: String = row.get(1)?;
    let data = serde_json::from_str(&data_str)?;
    Ok(Record {
        id: row.get(0)?,
        data,
    })
}

#[async_trait]
impl Db for Database {
    async fn create_collection(
        &self,
        name: &str,
        schema: &Option<CollectionSchema>,
    ) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.connect()?;
        let schema_str = serde_json::to_string(&schema)?;
        conn.execute(
            "INSERT INTO collections (name, schema) VALUES (?1, ?2)",
            params![name, schema_str],
        )
        .await?;
        Ok(conn.last_insert_rowid())
    }

    async fn get_collection(
        &self,
        id: i64,
    ) -> std::result::Result<Option<Collection>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.connect()?;
        let mut rows = conn
            .query("SELECT id, name, schema FROM collections WHERE id = ?1", params![id])
            .await?;
        let row = match rows.next().await? {
            Some(row) => row,
            None => return Ok(None),
        };
        Ok(Some(row_to_collection(&row)?))
    }

    async fn list_collections(
        &self,
    ) -> std::result::Result<Vec<Collection>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.connect()?;
        let mut rows = conn
            .query("SELECT id, name, schema FROM collections", ())
            .await?;
        let mut collections = Vec::new();
        while let Some(row) = rows.next().await? {
            collections.push(row_to_collection(&row)?);
        }
        Ok(collections)
    }

    async fn update_collection(
        &self,
        id: i64,
        name: Option<String>,
        schema: Option<CollectionSchema>,
    ) -> std::result::Result<Collection, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.connect()?;
        if let Some(name) = name {
            conn.execute(
                "UPDATE collections SET name = ?1 WHERE id = ?2",
                params![name, id],
            )
            .await?;
        }
        if let Some(schema) = schema {
            let schema_str = serde_json::to_string(&schema)?;
            conn.execute(
                "UPDATE collections SET schema = ?1 WHERE id = ?2",
                params![schema_str, id],
            )
            .await?;
        }
        let collection = self.get_collection(id).await?.ok_or("Collection not found")?;
        Ok(collection)
    }

    async fn delete_collection(&self, id: i64) -> Result<()> {
        let conn = self.connect()?;
        conn.execute("DELETE FROM collections WHERE id = ?1", params![id])
            .await?;
        Ok(())
    }

    async fn create_record(
        &self,
        collection_id: i64,
        data: &Value,
    ) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.connect()?;
        let data_str = serde_json::to_string(data)?;
        conn.execute(
            "INSERT INTO records (collection_id, data) VALUES (?1, ?2)",
            params![collection_id, data_str],
        )
        .await?;
        Ok(conn.last_insert_rowid())
    }

    async fn list_records(
        &self,
        collection_id: i64,
    ) -> std::result::Result<Vec<Record>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.connect()?;
        let mut rows = conn
            .query(
                "SELECT id, data FROM records WHERE collection_id = ?1",
                params![collection_id],
            )
            .await?;
        let mut records = Vec::new();
        while let Some(row) = rows.next().await? {
            records.push(row_to_record(&row)?);
        }
        Ok(records)
    }

    async fn get_record(
        &self,
        collection_id: i64,
        record_id: i64,
    ) -> std::result::Result<Option<Record>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.connect()?;
        let mut rows = conn
            .query(
                "SELECT id, data FROM records WHERE collection_id = ?1 AND id = ?2",
                params![collection_id, record_id],
            )
            .await?;
        let row = match rows.next().await? {
            Some(row) => row,
            None => return Ok(None),
        };
        Ok(Some(row_to_record(&row)?))
    }

    async fn update_record(
        &self,
        collection_id: i64,
        record_id: i64,
        data: &Value,
    ) -> std::result::Result<Record, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.connect()?;
        let data_str = serde_json::to_string(data)?;
        conn.execute(
            "UPDATE records SET data = ?1 WHERE collection_id = ?2 AND id = ?3",
            params![data_str, collection_id, record_id],
        )
        .await?;
        let record = self
            .get_record(collection_id, record_id)
            .await?
            .ok_or("Record not found")?;
        Ok(record)
    }

    async fn delete_record(&self, collection_id: i64, record_id: i64) -> Result<()> {
        let conn = self.connect()?;
        conn.execute(
            "DELETE FROM records WHERE collection_id = ?1 AND id = ?2",
            params![collection_id, record_id],
        )
        .await?;
        Ok(())
    }
}

#[async_trait]
impl Db for Mutex<Connection> {
    async fn create_collection(
        &self,
        name: &str,
        schema: &Option<CollectionSchema>,
    ) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.lock().await;
        let schema_str = serde_json::to_string(&schema)?;
        conn.execute(
            "INSERT INTO collections (name, schema) VALUES (?1, ?2)",
            params![name, schema_str],
        )
        .await?;
        Ok(conn.last_insert_rowid())
    }

    async fn get_collection(
        &self,
        id: i64,
    ) -> std::result::Result<Option<Collection>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.lock().await;
        let mut rows = conn
            .query("SELECT id, name, schema FROM collections WHERE id = ?1", params![id])
            .await?;
        let row = match rows.next().await? {
            Some(row) => row,
            None => return Ok(None),
        };
        Ok(Some(row_to_collection(&row)?))
    }

    async fn list_collections(
        &self,
    ) -> std::result::Result<Vec<Collection>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.lock().await;
        let mut rows = conn
            .query("SELECT id, name, schema FROM collections", ())
            .await?;
        let mut collections = Vec::new();
        while let Some(row) = rows.next().await? {
            collections.push(row_to_collection(&row)?);
        }
        Ok(collections)
    }

    async fn update_collection(
        &self,
        id: i64,
        name: Option<String>,
        schema: Option<CollectionSchema>,
    ) -> std::result::Result<Collection, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.lock().await;
        if let Some(name) = name {
            conn.execute(
                "UPDATE collections SET name = ?1 WHERE id = ?2",
                params![name, id],
            )
            .await?;
        }
        if let Some(schema) = schema {
            let schema_str = serde_json::to_string(&schema)?;
            conn.execute(
                "UPDATE collections SET schema = ?1 WHERE id = ?2",
                params![schema_str, id],
            )
            .await?;
        }
        let mut rows = conn
            .query("SELECT id, name, schema FROM collections WHERE id = ?1", params![id])
            .await?;
        let row = rows.next().await?.ok_or("Collection not found")?;
        Ok(row_to_collection(&row)?)
    }

    async fn delete_collection(&self, id: i64) -> Result<()> {
        let conn = self.lock().await;
        conn.execute("DELETE FROM collections WHERE id = ?1", params![id])
            .await?;
        Ok(())
    }

    async fn create_record(
        &self,
        collection_id: i64,
        data: &Value,
    ) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.lock().await;
        let data_str = serde_json::to_string(data)?;
        conn.execute(
            "INSERT INTO records (collection_id, data) VALUES (?1, ?2)",
            params![collection_id, data_str],
        )
        .await?;
        Ok(conn.last_insert_rowid())
    }

    async fn list_records(
        &self,
        collection_id: i64,
    ) -> std::result::Result<Vec<Record>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.lock().await;
        let mut rows = conn
            .query(
                "SELECT id, data FROM records WHERE collection_id = ?1",
                params![collection_id],
            )
            .await?;
        let mut records = Vec::new();
        while let Some(row) = rows.next().await? {
            records.push(row_to_record(&row)?);
        }
        Ok(records)
    }

    async fn get_record(
        &self,
        collection_id: i64,
        record_id: i64,
    ) -> std::result::Result<Option<Record>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.lock().await;
        let mut rows = conn
            .query(
                "SELECT id, data FROM records WHERE collection_id = ?1 AND id = ?2",
                params![collection_id, record_id],
            )
            .await?;
        let row = match rows.next().await? {
            Some(row) => row,
            None => return Ok(None),
        };
        Ok(Some(row_to_record(&row)?))
    }

    async fn update_record(
        &self,
        collection_id: i64,
        record_id: i64,
        data: &Value,
    ) -> std::result::Result<Record, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.lock().await;
        let data_str = serde_json::to_string(data)?;
        conn.execute(
            "UPDATE records SET data = ?1 WHERE collection_id = ?2 AND id = ?3",
            params![data_str, collection_id, record_id],
        )
        .await?;
        let record = self
            .get_record(collection_id, record_id)
            .await?
            .ok_or("Record not found")?;
        Ok(record)
    }

    async fn delete_record(&self, collection_id: i64, record_id: i64) -> Result<()> {
        let conn = self.lock().await;
        conn.execute(
            "DELETE FROM records WHERE collection_id = ?1 AND id = ?2",
            params![collection_id, record_id],
        )
        .await?;
        Ok(())
    }
}

pub async fn a_new_database_connection() -> Result<Database> {
    let db = Builder::new_local("local.db").build().await?;
    setup_database(&db).await?;
    Ok(db)
}

async fn setup_database(db: &Database) -> Result<()> {
    let conn = db.connect()?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS collections (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, schema JSON)",
        (),
    )
    .await?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS records (id INTEGER PRIMARY KEY AUTOINCREMENT, collection_id INTEGER NOT NULL, data TEXT NOT NULL)",
        (),
    )
    .await?;
    Ok(())
}
