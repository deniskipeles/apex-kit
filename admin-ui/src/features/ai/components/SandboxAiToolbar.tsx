import React, { useState, useEffect, useRef } from 'react';
import {
  Sparkles,
  Play,
  Database,
  RefreshCw,
  X,
  Send,
  LayoutTemplate,
  Terminal,
  Zap,
  ExternalLink,
  Paperclip,
  FileCode,
  Check,
  AlertTriangle,
  MessageSquare,
  Rocket,
  Maximize2,
} from 'lucide-react';
import { Button, Input, Select, Badge, Textarea } from '../../../components/ui/Elements';
import { architectService } from '../services/architectService';
import { templatesService } from '../../templates/services/templatesService';
import { collectionsService } from '../../collections/services/collectionsService';
import { scriptsService } from '../../scripts/services/scriptsService';
import { useToast } from '../../../components/feedback/Toast';
import { APP_CONFIG } from '../../../config/app.config';
import { AI_MODELS, DEFAULT_AI_MODEL } from '../../../config/ai-models';
import { Overlay } from '../../../components/overlay/Overlay';
import { apiClient } from '../../../lib/apiClient';
import { AiSession, ChatMessage, Script } from '@/src/types';

interface SandboxAiToolbarProps {
  sessionId: string;
}

export const SandboxAiToolbar = ({ sessionId }: SandboxAiToolbarProps) => {
  const [isOpen, setIsOpen] = useState(false);
  const [activeTab, setActiveTab] = useState<'chat' | 'preview' | 'tools'>('chat');
  const [session, setSession] = useState<AiSession | null>(null);
  const [input, setInput] = useState('');
  const [isThinking, setIsThinking] = useState(false);
  const [isApplying, setIsApplying] = useState(false);
  const [previewUrl, setPreviewUrl] = useState<string | null>(null);

  const [templates, setTemplates] = useState<any[]>([]);
  const [collections, setCollections] = useState<any[]>([]);
  const [scripts, setScripts] = useState<{
    local: Script[];
    shared: Script[];
    root_total: number;
    transparency_enabled: boolean;
  }>({ local: [], root_total: 0, shared: [], transparency_enabled: false });

  const [isAttachOpen, setIsAttachOpen] = useState(false);
  const attachBtnRef = useRef<HTMLButtonElement>(null);
  const [selectedModel, setSelectedModel] = useState(DEFAULT_AI_MODEL);

  const chatEndRef = useRef<HTMLDivElement>(null);
  const { toast } = useToast();

  const refreshResources = async () => {
    try {
      const [tmpls, cols, scrs] = await Promise.all([
        templatesService.list(),
        collectionsService.list(),
        scriptsService.list(),
      ]);
      setTemplates(tmpls);
      setCollections(cols);
      setScripts(scrs);
      return tmpls;
    } catch (e) {
      console.error('Failed to load resources', e);
      return [];
    }
  };

  useEffect(() => {
    if (!sessionId) return;
    apiClient.ai
      .getSession()
      .then((currentSession) => {
        setSession(currentSession);
      })
      .catch((err) => console.error('Failed to load session:', err));
    refreshResources();
  }, [sessionId]);

  useEffect(() => {
    if (activeTab === 'chat' && isOpen) {
      chatEndRef.current?.scrollIntoView({ behavior: 'smooth' });
    }
  }, [session?.messages, activeTab, session?.diff_summary, isOpen]);

  const handleSend = async () => {
    if (!input.trim() || !session) return;
    const userMsg = input;
    setInput('');
    setIsThinking(true);

    const newMsg: ChatMessage = { role: 'user', content: userMsg };
    setSession((prev) => (prev ? { ...prev, messages: [...(prev.messages || []), newMsg] } : null));

    try {
      const updated = await architectService.chat(userMsg, selectedModel);
      setSession(updated);

      if (updated.diff_summary) {
        toast('Changes drafted. Please review and apply.', 'info');
      } else if (updated.last_error) {
        toast('AI encountered an error. Check chat.', 'error');
      }
    } catch (e: any) {
      toast(e.message, 'error');
    } finally {
      setIsThinking(false);
    }
  };

  const handleApply = async () => {
    if (!session) return;
    setIsApplying(true);
    try {
      const updated = await architectService.applySessionChanges();
      setSession(updated);
      toast('Changes applied to Sandbox!', 'success');
      await refreshResources();
    } catch (e) {
      toast('Failed to apply changes', 'error');
    } finally {
      setIsApplying(false);
    }
  };

  const handlePublish = async () => {
    setIsApplying(true);
    try {
      await apiClient.root.publishSandbox(sessionId);
      toast('Sandbox successfully merged into Production!', 'success');
      window.location.href = '/_dashboard/sandboxes';
    } catch (e: any) {
      toast('Failed to publish sandbox', 'error');
    } finally {
      setIsApplying(false);
    }
  };

  const handleAttach = async (type: 'collection' | 'script' | 'template', id: string | number) => {
    setIsAttachOpen(false);
    let contentToAttach = '';
    let name = '';
    try {
      if (type === 'collection') {
        const col = collections.find((c) => c.id === id);
        if (col) {
          name = col.name;
          contentToAttach = JSON.stringify(
            { name: col.name, schema: col.schema, type: col.type },
            null,
            2
          );
        }
      } else if (type === 'script') {
        const scr = scripts.local.find((s) => s.id === id);
        if (scr) {
          name = scr.name;
          contentToAttach = `// Script: ${scr.name}\\n// Trigger: ${scr.trigger_type}\\n${scr.code}`;
        }
      } else if (type === 'template') {
        const t = templates.find((t) => t.id === id);
        if (t) {
          name = t.slug;
          contentToAttach = `<!-- Template: ${t.slug} -->\\n${t.content}`;
        }
      }
      if (contentToAttach) {
        setInput(
          (prev) =>
            `${prev}\\n\\n[Attached ${type}: ${name}]\\n\`\`\`json\\n${contentToAttach}\\n\`\`\`\\n\\n`
        );
        toast(`Attached ${name}`, 'success');
      }
    } catch (e) {
      toast('Failed to attach item', 'error');
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

  if (!sessionId) return null;

  return (
    <>
      {/* Floating Action Button when closed */}
      {!isOpen && (
        <button
          onClick={() => setIsOpen(true)}
          className="fixed bottom-6 right-6 z-[90] w-14 h-14 rounded-full bg-primary text-white shadow-2xl flex items-center justify-center hover:scale-110 transition-transform duration-300 group ring-4 ring-primary/20"
        >
          <Sparkles className="h-6 w-6 group-hover:animate-pulse" />
          {session?.diff_summary && (
            <div className="absolute -top-1 -right-1 w-4 h-4 bg-amber-500 rounded-full border-2 border-background"></div>
          )}
        </button>
      )}

      {/* Slide-out Right Panel */}
      <div
        className={`fixed inset-y-0 right-0 z-[100] flex flex-col bg-background/95 supports-[backdrop-filter]:bg-background/80 backdrop-blur-2xl border-l border-border/50 shadow-2xl transition-transform duration-300 w-full sm:w-[450px] lg:w-[500px] ${isOpen ? 'translate-x-0' : 'translate-x-full'}`}
      >
        {/* Header */}
        <div className="flex flex-col shrink-0 border-b border-border bg-secondary/10">
          <div className="flex items-center justify-between p-4">
            <div className="flex items-center gap-2 font-bold text-foreground">
              <Sparkles className="h-5 w-5 text-primary" /> Copilot
              <Badge
                variant="outline"
                className="text-[10px] uppercase ml-2 bg-background shadow-sm text-muted-foreground font-mono"
              >
                {sessionId.substring(0, 8)}
              </Badge>
            </div>
            <div className="flex items-center gap-2">
              <Button
                size="sm"
                variant="outline"
                className="h-8 text-xs font-semibold bg-background"
                onClick={handlePublish}
                disabled={isApplying}
              >
                <Rocket className="h-3 w-3 mr-1.5 text-primary" /> Publish
              </Button>
              <Button
                variant="ghost"
                size="icon"
                className="h-8 w-8 text-muted-foreground hover:text-foreground"
                onClick={() => setIsOpen(false)}
              >
                <X className="h-4 w-4" />
              </Button>
            </div>
          </div>
          {/* Tabs */}
          <div className="flex px-4 gap-4 text-sm font-medium border-t border-border/50">
            <button
              onClick={() => setActiveTab('chat')}
              className={`py-2.5 border-b-2 transition-colors flex items-center gap-1.5 ${activeTab === 'chat' ? 'border-primary text-foreground' : 'border-transparent text-muted-foreground hover:text-foreground'}`}
            >
              <MessageSquare className="h-3.5 w-3.5" /> Chat
            </button>
            <button
              onClick={() => {
                setActiveTab('preview');
                if (!previewUrl && templates.length > 0) {
                  const home =
                    templates.find((t) => t.slug.includes('index') || t.slug.includes('home')) ||
                    templates[0];
                  setPreviewUrl(`/sandbox/${sessionId}/render/${home.slug}`);
                }
              }}
              className={`py-2.5 border-b-2 transition-colors flex items-center gap-1.5 ${activeTab === 'preview' ? 'border-primary text-foreground' : 'border-transparent text-muted-foreground hover:text-foreground'}`}
            >
              <LayoutTemplate className="h-3.5 w-3.5" /> Preview
            </button>
            <button
              onClick={() => setActiveTab('tools')}
              className={`py-2.5 border-b-2 transition-colors flex items-center gap-1.5 ${activeTab === 'tools' ? 'border-primary text-foreground' : 'border-transparent text-muted-foreground hover:text-foreground'}`}
            >
              <Terminal className="h-3.5 w-3.5" /> API & Tools
            </button>
          </div>
        </div>

        {/* --- CHAT TAB --- */}
        {activeTab === 'chat' && (
          <>
            <div className="flex-1 overflow-y-auto p-4 space-y-5 custom-scrollbar">
              {(session?.messages || []).map((msg, i) => (
                <div
                  key={i}
                  className={`flex gap-3 w-full ${msg.role === 'user' ? 'justify-end' : 'justify-start'}`}
                >
                  {msg.role !== 'user' && (
                    <div className="h-7 w-7 rounded-full bg-primary/10 flex items-center justify-center shrink-0 border border-primary/20">
                      <Sparkles className="h-3.5 w-3.5 text-primary" />
                    </div>
                  )}
                  <div
                    className={`p-3 text-sm whitespace-pre-wrap leading-relaxed shadow-sm max-w-[85%] ${msg.role === 'user' ? 'bg-primary text-primary-foreground rounded-2xl rounded-tr-sm' : 'bg-card border border-border text-foreground rounded-2xl rounded-tl-sm'} ${msg.role === 'error' ? 'bg-destructive/10 text-destructive border-destructive/20' : ''}`}
                  >
                    {msg.content}
                  </div>
                </div>
              ))}

              {/* Pending Diff Box */}
              {session?.diff_summary && (
                <div className="border border-amber-500/30 bg-[#0d1117] rounded-xl overflow-hidden animate-in fade-in slide-in-from-bottom-2 shadow-lg">
                  <div className="bg-amber-500/10 px-3 py-2 border-b border-amber-500/20 flex justify-between items-center">
                    <span className="text-xs font-bold text-amber-500 flex items-center gap-2">
                      <AlertTriangle className="h-3 w-3" /> Draft Changes Generated
                    </span>
                  </div>
                  <div className="p-3 font-mono text-[10px] sm:text-xs">
                    <div className="space-y-0.5">
                      {session.diff_summary.split('\\n').map((line, i) => renderDiffLine(line, i))}
                    </div>
                  </div>
                  <div className="p-2 bg-background flex gap-2 justify-end border-t border-border">
                    <Button
                      size="sm"
                      variant="ghost"
                      onClick={() =>
                        setSession({ ...session, diff_summary: null, pending_manifest: null })
                      }
                      className="h-7 text-xs"
                    >
                      Discard
                    </Button>
                    <Button
                      size="sm"
                      onClick={handleApply}
                      isLoading={isApplying}
                      className="h-7 text-xs bg-amber-500 hover:bg-amber-600 text-black font-semibold"
                    >
                      <Check className="h-3 w-3 mr-1" /> Apply to Sandbox
                    </Button>
                  </div>
                </div>
              )}

              {isThinking && (
                <div className="flex w-full justify-start animate-in fade-in slide-in-from-bottom-2">
                  <div className="flex gap-3 max-w-[85%]">
                    <div className="h-7 w-7 rounded-full bg-secondary border border-border flex items-center justify-center shrink-0">
                      <Sparkles className="h-3.5 w-3.5 text-primary animate-pulse" />
                    </div>
                    <div className="p-3 bg-card border border-border rounded-2xl rounded-tl-sm flex items-center gap-2 shadow-sm">
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
                    </div>
                  </div>
                </div>
              )}
              <div ref={chatEndRef} />
            </div>

            {/* Input Area */}
            <div className="p-4 bg-background border-t border-border shrink-0">
              <div className="flex justify-between items-center mb-2 px-1">
                <span className="text-[10px] text-muted-foreground font-semibold uppercase tracking-wider">
                  Model
                </span>
                <Select
                  value={selectedModel}
                  onChange={(e: any) => setSelectedModel(e.target.value)}
                  className="h-6 text-[10px] py-0 w-32 border-transparent bg-secondary/50 focus:ring-0"
                >
                  {AI_MODELS?.map((m) => (
                    <option key={m.value} value={m.value}>
                      {m.label}
                    </option>
                  ))}
                </Select>
              </div>
              <div className="relative flex items-end gap-2 bg-secondary/10 border border-input rounded-xl p-2 focus-within:ring-1 focus-within:ring-primary focus-within:border-primary transition-all">
                <Button
                  ref={attachBtnRef}
                  variant="ghost"
                  size="icon"
                  className="shrink-0 h-8 w-8 text-muted-foreground hover:text-primary mb-0.5"
                  onClick={() => setIsAttachOpen(!isAttachOpen)}
                  title="Attach Context"
                >
                  <Paperclip className="h-4 w-4" />
                </Button>
                <Textarea
                  value={input}
                  onChange={(e: any) => setInput(e.target.value)}
                  onKeyDown={(e: any) => {
                    if (e.key === 'Enter' && !e.shiftKey) {
                      e.preventDefault();
                      handleSend();
                    }
                  }}
                  placeholder="Describe a feature, schema change, or UI..."
                  className="flex-1 bg-transparent border-0 focus-visible:ring-0 resize-none min-h-[40px] max-h-[150px] py-2 text-sm custom-scrollbar"
                  disabled={isThinking}
                />
                <Button
                  size="icon"
                  className="h-8 w-8 shrink-0 rounded-lg mb-0.5"
                  onClick={handleSend}
                  disabled={!input.trim() || isThinking || isApplying}
                >
                  <Send className="h-3.5 w-3.5" />
                </Button>

                {/* Attachment Overlay */}
                <Overlay
                  isOpen={isAttachOpen}
                  onClose={() => setIsAttachOpen(false)}
                  anchorRef={attachBtnRef}
                  align="start"
                  className="mb-2 z-[110]"
                >
                  <div className="w-64 bg-popover border border-border rounded-xl shadow-xl overflow-hidden flex flex-col max-h-[300px]">
                    <div className="p-2 bg-secondary/20 text-[10px] font-bold text-muted-foreground uppercase tracking-wider border-b border-border">
                      Attach Context
                    </div>
                    <div className="overflow-y-auto flex-1 p-1 custom-scrollbar">
                      <div className="px-2 py-1 text-[10px] text-muted-foreground font-semibold">
                        Collections
                      </div>
                      {(collections || [])?.map((c) => (
                        <button
                          key={c.id}
                          onClick={() => handleAttach('collection', c.id)}
                          className="w-full text-left px-2 py-1.5 text-xs hover:bg-accent rounded flex items-center gap-2"
                        >
                          <Database className="h-3 w-3 text-blue-500" /> {c.name}
                        </button>
                      ))}
                      <div className="px-2 py-1 text-[10px] text-muted-foreground font-semibold mt-1">
                        Templates
                      </div>
                      {(templates || [])?.map((t) => (
                        <button
                          key={t.id}
                          onClick={() => handleAttach('template', t.id)}
                          className="w-full text-left px-2 py-1.5 text-xs hover:bg-accent rounded flex items-center gap-2"
                        >
                          <LayoutTemplate className="h-3 w-3 text-purple-500" /> {t.slug}
                        </button>
                      ))}
                      <div className="px-2 py-1 text-[10px] text-muted-foreground font-semibold mt-1">
                        Scripts
                      </div>
                      {(scripts.local || [])?.map((s) => (
                        <button
                          key={s.id}
                          onClick={() => handleAttach('script', s.id)}
                          className="w-full text-left px-2 py-1.5 text-xs hover:bg-accent rounded flex items-center gap-2"
                        >
                          <FileCode className="h-3 w-3 text-yellow-500" /> {s.name}
                        </button>
                      ))}
                    </div>
                  </div>
                </Overlay>
              </div>
            </div>
          </>
        )}

        {/* --- PREVIEW TAB --- */}
        {activeTab === 'preview' && (
          <div className="flex-1 flex flex-col overflow-hidden bg-background">
            <div className="p-2 border-b border-border flex gap-2 items-center bg-secondary/10 shrink-0">
              <div className="flex-1 flex gap-2 overflow-x-auto no-scrollbar items-center px-1">
                {templates.length === 0 ? (
                  <span className="text-xs text-muted-foreground italic flex items-center gap-2">
                    <Terminal className="h-3 w-3" /> No templates available.
                  </span>
                ) : (
                  templates.map((t) => (
                    <button
                      key={t.id}
                      onClick={() => setPreviewUrl(`/sandbox/${sessionId}/render/${t.slug}`)}
                      className={`px-3 py-1 text-xs rounded-full whitespace-nowrap transition-colors flex items-center gap-1.5 ${previewUrl?.endsWith(t.slug) ? 'bg-primary text-primary-foreground shadow-sm' : 'bg-background hover:bg-secondary border border-border text-muted-foreground'}`}
                    >
                      <LayoutTemplate className="h-3 w-3 opacity-70" /> {t.slug}
                    </button>
                  ))
                )}
              </div>
              <div className="flex gap-1 border-l border-border/50 pl-2">
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-7 w-7"
                  onClick={() => refreshResources()}
                  title="Refresh Templates"
                >
                  <RefreshCw className="h-3.5 w-3.5" />
                </Button>
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-7 w-7"
                  onClick={() => window.open(previewUrl || '', '_blank')}
                  disabled={!previewUrl}
                  title="Open in New Tab"
                >
                  <ExternalLink className="h-3.5 w-3.5" />
                </Button>
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-7 w-7 hidden sm:flex"
                  onClick={() => {
                    window.location.href = `/_dashboard/sandbox/${sessionId}/ai-architect`;
                    setIsOpen(false);
                  }}
                  title="Expand IDE"
                >
                  <Maximize2 className="h-3.5 w-3.5" />
                </Button>
              </div>
            </div>
            <div className="flex-1 bg-white relative">
              {previewUrl ? (
                <iframe
                  src={previewUrl}
                  className="w-full h-full border-none bg-white"
                  title="Preview"
                />
              ) : (
                <div className="flex flex-col h-full items-center justify-center text-gray-400 gap-2">
                  <LayoutTemplate className="h-10 w-10 opacity-20" />
                  <span className="text-sm">Select a template to preview</span>
                </div>
              )}
            </div>
          </div>
        )}

        {/* --- TOOLS TAB --- */}
        {activeTab === 'tools' && (
          <div className="flex-1 p-6 space-y-4">
            <Button
              variant="outline"
              className="w-full justify-start gap-3 h-12"
              onClick={() =>
                window.open(`${APP_CONFIG.apiBaseUrl}/sandbox/${sessionId}/scalar`, '_blank')
              }
            >
              <div className="p-2 bg-blue-500/10 rounded text-blue-500">
                <Terminal className="h-4 w-4" />
              </div>
              <div className="text-left">
                <div className="font-bold">API Documentation</div>
                <div className="text-[10px] text-muted-foreground">
                  Interactive Swagger/Scalar API Reference
                </div>
              </div>
            </Button>
            <Button
              variant="outline"
              className="w-full justify-start gap-3 h-12"
              onClick={async () => {
                try {
                  await apiClient.reIndex();
                  toast('Search index re-generation started', 'success');
                } catch {
                  toast('Failed to trigger re-index', 'error');
                }
              }}
            >
              <div className="p-2 bg-amber-500/10 rounded text-amber-500">
                <Zap className="h-4 w-4" />
              </div>
              <div className="text-left">
                <div className="font-bold">Rebuild Search Index</div>
                <div className="text-[10px] text-muted-foreground">
                  Fix missing records in OSE/Tantivy search
                </div>
              </div>
            </Button>
            <Button
              variant="outline"
              className="w-full justify-start gap-3 h-12"
              onClick={() => {
                window.location.href = `/_dashboard/sandbox/${sessionId}/ai-architect`;
                setIsOpen(false);
              }}
            >
              <div className="p-2 bg-purple-500/10 rounded text-purple-500">
                <Maximize2 className="h-4 w-4" />
              </div>
              <div className="text-left">
                <div className="font-bold">Open Full IDE</div>
                <div className="text-[10px] text-muted-foreground">
                  Expand Architect into full-screen mode
                </div>
              </div>
            </Button>
          </div>
        )}
      </div>
    </>
  );
};
