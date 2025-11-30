import React, { useState, useEffect } from 'react';
import { Save, Sparkles } from 'lucide-react';
import { Button, Input, Label, Select } from '../../../components/ui/Elements';
import { Dialog } from '../../../components/ui/Dialog';
import { AiAction } from '../../../types';

interface AiActionEditorProps {
  isOpen: boolean;
  onClose: () => void;
  onSave: (data: Partial<AiAction>) => Promise<void>;
  initialData?: AiAction;
}

const MODELS = [
    { id: 'gemini-1.5-flash', name: 'Gemini 1.5 Flash (Fast)' },
    { id: 'gemini-1.5-pro', name: 'Gemini 1.5 Pro (Powerful)' },
    { id: 'gemini-pro', name: 'Gemini Pro (Legacy)' },
];

export const AiActionEditor = ({ isOpen, onClose, onSave, initialData }: AiActionEditorProps) => {
  const [formData, setFormData] = useState<Partial<AiAction>>({
    name: '',
    slug: '',
    model: 'gemini-1.5-flash',
    system_prompt: '',
    template: 'Analyze the following data: {{input}}'
  });
  const [isSaving, setIsSaving] = useState(false);

  useEffect(() => {
    if (initialData) setFormData(initialData);
  }, [initialData, isOpen]);

  const handleSave = async () => {
    setIsSaving(true);
    try {
        await onSave(formData);
        onClose();
    } finally {
        setIsSaving(false);
    }
  };

  // Simple auto-slug
  const handleNameChange = (val: string) => {
      if (!initialData) {
          setFormData(prev => ({ 
              ...prev, 
              name: val, 
              slug: val.toLowerCase().replace(/[^a-z0-9]/g, '-').replace(/-+/g, '-') 
          }));
      } else {
          setFormData(prev => ({ ...prev, name: val }));
      }
  };

  if (!isOpen) return null;

  return (
    <Dialog isOpen={isOpen} onClose={onClose} title={initialData ? 'Edit AI Action' : 'New AI Action'} size="lg">
      <div className="flex flex-col h-[70vh] gap-6">
        
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            <div className="space-y-2">
                <Label>Action Name</Label>
                <Input value={formData.name} onChange={(e: any) => handleNameChange(e.target.value)} placeholder="e.g. Summarize Text" autoFocus />
            </div>
            <div className="space-y-2">
                <Label>Slug (API Endpoint)</Label>
                <Input value={formData.slug} onChange={(e: any) => setFormData({...formData, slug: e.target.value})} className="font-mono" disabled={!!initialData} />
            </div>
        </div>

        <div className="space-y-2">
            <Label>AI Model</Label>
            <Select value={formData.model} onChange={(e: any) => setFormData({...formData, model: e.target.value})}>
                {MODELS.map(m => <option key={m.id} value={m.id}>{m.name}</option>)}
            </Select>
        </div>

        <div className="space-y-2">
            <Label>System Instruction (Optional)</Label>
            <Input 
                value={formData.system_prompt || ''} 
                onChange={(e: any) => setFormData({...formData, system_prompt: e.target.value})} 
                placeholder="You are a helpful assistant..." 
            />
        </div>

        <div className="flex-1 flex flex-col space-y-2">
            <Label className="flex items-center gap-2"><Sparkles className="h-3.5 w-3.5 text-primary" /> Prompt Template</Label>
            <div className="flex-1 relative">
                <textarea 
                    className="absolute inset-0 w-full h-full bg-[#1e1e1e] text-[#d4d4d4] font-mono text-sm p-4 rounded-md border border-border focus:outline-none focus:ring-1 focus:ring-primary resize-none leading-relaxed"
                    value={formData.template}
                    onChange={(e) => setFormData({...formData, template: e.target.value})}
                    placeholder="Describe the task... Use {{variable}} for dynamic inputs."
                    spellCheck={false}
                />
            </div>
            <p className="text-[10px] text-muted-foreground">Use <code>{`{{variable}}`}</code> syntax. These will become JSON inputs when running the action.</p>
        </div>

        <div className="flex justify-end gap-3 pt-4 border-t border-border mt-auto shrink-0">
            <Button variant="ghost" onClick={onClose}>Cancel</Button>
            <Button onClick={handleSave} isLoading={isSaving} disabled={!formData.name || !formData.slug || !formData.template}>
                <Save className="mr-2 h-4 w-4" /> Save Action
            </Button>
        </div>
      </div>
    </Dialog>
  );
};