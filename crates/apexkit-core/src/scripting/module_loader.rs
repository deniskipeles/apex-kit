use std::collections::HashMap;
use std::io::{Cursor, Write};
use std::path::Path;
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Duration;

use oxc::{Compiler, span::SourceType};
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

/// Transpiles TypeScript / TSX to JavaScript using the Oxc compiler.
pub fn transpile_ts(source_text: &str, file_path: &str) -> Result<String, String> {
    let path = Path::new(file_path);
    let is_explicit_ts = file_path.ends_with(".ts") || file_path.ends_with(".tsx");

    let source_type = if is_explicit_ts {
        SourceType::from_path(path).unwrap_or_else(|_| SourceType::ts())
    } else {
        // Automatically detect TypeScript keywords if file is unnamed or labeled .js
        if source_text.contains(": ")
            || source_text.contains("interface ")
            || source_text.contains("type ")
            || source_text.contains("as ")
        {
            SourceType::ts()
        } else {
            return Ok(source_text.to_string());
        }
    };

    let mut compiler = Compiler::default();
    match compiler.execute(source_text, source_type, path) {
        Ok(js_code) => Ok(js_code),
        Err(diags) => Err(format!("TypeScript Transpilation Error: {:?}", diags)),
    }
}

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
    pub r#type: String,
    #[serde(default = "default_path")]
    pub path: String,
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

    /// Stores a file in VFS, automatically transpiling TypeScript to JS beforehand.
    pub fn set_file(&self, scope: &str, path: &str, content: &str) {
        let executable_js = transpile_ts(content, path).unwrap_or_else(|_| content.to_string());
        self.files
            .write()
            .unwrap()
            .insert((scope.to_string(), path.to_string()), executable_js);
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

    pub fn remove_file(&self, scope: &str, path: &str) {
        self.files
            .write()
            .unwrap()
            .remove(&(scope.to_string(), path.to_string()));
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
        if base.starts_with("http://") || base.starts_with("https://") {
            if let Ok(base_url) = url::Url::parse(base) {
                if let Ok(joined) = base_url.join(name) {
                    return Ok(joined.to_string());
                }
            }
        }
        if name.starts_with("@/custom/") {
            let clean = name.strip_prefix("@/custom/").unwrap();
            return Ok(format!(
                "./modules/custom/{}.js",
                clean.trim_end_matches(".js").trim_end_matches(".ts")
            ));
        }
        if name.starts_with("@/esm/") {
            let clean = name.strip_prefix("@/esm/").unwrap();
            return Ok(format!(
                "./modules/esm/{}.js",
                clean.trim_end_matches(".js").trim_end_matches(".ts")
            ));
        }
        if name.starts_with("./") || name.starts_with("../") {
            return Ok(format!("{}{}", base, name));
        }
        Ok(name.to_string())
    }
}

static BLOCKING_HTTP_CLIENT: OnceLock<ureq::Agent> = OnceLock::new();

fn get_ureq_agent() -> &'static ureq::Agent {
    BLOCKING_HTTP_CLIENT.get_or_init(|| {
        ureq::builder()
            .timeout(Duration::from_secs(15))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
            .build()
    })
}

static MODULE_CACHE: OnceLock<RwLock<HashMap<String, String>>> = OnceLock::new();
fn get_module_cache() -> &'static RwLock<HashMap<String, String>> {
    MODULE_CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

// 1. Update the Struct to hold the scope string
pub struct ApexModuleLoader {
    pub vfs: VfsState,
    pub db: Arc<dyn Db>,
    pub scope: String, // <--- ADD THIS
}

impl Loader for ApexModuleLoader {
    fn load<'js>(
        &mut self,
        ctx: &Ctx<'js>,
        name: &str,
        _attr: Option<ImportAttributes<'js>>,
    ) -> QjsResult<Module<'js, Declared>> {
        let name_str = name.to_string();

        if name_str.ends_with(".wasm") {
            let js_code = format!("export default \"{}\";", name_str);
            return Module::declare(ctx.clone(), name, js_code);
        }

        let raw_code = if name.starts_with("http://") || name.starts_with("https://") {
            if let Some(cached) = get_module_cache().read().unwrap().get(name) {
                cached.clone()
            } else {
                let res = get_ureq_agent()
                    .get(name)
                    .call()
                    .map_err(|_| QjsError::Unknown)?;

                let downloaded = res.into_string().map_err(|_| QjsError::Unknown)?;

                get_module_cache()
                    .write()
                    .unwrap()
                    .insert(name_str.clone(), downloaded.clone());
                downloaded
            }
        // 2. Use the dynamic scope string here instead of hardcoded "root"
        } else if let Some(content) = self.vfs.get_file(&self.scope, name) {
            content
        } else if name.starts_with("./modules/") {
            let script_name = name
                .split('/')
                .last()
                .unwrap()
                .trim_end_matches(".js")
                .trim_end_matches(".ts")
                .to_string();

            // 3. This will now correctly be the Scoped DB (Tenant/Sandbox)
            let db = self.db.clone();

            let code_res = std::thread::spawn(move || {
                futures::executor::block_on(async { db.get_script_by_name(&script_name).await })
            })
            .join()
            .unwrap_or(Ok(None));

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

        let executable_code = transpile_ts(&raw_code, name).unwrap_or(raw_code);

        Module::declare(ctx.clone(), name, executable_code)
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

            zip.finish().map_err(|e| e.to_string())?;
        }
        Ok(buffer.into_inner())
    }

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
                        metadata: meta_val,
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
