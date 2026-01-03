import React, { useState } from 'react';
import { BrainCircuit, ExternalLink, Save } from 'lucide-react';
import { Card, CardHeader, CardTitle, CardContent, Input, Label, Switch, Button } from '../../../components/ui/Elements';
import { AppSettings } from '../../../types';
import { PasswordInput } from '../../../components/form/PasswordInput';

interface AiSettingsProps {
  settings: AppSettings;
  onChange: (settings: Partial<AppSettings>) => void;
  onSave: (data: Partial<AppSettings>) => Promise<void>;
}

export const AiSettings = ({ settings, onChange, onSave }: AiSettingsProps) => {
  const [isSaving, setIsSaving] = useState(false);

  const updateAi = (key: string, value: any) => {
    onChange({ ai: { ...settings.ai, [key]: value } });
  };

  const handleSaveClick = async () => {
      setIsSaving(true);
      try {
          await onSave({ ai: settings.ai });
      } finally {
          setIsSaving(false);
      }
  };

  return (
    <div className="space-y-6">
        <Card>
            <CardHeader>
                <div className="flex items-center justify-between">
                    <CardTitle className="flex items-center gap-2"><BrainCircuit className="h-4 w-4" /> AI Configuration</CardTitle>
                    <Switch checked={settings.ai?.enabled || false} onCheckedChange={(c: boolean) => updateAi('enabled', c)} />
                </div>
            </CardHeader>
            <CardContent className={`space-y-6 transition-opacity ${settings.ai?.enabled ? 'opacity-100' : 'opacity-50 pointer-events-none'}`}>
                <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
                    <div className="space-y-2"><Label>Provider</Label><Input value="Google Gemini" disabled className="bg-secondary/20" /></div>
                    <div className="space-y-2"><Label>API Key</Label><PasswordInput value={settings.ai?.apiKey || ''} onChange={(e: any) => updateAi('apiKey', e.target.value)} placeholder="AIzaSy..." /></div>
                </div>
                <div className="rounded-md bg-secondary/10 p-4 text-sm text-muted-foreground flex flex-col gap-2">
                    <p>ApexKit currently supports Google Gemini models. You need a valid API key to use AI Actions.</p>
                    <a href="https://aistudio.google.com/app/apikey" target="_blank" rel="noreferrer" className="text-primary hover:underline inline-flex items-center gap-1 w-fit">Get API Key <ExternalLink className="h-3 w-3" /></a>
                </div>
            </CardContent>
        </Card>

        <div className="flex justify-end">
            <Button onClick={handleSaveClick} isLoading={isSaving} className="w-full sm:w-auto">
                <Save className="mr-2 h-4 w-4" /> Save AI Settings
            </Button>
        </div>
    </div>
  );
};