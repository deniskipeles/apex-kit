import React, { useState } from 'react';
import { BrainCircuit, ExternalLink, Save } from 'lucide-react';
import {
  Card,
  CardHeader,
  CardTitle,
  CardContent,
  Label,
  Switch,
  Button,
  Select,
} from '../../../components/ui/Elements';
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
            <CardTitle className="flex items-center gap-2">
              <BrainCircuit className="h-4 w-4" /> AI Configuration
            </CardTitle>
            <Switch
              checked={settings.ai?.enabled || false}
              onCheckedChange={(c: boolean) => updateAi('enabled', c)}
            />
          </div>
        </CardHeader>
        <CardContent
          className={`space-y-6 transition-opacity ${settings.ai?.enabled ? 'opacity-100' : 'opacity-50 pointer-events-none'}`}
        >
          <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
            <div className="space-y-2">
              <Label>Default Inference Provider</Label>
              <Select
                value={settings.ai?.provider || 'gemini'}
                onChange={(e: any) => updateAi('provider', e.target.value)}
              >
                <option value="gemini">Google Gemini</option>
                <option value="groq">Groq (OpenAI Compatible)</option>
                <option value="openai">OpenAI API</option>
              </Select>
            </div>
            <div className="space-y-2">
              <Label>Master API Key</Label>
              <PasswordInput
                value={settings.ai?.apiKey || ''}
                onChange={(e: any) => updateAi('apiKey', e.target.value)}
                placeholder="Enter API Key..."
              />
            </div>
          </div>

          <div className="rounded-md bg-secondary/10 p-4 text-sm text-muted-foreground flex flex-col gap-3 border border-border border-dashed">
            <p>
              ApexKit supports <strong>Google Gemini</strong>, <strong>Groq</strong>, and{' '}
              <strong>OpenAI</strong> models. Enter your master API key above. Individual AI Actions
              can override the provider, but they will share this global API Key (ensure the key
              belongs to the provider you are using).
            </p>
            <div className="flex flex-wrap gap-4 pt-2">
              <a
                href="https://aistudio.google.com/app/apikey"
                target="_blank"
                rel="noreferrer"
                className="text-primary hover:underline inline-flex items-center gap-1 font-medium"
              >
                Get Gemini Key <ExternalLink className="h-3 w-3" />
              </a>
              <a
                href="https://console.groq.com/keys"
                target="_blank"
                rel="noreferrer"
                className="text-primary hover:underline inline-flex items-center gap-1 font-medium"
              >
                Get Groq Key <ExternalLink className="h-3 w-3" />
              </a>
              <a
                href="https://platform.openai.com/api-keys"
                target="_blank"
                rel="noreferrer"
                className="text-primary hover:underline inline-flex items-center gap-1 font-medium"
              >
                Get OpenAI Key <ExternalLink className="h-3 w-3" />
              </a>
            </div>
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
