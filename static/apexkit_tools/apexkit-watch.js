// apexkit-watch.js - ApexKit Developer Tools, Sync Watcher & IntelliSense Engine
import fs from "fs";
import path from "path";
import zlib from "zlib";
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

// Helper to construct scope-aware API URLs
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

// --- 3. TRIGGER TYPES DEFINITIONS ---
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

// --- 4. WORKSPACE TYPES & CONFIG GENERATOR ---
async function generateWorkspaceFiles() {
  console.log("  ⏳ Fetching collection schemas from DB...");
  let collectionsData = [];
  try {
    const fetchUrl = `${getScopedApiBase()}/collections`;
    const res = await fetch(fetchUrl, { headers: getAuthHeaders() });
    if (res.ok) {
      collectionsData = await res.json();
    } else {
      console.warn(`  ⚠️ Failed to fetch schemas. HTTP ${res.status}`);
    }
  } catch (err) {
    console.warn(`  ⚠️ Could not connect to DB for schemas: ${err.message}`);
  }

  // A. Generate Collection Interfaces & Expand Typings
  let collectionTypesStr = `export interface Collections {\n`;
  let collectionExpandsStr = `export interface CollectionExpands {\n`;
  const collectionNames = [];

  collectionsData.forEach((col) => {
    collectionNames.push(`"${col.name}"`);
    let fieldsStr = "";
    let expandsStr = "";

    if (col.schema) {
      // Standard Fields
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
        }
      }

      // Relations
      if (col.schema.relations) {
        for (const [name, rel] of Object.entries(col.schema.relations)) {
          const isMany = rel.relation_type === "many";
          const tsType = isMany ? "(number | string)[]" : "number | string";
          const safeName = /^[a-zA-Z_$][a-zA-Z0-9_$]*$/.test(name) ? name : `"${name}"`;
          fieldsStr += `    ${safeName}${rel.required ? "" : "?"}: ${tsType};\n`;

          const targetCol = rel.target_collection;
          const expandType = `{ id: number | string; data: Collections["${targetCol}"]; created: string; updated: string; expand?: any }`;
          expandsStr += `    ${safeName}?: ${isMany ? `Array<${expandType}>` : expandType};\n`;
        }
      }
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

export interface GraphqlConfig {
  parent: "Query" | "Mutation" | "User" | "_AuthUser" | string;
  name: string;
  args?: Record<string, string>;
  returnType: string;
}

declare global {
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

  /** Root Multitenancy and Administrative Manager (Only available in Root context) */
  const $root: {
    db: typeof $db;
    createTenant(id: string, config?: { name?: string; tier?: string; owner_id?: number }): Promise<boolean>;
    updateTenant(id: string, updates: { name?: string; status?: string; tier?: string; max_storage_mb?: number; max_vectors?: number; max_ai_requests?: number }): Promise<boolean>;
    deleteTenant(id: string): Promise<boolean>;
    getTenantDiskUsage(id: string): Promise<number>;
    listTenants(): Promise<any[]>;
    createSandbox(id: string, config?: { name?: string; clone_strategy?: "none" | "schema" | "partial" | "full"; clone_record_limit?: number; collections?: string[]; scripts?: string[]; templates?: string[] }): Promise<boolean>;
    updateSandbox(id: string, updates: { name?: string; status?: string; expires_at?: string }): Promise<boolean>;
    deleteSandbox(id: string): Promise<boolean>;
    getSandboxDiskUsage(id: string): Promise<number>;
    createKey(name: string, config?: { tenant_id?: string; issuer?: string; env_type?: "sys" | "tnnt" | "sk" | "pk"; roles?: string[]; bypass_cors?: boolean }): Promise<{ key: string; info: any }>;
    updateKey(id: number | string, updates: { name?: string; status?: string; roles?: string[]; bypass_cors?: boolean }): Promise<boolean>;
    deleteKey(id: number | string): Promise<boolean>;
    listKeys(): Promise<any[]>;
  } | null;

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

  /** Fast In-Memory Zip Creator & Extractor */
  const $zip: {
    create(files: Record<string, string | Uint8Array>): string;
    extract(base64Zip: string): Record<string, string>;
    inspect(base64Zip: string): { total_size: number; file_count: number; files: any[] };
  };

  /** Local AI Embeddings Engine */
  const $ai: {
    embed(text: string): Promise<number[]>;
    meanVector(vectors: number[][]): number[];
    cosineSimilarity(v1: number[], v2: number[]): number;
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

  /** Background Queue Orchestration System */
  const $queue: {
    spawn<T = any>(
      fnOrCode: ((pid: string, req: Request) => Promise<T> | T) | string,
      options?: { timeoutMs?: number; args?: any }
    ): Promise<{ pid: string; status: "queued" | "running" }>;
    status(pid: string): Promise<{
      pid: string;
      status: "queued" | "running" | "completed" | "failed" | "timed_out" | "not_found";
      runtime_ms: number;
      error?: string;
    }>;
    result<T = any>(pid: string): Promise<{
      pid: string;
      status: "queued" | "running" | "completed" | "failed" | "timed_out" | "not_found";
      runtime_ms: number;
      result?: T;
      error?: string;
    }>;
  };

  /** Subprocess Runner (Root scope only) */
  const $cmd: {
    run(
      program: string,
      args?: string[],
      options?: { cwd?: string; env?: Record<string, string>; timeout?: number }
    ): Promise<{ stdout: string; stderr: string; status: number }>;
    spawn(
      program: string,
      args?: string[],
      options?: {
        cwd?: string;
        env?: Record<string, string>;
        timeout?: number;
        onProgress?: { regex?: string; channel?: string; event?: string };
      }
    ): Promise<{ pid: number; status: string }>;
    status(pid: number): Promise<any>;
    setLimit(program: string, limit: number): Promise<any>;
    kill(pid: number): Promise<any>;
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

  /** WebAssembly Raw Loader & WASI Runner */
  const $wasm: {
    call(
      wasmBase64OrName: string,
      funcName: string,
      args: number[],
      options?: { name?: string; memoryMb?: number; timeoutMs?: number }
    ): Promise<number | number[]>;
    runWasi(
      wasmBase64OrName: string,
      cliArgs?: string[],
      options?: { name?: string; memoryMb?: number; timeoutMs?: number }
    ): Promise<boolean>;
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
    sleep(ms: number): Promise<void>;
    randomHex(len?: number): string;
  };

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

  class Headers {
    constructor(init?: Record<string, string> | [string, string][] | Headers);
    get(name: string): string | null;
    set(name: string, value: string): void;
    has(name: string): boolean;
    delete(name: string): void;
    forEach(callback: (value: string, key: string) => void): void;
  }

  class Response {
    constructor(
      body?: any,
      init?: { status?: number; statusText?: string; headers?: Record<string, string> | Headers }
    );
    readonly status: number;
    readonly statusText: string;
    readonly ok: boolean;
    readonly headers: Headers;
    readonly url: string;
    json(): Promise<any>;
    text(): Promise<string>;
    arrayBuffer(): Promise<ArrayBuffer>;
  }

  class Request {
    constructor(
      input: string | { url: string; method?: string; headers?: any; bodyData?: any },
      init?: { method?: string; headers?: any; body?: any }
    );
    readonly method: string;
    readonly url: string;
    readonly headers: Headers;
    readonly auth: AuthContext | null;
    readonly args: any;
    json(): Promise<any>;
    text(): Promise<string>;
    arrayBuffer(): Promise<ArrayBuffer>;
    clone(): Request;
  }
}
export {};
`;
  fs.writeFileSync("apexkit.d.ts", types.trim());
  console.log("  📄 Generated apexkit.d.ts with full Engine & Collection Typings");

  // B. Generate jsconfig.json
  const jsconfig = {
    compilerOptions: {
      target: "ES2022",
      module: "ESNext",
      moduleResolution: "node",
      baseUrl: ".",
      paths: {
        "@/custom/*": ["./modules/custom/*"],
        "@/esm/*": ["./modules/esm/*"],
      },
      allowJs: true,
      checkJs: true,
    },
    include: ["**/*.js", "**/*.ts", "apexkit.d.ts"],
  };
  fs.writeFileSync("jsconfig.json", JSON.stringify(jsconfig, null, 2));
  console.log("  📄 Generated jsconfig.json for VS Code module alias resolution");

  // C. Generate package.json if missing
  if (!fs.existsSync("package.json")) {
    const pkg = {
      name: "apexkit-workspace",
      type: "module",
      scope: SCOPE_KEY,
      dependencies: {},
    };
    fs.writeFileSync("package.json", JSON.stringify(pkg, null, 2));
    console.log("  📄 Generated package.json");
  }
}

// --- 5. INITIAL CODE GENERATORS ---
function getBoilerplateCode(fileType, cleanName, ext, triggerType, targetCollection) {
  const metadata = {
    name: cleanName,
    extension: ext,
    target_collection: targetCollection,
    type: fileType,
    path:
      fileType === "template"
        ? "./templates/"
        : fileType.includes("module")
        ? "./modules/custom/"
        : "./webhooks/",
    trigger_type: triggerType,
    active: true,
    visibility: "private",
  };

  const header = `/** @type {import("../apexkit").FileMetadata} */\nexport const __fileMetadata__ = ${JSON.stringify(
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
<script type="server/js">
export default async function (context) {
  const posts = await $db.records.list("${targetCollection || "posts"}", { limit: 10 }).catch(() => ({ items: [] }));
  return {
    title: "${cleanName.replace(/-/g, " ").toUpperCase()}",
    items: posts.items
  };
}
</script>

<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>{{ title }}</title>
  <link rel="stylesheet" href="/styles.css">
  <script src="/static/js/htmx.js"></script>
  <script src="/static/js/alpine.js" defer></script>
</head>
<body class="bg-slate-900 text-slate-100 min-h-screen p-8">
  <div class="max-w-4xl mx-auto space-y-6">
    <h1 class="text-3xl font-bold text-indigo-400">{{ title }}</h1>
    
    <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
      {% for item in items %}
      <div class="p-4 bg-slate-800 rounded-lg border border-slate-700 shadow">
        <h2 class="text-xl font-semibold">{{ item.data.title | default(value="Item #" ~ item.id) }}</h2>
        <p class="text-slate-400 text-sm mt-2">{{ item.created }}</p>
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
  await $realtime.send("${targetCollection || "general"}", "record_changed", {
    action: "${triggerType}",
    record_id: id,
    data
  });
}
`;

    case "cron":
      return `${header}/**
 * Scheduled Cron Job: ${cleanName}
 * @param {{ trigger: "cron", job: string }} event
 */
export default async function (event) {
  console.log(\`[Cron] Executing scheduled task: \${event.job}\`);
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

// --- 6. INTERACTIVE FILE WIZARD (create / new / init:file / --init-file) ---
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
  const ext =
    fileType === "template"
      ? "html"
      : ((await rl.question("Extension (js/ts) [js]: ")).trim() || "js");

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

// --- 7. WORKSPACE INIT WIZARD (init / --init) ---
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

// --- 8. COMPREHENSIVE PULL (pull / --pull) ---
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

      scripts.forEach((script) => {
        const meta = {
          id: script.id,
          name: script.name,
          extension: "js",
          target_collection: script.target_collection || null,
          type: script.metadata?.type || "webhook",
          path: script.metadata?.path || "./webhooks/",
          trigger_type: script.trigger_type || "manual",
          active: script.active !== undefined ? script.active : true,
          visibility: script.visibility || "private",
        };

        const targetDir = meta.path.replace(/^\.\//, "");
        fs.mkdirSync(targetDir, { recursive: true });

        const formattedContent = `/** @type {import("../apexkit").FileMetadata} */\nexport const __fileMetadata__ = ${JSON.stringify(
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

  // Re-generate local TypeScript IntelliSense engine
  await generateWorkspaceFiles();

  console.log(`\n✨ Pull complete! Hydrated ${pulledCount} items to local workspace.`);
  process.exit(0);
}

// --- 9. METADATA AUTO-INFERENCE HELPER ---
function inferFileMetadata(filePath, content) {
  const ext = path.extname(filePath).replace(/^\./, "");
  const baseName = path.basename(filePath, `.${ext}`);
  const normalizedPath = filePath.replace(/\\/g, "/");

  let deducedType = "webhook";
  let deducedPath = "./webhooks/";
  let deducedTrigger = "manual";

  if (normalizedPath.includes("templates/")) {
    deducedType = "template";
    deducedPath = "./templates/";
  } else if (normalizedPath.includes("modules/custom/")) {
    deducedType = "custom:module";
    deducedPath = "./modules/custom/";
  } else if (normalizedPath.includes("modules/esm/")) {
    deducedType = "esm:module";
    deducedPath = "./modules/esm/";
  } else if (normalizedPath.includes("ai_actions/")) {
    deducedType = "ai_action";
    deducedPath = "./ai_actions/";
  }

  const metaRe =
    /(?:export\s+const\s+__fileMetadata__\s*=\s*|<!--\s*__fileMetadata__\s*=\s*)(\{[\s\S]*?\})(?:;|\s*-->)/;
  const match = content.match(metaRe);

  let metaObj = {};
  if (match && match[1]) {
    try {
      metaObj = JSON.parse(match[1]);
    } catch (e) {}
  }

  const finalMeta = {
    name: metaObj.name || baseName,
    extension: metaObj.extension || ext,
    target_collection: metaObj.target_collection || null,
    type: metaObj.type || deducedType,
    path: metaObj.path || deducedPath,
    trigger_type: metaObj.trigger_type || deducedTrigger,
    active: metaObj.active !== undefined ? metaObj.active : true,
    visibility: metaObj.visibility || "private",
  };

  if (!match) {
    if (ext === "html") {
      return `<!--\n__fileMetadata__ = ${JSON.stringify(finalMeta, null, 2)}\n-->\n${content}`;
    } else {
      return `/** @type {import("../apexkit").FileMetadata} */\nexport const __fileMetadata__ = ${JSON.stringify(
        finalMeta,
        null,
        2
      )};\n\n${content}`;
    }
  }

  return content;
}

// --- 10. UNIFIED SYNC PUSH/COMMIT (commit vs push) ---
async function runManualPushOrCommit(commitToDb) {
  const modeLabel = commitToDb ? "Push & Commit to DB" : "Commit to VFS (Transient Memory)";
  console.log(`🚀 [${modeLabel}]: Syncing all local files to ApexKit (${BASE_URL})...\n`);

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

  ws.addEventListener("open", () => {
    const dirs = ["./webhooks", "./templates", "./ai_actions", "./modules"];
    let filesSent = 0;

    dirs.forEach((dir) => {
      if (!fs.existsSync(dir)) return;
      const files = fs.readdirSync(dir, { recursive: true });

      files.forEach((file) => {
        const fullPath = path.join(dir, file);
        if (fs.statSync(fullPath).isFile()) {
          const relativePath = fullPath.replace(/\\/g, "/").replace(/^\.\//, "");
          let content = fs.readFileSync(fullPath, "utf-8");
          content = inferFileMetadata(relativePath, content);

          console.log(`  ⬆️  Pushing ${relativePath} (commit_to_db: ${commitToDb})`);
          ws.send(
            JSON.stringify({
              type: "PushFile",
              payload: { path: relativePath, content: content, commit_to_db: commitToDb },
            })
          );
          filesSent++;
        }
      });
    });

    console.log(`\n✅ Synced ${filesSent} files to ApexKit! Closing connection.`);
    setTimeout(() => process.exit(0), 1000);
  });

  ws.addEventListener("error", () => {
    console.error("❌ Connection failed. Verify that the ApexKit server is running and check your API Key.");
    process.exit(1);
  });
}

// --- 11. WATCHER & LIVE SYNC CLIENT (watch / dev / default) ---
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
                  let content = fs.readFileSync(relativePath, "utf-8");
                  content = inferFileMetadata(relativePath, content);

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
`);
  process.exit(0);
} else {
  startWatcher();
}