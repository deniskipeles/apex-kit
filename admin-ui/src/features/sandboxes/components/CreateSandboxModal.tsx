import React, { useState, useEffect } from 'react';
import { Dialog } from '../../../components/ui/Dialog';
import { Button, Input, Select, Label, Textarea, Badge } from '../../../components/ui/Elements';
import { Checkbox } from '../../../components/form/Checkbox';
import { collectionsService } from '../../collections/services/collectionsService';
import { scriptsService } from '../../scripts/services/scriptsService';
import { templatesService } from '../../templates/services/templatesService';
import { architectService } from '../../ai/services/architectService';
import { useToast } from '../../../components/feedback/Toast';
import { AI_MODELS, DEFAULT_AI_MODEL } from '../../../config/ai-models';
import { Collection, Script, Template } from '../../../types';
import { Database, FileCode, LayoutTemplate, Loader2, Sparkles } from 'lucide-react';

interface CreateSandboxModalProps {
  isOpen: boolean;
  onClose: () => void;
  onSuccess: () => void;
}

export const CreateSandboxModal = ({ isOpen, onClose, onSuccess }: CreateSandboxModalProps) => {
  const { toast } = useToast();

  // Form State
  const [name, setName] = useState('');
  const [strategy, setStrategy] = useState<'none' | 'schema' | 'partial' | 'full' | 'selected'>(
    'none'
  );
  const [recordLimit, setRecordLimit] = useState<number>(100);
  const [model, setModel] = useState(DEFAULT_AI_MODEL);
  const [initialPrompt, setInitialPrompt] = useState('');

  // Selection State (for advancedSelected)
  const [selectedCols, setSelectedCols] = useState<string[]>([]);
  const [selectedScripts, setSelectedScripts] = useState<string[]>([]);
  const [selectedTemplates, setSelectedTemplates] = useState<string[]>([]);

  // Metadata State
  const [availableCols, setAvailableCols] = useState<Collection[]>([]);
  const [availableScripts, setAvailableScripts] = useState<Script[]>([]);
  const [availableTemplates, setAvailableTemplates] = useState<Template[]>([]);
  const [isLoadingResources, setIsLoadingResources] = useState(false);
  const [isSubmitting, setIsSubmitting] = useState(false);

  // Load parent resources when the modal opens
  useEffect(() => {
    if (isOpen) {
      setIsLoadingResources(true);
      Promise.all([collectionsService.list(), scriptsService.list(), templatesService.list()])
        .then(([cols, scrs, tmpls]) => {
          setAvailableCols(cols);
          setAvailableScripts(scrs.local || []);
          setAvailableTemplates(tmpls);
        })
        .catch(() => {
          toast('Failed to load parent resources for partial cloning', 'error');
        })
        .finally(() => {
          setIsLoadingResources(false);
        });
    }
  }, [isOpen, toast]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim()) return;

    setIsSubmitting(true);
    try {
      // Cast the custom "selected" options to parameters for the core API
      await architectService.createSession(
        name,
        initialPrompt.trim() ? initialPrompt : undefined,
        model,
        strategy,
        strategy === 'partial' || strategy === 'selected' ? recordLimit : undefined,
        strategy === 'selected' ? selectedCols : undefined,
        strategy === 'selected' ? selectedScripts : undefined,
        strategy === 'selected' ? selectedTemplates : undefined
      );

      toast(`Sandbox "${name}" initialized successfully`, 'success');
      onSuccess();
      handleClose();
    } catch (err: any) {
      toast(err.message || 'Failed to initialize sandbox', 'error');
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleClose = () => {
    setName('');
    setStrategy('none');
    setRecordLimit(100);
    setInitialPrompt('');
    setSelectedCols([]);
    setSelectedScripts([]);
    setSelectedTemplates([]);
    onClose();
  };

  const toggleSelection = (id: string, list: string[], setter: (v: string[]) => void) => {
    if (list.includes(id)) {
      setter(list.filter((x) => x !== id));
    } else {
      setter([...list, id]);
    }
  };

  return (
    <Dialog isOpen={isOpen} onClose={handleClose} title="Initialize Sandbox" size="lg">
      <form onSubmit={handleSubmit} className="space-y-6 pb-4">
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          <div className="space-y-2">
            <Label required>Sandbox Name</Label>
            <Input
              value={name}
              onChange={(e: any) => setName(e.target.value)}
              placeholder="e.g. staging-v2-test"
              required
              disabled={isSubmitting}
            />
          </div>

          <div className="space-y-2">
            <Label>Clone Strategy</Label>
            <Select
              value={strategy}
              onChange={(e: any) => setStrategy(e.target.value as any)}
              disabled={isSubmitting}
            >
              <option value="none">Empty Sandbox (None)</option>
              <option value="schema">Schemas Only (Structure)</option>
              <option value="partial">Partial Clone (Structure + Record Limit)</option>
              <option value="full">Full Clone (Direct DB Copy - No logs/vectors)</option>
              <option value="selected">Advanced Selective Clone (Custom)</option>
            </Select>
          </div>
        </div>

        {/* Partial Record Limit Input */}
        {(strategy === 'partial' || strategy === 'selected') && (
          <div className="space-y-2 animate-in fade-in slide-in-from-top-1 duration-200">
            <Label>Max Records to Clone (Per Collection)</Label>
            <Input
              type="number"
              min="0"
              value={recordLimit}
              onChange={(e: any) => setRecordLimit(Number(e.target.value))}
              disabled={isSubmitting}
            />
            <p className="text-[10px] text-muted-foreground">
              Wards off bloat by capping rows copied. Set to 0 to copy schemas with no data.
            </p>
          </div>
        )}

        {/* Immersive Selected Resources Configuration */}
        {strategy === 'selected' && (
          <div className="border border-border rounded-lg p-4 bg-secondary/10 space-y-4 animate-in fade-in duration-300">
            <h4 className="text-xs font-bold uppercase tracking-wider text-primary">
              Select Resources to Import
            </h4>

            {isLoadingResources ? (
              <div className="flex items-center justify-center py-6 text-xs text-muted-foreground gap-2">
                <Loader2 className="h-4 w-4 animate-spin text-primary" /> Loading current schemas
                and scripts...
              </div>
            ) : (
              <div className="grid grid-cols-1 md:grid-cols-3 gap-4 h-[250px] overflow-hidden">
                {/* 1. Collections Selection */}
                <div className="flex flex-col border border-border bg-background rounded-lg p-2 overflow-y-auto custom-scrollbar">
                  <div className="text-[10px] font-bold text-muted-foreground uppercase pb-1 mb-2 border-b border-border flex items-center gap-1.5">
                    <Database className="h-3 w-3 text-blue-500" /> Schemas ({availableCols.length})
                  </div>
                  {availableCols.map((c) => (
                    <div
                      key={c.id}
                      onClick={() => toggleSelection(c.name, selectedCols, setSelectedCols)}
                      className={`flex items-center gap-2 p-1.5 rounded cursor-pointer transition-colors hover:bg-secondary/50 ${selectedCols.includes(c.name) ? 'bg-primary/5 text-primary' : ''}`}
                    >
                      <Checkbox checked={selectedCols.includes(c.name)} onChange={() => {}} />
                      <span className="text-xs truncate font-medium">{c.name}</span>
                    </div>
                  ))}
                </div>

                {/* 2. Scripts Selection */}
                <div className="flex flex-col border border-border bg-background rounded-lg p-2 overflow-y-auto custom-scrollbar">
                  <div className="text-[10px] font-bold text-muted-foreground uppercase pb-1 mb-2 border-b border-border flex items-center gap-1.5">
                    <FileCode className="h-3 w-3 text-yellow-500" /> Scripts (
                    {availableScripts.length})
                  </div>
                  {availableScripts.map((s) => (
                    <div
                      key={s.id}
                      onClick={() => toggleSelection(s.name, selectedScripts, setSelectedScripts)}
                      className={`flex items-center gap-2 p-1.5 rounded cursor-pointer transition-colors hover:bg-secondary/50 ${selectedScripts.includes(s.name) ? 'bg-primary/5 text-primary' : ''}`}
                    >
                      <Checkbox checked={selectedScripts.includes(s.name)} onChange={() => {}} />
                      <span className="text-xs truncate font-medium">{s.name}</span>
                    </div>
                  ))}
                </div>

                {/* 3. Templates Selection */}
                <div className="flex flex-col border border-border bg-background rounded-lg p-2 overflow-y-auto custom-scrollbar">
                  <div className="text-[10px] font-bold text-muted-foreground uppercase pb-1 mb-2 border-b border-border flex items-center gap-1.5">
                    <LayoutTemplate className="h-3 w-3 text-purple-500" /> Pages (
                    {availableTemplates.length})
                  </div>
                  {availableTemplates.map((t) => (
                    <div
                      key={t.id}
                      onClick={() =>
                        toggleSelection(t.slug, selectedTemplates, setSelectedTemplates)
                      }
                      className={`flex items-center gap-2 p-1.5 rounded cursor-pointer transition-colors hover:bg-secondary/50 ${selectedTemplates.includes(t.slug) ? 'bg-primary/5 text-primary' : ''}`}
                    >
                      <Checkbox checked={selectedTemplates.includes(t.slug)} onChange={() => {}} />
                      <span className="text-xs truncate font-medium">{t.slug}</span>
                    </div>
                  ))}
                </div>
              </div>
            )}
          </div>
        )}

        <div className="h-px bg-border/50" />

        {/* AI Copilot Initialization Prompt */}
        <div className="space-y-4">
          <h4 className="text-xs font-bold uppercase tracking-wider text-muted-foreground flex items-center gap-2">
            <Sparkles className="h-4 w-4 text-primary" /> Optional: AI Copilot Prompt on Launch
          </h4>

          <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
            <div className="space-y-1 md:col-span-1">
              <Label>Copilot Model</Label>
              <Select
                value={model}
                onChange={(e: any) => setModel(e.target.value)}
                disabled={isSubmitting}
              >
                {AI_MODELS.map((m) => (
                  <option key={m.value} value={m.value}>
                    {m.label}
                  </option>
                ))}
              </Select>
            </div>
            <div className="space-y-1 md:col-span-2">
              <Label>Description / Instructions for the Sandbox</Label>
              <Textarea
                value={initialPrompt}
                onChange={(e: any) => setInitialPrompt(e.target.value)}
                placeholder="e.g. Build a mini blog with post authors, comments, and a responsive frontend template..."
                rows={2}
                className="resize-none text-xs"
                disabled={isSubmitting}
              />
            </div>
          </div>
        </div>

        {/* Footer Actions */}
        <div className="flex justify-end gap-3 pt-4 border-t border-border">
          <Button type="button" variant="ghost" onClick={handleClose} disabled={isSubmitting}>
            Cancel
          </Button>
          <Button type="submit" isLoading={isSubmitting} disabled={!name.trim()}>
            Start Cloning & Launch
          </Button>
        </div>
      </form>
    </Dialog>
  );
};
