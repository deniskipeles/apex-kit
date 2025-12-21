import React, { useState, useEffect } from 'react';
import { Save, Code } from 'lucide-react';
import { Button, Input, Label, Select } from '../../../components/ui/Elements';
import { Dialog } from '../../../components/ui/Dialog';
import { Template, Script } from '../../../types';
import { scriptsService } from '../../scripts/services/scriptsService';
import { AiCodeAssistant } from '../../ai/components/AiCodeAssistant';
import { CodeEditor } from '../../../components/form/CodeEditor'; // Import Monaco Editor

interface TemplateEditorProps {
  isOpen: boolean;
  onClose: () => void;
  onSave: (data: Partial<Template>) => Promise<void>;
  initialData?: Template;
}

export const TemplateEditor = ({ isOpen, onClose, onSave, initialData }: TemplateEditorProps) => {
  const [formData, setFormData] = useState<Partial<Template>>({
    slug: '',
    content: '<!-- HTML/HTMX/Tera Here -->\n<h1>Hello {{ params.name }}</h1>',
    script_id: null
  });
  const [scripts, setScripts] = useState<Script[]>([]);
  const [isSaving, setIsSaving] = useState(false);

  useEffect(() => {
    if (initialData) setFormData(initialData);
    // Load scripts for dropdown
    scriptsService.list().then(setScripts);
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

  if (!isOpen) return null;

  return (
    <Dialog isOpen={isOpen} onClose={onClose} title={initialData ? 'Edit Template' : 'New Template'} size="xl">
      <div className="flex flex-col h-[85vh]">
        <div className="grid grid-cols-1 md:grid-cols-3 gap-6 mb-6 h-full overflow-hidden">
            
            {/* Sidebar Settings */}
            <div className="space-y-4 overflow-y-auto pr-2">
                <div className="space-y-2">
                    <Label>Slug (URL Path)</Label>
                    <Input 
                        value={formData.slug} 
                        onChange={(e: any) => setFormData({...formData, slug: e.target.value})} 
                        placeholder="components/header" 
                        disabled={!!initialData} // Slug immutable
                        className="font-mono"
                    />
                    <p className="text-[10px] text-muted-foreground">Public URL: /render/{formData.slug || '...'}</p>
                </div>
                
                <div className="space-y-2">
                    <Label>Linked Data Script</Label>
                    <Select 
                        value={formData.script_id || ''} 
                        onChange={(e: any) => setFormData({...formData, script_id: e.target.value || null})}
                    >
                        <option value="">-- None --</option>
                        {scripts.map(s => (
                            <option key={s.id} value={s.id}>{s.name}</option>
                        ))}
                    </Select>
                    <p className="text-[10px] text-muted-foreground">Script output will be available as variables.</p>
                </div>

                 <div className="pt-4 border-t border-border">
                    <Label className="mb-2 block">AI Assistant</Label>
                    <AiCodeAssistant 
                        currentCode={formData.content || ''}
                        contextType="template"
                        onApply={(code) => setFormData({ ...formData, content: code })}
                    />
                </div>
            </div>

            {/* Editor Area */}
            <div className="md:col-span-2 flex flex-col h-full border-l border-border pl-0 md:pl-6">
                <div className="flex items-center justify-between mb-2">
                    <Label className="flex items-center gap-2"><Code className="h-4 w-4"/> HTML / Tera Template</Label>
                    <div className="text-[10px] text-muted-foreground font-mono">
                         Supported: HTML5, Tailwind, HTMX, Tera Syntax
                    </div>
                </div>
                
                <div className="flex-1 min-h-[400px]">
                    <CodeEditor 
                        value={formData.content || ''}
                        onChange={(val) => setFormData({...formData, content: val})}
                        language="html"
                        height="100%"
                        label="TEMPLATE CODE"
                    />
                </div>
            </div>
        </div>

        <div className="flex justify-end gap-3 pt-4 border-t border-border mt-auto">
            <Button variant="ghost" onClick={onClose}>Cancel</Button>
            <Button onClick={handleSave} isLoading={isSaving} disabled={!formData.slug}>
                <Save className="mr-2 h-4 w-4" /> Save Template
            </Button>
        </div>
      </div>
    </Dialog>
  );
};