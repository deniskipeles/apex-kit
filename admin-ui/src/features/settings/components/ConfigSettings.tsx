import React, { useState, useEffect } from 'react';
import { Settings2, Plus, Trash2, Edit2, Lock, Eye, EyeOff, RefreshCw } from 'lucide-react';
import { Card, CardHeader, CardTitle, CardContent, Button, Input, Label, Switch, Badge } from '../../../components/ui/Elements';
import { Dialog } from '../../../components/ui/Dialog';
import { configService, ConfigItem } from '@/src/features/settings/services/configService';
import { useToast } from '../../../components/feedback/Toast';

export const ConfigSettings = () => {
    const [configs, setConfigs] = useState<ConfigItem[]>([]);
    const [isLoading, setIsLoading] = useState(true);
    const [isModalOpen, setIsModalOpen] = useState(false);
    const [editingItem, setEditingItem] = useState<ConfigItem | null>(null);
    
    // Form State
    const [formKey, setFormKey] = useState('');
    const [formValue, setFormValue] = useState('');
    const [formEncrypt, setFormEncrypt] = useState(false);
    const [isSaving, setIsSaving] = useState(false);

    const { toast } = useToast();

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

    useEffect(() => {
        loadConfigs();
    }, []);

    const handleOpenModal = (item?: ConfigItem) => {
        if (item) {
            setEditingItem(item);
            setFormKey(item.key);
            setFormValue(''); // Don't prefill encrypted values, force re-entry or leave blank to keep? 
            // API set_config is upsert. If we send empty value, it overwrites. 
            // Since we can't decrypt on client, editing usually means overwriting the value.
            setFormEncrypt(item.encrypted);
        } else {
            setEditingItem(null);
            setFormKey('');
            setFormValue('');
            setFormEncrypt(false);
        }
        setIsModalOpen(true);
    };

    const handleSave = async () => {
        if (!formKey || !formValue) return;
        setIsSaving(true);
        try {
            await configService.set(formKey, formValue, formEncrypt);
            toast(`Configuration '${formKey}' saved`, 'success');
            setIsModalOpen(false);
            loadConfigs();
        } catch (e) {
            toast('Failed to save configuration', 'error');
        } finally {
            setIsSaving(false);
        }
    };

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

    return (
        <div className="space-y-6">
            <Card>
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
                    <div className="rounded-md border border-border bg-card overflow-hidden">
                        {configs.length === 0 ? (
                            <div className="p-8 text-center text-muted-foreground text-sm">
                                No custom configurations found.
                            </div>
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
                    
                    <div className="mt-4 p-3 bg-amber-500/5 border border-amber-500/20 rounded text-xs text-amber-600/80">
                        <strong>Warning:</strong> Modifying system keys (e.g. `github_client_id`, `openai_key`) directly here may affect application stability. Use with caution.
                    </div>
                </CardContent>
            </Card>

            <Dialog isOpen={isModalOpen} onClose={() => setIsModalOpen(false)} title={editingItem ? 'Edit Configuration' : 'Add Configuration'} size="sm">
                <div className="space-y-4">
                    <div className="space-y-2">
                        <Label required>Config Key</Label>
                        <Input 
                            value={formKey} 
                            onChange={(e: any) => setFormKey(e.target.value)} 
                            placeholder="e.g. STRIPE_SECRET_KEY" 
                            disabled={!!editingItem} // Key immutable on edit
                            className="font-mono"
                        />
                    </div>
                    <div className="space-y-2">
                        <Label required>Value</Label>
                        {editingItem && editingItem.encrypted ? (
                            <p className="text-xs text-muted-foreground mb-2">Enter a new value to overwrite the existing encrypted secret.</p>
                        ) : null}
                        <div className="relative">
                            <Input 
                                type={formEncrypt ? "password" : "text"}
                                value={formValue} 
                                onChange={(e: any) => setFormValue(e.target.value)} 
                                placeholder="Enter value..." 
                            />
                        </div>
                    </div>
                    <div className="flex items-center justify-between p-3 border border-border rounded bg-secondary/5">
                        <div className="space-y-0.5">
                            <Label>Encrypt Value</Label>
                            <p className="text-[10px] text-muted-foreground">Store securely using system master key.</p>
                        </div>
                        <Switch checked={formEncrypt} onCheckedChange={setFormEncrypt} />
                    </div>

                    <div className="flex justify-end gap-2 pt-2">
                        <Button variant="ghost" onClick={() => setIsModalOpen(false)}>Cancel</Button>
                        <Button onClick={handleSave} isLoading={isSaving} disabled={!formKey || !formValue}>
                            {editingItem ? 'Update' : 'Create'}
                        </Button>
                    </div>
                </div>
            </Dialog>
        </div>
    );
};