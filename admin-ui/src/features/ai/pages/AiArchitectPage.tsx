import React, { useState, useEffect, useRef, useCallback } from 'react';
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
  Send,
  Code,
  RefreshCw,
  UploadCloud,
  X,
  FileJson,
  User,
  Terminal,
} from 'lucide-react';
import {
  Button,
  Card,
  CardContent,
  Input,
  Select,
  Badge,
  Textarea,
  CardHeader,
  CardTitle,
} from '../../../components/ui/Elements';
import { useToast } from '../../../components/feedback/Toast';
import { apiClient } from '../../../lib/apiClient';
import { AI_MODELS, DEFAULT_AI_MODEL } from '../../../config/ai-models';
import { AiSession, ChatMessage } from '../../../types';
import { architectService } from '../services/architectService';

export const AiArchitectPage = () => {
  const [sessions, setSessions] = useState<AiSession[]>([]);
  const [activeSession, setActiveSession] = useState<AiSession | null>(null);
  const [loading, setLoading] = useState(true);
  const [isCreating, setIsCreating] = useState(false);
  const [selectedModel, setSelectedModel] = useState(DEFAULT_AI_MODEL);
  const [newProjectName, setNewProjectName] = useState('');
  const [cloneStrategy, setCloneStrategy] = useState('none');
  const [cloneRecordLimit, setCloneRecordLimit] = useState(100);

  // Scoped Sandbox Execution State
  const [input, setInput] = useState('');
  const [isThinking, setIsThinking] = useState(false);
  const [isApplying, setIsApplying] = useState(false);

  const scrollRef = useRef<HTMLDivElement>(null);
  const { toast } = useToast();

  const isSandbox = apiClient.getScope().type === 'sandbox';
  const sandboxId = apiClient.getScope().id;

  // 1. Initial Load Router
  const loadData = useCallback(async () => {
    setLoading(true);
    try {
      if (isSandbox) {
        // Scoped Sandbox View: Fetch the active session from Sandbox DB
        const active = await apiClient.ai.getSession();
        setActiveSession(active);
      } else {
        // Root View: Fetch list of all active projects
        const list = await architectService.listSessions();
        setSessions(list);
      }
    } catch (e: any) {
      toast('Failed to initialize workspace data', 'error');
    } finally {
      setLoading(false);
    }
  }, [isSandbox, toast]);

  useEffect(() => {
    loadData();
  }, [loadData]);

  // Scroll chat window down
  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [activeSession?.messages, isThinking]);

  // --- ACTIONS (ROOT VIEW) ---

  const handleCreateProject = async () => {
    if (!newProjectName.trim()) return;
    setIsCreating(true);
    try {
      const newSession = await architectService.createSession(
        newProjectName,
        undefined,
        selectedModel,
        cloneStrategy,
        cloneStrategy === 'partial' ? cloneRecordLimit : undefined
      );
      setSessions([newSession, ...sessions]);
      setActiveSession(newSession);
      setNewProjectName('');
      toast('Project initialized successfully.', 'success');
    } catch (e: any) {
      toast(e.message || 'Failed to create project', 'error');
    } finally {
      setIsCreating(false);
    }
  };

  // --- ACTIONS (SCOPED SANDBOX CHAT) ---

  const handleSendPrompt = async () => {
    if (!input.trim() || !activeSession) return;
    const promptText = input;
    setInput('');
    setIsThinking(true);

    // Optimistic UI update
    const optimisticMessages: ChatMessage[] = [
      ...activeSession.messages,
      { role: 'user', content: promptText },
    ];
    setActiveSession({ ...activeSession, messages: optimisticMessages });

    try {
      const updated = await apiClient.ai.chat(promptText, selectedModel);
      setActiveSession(updated);
      if (updated.diff_summary) {
        toast('Changes drafted. Review diff and apply.', 'info');
      }
    } catch (e: any) {
      toast(e.message || 'Inference engine failed', 'error');
    } finally {
      setIsThinking(false);
    }
  };

  const handleApplyPending = async () => {
    if (!activeSession) return;
    setIsApplying(true);
    try {
      const updated = await apiClient.ai.applySessionChanges();
      setActiveSession(updated);
      toast('Changes applied successfully to Sandbox DB.', 'success');
    } catch (e: any) {
      toast('Failed to apply schema changes', 'error');
    } finally {
      setIsApplying(false);
    }
  };

  const handleDiscardPending = () => {
    if (!activeSession) return;
    setActiveSession({
      ...activeSession,
      pending_manifest: null,
      diff_summary: null,
    });
    toast('Draft changes discarded.', 'info');
  };

  const handlePublishProject = async () => {
    if (!sandboxId) return;
    setIsApplying(true);
    try {
      await apiClient.root.publishSandbox(sandboxId);
      toast('Project successfully merged and published to Production!', 'success');
      // Redirect back to sandboxes list
      window.location.href = '/_dashboard/sandboxes';
    } catch (e: any) {
      toast('Failed to publish project', 'error');
    } finally {
      setIsApplying(false);
    }
  };

  // --- RENDER SCOPED SANDBOX CHAT VIEW ---
  if (isSandbox) {
    if (loading || !activeSession) {
      return (
        <div className="flex h-[80vh] items-center justify-center flex-col gap-4">
          <Loader2 className="animate-spin text-amber-500 h-10 w-10" />
          <p className="text-muted-foreground animate-pulse text-sm font-medium">
            Connecting to sandbox environment...
          </p>
        </div>
      );
    }

    return (
      <div className="grid grid-cols-1 lg:grid-cols-12 gap-6 h-[calc(100vh-140px)] animate-in fade-in duration-300">
        {/* Left Side: Immersive Chat Column */}
        <div className="lg:col-span-8 flex flex-col h-full bg-card border border-border rounded-xl overflow-hidden shadow-sm">
          <div className="p-4 border-b border-border bg-secondary/10 flex items-center justify-between shrink-0">
            <div className="flex items-center gap-2">
              <Bot className="h-5 w-5 text-amber-500" />
              <span className="font-bold text-sm">Architect Copilot</span>
            </div>
            <div className="w-48">
              <Select
                value={selectedModel}
                onChange={(e: any) => setSelectedModel(e.target.value)}
                className="h-7 text-xs py-0"
              >
                {AI_MODELS.map((m) => (
                  <option key={m.value} value={m.value}>
                    {m.label}
                  </option>
                ))}
              </Select>
            </div>
          </div>

          {/* Chat Logs */}
          <div className="flex-1 overflow-y-auto p-4 space-y-4 custom-scrollbar" ref={scrollRef}>
            <div className="flex gap-3 text-sm text-muted-foreground p-4 bg-secondary/10 rounded-lg border border-dashed border-border">
              <Bot className="h-5 w-5 shrink-0 text-amber-500" />
              <div>
                <p>
                  I am your local AI Architect. Describe your application schema, triggers, or page
                  layouts, and I will generate the required manifests.
                </p>
              </div>
            </div>

            {activeSession?.messages?.map((msg, idx) => (
              <div
                key={idx}
                className={`flex gap-3 ${msg.role === 'user' ? 'flex-row-reverse' : ''}`}
              >
                <div
                  className={`h-8 w-8 rounded-full flex items-center justify-center shrink-0 ${msg.role === 'user' ? 'bg-primary text-primary-foreground' : 'bg-secondary text-secondary-foreground'}`}
                >
                  {msg.role === 'user' ? (
                    <User className="h-4 w-4" />
                  ) : (
                    <Bot className="h-4 w-4 text-amber-500" />
                  )}
                </div>
                <div
                  className={`rounded-lg p-3 max-w-[85%] text-sm whitespace-pre-wrap ${msg.role === 'user' ? 'bg-primary/10 border border-primary/20' : 'bg-background border border-border'}`}
                >
                  {msg.content}
                </div>
              </div>
            ))}

            {isThinking && (
              <div className="flex gap-3 animate-pulse">
                <div className="h-8 w-8 rounded-full bg-secondary flex items-center justify-center shrink-0">
                  <Bot className="h-4 w-4 text-amber-500" />
                </div>
                <div className="flex items-center gap-2 text-xs text-muted-foreground mt-2">
                  <RefreshCw className="h-3 w-3 animate-spin" /> Coding...
                </div>
              </div>
            )}
          </div>

          {/* Input Bar */}
          <div className="p-3 border-t border-border bg-background shrink-0 relative flex items-center gap-2">
            <Input
              value={input}
              onChange={(e: any) => setInput(e.target.value)}
              onKeyDown={(e: any) => e.key === 'Enter' && !e.shiftKey && handleSendPrompt()}
              placeholder={
                activeSession.diff_summary
                  ? 'Refine these changes or describe new ones...'
                  : 'Describe a feature or DB schema...'
              }
              className="shadow-inner bg-secondary/30 pr-12 h-10 border-transparent focus:border-primary"
              disabled={isThinking}
            />
            <Button
              size="icon"
              className="h-10 w-10 shrink-0 bg-amber-500 hover:bg-amber-600 text-black"
              onClick={handleSendPrompt}
              disabled={!input.trim() || isThinking}
            >
              <Send className="h-4.5 w-4.5" />
            </Button>
          </div>
        </div>

        {/* Right Side: Deployment & Diff Dashboard */}
        <div className="lg:col-span-4 flex flex-col h-full gap-4">
          {/* Context Stats */}
          <Card className="shrink-0">
            <CardHeader className="pb-3 border-b border-border/50 bg-secondary/10">
              <CardTitle className="text-sm">Sandbox Context</CardTitle>
            </CardHeader>
            <CardContent className="pt-4 space-y-2 text-xs text-muted-foreground">
              <div>
                <span className="font-semibold text-foreground">Sandbox ID:</span>{' '}
                <code className="font-mono text-[10px] bg-secondary/50 px-1 rounded">
                  #{sandboxId}
                </code>
              </div>
              <div>
                <span className="font-semibold text-foreground">App Name:</span>{' '}
                {activeSession.current_manifest?.app_name || 'Not initialized'}
              </div>
              {activeSession.current_manifest && (
                <div className="flex gap-2 pt-2">
                  <Badge variant="secondary">
                    {activeSession.current_manifest.collections.length} Collections
                  </Badge>
                  <Badge variant="secondary">
                    {activeSession.current_manifest.templates.length} Pages
                  </Badge>
                </div>
              )}
            </CardContent>
          </Card>

          {/* Pending Manifest Diff Accordion */}
          <Card className="flex-1 overflow-hidden flex flex-col">
            <CardHeader className="pb-3 border-b border-border/50 bg-secondary/10 shrink-0">
              <CardTitle className="text-sm flex items-center gap-2">
                <Terminal className="h-4 w-4 text-amber-500" /> Pending Changes
              </CardTitle>
            </CardHeader>
            <CardContent className="flex-1 overflow-y-auto p-4 font-mono text-xs custom-scrollbar">
              {activeSession.diff_summary ? (
                <pre className="whitespace-pre-wrap leading-relaxed text-foreground/80">
                  {activeSession.diff_summary}
                </pre>
              ) : (
                <div className="h-full flex flex-col items-center justify-center text-muted-foreground/40 italic gap-2">
                  <Code className="h-8 w-8" />
                  <span>No pending changes. Describe features in the chat to generate code.</span>
                </div>
              )}
            </CardContent>

            {activeSession.pending_manifest && (
              <div className="p-3 border-t border-border bg-background/50 flex gap-2 shrink-0">
                <Button
                  variant="outline"
                  className="flex-1 h-9 text-xs"
                  onClick={handleDiscardPending}
                  disabled={isApplying}
                >
                  Discard
                </Button>
                <Button
                  className="flex-1 h-9 text-xs bg-amber-500 hover:bg-amber-600 text-black font-semibold"
                  onClick={handleApplyPending}
                  isLoading={isApplying}
                >
                  Apply to DB
                </Button>
              </div>
            )}
          </Card>

          {/* Publishing Box */}
          <Card className="shrink-0 border-primary/20 bg-primary/5">
            <CardContent className="pt-6 space-y-3">
              <p className="text-xs text-muted-foreground leading-relaxed">
                Ready to launch? Publishing will deploy this sandbox's schemas, scripts, and
                templates directly to your Production workspace.
              </p>
              <Button
                className="w-full h-10 font-bold"
                onClick={handlePublishProject}
                disabled={!activeSession.current_manifest || isApplying}
              >
                <UploadCloud className="mr-2 h-4 w-4" /> Publish to Production
              </Button>
            </CardContent>
          </Card>
        </div>
      </div>
    );
  }

  // --- RENDER GLOBAL ROOT PROJECT LIST VIEW ---
  return (
    <div className="space-y-8 pb-20 max-w-7xl mx-auto animate-in fade-in duration-300">
      {/* Header */}
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4">
        <div className="space-y-1">
          <h1 className="text-3xl font-extrabold tracking-tight flex items-center gap-3 bg-clip-text text-transparent bg-gradient-to-r from-primary to-purple-500">
            <Sparkles className="h-8 w-8 text-primary" /> AI Architect
          </h1>
          <p className="text-muted-foreground text-sm md:text-base max-w-2xl">
            Provision structured workspace templates and build out database schemas using natural
            language.
          </p>
        </div>
      </div>

      {/* Creation form */}
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
                className="h-11 bg-background/80 border-primary/20 focus:border-primary"
                onKeyDown={(e: any) => e.key === 'Enter' && handleCreateProject()}
              />
            </div>
            <Button
              onClick={handleCreateProject}
              isLoading={isCreating}
              disabled={!newProjectName}
              className="h-11 px-8 font-semibold shadow-lg hover:shadow-primary/25 transition-all"
            >
              Start Project
            </Button>
          </div>
        </div>

        {/* Advanced Options */}
        <div className="w-full border-t border-border/50 pt-4 mt-2 flex flex-col md:flex-row items-center gap-4 px-4 pb-2">
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
              Initial Database State
            </label>
            <Select
              value={cloneStrategy}
              onChange={(e: any) => setCloneStrategy(e.target.value)}
              className="h-9 bg-background/80 border-border/50"
            >
              <option value="none">Empty Sandbox</option>
              <option value="schema">Clone Schema Only</option>
              <option value="partial">Clone Schema + partial records</option>
              <option value="full">Full Clone</option>
            </Select>
          </div>
          <div
            className={`w-full md:w-1/3 transition-opacity duration-300 ${cloneStrategy === 'partial' ? 'opacity-100' : 'opacity-30 pointer-events-none'}`}
          >
            <label className="text-xs font-semibold text-muted-foreground mb-1 block">
              Record Limit (Per Table)
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

      {/* Grid listing */}
      {loading ? (
        <div className="flex flex-col items-center justify-center py-20 gap-4 text-muted-foreground">
          <Loader2 className="h-10 w-10 animate-spin text-primary" />
          <p>Loading projects list...</p>
        </div>
      ) : !Array.isArray(sessions) || sessions.length === 0 ? ( // [FIXED] Guard empty states
        <div className="flex flex-col items-center justify-center py-20 text-center border-2 border-dashed border-border rounded-xl bg-secondary/5">
          <Bot className="h-10 w-10 text-muted-foreground/50 mb-4" />
          <h3 className="text-xl font-semibold mb-2">No Projects Found</h3>
          <p className="text-muted-foreground max-w-sm">
            Create a project using the creation field above to activate the AI Architect.
          </p>
        </div>
      ) : (
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-6">
          {sessions.map(
            (
              session // [FIXED] Safe map execution
            ) => (
              <div
                key={session.id}
                onClick={() => {
                  window.location.href = `/_dashboard/sandbox/${session.id}/ai-architect`;
                }}
                className="group relative bg-card border border-border rounded-xl p-5 cursor-pointer hover:border-primary/50 hover:shadow-lg hover:-translate-y-1 transition-all duration-200 flex flex-col h-full justify-between"
              >
                <div>
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
                </div>

                <div className="mt-4 pt-4 border-t border-border flex justify-between items-center">
                  {session.current_manifest ? (
                    <div className="flex gap-2">
                      <Badge variant="secondary">
                        {session.current_manifest.collections.length} Cols
                      </Badge>
                      <Badge variant="secondary">
                        {session.current_manifest.templates.length} Pages
                      </Badge>
                    </div>
                  ) : (
                    <span className="text-xs text-muted-foreground italic">
                      Container setup pending...
                    </span>
                  )}

                  <span className="text-[10px] font-mono text-muted-foreground">
                    #{session.id.substring(0, 8)}
                  </span>
                </div>
              </div>
            )
          )}
        </div>
      )}
    </div>
  );
};
// =========================== apex-kit/admin-ui/src/features/ai/pages/AiArchitectPage.tsx ends here ===========================
