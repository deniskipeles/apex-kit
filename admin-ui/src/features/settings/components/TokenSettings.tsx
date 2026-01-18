import React, { useState, useEffect } from 'react';
import { Key, Plus, Copy, Trash2, Check, Loader2, ShieldAlert } from 'lucide-react';
import { Card, CardHeader, CardTitle, CardContent, Input, Button, Badge, Label, Select, Switch } from '@/src/components/ui/Elements';
import { Dialog } from '@/src/components/ui/Dialog';
import { AppSettings, ApiKey } from '@/src/types'; // You might need to update types to include ApiToken if missing
import { apiClient } from '@/src/lib/apiClient';
import { useToast } from '@/src/components/feedback/Toast';

interface TokenSettingsProps {
    settings: AppSettings;
    onChange: (settings: Partial<AppSettings>) => void;
}

export const TokenSettings = ({ settings, onChange }: TokenSettingsProps) => {
    const { toast } = useToast();
    const [tokens, setTokens] = useState<any[]>([]);
    const [isLoading, setIsLoading] = useState(true);

    // Create Modal State
    const [isCreating, setIsCreating] = useState(false);
    const [isSubmitting, setIsSubmitting] = useState(false);
    const [newTokenName, setNewTokenName] = useState('');
    const [newTokenRole, setNewTokenRole] = useState('admin');
    const [newScope, setNewScope] = useState('');
    const [newTargetScope, setNewTargetScope] = useState('');
    const [bypassCors, setBypassCors] = useState(true);
    const [createdKey, setCreatedKey] = useState<string | null>(null);

    // Load Keys
    useEffect(() => {
        loadKeys();
    }, []);

    const loadKeys = async () => {
        setIsLoading(true);
        try {
            const list = await apiClient.keys.list();
            setTokens(list);
        } catch (e) {
            toast("Failed to load API keys", "error");
        } finally {
            setIsLoading(false);
        }
    };

    const handleCreate = async () => {
        if (!newTokenName) return;
        setIsSubmitting(true);
        try {
            const scope = newScope === 'tenant:' ? newScope + newTargetScope : newScope;
            const res = await apiClient.keys.create(newTokenName, newTokenRole, scope, bypassCors);
            setCreatedKey(res.key); // Show the raw key ONE TIME
            toast("API Key created", "success");
            loadKeys(); // Refresh list
        } catch (e) {
            toast("Failed to create key", "error");
        } finally {
            setIsSubmitting(false);
        }
    };

    const handleDelete = async (id: string) => {
        if (!confirm("Are you sure? This will immediately revoke access for any application using this key.")) return;
        try {
            await apiClient.keys.delete(id);
            setTokens(prev => prev.filter(t => t.id !== id));
            toast("Key revoked", "success");
        } catch (e) {
            toast("Failed to delete key", "error");
        }
    };

    const handleCloseModal = () => {
        setIsCreating(false);
        setCreatedKey(null);
        setNewTokenName('');
        setNewTokenRole('admin');
    };

    const copyToClipboard = (text: string) => {
        navigator.clipboard.writeText(text);
        toast('Copied to clipboard', 'success');
    };

    return (
        <div className="space-y-6">
            <Card>
                <CardHeader>
                    <div className="flex items-center justify-between">
                        <CardTitle className="flex items-center gap-2"><Key className="h-4 w-4" /> Super Tokens (API Keys)</CardTitle>
                        <Button size="sm" onClick={() => setIsCreating(true)}><Plus className="mr-2 h-4 w-4" /> Generate Token</Button>
                    </div>
                </CardHeader>
                <CardContent>
                    <div className="space-y-4">
                        {isLoading ? (
                            <div className="flex justify-center p-8"><Loader2 className="animate-spin text-muted-foreground" /></div>
                        ) : tokens.length === 0 ? (
                            <div className="text-center py-8 text-muted-foreground text-sm border-2 border-dashed border-border rounded-lg">
                                No active tokens found. Generate one to access the API programmatically.
                            </div>
                        ) : (
                            <div className="divide-y divide-border border border-border rounded-md overflow-hidden bg-card">
                                {tokens.map(token => (
                                    <div key={token.id} className="flex items-center justify-between p-4 hover:bg-secondary/20 transition-colors">
                                        <div className="space-y-1">
                                            <div className="font-medium flex items-center gap-2">
                                                {token.name}
                                                <Badge variant={token.role === 'admin' ? 'primary' : 'secondary'} className="text-[10px] capitalize">{token.role}</Badge>
                                            </div>
                                            <div className="font-medium flex items-center gap-2">
                                                SCOPE : {token.scope ? token.scope : 'root'}
                                                <Badge variant={token.bypass_cors === true ? 'primary' : 'secondary'} className="text-[10px] capitalize">{token.bypass_cors ? 'Bypass CORS' : 'Blocked by CORS'}</Badge>
                                            </div>
                                            <div className="text-xs text-muted-foreground font-mono flex items-center gap-2">
                                                <span className="bg-secondary/50 px-1.5 py-0.5 rounded">{token.prefix}****************</span>
                                                <span>• Created {new Date(token.created).toLocaleDateString()}</span>
                                            </div>
                                        </div>
                                        <Button size="icon" variant="ghost" className="h-8 w-8 text-muted-foreground hover:text-destructive hover:bg-destructive/10" onClick={() => handleDelete(token.id)}>
                                            <Trash2 className="h-4 w-4" />
                                        </Button>
                                    </div>
                                ))}
                            </div>
                        )}
                    </div>
                </CardContent>
            </Card>

            {/* Creation Dialog */}
            <Dialog isOpen={isCreating} onClose={handleCloseModal} title="Generate API Token" size="sm">
                {!createdKey ? (
                    <div className="space-y-4">
                        <div className="space-y-2">
                            <Label required>Token Name</Label>
                            <Input
                                placeholder="e.g. Mobile App, CI/CD Pipeline"
                                value={newTokenName}
                                onChange={(e: any) => setNewTokenName(e.target.value)}
                                autoFocus
                            />
                        </div>
                        <div className="space-y-2">
                            <Label>Role</Label>
                            <Select value={newTokenRole} onChange={(e: any) => setNewTokenRole(e.target.value)}>
                                <option value="admin">Admin (Full Access)</option>
                                <option value="user">User (Standard Access)</option>
                            </Select>
                            <p className="text-[10px] text-muted-foreground">
                                Admin tokens bypass all collection policies. User tokens are subject to "auth" rules.
                            </p>
                        </div>
                        <div className="space-y-2">
                            <Label>Scope</Label>
                            <Select value={newScope} onChange={(e: any) => setNewScope(e.target.value)}>
                                <option value="root">Root (This Environment)</option>
                                <option value="*">Global (All Tenants)</option>
                                <option value="tenant:">Specific Tenant (Enter ID)</option>
                            </Select>
                        </div>

                        {newScope === 'tenant:' && (
                            <Input placeholder="Tenant ID" onChange={(e) => setNewTargetScope(e.target.value)} />
                        )}

                        <div className="flex items-center justify-between p-3 border border-border rounded bg-secondary/5">
                            <div className="space-y-0.5">
                                <Label>Bypass CORS</Label>
                                <p className="text-[10px] text-muted-foreground">Allow access from any origin (e.g. mobile apps, 3rd party servers).</p>
                            </div>
                            <Switch checked={bypassCors} onCheckedChange={setBypassCors} />
                        </div>
                        <div className="flex justify-end gap-2 pt-2 border-t border-border mt-4">
                            <Button variant="ghost" onClick={handleCloseModal}>Cancel</Button>
                            <Button onClick={handleCreate} disabled={!newTokenName} isLoading={isSubmitting}>Generate</Button>
                        </div>
                    </div>
                ) : (
                    <div className="space-y-4 animate-in zoom-in-95">
                        <div className="rounded-md bg-emerald-500/10 border border-emerald-500/20 p-4 flex gap-3">
                            <Check className="h-5 w-5 text-emerald-500 shrink-0" />
                            <div className="space-y-1">
                                <h4 className="text-sm font-bold text-emerald-600">Token Generated!</h4>
                                <p className="text-xs text-emerald-600/80">Copy this key now. You will <strong className="underline">not</strong> be able to see it again.</p>
                            </div>
                        </div>

                        <div className="space-y-1">
                            <div className="flex items-center gap-2">
                                <Input value={createdKey} readOnly className="font-mono text-sm bg-secondary/50 text-foreground" />
                                <Button size="icon" variant="outline" onClick={() => copyToClipboard(createdKey)}><Copy className="h-4 w-4" /></Button>
                            </div>
                        </div>

                        <div className="flex justify-end pt-2 border-t border-border mt-4">
                            <Button onClick={handleCloseModal}>Done</Button>
                        </div>
                    </div>
                )}
            </Dialog>
        </div>
    );
};