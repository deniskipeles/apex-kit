import React, { useState, useEffect } from 'react';
import { Save, Code, Database } from 'lucide-react';
import { Button, Input, Label, Select, Switch } from '../../../components/ui/Elements';
import { Dialog } from '../../../components/ui/Dialog';
import { Script, Collection } from '../../../types';
import { AiCodeAssistant } from '../../ai/components/AiCodeAssistant';
import { collectionsService } from '../../collections/services/collectionsService';
import { CodeEditor } from '../../../components/form/CodeEditor'; // Import Monaco Editor

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
    { value: 'before_create', label: 'Before Create Record', group: 'Record Write' },
    { value: 'after_create', label: 'After Create Record', group: 'Record Write' },
    { value: 'before_update', label: 'Before Update Record', group: 'Record Write' },
    { value: 'after_update', label: 'After Update Record', group: 'Record Write' },
    { value: 'before_delete', label: 'Before Delete Record', group: 'Record Write' },
    { value: 'after_delete', label: 'After Delete Record', group: 'Record Write' },
    
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
    { value: 'before_list_collections', label: 'Before List Collections', group: 'Schema' },
    { value: 'after_list_collections', label: 'After List Collections', group: 'Schema' },
    { value: 'before_get_collection', label: 'Before Get Collection', group: 'Schema' },
    { value: 'after_get_collection', label: 'After Get Collection', group: 'Schema' },

    // --- Relations ---
    { value: 'before_relation_create', label: 'Before Create Relation', group: 'Relations' },
    { value: 'after_relation_create', label: 'After Create Relation', group: 'Relations' },
    { value: 'before_relation_delete', label: 'Before Delete Relation', group: 'Relations' },
    { value: 'after_relation_delete', label: 'After Delete Relation', group: 'Relations' },

    // --- Users & Auth ---
    { value: 'before_user_create', label: 'Before User Register', group: 'Auth' },
    { value: 'after_user_create', label: 'After User Register', group: 'Auth' },
    { value: 'before_user_delete', label: 'Before User Delete', group: 'Auth' },
    { value: 'after_user_delete', label: 'After User Delete', group: 'Auth' },
    { value: 'before_list_users', label: 'Before List Users', group: 'Auth' },
    { value: 'after_list_users', label: 'After List Users', group: 'Auth' },

    // --- Files ---
    { value: 'before_file_upload', label: 'Before File Upload', group: 'Storage' },
    { value: 'after_file_upload', label: 'After File Upload', group: 'Storage' },
    { value: 'before_file_delete', label: 'Before File Delete', group: 'Storage' },

    // --- AI & Vectors ---
    { value: 'before_ai_run', label: 'Before AI Action', group: 'AI' },
    { value: 'after_ai_run', label: 'After AI Action', group: 'AI' },
    { value: 'on_vectorization_start', label: 'On Vectorization Start', group: 'AI' },

    // --- Multi-Tenancy ---
    { value: 'before_tenant_create', label: 'Before Tenant Provision', group: 'Tenant' },
    { value: 'after_tenant_create', label: 'After Tenant Provision', group: 'Tenant' },
    { value: 'before_list_tenants', label: 'Before List Tenants', group: 'Tenant' },

    // --- [NEW] Tenant & Sandbox Requests (Traffic/Quota) ---
    { value: 'before_tenant_request', label: 'Before Tenant Request', group: 'Traffic' },
    { value: 'after_tenant_request', label: 'After Tenant Request', group: 'Traffic' },
    { value: 'before_sandbox_request', label: 'Before Sandbox Request', group: 'Traffic' },
    { value: 'after_sandbox_request', label: 'After Sandbox Request', group: 'Traffic' },
];

const DEFAULT_CODE = {
    manual: `// Manual API Endpoint\n// POST /api/v1/run/{script_name}\nexport default async function(req) {\n    const body = await req.json();\n    return new Response({ message: "Hello!" });\n}`,
    
    cron: `// Scheduled Job\nexport default async function() {\n    log("Running cron job...");\n    // await $db.delete("logs", ...)\n}`,
    
    hook: `// Record Hook (Write)\n// Context: e.record, e.collection, e.auth\nexport default async function(e) {\n    if (!e.record.data.title) throw new Error("Title required");\n    // Return modified data to be saved\n    return e.record.data;\n}`,

    filter: `// Filter/Read Hook\n// Context: e.data, e.auth\nexport default async function(e) {\n    // Modify query parameters or output data\n    // e.g. e.data.items = e.data.items.filter(i => ...)\n    return e.data;\n}`,

    system: `// System Event Hook\n// Context: e.trigger, e.data, e.auth\nexport default async function(e) {\n    log("Event Triggered: " + e.trigger);\n    // Throwing error blocks operation for 'before_' hooks\n    // if (e.trigger === 'before_file_upload' && e.auth.role !== 'admin') throw new Error("Admins only");\n}`,

    graphql: `// GraphQL Resolver Configuration
    // NOTE: Use strict JSON syntax for keys/values in the graphql object
    export const graphql = {
    "parent": "Query",
    "name": "myCustomField",
    "args": {
        "someArg": "String!"
    },
    "returnType": "JSON"
    };

    export default async function(req) {
    const args = await req.json(); // Arguments passed here
    
    return new Response({
        received: args.someArg,
        timestamp: new Date().toISOString()
    });
    }`,

    // [ADD] Default for Traffic Hooks
    traffic: `// Traffic Control Hook
    // Context: e.data.path, e.data.ip, e.data.method
    export default async function(e) {
        // Example: Rate Limit or Audit
        // const key = "ip:" + e.data.ip;
        // const count = await $cache.incr(key, 1);
        // if (count > 100) throw new Error("Rate limit exceeded");
        
        // For 'after_' hooks, e.data.status is available
        if (e.trigger.startsWith('after_')) {
            log(e.trigger + " " + e.data.path + " " + e.data.status);
        }
    }`
};

export const ScriptEditor = ({ isOpen, onClose, onSave, initialData }: ScriptEditorProps) => {
    const [formData, setFormData] = useState<Partial<Script>>({
        name: '',
        trigger_type: 'manual',
        target_collection: null,
        code: DEFAULT_CODE.manual,
        active: true
    });
    const [collections, setCollections] = useState<Collection[]>([]);
    const [isSaving, setIsSaving] = useState(false);

    useEffect(() => {
        collectionsService.list().then(setCollections);
    }, []);

    useEffect(() => {
        if (initialData) {
            setFormData(initialData);
        } else {
            setFormData({
                name: '',
                trigger_type: 'manual',
                target_collection: null,
                code: DEFAULT_CODE.manual,
                active: true
            });
        }
    }, [initialData, isOpen]);

    const handleSave = async () => {
        setIsSaving(true);
        try {
            const cleanData = {
                ...formData,
                // Only save target_collection if it is a Record Hook
                target_collection: isRecordHook(formData.trigger_type || '') ? formData.target_collection : null
            };
            await onSave(cleanData);
            onClose();
        } finally {
            setIsSaving(false);
        }
    };

    const isRecordHook = (type: string) => [
        'before_create', 'after_create', 'before_update', 'after_update', 'before_delete', 'after_delete'
    ].includes(type);

    const handleTriggerChange = (type: string) => {
        let newCode = formData.code;
        
        // Only reset code if it looks like one of the defaults
        const isDefault = Object.values(DEFAULT_CODE).some(code => formData.code === code);
        
        if (isDefault) {
            if (type === 'manual') {
                newCode = DEFAULT_CODE.manual;
            } else if (type === 'cron') {
                newCode = DEFAULT_CODE.cron;
            } else if (type === 'graphql') {
                newCode = DEFAULT_CODE.graphql;
            } else if (isRecordHook(type)) {
                newCode = DEFAULT_CODE.hook;
            } else if (type.includes('_list_') || type.includes('_get_')) {
                newCode = DEFAULT_CODE.filter;
            // [NEW] Check for traffic hooks
            } else if (type.includes('_request')) {
                newCode = DEFAULT_CODE.traffic;
            } else {
                newCode = DEFAULT_CODE.system;
            }
        }
        setFormData({ ...formData, trigger_type: type, code: newCode });
    };

    if (!isOpen) return null;

    return (
        <Dialog isOpen={isOpen} onClose={onClose} title={initialData ? 'Edit Script' : 'New Script'} size="xl">
            <div className="flex flex-col h-[85vh]"> 
                <div className="grid grid-cols-1 md:grid-cols-3 gap-6 mb-6 h-full overflow-hidden">

                    {/* Sidebar Settings */}
                    <div className="space-y-5 overflow-y-auto pr-2">
                        <div className="space-y-2">
                            <Label>Script Name / ID</Label>
                            <Input
                                value={formData.name}
                                onChange={(e: any) => setFormData({ ...formData, name: e.target.value })}
                                placeholder="e.g. validate-post"
                                className="font-mono"
                                disabled={!!initialData}
                            />
                            <p className="text-[10px] text-muted-foreground">
                                {formData.trigger_type === 'manual' ? 'Public URL: /api/v1/run/' + (formData.name || '...') : 'Unique identifier for this script.'}
                            </p>
                        </div>

                        <div className="space-y-2">
                            <Label>Trigger Type</Label>
                            <Select
                                value={formData.trigger_type}
                                onChange={(e: any) => handleTriggerChange(e.target.value)}
                            >
                                {TRIGGER_TYPES.reduce((acc: any[], t) => {
                                    const group = t.group;
                                    if (!acc.find(g => g.label === group)) {
                                        acc.push({ label: group, options: [] });
                                    }
                                    acc.find(g => g.label === group).options.push(t);
                                    return acc;
                                }, []).map((group: any) => (
                                    <optgroup key={group.label} label={group.label}>
                                        {group.options.map((t: any) => (
                                            <option key={t.value} value={t.value}>{t.label}</option>
                                        ))}
                                    </optgroup>
                                ))}
                            </Select>
                        </div>

                        {isRecordHook(formData.trigger_type || '') && (
                            <div className="space-y-2 animate-in fade-in slide-in-from-top-2">
                                <Label className="flex items-center gap-2 text-primary"><Database className="h-3 w-3" /> Target Collection</Label>
                                <Select
                                    value={formData.target_collection || ''}
                                    onChange={(e: any) => setFormData({ ...formData, target_collection: e.target.value || null })}
                                >
                                    <option value="">(Global - All Collections)</option>
                                    {collections.map(c => (
                                        <option key={c.name} value={c.name}>{c.name}</option>
                                    ))}
                                </Select>
                                <p className="text-[10px] text-muted-foreground">Optional. Runs on all collections if empty.</p>
                            </div>
                        )}

                        <div className="flex items-center justify-between p-3 border border-border rounded-lg bg-secondary/5">
                            <Label className="cursor-pointer" onClick={() => setFormData({ ...formData, active: !formData.active })}>Active</Label>
                            <Switch checked={formData.active} onCheckedChange={(c: boolean) => setFormData({ ...formData, active: c })} />
                        </div>

                        <div className="pt-4 border-t border-border">
                            <Label className="mb-2 block">AI Assistant</Label>
                            <AiCodeAssistant
                                currentCode={formData.code || ''}
                                contextType="script"
                                onApply={(code) => setFormData({ ...formData, code })}
                            />
                        </div>
                    </div>

                    {/* Code Editor Area */}
                    <div className="md:col-span-2 flex flex-col h-full border-l border-border pl-0 md:pl-6">
                        <div className="flex items-center justify-between mb-2">
                            <Label className="flex items-center gap-2"><Code className="h-4 w-4" /> JavaScript Logic</Label>
                            <div className="text-[10px] text-muted-foreground font-mono">
                                {isRecordHook(formData.trigger_type || '') ? 'Context: e.record, e.auth' : 'Globals: $db, $http'}
                            </div>
                        </div>

                        <div className="flex-1 min-h-[400px]">
                            <CodeEditor
                                value={formData.code || ''}
                                onChange={(val) => setFormData({ ...formData, code: val })}
                                language="javascript"
                                withTypes={true} 
                                height="100%"
                                label="SERVER SCRIPT"
                                collections={collections}
                            />
                        </div>
                    </div>
                </div>

                <div className="flex justify-end gap-3 pt-4 border-t border-border mt-auto">
                    <Button variant="ghost" onClick={onClose}>Cancel</Button>
                    <Button onClick={handleSave} isLoading={isSaving} disabled={!formData.name}>
                        <Save className="mr-2 h-4 w-4" /> Save Script
                    </Button>
                </div>
            </div>
        </Dialog>
    );
};