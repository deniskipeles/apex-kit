
import React, { useState } from 'react';
import { Mail, Shield, Send, Globe, Lock } from 'lucide-react';
import { Card, CardHeader, CardTitle, CardContent, Input, Label, Switch, Button, Textarea } from '../../../components/ui/Elements'; // Assuming Textarea is exported
import { AppSettings } from '../../../types';
import { settingsService } from '../services/settingsService';
import { useToast } from '../../../components/feedback/Toast';
import { PasswordInput } from '../../../components/form/PasswordInput';

interface SecuritySettingsProps {
  settings: AppSettings;
  onChange: (settings: Partial<AppSettings>) => void;
}

export const SecuritySettings = ({ settings, onChange }: SecuritySettingsProps) => {
  const [isTestingEmail, setIsTestingEmail] = useState(false);
  const { toast } = useToast();

  const updateSmtp = (key: string, value: any) => {
      onChange({ smtp: { ...settings.smtp, [key]: value } });
  };

  const updateSecurity = (key: string, value: any) => {
      onChange({ security: { ...settings.security, [key]: value } });
  };

  const handleTestEmail = async () => {
      setIsTestingEmail(true);
      try {
          await settingsService.testEmail('test@example.com');
          toast('Test email sent successfully', 'success');
      } catch (e) {
          toast('Failed to send test email', 'error');
      } finally {
          setIsTestingEmail(false);
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
                        <p className="text-[10px] text-muted-foreground">Comma separated list of full URLs (including protocol).</p>
                    </div>
                </div>
                {settings.security.corsAllowAll && (
                    <div className="mt-4 p-3 bg-amber-500/10 border border-amber-500/20 rounded-md text-xs text-amber-600 flex items-center gap-2">
                        <Lock className="h-3 w-3" /> Warning: Your API is currently accessible from any website.
                    </div>
                )}
            </CardContent>
        </Card>

        {/* SMTP Card (Existing) */}
        <Card>
            <CardHeader>
                <div className="flex items-center justify-between">
                    <CardTitle className="flex items-center gap-2"><Mail className="h-4 w-4" /> SMTP Configuration</CardTitle>
                    <Switch 
                        checked={settings.smtp.enabled}
                        onCheckedChange={(c: boolean) => updateSmtp('enabled', c)}
                    />
                </div>
            </CardHeader>
            <CardContent className={`space-y-6 transition-opacity ${settings.smtp.enabled ? 'opacity-100' : 'opacity-50 pointer-events-none'}`}>
               {/* ... existing SMTP inputs ... */}
               <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
                    <div className="space-y-2 md:col-span-2">
                        <Label>SMTP Host</Label>
                        <Input value={settings.smtp.host} onChange={(e: any) => updateSmtp('host', e.target.value)} placeholder="smtp.example.com" />
                    </div>
                    <div className="space-y-2">
                        <Label>Port</Label>
                        <Input type="number" value={settings.smtp.port} onChange={(e: any) => updateSmtp('port', Number(e.target.value))} placeholder="587" />
                    </div>
                </div>
                <div className="space-y-2">
                    <Label>From Email Address</Label>
                    <Input value={settings.smtp.fromEmail} onChange={(e: any) => updateSmtp('fromEmail', e.target.value)} placeholder="noreply@yourdomain.com" />
                </div>
                <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
                     <div className="space-y-2">
                        <Label>Username</Label>
                        <Input value={settings.smtp.username} onChange={(e: any) => updateSmtp('username', e.target.value)} />
                    </div>
                    <div className="space-y-2">
                        <Label>Password</Label>
                        <PasswordInput value={settings.smtp.password || ''} onChange={(e: any) => updateSmtp('password', e.target.value)} placeholder="Leave empty to keep unchanged" />
                    </div>
                </div>
                <div className="pt-4 border-t border-border flex justify-end">
                     <Button variant="secondary" size="sm" className="w-full sm:w-auto" onClick={handleTestEmail} isLoading={isTestingEmail} disabled={!settings.smtp.enabled}>
                         <Send className="mr-2 h-3 w-3" /> Send Test Email
                     </Button>
                </div>
            </CardContent>
        </Card>
    </div>
  );
};
