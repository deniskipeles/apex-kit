import React, { useState, useEffect } from 'react';
import { Plus, Trash2, Zap, Terminal, Edit, Info } from 'lucide-react';
import { Button, Badge } from '../../../components/ui/Elements';
import { DataGrid } from '../../../components/data/DataGrid';
import { AiActionEditor } from '../components/AiActionEditor';
import { AiActionTester } from '../components/AiActionTester';
import { aiService } from '../services/aiService';
import { AiAction } from '../../../types';
import { useToast } from '../../../components/feedback/Toast';
import { ConfirmDialog } from '../../../components/feedback/ConfirmDialog';
import { ImportExportToolbar } from '@/src/components/ImportExportToolbar';
import { Dialog } from '../../../components/ui/Dialog';

export const AiActionsPage = () => {
  const [actions, setActions] = useState<AiAction[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [editorOpen, setEditorOpen] = useState(false);
  const [testerOpen, setTesterOpen] = useState(false);
  const [selectedAction, setSelectedAction] = useState<AiAction | null>(null);
  const [deleteId, setDeleteId] = useState<string | null>(null);
  const [docsOpen, setDocsOpen] = useState(false);

  const { toast } = useToast();

  const loadActions = async () => {
    setIsLoading(true);
    try {
      const data = await aiService.list();
      setActions(data);
    } catch (e) {
      console.log(e);
      toast('Failed to load actions', 'error');
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => {
    loadActions();
  }, []);

  const handleSave = async (data: Partial<AiAction>) => {
    try {
      if (selectedAction) {
        // Edit flow: Delete old action first, then create the updated version
        await aiService.delete(selectedAction.id);
      }
      await aiService.create(data);
      toast(
        selectedAction ? 'Action updated successfully' : 'Action created successfully',
        'success'
      );
      setEditorOpen(false);
      loadActions();
    } catch (e) {
      toast('Failed to save', 'error');
    }
  };

  const handleDelete = async () => {
    if (!deleteId) return;
    await aiService.delete(deleteId);
    toast('Action deleted', 'success');
    setDeleteId(null);
    loadActions();
  };

  const columns = [
    {
      field: 'name',
      headerName: 'Name / Endpoint',
      renderCell: (a: AiAction) => (
        <div>
          <div className="font-medium text-foreground">{a.name}</div>
          <div className="text-[10px] text-muted-foreground font-mono">/api/v1/ai/run/{a.slug}</div>
        </div>
      ),
    },
    {
      field: 'provider',
      headerName: 'Provider / Model',
      width: '200px',
      renderCell: (a: AiAction) => {
        const provider = a.config?.provider || 'gemini';
        return (
          <div className="flex flex-col gap-1.5">
            <Badge
              variant="secondary"
              className={`text-[9px] uppercase tracking-wider font-bold w-fit ${
                provider === 'gemini'
                  ? 'bg-blue-500/10 text-blue-400 border-blue-500/20'
                  : provider === 'groq'
                    ? 'bg-purple-500/10 text-purple-400 border-purple-500/20'
                    : 'bg-pink-500/10 text-pink-400 border-pink-500/20'
              }`}
            >
              {provider}
            </Badge>
            <div className="text-xs text-muted-foreground truncate max-w-[150px] font-mono">
              {a.model}
            </div>
          </div>
        );
      },
    },
    {
      field: 'features',
      headerName: 'Enabled Features',
      width: '220px',
      renderCell: (a: AiAction) => {
        const grounding = a.config?.grounding;
        const streaming = a.config?.streaming;
        const url_context = a.config?.url_context;

        return (
          <div className="flex flex-wrap gap-1">
            {streaming && (
              <Badge
                variant="outline"
                className="text-[9px] font-bold text-purple-400 border-purple-500/20 bg-purple-500/5"
              >
                Streaming
              </Badge>
            )}
            {grounding && (
              <Badge
                variant="outline"
                className="text-[9px] font-bold text-emerald-400 border-emerald-500/20 bg-emerald-500/5"
              >
                Search
              </Badge>
            )}
            {url_context && (
              <Badge
                variant="outline"
                className="text-[9px] font-bold text-teal-400 border-teal-500/20 bg-teal-500/5"
              >
                Scraper
              </Badge>
            )}
            {!streaming && !grounding && !url_context && (
              <span className="text-xs text-muted-foreground/40 italic">-</span>
            )}
          </div>
        );
      },
    },
    {
      field: 'actions',
      headerName: '',
      align: 'right' as const,
      width: '150px',
      renderCell: (a: AiAction) => (
        <div className="flex justify-end gap-1">
          <Button
            size="icon"
            variant="ghost"
            onClick={(e) => {
              e.stopPropagation();
              setSelectedAction(a);
              setTesterOpen(true);
            }}
            title="Test Run"
          >
            <Zap className="h-4 w-4 text-emerald-500" />
          </Button>
          <Button
            size="icon"
            variant="ghost"
            onClick={(e) => {
              e.stopPropagation();
              setSelectedAction(a);
              setEditorOpen(true);
            }}
            title="Edit Action"
          >
            <Edit className="h-4 w-4 text-muted-foreground hover:text-primary" />
          </Button>
          <Button
            size="icon"
            variant="ghost"
            onClick={(e) => {
              e.stopPropagation();
              setDeleteId(a.id);
            }}
            title="Delete Action"
          >
            <Trash2 className="h-4 w-4 text-muted-foreground hover:text-destructive" />
          </Button>
        </div>
      ),
    },
  ];

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-3xl font-bold tracking-tight">AI Actions</h2>
          <p className="text-muted-foreground">Manage LLM prompt templates and endpoints.</p>
        </div>
        <div className="flex gap-2">
          <Button variant="outline" onClick={() => setDocsOpen(true)}>
            <Info className="mr-2 h-4 w-4" /> Docs Guide
          </Button>
          <ImportExportToolbar
            onExport={aiService.exportActions}
            onImport={aiService.importActions}
          />
          <Button
            onClick={() => {
              setSelectedAction(null);
              setEditorOpen(true);
            }}
          >
            <Plus className="mr-2 h-4 w-4" /> New Action
          </Button>
        </div>
      </div>

      <div className="rounded-xl border border-border bg-card/50 backdrop-blur-sm overflow-hidden shadow-sm flex flex-col min-h-[400px]">
        <div className="flex-1">
          <DataGrid data={actions} columns={columns} keyField="id" isLoading={isLoading} />
        </div>
      </div>

      <AiActionEditor
        isOpen={editorOpen}
        onClose={() => setEditorOpen(false)}
        onSave={handleSave}
        initialData={selectedAction || undefined}
      />

      {selectedAction && (
        <AiActionTester
          action={selectedAction}
          isOpen={testerOpen}
          onClose={() => setTesterOpen(false)}
        />
      )}

      <ConfirmDialog
        isOpen={!!deleteId}
        title="Delete Action"
        description="Are you sure? Apps using this endpoint will fail."
        onConfirm={handleDelete}
        onCancel={() => setDeleteId(null)}
      />

      {/* Complete AI Actions Developer Documentation Modal */}
      <Dialog
        isOpen={docsOpen}
        onClose={() => setDocsOpen(false)}
        title="AI Actions Developer Guide"
        size="lg"
      >
        <div className="space-y-5 pb-4 text-sm text-muted-foreground leading-relaxed max-h-[70vh] overflow-y-auto pr-2 custom-scrollbar">
          <p>
            AI Actions allow you to execute complex, pre-defined LLM prompt templates securely from
            your frontend code. By defining them on the server, you protect your master
            Gemini/OpenAI API keys from being exposed to the browser.
          </p>

          <div className="h-px bg-border/50" />

          {/* 1. Standard Non-Streaming Integration */}
          <div>
            <h4 className="font-bold text-foreground mb-2 flex items-center gap-2">
              <Terminal className="h-4 w-4 text-blue-400" /> 1. Standard Execution (Non-Streaming)
            </h4>
            <p className="text-xs mb-3">
              Waits for the LLM to completely finish generating before returning a single, atomic
              JSON payload response.
            </p>
            <pre className="p-3 bg-[#0d1117] rounded-lg border border-white/5 font-mono text-xs text-[#e6edf3] overflow-x-auto">
              {`import { ApexKit } from '@apexkit/sdk';

const client = new ApexKit('https://your-api.com');

// Execute your defined Action atomically by its slug
const response = await client.ai.run('your-action-slug', {
  prompt_variable: 'Describe custom compiler mechanics...'
});

console.log(response.result);   // Full generated output string
console.log(response.metadata); // Google Search grounding & sources (if enabled)`}
            </pre>
          </div>

          <div className="h-px bg-border/30" />

          {/* 2. Real-Time Streaming Integration */}
          <div>
            <h4 className="font-bold text-foreground mb-2 flex items-center gap-2">
              <Terminal className="h-4 w-4 text-purple-400" /> 2. Real-Time Streaming Execution
              (SSE)
            </h4>
            <p className="text-xs mb-3">
              Pass an <code>onChunk</code> callback function as the third argument to receive and
              display word tokens in real-time as they are being computed by the LLM.
            </p>
            <pre className="p-3 bg-[#0d1117] rounded-lg border border-white/5 font-mono text-xs text-[#e6edf3] overflow-x-auto">
              {`import { ApexKit } from '@apexkit/sdk';

const client = new ApexKit('https://your-api.com');

// Pass an onChunk callback function to process incoming stream tokens
const response = await client.ai.run(
  'your-action-slug', 
  { prompt_variable: 'Explain compiler lexing loops...' },
  (token) => {
    // This callback fires instantly as each chunk is yielded by the LLM
    process.stdout.write(token); // Or append to your state: setOutput(prev => prev + token)
  }
);`}
            </pre>
          </div>

          <div className="h-px bg-border/50" />

          <div>
            <h4 className="font-bold text-foreground mb-1.5">Template Variables</h4>
            <p className="text-xs">
              Variables declared in your prompt template using Handlebars notation (e.g.{' '}
              <code>{'{{variable_name}}'}</code>) automatically become required arguments in your
              client-side SDK payloads.
            </p>
          </div>

          <div>
            <h4 className="font-bold text-foreground mb-1.5">Security Benefits</h4>
            <p className="text-xs bg-secondary/15 p-2.5 rounded border border-border border-dashed leading-relaxed">
              By encapsulating the system prompts and parameters on the backend, you prevent prompt
              injections, hide instructions, and restrict model rate limits securely.
            </p>
          </div>

          <div className="pt-4 flex justify-end">
            <Button onClick={() => setDocsOpen(false)}>Close Guide</Button>
          </div>
        </div>
      </Dialog>
    </div>
  );
};
