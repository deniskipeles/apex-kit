import React, { useState, useEffect } from 'react';
import {
  Sparkles,
  Plus,
  MessageSquare,
  Calendar,
  ArrowRight,
  Loader2,
  Bot,
  Layers,
  FileCode2,
} from 'lucide-react';
import { Button, Card, CardContent, Input, Select } from '../../../components/ui/Elements';
import { AiSessionPanel } from '../components/AiSessionPanel';
import { architectService, AiSession } from '../services/architectService';
import { useToast } from '../../../components/feedback/Toast';
import { AI_MODELS, DEFAULT_AI_MODEL } from '../../../config/ai-models';

export const AiArchitectPage = () => {
  const [sessions, setSessions] = useState<AiSession[]>([]);
  const [activeSession, setActiveSession] = useState<AiSession | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [isCreating, setIsCreating] = useState(false);
  const [selectedModel, setSelectedModel] = useState(DEFAULT_AI_MODEL);
  const [newProjectName, setNewProjectName] = useState('');
  const [cloneStrategy, setCloneStrategy] = useState('none');
  const [cloneRecordLimit, setCloneRecordLimit] = useState(100);
  const { toast } = useToast();

  const loadSessions = async () => {
    try {
      const data = await architectService.listSessions();
      setSessions(data);
    } catch (e) {
      console.error(e);
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => {
    loadSessions();
  }, []);

  const handleCreate = async () => {
    if (!newProjectName.trim()) return;
    setIsCreating(true);
    try {
      const newSession = await architectService.createSession(
        newProjectName,
        undefined,
        selectedModel,
        cloneStrategy, // Pass strategy
        cloneStrategy === 'partial' ? cloneRecordLimit : undefined // Pass limit if partial
      );
      setSessions([newSession, ...sessions]);
      setActiveSession(newSession);
      setNewProjectName('');
      toast('Project initialized successfully. Data cloning in background.', 'success');
    } catch (e: any) {
      toast(e.message || 'Failed to create session', 'error');
    } finally {
      setIsCreating(false);
    }
  };

  return (
    <div className="space-y-8 pb-20 max-w-7xl mx-auto">
      {/* Header Section */}
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4">
        <div className="space-y-1">
          <h1 className="text-3xl font-extrabold tracking-tight flex items-center gap-3 bg-clip-text text-transparent bg-gradient-to-r from-primary to-purple-500">
            <Sparkles className="h-8 w-8 text-primary" /> AI Architect
          </h1>
          <p className="text-muted-foreground text-sm md:text-base max-w-2xl">
            Build, iterate, and deploy full-stack applications using natural language. Describe your
            app, and the AI will generate the schema, API, and UI.
          </p>
        </div>
      </div>

      {/* Create New Project Bar */}
      <div className="relative group rounded-xl bg-gradient-to-r from-primary/10 via-purple-500/5 to-primary/5 border border-primary/20 p-1 shadow-sm transition-all hover:shadow-md hover:border-primary/40">
        <div className="bg-card/50 backdrop-blur-sm rounded-lg p-4 md:p-6 flex flex-col md:flex-row gap-4 items-center">
          <div className="h-12 w-12 rounded-full bg-gradient-to-br from-primary to-purple-600 flex items-center justify-center shrink-0 shadow-lg group-hover:scale-105 transition-transform">
            <Plus className="h-6 w-6 text-white" />
          </div>

          <div className="flex-1 flex flex-col md:flex-row gap-3 w-full">
            <div className="flex-1">
              <Input
                placeholder="What do you want to build? (e.g. Job Board, CRM)..."
                value={newProjectName}
                onChange={(e: any) => setNewProjectName(e.target.value)}
                className="h-11 bg-background/80 border-primary/20 focus:border-primary focus:ring-primary/20 text-base"
                onKeyDown={(e: any) => e.key === 'Enter' && handleCreate()}
              />
            </div>
            <div className="w-full md:w-64">
              <Select
                value={selectedModel}
                onChange={(e: any) => setSelectedModel(e.target.value)}
                className="h-11 bg-background/80 border-primary/20"
              >
                {AI_MODELS.map((m) => (
                  <option key={m.value} value={m.value}>
                    {m.label}
                  </option>
                ))}
              </Select>
            </div>
            <Button
              onClick={handleCreate}
              isLoading={isCreating}
              disabled={!newProjectName}
              className="h-11 px-8 font-semibold shadow-lg hover:shadow-primary/25 transition-all"
            >
              Start Project
            </Button>
          </div>
        </div>

        {/* [NEW] ADVANCED OPTIONS */}
        <div className="w-full border-t border-border/50 pt-4 mt-2 flex flex-col md:flex-row items-center gap-3">
          <div className="w-full md:w-1/3">
            <label className="text-xs font-semibold text-muted-foreground mb-1 block">
              AI Model
            </label>
            <Select
              value={selectedModel}
              onChange={(e: any) => setSelectedModel(e.target.value)}
              className="h-9 bg-background/80 border-border/50"
            >
              {AI_MODELS.map((m) => (
                <option key={m.value} value={m.value}>
                  {m.label}
                </option>
              ))}
            </Select>
          </div>
          <div className="w-full md:w-1/3">
            <label className="text-xs font-semibold text-muted-foreground mb-1 block">
              Initial State
            </label>
            <Select
              value={cloneStrategy}
              onChange={(e: any) => setCloneStrategy(e.target.value)}
              className="h-9 bg-background/80 border-border/50"
            >
              <option value="none">Empty Sandbox</option>
              <option value="schema">Clone Schema Only</option>
              <option value="partial">Clone Schema + N Records</option>
              <option value="full">Full Clone</option>
            </Select>
          </div>
          <div
            className={`w-full md:w-1/3 transition-opacity duration-300 ${cloneStrategy === 'partial' ? 'opacity-100' : 'opacity-30 pointer-events-none'}`}
          >
            <label className="text-xs font-semibold text-muted-foreground mb-1 block">
              Record Limit
            </label>
            <Input
              type="number"
              value={cloneRecordLimit}
              onChange={(e: any) => setCloneRecordLimit(Number(e.target.value))}
              className="h-9 bg-background/80 border-border/50"
              disabled={cloneStrategy !== 'partial'}
            />
          </div>
        </div>
      </div>

      {/* Session Grid */}
      {isLoading ? (
        <div className="flex flex-col items-center justify-center py-20 gap-4 text-muted-foreground animate-in fade-in">
          <Loader2 className="h-10 w-10 animate-spin text-primary" />
          <p>Loading your workspace...</p>
        </div>
      ) : sessions.length === 0 ? (
        <div className="flex flex-col items-center justify-center py-20 text-center border-2 border-dashed border-border rounded-xl bg-secondary/5">
          <div className="h-20 w-20 bg-secondary/30 rounded-full flex items-center justify-center mb-4">
            <Bot className="h-10 w-10 text-muted-foreground" />
          </div>
          <h3 className="text-xl font-semibold mb-2">No projects yet</h3>
          <p className="text-muted-foreground max-w-md mb-6">
            Start your first AI-driven development session by entering a project name above.
          </p>
        </div>
      ) : (
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-6">
          {sessions.map((session) => (
            <div
              key={session.id}
              onClick={() => setActiveSession(session)}
              className="group relative bg-card border border-border rounded-xl p-5 cursor-pointer hover:border-primary/50 hover:shadow-lg hover:-translate-y-1 transition-all duration-200 flex flex-col h-full"
            >
              {/* Card Header */}
              <div className="flex justify-between items-start mb-4">
                <div className="space-y-1 overflow-hidden">
                  <h3 className="font-bold text-lg truncate pr-2 group-hover:text-primary transition-colors">
                    {session.name}
                  </h3>
                  <div className="flex items-center gap-2 text-xs text-muted-foreground">
                    <Calendar className="h-3 w-3" />
                    <span>{new Date(session.created_at).toLocaleDateString()}</span>
                    <span>•</span>
                    <MessageSquare className="h-3 w-3" />
                    <span>{session.messages.length} msgs</span>
                  </div>
                </div>
                <div className="h-8 w-8 rounded-full bg-secondary flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity">
                  <ArrowRight className="h-4 w-4 text-primary" />
                </div>
              </div>

              {/* Card Stats / Badges */}
              <div className="mt-auto space-y-4">
                {session.current_manifest ? (
                  <div className="flex flex-wrap gap-2">
                    <span className="inline-flex items-center gap-1 bg-blue-500/10 text-blue-500 px-2 py-1 rounded-md text-xs font-medium border border-blue-500/20">
                      <Layers className="h-3 w-3" />
                      {session.current_manifest.collections.length} Collections
                    </span>
                    <span className="inline-flex items-center gap-1 bg-purple-500/10 text-purple-500 px-2 py-1 rounded-md text-xs font-medium border border-purple-500/20">
                      <FileCode2 className="h-3 w-3" />
                      {session.current_manifest.templates.length} Pages
                    </span>
                  </div>
                ) : (
                  <div className="text-xs text-muted-foreground italic py-1">
                    Initial setup pending...
                  </div>
                )}

                {/* Action Footer */}
                <div className="pt-4 mt-2 border-t border-border flex justify-end">
                  <Button
                    variant="outline"
                    size="sm"
                    className="w-full bg-background/50 hover:bg-primary hover:text-primary-foreground transition-colors text-xs h-9"
                    onClick={(e) => {
                      e.stopPropagation();
                      window.location.href = `/_dashboard/sandbox/${session.id}`;
                    }}
                  >
                    Launch Sandbox UI
                  </Button>
                </div>
              </div>
            </div>
          ))}
        </div>
      )}

      {/* Slide-over Panel */}
      <AiSessionPanel
        session={activeSession}
        onClose={() => setActiveSession(null)}
        onUpdate={(updated) => {
          setSessions((prev) => prev.map((s) => (s.id === updated.id ? updated : s)));
          setActiveSession(updated);
        }}
      />
    </div>
  );
};
