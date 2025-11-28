import React, { useState, useEffect } from 'react';
import { Save, X, Play } from 'lucide-react';
import { Button, Input, Label, Select, Switch } from '../../../components/ui/Elements';
import { Dialog } from '../../../components/ui/Dialog';
import { Script } from '../../../types';

interface ScriptEditorProps {
  isOpen: boolean;
  onClose: () => void;
  onSave: (data: Partial<Script>) => Promise<void>;
  initialData?: Script;
}

export const ScriptEditor = ({ isOpen, onClose, onSave, initialData }: ScriptEditorProps) => {
  const [formData, setFormData] = useState<Partial<Script>>({
    name: '',
    trigger_type: 'manual',
    code: '// Available globals: $db, $http, $input, log()\n\nconst name = $input.name || "World";\nlog("Hello " + name);\nreturn { message: "Hello " + name };',
    active: true
  });
  const [isSaving, setIsSaving] = useState(false);

  useEffect(() => {
    if (initialData) {
      setFormData(initialData);
    }
  }, [initialData]);

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
    <Dialog isOpen={isOpen} onClose={onClose} title={initialData ? 'Edit Script' : 'New Script'} size="xl">
      <div className="flex flex-col h-[70vh]">
        <div className="grid grid-cols-1 md:grid-cols-3 gap-6 mb-6">
            <div className="space-y-4">
                <div className="space-y-2">
                    <Label>Script Name (Slug)</Label>
                    <Input 
                        value={formData.name} 
                        onChange={(e: any) => setFormData({...formData, name: e.target.value})} 
                        placeholder="my-custom-endpoint" 
                        className="font-mono"
                        disabled={!!initialData} // Slugs usually immutable after creation
                    />
                    <p className="text-[10px] text-muted-foreground">Used in URL: /api/v1/run/{formData.name || '...'}</p>
                </div>
                
                <div className="space-y-2">
                    <Label>Trigger Type</Label>
                    <Select 
                        value={formData.trigger_type} 
                        onChange={(e: any) => setFormData({...formData, trigger_type: e.target.value})}
                    >
                        <option value="manual">Manual (API Endpoint)</option>
                        <option value="cron">Scheduled (Cron)</option>
                        <option value="before_create">Before Record Create</option>
                        <option value="after_create">After Record Create</option>
                    </Select>
                </div>

                <div className="flex items-center justify-between p-3 border border-border rounded-lg bg-secondary/5">
                    <Label>Active</Label>
                    <Switch checked={formData.active} onCheckedChange={(c: boolean) => setFormData({...formData, active: c})} />
                </div>
            </div>

            <div className="md:col-span-2 flex flex-col">
                <Label className="mb-2">JavaScript Code</Label>
                <textarea 
                    className="flex-1 w-full bg-[#1e1e1e] text-[#d4d4d4] font-mono text-sm p-4 rounded-md border border-border focus:outline-none focus:ring-1 focus:ring-primary resize-none leading-relaxed"
                    value={formData.code}
                    onChange={(e) => setFormData({...formData, code: e.target.value})}
                    spellCheck={false}
                />
            </div>
        </div>

        <div className="flex justify-end gap-3 pt-4 border-t border-border mt-auto">
            <Button variant="ghost" onClick={onClose}>Cancel</Button>
            <Button onClick={handleSave} isLoading={isSaving} disabled={!formData.name}>
                <Save className="mr-2 h-4 w-4" /> Save Script
            </Button>
        </div>
      </div>
    </Dialog>
  );
};