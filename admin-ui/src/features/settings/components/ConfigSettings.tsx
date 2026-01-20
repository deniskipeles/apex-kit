import React, { useState, useEffect } from 'react';
import { Settings2, Plus, Trash2, Edit2, Lock, Eye, EyeOff, RefreshCw, FileJson, Type } from 'lucide-react';
import { Card, CardHeader, CardTitle, CardContent, Button, Input, Label, Switch, Badge } from '../../../components/ui/Elements';
import { Dialog } from '../../../components/ui/Dialog';
import { JSONEditor } from '../../../components/form/JsonEditor'; // Import JSON Editor
import { configService, ConfigItem } from '@/src/features/settings/services/configService';
import { useToast } from '../../../components/feedback/Toast';

export const ConfigSettings = () => {
    // state ...
    const [configs, setConfigs] = useState<ConfigItem[]>([]);
    const [isLoading, setIsLoading] = useState(true);
    const [isModalOpen, setIsModalOpen] = useState(false);
    const [editingItem, setEditingItem] = useState<ConfigItem | null>(null);
    
    // Form State
    const [formKey, setFormKey] = useState('');
    const [formValue, setFormValue] = useState('');
    const [formEncrypt, setFormEncrypt] = useState(false);
    const [isJsonMode, setIsJsonMode] = useState(false); // [NEW] Toggle mode
    const [isSaving, setIsSaving] = useState(false);

    const { toast } = useToast();

    // ... loadConfigs, handleDelete ...
    const loadConfigs = async () => {
        setIsLoading(true);
        try {
            const data = await configService.list();
            setConfigs(data);
        } catch (e) {
            toast('Failed to load system configs', 'error');
        } finally {
            setIsLoading(false);
        }
    };

    useEffect(() => { loadConfigs(); }, []);

    const handleDelete = async (key: string) => {
        if (!confirm(`Are you sure you want to delete '${key}'? This may break system functionality.`)) return;
        try {
            await configService.delete(key);
            toast('Configuration deleted', 'success');
            loadConfigs();
        } catch (e) {
            toast('Failed to delete', 'error');
        }
    };

    const handleOpenModal = (item?: ConfigItem) => {
        if (item) {
            setEditingItem(item);
            setFormKey(item.key);
            setFormEncrypt(item.encrypted);
            
            // Detect if value is JSON
            let isJson = false;
            let val = item.value || '';
            
            if (!item.encrypted) {
                try {
                    const parsed = JSON.parse(val);
                    if (typeof parsed === 'object' && parsed !== null) {
                        isJson = true;
                        // Prettify
                        val = JSON.stringify(parsed, null, 2);
                    }
                } catch {}
            }
            
            setIsJsonMode(isJson);
            setFormValue(val);
        } else {
            setEditingItem(null);
            setFormKey('');
            setFormValue('');
            setFormEncrypt(false);
            setIsJsonMode(false);
        }
        setIsModalOpen(true);
    };

    const handleSave = async () => {
        if (!formKey) return;
        setIsSaving(true);
        try {
            // If JSON mode, ensure valid JSON
            let valToSend = formValue;
            if (isJsonMode) {
                try {
                    // Minify for storage (optional, but good practice)
                    valToSend = JSON.stringify(JSON.parse(formValue));
                } catch {
                    toast('Invalid JSON format', 'error');
                    setIsSaving(false);
                    return;
                }
            }
            
            await configService.set(formKey, valToSend, formEncrypt);
            toast(`Configuration '${formKey}' saved`, 'success');
            setIsModalOpen(false);
            loadConfigs();
        } catch (e) {
            toast('Failed to save configuration', 'error');
        } finally {
            setIsSaving(false);
        }
    };

    return (
        <div className="space-y-6">
            <Card>
                {/* ... Header ... */}
                <CardHeader className="flex flex-row items-center justify-between">
                    <CardTitle className="flex items-center gap-2">
                        <Settings2 className="h-4 w-4" /> System Config Registry
                    </CardTitle>
                    <div className="flex gap-2">
                        <Button variant="ghost" size="icon" onClick={loadConfigs}><RefreshCw className="h-4 w-4" /></Button>
                        <Button size="sm" onClick={() => handleOpenModal()}><Plus className="mr-2 h-4 w-4" /> Add Key</Button>
                    </div>
                </CardHeader>

                <CardContent>
                    {/* ... Table ... */}
                    <div className="rounded-md border border-border bg-card overflow-hidden">
                        {configs.length === 0 ? (
                            <div className="p-8 text-center text-muted-foreground text-sm">No custom configurations found.</div>
                        ) : (
                            <table className="w-full text-sm text-left">
                                <thead className="bg-secondary/30 text-xs uppercase font-semibold text-muted-foreground">
                                    <tr>
                                        <th className="px-4 py-3">Key</th>
                                        <th className="px-4 py-3">Value</th>
                                        <th className="px-4 py-3 w-[100px]">Status</th>
                                        <th className="px-4 py-3 w-[120px] text-right">Updated</th>
                                        <th className="px-4 py-3 w-[100px]"></th>
                                    </tr>
                                </thead>
                                <tbody className="divide-y divide-border">
                                    {configs.map((item) => (
                                        <tr key={item.key} className="hover:bg-secondary/10 transition-colors group">
                                            <td className="px-4 py-3 font-mono font-medium text-primary">{item.key}</td>
                                            <td className="px-4 py-3 font-mono text-muted-foreground truncate max-w-[200px]">
                                                {item.encrypted ? '******' : item.value}
                                            </td>
                                            <td className="px-4 py-3">
                                                {item.encrypted ? (
                                                    <Badge variant="outline" className="text-[10px] gap-1 border-emerald-500/20 text-emerald-500 bg-emerald-500/10">
                                                        <Lock className="h-3 w-3" /> Encrypted
                                                    </Badge>
                                                ) : (
                                                    <Badge variant="secondary" className="text-[10px]">Plain</Badge>
                                                )}
                                            </td>
                                            <td className="px-4 py-3 text-right text-xs text-muted-foreground">
                                                {new Date(item.updated_at).toLocaleDateString()}
                                            </td>
                                            <td className="px-4 py-3 text-right">
                                                <div className="flex justify-end gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                                                    <Button variant="ghost" size="icon" className="h-8 w-8" onClick={() => handleOpenModal(item)}>
                                                        <Edit2 className="h-4 w-4" />
                                                    </Button>
                                                    <Button variant="ghost" size="icon" className="h-8 w-8 text-destructive hover:bg-destructive/10" onClick={() => handleDelete(item.key)}>
                                                        <Trash2 className="h-4 w-4" />
                                                    </Button>
                                                </div>
                                            </td>
                                        </tr>
                                    ))}
                                </tbody>
                            </table>
                        )}
                    </div>
                </CardContent>
            </Card>

            <Dialog isOpen={isModalOpen} onClose={() => setIsModalOpen(false)} title={editingItem ? 'Edit Configuration' : 'Add Configuration'} size="md">
                <div className="space-y-4">
                    <div className="space-y-2">
                        <Label required>Config Key</Label>
                        <Input 
                            value={formKey} 
                            onChange={(e: any) => setFormKey(e.target.value)} 
                            placeholder="e.g. STRIPE_SECRET_KEY" 
                            disabled={!!editingItem} 
                            className="font-mono"
                        />
                    </div>
                    
                    <div className="space-y-2">
                        <div className="flex justify-between items-center">
                            <Label required>Value</Label>
                            <div className="flex items-center gap-1 bg-secondary/30 p-0.5 rounded-lg border border-border">
                                <button 
                                    onClick={() => setIsJsonMode(false)}
                                    className={`px-2 py-0.5 rounded text-[10px] flex items-center gap-1 transition-all ${!isJsonMode ? 'bg-background shadow-sm text-foreground' : 'text-muted-foreground hover:text-foreground'}`}
                                >
                                    <Type className="h-3 w-3" /> Text
                                </button>
                                <button 
                                    onClick={() => setIsJsonMode(true)}
                                    className={`px-2 py-0.5 rounded text-[10px] flex items-center gap-1 transition-all ${isJsonMode ? 'bg-background shadow-sm text-foreground' : 'text-muted-foreground hover:text-foreground'}`}
                                >
                                    <FileJson className="h-3 w-3" /> JSON
                                </button>
                            </div>
                        </div>
                        
                        {editingItem && editingItem.encrypted ? (
                            <p className="text-xs text-muted-foreground mb-2 p-2 bg-secondary/10 rounded border border-border">
                                Value is encrypted. Entering a new value will overwrite it.
                            </p>
                        ) : null}

                        {/* Input Switcher */}
                        {isJsonMode ? (
                            <div className="h-[200px] border border-input rounded-md overflow-hidden">
                                <JSONEditor 
                                    value={formValue} 
                                    onChange={setFormValue} 
                                    height="100%" 
                                />
                            </div>
                        ) : (
                            <div className="relative">
                                <Input 
                                    type={formEncrypt ? "password" : "text"}
                                    value={formValue} 
                                    onChange={(e: any) => setFormValue(e.target.value)} 
                                    placeholder="Enter value..." 
                                />
                            </div>
                        )}
                    </div>

                    <div className="flex items-center justify-between p-3 border border-border rounded bg-secondary/5">
                        <div className="space-y-0.5">
                            <Label>Encrypt Value</Label>
                            <p className="text-[10px] text-muted-foreground">Store securely using system master key.</p>
                        </div>
                        <Switch checked={formEncrypt} onCheckedChange={setFormEncrypt} disabled={isJsonMode} />
                    </div>

                    <div className="flex justify-end gap-2 pt-2">
                        <Button variant="ghost" onClick={() => setIsModalOpen(false)}>Cancel</Button>
                        <Button onClick={handleSave} isLoading={isSaving} disabled={!formKey || (!formValue && !editingItem)}>
                            {editingItem ? 'Update' : 'Create'}
                        </Button>
                    </div>
                </div>
            </Dialog>
        </div>
    );
};