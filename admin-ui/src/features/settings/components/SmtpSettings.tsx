import React, { useState } from 'react';
import { Mail, Send, Save } from 'lucide-react';
import { Card, CardHeader, CardTitle, CardContent, Input, Label, Switch, Button } from '../../../components/ui/Elements';
import { AppSettings } from '../../../types';
import { settingsService } from '../services/settingsService';
import { useToast } from '../../../components/feedback/Toast';
import { PasswordInput } from '../../../components/form/PasswordInput';

interface SmtpSettingsProps {
  settings: AppSettings;
  onChange: (settings: Partial<AppSettings>) => void;
  onSave: (data: Partial<AppSettings>) => Promise<void>;
}

export const SmtpSettings = ({ settings, onChange, onSave }: SmtpSettingsProps) => {
  const [isTestingEmail, setIsTestingEmail] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const { toast } = useToast();

  const updateSmtp = (key: string, value: any) => {
      onChange({ smtp: { ...settings.smtp, [key]: value } });
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

  const handleSaveClick = async () => {
      setIsSaving(true);
      try {
          await onSave({ smtp: settings.smtp });
      } finally {
          setIsSaving(false);
      }
  };

  return (
    <div className="space-y-6">
        <Card>
            <CardHeader>
                <div className="flex items-center justify-between">
                    <CardTitle className="flex items-center gap-2"><Mail className="h-4 w-4" /> SMTP Configuration</CardTitle>
                    <Switch checked={settings.smtp.enabled} onCheckedChange={(c: boolean) => updateSmtp('enabled', c)} />
                </div>
            </CardHeader>
            <CardContent className={`space-y-6 transition-opacity ${settings.smtp.enabled ? 'opacity-100' : 'opacity-50 pointer-events-none'}`}>
               <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
                    <div className="space-y-2 md:col-span-2"><Label>SMTP Host</Label><Input value={settings.smtp.host} onChange={(e: any) => updateSmtp('host', e.target.value)} placeholder="smtp.example.com" /></div>
                    <div className="space-y-2"><Label>Port</Label><Input type="number" value={settings.smtp.port} onChange={(e: any) => updateSmtp('port', Number(e.target.value))} placeholder="587" /></div>
                </div>
                <div className="space-y-2"><Label>From Email Address</Label><Input value={settings.smtp.fromEmail} onChange={(e: any) => updateSmtp('fromEmail', e.target.value)} placeholder="noreply@yourdomain.com" /></div>
                <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
                     <div className="space-y-2"><Label>Username</Label><Input value={settings.smtp.username} onChange={(e: any) => updateSmtp('username', e.target.value)} /></div>
                     <div className="space-y-2"><Label>Password</Label><PasswordInput value={settings.smtp.password || ''} onChange={(e: any) => updateSmtp('password', e.target.value)} placeholder="Leave empty to keep unchanged" /></div>
                </div>
                <div className="pt-4 border-t border-border flex justify-between">
                     <Button variant="outline" size="sm" onClick={handleTestEmail} isLoading={isTestingEmail} disabled={!settings.smtp.enabled}><Send className="mr-2 h-3 w-3" /> Send Test Email</Button>
                </div>
            </CardContent>
        </Card>
        
        <div className="flex justify-end">
            <Button onClick={handleSaveClick} isLoading={isSaving} className="w-full sm:w-auto">
                <Save className="mr-2 h-4 w-4" /> Save SMTP Settings
            </Button>
        </div>
    </div>
  );
};