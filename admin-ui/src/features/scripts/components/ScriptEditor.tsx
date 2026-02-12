import React, { useState, useEffect } from 'react';
import { Save, Code, Database, Globe, Lock, Info, ShieldCheck } from 'lucide-react';
import { Button, Input, Label, Select, Switch, Badge } from '../../../components/ui/Elements';
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

    // --- [NEW] Tenant & Sandbox Requests (Traffic/Quota) ---
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
    traffic: `export default async function(e) {\n    log(e.trigger + " " + e.data.path);\n}`
};

export const ScriptEditor = ({ isOpen, onClose, onSave, initialData }: ScriptEditorProps) => {
    const [formData, setFormData] = useState<Partial<Script & { visibility: string }>>({
        name: '',
        trigger_type: 'manual',
        target_collection: '',
        visibility: 'private',
        code: DEFAULT_CODE.manual,
        active: true
    });
    const [collections, setCollections] = useState<Collection[]>([]);
    const [isSaving, setIsSaving] = useState(false);
    const [isRoot, setIsRoot] = useState(false);

    useEffect(() => {
        collectionsService.list().then(setCollections);
        // setIsRoot(apiClient.getScope().type === 'root');
    }, []);

    useEffect(() => {
        if (initialData) {
            setFormData({
                ...initialData,
                target_collection: initialData.target_collection || '',
                visibility: (initialData as any).visibility || 'private'
            });
        } else {
            setFormData({
                name: '',
                trigger_type: 'manual',
                target_collection: '',
                visibility: 'private',
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
                target_collection: isScopedByCollection(formData.trigger_type || '') ? formData.target_collection : null
            };
            await onSave(cleanData);
            onClose();
        } finally {
            setIsSaving(false);
        }
    };

    // Determine if we should show the "Target Collection" dropdown
    const isScopedByCollection = (type: string) => {
        return type.includes('_create') || 
               type.includes('_update') || 
               type.includes('_delete') || 
               type.includes('_records') || 
               type.includes('_record');
    };

    const handleTriggerChange = (type: string) => {
        let newCode = formData.code;
        const isDefault = Object.values(DEFAULT_CODE).some(code => formData.code === code);
        
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
        <Dialog isOpen={isOpen} onClose={onClose} title={initialData ? 'Edit Script' : 'New Script'} size="xl">
            <div className="flex flex-col h-[85vh]"> 
                <div className="grid grid-cols-1 md:grid-cols-3 gap-6 mb-6 h-full overflow-hidden">

                    {/* Sidebar Settings */}
                    <div className="space-y-5 overflow-y-auto pr-2 custom-scrollbar">
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
                                {formData.trigger_type === 'manual' ? 'Public URL: /api/v1/run/' + (formData.name || '...') : 'System identifier.'}
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

                        {/* VISIBILITY FIELD */}
                        <div className="space-y-2">
                            <Label className="flex items-center gap-2">
                                {formData.visibility === 'public' ? <Globe className="h-3 w-3 text-primary" /> : <Lock className="h-3 w-3 text-muted-foreground" />}
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

                        {/* TARGET COLLECTION FIELD */}
                        {isScopedByCollection(formData.trigger_type || '') && (
                            <div className="space-y-2 animate-in fade-in slide-in-from-top-2">
                                <Label className="flex items-center gap-2 text-primary">
                                    <Database className="h-3 w-3" /> Target Collection
                                </Label>
                                <Select
                                    value={formData.target_collection || ''}
                                    onChange={(e: any) => setFormData({ ...formData, target_collection: e.target.value || '' })}
                                >
                                    <option value="">(Global - All Collections)</option>
                                    {collections.map(c => (
                                        <option key={c.name} value={c.name}>{c.name}</option>
                                    ))}
                                </Select>
                                <p className="text-[10px] text-muted-foreground">Attach this hook to a specific table.</p>
                            </div>
                        )}

                        <div className="flex items-center justify-between p-3 border border-border rounded-lg bg-secondary/5">
                            <Label className="cursor-pointer" onClick={() => setFormData({ ...formData, active: !formData.active })}>Active Status</Label>
                            <Switch checked={formData.active} onCheckedChange={(c: boolean) => setFormData({ ...formData, active: c })} />
                        </div>

                        <div className="pt-4 border-t border-border">
                            <Label className="mb-2 block text-xs uppercase tracking-widest text-muted-foreground">Copilot</Label>
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
                                {isScopedByCollection(formData.trigger_type || '') ? 'Context: e.record, e.auth' : 'Globals: $db, $http, $run'}
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
                    <Button variant="ghost" onClick={onClose}>Cancel</Button>
                    <Button onClick={handleSave} isLoading={isSaving} disabled={!formData.name}>
                        <Save className="mr-2 h-4 w-4" /> Save Script
                    </Button>
                </div>
            </div>
        </Dialog>
    );
};