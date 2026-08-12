use std::collections::HashMap;
use std::io::{Cursor, Read, Write};
use std::sync::{Arc, RwLock};

use regex::Regex;
use rquickjs::loader::{ImportAttributes, Loader, Resolver};
use rquickjs::module::Declared;
use rquickjs::{Ctx, Error as QjsError, Module, Result as QjsResult};
use serde::{Deserialize, Serialize};
use serde_json::json;
use zip::write::FileOptions;
use zip::{ZipArchive, ZipWriter};

use crate::database::traits::Db;
use crate::models::{CreateTemplateReq, ai::CreateActionReq, script::CreateScriptReq};

// --- FILE METADATA SCHEMA ---
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
    pub trigger_type: String, // "before_create_record" | "manually" | "cron" | "graphql"
    #[serde(default = "default_true")]
    pub active: bool,
    #[serde(default = "default_visibility")]
    pub visibility: String, // "public" | "private"
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

// --- SCOPE-AWARE VIRTUAL FILE SYSTEM (VFS) ---
#[derive(Clone, Default)]
pub struct VfsState {
    // Key: (scope_id, file_path) -> content
    // e.g. ("root", "webhooks/test.js") or ("tenant:app-1", "webhooks/test.js")
    pub files: Arc<RwLock<HashMap<(String, String), String>>>,
}

impl VfsState {
    pub fn new() -> Self {
        Self {
            files: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Set a file for a specific tenant, sandbox, or root scope
    pub fn set_file(&self, scope: &str, path: &str, content: &str) {
        self.files
            .write()
            .unwrap()
            .insert((scope.to_string(), path.to_string()), content.to_string());
    }

    /// Retrieve a file for a scope with fallback to 'root' for shared modules
    pub fn get_file(&self, scope: &str, path: &str) -> Option<String> {
        let lock = self.files.read().unwrap();

        // 1. Check exact scope (e.g. "tenant:app-1")
        if let Some(content) = lock.get(&(scope.to_string(), path.to_string())) {
            return Some(content.clone());
        }

        // 2. Fallback to "root" scope for shared modules
        if scope != "root" {
            if let Some(content) = lock.get(&("root".to_string(), path.to_string())) {
                return Some(content.clone());
            }
        }

        None
    }
}

// --- RQUICKJS MODULE RESOLVER & LOADER ---
pub struct ApexModuleResolver;

impl Resolver for ApexModuleResolver {
    fn resolve<'js>(
        &mut self,
        _ctx: &Ctx<'js>,
        base: &str,
        name: &str,
        _attributes: Option<ImportAttributes<'js>>, // <-- Added parameter
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

pub struct ApexModuleLoader {
    pub vfs: VfsState,
}

impl Loader for ApexModuleLoader {
    fn load<'js>(
        &mut self,
        ctx: &Ctx<'js>,
        name: &str,
        _attributes: Option<ImportAttributes<'js>>,
    ) -> QjsResult<Module<'js, Declared>> {
        let code = if name.starts_with("http://") || name.starts_with("https://") {
            let res = reqwest::blocking::get(name).map_err(|_| QjsError::Unknown)?;
            res.text().map_err(|_| QjsError::Unknown)?
        } else if let Some(content) = self.vfs.get_file("root", name) {
            // <--- Checks active VFS
            content
        } else if std::path::Path::new(name).exists() {
            std::fs::read_to_string(name).map_err(|_| QjsError::Unknown)?
        } else {
            return Err(QjsError::Unknown);
        };

        Module::declare(ctx.clone(), name, code)
    }
}

// --- WORKSPACE ZIP EXPORT / IMPORT MANAGER ---
pub struct WorkspaceManager;

impl WorkspaceManager {
    /// Generates a ZIP archive containing all scripts, templates, and AI actions from SQLite.
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
                "scope": scope_id,
                "__track__": false,
                "dependencies": {}
            });
            zip.start_file("package.json", options)
                .map_err(|e| e.to_string())?;
            zip.write_all(serde_json::to_string_pretty(&pkg_json).unwrap().as_bytes())
                .map_err(|e| e.to_string())?;

            // 2. Export Scripts (Webhooks & Hooks)
            if let Ok(scripts) = db.list_scripts().await {
                for script in scripts {
                    let meta = FileMetadata {
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

                    let meta_js = format!(
                        "export const __fileMetadata__ = {};\n\n{}",
                        serde_json::to_string_pretty(&meta).unwrap(),
                        script.code
                    );

                    let filename = format!("webhooks/{}.js", script.name);
                    zip.start_file(&filename, options)
                        .map_err(|e| e.to_string())?;
                    zip.write_all(meta_js.as_bytes())
                        .map_err(|e| e.to_string())?;
                }
            }

            // 3. Export Templates
            if let Ok(templates) = db.list_templates().await {
                for tmpl in templates {
                    let meta = FileMetadata {
                        id: Some(tmpl.id),
                        name: tmpl.slug.clone(),
                        extension: "html".to_string(),
                        target_collection: None,
                        r#type: "template".to_string(),
                        path: "./templates/".to_string(),
                        trigger_type: "ssr".to_string(),
                        active: true,
                        visibility: "private".to_string(),
                    };

                    let meta_html = format!(
                        "<!--\n__fileMetadata__ = {}\n-->\n{}",
                        serde_json::to_string_pretty(&meta).unwrap(),
                        tmpl.content
                    );

                    let filename = format!("templates/{}", tmpl.slug);
                    zip.start_file(&filename, options)
                        .map_err(|e| e.to_string())?;
                    zip.write_all(meta_html.as_bytes())
                        .map_err(|e| e.to_string())?;
                }
            }

            // 4. Export AI Actions
            if let Ok(actions) = db.list_ai_actions().await {
                for action in actions {
                    let meta = FileMetadata {
                        id: Some(action.id),
                        name: action.slug.clone(),
                        extension: "json".to_string(),
                        target_collection: None,
                        r#type: "ai_action".to_string(),
                        path: "./ai_actions/".to_string(),
                        trigger_type: "ai_run".to_string(),
                        active: true,
                        visibility: "private".to_string(),
                    };

                    let action_payload = json!({
                        "__fileMetadata__": meta,
                        "action": action
                    });

                    let filename = format!("ai_actions/{}.json", action.slug);
                    zip.start_file(&filename, options)
                        .map_err(|e| e.to_string())?;
                    zip.write_all(
                        serde_json::to_string_pretty(&action_payload)
                            .unwrap()
                            .as_bytes(),
                    )
                    .map_err(|e| e.to_string())?;
                }
            }

            zip.finish().map_err(|e| e.to_string())?;
        }

        Ok(buffer.into_inner())
    }

    /// Unzips a bundle and imports all files into the database based on embedded `__fileMetadata__`.
    pub async fn import_workspace_zip(db: &Arc<dyn Db>, zip_bytes: &[u8]) -> Result<usize, String> {
        let cursor = Cursor::new(zip_bytes);
        let mut archive = ZipArchive::new(cursor).map_err(|e| format!("Invalid ZIP: {}", e))?;
        let mut imported_count = 0;

        for i in 0..archive.len() {
            let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
            if file.is_dir() {
                continue;
            }

            let mut content = String::new();
            if file.read_to_string(&mut content).is_ok() {
                if Self::commit_file_to_db(db, file.name(), &content)
                    .await
                    .is_ok()
                {
                    imported_count += 1;
                }
            }
        }

        Ok(imported_count)
    }

    /// Parses an individual file, extracts `__fileMetadata__`, and commits changes to SQLite DB.
    pub async fn commit_file_to_db(
        db: &Arc<dyn Db>,
        path: &str,
        content: &str,
    ) -> Result<String, String> {
        let meta_re = Regex::new(r"(?s)(?:export\s+const\s+__fileMetadata__\s*=\s*|<!--\s*__fileMetadata__\s*=\s*)(\{[\s\S]*?\})(?:;|\s*-->)").unwrap();

        if let Some(caps) = meta_re.captures(content) {
            let meta_json = caps.get(1).unwrap().as_str();
            let meta: FileMetadata = serde_json::from_str(meta_json)
                .map_err(|e| format!("Invalid __fileMetadata__: {}", e))?;

            let clean_code = meta_re.replace(content, "").trim().to_string();

            match meta.r#type.as_str() {
                "webhook" | "custom:module" => {
                    db.create_script(CreateScriptReq {
                        name: meta.name.clone(),
                        trigger_type: meta.trigger_type,
                        target_collection: meta.target_collection,
                        code: clean_code,
                        active: meta.active,
                        visibility: meta.visibility,
                    })
                    .await
                    .map_err(|e| e.to_string())?;
                    Ok(format!("Script '{}' committed to DB", meta.name))
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

                            // FIX: Changed create_action to create_ai_action
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
            Err(format!(
                "No __fileMetadata__ header block found in {}",
                path
            ))
        }
    }
}
