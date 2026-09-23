// apexkit-watch.js - ApexKit Developer Tools, Sync Watcher & IntelliSense Engine
import fs from "fs";
import path from "path";
import * as readline from "readline/promises";

// --- 1. ENV LOADER ---
if (fs.existsSync(".env")) {
  const envConfig = fs.readFileSync(".env", "utf-8");
  envConfig.split("\n").forEach((line) => {
    const trimmed = line.trim();
    if (trimmed && !trimmed.startsWith("#")) {
      const [key, ...valueParts] = trimmed.split("=");
      if (key && valueParts.length > 0) {
        const val = valueParts.join("=").split("#")[0].trim();
        process.env[key.trim()] = val;
      }
    }
  });
}

// --- 2. CONFIG & CLI COMMAND PARSER ---
const API_KEY = process.env.APEXKIT_API_KEY || "root_sys_prod_12345678_abcd";
const SCOPE_KEY = process.env.SCOPE_KEY || "root";
const BASE_URL = process.env.APEXKIT_URL || `http://localhost:${process.env.PORT || 5000}`;
const WS_URL = `${BASE_URL.replace(/^http/, "ws")}/dev/sync?api_key=${API_KEY}&scope_key=${SCOPE_KEY}`;

const RAW_ARGS = process.argv.slice(2);
const SUBCOMMAND = RAW_ARGS.find((a) => !a.startsWith("-"))?.toLowerCase() || "";

const IS_INIT = RAW_ARGS.includes("--init") || SUBCOMMAND === "init";
const IS_INIT_FILE =
  RAW_ARGS.includes("--init-file") ||
  SUBCOMMAND === "create" ||
  SUBCOMMAND === "new" ||
  SUBCOMMAND === "init:file" ||
  SUBCOMMAND === "init-file";
const IS_PULL = RAW_ARGS.includes("--pull") || SUBCOMMAND === "pull";
const IS_COMMIT = RAW_ARGS.includes("--commit") || SUBCOMMAND === "commit";
const IS_PUSH = RAW_ARGS.includes("--push") || SUBCOMMAND === "push";
const IS_STATUS = RAW_ARGS.includes("--status") || SUBCOMMAND === "status" || SUBCOMMAND === "info";
const NO_AUTO_COMMIT = RAW_ARGS.includes("--no-auto-commit");

// --- CATEGORY FLAGS: --scripts | --templates | --actions | --all (Default: scripts) ---
const hasScriptsFlag = RAW_ARGS.includes("--scripts") || RAW_ARGS.includes("-s");
const hasTemplatesFlag = RAW_ARGS.includes("--templates") || RAW_ARGS.includes("-t");
const hasActionsFlag =
  RAW_ARGS.includes("--actions") || RAW_ARGS.includes("--ai-actions") || RAW_ARGS.includes("-a");
const hasAllFlag = RAW_ARGS.includes("--all") || RAW_ARGS.includes("-A");

let syncScripts = false;
let syncTemplates = false;
let syncActions = false;

if (hasAllFlag) {
  syncScripts = true;
  syncTemplates = true;
  syncActions = true;
} else if (hasScriptsFlag || hasTemplatesFlag || hasActionsFlag) {
  syncScripts = hasScriptsFlag;
  syncTemplates = hasTemplatesFlag;
  syncActions = hasActionsFlag;
} else {
  // Default to scripts only if no flag is specified
  syncScripts = true;
}

// Parse --delete-remote flag
const deleteRemoteArg = RAW_ARGS.find((a) => a.startsWith("--delete-remote"));
let DELETE_REMOTE_TYPES = [];
if (deleteRemoteArg) {
  const value = deleteRemoteArg.split("=")[1];
  if (!value || value === "all" || value === "true") {
    DELETE_REMOTE_TYPES = ["scripts", "templates", "ai_actions"];
  } else {
    DELETE_REMOTE_TYPES = value.split(",").map((s) => s.trim().toLowerCase());
  }
}

function getScopedApiBase() {
  let scopePath = "";
  if (SCOPE_KEY.startsWith("tenant:")) {
    scopePath = `/tenant/${SCOPE_KEY.replace("tenant:", "")}`;
  } else if (SCOPE_KEY.startsWith("sandbox:")) {
    scopePath = `/sandbox/${SCOPE_KEY.replace("sandbox:", "")}`;
  }
  return `${BASE_URL.replace(/\/$/, "")}${scopePath}/api/v1`;
}

function getAuthHeaders() {
  return {
    "x-api-key": API_KEY,
    Authorization: `Bearer ${API_KEY}`,
    "Content-Type": "application/json",
  };
}

// --- 3. TRIGGER DEFINITIONS ---
const TRIGGER_DEFINITIONS = [
  { id: "manual", name: "manual (Public/Protected HTTP Webhook Endpoint)", desc: "Triggered via HTTP GET/POST/PUT/DELETE on /api/v1/run/:name or /api/v1/webhook/:name" },
  { id: "before_create_record", name: "before_create_record", desc: "Runs before a record is inserted. Can validate or mutate data." },
  { id: "after_create_record", name: "after_create_record", desc: "Runs after a record is inserted. Used for notifications/indexes." },
  { id: "before_update_record", name: "before_update_record", desc: "Runs before an existing record is updated. Can validate changes." },
  { id: "after_update_record", name: "after_update_record", desc: "Runs after a record is updated." },
  { id: "before_delete_record", name: "before_delete_record", desc: "Runs before a record is deleted. Can block deletion." },
  { id: "after_delete_record", name: "after_delete_record", desc: "Runs after a record has been deleted." },
  { id: "before_list_records", name: "before_list_records", desc: "Intercepts list queries before they execute." },
  { id: "after_list_records", name: "after_list_records", desc: "Filters or transforms listed records before response." },
  { id: "before_get_record", name: "before_get_record", desc: "Runs before a single record is fetched." },
  { id: "after_get_record", name: "after_get_record", desc: "Transforms a fetched record before response." },
  { id: "before_user_login", name: "before_user_login", desc: "Runs before user authentication. Can verify IP/Rate limits." },
  { id: "after_user_login", name: "after_user_login", desc: "Runs after successful user login." },
  { id: "before_user_create", name: "before_user_create", desc: "Runs before user registration. Enforce roles or rules." },
  { id: "after_user_create", name: "after_user_create", desc: "Runs after user registration. Trigger welcome emails." },
  { id: "before_user_delete", name: "before_user_delete", desc: "Runs before a user is deleted." },
  { id: "after_user_delete", name: "after_user_delete", desc: "Runs after a user is deleted." },
  { id: "before_list_users", name: "before_list_users", desc: "Intercepts user list queries." },
  { id: "after_list_users", name: "after_list_users", desc: "Transforms user list responses." },
  { id: "before_collection_create", name: "before_collection_create", desc: "Validates collection schema before creation." },
  { id: "after_collection_create", name: "after_collection_create", desc: "Runs after collection creation." },
  { id: "before_collection_update", name: "before_collection_update", desc: "Runs before collection schema updates." },
  { id: "after_collection_update", name: "after_collection_update", desc: "Runs after collection schema updates." },
  { id: "before_collection_delete", name: "before_collection_delete", desc: "Runs before collection deletion." },
  { id: "before_file_upload", name: "before_file_upload", desc: "Runs before a file is written to storage." },
  { id: "after_file_upload", name: "after_file_upload", desc: "Runs after file upload. Can trigger virus scan or indexing." },
  { id: "before_file_delete", name: "before_file_delete", desc: "Runs before file is deleted." },
  { id: "after_file_delete", name: "after_file_delete", desc: "Runs after file is deleted." },
  { id: "before_relation_create", name: "before_relation_create", desc: "Runs before relationship is linked." },
  { id: "after_relation_create", name: "after_relation_create", desc: "Runs after relationship is linked." },
  { id: "before_relation_delete", name: "before_relation_delete", desc: "Runs before relationship is removed." },
  { id: "after_relation_delete", name: "after_relation_delete", desc: "Runs after relationship is removed." },
  { id: "before_tenant_create", name: "before_tenant_create", desc: "Runs before a tenant database is provisioned." },
  { id: "after_tenant_create", name: "after_tenant_create", desc: "Runs after tenant database is provisioned." },
  { id: "before_tenant_request", name: "before_tenant_request", desc: "Intercepts all incoming HTTP requests to a tenant." },
  { id: "after_tenant_request", name: "after_tenant_request", desc: "Processes tenant HTTP response metrics." },
  { id: "before_sandbox_request", name: "before_sandbox_request", desc: "Intercepts sandbox requests." },
  { id: "after_sandbox_request", name: "after_sandbox_request", desc: "Processes sandbox response metrics." },
  { id: "before_ai_run", name: "before_ai_run", desc: "Intercepts AI Prompt Actions to inject RAG search vectors." },
  { id: "after_ai_run", name: "after_ai_run", desc: "Runs after AI Prompt execution." },
  { id: "on_vectorization_start", name: "on_vectorization_start", desc: "Fires when revectorization job begins." },
  { id: "cron", name: "cron (Scheduled Job)", desc: "Executed periodically according to Cron expression settings." },
  { id: "graphql", name: "graphql (Custom Resolver)", desc: "Dynamic Query/Mutation field resolver for GraphQL schema." }
];

// --- 4. AUTOMATED REMOTE TYPE RESOLUTION & SCANNER ---
function scanWorkspaceForRemoteUrls() {
  const urls = new Set();
  const dirs = ["./webhooks", "./modules", "./templates"];
  const urlRegex =
    /(?:import|export)\s+(?:[\s\S]*?from\s+)?["'](https?:\/\/[^"']+)["']|import\s*\(\s*["'](https?:\/\/[^"']+)["']\s*\)|@jsxImportSource\s+(https?:\/\/[^\s*]+)/g;

  dirs.forEach((dir) => {
    if (!fs.existsSync(dir)) return;
    const files = fs.readdirSync(dir, { recursive: true });
    files.forEach((file) => {
      const fullPath = path.join(dir, file);
      if (fs.statSync(fullPath).isFile()) {
        const ext = path.extname(fullPath);
        if ([".js", ".ts", ".jsx", ".tsx", ".html"].includes(ext)) {
          try {
            const content = fs.readFileSync(fullPath, "utf-8");
            let match;
            while ((match = urlRegex.exec(content)) !== null) {
              const url = match[1] || match[2] || match[3];
              if (url) urls.add(url.trim());
            }
          } catch (e) {}
        }
      }
    });
  });

  return Array.from(urls);
}

async function resolveRemoteTypeUrl(importUrl) {
  try {
    const res = await fetch(importUrl, {
      method: "GET",
      headers: { "User-Agent": "ApexKit-CLI/1.0" },
      redirect: "follow",
    });

    const dtsHeader = res.headers.get("x-typescript-types");
    if (dtsHeader) {
      return dtsHeader.startsWith("http") ? dtsHeader : new URL(dtsHeader, importUrl).href;
    }

    if (importUrl.endsWith(".d.ts")) {
      return importUrl;
    }

    if (importUrl.includes("esm.sh")) {
      const dtsParamUrl = importUrl.includes("?") ? `${importUrl}&dts` : `${importUrl}?dts`;
      const paramRes = await fetch(dtsParamUrl, { method: "HEAD", redirect: "follow" });
      const paramHeader = paramRes.headers.get("x-typescript-types");
      if (paramHeader) {
        return paramHeader.startsWith("http") ? paramHeader : new URL(paramHeader, importUrl).href;
      }
    }
  } catch (e) {}
  return null;
}

/**
 * Recursively fetches a .d.ts file and all relative imports/exports inside it
 */
async function downloadDtsTree(currentDtsUrl, baseDir, relativeSubpath = "index.d.ts", visited = new Set()) {
  if (visited.has(currentDtsUrl)) return;
  visited.add(currentDtsUrl);

  const localFilePath = path.join(baseDir, relativeSubpath);
  fs.mkdirSync(path.dirname(localFilePath), { recursive: true });

  try {
    const res = await fetch(currentDtsUrl, {
      headers: { "User-Agent": "ApexKit-CLI/1.0" },
      redirect: "follow",
    });
    if (!res.ok) return;

    let content = await res.text();

    // Regex to locate relative imports or exports like:
    // import { Hono } from './hono.d.ts';
    // export * from './types.d.ts';
    const relImportRegex = /(?:import|export)\s+[\s\S]*?from\s+["'](\.[^"']+)["']/g;
    const relMatches = [];
    let match;
    while ((match = relImportRegex.exec(content)) !== null) {
      relMatches.push(match[1]);
    }

    // Write file locally
    fs.writeFileSync(localFilePath, content);

    // Recursively pull all dependencies relative to current URL
    for (const relPath of relMatches) {
      // Normalize extension: ensure it resolves to .d.ts on the CDN
      let targetRel = relPath;
      if (!targetRel.endsWith(".d.ts") && !targetRel.endsWith(".ts")) {
        targetRel = `${targetRel}.d.ts`;
      }

      const nextUrl = new URL(targetRel, currentDtsUrl).href;
      const nextLocalSubpath = path.normalize(
        path.join(path.dirname(relativeSubpath), targetRel.replace(/^\.\//, ""))
      );

      await downloadDtsTree(nextUrl, baseDir, nextLocalSubpath, visited);
    }
  } catch (err) {
    console.warn(`  ⚠️ Failed to recursively pull: ${currentDtsUrl} -> ${err.message}`);
  }
}

async function pullRemoteModuleTypes(urls) {
  const typesDir = path.join(".apexkit", "types");
  fs.mkdirSync(typesDir, { recursive: true });

  const resolvedPaths = {};

  for (const rawUrl of urls) {
    try {
      // Use directory container for each package to support relative child imports cleanly
      const urlFolder = rawUrl.replace(/[^a-zA-Z0-9_.-]/g, "_");
      const packageDir = path.join(typesDir, urlFolder);
      const rootDtsFile = path.join(packageDir, "index.d.ts");

      if (fs.existsSync(rootDtsFile) && fs.statSync(rootDtsFile).size > 0) {
        resolvedPaths[rawUrl] = `./${rootDtsFile.replace(/\\/g, "/")}`;
        continue;
      }

      console.log(`  🌐 [Types]: Resolving remote type tree for ${rawUrl}...`);
      const dtsUrl = await resolveRemoteTypeUrl(rawUrl);

      if (dtsUrl) {
        fs.mkdirSync(packageDir, { recursive: true });
        await downloadDtsTree(dtsUrl, packageDir, "index.d.ts");

        if (fs.existsSync(rootDtsFile)) {
          resolvedPaths[rawUrl] = `./${rootDtsFile.replace(/\\/g, "/")}`;
          console.log(`  📦 [Types]: Successfully cached dependency tree in ${packageDir}`);
        }
      }
    } catch (err) {
      console.warn(`  ⚠️ Could not pull remote types for ${rawUrl}: ${err.message}`);
    }
  }

  return resolvedPaths;
}

// --- 5. ACCURATE WORKSPACE TYPES & CONFIG GENERATOR ---
async function generateWorkspaceFiles() {
  console.log("  ⏳ Fetching collection schemas from DB...");
  let collectionsData = [];
  try {
    const fetchUrl = `${getScopedApiBase()}/collections`;
    const res = await fetch(fetchUrl, { headers: getAuthHeaders() });
    if (res.ok) {
      collectionsData = await res.json();
    }
  } catch (err) {
    console.warn(`  ⚠️ Could not connect to DB for schemas: ${err.message}`);
  }

  // Pre-calculate lookup maps for collection IDs and Names
  const colIdToName = new Map();
  const colNameToId = new Map();
  collectionsData.forEach((col) => {
    colIdToName.set(String(col.id), col.name);
    colNameToId.set(col.name, col.id);
  });

  // Calculate Reverse Relations: Target Collection Name -> Array of Reverse Link Definitions
  const reverseRelationsMap = new Map();
  collectionsData.forEach((originCol) => {
    if (originCol.schema?.relations) {
      for (const [relName, relDef] of Object.entries(originCol.schema.relations)) {
        const rawTarget = relDef.target_collection;
        const targetName = colIdToName.get(String(rawTarget)) || rawTarget;

        if (!reverseRelationsMap.has(targetName)) {
          reverseRelationsMap.set(targetName, []);
        }
        reverseRelationsMap.get(targetName).push({
          originCollection: originCol.name,
          relationFieldName: relName,
          cardinality: relDef.relation_type, // 'many' or 'one'
        });
      }
    }
  });

  let collectionTypesStr = `export interface Collections {\n`;
  let collectionExpandsStr = `export interface CollectionExpands {\n`;
  const collectionNames = [];

  collectionsData.forEach((col) => {
    collectionNames.push(`"${col.name}"`);
    let fieldsStr = "";
    let expandsStr = "";

    if (col.schema) {
      // 1. STANDARD FIELDS
      if (col.schema.fields) {
        for (const [name, def] of Object.entries(col.schema.fields)) {
          let tsType = "any";
          switch (def.type) {
            case "string":
            case "text":
            case "email":
            case "url":
            case "date":
            case "blob":
            case "file":
              tsType = "string";
              break;
            case "number":
              tsType = "number";
              break;
            case "owner":
            case "relation":
              // ✅ Foreign keys and owners are ALWAYS scalar IDs on the record itself
              tsType = "number | string";
              break;
            case "boolean":
              tsType = "boolean";
              break;
            case "vector":
              tsType = "number[]";
              break;
            case "geopoint":
              tsType = "{ lat: number; lng: number }";
              break;
            case "json":
              tsType = "Record<string, any> | any[]";
              break;
            case "select":
              tsType =
                def.options && def.options.length > 0
                  ? def.options.map((o) => `"${o}"`).join(" | ")
                  : "string";
              break;
          }
          const safeName = /^[a-zA-Z_$][a-zA-Z0-9_$]*$/.test(name) ? name : `"${name}"`;
          fieldsStr += `    ${safeName}${def.required ? "" : "?"}: ${tsType};\n`;

          // If it's an Owner field, it expands to the User Profile map
          if (def.type === "owner") {
            expandsStr += `    ${safeName}?: { id: number | string; email: string; role: string; metadata?: Record<string, any> };\n`;
          }
        }
      }

      // 2. FORWARD RELATIONS (Directly on this collection)
      if (col.schema.relations) {
        for (const [name, rel] of Object.entries(col.schema.relations)) {
          const rawTarget = rel.target_collection;
          const targetColName = colIdToName.get(String(rawTarget)) || rawTarget;
          const safeName = /^[a-zA-Z_$][a-zA-Z0-9_$]*$/.test(name) ? name : `"${name}"`;

          // ✅ BULLETPROOF RULE 1:
          // A record holding the foreign key field stores a SCALAR foreign key ID, NEVER an array.
          fieldsStr += `    ${safeName}${rel.required ? "" : "?"}: number | string;\n`;

          // ✅ BULLETPROOF RULE 2:
          // Forward expansion from the foreign key field points to ONE target record.
          // It is ALWAYS a Map/Object, NEVER an array!
          const targetRecordType = `Collections["${targetColName}"]`;
          const expandSingleType = `{ id: number | string; data: ${targetRecordType}; created: string; updated: string; expand?: any }`;

          expandsStr += `    ${safeName}?: ${expandSingleType};\n`;
        }
      }

      // 3. REVERSE RELATIONS (Parent expanding children)
      const reverseLinks = reverseRelationsMap.get(col.name) || [];
      const fieldCountByName = new Map();
      reverseLinks.forEach((rev) => {
        fieldCountByName.set(rev.originCollection, (fieldCountByName.get(rev.originCollection) || 0) + 1);
      });

      reverseLinks.forEach((rev) => {
        const isDuplicate = (fieldCountByName.get(rev.originCollection) || 0) > 1;
        const revKey = isDuplicate ? `${rev.originCollection}_via_${rev.relationFieldName}` : rev.originCollection;
        const safeRevKey = /^[a-zA-Z_$][a-zA-Z0-9_$]*$/.test(revKey) ? revKey : `"${revKey}"`;

        const originRecordType = `Collections["${rev.originCollection}"]`;
        const childRecordType = `{ id: number | string; data: ${originRecordType}; created: string; updated: string; expand?: any }`;

        // ✅ BULLETPROOF RULE 3:
        // Only return an Array when reverse-expanded from parent AND cardinality is 'many'.
        // If the reverse relation is unique ('one'), return a single Map/Object.
        if (rev.cardinality === "one") {
          expandsStr += `    ${safeRevKey}?: ${childRecordType};\n`;
        } else {
          expandsStr += `    ${safeRevKey}?: Array<${childRecordType}>;\n`;
        }
      });
    }

    collectionTypesStr += `  "${col.name}": {\n${fieldsStr}  };\n`;
    collectionExpandsStr += `  "${col.name}": {\n${expandsStr}    [reverse_relation: string]: any;\n  };\n`;
  });

  if (collectionsData.length === 0) {
    collectionTypesStr += `  [key: string]: Record<string, any>;\n`;
    collectionExpandsStr += `  [key: string]: any;\n`;
  }
  collectionTypesStr += `}\n\nexport type CollectionName = ${
    collectionNames.length > 0 ? collectionNames.join(" | ") : "string"
  };\n`;
  collectionExpandsStr += `}\n\n`;

  const triggerUnion = TRIGGER_DEFINITIONS.map((t) => `  /** ${t.desc} */\n  | "${t.id}"`).join("\n");

  const types = `/**
 * ApexKit Global Types & IntelliSense
 * Auto-generated by apexkit-watch.js
 */

${collectionTypesStr}
${collectionExpandsStr}

export type TriggerType =
${triggerUnion};

export type ScriptType = "webhook" | "custom:module" | "esm:module" | "template" | "ai_action";
export type ScriptVisibility = "private" | "public";

export interface FileMetadata {
  id?: number;
  name?: string;
  extension?: "js" | "ts" | "html" | "json" | string;
  target_collection?: CollectionName | null;
  type?: ScriptType;
  path?: string;
  trigger_type?: TriggerType;
  active?: boolean;
  visibility?: ScriptVisibility;
}

export type ApexKitDb = typeof $db;
export type ApexKitHttp = typeof $http;
export type ApexKitFiles = typeof $files;
export type ApexKitFs = typeof $fs;
export type ApexKitCache = typeof $cache;
export type ApexKitEnv = typeof $env;

export interface AuthContext {
  id: number | string;
  email: string;
  role: string;
  scope: string;
}

export interface RecordItem<T extends keyof Collections = any> {
  id: number | string;
  data: T extends keyof Collections ? Collections[T] : any;
  created: string;
  updated: string;
  expand?: T extends keyof CollectionExpands ? CollectionExpands[T] : any;
}

export interface RecordHookEvent<T extends keyof Collections = any> {
  trigger: TriggerType;
  record: {
    id: number | string | null;
    data: T extends keyof Collections ? Collections[T] : any;
  };
  collection: {
    id: number;
    name: string;
    schema?: any;
  };
  auth: AuthContext | null;
}

export interface VoidHookEvent {
  trigger: TriggerType;
  data: any;
  auth: AuthContext | null;
  timestamp: string;
}

declare global {
  namespace JSX {
    interface Element extends Promise<string>, String {
      [key: string]: any;
      toString(): string;
    }
    interface IntrinsicElements {
      [elemName: string]: any;
    }
    interface ElementChildrenAttribute {
      children: {};
    }
  }

  const __fileMetadata__: FileMetadata;

  /** ApexKit Database Client */
  const $db: {
    records: {
      list<T extends keyof Collections>(
        collection: T,
        options?: {
          page?: number;
          per_page?: number;
          limit?: number;
          offset?: number;
          sort?: string;
          filter?: string | Record<string, any>;
          expand?: string;
          fields?: string;
        }
      ): Promise<{ items: RecordItem<T>[]; total: number }>;

      get<T extends keyof Collections>(
        collection: T,
        id: number | string,
        expand?: string
      ): Promise<RecordItem<T> | null>;

      create<T extends keyof Collections>(
        collection: T,
        data: Partial<Collections[T]>
      ): Promise<{ id: number | string }>;

      update<T extends keyof Collections>(
        collection: T,
        id: number | string,
        data: Partial<Collections[T]>
      ): Promise<RecordItem<T>>;

      delete(collection: string, id: number | string): Promise<boolean>;

      searchVector<T extends keyof Collections>(
        collection: T,
        field: string,
        vector: number[],
        limit?: number
      ): Promise<(RecordItem<T> & { _score: number })[]>;

      getVector(
        collection: string,
        id: number | string
      ): Promise<{ field_name: string; vector: number[]; model: string }[]>;

      instantSearch(
        collection: string,
        query: string,
        limit?: number
      ): Promise<{ id: number | string; score: number; snippet: any }[]>;
    };

    query(queryObject: {
      from: string;
      select?: any[];
      where?: Record<string, any>;
      group_by?: string[];
      sort?: string;
      limit?: number;
      offset?: number;
      system?: boolean;
      pipeline?: any[];
    }): Promise<any[]>;

    users: {
      create(
        email: string,
        passwordHash: string,
        role: string,
        metadata?: Record<string, any>
      ): Promise<{ id: number | string; email: string; role: string }>;
      get(email: string): Promise<{ id: number | string; email: string; role: string; metadata?: any } | null>;
    };

    collections: {
      list(): Promise<{ id: number; name: string; schema?: any; index?: string }[]>;
    };

    files: {
      list(
        limit?: number,
        offset?: number
      ): Promise<
        { id: number | string; filename: string; original_name: string; mime_type: string; size: number; created_at: string }[]
      >;
    };
  };

  /** Universal HTTP Client */
  const $http: {
    get(url: string): Promise<string>;
    post(url: string, body: any): Promise<string>;
  };

  /** Native Storage & File Engine */
  const $files: {
    read(filename: string): Promise<string>;
    delete(filenameOrId: string | number): Promise<boolean>;
    save(
      filename: string,
      data: string | ArrayBuffer | Uint8Array,
      mime?: string
    ): Promise<{ id: number | string; url: string; filename: string }>;
    getSignedUrl(filename: string, ttl_secs?: number): Promise<string>;
  };

  /** Scoped Virtual File System */
  const $fs: {
    read(path: string): Promise<string>;
    readBytes(path: string): Promise<string>;
    write(path: string, content: string): Promise<boolean>;
    writeBytes(path: string, data: string | ArrayBuffer | Uint8Array): Promise<boolean>;
    delete(path: string): Promise<boolean>;
    list(path: string): Promise<{ name: string; isDir: boolean; size: number }[]>;
    exists(path: string): Promise<boolean>;
    mkdir(path: string): Promise<boolean>;
    stat(path: string): Promise<{ size: number; isDir: boolean; created?: number; modified?: number }>;
  };

  /** In-Memory & Tenant-Isolated Fast Cache */
  const $cache: {
    get(key: string): Promise<string | null>;
    set(key: string, value: any, ttl_secs?: number): Promise<void>;
    delete(key: string): Promise<void>;
    del(key: string): Promise<void>;
    incr(key: string, delta?: number): Promise<number>;
    listKeys(): Promise<string[]>;
  };

  /** Outbound SMTP Email Dispatcher */
  const $mail: {
    send(to: string, subject: string, body: string): Promise<boolean>;
  };

  /** Real-time WebSocket & SSE Event Dispatcher */
  const $realtime: {
    send(channel: string, event: string, data: any): Promise<boolean>;
  };

  /** Script Inter-calling Engine */
  const $run: {
    script(name: string, payload: any): Promise<any>;
  };

  /** Cryptographic & String Utilities */
  const $util: {
    uuid(): string;
    slugify(text: string): string;
    hash(text: string, alg: "sha256" | "sha512"): string;
    hmac(text: string, key: string): string;
    base64Encode(data: string | ArrayBuffer | Uint8Array): string;
    base64EncodeBuffer(buffer: ArrayBuffer | Uint8Array): string;
    base64Decode(text: string): string;
    base64DecodeBuffer(text: string): ArrayBuffer;
    textEncode(text: string): ArrayBuffer;
    textDecode(data: ArrayBuffer | Uint8Array): string;
    hexEncode(data: string | ArrayBuffer | Uint8Array): string;
    hexDecode(hex: string): ArrayBuffer;
    sleep(ms: number): Promise<void>;
    randomHex(len?: number): string;
  };

  /** Standard Web APIs */
  const crypto: {
    randomUUID(): string;
    getRandomValues<T extends Int8Array | Int16Array | Int32Array | Uint8Array | Uint16Array | Uint32Array | Uint8ClampedArray | Float32Array | Float64Array | DataView | null>(array: T): T;
    subtle: {
      digest(algorithm: string | any, data: ArrayBuffer | Uint8Array): Promise<ArrayBuffer>;
      importKey(format: string, keyData: any, algo: any, extractable: boolean, usages: string[]): Promise<any>;
      sign(algorithm: string | any, key: any, data: ArrayBuffer | Uint8Array): Promise<ArrayBuffer>;
      verify(algorithm: string | any, key: any, signature: ArrayBuffer | Uint8Array, data: ArrayBuffer | Uint8Array): Promise<boolean>;
    }
  };

  class Blob {
    constructor(sources?: any[], options?: any);
    arrayBuffer(): Promise<ArrayBuffer>;
    text(): Promise<string>;
    readonly size: number;
    readonly type: string;
  }

  class ReadableStream {
    constructor(underlyingSource?: any);
    getReader(): any;
    cancel(): Promise<void>;
    readonly locked: boolean;
  }

  const btoa: (data: string) => string;
  const atob: (data: string) => string;

  class TextEncoder {
    constructor(encoding?: string);
    encode(input?: string): Uint8Array;
    encodeInto(source: string, destination: Uint8Array): { read: number; written: number };
  }

  class TextDecoder {
    constructor(label?: string, options?: { fatal?: boolean; ignoreBOM?: boolean });
    decode(input?: ArrayBuffer | Uint8Array, options?: { stream?: boolean }): string;
  }

  function setTimeout(callback: Function, ms: number): string;
  function clearTimeout(id: string): void;

  /** Environment & Configuration Secrets Accessor */
  const $env: {
    get(key: string): Promise<string>;
    has(key: string): Promise<boolean>;
    list(): Promise<Record<string, string>>;
    readonly BASE_URL: string;
    readonly APP_URL: string;
    readonly LOCAL_BASE_URL: string;
    readonly LOCAL_APP_URL: string;
    readonly SMTP_BLOCKED: boolean;
  };

  function fetch(
    input: string | Request | URL,
    init?: { method?: string; headers?: any; body?: any; redirect?: "follow" | "manual" | "error" }
  ): Promise<Response>;
}

// --- RUNTIME MODULE DEFINITIONS ---
declare module "react/jsx-runtime" {
  export const jsx: any;
  export const jsxs: any;
  export const Fragment: any;
}

declare module "react/jsx-dev-runtime" {
  export const jsxDEV: any;
  export const Fragment: any;
}

declare module "hono/jsx/jsx-runtime" {
  export const jsx: any;
  export const jsxs: any;
  export const Fragment: any;
}

declare module "hono/jsx" {
  export const jsx: any;
  export const jsxs: any;
  export const Fragment: any;
  export type FC<T = any> = (props: T) => any;
}

// --- HONO CONTEXT OVERLOADS ---
declare module "hono" {
  interface Context {
    html(html: string | Promise<string> | JSX.Element | any, status?: any, headers?: any): Response | Promise<Response>;
  }
}

// --- REMOTE URL MODULE DECLARATIONS & FALLBACKS ---
declare module "https://*" {
  const content: any;
  export default content;
  export * from "https://*";
}

declare module "http://*" {
  const content: any;
  export default content;
  export * from "http://*";
}

export {};
`;
  fs.writeFileSync("apexkit.d.ts", types.trim());
  console.log("  📄 Generated apexkit.d.ts with Accurate Scaled Typings & Universal JSX");

  // Dynamically scan all files and auto-pull remote module types
  const remoteUrls = scanWorkspaceForRemoteUrls();
  const resolvedRemoteTypes = await pullRemoteModuleTypes(remoteUrls);

  const dynamicPaths = {
    "@/custom/*": ["./modules/custom/*"],
    "@/esm/*": ["./modules/esm/*"],
  };

  // Map each pulled remote URL to its local cached definition
  for (const [remoteUrl, localPath] of Object.entries(resolvedRemoteTypes)) {
    dynamicPaths[remoteUrl] = [localPath];
    dynamicPaths[`${remoteUrl}/*`] = [`${localPath}/*`];
  }

  const jsconfig = {
    compilerOptions: {
      target: "ES2022",
      module: "ESNext",
      ignoreDeprecations: "6.0",
      moduleResolution: "bundler",
      allowImportingTsExtensions: true,
      jsx: "preserve",
      baseUrl: ".",
      paths: dynamicPaths,
      allowJs: true,
      checkJs: true,
    },
    include: [
      "**/*.js",
      "**/*.ts",
      "**/*.jsx",
      "**/*.tsx",
      "**/*.html",
      "apexkit.d.ts",
      ".apexkit/types/**/*.d.ts",
    ],
  };
  fs.writeFileSync("jsconfig.json", JSON.stringify(jsconfig, null, 2));
  console.log("  📄 Generated jsconfig.json with Auto-Mapped Remote Types & JSX Support");
}

// --- 5. INITIAL CODE GENERATOR ---
function getBoilerplateCode(fileType, cleanName, ext, triggerType, targetCollection) {
  const metadataPath =
    fileType === "template"
      ? "./templates/"
      : fileType.includes("module")
      ? "./modules/custom/"
      : "./webhooks/";

  const typeImportPath = metadataPath.includes("modules/") ? "../../apexkit" : "../apexkit";

  const metadata = {
    name: cleanName,
    extension: ext,
    target_collection: targetCollection,
    type: fileType,
    path: metadataPath,
    trigger_type: triggerType,
    active: true,
    visibility: "private",
  };

  const header = `/** @type {import("${typeImportPath}").FileMetadata} */\nexport const __fileMetadata__ = ${JSON.stringify(
    metadata,
    null,
    2
  )};\n\n`;

  if (fileType === "custom:module") {
    return `${header}/**
 * Reusable Helper Module: ${cleanName}
 * Import elsewhere via: import { ${cleanName.replace(/-/g, "_")} } from "@/custom/${cleanName}";
 */
export function ${cleanName.replace(/-/g, "_")}(data = {}) {
  return {
    success: true,
    processed_at: new Date().toISOString(),
    data
  };
}
`;
  }

  if (fileType === "template") {
    return `<!--
__fileMetadata__ = ${JSON.stringify(metadata, null, 2)}
-->
<script server>
/** @type {import("../apexkit").ApexKitDb} */
const db = $db;

export default async function (context) {
  const posts = await db.records.list(\`${targetCollection || "posts"}\`, { limit: 10 }).catch(() => ({ items: [] }));
  return {
    title: \`${cleanName.replace(/-/g, " ").toUpperCase()}\`,
    items: posts.items
  };
}
</script>

<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>{{ title }}</title>
  <script src="https://cdn.tailwindcss.com"></script>
  <script src="/static/js/htmx.js"></script>
  <script src="/static/js/alpine.js" defer></script>
</head>
<body class="bg-slate-900 text-slate-100 min-h-screen p-8">
  <div class="max-w-4xl mx-auto space-y-6">
    <h1 class="text-3xl font-bold text-indigo-400">{{ title }}</h1>
    
    <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
      {% for post in posts %}
      <div class="p-4 bg-slate-800 rounded-lg border border-slate-700 shadow">
        <h2 class="text-xl font-semibold">{{ post.data.title | default(value="Item #" ~ post.id) }}</h2>
        <p class="text-slate-400 text-sm mt-2">{{ post.created }}</p>
      </div>
      {% endfor %}
    </div>
  </div>
</body>
</html>
`;
  }

  switch (triggerType) {
    case "manual":
      return `${header}import { Hono } from "https://esm.sh/hono";

const app = new Hono();

app.all("/", async (c) => {
  const body = await c.req.json().catch(() => ({}));
  return c.json({
    success: true,
    method: c.req.method,
    path: c.req.path,
    received: body
  });
});

export default async function (req) {
  return app.fetch(req);
}
`;

    case "before_create_record":
    case "before_update_record":
      return `${header}/**
 * Hook: ${triggerType}
 * Collection: ${targetCollection || "All Collections"}
 * 
 * @param {import("../apexkit").RecordHookEvent} event
 * @returns {Promise<any>} Return modified record object or throw an Error to block operation
 */
export default async function (event) {
  const data = event.record.data;
  return data;
}
`;

    case "after_create_record":
    case "after_update_record":
      return `${header}/**
 * Hook: ${triggerType}
 * Collection: ${targetCollection || "All Collections"}
 * 
 * @param {import("../apexkit").RecordHookEvent} event
 */
export default async function (event) {
  const { id, data } = event.record;
  await $realtime.send(\`${targetCollection || "general"}\`, "record_changed", {
    action: \`${triggerType}\`,
    record_id: id,
    data
  });
}
`;

    default:
      return `${header}/**
 * Trigger: ${triggerType}
 * @param {any} event
 */
export default async function (event) {
  console.log(\`Hook ${triggerType} executed in ${cleanName}\`);
  return true;
}
`;
  }
}

// --- 6. INTERACTIVE FILE WIZARD ---
async function runInitFile() {
  const rl = readline.createInterface({ input: process.stdin, output: process.stdout });

  console.log("🚀 ApexKit Interactive File Creation Wizard\n");

  console.log("Select File Category:");
  console.log("  1) Webhook / API Endpoint / System Event Hook");
  console.log("  2) Custom Reusable Module (imported via @/custom/...)");
  console.log("  3) SSR HTML / Tera Page Template");
  const catChoice = (await rl.question("\nChoice (1-3) [1]: ")).trim() || "1";

  let fileType = "webhook";
  let targetDir = "./webhooks/";

  if (catChoice === "2") {
    fileType = "custom:module";
    targetDir = "./modules/custom/";
  } else if (catChoice === "3") {
    fileType = "template";
    targetDir = "./templates/";
  }

  const rawName = (await rl.question("\nEnter File Name (e.g. process-order): ")).trim();
  if (!rawName) {
    console.error("❌ File name is required!");
    rl.close();
    process.exit(1);
  }

  const cleanName = rawName.replace(/[^a-zA-Z0-9_-]/g, "");
  const allowedScriptExts = ["js", "ts", "jsx", "tsx"];
  const rawExt =
    fileType === "template"
      ? "html"
      : ((await rl.question("Extension (js/ts/jsx/tsx) [js]: ")).trim().toLowerCase() || "js");
  const ext = fileType === "template" ? "html" : (allowedScriptExts.includes(rawExt) ? rawExt : "js");

  let triggerType = "manual";
  let targetCollection = null;

  if (fileType === "webhook") {
    console.log("\nSelect Trigger Type:");
    TRIGGER_DEFINITIONS.forEach((t, i) => {
      console.log(`  ${String(i + 1).padStart(2, " ")}) ${t.name}`);
    });

    const trigChoice = parseInt((await rl.question("\nChoice (1-42) [1]: ")).trim() || "1", 10);
    const selectedTrig = TRIGGER_DEFINITIONS[trigChoice - 1] || TRIGGER_DEFINITIONS[0];
    triggerType = selectedTrig.id;

    if (triggerType.includes("_record") || triggerType === "before_ai_run") {
      const colInput = (await rl.question("\nTarget Collection Name (leave blank for all): ")).trim();
      targetCollection = colInput || null;
    }
  } else if (fileType === "template") {
    const colInput = (
      await rl.question("\nPrimary Collection to Query (e.g. posts, leave blank for none): ")
    ).trim();
    targetCollection = colInput || null;
  }

  const codeBoilerplate = getBoilerplateCode(fileType, cleanName, ext, triggerType, targetCollection);

  fs.mkdirSync(targetDir, { recursive: true });
  const fullFilePath = path.join(targetDir, `${cleanName}.${ext}`);
  fs.writeFileSync(fullFilePath, codeBoilerplate);

  console.log(`\n✨ Successfully created: ${fullFilePath}`);
  console.log(`🚀 Save this file while \`node apexkit-watch.js\` is running to sync live to ApexKit!`);

  rl.close();
  process.exit(0);
}

// --- 7. WORKSPACE INIT WIZARD ---
async function runInit() {
  console.log("🚀 Welcome to ApexKit Local Workspace Setup!\n");
  const rl = readline.createInterface({ input: process.stdin, output: process.stdout });

  const url = await rl.question(`1. Enter ApexKit Server URL (${BASE_URL}): `);
  const finalUrl = url.trim() || BASE_URL;

  const key = await rl.question(`2. Enter API Key (${API_KEY}): `);
  const finalKey = key.trim() || API_KEY;

  const scope = await rl.question(`3. Enter Scope (root, tenant:id, sandbox:id) [${SCOPE_KEY}]: `);
  const finalScope = scope.trim() || SCOPE_KEY;

  const envContent = `APEXKIT_URL=${finalUrl}\nAPEXKIT_API_KEY=${finalKey}\nSCOPE_KEY=${finalScope}\n`;
  fs.writeFileSync(".env", envContent);
  console.log("\n✅ Saved .env file!");

  const dirs = ["webhooks", "modules/custom", "modules/esm", "templates", "ai_actions", "schemas"];
  dirs.forEach((d) => {
    fs.mkdirSync(d, { recursive: true });
    console.log(`  📁 Created ./${d}`);
  });

  process.env.APEXKIT_URL = finalUrl;
  process.env.APEXKIT_API_KEY = finalKey;
  process.env.SCOPE_KEY = finalScope;

  await generateWorkspaceFiles();

  console.log("\n🎉 Workspace Initialized! Run `node apexkit-watch.js pull` to download remote files.");
  rl.close();
  process.exit(0);
}

// --- 8. COMPREHENSIVE PULL ---
async function runPull() {
  console.log(`📥 Pulling all collections, scripts, templates, and AI actions from ApexKit (${SCOPE_KEY})...\n`);

  const apiBase = getScopedApiBase();
  const headers = getAuthHeaders();

  const dirs = ["webhooks", "modules/custom", "modules/esm", "templates", "ai_actions", "schemas"];
  dirs.forEach((d) => fs.mkdirSync(d, { recursive: true }));

  let pulledCount = 0;

  // 1. Pull Collections & Schema
  try {
    const colRes = await fetch(`${apiBase}/collections`, { headers });
    if (colRes.ok) {
      const collections = await colRes.json();
      fs.writeFileSync("schemas/collections.json", JSON.stringify(collections, null, 2));
      console.log(`  📦 [Collections]: Saved schemas/collections.json (${collections.length} collections)`);
      pulledCount += collections.length;
    }

    const schemaRes = await fetch(`${apiBase}/admin/export-schema`, { headers });
    if (schemaRes.ok) {
      const schemaJson = await schemaRes.json();
      fs.writeFileSync("schemas/apex_schema.json", JSON.stringify(schemaJson, null, 2));
      console.log(`  📦 [Schema]: Saved schemas/apex_schema.json`);
    }
  } catch (e) {
    console.warn(`  ⚠️ Schema pull warning: ${e.message}`);
  }

  // 2. Pull Scripts & Modules
  try {
    const scriptRes = await fetch(`${apiBase}/admin/scripts`, { headers });
    if (scriptRes.ok) {
      const payload = await scriptRes.json();
      const scripts = Array.isArray(payload) ? payload : payload.local || [];

      const allowedScriptExts = ["js", "ts", "jsx", "tsx"];

      scripts.forEach((script) => {
        const remoteExt = script.metadata?.extension?.toLowerCase();
        const extension = allowedScriptExts.includes(remoteExt) ? remoteExt : "js";

        const meta = {
          id: script.id,
          name: script.name,
          extension: extension,
          target_collection: script.target_collection || null,
          type: script.metadata?.type || "webhook",
          path: script.metadata?.path || "./webhooks/",
          trigger_type: script.trigger_type || "manual",
          active: script.active !== undefined ? script.active : true,
          visibility: script.visibility || "private",
        };

        const targetDir = meta.path.replace(/^\.\//, "");
        fs.mkdirSync(targetDir, { recursive: true });

        const typeImportPath = meta.path.includes("modules/") ? "../../apexkit" : "../apexkit";

        const formattedContent = `/** @type {import("${typeImportPath}").FileMetadata} */\nexport const __fileMetadata__ = ${JSON.stringify(
          meta,
          null,
          2
        )};\n\n${script.code}`;

        const filePath = path.join(targetDir, `${script.name}.${meta.extension}`);
        fs.writeFileSync(filePath, formattedContent);
        console.log(`  📜 [Script]: ${filePath}`);
        pulledCount++;
      });
    }
  } catch (e) {
    console.warn(`  ⚠️ Scripts pull warning: ${e.message}`);
  }

  // 3. Pull Templates
  try {
    const tmplRes = await fetch(`${apiBase}/admin/templates`, { headers });
    if (tmplRes.ok) {
      const templates = await tmplRes.json();
      templates.forEach((tmpl) => {
        const meta = {
          name: tmpl.slug,
          extension: "html",
          type: "template",
          path: "./templates/",
          active: true,
        };

        const formattedContent = `<!--\n__fileMetadata__ = ${JSON.stringify(
          meta,
          null,
          2
        )}\n-->\n${tmpl.content}`;

        const filePath = path.join("templates", `${tmpl.slug}.html`);
        fs.writeFileSync(filePath, formattedContent);
        console.log(`  🎨 [Template]: ${filePath}`);
        pulledCount++;
      });
    }
  } catch (e) {
    console.warn(`  ⚠️ Templates pull warning: ${e.message}`);
  }

  // 4. Pull AI Actions
  try {
    const aiRes = await fetch(`${apiBase}/admin/ai/actions`, { headers });
    if (aiRes.ok) {
      const actions = await aiRes.json();
      actions.forEach((action) => {
        const filePath = path.join("ai_actions", `${action.slug}.json`);
        fs.writeFileSync(filePath, JSON.stringify({ action }, null, 2));
        console.log(`  🤖 [AI Action]: ${filePath}`);
        pulledCount++;
      });
    }
  } catch (e) {
    console.warn(`  ⚠️ AI Actions pull warning: ${e.message}`);
  }

  // Ensure .apexkit/types is ready and regenerate workspace definitions + remote types
  fs.mkdirSync(path.join(".apexkit", "types"), { recursive: true });
  await generateWorkspaceFiles();

  console.log(`\n✨ Pull complete! Hydrated ${pulledCount} items and resolved remote types.`);
  process.exit(0);
}

// --- 9. METADATA AUTO-INFERENCE HELPER ---
function inferFileMetadata(filePath, content) {
  const ext = path.extname(filePath).replace(/^\./, "");
  const baseName = path.basename(filePath, `.${ext}`);
  const normalizedPath = filePath.replace(/\\/g, "/");

  // Ignore system/hidden files
  if (baseName.startsWith(".") || ext === "gitkeep" || baseName === ".gitkeep") {
    return null;
  }

  // 1. JSON files (AI Actions) -> Pure JSON
  if (normalizedPath.includes("ai_actions/") || ext === "json") {
    try {
      const parsed = JSON.parse(content);
      return JSON.stringify(parsed, null, 2);
    } catch (e) {
      return content;
    }
  }

  // 2. HTML files (Templates) -> HTML Comment metadata
  if (normalizedPath.includes("templates/") || ext === "html") {
    const htmlMetaRe = /<!--\s*__fileMetadata__\s*=\s*(\{[\s\S]*?\})\s*-->/;
    if (!htmlMetaRe.test(content)) {
      const finalMeta = {
        name: baseName,
        extension: "html",
        type: "template",
        path: "./templates/",
        active: true,
      };
      return `<!--\n__fileMetadata__ = ${JSON.stringify(finalMeta, null, 2)}\n-->\n${content}`;
    }
    return content;
  }

  // 3. Scripts / Webhooks / Modules -> JS/TS/JSX/TSX Export metadata
  let deducedType = "webhook";
  let deducedPath = "./webhooks/";
  let deducedTrigger = "manual";

  if (normalizedPath.includes("modules/custom/")) {
    deducedType = "custom:module";
    deducedPath = "./modules/custom/";
  } else if (normalizedPath.includes("modules/esm/")) {
    deducedType = "esm:module";
    deducedPath = "./modules/esm/";
  }

  const jsMetaRe = /(?:\/\*\*[\s\S]*?\*\/\s*)?export\s+const\s+__fileMetadata__\s*=\s*(\{[\s\S]*?\});?/;
  const match = content.match(jsMetaRe);

  let metaObj = {};
  if (match && match[1]) {
    try {
      metaObj = JSON.parse(match[1]);
    } catch (e) {}
  }

  // Prioritize real file extension if js|ts|jsx|tsx, otherwise force to js
  const allowedScriptExts = ["js", "ts", "jsx", "tsx"];
  const fileExt = ext.toLowerCase();
  const prioritizedExt = allowedScriptExts.includes(fileExt) ? fileExt : "js";

  const finalMeta = {
    name: metaObj.name || baseName,
    extension: prioritizedExt,
    target_collection: metaObj.target_collection || null,
    type: metaObj.type || deducedType,
    path: metaObj.path || deducedPath,
    trigger_type: metaObj.trigger_type || deducedTrigger,
    active: metaObj.active !== undefined ? metaObj.active : true,
    visibility: metaObj.visibility || "private",
  };

  const typeImportPath = finalMeta.path.includes("modules/") ? "../../apexkit" : "../apexkit";
  const formattedMetadata = `/** @type {import("${typeImportPath}").FileMetadata} */\nexport const __fileMetadata__ = ${JSON.stringify(
    finalMeta,
    null,
    2
  )};`;

  if (!match) {
    return `${formattedMetadata}\n\n${content}`;
  }

  // If metadata extension differed or needs updating, replace with prioritized definition
  if (metaObj.extension !== prioritizedExt || content.includes('import("../apexkit")') && typeImportPath.includes("../..")) {
    return content.replace(jsMetaRe, formattedMetadata);
  }

  return content;
}

// --- PRUNE ORPHANED REMOTE ENTITIES ---
async function pruneRemoteOrphans(localScripts, localTemplates, localAiActions) {
  if (DELETE_REMOTE_TYPES.length === 0) return;

  const apiBase = getScopedApiBase();
  const headers = getAuthHeaders();

  console.log(`\n🧹 [Prune Remote]: Checking for orphaned items in ApexKit (${DELETE_REMOTE_TYPES.join(", ")})...`);

  if (DELETE_REMOTE_TYPES.includes("scripts") || DELETE_REMOTE_TYPES.includes("all")) {
    try {
      const res = await fetch(`${apiBase}/admin/scripts`, { headers });
      if (res.ok) {
        const payload = await res.json();
        const remoteScripts = Array.isArray(payload) ? payload : payload.local || [];

        for (const script of remoteScripts) {
          if (!localScripts.has(script.name)) {
            console.log(`  🗑️ [Deleted Remote Script]: ${script.name} (ID: ${script.id})`);
            await fetch(`${apiBase}/admin/scripts/${script.id}`, { method: "DELETE", headers });
          }
        }
      }
    } catch (e) {
      console.warn(`  ⚠️ Failed to prune remote scripts: ${e.message}`);
    }
  }

  if (DELETE_REMOTE_TYPES.includes("templates") || DELETE_REMOTE_TYPES.includes("all")) {
    try {
      const res = await fetch(`${apiBase}/admin/templates`, { headers });
      if (res.ok) {
        const remoteTemplates = await res.json();
        for (const tmpl of remoteTemplates) {
          if (!localTemplates.has(tmpl.slug)) {
            console.log(`  🗑️ [Deleted Remote Template]: ${tmpl.slug} (ID: ${tmpl.id})`);
            await fetch(`${apiBase}/admin/templates/${tmpl.id}`, { method: "DELETE", headers });
          }
        }
      }
    } catch (e) {
      console.warn(`  ⚠️ Failed to prune remote templates: ${e.message}`);
    }
  }

  if (
    DELETE_REMOTE_TYPES.includes("ai_actions") ||
    DELETE_REMOTE_TYPES.includes("actions") ||
    DELETE_REMOTE_TYPES.includes("all")
  ) {
    try {
      const res = await fetch(`${apiBase}/admin/ai/actions`, { headers });
      if (res.ok) {
        const remoteActions = await res.json();
        for (const action of remoteActions) {
          if (!localAiActions.has(action.slug)) {
            console.log(`  🗑️ [Deleted Remote AI Action]: ${action.slug} (ID: ${action.id})`);
            await fetch(`${apiBase}/admin/ai/actions/${action.id}`, { method: "DELETE", headers });
          }
        }
      }
    } catch (e) {
      console.warn(`  ⚠️ Failed to prune remote AI actions: ${e.message}`);
    }
  }
}

// --- 10. UNIFIED SYNC PUSH/COMMIT (With Scope & Category Filtering) ---
async function runManualPushOrCommit(commitToDb) {
  const selectedCategories = [];
  if (syncScripts) selectedCategories.push("Scripts (--scripts)");
  if (syncTemplates) selectedCategories.push("Templates (--templates)");
  if (syncActions) selectedCategories.push("AI Actions (--actions)");

  const modeLabel = commitToDb ? "Push & Commit to DB" : "Commit to VFS (Memory)";
  console.log(`🚀 [${modeLabel}]: Syncing ${selectedCategories.join(", ")} to ApexKit (${BASE_URL})...\n`);

  let WSClient = globalThis.WebSocket;
  if (!WSClient) {
    try {
      const wsModule = await import("ws");
      WSClient = wsModule.default || wsModule;
    } catch (e) {
      console.error(
        "❌ WebSocket is not natively supported in your Node version. Please install 'ws' with: npm install ws"
      );
      process.exit(1);
    }
  }

  const ws = new WSClient(WS_URL);

  const targetDirs = [];
  if (syncScripts) {
    targetDirs.push("./webhooks", "./modules");
  }
  if (syncTemplates) {
    targetDirs.push("./templates");
  }
  if (syncActions) {
    targetDirs.push("./ai_actions");
  }

  const filesToPush = [];
  const localScripts = new Set();
  const localTemplates = new Set();
  const localAiActions = new Set();

  targetDirs.forEach((dir) => {
    if (!fs.existsSync(dir)) return;
    const files = fs.readdirSync(dir, { recursive: true });

    files.forEach((file) => {
      const fullPath = path.join(dir, file);
      if (fs.statSync(fullPath).isFile()) {
        const relativePath = fullPath.replace(/\\/g, "/").replace(/^\.\//, "");
        const baseName = path.basename(relativePath);

        // Explicitly skip .gitkeep and hidden files
        if (baseName.startsWith(".") || baseName.endsWith(".gitkeep")) return;

        const rawContent = fs.readFileSync(fullPath, "utf-8");
        const content = inferFileMetadata(relativePath, rawContent);

        if (!content) return; // Skip ignored/hidden files

        // Track local entities
        if (relativePath.startsWith("templates/") || relativePath.endsWith(".html")) {
          const stem = path.basename(relativePath, path.extname(relativePath));
          localTemplates.add(stem);
        } else if (relativePath.startsWith("ai_actions/") || relativePath.endsWith(".json")) {
          try {
            const parsed = JSON.parse(content);
            const slug = parsed.action?.slug || parsed.slug || path.basename(relativePath, ".json");
            localAiActions.add(slug);
          } catch (e) {}
        } else {
          const stem = path.basename(relativePath, path.extname(relativePath));
          localScripts.add(stem);
        }

        filesToPush.push({ path: relativePath, content });
      }
    });
  });

  if (filesToPush.length === 0) {
    console.log(`⚠️ No files found for the selected targets (${selectedCategories.join(", ")}).`);
    process.exit(0);
  }

  let acksReceived = 0;
  let hasErrors = false;

  ws.addEventListener("message", (event) => {
    try {
      const msg = JSON.parse(event.data);
      if (msg.type === "SyncAck") {
        acksReceived++;
        console.log(`  ✅ [Committed]: ${msg.payload.message}`);
      } else if (msg.type === "Error") {
        acksReceived++;
        hasErrors = true;
        console.error(`  ❌ [Error]: ${msg.payload.message}`);
      }

      if (acksReceived >= filesToPush.length) {
        finishSync();
      }
    } catch (e) {}
  });

  async function finishSync() {
    if (commitToDb && DELETE_REMOTE_TYPES.length > 0) {
      await pruneRemoteOrphans(localScripts, localTemplates, localAiActions);
    }
    console.log(`\n🎉 Done! ${filesToPush.length} file(s) processed.`);
    setTimeout(() => {
      try {
        ws.close();
      } catch (e) {}
      process.exit(hasErrors ? 1 : 0);
    }, 200);
  }

  ws.addEventListener("open", () => {
    filesToPush.forEach((item) => {
      console.log(`  ⬆️  Pushing ${item.path} (commit_to_db: ${commitToDb})`);
      ws.send(
        JSON.stringify({
          type: "PushFile",
          payload: { path: item.path, content: item.content, commit_to_db: commitToDb },
        })
      );
    });

    // Safety timeout in case a socket ACK drops
    setTimeout(() => {
      if (acksReceived < filesToPush.length) {
        console.log(`\n⚠️ Finished pushing. Waiting response timeout reached.`);
        finishSync();
      }
    }, 4000);
  });

  ws.addEventListener("error", (err) => {
    console.error("❌ Connection failed. Verify that ApexKit is running and check your API Key / Port.");
    process.exit(1);
  });
}

// --- 11. WATCHER & LIVE SYNC CLIENT ---
let ws;
let isReconnecting = false;
let reconnectTimer = null;
let hasLoggedWaiting = false;
const debounceMap = new Map();

function scheduleReconnect() {
  if (isReconnecting) return;
  isReconnecting = true;

  if (reconnectTimer) clearTimeout(reconnectTimer);

  if (!hasLoggedWaiting) {
    console.log("🔌 Connection unavailable. Retrying...\n");
    hasLoggedWaiting = true;
  }

  reconnectTimer = setTimeout(() => {
    isReconnecting = false;
    connectWebSocket(true);
  }, 3000);
}

async function connectWebSocket(isRetry = false) {
  if (!isRetry) {
    console.log(`⚡ Connecting to ApexKit (URL: ${BASE_URL}, Scope: ${SCOPE_KEY})...`);
  }

  let WSClient = globalThis.WebSocket;
  if (!WSClient) {
    try {
      const wsModule = await import("ws");
      WSClient = wsModule.default || wsModule;
    } catch (e) {
      console.error(
        "❌ WebSocket is not natively supported in your Node version. Please install 'ws' with: npm install ws"
      );
      process.exit(1);
    }
  }

  try {
    if (ws) {
      try {
        ws.close();
      } catch (e) {}
    }
    ws = new WSClient(WS_URL);
  } catch (err) {
    scheduleReconnect();
    return;
  }

  ws.addEventListener("open", async () => {
    if (reconnectTimer) clearTimeout(reconnectTimer);
    isReconnecting = false;
    hasLoggedWaiting = false;
    console.log("✅ Connected to ApexKit Live Engine!");
    console.log(`👀 Watching local files... (Auto-Commit to DB: ${!NO_AUTO_COMMIT})`);
  });

  ws.addEventListener("message", (event) => {
    try {
      const dataStr = typeof event.data === "string" ? event.data : event.data.toString();
      const msg = JSON.parse(dataStr);

      if (msg.type === "SyncAck") {
        console.log(`🚀 [DB Sync]: ${msg.payload.message}`);
      } else if (msg.type === "Error") {
        console.error(`❌ [Error]: ${msg.payload.message}`);
      } else if (msg.type === "Ping") {
        ws.send(JSON.stringify({ type: "Pong" }));
      }
    } catch (e) {}
  });

  ws.addEventListener("error", () => {
    scheduleReconnect();
  });

  ws.addEventListener("close", () => {
    scheduleReconnect();
  });
}

function startWatcher() {
  connectWebSocket();

  const watchDirs = ["./webhooks", "./templates", "./ai_actions", "./modules"];
  watchDirs.forEach((dir) => {
    if (fs.existsSync(dir)) {
      fs.watch(dir, { recursive: true }, (eventType, filename) => {
        if (!filename) return;

        if (
          filename.endsWith(".js") ||
          filename.endsWith(".ts") ||
          filename.endsWith(".jsx") ||
          filename.endsWith(".tsx") ||
          filename.endsWith(".html") ||
          filename.endsWith(".json")
        ) {
          const relativePath = path.join(dir, filename).replace(/\\/g, "/").replace(/^\.\//, "");

          clearTimeout(debounceMap.get(relativePath));
          debounceMap.set(
            relativePath,
            setTimeout(() => {
              try {
                if (fs.existsSync(relativePath)) {
                  let raw = fs.readFileSync(relativePath, "utf-8");
                  let content = inferFileMetadata(relativePath, raw);

                  if (!content) return; // Skip ignored/hidden files

                  console.log(
                    `📝 Synced: ${relativePath} ${NO_AUTO_COMMIT ? "(VFS Only)" : "(Committed to DB)"}`
                  );

                  if (ws && ws.readyState === 1) {
                    ws.send(
                      JSON.stringify({
                        type: "PushFile",
                        payload: {
                          path: relativePath,
                          content: content,
                          commit_to_db: !NO_AUTO_COMMIT,
                        },
                      })
                    );
                  }
                }
              } catch (err) {
                console.error(`⚠️ Could not read ${relativePath}:`, err.message);
              }
            }, 100)
          );
        }
      });
    }
  });
}

// --- 12. CLI ROUTER ---
if (IS_INIT) {
  runInit();
} else if (IS_INIT_FILE) {
  runInitFile();
} else if (IS_PULL) {
  runPull();
} else if (IS_COMMIT) {
  runManualPushOrCommit(false); // VFS Only
} else if (IS_PUSH) {
  runManualPushOrCommit(true); // Permanent DB Commit
} else if (IS_STATUS) {
  console.log(`ApexKit Workspace Information:
  • Server URL: ${BASE_URL}
  • Target Scope: ${SCOPE_KEY}
  • API Key: ${API_KEY.substring(0, 16)}...
  • Auto-Commit: ${!NO_AUTO_COMMIT}
  • Sync Targets: ${syncScripts ? "Scripts " : ""}${syncTemplates ? "Templates " : ""}${syncActions ? "Actions " : ""}
  • Prune Flags: ${DELETE_REMOTE_TYPES.length > 0 ? DELETE_REMOTE_TYPES.join(", ") : "none"}
`);
  process.exit(0);
} else {
  startWatcher();
}