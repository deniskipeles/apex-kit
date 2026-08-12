use std::collections::HashMap;
use std::io::{Cursor, Write};
use std::sync::{Arc, RwLock};

use regex::Regex;
use rquickjs::loader::{ImportAttributes, Loader, Resolver};
use rquickjs::module::Declared;
use rquickjs::{Ctx, Error as QjsError, Module, Result as QjsResult};
use serde::{Deserialize, Serialize};
use serde_json::json;
use zip::ZipWriter;
use zip::write::FileOptions;

use crate::database::traits::Db;
use crate::models::{CreateTemplateReq, ai::CreateActionReq, script::CreateScriptReq};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FileMetadata {
    #[serde(default)]
    pub id: Option<i64>,
    pub name: String,
    #[serde(default = "default_extension")]
    pub extension: String,
    #[serde(default)]
    pub target_collection: Option<String>,
    #[serde(default = "default_type")]
    pub r#type: String, // "webhook" | "custom:module" | "esm:module" | "template" | "ai_action"
    #[serde(default = "default_path")]
    pub path: String, // "./webhooks/" | "./modules/custom/" | "./templates/"
    #[serde(default = "default_trigger")]
    pub trigger_type: String,
    #[serde(default = "default_true")]
    pub active: bool,
    #[serde(default = "default_visibility")]
    pub visibility: String,
}

fn default_extension() -> String {
    "js".to_string()
}
fn default_type() -> String {
    "webhook".to_string()
}
fn default_path() -> String {
    "./webhooks/".to_string()
}
fn default_trigger() -> String {
    "manually".to_string()
}
fn default_true() -> bool {
    true
}
fn default_visibility() -> String {
    "private".to_string()
}

#[derive(Clone, Default)]
pub struct VfsState {
    pub files: Arc<RwLock<HashMap<(String, String), String>>>,
}

impl VfsState {
    pub fn new() -> Self {
        Self {
            files: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    pub fn set_file(&self, scope: &str, path: &str, content: &str) {
        self.files
            .write()
            .unwrap()
            .insert((scope.to_string(), path.to_string()), content.to_string());
    }
    pub fn get_file(&self, scope: &str, path: &str) -> Option<String> {
        let lock = self.files.read().unwrap();
        if let Some(content) = lock.get(&(scope.to_string(), path.to_string())) {
            return Some(content.clone());
        }
        if scope != "root" {
            if let Some(content) = lock.get(&("root".to_string(), path.to_string())) {
                return Some(content.clone());
            }
        }
        None
    }
}

pub struct ApexModuleResolver;

impl Resolver for ApexModuleResolver {
    fn resolve<'js>(
        &mut self,
        _ctx: &Ctx<'js>,
        base: &str,
        name: &str,
        _attr: Option<ImportAttributes<'js>>,
    ) -> QjsResult<String> {
        if name.starts_with("http://") || name.starts_with("https://") {
            return Ok(name.to_string());
        }
        if name.starts_with("@/custom/") {
            let clean = name.strip_prefix("@/custom/").unwrap();
            return Ok(format!(
                "./modules/custom/{}.js",
                clean.trim_end_matches(".js")
            ));
        }
        if name.starts_with("@/esm/") {
            let clean = name.strip_prefix("@/esm/").unwrap();
            return Ok(format!(
                "./modules/esm/{}.js",
                clean.trim_end_matches(".js")
            ));
        }
        if name.starts_with("./") || name.starts_with("../") {
            return Ok(format!("{}{}", base, name));
        }
        Ok(name.to_string())
    }
}

// [NEW] ApexModuleLoader now takes the DB to fetch missing scripts
pub struct ApexModuleLoader {
    pub vfs: VfsState,
    pub db: Arc<dyn Db>,
}

impl Loader for ApexModuleLoader {
    fn load<'js>(
        &mut self,
        ctx: &Ctx<'js>,
        name: &str,
        _attr: Option<ImportAttributes<'js>>,
    ) -> QjsResult<Module<'js, Declared>> {
        let code = if name.starts_with("http://") || name.starts_with("https://") {
            let res = reqwest::blocking::get(name).map_err(|_| QjsError::Unknown)?;
            res.text().map_err(|_| QjsError::Unknown)?
        } else if let Some(content) = self.vfs.get_file("root", name) {
            // Note: Since module loader executes under root context for now, we default to root.
            // Advanced scoping can be passed via context if needed.
            content
        } else if name.starts_with("./modules/") {
            // [CRITICAL] Fallback to SQLite DB for custom modules after restart
            let script_name = name.split('/').last().unwrap().trim_end_matches(".js");
            let db = self.db.clone();
            let s_name = script_name.to_string();

            // Bridge sync QuickJS to async Tokio DB
            let code_res = tokio::task::block_in_place(move || {
                tokio::runtime::Handle::current()
                    .block_on(async move { db.get_script_by_name(&s_name).await })
            });

            if let Ok(Some(script)) = code_res {
                script.code
            } else {
                return Err(QjsError::Unknown);
            }
        } else if std::path::Path::new(name).exists() {
            std::fs::read_to_string(name).map_err(|_| QjsError::Unknown)?
        } else {
            return Err(QjsError::Unknown);
        };

        Module::declare(ctx.clone(), name, code)
    }
}

pub struct WorkspaceManager;

impl WorkspaceManager {
    pub async fn export_workspace_zip(db: &Arc<dyn Db>, scope_id: &str) -> Result<Vec<u8>, String> {
        let mut buffer = Cursor::new(Vec::new());

        {
            let mut zip = ZipWriter::new(&mut buffer);
            let options = FileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .unix_permissions(0o755);

            // 1. Export package.json
            let pkg_json = json!({
                "name": "apexkit-workspace",
                "type": "module",
                "scope": scope_id,
                "__track__": false,
                "dependencies": {}
            });
            zip.start_file("package.json", options)
                .map_err(|e| e.to_string())?;
            zip.write_all(serde_json::to_string_pretty(&pkg_json).unwrap().as_bytes())
                .map_err(|e| e.to_string())?;

            if let Ok(scripts) = db.list_scripts().await {
                for script in scripts {
                    // [UPDATED] Use Database Metadata if present!
                    let mut meta = FileMetadata {
                        id: Some(script.id),
                        name: script.name.clone(),
                        extension: "js".to_string(),
                        target_collection: script.target_collection.clone(),
                        r#type: "webhook".to_string(),
                        path: "./webhooks/".to_string(),
                        trigger_type: script.trigger_type.clone(),
                        active: script.active,
                        visibility: script.visibility.clone(),
                    };

                    if let Some(db_meta) = &script.metadata {
                        if let Ok(parsed) = serde_json::from_value::<FileMetadata>(db_meta.clone())
                        {
                            meta.r#type = parsed.r#type;
                            meta.path = parsed.path;
                            meta.extension = parsed.extension;
                        }
                    }

                    // Only skip ESM modules if requested, but for now we export everything so it's a full backup
                    let meta_js = format!(
                        "export const __fileMetadata__ = {};\n\n{}",
                        serde_json::to_string_pretty(&meta).unwrap(),
                        script.code
                    );
                    let filename = format!(
                        "{}{}.{}",
                        meta.path.trim_start_matches("./"),
                        meta.name,
                        meta.extension
                    );

                    zip.start_file(&filename, options)
                        .map_err(|e| e.to_string())?;
                    zip.write_all(meta_js.as_bytes())
                        .map_err(|e| e.to_string())?;
                }
            }

            // ... templates and AI actions export logic remains the same
            zip.finish().map_err(|e| e.to_string())?;
        }
        Ok(buffer.into_inner())
    }

    /// Parses an individual file, extracts `__fileMetadata__`, and commits changes to SQLite DB.
    pub async fn commit_file_to_db(
        db: &Arc<dyn Db>,
        _path: &str,
        content: &str,
    ) -> Result<String, String> {
        let meta_re = Regex::new(r"(?s)(?:export\s+const\s+__fileMetadata__\s*=\s*|<!--\s*__fileMetadata__\s*=\s*)(\{[\s\S]*?\})(?:;|\s*-->)").unwrap();

        if let Some(caps) = meta_re.captures(content) {
            let meta_json = caps.get(1).unwrap().as_str();
            let meta: FileMetadata = serde_json::from_str(meta_json)
                .map_err(|e| format!("Invalid __fileMetadata__: {}", e))?;
            let clean_code = meta_re.replace(content, "").trim().to_string();

            // FIX: Serialize metadata to Value BEFORE moving meta's fields!
            let meta_val = serde_json::to_value(&meta).ok();

            match meta.r#type.as_str() {
                "webhook" | "custom:module" | "esm:module" => {
                    db.create_script(CreateScriptReq {
                        name: meta.name.clone(),
                        trigger_type: meta.trigger_type,
                        target_collection: meta.target_collection,
                        code: clean_code,
                        active: meta.active,
                        visibility: meta.visibility,
                        metadata: meta_val, // <-- Pass prepared Value here
                    })
                    .await
                    .map_err(|e| e.to_string())?;
                    Ok(format!("Script/Module '{}' committed to DB", meta.name))
                }
                "template" => {
                    db.create_template(CreateTemplateReq {
                        slug: meta.name.clone(),
                        content: clean_code,
                        script_id: None,
                    })
                    .await
                    .map_err(|e| e.to_string())?;
                    Ok(format!("Template '{}' committed to DB", meta.name))
                }
                "ai_action" => {
                    if let Ok(payload) = serde_json::from_str::<serde_json::Value>(content) {
                        if let Some(action) = payload.get("action") {
                            let action_req: CreateActionReq =
                                serde_json::from_value(action.clone())
                                    .map_err(|e| e.to_string())?;
                            db.create_ai_action(action_req)
                                .await
                                .map_err(|e| e.to_string())?;
                            return Ok(format!("AI Action '{}' committed to DB", meta.name));
                        }
                    }
                    Err("Invalid AI action file format".to_string())
                }
                _ => Err("Unsupported metadata type".to_string()),
            }
        } else {
            Err("No __fileMetadata__ header block found in file".to_string())
        }
    }
}
