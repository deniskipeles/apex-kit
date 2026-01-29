import React, { useState, useEffect } from 'react';
import { Plus, Play, Trash2, Code, Zap, Database } from 'lucide-react';
import { Button, Badge } from '../../../components/ui/Elements';
import { DataGrid } from '../../../components/data/DataGrid';
import { ScriptEditor } from '../components/ScriptEditor';
import { ScriptTester } from '../components/ScriptTester';
import { scriptsService } from '../services/scriptsService';
import { Script } from '../../../types';
import { useToast } from '../../../components/feedback/Toast';
import { ConfirmDialog } from '../../../components/feedback/ConfirmDialog';
import { ImportExportToolbar } from '@/src/components/ImportExportToolbar';

export const ScriptsPage = () => {
    const [scripts, setScripts] = useState<Script[]>([]);
    const [isLoading, setIsLoading] = useState(true);
    const [editorOpen, setEditorOpen] = useState(false);
    const [testerOpen, setTesterOpen] = useState(false);
    const [selectedScript, setSelectedScript] = useState<Script | null>(null);
    const [scriptToDelete, setScriptToDelete] = useState<Script | null>(null);

    const { toast } = useToast();

    const loadScripts = async () => {
        setIsLoading(true);
        try {
            const data = await scriptsService.list();
            setScripts(data);
        } catch (e) {
            toast('Failed to load scripts', 'error');
        } finally {
            setIsLoading(false);
        }
    };

    useEffect(() => { loadScripts(); }, []);

    const handleCreate = async (data: Partial<Script>) => {
        await scriptsService.create(data);
        toast('Script saved successfully', 'success');
        loadScripts();
    };

    const handleDelete = async () => {
        if (!scriptToDelete) return;
        await scriptsService.delete(scriptToDelete.id);
        toast('Script deleted', 'success');
        setScriptToDelete(null);
        loadScripts();
    };

    const columns = [
        {
            field: 'name', headerName: 'Name',
            renderCell: (s: Script) => (
                <div className="flex flex-col">
                    <span className="font-medium text-primary font-mono">{s.name}</span>
                    {s.trigger_type === 'manual' && <span className="text-[10px] text-muted-foreground truncate max-w-[200px]">/api/v1/run/{s.name}</span>}
                </div>
            )
        },
        {
            field: 'trigger_type', headerName: 'Trigger', width: '140px',
            renderCell: (s: Script) => (
                <Badge variant="outline" className={`uppercase text-[10px] ${s.trigger_type === 'manual' ? 'border-blue-500/30 text-blue-500' : 'border-orange-500/30 text-orange-500'}`}>
                    {s.trigger_type.replace('_', ' ')}
                </Badge>
            )
        },
        {
            field: 'target_collection', headerName: 'Target', width: '120px',
            renderCell: (s: Script) => s.target_collection ? (
                <span className="flex items-center gap-1 text-xs text-foreground/80 bg-secondary/30 px-2 py-1 rounded">
                    <Database className="h-3 w-3 opacity-50" /> {s.target_collection}
                </span>
            ) : (
                <span className="text-muted-foreground text-xs opacity-50">-</span>
            )
        },
        {
            field: 'active', headerName: 'Status', width: '90px',
            renderCell: (s: Script) => s.active ? <span className="text-xs text-emerald-500 font-medium flex items-center gap-1"><Zap className="h-3 w-3" /> Active</span> : <span className="text-xs text-muted-foreground">Disabled</span>
        },
        {
            field: 'actions', headerName: '', align: 'right' as const, width: '120px',
            renderCell: (s: Script) => (
                <div className="flex justify-end gap-1">
                    {s.trigger_type === 'manual' && (
                        <Button size="icon" variant="ghost" onClick={(e) => { e.stopPropagation(); setSelectedScript(s); setTesterOpen(true); }} title="Test Run">
                            <Play className="h-4 w-4 text-emerald-500" />
                        </Button>
                    )}
                    <Button size="icon" variant="ghost" onClick={(e) => { e.stopPropagation(); setScriptToDelete(s); }}>
                        <Trash2 className="h-4 w-4 text-muted-foreground hover:text-destructive" />
                    </Button>
                </div>
            )
        }
    ];

    return (
        <div className="space-y-6">
            <div className="flex items-center justify-between">
                <div>
                    <h2 className="text-3xl font-bold tracking-tight">Scripting</h2>
                    <p className="text-muted-foreground">Custom server-side logic, endpoints, and event hooks.</p>
                </div>
                <div className="flex gap-2">
                    <ImportExportToolbar
                        onExport={scriptsService.export}
                        onImport={scriptsService.import}
                    />
                    <Button onClick={() => { setSelectedScript(null); setEditorOpen(true); }}>
                        <Plus className="mr-2 h-4 w-4" /> New Script
                    </Button>
                </div>
            </div>

            <DataGrid
                data={scripts}
                columns={columns}
                keyField="id"
                isLoading={isLoading}
                onRowClick={(s) => { setSelectedScript(s); setEditorOpen(true); }}
            />

            {/* Editor Modal */}
            <ScriptEditor
                isOpen={editorOpen}
                onClose={() => setEditorOpen(false)}
                onSave={handleCreate}
                initialData={selectedScript || undefined}
            />

            {/* Tester Modal */}
            {selectedScript && (
                <ScriptTester
                    script={selectedScript}
                    isOpen={testerOpen}
                    onClose={() => setTesterOpen(false)}
                />
            )}

            {/* Delete Confirm */}
            <ConfirmDialog
                isOpen={!!scriptToDelete}
                title="Delete Script"
                description={`Are you sure you want to delete "${scriptToDelete?.name}"?`}
                onConfirm={handleDelete}
                onCancel={() => setScriptToDelete(null)}
            />
        </div>
    );
};