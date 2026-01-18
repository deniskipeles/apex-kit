import React, { useState } from 'react';
import { Mail, Send, Save, FileText, Code } from 'lucide-react';
import { Card, CardHeader, CardTitle, CardContent, Input, Label, Switch, Button, Textarea } from '../../../components/ui/Elements';
import { AppSettings } from '../../../types';
import { useToast } from '../../../components/feedback/Toast';
import { PasswordInput } from '../../../components/form/PasswordInput';
import { apiClient } from '@/src/lib/apiClient';

interface SmtpSettingsProps {
  settings: AppSettings;
  onChange: (settings: Partial<AppSettings>) => void;
  onSave: (data: Partial<AppSettings>) => Promise<void>;
}

export const SmtpSettings = ({ settings, onChange, onSave }: SmtpSettingsProps) => {
  const [isTestingEmail, setIsTestingEmail] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [testEmail, setTestEmail] = useState('');
  const { toast } = useToast();

  const updateSmtp = (key: string, value: any) => {
      onChange({ smtp: { ...settings.smtp, [key]: value } });
  };

  const handleTestEmail = async () => {
      if (!testEmail) return;
      setIsTestingEmail(true);
      try {
          await apiClient.system.testEmail(testEmail);
          toast('Test email sent successfully', 'success');
      } catch (e: any) {
          toast(`Failed to send: ${e.message}`, 'error');
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
                    <CardTitle className="flex items-center gap-2"><Mail className="h-4 w-4" /> Mailer Configuration</CardTitle>
                    <Switch checked={settings.smtp.enabled} onCheckedChange={(c: boolean) => updateSmtp('enabled', c)} />
                </div>
            </CardHeader>
            <CardContent className={`space-y-6 transition-opacity ${settings.smtp.enabled ? 'opacity-100' : 'opacity-50 pointer-events-none'}`}>
               
               <div className="rounded-md bg-secondary/10 p-4 border border-border">
                   <div className="flex items-center gap-2 mb-4 text-sm font-semibold"><Code className="h-4 w-4" /> Transport Settings</div>
                   <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
                        <div className="space-y-2 md:col-span-2"><Label>SMTP Host</Label><Input value={settings.smtp.host} onChange={(e: any) => updateSmtp('host', e.target.value)} placeholder="smtp.example.com" /></div>
                        <div className="space-y-2"><Label>Port</Label><Input type="number" value={settings.smtp.port} onChange={(e: any) => updateSmtp('port', Number(e.target.value))} placeholder="587" /></div>
                    </div>
                    <div className="grid grid-cols-1 gap-4 md:grid-cols-2 mt-4">
                         <div className="space-y-2"><Label>Username</Label><Input value={settings.smtp.username} onChange={(e: any) => updateSmtp('username', e.target.value)} /></div>
                         <div className="space-y-2"><Label>Password</Label><PasswordInput value={settings.smtp.password || ''} onChange={(e: any) => updateSmtp('password', e.target.value)} placeholder="Leave empty to keep unchanged" /></div>
                    </div>
                    <div className="space-y-2 mt-4"><Label>From Address</Label><Input value={settings.smtp.fromEmail} onChange={(e: any) => updateSmtp('fromEmail', e.target.value)} placeholder="noreply@yourdomain.com" /></div>
               </div>

               <div className="rounded-md bg-secondary/10 p-4 border border-border">
                   <div className="flex items-center gap-2 mb-4 text-sm font-semibold"><FileText className="h-4 w-4" /> Email Templates</div>
                   
                   <div className="space-y-4">
                       <div className="space-y-2">
                           <Label>Welcome Email</Label>
                           <Textarea 
                                className="font-mono text-xs min-h-[80px]"
                                value={settings.smtp.template_welcome || ''}
                                onChange={(e: any) => updateSmtp('template_welcome', e.target.value)}
                                placeholder="Welcome to {{app_name}}!"
                           />
                       </div>
                       <div className="space-y-2">
                           <Label>Password Reset</Label>
                           <Textarea 
                                className="font-mono text-xs min-h-[80px]"
                                value={settings.smtp.template_reset || ''}
                                onChange={(e: any) => updateSmtp('template_reset', e.target.value)}
                                placeholder="Click here to reset: {{link}}"
                           />
                       </div>
                       <div className="space-y-2">
                           <Label>Verification</Label>
                           <Textarea 
                                className="font-mono text-xs min-h-[80px]"
                                value={settings.smtp.template_verify || ''}
                                onChange={(e: any) => updateSmtp('template_verify', e.target.value)}
                                placeholder="Verify your email: {{link}}"
                           />
                       </div>
                   </div>
               </div>

               <div className="pt-4 border-t border-border flex items-end gap-2">
                     <div className="flex-1 space-y-1">
                        <Label>Test Recipient</Label>
                        <Input value={testEmail} onChange={(e: any) => setTestEmail(e.target.value)} placeholder="your@email.com" />
                     </div>
                     <Button variant="outline" onClick={handleTestEmail} isLoading={isTestingEmail} disabled={!settings.smtp.enabled || !testEmail}>
                        <Send className="mr-2 h-3 w-3" /> Send Test
                     </Button>
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