import React, { useState } from 'react';
import { Mail, Send, Save, FileText, Code, Info } from 'lucide-react';
import {
  Card,
  CardHeader,
  CardTitle,
  CardContent,
  Input,
  Label,
  Switch,
  Button,
  Textarea,
} from '../../../components/ui/Elements';
import { Dialog } from '../../../components/ui/Dialog';
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
  const [testTemplate, setTestTemplate] = useState('generic');
  const [isDocsOpen, setIsDocsOpen] = useState(false);
  const { toast } = useToast();

  const updateSmtp = (key: string, value: any) => {
    onChange({ smtp: { ...settings.smtp, [key]: value } });
  };

  const handleTestEmail = async () => {
    if (!testEmail) return;
    setIsTestingEmail(true);
    try {
      await apiClient.system.testEmail(
        testEmail,
        testTemplate === 'generic' ? undefined : testTemplate
      );
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
            <CardTitle className="flex items-center gap-2">
              <Mail className="h-4 w-4" /> Mailer Configuration
            </CardTitle>
            <Switch
              checked={settings.smtp.enabled}
              onCheckedChange={(c: boolean) => updateSmtp('enabled', c)}
            />
          </div>
        </CardHeader>
        <CardContent
          className={`space-y-6 transition-opacity ${settings.smtp.enabled ? 'opacity-100' : 'opacity-50 pointer-events-none'}`}
        >
          <div className="rounded-md bg-secondary/10 p-4 border border-border">
            <div className="flex items-center gap-2 mb-4 text-sm font-semibold">
              <Code className="h-4 w-4" /> Transport Settings
            </div>
            <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
              <div className="space-y-2 md:col-span-2">
                <Label>SMTP Host</Label>
                <Input
                  value={settings.smtp.host}
                  onChange={(e: any) => updateSmtp('host', e.target.value)}
                  placeholder="smtp.example.com"
                />
              </div>
              <div className="space-y-2">
                <Label>Port</Label>
                <Input
                  type="number"
                  value={settings.smtp.port}
                  onChange={(e: any) => updateSmtp('port', Number(e.target.value))}
                  placeholder="587"
                />
              </div>
            </div>
            <div className="grid grid-cols-1 gap-4 md:grid-cols-2 mt-4">
              <div className="space-y-2">
                <Label>Username</Label>
                <Input
                  value={settings.smtp.username}
                  onChange={(e: any) => updateSmtp('username', e.target.value)}
                />
              </div>
              <div className="space-y-2">
                <Label>Password</Label>
                <PasswordInput
                  value={settings.smtp.password || ''}
                  onChange={(e: any) => updateSmtp('password', e.target.value)}
                  placeholder="Leave empty to keep unchanged"
                />
              </div>
            </div>
            <div className="space-y-2 mt-4">
              <Label>From Address</Label>
              <Input
                value={settings.smtp.fromEmail}
                onChange={(e: any) => updateSmtp('fromEmail', e.target.value)}
                placeholder="noreply@yourdomain.com"
              />
            </div>
          </div>

          <div className="rounded-md bg-secondary/10 p-4 border border-border">
            <div className="flex items-center justify-between mb-4">
              <div className="flex items-center gap-2 text-sm font-semibold">
                <FileText className="h-4 w-4" /> Email Templates
              </div>
              <Button
                variant="ghost"
                size="sm"
                className="h-7 text-xs"
                onClick={() => setIsDocsOpen(true)}
              >
                <Info className="h-3.5 w-3.5 mr-1" /> Template Docs
              </Button>
            </div>

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

          <div className="pt-4 border-t border-border flex flex-col md:flex-row items-end gap-3">
            <div className="flex-1 space-y-1 w-full">
              <Label>Test Recipient</Label>
              <Input
                value={testEmail}
                onChange={(e: any) => setTestEmail(e.target.value)}
                placeholder="your@email.com"
              />
            </div>
            <div className="flex-1 space-y-1 w-full">
              <Label>Template to Test</Label>
              <select
                value={testTemplate}
                onChange={(e: any) => setTestTemplate(e.target.value)}
                className="flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50 bg-card text-foreground"
              >
                <option value="generic">Generic Test</option>
                <option value="welcome">Welcome Email</option>
                <option value="reset">Password Reset</option>
                <option value="verify">Verification Email</option>
              </select>
            </div>
            <Button
              variant="outline"
              onClick={handleTestEmail}
              isLoading={isTestingEmail}
              disabled={!settings.smtp.enabled || !testEmail}
              className="w-full md:w-auto"
            >
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

      <Dialog
        isOpen={isDocsOpen}
        onClose={() => setIsDocsOpen(false)}
        title="Template Variables"
        size="md"
      >
        <div className="space-y-4 text-sm text-muted-foreground pb-4">
          <p>
            You can use the following variables in your email templates. They will be automatically
            replaced with the correct values when the email is sent.
          </p>
          <ul className="space-y-4 pt-2">
            <li className="flex items-start gap-3">
              <code className="text-primary bg-primary/10 px-1.5 py-0.5 rounded font-bold shrink-0">
                {'{{app_name}}'}
              </code>
              <span>The name of your application (set in General settings).</span>
            </li>
            <li className="flex items-start gap-3">
              <code className="text-primary bg-primary/10 px-1.5 py-0.5 rounded font-bold shrink-0">
                {'{{email}}'}
              </code>
              <span>The email address of the recipient.</span>
            </li>
            <li className="flex items-start gap-3">
              <code className="text-primary bg-primary/10 px-1.5 py-0.5 rounded font-bold shrink-0">
                {'{{link}}'}
              </code>
              <span>
                The secure link for password resets or email verification (only available in those
                specific templates).
              </span>
            </li>
            <li className="flex items-start gap-3">
              <code className="text-primary bg-primary/10 px-1.5 py-0.5 rounded font-bold shrink-0">
                {'{{token}}'}
              </code>
              <span>
                The raw, secure single-use token (a UUID) if you prefer to build your own link.
              </span>
            </li>
          </ul>
        </div>
      </Dialog>
    </div>
  );
};
