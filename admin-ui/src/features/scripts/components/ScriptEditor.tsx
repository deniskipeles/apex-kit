import React, { useState, useEffect } from 'react';
import { 
  Save, 
  Code, 
  Database, 
  Globe, 
  Lock, 
  ShieldCheck, 
  HelpCircle, 
  Play 
} from 'lucide-react';
import { Button, Input, Label, Select, Switch, Separator } from '../../../components/ui/Elements';
import { Dialog } from '../../../components/ui/Dialog';
import { Script, Collection } from '../../../types';
import { AiCodeAssistant } from '../../ai/components/AiCodeAssistant';
import { collectionsService } from '../../collections/services/collectionsService';
import { CodeEditor } from '../../../components/form/CodeEditor';
import { apiClient } from '@/src/lib/apiClient';

interface ScriptEditorProps {
  isOpen: boolean;
  onClose: () => void;
  onSave: (data: Partial<Script>) => Promise<void>;
  initialData?: Script;
}

const TRIGGER_TYPES = [
  // --- API & System ---
  { value: 'manual', label: 'Manual Endpoint (API)', group: 'API' },
  { value: 'graphql', label: 'GraphQL Resolver', group: 'API' },
  { value: 'cron', label: 'Scheduled Job (Cron)', group: 'System' },

  // --- Data Records (Write) ---
  { value: 'before_create_record', label: 'Before Create Record', group: 'Record Write' },
  { value: 'after_create_record', label: 'After Create Record', group: 'Record Write' },
  { value: 'before_update_record', label: 'Before Update Record', group: 'Record Write' },
  { value: 'after_update_record', label: 'After Update Record', group: 'Record Write' },
  { value: 'before_delete_record', label: 'Before Delete Record', group: 'Record Write' },
  { value: 'after_delete_record', label: 'After Delete Record', group: 'Record Write' },

  // --- Data Records (Read/Filter) ---
  { value: 'before_list_records', label: 'Before List Records (Filter Query)', group: 'Record Read' },
  { value: 'after_list_records', label: 'After List Records (Filter Output)', group: 'Record Read' },
  { value: 'before_get_record', label: 'Before Get Record', group: 'Record Read' },
  { value: 'after_get_record', label: 'After Get Record (Filter Output)', group: 'Record Read' },

  // --- Collections (Schema) ---
  { value: 'before_collection_create', label: 'Before Create Collection', group: 'Schema' },
  { value: 'after_collection_create', label: 'After Create Collection', group: 'Schema' },
  { value: 'before_collection_update', label: 'Before Update Collection', group: 'Schema' },
  { value: 'after_collection_update', label: 'After Update Collection', group: 'Schema' },
  { value: 'before_collection_delete', label: 'Before Delete Collection', group: 'Schema' },

  // --- Tenant & Sandbox Requests (Traffic/Quota) ---
  { value: 'before_tenant_request', label: 'Before Tenant Request', group: 'Traffic' },
  { value: 'after_tenant_request', label: 'After Tenant Request', group: 'Traffic' },
  { value: 'before_sandbox_request', label: 'Before Sandbox Request', group: 'Traffic' },
  { value: 'after_sandbox_request', label: 'After Sandbox Request', group: 'Traffic' },
];

const DEFAULT_CODE = {
  manual: `export default async function(req) {\n    const body = await req.json();\n    return new Response({ message: "Hello!" });\n}`,
  cron: `export default async function() {\n    log("Running cron job...");\n}`,
  hook: `export default async function(e) {\n    // Context: e.record, e.collection, e.auth\n    return e.record.data;\n}`,
  filter: `export default async function(e) {\n    // Context: e.data, e.auth\n    return e.data;\n}`,
  system: `export default async function(e) {\n    log("Event Triggered: " + e.trigger);\n}`,
  graphql: `export const graphql = {\n  "parent": "Query",\n  "name": "customField",\n  "args": {},\n  "returnType": "JSON"\n};\n\nexport default async function(req) {\n    return new Response({ success: true });\n}`,
  traffic: `export default async function(e) {\n    log(e.trigger + " " + e.data.path);\n}`,
};

// --- DOCUMENTATION REGISTRY ---
interface TriggerDoc {
  name: string;
  desc: string;
  signature: string;
  payload: string;
  template: string;
  returns: string;
}

const TRIGGER_DOCS: Record<string, TriggerDoc> = {
  // API & System
  manual: {
    name: 'Manual Endpoint (API)',
    desc: 'Exposes a secure HTTP endpoint at /api/v1/run/{script_name}. Perfect for webhooks, external integrations, or handling custom user-submitted payloads.',
    signature: 'export default async function(req: Request): Promise<Response | object>',
    payload: `// Input Payload: e.g. POST to /api/v1/run/my-script\n{\n  "amount": 1500,\n  "currency": "USD"\n}`,
    template: `export default async function(req) {\n  const body = await req.json();\n  log("Processing payment: " + body.amount);\n  \n  // Return a standard HTTP Response\n  return new Response({ success: true, message: "Processed" });\n}`,
    returns: 'Must return a standard `Response` object or a plain JSON object (which is automatically wrapped in a 200 OK response).'
  },
  graphql: {
    name: 'GraphQL Resolver',
    desc: 'Binds a custom fields resolver to any existing GraphQL type (e.g. Query, Mutation, or User) using in-memory schema declarations.',
    signature: 'export const graphql = { parent: string, name: string, args: object, returnType: string };\nexport default async function(req: Request): Promise<any>',
    payload: `// Input arguments passed from GraphQL query:\n{\n  "id": "123",\n  "parent": { "id": 1, "email": "test@test.com" }\n}`,
    template: `export const graphql = {\n  "parent": "Query",\n  "name": "customField",\n  "args": { "id": "String" },\n  "returnType": "JSON"\n};\n\nexport default async function(req) {\n  const args = await req.json();\n  const post = await $db.records.get('posts', args.id);\n  return new Response(post);\n}`,
    returns: 'Must return the value matching your declared `returnType` (e.g., String, Int, JSON, or custom Object).'
  },
  cron: {
    name: 'Scheduled Job (Cron)',
    desc: 'Executes asynchronously on a background timer (configured in Settings > Cron Jobs). Great for periodic data cleanup, syncs, or automated report generation.',
    signature: 'export default async function(): Promise<void>',
    payload: `// No payload is passed.\n// Access system globals like $db, $http, $mail, and $ai.`,
    template: `export default async function() {\n  log("Running nightly database cleanup...");\n  const limitDate = new Date(Date.now() - 30 * 86400000).toISOString();\n  \n  // Prune historical log entries\n  // await $db.query(...) ...\n}`,
    returns: 'Void. Returning a value has no effect.'
  },

  // Record Write
  before_create_record: {
    name: 'Before Create Record',
    desc: 'Intercepts a database insert transaction before it is committed. Use this to sanitize input fields, compute custom defaults (like slugs), or validate complex business rules.',
    signature: 'export default async function(e: HookEvent): Promise<RecordData | false>',
    payload: `{\n  "record": { "id": null, "data": { "title": "My Post" } },\n  "collection": { "id": 1, "name": "posts", "schema": {...} },\n  "auth": { "id": 1, "email": "admin@apexkit.io", "role": "admin" },\n  "trigger": "before_create_record"\n}`,
    template: `export default async function(e) {\n  // Compute a clean slug automatically\n  e.record.data.slug = e.record.data.title.toLowerCase().replace(/\\s+/g, '-');\n  \n  // Return the modified record data object\n  return e.record.data;\n}`,
    returns: 'Return the modified record data object. Throw an error or return `false` to abort the transaction.'
  },
  after_create_record: {
    name: 'After Create Record',
    desc: 'Fires asynchronously after a record is successfully written to the database. Ideal for publishing event triggers, triggering Discord webhooks, or starting AI vectorization.',
    signature: 'export default async function(e: HookEvent): Promise<void>',
    payload: `{\n  "record": { "id": 42, "data": { "title": "My Post", "slug": "my-post" } },\n  "collection": { "id": 1, "name": "posts" },\n  "auth": { "id": 1, "email": "admin@apexkit.io", "role": "admin" }\n}`,
    template: `export default async function(e) {\n  log("Record #" + e.record.id + " was written. Sending webhook...");\n  await $http.post("https://discord.com/api/webhooks/...", {\n    content: "New Post Published: " + e.record.data.title\n  });\n}`,
    returns: 'Void. Returning a value has no effect.'
  },
  before_update_record: {
    name: 'Before Update Record',
    desc: 'Fires inside the transaction before updating an existing database record. Let’s you validate changes or append timestamps.',
    signature: 'export default async function(e: HookEvent): Promise<RecordData | false>',
    payload: `{\n  "record": { "id": 42, "data": { "title": "Updated Title" } },\n  "collection": { "id": 1, "name": "posts" },\n  "auth": { "id": 1, "email": "admin@apexkit.io", "role": "admin" }\n}`,
    template: `export default async function(e) {\n  // Block edits if post status is "locked"\n  if (e.record.data.locked) {\n    throw "Cannot edit locked document";\n  }\n  return e.record.data;\n}`,
    returns: 'Return the modified data payload, or throw/return `false` to roll back.'
  },
  after_update_record: {
    name: 'After Update Record',
    desc: 'Fires asynchronously after an update transaction finishes. Use for sync logs or refreshing indexes.',
    signature: 'export default async function(e: HookEvent): Promise<void>',
    payload: `{\n  "record": { "id": 42, "data": { "title": "Updated Title" } },\n  "collection": { "id": 1, "name": "posts" },\n  "auth": { "id": 1, "email": "admin@apexkit.io", "role": "admin" }\n}`,
    template: `export default async function(e) {\n  log("Record updated successfully: " + e.record.id);\n}`,
    returns: 'Void.'
  },
  before_delete_record: {
    name: 'Before Delete Record',
    desc: 'Intercepts record deletions before the transaction is finalized. Allows enforcing foreign constraint checks (e.g. prevent deleting a category if active products exist).',
    signature: 'export default async function(e: HookEvent): Promise<boolean>',
    payload: `{\n  "record": { "id": 42, "data": { "title": "Post To Delete" } },\n  "collection": { "id": 1, "name": "posts" },\n  "auth": { "id": 1, "email": "admin@apexkit.io", "role": "admin" }\n}`,
    template: `export default async function(e) {\n  if (e.record.data.is_protected) {\n    throw "This system record is protected and cannot be deleted.";\n  }\n  return true;\n}`,
    returns: 'Return `true` to allow deletion. Throw an error or return `false` to abort the transaction.'
  },
  after_delete_record: {
    name: 'After Delete Record',
    desc: 'Fires asynchronously after record deletion. Useful for cleanups (e.g., deleting orphaned assets from S3 storage).',
    signature: 'export default async function(e: HookEvent): Promise<void>',
    payload: `{\n  "record": { "id": 42, "data": { "title": "Post To Delete" } },\n  "collection": { "id": 1, "name": "posts" }\n}`,
    template: `export default async function(e) {\n  log("Record deleted. Purging files associated with ID: " + e.record.id);\n}`,
    returns: 'Void.'
  },

  // Record Read
  before_list_records: {
    name: 'Before List Records (Filter Query)',
    desc: 'Intercepts record list/page queries. Allows modifying or appending system filter parameters (e.g. enforce scoping tenant_id to active tenant without trusting frontend variables).',
    signature: 'export default async function(e: HookEvent): Promise<QueryOptions>',
    payload: `{\n  "data": { "limit": 20, "page": 1, "filter": "status = 'active'" },\n  "collection": { "id": 1, "name": "posts" },\n  "auth": { "id": 1, "email": "admin@apexkit.io", "role": "admin" }\n}`,
    template: `export default async function(e) {\n  // Force filter on active user scope\n  e.data.filter = e.data.filter ? e.data.filter + " AND owner_id = " + e.auth.id : "owner_id = " + e.auth.id;\n  return e.data;\n}`,
    returns: 'Must return the modified `QueryOptions` JSON object.'
  },
  after_list_records: {
    name: 'After List Records (Filter Output)',
    desc: 'Post-processing filter executed after a list is loaded from SQLite, but before sending the response to the client. Ideal for on-the-fly decryption or data redaction.',
    signature: 'export default async function(e: HookEvent): Promise<RecordListResponse>',
    payload: `{\n  "data": { "items": [{ "id": 1, "data": { "email": "hidden@test.com" } }], "total": 1 },\n  "collection": { "id": 1, "name": "posts" }\n}`,
    template: `export default async function(e) {\n  // Redact emails if requester is not admin\n  if (e.auth?.role !== "admin") {\n    e.data.items.forEach(item => {\n      item.data.email = "******";\n    });\n  }\n  return e.data;\n}`,
    returns: 'Must return the modified record list payload.'
  },
  before_get_record: {
    name: 'Before Get Record',
    desc: 'Fires before retrieving a single record from SQLite. You can inspect request parameters or abort the get.',
    signature: 'export default async function(e: HookEvent): Promise<void>',
    payload: `{\n  "data": { "id": 42 },\n  "collection": { "id": 1, "name": "posts" }\n}`,
    template: `export default async function(e) {\n  log("Fired before loading record #" + e.data.id);\n}`,
    returns: 'Void.'
  },
  after_get_record: {
    name: 'After Get Record (Filter Output)',
    desc: 'Post-processing filter executed after loading a single record. Ideal for decrypting fields or applying dynamic access constraints.',
    signature: 'export default async function(e: HookEvent): Promise<RecordResponse>',
    payload: `{\n  "data": { "id": 42, "data": { "ssn": "encrypted_value" } },\n  "collection": { "id": 1, "name": "posts" }\n}`,
    template: `export default async function(e) {\n  // Perform decryption if authorized\n  if (e.auth?.role === "admin") {\n    // e.data.data.ssn = decrypt(e.data.data.ssn)\n  }\n  return e.data;\n}`,
    returns: 'Must return the modified record response.'
  },

  // Schema
  before_collection_create: {
    name: 'Before Create Collection',
    desc: 'Fires before a new collection schema is committed to the database. Useful for enforcing naming conventions or globally blocking creation.',
    signature: 'export default async function(e: HookEvent): Promise<CollectionData | false>',
    payload: `{\n  "data": { "name": "new_table", "schema": { "fields": {} } },\n  "auth": { "id": 1, "role": "admin" }\n}`,
    template: `export default async function(e) {\n  if (!e.data.name.startsWith("prefix_")) {\n    throw "All collections must start with 'prefix_'";\n  }\n  return e.data;\n}`,
    returns: 'Return the modified collection object, or throw an error to abort.'
  },
  after_collection_create: {
    name: 'After Create Collection',
    desc: 'Fires asynchronously after a collection is created. Useful for audit logging or dispatching notifications.',
    signature: 'export default async function(e: HookEvent): Promise<void>',
    payload: `{\n  "data": { "id": 10, "name": "new_table" }\n}`,
    template: `export default async function(e) {\n  log("Collection created: " + e.data.name);\n}`,
    returns: 'Void.'
  },
  before_collection_update: {
    name: 'Before Update Collection',
    desc: 'Fires before a collection schema is modified. Useful for blocking destructive changes (e.g., preventing field deletion).',
    signature: 'export default async function(e: HookEvent): Promise<CollectionUpdate | false>',
    payload: `{\n  "data": { "id": 10, "updates": { "name": "new_name" } },\n  "auth": { "id": 1, "role": "admin" }\n}`,
    template: `export default async function(e) {\n  // Example: Prevent renaming\n  if (e.data.updates.name) {\n    throw "Renaming collections is disabled by policy.";\n  }\n  return e.data.updates;\n}`,
    returns: 'Return the modified update payload, or throw an error to block.'
  },
  after_collection_update: {
    name: 'After Update Collection',
    desc: 'Fires asynchronously after a collection schema is modified.',
    signature: 'export default async function(e: HookEvent): Promise<void>',
    payload: `{\n  "data": { "id": 10 }\n}`,
    template: `export default async function(e) {\n  log("Collection updated: " + e.data.id);\n}`,
    returns: 'Void.'
  },
  before_collection_delete: {
    name: 'Before Delete Collection',
    desc: 'Intercepts a collection deletion request. Return false or throw an error to prevent dropping the table.',
    signature: 'export default async function(e: HookEvent): Promise<boolean>',
    payload: `{\n  "data": { "id": 10 },\n  "auth": { "role": "admin" }\n}`,
    template: `export default async function(e) {\n  throw "Collection deletion is disabled in production.";\n}`,
    returns: 'Return `true` to allow, throw an error to block.'
  },

  // Traffic
  before_tenant_request: {
    name: 'Before Tenant Request',
    desc: 'A global middleware hook that intercepts EVERY HTTP request routed to a tenant. Use to enforce custom IP blocking, WAF logic, or rate limits.',
    signature: 'export default async function(e: HookEvent): Promise<boolean>',
    payload: `{\n  "data": {\n    "tenant_id": "app-1",\n    "path": "/api/v1/collections",\n    "method": "GET",\n    "ip": "192.168.1.1",\n    "ingress": 1024,\n    "egress": 0\n  }\n}`,
    template: `export default async function(e) {\n  // Block a specific IP\n  if (e.data.ip === "10.0.0.1") {\n    throw "IP Blocked by WAF";\n  }\n  return true;\n}`,
    returns: 'Return `true` to allow the request to proceed. Throw an error to return a 429/403 HTTP response.'
  },
  after_tenant_request: {
    name: 'After Tenant Request',
    desc: 'Fires asynchronously after a tenant HTTP request completes. Ideal for capturing egress/ingress byte counts for custom billing integrations.',
    signature: 'export default async function(e: HookEvent): Promise<void>',
    payload: `{\n  "data": {\n    "tenant_id": "app-1",\n    "path": "/api/v1/collections",\n    "status": 200,\n    "ingress": 1024,\n    "egress": 4096\n  }\n}`,
    template: `export default async function(e) {\n  // Use $cache.incr to aggregate bandwidth usage for billing\n  await $cache.incr("bandwidth_" + e.data.tenant_id, e.data.egress);\n}`,
    returns: 'Void.'
  },
  before_sandbox_request: {
    name: 'Before Sandbox Request',
    desc: 'A global middleware hook that intercepts HTTP requests routed to an ephemeral Sandbox session.',
    signature: 'export default async function(e: HookEvent): Promise<boolean>',
    payload: `{\n  "data": { "sandbox_id": "abc-123", "path": "/", "method": "GET", "ip": "...", "ingress": 0 }\n}`,
    template: `export default async function(e) {\n  return true;\n}`,
    returns: 'Return `true` to allow, throw an error to block.'
  },
  after_sandbox_request: {
    name: 'After Sandbox Request',
    desc: 'Fires asynchronously after a Sandbox HTTP request completes.',
    signature: 'export default async function(e: HookEvent): Promise<void>',
    payload: `{\n  "data": { "sandbox_id": "abc-123", "path": "/", "status": 200, "ingress": 0, "egress": 1024 }\n}`,
    template: `export default async function(e) {\n  // log sandbox traffic\n}`,
    returns: 'Void.'
  }
};

export const ScriptEditor = ({ isOpen, onClose, onSave, initialData }: ScriptEditorProps) => {
  const [formData, setFormData] = useState<Partial<Script & { visibility: string }>>({
    name: '',
    trigger_type: 'manual',
    target_collection: '',
    visibility: 'private',
    code: DEFAULT_CODE.manual,
    active: true,
  });
  const [collections, setCollections] = useState<Collection[]>([]);
  const [isSaving, setIsSaving] = useState(false);
  const [isRoot, setIsRoot] = useState(false);
  const isShared = (initialData as any)?.isShared;

  // State for trigger documentation modal
  const [isDocsOpen, setIsDocsOpen] = useState(false);
  const activeDoc = TRIGGER_DOCS[formData.trigger_type || 'manual'] || TRIGGER_DOCS.manual;

  useEffect(() => {
    collectionsService.list().then(setCollections);
    setIsRoot(apiClient.getScope().type === 'root');
  }, []);

  useEffect(() => {
    if (initialData) {
      setFormData({
        ...initialData,
        target_collection: initialData.target_collection || '',
        visibility: (initialData as any).visibility || 'private',
      });
    } else {
      setFormData({
        name: '',
        trigger_type: 'manual',
        target_collection: '',
        visibility: 'private',
        code: DEFAULT_CODE.manual,
        active: true,
      });
    }
  }, [initialData, isOpen]);

  const handleSave = async () => {
    setIsSaving(true);
    try {
      const cleanData = {
        ...formData,
        target_collection: isScopedByCollection(formData.trigger_type || '')
          ? formData.target_collection
          : null,
      };
      await onSave(cleanData);
      onClose();
    } finally {
      setIsSaving(false);
    }
  };

  const isScopedByCollection = (type: string) => {
    return (
      type.includes('_create') ||
      type.includes('_update') ||
      type.includes('_delete') ||
      type.includes('_records') ||
      type.includes('_record')
    );
  };

  const handleTriggerChange = (type: string) => {
    let newCode = formData.code;
    const isDefault = Object.values(DEFAULT_CODE).some((code) => formData.code === code);

    if (isDefault) {
      if (type === 'manual') newCode = DEFAULT_CODE.manual;
      else if (type === 'cron') newCode = DEFAULT_CODE.cron;
      else if (type === 'graphql') newCode = DEFAULT_CODE.graphql;
      else if (type.includes('_create') || type.includes('_update')) newCode = DEFAULT_CODE.hook;
      else if (type.includes('_list_') || type.includes('_get_')) newCode = DEFAULT_CODE.filter;
      else if (type.includes('_request')) newCode = DEFAULT_CODE.traffic;
      else newCode = DEFAULT_CODE.system;
    }
    setFormData({ ...formData, trigger_type: type, code: newCode });
  };

  if (!isOpen) return null;

  return (
    <Dialog
      isOpen={isOpen}
      onClose={onClose}
      title={initialData ? 'Edit Script' : 'New Script'}
      size="xl"
    >
      <div className="flex flex-col h-[85vh]">
        <div className="grid grid-cols-1 md:grid-cols-3 gap-6 mb-6 h-full overflow-hidden">
          
          {/* Sidebar Settings */}
          <div className="space-y-5 overflow-y-auto pr-2 custom-scrollbar">
            
            {/* Script Name */}
            <div className="space-y-2">
              <Label required>Script Name / ID</Label>
              <Input
                value={formData.name}
                onChange={(e: any) => setFormData({ ...formData, name: e.target.value })}
                placeholder="e.g. process-payment"
                className="font-mono text-sm"
                disabled={!!initialData}
              />
              <p className="text-[10px] text-muted-foreground">
                {formData.trigger_type === 'manual'
                  ? 'Public URL: /api/v1/run/' + (formData.name || '...')
                  : 'System identifier.'}
              </p>
            </div>

            {/* Trigger Type & Docs Info Button */}
            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <Label>Trigger Type</Label>
                <button
                  type="button"
                  onClick={() => setIsDocsOpen(true)}
                  className="text-xs text-primary hover:text-primary/85 flex items-center gap-1 transition-colors font-semibold"
                  title="View Specifications & Signature"
                >
                  <HelpCircle className="h-3.5 w-3.5" /> Trigger Specs
                </button>
              </div>
              <Select
                value={formData.trigger_type}
                onChange={(e: any) => handleTriggerChange(e.target.value)}
              >
                {TRIGGER_TYPES.reduce((acc: any[], t) => {
                  const group = t.group;
                  if (!acc.find((g) => g.label === group)) {
                    acc.push({ label: group, options: [] });
                  }
                  acc.find((g) => g.label === group).options.push(t);
                  return acc;
                }, []).map((group: any) => (
                  <optgroup key={group.label} label={group.label}>
                    {group.options.map((t: any) => (
                      <option key={t.value} value={t.value}>
                        {t.label}
                      </option>
                    ))}
                  </optgroup>
                ))}
              </Select>
            </div>

            {/* Visibility Field */}
            <div className="space-y-2">
              <Label className="flex items-center gap-2">
                {formData.visibility === 'public' ? (
                  <Globe className="h-3 w-3 text-primary" />
                ) : (
                  <Lock className="h-3 w-3 text-muted-foreground" />
                )}
                Visibility
              </Label>
              <Select
                value={formData.visibility || 'private'}
                onChange={(e: any) => setFormData({ ...formData, visibility: e.target.value })}
                disabled={!isRoot}
              >
                <option value="private">Private (Current Scope Only)</option>
                <option value="public">Public (Shared Root Script)</option>
              </Select>
              <p className="text-[10px] text-muted-foreground">
                {formData.visibility === 'public'
                  ? 'Tenants can call this script via $run.script().'
                  : 'Only accessible within this environment.'}
              </p>
            </div>

            {/* Target Collection Field */}
            {isScopedByCollection(formData.trigger_type || '') && (
              <div className="space-y-2 animate-in fade-in slide-in-from-top-2">
                <Label className="flex items-center gap-2 text-primary">
                  <Database className="h-3 w-3" /> Target Collection
                </Label>
                <Select
                  value={formData.target_collection || ''}
                  onChange={(e: any) =>
                    setFormData({ ...formData, target_collection: e.target.value || '' })
                  }
                >
                  <option value="">(Global - All Collections)</option>
                  {collections.map((c) => (
                    <option key={c.name} value={c.name}>
                      {c.name}
                    </option>
                  ))}
                </Select>
                <p className="text-[10px] text-muted-foreground">
                  Attach this hook to a specific table.
                </p>
              </div>
            )}

            {/* Active Status Toggle */}
            <div className="flex items-center justify-between p-3 border border-border rounded-lg bg-secondary/5">
              <Label
                className="cursor-pointer"
                onClick={() => setFormData({ ...formData, active: !formData.active })}
              >
                Active Status
              </Label>
              <Switch
                checked={formData.active}
                onCheckedChange={(c: boolean) => setFormData({ ...formData, active: c })}
              />
            </div>

            {/* Copilot AI Extension */}
            <div className="pt-4 border-t border-border">
              <Label className="mb-2 block text-xs uppercase tracking-widest text-muted-foreground">
                Copilot
              </Label>
              <AiCodeAssistant
                currentCode={formData.code || ''}
                contextType="script"
                onApply={(code) => setFormData({ ...formData, code })}
              />
            </div>
          </div>

          {/* Editor Area */}
          <div className="md:col-span-2 flex flex-col h-full border-l border-border pl-0 md:pl-6">
            <div className="flex items-center justify-between mb-2">
              <div className="flex items-center gap-2">
                <ShieldCheck className="h-4 w-4 text-emerald-500" />
                <span className="text-sm font-semibold">Server Runtime (Boa)</span>
              </div>
              <div className="text-[10px] text-muted-foreground font-mono">
                {isScopedByCollection(formData.trigger_type || '')
                  ? 'Context: e.record, e.auth'
                  : 'Globals: $db, $http, $run'}
              </div>
            </div>

            <div className="flex-1 min-h-[400px]">
              <CodeEditor
                value={formData.code || ''}
                onChange={(val) => setFormData({ ...formData, code: val })}
                language="javascript"
                withTypes={true}
                height="100%"
                label="JS LOGIC"
                collections={collections}
              />
            </div>
          </div>
        </div>

        {/* Modal Footer */}
        <div className="flex justify-end gap-3 pt-4 border-t border-border mt-auto">
          <Button variant="ghost" onClick={onClose}>
            {isShared ? 'Close' : 'Cancel'}
          </Button>
          {!isShared && (
            <Button onClick={handleSave} isLoading={isSaving} disabled={!formData.name}>
              <Save className="mr-2 h-4 w-4" /> Save Script
            </Button>
          )}
        </div>
      </div>

      {/* Trigger Type Documentation Dialog */}
      <Dialog
        isOpen={isDocsOpen}
        onClose={() => setIsDocsOpen(false)}
        title={`${activeDoc.name} Specs`}
        size="lg"
        zIndex={75} // Ensure it renders above the main Script Editor modal
      >
        <div className="space-y-5 pb-4 font-sans text-sm text-muted-foreground leading-relaxed">
          <div>
            <h4 className="text-sm font-bold text-foreground mb-1">Description</h4>
            <p>{activeDoc.desc}</p>
          </div>

          <Separator />

          <div>
            <h4 className="text-sm font-bold text-foreground mb-1.5 flex items-center gap-1.5">
              <Code className="h-4 w-4 text-primary" /> Function Signature
            </h4>
            <pre className="p-3 bg-[#0d1117] rounded-lg border border-white/5 font-mono text-xs text-emerald-400 overflow-x-auto">
              <code>{activeDoc.signature}</code>
            </pre>
          </div>

          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            <div>
              <h4 className="text-sm font-bold text-foreground mb-1.5 flex items-center gap-1.5">
                <Database className="h-4 w-4 text-blue-400" /> Event Parameter (Payload)
              </h4>
              <pre className="p-3 bg-[#0d1117] rounded-lg border border-white/5 font-mono text-xs text-[#e6edf3] overflow-x-auto h-[160px] custom-scrollbar">
                <code>{activeDoc.payload}</code>
              </pre>
            </div>
            <div>
              <h4 className="text-sm font-bold text-foreground mb-1.5 flex items-center gap-1.5">
                <Play className="h-4 w-4 text-amber-400" /> Code Template
              </h4>
              <pre className="p-3 bg-[#0d1117] rounded-lg border border-white/5 font-mono text-xs text-[#e6edf3] overflow-x-auto h-[160px] custom-scrollbar">
                <code>{activeDoc.template}</code>
              </pre>
            </div>
          </div>

          <Separator />

          <div>
            <h4 className="text-sm font-bold text-foreground mb-1">Returns</h4>
            <p className="text-xs text-foreground bg-secondary/20 p-2.5 rounded border border-border border-dashed leading-relaxed">
              {activeDoc.returns}
            </p>
          </div>

          <div className="pt-4 flex justify-end">
            <Button onClick={() => setIsDocsOpen(false)}>Done</Button>
          </div>
        </div>
      </Dialog>
    </Dialog>
  );
};