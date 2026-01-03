import React, { useState } from 'react';
import { Shield, Globe, Lock, Save } from 'lucide-react';
import { Card, CardHeader, CardTitle, CardContent, Label, Switch, Button, Textarea } from '../../../components/ui/Elements';
import { AppSettings } from '../../../types';

interface SecuritySettingsProps {
  settings: AppSettings;
  onChange: (settings: Partial<AppSettings>) => void;
  onSave: (data: Partial<AppSettings>) => Promise<void>;
}

export const SecuritySettings = ({ settings, onChange, onSave }: SecuritySettingsProps) => {
  const [isSaving, setIsSaving] = useState(false);

  const updateSecurity = (key: string, value: any) => {
      onChange({ security: { ...settings.security, [key]: value } });
  };

  const handleSaveClick = async () => {
      setIsSaving(true);
      try {
          await onSave({
              allowPublicRegistration: settings.allowPublicRegistration,
              security: settings.security
          });
      } finally {
          setIsSaving(false);
      }
  };

  return (
    <div className="space-y-6">
        {/* Access Control Card */}
        <Card>
            <CardHeader>
                <CardTitle className="flex items-center gap-2"><Shield className="h-4 w-4" /> Access Control</CardTitle>
            </CardHeader>
            <CardContent>
                <div className="flex flex-col sm:flex-row sm:items-center justify-between rounded-lg border border-border p-4 gap-4">
                    <div className="space-y-0.5">
                        <Label className="text-base">Allow Public Registration</Label>
                        <p className="text-sm text-muted-foreground">If enabled, anyone can create an account.</p>
                    </div>
                    <Switch 
                        checked={settings.allowPublicRegistration}
                        onCheckedChange={(c: boolean) => onChange({ allowPublicRegistration: c })}
                    />
                </div>
            </CardContent>
        </Card>

        {/* CORS Settings Card */}
        <Card>
            <CardHeader>
                <div className="flex items-center justify-between">
                    <div className="space-y-1">
                        <CardTitle className="flex items-center gap-2"><Globe className="h-4 w-4" /> CORS Configuration</CardTitle>
                        <p className="text-xs text-muted-foreground">Control which websites can access your API.</p>
                    </div>
                    <div className="flex items-center gap-2">
                        <span className="text-xs font-medium text-muted-foreground">{settings.security.corsAllowAll ? 'Public API' : 'Restricted'}</span>
                        <Switch 
                            checked={settings.security.corsAllowAll}
                            onCheckedChange={(c: boolean) => updateSecurity('corsAllowAll', c)}
                        />
                    </div>
                </div>
            </CardHeader>
            <CardContent>
                <div className={`space-y-4 transition-all duration-300 ${settings.security.corsAllowAll ? 'opacity-50 pointer-events-none grayscale' : 'opacity-100'}`}>
                    <div className="space-y-2">
                        <Label>Allowed Origins</Label>
                        <Textarea 
                            value={settings.security.corsOrigins}
                            onChange={(e: any) => updateSecurity('corsOrigins', e.target.value)}
                            placeholder="https://myapp.com, http://localhost:3000"
                            className="font-mono text-xs min-h-[80px]"
                        />
                        <p className="text-[10px] text-muted-foreground">Comma separated list of full URLs.</p>
                    </div>
                </div>
                {settings.security.corsAllowAll && (
                    <div className="mt-4 p-3 bg-amber-500/10 border border-amber-500/20 rounded-md text-xs text-amber-600 flex items-center gap-2">
                        <Lock className="h-3 w-3" /> Warning: Your API is currently accessible from any website.
                    </div>
                )}
            </CardContent>
        </Card>

        {/* Global Save for this Tab */}
        <div className="flex justify-end">
            <Button onClick={handleSaveClick} isLoading={isSaving} className="w-full sm:w-auto">
                <Save className="mr-2 h-4 w-4" /> Save Security Settings
            </Button>
        </div>
    </div>
  );
};