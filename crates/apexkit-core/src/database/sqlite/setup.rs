use crate::database::traits::VectorProvider;
use crate::models::ChangesetEvent;
use crate::search::SearchManager;
use rusqlite::{Connection, Result};
use std::sync::Arc;

pub async fn a_new_database_connection(
    vector_provider: Arc<dyn VectorProvider>,
    forwarder: Option<Arc<dyn crate::batching::WriteForwarder>>,
    event_tx: Option<tokio::sync::broadcast::Sender<ChangesetEvent>>,
) -> std::result::Result<super::connections::ApexKit, Box<dyn std::error::Error + Send + Sync>> {
    let base_path = "storage/system";
    let base_dirs = vec![
        base_path,
        "storage/system/uploads",
        "storage/system/indexes",
        "storage/tenants",
        "storage/sandboxes",
        "storage/tmp",
    ];

    for dir in base_dirs {
        std::fs::create_dir_all(dir)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
    }

    let core = Connection::open(format!("{}/core.db", base_path))?;
    let data = Connection::open(format!("{}/data.db", base_path))?;
    let log = Connection::open(format!("{}/logs.db", base_path))?;
    let sys = Connection::open(format!("{}/system.db", base_path))?;
    let vec = Connection::open(format!("{}/vectors.db", base_path))?;

    apply_pragmas(&core)?;
    apply_pragmas(&data)?;
    apply_pragmas(&log)?;
    apply_pragmas(&sys)?;
    apply_pragmas(&vec)?;

    setup_core(&core)?;
    setup_data(&data)?;
    setup_logs(&log)?;
    setup_sys(&sys)?;
    setup_vectors(&vec)?;

    let mut instance = super::connections::ApexKit::new(
        base_path,
        core,
        data,
        log,
        sys,
        vec,
        vector_provider,
        forwarder,
        event_tx,
        "root".to_string(),
    );

    instance
        .get_core_read()
        .await
        .execute_batch("PRAGMA busy_timeout = 5000;")?;
    instance
        .get_data_read()
        .await
        .execute_batch("PRAGMA busy_timeout = 5000;")?;
    instance
        .get_log_read()
        .await
        .execute_batch("PRAGMA busy_timeout = 5000;")?;
    instance
        .get_sys_read()
        .await
        .execute_batch("PRAGMA busy_timeout = 5000;")?;
    instance
        .get_vector_read()
        .await
        .execute_batch("PRAGMA busy_timeout = 5000;")?;

    instance.set_search_manager(Arc::new(SearchManager::new("storage/system/indexes")));

    Ok(instance)
}

pub fn calculate_dir_size(path: &std::path::Path) -> std::io::Result<u64> {
    let mut total_size = 0;
    if path.is_dir() {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            if metadata.is_dir() {
                total_size += calculate_dir_size(&entry.path())?;
            } else {
                total_size += metadata.len();
            }
        }
    } else if path.exists() {
        total_size = path.metadata()?.len();
    }
    Ok(total_size)
}

pub fn apply_pragmas(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;
        PRAGMA busy_timeout = 5000;
        PRAGMA temp_store = MEMORY;
        PRAGMA cache_size = -64000;
        PRAGMA wal_autocheckpoint = 1000;
        PRAGMA mmap_size = 30000000000;
    ",
    )?;
    Ok(())
}

pub fn setup_core(conn: &Connection) -> Result<()> {
    conn.execute("CREATE TABLE IF NOT EXISTS users (id INTEGER PRIMARY KEY AUTOINCREMENT, email TEXT NOT NULL UNIQUE, password_hash TEXT NOT NULL, role TEXT NOT NULL, is_verified BOOLEAN DEFAULT 0, metadata JSON DEFAULT '{}')", [])?;
    let _ = conn.execute(
        "ALTER TABLE users ADD COLUMN metadata JSON DEFAULT '{}'",
        [],
    );

    conn.execute("CREATE TABLE IF NOT EXISTS auth_identities (id INTEGER PRIMARY KEY AUTOINCREMENT, user_id INTEGER NOT NULL, provider TEXT NOT NULL, provider_id TEXT NOT NULL)", [])?;
    conn.execute("CREATE TABLE IF NOT EXISTS auth_tokens (token TEXT PRIMARY KEY, user_id INTEGER NOT NULL, type TEXT NOT NULL, expires_at DATETIME NOT NULL)", [])?;
    conn.execute("CREATE TABLE IF NOT EXISTS _system_config_settings (key TEXT PRIMARY KEY, value TEXT NOT NULL, encrypted BOOLEAN DEFAULT 0, updated_at DATETIME DEFAULT CURRENT_TIMESTAMP)", [])?;
    let _ = conn.execute("DROP TABLE IF EXISTS _api_keys", []); // Clean transition
    conn.execute(
        "CREATE TABLE IF NOT EXISTS _api_keys_v2 (
        id INTEGER PRIMARY KEY AUTOINCREMENT, 
        name TEXT NOT NULL, 
        tenant_id TEXT NOT NULL, 
        key_id TEXT NOT NULL, 
        secret_hash TEXT NOT NULL, 
        issuer TEXT NOT NULL,
        env_type TEXT NOT NULL,
        roles JSON DEFAULT '[]', 
        status TEXT DEFAULT 'active', 
        bypass_cors BOOLEAN DEFAULT 0,
        created_at DATETIME DEFAULT CURRENT_TIMESTAMP
    )",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_api_keys_lookup ON _api_keys_v2(tenant_id, key_id)",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS _tenants (
        id TEXT PRIMARY KEY,
        name TEXT,
        owner_id INTEGER,
        status TEXT DEFAULT 'active',
        tier TEXT DEFAULT 'free',
        max_storage_mb INTEGER DEFAULT 500,
        current_storage_mb REAL DEFAULT 0,
        max_vectors INTEGER DEFAULT 10000,
        current_vectors INTEGER DEFAULT 0,
        max_ai_requests INTEGER DEFAULT 100,
        current_ai_requests INTEGER DEFAULT 0,
        created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
        updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
    )",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS _sandboxes (
        id TEXT PRIMARY KEY,
        name TEXT,
        owner_id INTEGER,
        status TEXT DEFAULT 'active',
        max_storage_mb INTEGER DEFAULT 100,
        current_storage_mb REAL DEFAULT 0,
        expires_at DATETIME,
        created_at DATETIME DEFAULT CURRENT_TIMESTAMP
    )",
        [],
    )?;
    // [NEW] Apply schema modifications safely
    let _ = conn.execute(
        "ALTER TABLE _sandboxes ADD COLUMN scope TEXT DEFAULT 'root'",
        [],
    );
    let _ = conn.execute("ALTER TABLE _sandboxes ADD COLUMN tenant_id TEXT", []);

    Ok(())
}

pub fn setup_data(conn: &Connection) -> Result<()> {
    conn.execute("CREATE TABLE IF NOT EXISTS collections (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, schema JSON, index_key TEXT UNIQUE)", [])?;
    let _ = conn.execute("ALTER TABLE collections ADD COLUMN index_key TEXT", []);

    conn.execute("CREATE TABLE IF NOT EXISTS records (id INTEGER PRIMARY KEY AUTOINCREMENT, collection_id INTEGER NOT NULL, data JSONB NOT NULL, created DATETIME DEFAULT CURRENT_TIMESTAMP, updated DATETIME DEFAULT CURRENT_TIMESTAMP)", [])?;

    let _ = conn.execute(
        "ALTER TABLE records ADD COLUMN created DATETIME DEFAULT CURRENT_TIMESTAMP",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE records ADD COLUMN updated DATETIME DEFAULT CURRENT_TIMESTAMP",
        [],
    );

    conn.execute("CREATE TABLE IF NOT EXISTS _storage_files (id INTEGER PRIMARY KEY AUTOINCREMENT, filename TEXT NOT NULL, original_name TEXT NOT NULL, mime_type TEXT NOT NULL, size INTEGER NOT NULL, user_id INTEGER, created_at DATETIME DEFAULT CURRENT_TIMESTAMP)", [])?;
    conn.execute("CREATE TABLE IF NOT EXISTS _relations (id INTEGER PRIMARY KEY AUTOINCREMENT, origin_col_id INTEGER NOT NULL, origin_rec_id INTEGER NOT NULL, target_col_id INTEGER NOT NULL, target_rec_id INTEGER NOT NULL, rel_name TEXT NOT NULL, properties JSON)", [])?;
    conn.execute("CREATE INDEX IF NOT EXISTS idx_relations_origin ON _relations(origin_col_id, origin_rec_id, rel_name)", [])?;
    conn.execute("CREATE INDEX IF NOT EXISTS idx_relations_target ON _relations(target_col_id, target_rec_id)", [])?;

    // [FIX] Unique constraint to prevent duplicate relationship links
    conn.execute("CREATE UNIQUE INDEX IF NOT EXISTS idx_relations_unique ON _relations(origin_col_id, origin_rec_id, target_col_id, target_rec_id, rel_name)", [])?;

    conn.execute("CREATE TABLE IF NOT EXISTS _unique_values (index_key TEXT NOT NULL, value TEXT NOT NULL, record_id INTEGER NOT NULL, PRIMARY KEY (index_key, value))", [])?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_unique_lookup ON _unique_values(index_key, value)",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_unique_record ON _unique_values(record_id)",
        [],
    )?;
    Ok(())
}

pub fn setup_logs(conn: &Connection) -> Result<()> {
    conn.execute("CREATE TABLE IF NOT EXISTS _audit_logs (id INTEGER PRIMARY KEY AUTOINCREMENT, level TEXT NOT NULL, message TEXT NOT NULL, source TEXT NOT NULL, meta JSON, timestamp DATETIME DEFAULT CURRENT_TIMESTAMP)", [])?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS _system_logs (
        id INTEGER PRIMARY KEY AUTOINCREMENT, 
        level TEXT NOT NULL, 
        target TEXT NOT NULL, 
        message TEXT NOT NULL, 
        timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
    )",
        [],
    )?;
    Ok(())
}

pub fn setup_sys(conn: &Connection) -> Result<()> {
    conn.execute("CREATE TABLE IF NOT EXISTS _ai_actions (id INTEGER PRIMARY KEY AUTOINCREMENT, slug TEXT UNIQUE NOT NULL, name TEXT NOT NULL, model TEXT NOT NULL, system_prompt TEXT, template TEXT NOT NULL, config JSON, created_at DATETIME DEFAULT CURRENT_TIMESTAMP)", [])?;
    conn.execute("CREATE TABLE IF NOT EXISTS _ai_sessions (id TEXT PRIMARY KEY, name TEXT NOT NULL, messages JSON, current_manifest JSON, pending_manifest JSON, diff_summary TEXT, last_error TEXT, created_at DATETIME DEFAULT CURRENT_TIMESTAMP)", [])?;
    conn.execute("CREATE TABLE IF NOT EXISTS _plugins (id TEXT PRIMARY KEY, name TEXT NOT NULL, version TEXT NOT NULL, manifest JSON NOT NULL, description TEXT, created_at DATETIME DEFAULT CURRENT_TIMESTAMP)", [])?;
    conn.execute("CREATE TABLE IF NOT EXISTS _templates (id INTEGER PRIMARY KEY AUTOINCREMENT, slug TEXT UNIQUE NOT NULL, content TEXT NOT NULL, script_id INTEGER, created_at DATETIME DEFAULT CURRENT_TIMESTAMP)", [])?;
    conn.execute("CREATE TABLE IF NOT EXISTS _scripts (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT UNIQUE NOT NULL, trigger_type TEXT NOT NULL, code TEXT NOT NULL, active BOOLEAN DEFAULT 1, target_collection TEXT, visibility TEXT DEFAULT 'private', created_at DATETIME DEFAULT CURRENT_TIMESTAMP)", [])?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS _replicas (
        id TEXT PRIMARY KEY,
        scopes TEXT NOT NULL,
        last_seen DATETIME DEFAULT CURRENT_TIMESTAMP
    )",
        [],
    )?;

    let _ = conn.execute(
        "ALTER TABLE _scripts ADD COLUMN visibility TEXT DEFAULT 'private'",
        [],
    );
    Ok(())
}

pub fn setup_vectors(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS vectors (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            collection_id INTEGER NOT NULL,
            record_id INTEGER NOT NULL,
            field_name TEXT NOT NULL,
            vector BLOB NOT NULL,
            model TEXT NOT NULL DEFAULT 'unknown',
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            UNIQUE(collection_id, record_id, field_name, model) 
        )",
        [],
    )?;
    let _ = conn.execute("CREATE UNIQUE INDEX IF NOT EXISTS idx_vec_unique_model ON vectors(collection_id, record_id, field_name, model)", []);
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_vec_record ON vectors(record_id)",
        [],
    )?;
    Ok(())
}
