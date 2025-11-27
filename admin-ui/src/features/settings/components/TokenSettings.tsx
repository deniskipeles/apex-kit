
import React, { useState } from 'react';
import { Key, Plus, Copy, Trash2, Check } from 'lucide-react';
import { Card, CardHeader, CardTitle, CardContent, Input, Button, Badge, Label } from '../../../components/ui/Elements';
import { Dialog } from '../../../components/ui/Dialog';
import { AppSettings } from '../../../types';
import { settingsService } from '../services/settingsService';
import { useToast } from '../../../components/feedback/Toast';

interface TokenSettingsProps {
  settings: AppSettings;
  onChange: (settings: Partial<AppSettings>) => void;
}

export const TokenSettings = ({ settings, onChange }: TokenSettingsProps) => {
  const { toast } = useToast();
  const [isCreating, setIsCreating] = useState(false);
  const [tokenName, setTokenName] = useState('');
  const [createdToken, setCreatedToken] = useState<string | null>(null);

  const handleCreate = async () => {
      if (!tokenName) return;
      const { token, rawKey } = await settingsService.generateToken(tokenName);
      
      onChange({ apiTokens: [token, ...settings.apiTokens] });
      setCreatedToken(rawKey);
      setTokenName('');
      toast('API Token created', 'success');
  };

  const handleDelete = (id: string) => {
      onChange({ apiTokens: settings.apiTokens.filter(t => t.id !== id) });
      toast('Token revoked', 'info');
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
                    <Button size="sm" onClick={() => setIsCreating(true)}><Plus className="mr-2 h-4 w-4" /> Generate New Token</Button>
                </div>
            </CardHeader>
            <CardContent>
                <div className="space-y-4">
                    {settings.apiTokens.length === 0 && (
                        <div className="text-center py-8 text-muted-foreground text-sm">
                            No active tokens found. Generate one to access the API externally.
                        </div>
                    )}
                    {settings.apiTokens.map(token => (
                        <div key={token.id} className="flex items-center justify-between p-4 rounded-lg border border-border bg-card">
                            <div className="space-y-1">
                                <div className="font-medium flex items-center gap-2">
                                    {token.name} 
                                    <Badge variant="secondary" className="text-[10px] font-mono">ID: {token.id}</Badge>
                                </div>
                                <div className="text-xs text-muted-foreground">Created: {new Date(token.created).toLocaleDateString()}</div>
                            </div>
                            <div className="flex items-center gap-4">
                                <code className="bg-secondary/50 px-2 py-1 rounded text-xs font-mono text-muted-foreground hidden sm:block">{token.key}</code>
                                <Button size="icon" variant="ghost" className="h-8 w-8 text-destructive" onClick={() => handleDelete(token.id)}><Trash2 className="h-4 w-4" /></Button>
                            </div>
                        </div>
                    ))}
                </div>
            </CardContent>
        </Card>

        {/* Creation Dialog */}
        <Dialog isOpen={isCreating} onClose={() => { setIsCreating(false); setCreatedToken(null); }} title="Generate API Token" size="sm">
            {!createdToken ? (
                <div className="space-y-4">
                    <div className="space-y-2">
                        <Label>Token Name</Label>
                        <Input placeholder="e.g. Mobile App, Zapier Integration" value={tokenName} onChange={(e: any) => setTokenName(e.target.value)} autoFocus />
                    </div>
                    <div className="flex justify-end gap-2 pt-2">
                        <Button variant="ghost" onClick={() => setIsCreating(false)}>Cancel</Button>
                        <Button onClick={handleCreate} disabled={!tokenName}>Generate</Button>
                    </div>
                </div>
            ) : (
                <div className="space-y-4">
                    <div className="rounded-md bg-emerald-500/10 border border-emerald-500/20 p-4 text-emerald-600 text-sm">
                        Token generated successfully! Copy it now, you won't be able to see it again.
                    </div>
                    <div className="flex items-center gap-2">
                        <Input value={createdToken} readOnly className="font-mono text-sm" />
                        <Button size="icon" variant="outline" onClick={() => copyToClipboard(createdToken)}><Copy className="h-4 w-4" /></Button>
                    </div>
                    <div className="flex justify-end pt-2">
                        <Button onClick={() => { setIsCreating(false); setCreatedToken(null); }}>Done</Button>
                    </div>
                </div>
            )}
        </Dialog>
    </div>
  );
};
