import React, { useState, useEffect } from 'react';
import { Save, Sparkles, Info, BrainCircuit, Globe, RefreshCw, Layers } from 'lucide-react';
import { Button, Input, Label, Select, Switch } from '../../../components/ui/Elements';
import { Dialog } from '../../../components/ui/Dialog';
import { AiAction } from '../../../types';

interface AiActionEditorProps {
  isOpen: boolean;
  onClose: () => void;
  onSave: (data: Partial<AiAction>) => Promise<void>;
  initialData?: AiAction;
}

const PROVIDER_GROUPS = [
  { value: 'gemini', label: 'Google Gemini' },
  { value: 'groq', label: 'Groq (OpenAI Compatible)' },
  { value: 'openai', label: 'OpenAI API' },
];

const MODEL_GROUPS = {
  gemini: [
    { id: 'gemini-2.5-flash', name: 'Gemini 2.5 Flash' },
    { id: 'gemini-2.5-flash-lite', name: 'Gemini 2.5 Flash Lite' },
    { id: 'gemini-2.0-flash', name: 'Gemini 2.0 Flash' },
    { id: 'gemini-2.5-pro', name: 'Gemini 2.5 Pro' },
  ],
  groq: [
    { id: 'llama-3.3-70b-versatile', name: 'Llama 3.3 (70B)' },
    { id: 'llama3-8b-8192', name: 'Llama 3 (8B)' },
    { id: 'mixtral-8x7b-32768', name: 'Mixtral 8x7b' },
  ],
  openai: [
    { id: 'gpt-4o', name: 'GPT-4o' },
    { id: 'gpt-4o-mini', name: 'GPT-4o Mini' },
    { id: 'o1-mini', name: 'o1 Mini' },
  ],
};

export const AiActionEditor = ({ isOpen, onClose, onSave, initialData }: AiActionEditorProps) => {
  const [formData, setFormData] = useState<Partial<AiAction>>({
    name: '',
    slug: '',
    model: 'gemini-2.5-flash',
    system_prompt: '',
    template: 'User Request: {{input}}',
    config: {
      provider: 'gemini',
      grounding: false,
      streaming: false,
      url_context: false,
    },
  });
  const [isSaving, setIsSaving] = useState(false);

  useEffect(() => {
    if (initialData) {
      setFormData({
        ...initialData,
        config: {
          provider: 'gemini',
          grounding: false,
          streaming: false,
          url_context: false,
          ...(initialData.config || {}),
        },
      });
    } else {
      setFormData({
        name: '',
        slug: '',
        model: 'gemini-2.5-flash',
        system_prompt: '',
        template: 'User Request: {{input}}',
        config: {
          provider: 'gemini',
          grounding: false,
          streaming: false,
          url_context: false,
        },
      });
    }
  }, [initialData, isOpen]);

  const updateConfig = (key: string, value: any) => {
    setFormData((prev) => ({
      ...prev,
      config: {
        ...(prev.config || {}),
        [key]: value,
      },
    }));
  };

  const handleProviderChange = (prov: string) => {
    const defaultModel = MODEL_GROUPS[prov as keyof typeof MODEL_GROUPS]?.[0]?.id || '';
    setFormData((prev) => ({
      ...prev,
      model: defaultModel,
      config: {
        ...(prev.config || {}),
        provider: prov,
        grounding: prov === 'gemini' ? prev.config?.grounding || false : false, // Grounding is exclusive to Gemini
      },
    }));
  };

  const handleSave = async () => {
    setIsSaving(true);
    try {
      await onSave(formData);
      onClose();
    } finally {
      setIsSaving(false);
    }
  };

  const handleNameChange = (val: string) => {
    if (!initialData) {
      const slug = val
        .toLowerCase()
        .replace(/[^a-z0-9]+/g, '-')
        .replace(/(^-|-$)+/g, '');
      setFormData((prev) => ({ ...prev, name: val, slug }));
    } else {
      setFormData((prev) => ({ ...prev, name: val }));
    }
  };

  if (!isOpen) return null;

  const currentProvider = formData.config?.provider || 'gemini';
  const availableModels = MODEL_GROUPS[currentProvider as keyof typeof MODEL_GROUPS] || [];

  return (
    <Dialog
      isOpen={isOpen}
      onClose={onClose}
      title={initialData ? 'Edit AI Action' : 'New AI Action'}
      size="lg"
    >
      <div className="flex flex-col h-[75vh] gap-5">
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          <div className="space-y-2">
            <Label required>Action Name</Label>
            <Input
              value={formData.name}
              onChange={(e: any) => handleNameChange(e.target.value)}
              placeholder="e.g. Code Reviewer"
              autoFocus
            />
          </div>
          <div className="space-y-2">
            <Label required>Slug (API Endpoint)</Label>
            <Input
              value={formData.slug}
              onChange={(e: any) => setFormData({ ...formData, slug: e.target.value })}
              className="font-mono"
              disabled={!!initialData}
            />
          </div>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          <div className="space-y-2">
            <Label required>Inference Provider</Label>
            <Select
              value={currentProvider}
              onChange={(e: any) => handleProviderChange(e.target.value)}
            >
              {PROVIDER_GROUPS.map((p) => (
                <option key={p.value} value={p.value}>
                  {p.label}
                </option>
              ))}
            </Select>
          </div>

          <div className="space-y-2">
            <Label required>AI Model</Label>
            <Select
              value={formData.model}
              onChange={(e: any) => setFormData({ ...formData, model: e.target.value })}
            >
              {availableModels.map((m) => (
                <option key={m.id} value={m.id}>
                  {m.name}
                </option>
              ))}
            </Select>
          </div>
        </div>

        {/* Dynamic Config Controls */}
        <div className="grid grid-cols-1 md:grid-cols-3 gap-3 p-3.5 bg-secondary/15 rounded-lg border border-border border-dashed">
          {currentProvider === 'gemini' && (
            <div className="flex items-center justify-between p-2 bg-background rounded border border-border/50">
              <span className="text-xs font-semibold flex items-center gap-1.5">
                <Globe className="h-4.5 w-4.5 text-blue-400" /> Google Search
              </span>
              <Switch
                checked={formData.config?.grounding || false}
                onCheckedChange={(c) => updateConfig('grounding', c)}
              />
            </div>
          )}
          <div className="flex items-center justify-between p-2 bg-background rounded border border-border/50">
            <span className="text-xs font-semibold flex items-center gap-1.5">
              <RefreshCw className="h-4.5 w-4.5 text-purple-400 animate-spin-slow" /> SSE Streaming
            </span>
            <Switch
              checked={formData.config?.streaming || false}
              onCheckedChange={(c) => updateConfig('streaming', c)}
            />
          </div>
          <div className="flex items-center justify-between p-2 bg-background rounded border border-border/50">
            <span className="text-xs font-semibold flex items-center gap-1.5">
              <BrainCircuit className="h-4.5 w-4.5 text-emerald-400" /> URL Scraper
            </span>
            <Switch
              checked={formData.config?.url_context || false}
              onCheckedChange={(c) => updateConfig('url_context', c)}
            />
          </div>
        </div>

        <div className="space-y-2">
          <Label>System Instructions (Behavior Persona)</Label>
          <Input
            value={formData.system_prompt || ''}
            onChange={(e: any) => setFormData({ ...formData, system_prompt: e.target.value })}
            placeholder="You are an expert compiler engineer..."
          />
        </div>

        <div className="flex-1 flex flex-col space-y-2 min-h-[150px]">
          <div className="flex justify-between items-center">
            <Label className="flex items-center gap-2">
              <Sparkles className="h-3.5 w-3.5 text-primary" /> User Prompt Template
            </Label>
          </div>
          <div className="flex-1 relative rounded-md border border-input overflow-hidden">
            <textarea
              className="absolute inset-0 w-full h-full bg-[#1e1e1e] text-[#d4d4d4] font-mono text-sm p-4 focus:outline-none resize-none leading-relaxed"
              value={formData.template}
              onChange={(e) => setFormData({ ...formData, template: e.target.value })}
              placeholder="Analyze the following text: {{text}}"
              spellCheck={false}
            />
          </div>
        </div>

        <div className="flex items-center justify-between pt-4 border-t border-border mt-auto shrink-0">
          <div className="flex items-center gap-2 text-xs text-muted-foreground">
            <Info className="h-4 w-4" />
            <span>Parameters in template become SDK payload keys.</span>
          </div>
          <div className="flex gap-3">
            <Button variant="ghost" onClick={onClose}>
              Cancel
            </Button>
            <Button
              onClick={handleSave}
              isLoading={isSaving}
              disabled={!formData.name || !formData.slug || !formData.template}
            >
              <Save className="mr-2 h-4 w-4" /> Save Action
            </Button>
          </div>
        </div>
      </div>
    </Dialog>
  );
};
