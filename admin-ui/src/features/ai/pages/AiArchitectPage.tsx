import React, { useState, useEffect, useRef, useCallback } from 'react';
import {
  Sparkles,
  Loader2,
  Bot,
  Send,
  Code,
  User,
  Terminal,
  Wand2,
  Rocket,
  Check,
  AlertTriangle,
  Database,
} from 'lucide-react';
import {
  Button,
  Card,
  CardContent,
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

export const AiArchitectPage = () => {
  const [activeSession, setActiveSession] = useState<AiSession | null>(null);
  const [loading, setLoading] = useState(true);
  const [selectedModel, setSelectedModel] = useState(DEFAULT_AI_MODEL);

  const [input, setInput] = useState('');
  const [isThinking, setIsThinking] = useState(false);
  const [isApplying, setIsApplying] = useState(false);

  const scrollRef = useRef<HTMLDivElement>(null);
  const { toast } = useToast();

  const isSandbox = apiClient.getScope().type === 'sandbox';
  const sandboxId = apiClient.getScope().id;

  const loadData = useCallback(async () => {
    if (!isSandbox) {
      toast('AI Architect IDE is only available inside a Sandbox context.', 'error');
      window.location.href = '/_dashboard';
      return;
    }

    setLoading(true);
    try {
      const active = await apiClient.ai.getSession();
      setActiveSession(active);
    } catch (e: any) {
      toast('Failed to load sandbox AI session.', 'error');
    } finally {
      setLoading(false);
    }
  }, [isSandbox, toast]);

  useEffect(() => {
    loadData();
  }, [loadData]);

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [activeSession?.messages, isThinking]);

  const handleSendPrompt = async () => {
    if (!input.trim() || !activeSession) return;
    const promptText = input;
    setInput('');
    setIsThinking(true);

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
      toast('Sandbox successfully merged and published to Production!', 'success');
      window.location.href = '/_dashboard';
    } catch (e: any) {
      toast('Failed to publish project', 'error');
    } finally {
      setIsApplying(false);
    }
  };

  const renderDiffLine = (line: string, i: number) => {
    if (line.startsWith('+'))
      return (
        <div
          key={i}
          className="text-emerald-400 bg-emerald-500/10 px-2 py-0.5 rounded my-0.5 border border-emerald-500/20"
        >
          {line}
        </div>
      );
    if (line.startsWith('~'))
      return (
        <div
          key={i}
          className="text-amber-400 bg-amber-500/10 px-2 py-0.5 rounded my-0.5 border border-amber-500/20"
        >
          {line}
        </div>
      );
    if (line.startsWith('-'))
      return (
        <div
          key={i}
          className="text-red-400 bg-red-500/10 px-2 py-0.5 rounded my-0.5 border border-red-500/20"
        >
          {line}
        </div>
      );
    return (
      <div key={i} className="px-2 py-0.5 opacity-70">
        {line}
      </div>
    );
  };

  if (loading || !activeSession) {
    return (
      <div className="flex h-[80vh] items-center justify-center flex-col gap-4">
        <div className="relative">
          <div className="absolute inset-0 bg-primary/20 blur-xl rounded-full animate-pulse"></div>
          <Loader2 className="animate-spin text-primary h-12 w-12 relative z-10" />
        </div>
        <p className="text-muted-foreground animate-pulse text-sm font-medium">
          Connecting to AI Copilot...
        </p>
      </div>
    );
  }

  return (
    <div className="grid grid-cols-1 lg:grid-cols-12 gap-6 h-[calc(100vh-140px)] animate-in fade-in duration-500 max-w-[1600px] mx-auto">
      {/* Left Side: Immersive Chat Column */}
      <div className="lg:col-span-7 xl:col-span-8 flex flex-col h-full bg-card/80 backdrop-blur-xl border border-border rounded-2xl overflow-hidden shadow-xl">
        <div className="p-4 border-b border-border/50 bg-secondary/5 flex items-center justify-between shrink-0">
          <div className="flex items-center gap-3">
            <div className="p-2 bg-primary/10 rounded-lg">
              <Wand2 className="h-5 w-5 text-primary" />
            </div>
            <div>
              <span className="font-bold text-sm block">Architect IDE</span>
              <span className="text-[10px] text-muted-foreground uppercase tracking-widest font-mono">
                Sandbox #{sandboxId.substring(0, 8)}
              </span>
            </div>
          </div>
          <div className="w-48 hidden sm:block">
            <Select
              value={selectedModel}
              onChange={(e: any) => setSelectedModel(e.target.value)}
              className="h-8 text-xs bg-background/50 border-transparent hover:border-border transition-colors focus:ring-0 shadow-none"
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
        <div
          className="flex-1 overflow-y-auto p-4 sm:p-6 space-y-6 custom-scrollbar"
          ref={scrollRef}
        >
          {activeSession?.messages.length === 0 && !isThinking ? (
            <div className="h-full flex flex-col items-center justify-center text-center space-y-4 opacity-70">
              <Bot className="h-16 w-16 text-muted-foreground/30" />
              <div className="space-y-1">
                <h3 className="font-semibold text-lg text-foreground">What are we building?</h3>
                <p className="text-sm text-muted-foreground max-w-sm">
                  Describe your database schema, server-side logic, or UI templates, and I will
                  generate the manifests securely in this isolated sandbox.
                </p>
              </div>
            </div>
          ) : null}

          {activeSession?.messages?.map((msg, idx) => (
            <div
              key={idx}
              className={`flex w-full ${msg.role === 'user' ? 'justify-end' : 'justify-start'} animate-in fade-in slide-in-from-bottom-2 duration-300`}
            >
              <div
                className={`flex gap-3 max-w-[85%] md:max-w-[75%] ${msg.role === 'user' ? 'flex-row-reverse' : ''}`}
              >
                <div
                  className={`h-8 w-8 rounded-full flex items-center justify-center shrink-0 shadow-sm ${msg.role === 'user' ? 'bg-primary text-primary-foreground' : 'bg-secondary text-secondary-foreground border border-border'}`}
                >
                  {msg.role === 'user' ? (
                    <User className="h-4 w-4" />
                  ) : (
                    <Sparkles className="h-4 w-4 text-primary" />
                  )}
                </div>
                <div
                  className={`p-4 text-sm whitespace-pre-wrap shadow-sm leading-relaxed ${
                    msg.role === 'user'
                      ? 'bg-primary text-primary-foreground rounded-2xl rounded-tr-sm'
                      : 'bg-background border border-border text-foreground rounded-2xl rounded-tl-sm'
                  }`}
                >
                  {msg.content}
                </div>
              </div>
            </div>
          ))}

          {isThinking && (
            <div className="flex w-full justify-start animate-in fade-in slide-in-from-bottom-2 duration-300">
              <div className="flex gap-3 max-w-[85%]">
                <div className="h-8 w-8 rounded-full bg-secondary border border-border flex items-center justify-center shrink-0">
                  <Sparkles className="h-4 w-4 text-primary animate-pulse" />
                </div>
                <div className="p-4 bg-background border border-border rounded-2xl rounded-tl-sm flex items-center gap-3 shadow-sm">
                  <div className="flex gap-1">
                    <span
                      className="w-1.5 h-1.5 bg-primary/60 rounded-full animate-bounce"
                      style={{ animationDelay: '0ms' }}
                    ></span>
                    <span
                      className="w-1.5 h-1.5 bg-primary/60 rounded-full animate-bounce"
                      style={{ animationDelay: '150ms' }}
                    ></span>
                    <span
                      className="w-1.5 h-1.5 bg-primary/60 rounded-full animate-bounce"
                      style={{ animationDelay: '300ms' }}
                    ></span>
                  </div>
                  <span className="text-xs text-muted-foreground font-medium uppercase tracking-widest">
                    Architect is designing...
                  </span>
                </div>
              </div>
            </div>
          )}
        </div>

        {/* Input Bar */}
        <div className="p-4 bg-background/50 backdrop-blur-md shrink-0 border-t border-border/50">
          <div className="relative group flex items-end gap-2 bg-background border border-input rounded-2xl p-2 shadow-sm focus-within:ring-1 focus-within:ring-primary focus-within:border-primary transition-all">
            <Textarea
              value={input}
              onChange={(e: any) => setInput(e.target.value)}
              onKeyDown={(e: any) => {
                if (e.key === 'Enter' && !e.shiftKey) {
                  e.preventDefault();
                  handleSendPrompt();
                }
              }}
              placeholder={
                activeSession.diff_summary
                  ? 'Refine these changes or describe new ones...'
                  : 'Describe a feature, schema change, or UI tweak...'
              }
              className="flex-1 bg-transparent border-0 focus-visible:ring-0 resize-none min-h-[44px] max-h-[200px] py-3 text-sm custom-scrollbar"
              disabled={isThinking}
            />
            <Button
              size="icon"
              className="h-10 w-10 shrink-0 rounded-xl mb-1 mr-1"
              onClick={handleSendPrompt}
              disabled={!input.trim() || isThinking}
            >
              <Send className="h-4 w-4" />
            </Button>
          </div>
        </div>
      </div>

      {/* Right Side: Deployment & Diff Dashboard */}
      <div className="lg:col-span-5 xl:col-span-4 flex flex-col h-full gap-4">
        {/* Pending Manifest Diff Accordion */}
        <Card className="flex-1 overflow-hidden flex flex-col border-primary/20 shadow-lg">
          <CardHeader className="pb-3 border-b border-border/50 bg-secondary/5 shrink-0">
            <CardTitle className="text-sm flex items-center justify-between">
              <div className="flex items-center gap-2">
                <Terminal className="h-4 w-4 text-primary" />
                Pending Changes
              </div>
              {activeSession.diff_summary && (
                <Badge variant="warning" className="h-5 animate-pulse">
                  Draft
                </Badge>
              )}
            </CardTitle>
          </CardHeader>
          <CardContent className="flex-1 overflow-y-auto p-4 font-mono text-xs custom-scrollbar bg-[#0d1117] text-[#c9d1d9] m-3 rounded-lg border border-white/10 shadow-inner">
            {activeSession.diff_summary ? (
              <div className="space-y-0.5">
                {activeSession.diff_summary.split('\n').map((line, i) => renderDiffLine(line, i))}
              </div>
            ) : (
              <div className="h-full flex flex-col items-center justify-center text-muted-foreground/40 italic gap-3">
                <Code className="h-10 w-10 opacity-20" />
                <span className="text-center px-4">
                  No pending changes. Describe features in the chat to generate code.
                </span>
              </div>
            )}
          </CardContent>

          {activeSession.pending_manifest && (
            <div className="p-3 border-t border-border/50 bg-secondary/5 flex gap-2 shrink-0">
              <Button
                variant="outline"
                className="flex-1 h-10 text-xs"
                onClick={handleDiscardPending}
                disabled={isApplying}
              >
                Discard
              </Button>
              <Button
                className="flex-[2] h-10 text-xs font-semibold shadow-md"
                onClick={handleApplyPending}
                isLoading={isApplying}
              >
                <Database className="h-4 w-4 mr-2" /> Apply to Sandbox
              </Button>
            </div>
          )}
        </Card>

        {/* Publishing Box */}
        <Card className="shrink-0 bg-gradient-to-br from-primary/10 via-background to-secondary/20 border-primary/30 relative overflow-hidden">
          <div className="absolute top-0 right-0 p-6 opacity-10 pointer-events-none">
            <Rocket size={100} />
          </div>
          <CardContent className="pt-6 space-y-4 relative z-10">
            <div>
              <h3 className="font-bold text-foreground flex items-center gap-2 mb-1">
                <Rocket className="h-4 w-4 text-primary" /> Ready to Launch?
              </h3>
              <p className="text-xs text-muted-foreground leading-relaxed">
                Publishing will merge this sandbox's schemas, scripts, and templates directly to
                Production.
              </p>
            </div>
            {/* [FIXED] Removed the `!activeSession.current_manifest` check. The backend now calculates the manifest dynamically! */}
            <Button
              className="w-full h-11 font-bold shadow-lg shadow-primary/20 hover:scale-[1.02] transition-transform"
              onClick={handlePublishProject}
              disabled={isApplying}
            >
              <Check className="mr-2 h-4 w-4" /> Merge to Production
            </Button>
          </CardContent>
        </Card>
      </div>
    </div>
  );
};
