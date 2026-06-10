import React, { useState, useEffect } from 'react';
import { Key, Plus, Copy, Trash2, Check, Loader2, Edit2, ShieldAlert } from 'lucide-react';
import {
  Card,
  CardHeader,
  CardTitle,
  CardContent,
  Input,
  Button,
  Badge,
  Label,
  Select,
  Switch,
} from '@/src/components/ui/Elements';
import { Dialog } from '@/src/components/ui/Dialog';
import { AppSettings } from '@/src/types';
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
  const [isRootScope, setIsRootScope] = useState(false);

  // Create Modal State
  const [isCreating, setIsCreating] = useState(false);
  const [isSubmitting, setIsSubmitting] = useState(false);

  const [newTokenName, setNewTokenName] = useState('');
  const [newEnvType, setNewEnvType] = useState('sys');
  const [newTargetTenant, setNewTargetTenant] = useState('');
  const [newRoles, setNewRoles] = useState('admin');
  const [bypassCors, setBypassCors] = useState(true);

  const [createdKey, setCreatedKey] = useState<string | null>(null);

  // Edit State
  const [editingToken, setEditingToken] = useState<any | null>(null);

  // Load Keys
  useEffect(() => {
    setIsRootScope(apiClient.getScope().type === 'root');
    loadKeys();
  }, []);

  const loadKeys = async () => {
    setIsLoading(true);
    try {
      const list = await apiClient.keys.list();
      setTokens(list);
    } catch (e) {
      toast('Failed to load API keys', 'error');
    } finally {
      setIsLoading(false);
    }
  };

  const handleCreate = async () => {
    if (!newTokenName) return;
    setIsSubmitting(true);
    try {
      // Map the modern inputs to the legacy signature expected by the SDK client
      const legacyRole = newRoles; // Comma-separated roles
      const legacyScope = newTargetTenant ? `tenant:${newTargetTenant}` : 'root';

      const res = await apiClient.keys.create(
        newTokenName,
        legacyRole,
        legacyScope,
        bypassCors,
        newEnvType,
        newRoles.split(',').map((i) => i.trim()),
        newTargetTenant
      );

      setCreatedKey(res.key);
      toast('API Key generated', 'success');
      loadKeys();
    } catch (e: any) {
      toast(e.message || 'Failed to create key', 'error');
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleDelete = async (id: string) => {
    if (!confirm('Are you sure? This will instantly break any applications using this key.'))
      return;
    try {
      await apiClient.keys.delete(id);
      setTokens((prev) => prev.filter((t) => t.id !== id));
      toast('Key revoked', 'success');
    } catch (e) {
      toast('Failed to delete key', 'error');
    }
  };

  const handleUpdate = async () => {
    if (!editingToken) return;
    setIsSubmitting(true);
    try {
      const roleArr = newRoles
        .split(',')
        .map((s) => s.trim())
        .filter(Boolean);
      await apiClient.keys.update(editingToken.id, {
        name: newTokenName,
        roles: roleArr,
        bypass_cors: bypassCors,
      });

      toast('Key updated', 'success');
      loadKeys();
      handleCloseModal();
    } catch (e) {
      toast('Failed to update key', 'error');
    } finally {
      setIsSubmitting(false);
    }
  };

  const openEditModal = (token: any) => {
    setEditingToken(token);
    setNewTokenName(token.name);
    setNewRoles((token.roles || []).join(', '));
    setBypassCors(token.bypass_cors);
    setNewEnvType(token.env_type || 'sys');
    setNewTargetTenant(token.tenant_id !== 'root' ? token.tenant_id : '');
    setIsCreating(true);
  };

  const handleCloseModal = () => {
    setIsCreating(false);
    setEditingToken(null);
    setCreatedKey(null);
    setNewTokenName('');
    setNewRoles('admin');
    setBypassCors(true);
    setNewTargetTenant('');
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
            <CardTitle className="flex items-center gap-2">
              <Key className="h-4 w-4" /> API Keys
            </CardTitle>
            <Button size="sm" onClick={() => setIsCreating(true)}>
              <Plus className="mr-2 h-4 w-4" /> Generate Key
            </Button>
          </div>
        </CardHeader>
        <CardContent>
          <div className="space-y-4">
            {isLoading ? (
              <div className="flex justify-center p-8">
                <Loader2 className="animate-spin text-muted-foreground" />
              </div>
            ) : tokens.length === 0 ? (
              <div className="text-center py-8 text-muted-foreground text-sm border-2 border-dashed border-border rounded-lg">
                No active tokens found.
              </div>
            ) : (
              <div className="divide-y divide-border border border-border rounded-xl overflow-hidden bg-card shadow-sm">
                {tokens.map((token) => (
                  <div
                    key={token.id}
                    className={`flex flex-col sm:flex-row sm:items-center sm:justify-between p-4 gap-4 transition-colors ${token.status === 'revoked' ? 'opacity-50' : 'hover:bg-secondary/10'}`}
                  >
                    <div className="space-y-2 min-w-0 flex-1">
                      <div className="flex flex-wrap items-center gap-2">
                        <span
                          className="font-bold text-foreground truncate max-w-[200px]"
                          title={token.name}
                        >
                          {token.name}
                        </span>
                        <Badge
                          variant="outline"
                          className={`text-[9px] px-2 py-0.5 uppercase tracking-wider font-bold shrink-0 ${token.env_type === 'sys' ? 'border-primary/50 text-primary' : 'border-blue-500/50 text-blue-500'}`}
                        >
                          {token.env_type === 'sys'
                            ? 'ROOT'
                            : token.env_type === 'tnnt'
                              ? 'TENANT (ROOT)'
                              : token.env_type === 'sk'
                                ? 'SERVER'
                                : 'PUBLIC'}
                        </Badge>
                        {(token.roles || []).map((r: string) => (
                          <Badge
                            key={r}
                            variant="secondary"
                            className="text-[9px] px-2 py-0.5 capitalize shrink-0"
                          >
                            {r}
                          </Badge>
                        ))}
                      </div>

                      <div className="flex flex-wrap items-center gap-x-4 gap-y-1.5 text-xs text-muted-foreground font-mono">
                        <div className="flex items-center gap-1.5">
                          <span className="text-[10px] text-muted-foreground font-sans uppercase font-bold tracking-wider">
                            Scope:
                          </span>
                          <code className="bg-secondary/50 px-1.5 py-0.5 rounded text-[11px] font-semibold text-foreground">
                            {token.tenant_id === 'root' ? 'GLOBAL' : `tenant:${token.tenant_id}`}
                          </code>
                        </div>
                        <div className="flex items-center gap-1.5">
                          <span className="bg-secondary/40 px-2 py-0.5 rounded text-[11px] tracking-wider text-muted-foreground/80">
                            {token.issuer === 'root' ? 'root_' : 'tnt_'}
                            {token.tenant_id !== 'root' ? `..._` : 'sys_prod_'}••••••••_
                            {token.key_id}
                          </span>
                        </div>
                      </div>
                    </div>

                    <div className="flex sm:flex-col justify-end gap-1.5 border-t border-border/40 sm:border-0 pt-3 sm:pt-0 shrink-0">
                      <div className="flex gap-1 justify-end">
                        <Button
                          size="icon"
                          variant="ghost"
                          className="h-8 w-8 hover:bg-secondary"
                          onClick={() => openEditModal(token)}
                        >
                          <Edit2 className="h-4 w-4" />
                        </Button>
                        <Button
                          size="icon"
                          variant="ghost"
                          className="h-8 w-8 text-muted-foreground hover:text-destructive hover:bg-destructive/10"
                          onClick={() => handleDelete(token.id)}
                        >
                          <Trash2 className="h-4 w-4" />
                        </Button>
                      </div>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        </CardContent>
      </Card>

      <Dialog
        isOpen={isCreating}
        onClose={handleCloseModal}
        title={editingToken ? 'Edit Key' : 'Generate Key'}
        size="sm"
      >
        {!createdKey ? (
          <div className="space-y-4">
            <div className="space-y-2">
              <Label required>Key Name</Label>
              <Input
                placeholder="e.g. CI/CD Pipeline, iOS App"
                value={newTokenName}
                onChange={(e: any) => setNewTokenName(e.target.value)}
                autoFocus
              />
            </div>

            {!editingToken && isRootScope && (
              <div className="space-y-2">
                <Label>Environment Tier</Label>
                <Select value={newEnvType} onChange={(e: any) => setNewEnvType(e.target.value)}>
                  <option value="sys">Root System Key (Full Access)</option>
                  <option value="tnnt">Tenant Admin Key (Cross-Boundary)</option>
                </Select>
              </div>
            )}

            {!editingToken && !isRootScope && (
              <div className="space-y-2">
                <Label>Key Type</Label>
                <Select value={newEnvType} onChange={(e: any) => setNewEnvType(e.target.value)}>
                  <option value="sk">Server-Side Secret Key</option>
                  <option value="pk">Client-Side Public Key</option>
                </Select>
              </div>
            )}

            {!editingToken && isRootScope && newEnvType === 'tnnt' && (
              <Input
                placeholder="Target Tenant ID"
                value={newTargetTenant}
                onChange={(e: any) => setNewTargetTenant(e.target.value)}
              />
            )}

            <div className="space-y-2">
              <Label>Roles (Comma separated)</Label>
              <Input
                placeholder="admin, custom_role"
                value={newRoles}
                onChange={(e: any) => setNewRoles(e.target.value)}
              />
            </div>

            <div className="flex items-center justify-between p-3 border border-border rounded bg-secondary/5">
              <div className="space-y-0.5">
                <Label>Bypass CORS</Label>
                <p className="text-[10px] text-muted-foreground">
                  Allow API access from any origin (e.g. mobile apps, 3rd party servers).
                </p>
              </div>
              <Switch checked={bypassCors} onCheckedChange={setBypassCors} />
            </div>

            <div className="flex justify-end gap-2 pt-2 border-t border-border mt-4">
              <Button variant="ghost" onClick={handleCloseModal}>
                Cancel
              </Button>
              <Button
                onClick={editingToken ? handleUpdate : handleCreate}
                disabled={!newTokenName}
                isLoading={isSubmitting}
              >
                {editingToken ? 'Save' : 'Generate Key'}
              </Button>
            </div>
          </div>
        ) : (
          <div className="space-y-4 animate-in zoom-in-95">
            <div className="rounded-md bg-emerald-500/10 border border-emerald-500/20 p-4 flex gap-3">
              <Check className="h-5 w-5 text-emerald-500 shrink-0" />
              <div className="space-y-1">
                <h4 className="text-sm font-bold text-emerald-600">Secure Key Issued!</h4>
                <p className="text-xs text-emerald-600/80">
                  Copy this key now. We only store the cryptographic hash of the secret component,
                  meaning you will <strong className="underline">not</strong> be able to see it
                  again.
                </p>
              </div>
            </div>

            <div className="space-y-1">
              <div className="flex items-center gap-2">
                <Input
                  value={createdKey}
                  readOnly
                  className="font-mono text-sm bg-secondary/50 text-foreground"
                />
                <Button size="icon" variant="outline" onClick={() => copyToClipboard(createdKey)}>
                  <Copy className="h-4 w-4" />
                </Button>
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
