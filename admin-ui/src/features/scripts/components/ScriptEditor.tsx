// =========================== /teamspace/studios/this_studio/tinybase/tinybase/admin-ui/src/features/scripts/components/ScriptEditor.tsx ===========================
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
    { value: 'manual', label: 'Manual Endpoint (API)', group: 'API' },
    { value: 'cron', label: 'Scheduled Job (Cron)', group: 'System' },
    { value: 'before_create', label: 'Before Create', group: 'Hooks' },
    { value: 'after_create', label: 'After Create', group: 'Hooks' },
    { value: 'before_update', label: 'Before Update', group: 'Hooks' },
    { value: 'after_update', label: 'After Update', group: 'Hooks' },
    { value: 'before_delete', label: 'Before Delete', group: 'Hooks' },
    { value: 'after_delete', label: 'After Delete', group: 'Hooks' },
];

const DEFAULT_CODE = {
    manual: `// Manual API Endpoint\nexport default async function(req) {\n    const body = await req.json();\n    return new Response({ message: "Hello!" });\n}`,
    cron: `// Scheduled Job\nexport default async function() {\n    log("Running cron job...");\n    // await $db.delete("logs", ...)\n}`,
    hook: `// Database Hook\nexport default async function(e) {\n    // e.record, e.collection, e.auth\n    if (!e.record.data.title) throw new Error("Title required");\n    return e.record.data; // Return modified data\n}`
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
                target_collection: isHookType(formData.trigger_type || '') ? formData.target_collection : null
            };
            await onSave(cleanData);
            onClose();
        } finally {
            setIsSaving(false);
        }
    };

    const isHookType = (type: string) => ['before_create', 'after_create', 'before_update', 'after_update', 'before_delete', 'after_delete'].includes(type);

    const handleTriggerChange = (type: string) => {
        let newCode = formData.code;
        // Only reset code if it looks like the default of another type
        if (formData.code === DEFAULT_CODE.manual || formData.code === DEFAULT_CODE.cron || formData.code === DEFAULT_CODE.hook) {
            if (type === 'manual') newCode = DEFAULT_CODE.manual;
            else if (type === 'cron') newCode = DEFAULT_CODE.cron;
            else newCode = DEFAULT_CODE.hook;
        }
        setFormData({ ...formData, trigger_type: type, code: newCode });
    };

    if (!isOpen) return null;

    return (
        <Dialog isOpen={isOpen} onClose={onClose} title={initialData ? 'Edit Script' : 'New Script'} size="xl">
            <div className="flex flex-col h-[85vh]"> {/* Increased height for editor */}
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
                                {TRIGGER_TYPES.map(t => <option key={t.value} value={t.value}>{t.label}</option>)}
                            </Select>
                        </div>

                        {isHookType(formData.trigger_type || '') && (
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

                        {/* AI Assistant moved here for better layout */}
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
                                {isHookType(formData.trigger_type || '') ? 'Context: e.record, e.auth' : 'Globals: $db, $http'}
                            </div>
                        </div>

                        <div className="flex-1 min-h-[400px]">
                            <CodeEditor
                                value={formData.code || ''}
                                onChange={(val) => setFormData({ ...formData, code: val })}
                                language="javascript"
                                withTypes={true} // Enables $db, $http auto-complete from TinyBase types
                                height="100%"
                                label="SERVER SCRIPT"
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