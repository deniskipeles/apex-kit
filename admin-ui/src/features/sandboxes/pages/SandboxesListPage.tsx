import React, { useState, useEffect } from 'react';
import {
  BoxIcon,
  Bot,
  Database,
  Wand2,
  RefreshCw,
  MessageSquare,
  Calendar,
  ArrowRight,
  Layers,
  Trash2,
} from 'lucide-react';
import { Button, Input, Select, Textarea, Badge } from '../../../components/ui/Elements';
import { useToast } from '../../../components/feedback/Toast';
import { ConfirmDialog } from '../../../components/feedback/ConfirmDialog';
import { apiClient } from '../../../lib/apiClient';
import { AI_MODELS, DEFAULT_AI_MODEL } from '../../../config/ai-models';
import { architectService } from '../../ai/services/architectService';

interface SandboxesListPageProps {
  onNavigate: (view: string) => void;
}

export const SandboxesListPage = ({ onNavigate }: SandboxesListPageProps) => {
  const [sessions, setSessions] = useState<any[]>([]);
  const [loading, setLoading] = useState(true);
  const [isCreating, setIsCreating] = useState(false);
  const [deleteId, setDeleteId] = useState<string | null>(null);

  const [newProjectName, setNewProjectName] = useState('');
  const [selectedModel, setSelectedModel] = useState(DEFAULT_AI_MODEL);
  const [cloneStrategy, setCloneStrategy] = useState('none');
  const [cloneRecordLimit, setCloneRecordLimit] = useState(100);

  const { toast } = useToast();

  const loadSandboxes = async () => {
    setLoading(true);
    try {
      const list = await architectService.listSessions();
      setSessions(list);
    } catch (e: any) {
      toast('Failed to load sandboxes', 'error');
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadSandboxes();
  }, []);

  const handleCreateSandbox = async () => {
    if (!newProjectName.trim()) return;
    setIsCreating(true);
    try {
      const newSession = await architectService.createSession(
        newProjectName.split('\n')[0].substring(0, 40).trim(),
        newProjectName,
        selectedModel,
        cloneStrategy,
        cloneStrategy === 'partial' ? cloneRecordLimit : undefined
      );
      toast('Sandbox created! Launching environment...', 'success');
      onNavigate(`sandbox__${newSession.id}__dashboard`);
    } catch (e: any) {
      toast(e.message || 'Failed to create sandbox', 'error');
    } finally {
      setIsCreating(false);
    }
  };

  const handleDelete = async () => {
    if (!deleteId) return;
    try {
      await apiClient.root.deleteSandbox(deleteId);
      toast('Sandbox deleted', 'success');
      setDeleteId(null);
      loadSandboxes();
    } catch (e: any) {
      toast(e.message || 'Failed to delete sandbox', 'error');
    }
  };

  return (
    <div className="space-y-8 pb-20 max-w-7xl mx-auto animate-in fade-in duration-500">
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4">
        <div className="space-y-2">
          <h1 className="text-3xl font-extrabold tracking-tight flex items-center gap-3">
            <BoxIcon className="h-8 w-8 text-primary" /> Sandboxes
          </h1>
          <p className="text-muted-foreground text-sm md:text-base max-w-2xl">
            Ephemeral development environments. Use the AI Architect to safely provision schemas,
            records, and UI templates before merging to Production.
          </p>
        </div>
        <Button
          variant="outline"
          onClick={loadSandboxes}
          disabled={loading}
          className="bg-background shadow-sm"
        >
          <RefreshCw className={`h-4 w-4 mr-2 ${loading ? 'animate-spin' : ''}`} /> Refresh
        </Button>
      </div>

      <div className="relative group rounded-3xl bg-gradient-to-br from-primary/20 via-purple-500/10 to-primary/5 p-[1px] shadow-xl transition-all hover:shadow-primary/10">
        <div className="bg-card/90 backdrop-blur-xl rounded-[23px] flex flex-col overflow-hidden">
          <Textarea
            className="border-0 focus-visible:ring-0 text-lg md:text-xl resize-none min-h-[120px] p-6 bg-transparent placeholder:text-muted-foreground/50"
            placeholder="Describe what you want to build in a new Sandbox... (e.g. A job board with users, companies, and applications...)"
            value={newProjectName}
            onChange={(e: any) => setNewProjectName(e.target.value)}
            onKeyDown={(e: any) => {
              if (e.key === 'Enter' && !e.shiftKey && newProjectName.trim()) {
                e.preventDefault();
                handleCreateSandbox();
              }
            }}
          />

          <div className="flex flex-col sm:flex-row items-start sm:items-center justify-between p-4 border-t border-border/50 bg-secondary/10 gap-4">
            <div className="flex flex-wrap items-center gap-3 w-full sm:w-auto">
              <div className="flex items-center gap-2 bg-background border border-border px-3 py-1.5 rounded-lg">
                <Bot className="h-4 w-4 text-muted-foreground" />
                <Select
                  value={selectedModel}
                  onChange={(e: any) => setSelectedModel(e.target.value)}
                  className="h-7 text-xs border-0 bg-transparent p-0 focus:ring-0 min-w-[140px]"
                >
                  {AI_MODELS.map((m) => (
                    <option key={m.value} value={m.value}>
                      {m.label}
                    </option>
                  ))}
                </Select>
              </div>

              <div className="flex items-center gap-2 bg-background border border-border px-3 py-1.5 rounded-lg">
                <Database className="h-4 w-4 text-muted-foreground" />
                <Select
                  value={cloneStrategy}
                  onChange={(e: any) => setCloneStrategy(e.target.value)}
                  className="h-7 text-xs border-0 bg-transparent p-0 focus:ring-0 min-w-[140px]"
                >
                  <option value="none">Empty Database</option>
                  <option value="schema">Clone Schema Only</option>
                  <option value="partial">Clone Schema + Data Sample</option>
                  <option value="full">Full DB Clone</option>
                </Select>
              </div>

              {cloneStrategy === 'partial' && (
                <div className="flex items-center gap-2 bg-background border border-border px-3 py-1.5 rounded-lg w-full sm:w-auto">
                  <span className="text-xs text-muted-foreground whitespace-nowrap">Limit:</span>
                  <Input
                    type="number"
                    value={cloneRecordLimit}
                    onChange={(e: any) => setCloneRecordLimit(Number(e.target.value))}
                    className="h-7 w-20 text-xs border-0 bg-transparent p-0 focus:ring-0"
                  />
                </div>
              )}
            </div>

            <Button
              onClick={handleCreateSandbox}
              isLoading={isCreating}
              disabled={!newProjectName.trim()}
              className="h-11 px-6 w-full sm:w-auto rounded-xl font-bold shadow-md hover:scale-[1.02] transition-transform"
            >
              <Wand2 className="mr-2 h-4 w-4" /> Create Sandbox
            </Button>
          </div>
        </div>
      </div>

      {loading ? (
        <div className="flex flex-col items-center justify-center py-20 gap-4 text-muted-foreground">
          <div className="h-8 w-8 border-2 border-primary border-t-transparent rounded-full animate-spin"></div>
        </div>
      ) : sessions.length === 0 ? (
        <div className="flex flex-col items-center justify-center py-20 text-center border-2 border-dashed border-border rounded-2xl bg-secondary/5">
          <div className="w-16 h-16 bg-secondary rounded-full flex items-center justify-center mb-4">
            <Layers className="h-8 w-8 text-muted-foreground/50" />
          </div>
          <h3 className="text-xl font-semibold mb-2">No Active Sandboxes</h3>
          <p className="text-muted-foreground max-w-sm">
            Use the prompt box above to spin up a new ephemeral environment.
          </p>
        </div>
      ) : (
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-6">
          {sessions.map((session) => (
            <div
              key={session.id}
              onClick={() => {
                onNavigate(`sandbox__${session.id}__dashboard`);
              }}
              className="group relative bg-card border border-border rounded-2xl p-6 cursor-pointer hover:border-primary/50 hover:shadow-xl hover:-translate-y-1 transition-all duration-300 flex flex-col h-full justify-between"
            >
              <div className="absolute top-4 right-4 opacity-0 group-hover:opacity-100 transition-opacity z-20">
                <Button
                  size="icon"
                  variant="ghost"
                  className="h-8 w-8 text-muted-foreground hover:text-destructive hover:bg-destructive/10"
                  onClick={(e: any) => {
                    e.stopPropagation();
                    setDeleteId(session.id);
                  }}
                >
                  <Trash2 className="h-4 w-4" />
                </Button>
              </div>

              <div>
                <div className="flex justify-between items-start mb-4 pr-10">
                  <div className="space-y-1.5 overflow-hidden">
                    <h3 className="font-bold text-lg leading-tight line-clamp-2 group-hover:text-primary transition-colors">
                      {session.name}
                    </h3>
                    <div className="flex items-center gap-2 text-xs text-muted-foreground font-medium mt-2">
                      <Calendar className="h-3.5 w-3.5" />
                      <span>
                        {session.expires_at
                          ? `Expires: ${new Date(session.expires_at).toLocaleDateString()}`
                          : 'Permanent'}
                      </span>
                    </div>
                  </div>
                </div>
              </div>

              <div className="mt-6 pt-4 border-t border-border flex justify-between items-center">
                {/* [FIXED] Render genuine telemetry indicators instead of analyzing skeleton */}
                <div className="flex gap-2">
                  <Badge variant="secondary" className="bg-secondary/50 font-mono text-[10px]">
                    {session.current_storage_mb
                      ? `${session.current_storage_mb.toFixed(2)} MB`
                      : '0 MB'}{' '}
                    / {session.max_storage_mb || 100} MB
                  </Badge>
                  <Badge
                    variant="outline"
                    className={`text-[10px] font-mono capitalize px-2 py-0.5 border ${session.status === 'active' ? 'text-green-500 border-green-500/20 bg-green-500/5' : 'text-zinc-500'}`}
                  >
                    {session.status || 'Active'}
                  </Badge>
                </div>
                <div className="flex items-center gap-2">
                  <span className="text-[10px] font-mono text-muted-foreground/60 bg-secondary/30 px-1.5 py-0.5 rounded">
                    #{session.id.substring(0, 8)}
                  </span>
                  <div className="h-6 w-6 rounded-full bg-primary/10 flex items-center justify-center shrink-0 group-hover:bg-primary group-hover:text-primary-foreground transition-colors">
                    <ArrowRight className="h-3 w-3 text-primary group-hover:text-white" />
                  </div>
                </div>
              </div>
            </div>
          ))}
        </div>
      )}

      <ConfirmDialog
        isOpen={!!deleteId}
        title="Delete Sandbox"
        description="Are you sure you want to delete this sandbox? All experimental schema and data will be permanently removed."
        onConfirm={handleDelete}
        onCancel={() => setDeleteId(null)}
      />
    </div>
  );
};
