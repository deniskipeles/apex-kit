import React, { useState, useEffect, useRef } from 'react';
import {
  Sparkles,
  Play,
  Database,
  RefreshCw,
  Maximize2,
  X,
  Send,
  ChevronDown,
  LayoutTemplate,
  Terminal,
  Zap,
  ExternalLink,
  Paperclip,
  FileCode,
  FileJson,
  Check,
  AlertTriangle,
} from 'lucide-react';
import { Button, Input, Select, Badge, Card } from '../../../components/ui/Elements';
import { architectService, AiSession, ChatMessage } from '../services/architectService';
import { templatesService } from '../../templates/services/templatesService';
import { collectionsService } from '../../collections/services/collectionsService';
import { scriptsService } from '../../scripts/services/scriptsService';
import { useToast } from '../../../components/feedback/Toast';
import { APP_CONFIG } from '../../../config/app.config';
import { AI_MODELS, DEFAULT_AI_MODEL } from '../../../config/ai-models';
import { Overlay } from '../../../components/overlay/Overlay';

interface SandboxAiToolbarProps {
  sessionId: string;
}

export const SandboxAiToolbar = ({ sessionId }: SandboxAiToolbarProps) => {
  const [isOpen, setIsOpen] = useState(true);
  const [activeTab, setActiveTab] = useState<'chat' | 'preview' | 'tools' | null>(null);
  const [session, setSession] = useState<AiSession | null>(null);
  const [input, setInput] = useState('');
  const [isThinking, setIsThinking] = useState(false);
  const [isApplying, setIsApplying] = useState(false);
  const [previewUrl, setPreviewUrl] = useState<string | null>(null);

  // Resource Lists for Attachments
  const [templates, setTemplates] = useState<any[]>([]);
  const [collections, setCollections] = useState<any[]>([]);
  const [scripts, setScripts] = useState<any[]>([]);

  // Attachment Menu State
  const [isAttachOpen, setIsAttachOpen] = useState(false);
  const attachBtnRef = useRef<HTMLButtonElement>(null);

  // Model State
  const [selectedModel, setSelectedModel] = useState(DEFAULT_AI_MODEL);

  const chatEndRef = useRef<HTMLDivElement>(null);
  const { toast } = useToast();

  // Helper to refresh resources
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

  // Initial Load
  useEffect(() => {
    if (!sessionId) return;

    architectService.listSessions().then((sessions) => {
      const current = sessions.find((s) => s.id === sessionId);
      if (current) setSession(current);
    });

    refreshResources();
  }, [sessionId]);

  // Scroll chat
  useEffect(() => {
    if (activeTab === 'chat') {
      chatEndRef.current?.scrollIntoView({ behavior: 'smooth' });
    }
  }, [session?.messages, activeTab, session?.diff_summary]);

  // --- ACTIONS ---

  const handleSend = async () => {
    if (!input.trim() || !session) return;
    const userMsg = input;
    setInput('');
    setIsThinking(true);

    // Optimistic UI
    const newMsg: ChatMessage = { role: 'user', content: userMsg };
    setSession((prev) => (prev ? { ...prev, messages: [...prev.messages, newMsg] } : null));

    try {
      const updated = await architectService.chat(session.id, userMsg, selectedModel);
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
      // Need to add this method to architectService if not present, assume it maps to /apply
      // Using raw fetch here if service update isn't propagated yet, or service method
      const res = await architectService.applySessionChanges(session.id);

      if (!res.ok) throw new Error('Failed to apply');

      const updatedSession = await res.json();
      setSession(updatedSession);
      toast('Changes applied to Sandbox!', 'success');

      // Refresh local resources to match new state
      await refreshResources();
    } catch (e) {
      toast('Failed to apply changes', 'error');
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
          // Transform to backend schema format
          contentToAttach = JSON.stringify(
            {
              name: col.name,
              schema: col.schema, // Already in correct format from service
              type: col.type,
            },
            null,
            2
          );
        }
      } else if (type === 'script') {
        const scr = scripts.find((s) => s.id === id);
        if (scr) {
          name = scr.name;
          contentToAttach = `// Script: ${scr.name}\n// Trigger: ${scr.trigger_type}\n${scr.code}`;
        }
      } else if (type === 'template') {
        const t = templates.find((t) => t.id === id);
        if (t) {
          name = t.slug;
          contentToAttach = `<!-- Template: ${t.slug} -->\n${t.content}`;
        }
      }

      if (contentToAttach) {
        setInput(
          (prev) =>
            `${prev}\n\n[Attached ${type}: ${name}]\n\`\`\`json\n${contentToAttach}\n\`\`\`\n\n`
        );
        toast(`Attached ${name}`, 'success');
      }
    } catch (e) {
      toast('Failed to attach item', 'error');
    }
  };

  const toggleTab = async (tab: 'chat' | 'preview' | 'tools') => {
    if (activeTab === tab) {
      setActiveTab(null);
    } else {
      setActiveTab(tab);
      if (tab === 'preview') {
        const latestTemplates = await refreshResources();
        if (!previewUrl && latestTemplates.length > 0) {
          const home =
            latestTemplates.find((t) => t.slug.includes('index') || t.slug.includes('home')) ||
            latestTemplates[0];
          setPreviewUrl(`/sandbox/${session.id}/render/${home.slug}`);
        }
      }
    }
  };

  if (!sessionId) return null;

  return (
    <div className="fixed bottom-6 left-1/2 -translate-x-1/2 z-[100] flex flex-col items-center gap-4 w-full max-w-3xl pointer-events-none">
      {/* 1. CHAT PANEL */}
      {activeTab === 'chat' && (
        <div className="w-full h-[600px] bg-background/95 backdrop-blur-md border border-primary/20 rounded-xl shadow-2xl flex flex-col pointer-events-auto overflow-hidden animate-in slide-in-from-bottom-5 zoom-in-95">
          <div className="p-3 border-b border-border flex justify-between items-center bg-primary/5">
            <div className="flex items-center gap-2 font-semibold text-primary">
              <Sparkles className="h-4 w-4" />
              <span className="hidden sm:inline">Architect Chat</span>
            </div>

            <div className="flex items-center gap-2">
              <div className="w-40">
                <Select
                  value={selectedModel}
                  onChange={(e: any) => setSelectedModel(e.target.value)}
                  className="h-7 text-xs py-0 bg-background/50 border-transparent hover:border-border transition-colors focus:ring-0 shadow-none"
                >
                  {AI_MODELS.map((m) => (
                    <option key={m.value} value={m.value}>
                      {m.label}
                    </option>
                  ))}
                </Select>
              </div>
              <Button
                variant="ghost"
                size="icon"
                className="h-6 w-6"
                onClick={() => setActiveTab(null)}
              >
                <X className="h-4 w-4" />
              </Button>
            </div>
          </div>

          <div className="flex-1 overflow-y-auto p-4 space-y-4 bg-gradient-to-b from-transparent to-black/5">
            {session?.messages.map((msg, i) => (
              <div
                key={i}
                className={`flex gap-3 ${msg.role === 'user' ? 'flex-row-reverse' : ''}`}
              >
                <div
                  className={`max-w-[85%] rounded-lg p-3 text-sm whitespace-pre-wrap shadow-sm ${msg.role === 'user' ? 'bg-primary text-primary-foreground' : 'bg-card border border-border text-foreground'} ${msg.role === 'error' ? 'bg-destructive/10 text-destructive border-destructive/20' : ''}`}
                >
                  {msg.content}
                </div>
              </div>
            ))}

            {/* PENDING CHANGES DIFF VIEW */}
            {session?.diff_summary && (
              <div className="mx-4 my-2 border border-amber-500/30 bg-amber-500/5 rounded-lg overflow-hidden animate-in fade-in slide-in-from-bottom-2">
                <div className="bg-amber-500/10 px-3 py-2 border-b border-amber-500/20 flex justify-between items-center">
                  <span className="text-xs font-bold text-amber-500 flex items-center gap-2">
                    <AlertTriangle className="h-3 w-3" /> Pending Changes
                  </span>
                </div>
                <div className="p-3">
                  <pre className="text-xs font-mono text-foreground/80 whitespace-pre-wrap">
                    {session.diff_summary}
                  </pre>
                </div>
                <div className="p-2 bg-background/50 flex gap-2 justify-end border-t border-amber-500/10">
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={() => {
                      /* Logic to discard? */
                    }}
                    className="h-7 text-xs"
                  >
                    Refine
                  </Button>
                  <Button
                    size="sm"
                    onClick={handleApply}
                    isLoading={isApplying}
                    className="h-7 text-xs bg-amber-500 hover:bg-amber-600 text-black"
                  >
                    <Check className="h-3 w-3 mr-1" /> Apply Changes
                  </Button>
                </div>
              </div>
            )}

            {isThinking && (
              <div className="flex items-center gap-2 text-xs text-muted-foreground animate-pulse p-2">
                <RefreshCw className="h-3 w-3 animate-spin" /> Architect is building...
              </div>
            )}
            <div ref={chatEndRef} />
          </div>

          <div className="p-3 border-t border-border bg-background">
            <div className="relative flex items-center gap-2">
              {/* Attach Button */}
              <Button
                ref={attachBtnRef}
                variant="ghost"
                size="icon"
                className="shrink-0 h-9 w-9 text-muted-foreground hover:text-primary"
                onClick={() => setIsAttachOpen(!isAttachOpen)}
                title="Attach Context"
              >
                <Paperclip className="h-4 w-4" />
              </Button>

              <Input
                value={input}
                onChange={(e: any) => setInput(e.target.value)}
                onKeyDown={(e: any) => e.key === 'Enter' && !e.shiftKey && handleSend()}
                placeholder={
                  session?.diff_summary
                    ? 'Refine these changes or describe new ones...'
                    : 'Describe a feature, schema change, or UI tweak...'
                }
                className="shadow-inner bg-secondary/30 border-transparent focus:border-primary"
                autoFocus
              />
              <Button
                size="icon"
                className="h-9 w-9 shrink-0"
                onClick={handleSend}
                disabled={!input.trim() || isThinking || isApplying}
              >
                <Send className="h-4 w-4" />
              </Button>

              {/* Attachments Overlay */}
              <Overlay
                isOpen={isAttachOpen}
                onClose={() => setIsAttachOpen(false)}
                anchorRef={attachBtnRef}
                align="start"
                className="mb-2"
              >
                <div className="w-64 bg-popover border border-border rounded-lg shadow-xl overflow-hidden flex flex-col max-h-[300px]">
                  <div className="p-2 bg-secondary/20 text-[10px] font-bold text-muted-foreground uppercase tracking-wider border-b border-border">
                    Attach Context
                  </div>
                  <div className="overflow-y-auto flex-1 p-1">
                    {/* Collections */}
                    <div className="px-2 py-1 text-[10px] text-muted-foreground font-semibold">
                      Collections
                    </div>
                    {collections.map((c) => (
                      <button
                        key={c.id}
                        onClick={() => handleAttach('collection', c.id)}
                        className="w-full text-left px-2 py-1.5 text-xs hover:bg-accent rounded flex items-center gap-2"
                      >
                        <Database className="h-3 w-3 text-blue-500" /> {c.name}
                      </button>
                    ))}

                    {/* Templates */}
                    <div className="px-2 py-1 text-[10px] text-muted-foreground font-semibold mt-1">
                      Templates
                    </div>
                    {templates.map((t) => (
                      <button
                        key={t.id}
                        onClick={() => handleAttach('template', t.id)}
                        className="w-full text-left px-2 py-1.5 text-xs hover:bg-accent rounded flex items-center gap-2"
                      >
                        <LayoutTemplate className="h-3 w-3 text-purple-500" /> {t.slug}
                      </button>
                    ))}

                    {/* Scripts */}
                    <div className="px-2 py-1 text-[10px] text-muted-foreground font-semibold mt-1">
                      Scripts
                    </div>
                    {scripts.map((s) => (
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
        </div>
      )}

      {/* 2. PREVIEW PANEL */}
      {activeTab === 'preview' && (
        <div className="w-full h-[600px] bg-background border border-border rounded-xl shadow-2xl flex flex-col pointer-events-auto overflow-hidden animate-in slide-in-from-bottom-5">
          <div className="p-2 border-b border-border flex gap-2 items-center bg-secondary/20">
            {/* Templates List */}
            <div className="flex-1 flex gap-2 overflow-x-auto no-scrollbar items-center px-1">
              {templates.length === 0 ? (
                <span className="text-xs text-muted-foreground italic flex items-center gap-2">
                  <Terminal className="h-3 w-3" /> No templates yet. Ask Architect to build one.
                </span>
              ) : (
                templates.map((t) => (
                  <button
                    key={t.id}
                    onClick={() => setPreviewUrl(`/sandbox/${session.id}/render/${t.slug}`)}
                    className={`px-3 py-1 text-xs rounded-full whitespace-nowrap transition-colors flex items-center gap-1.5 ${previewUrl?.endsWith(t.slug) ? 'bg-primary text-primary-foreground shadow-sm' : 'bg-background hover:bg-secondary border border-border text-muted-foreground'}`}
                  >
                    <LayoutTemplate className="h-3 w-3 opacity-70" />
                    {t.slug}
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
                className="h-7 w-7"
                onClick={() => setActiveTab(null)}
              >
                <X className="h-4 w-4" />
              </Button>
            </div>
          </div>
          <div className="flex-1 bg-white relative">
            {previewUrl ? (
              <iframe src={previewUrl} className="w-full h-full border-none" title="Preview" />
            ) : (
              <div className="flex flex-col h-full items-center justify-center text-gray-400 gap-2">
                <LayoutTemplate className="h-10 w-10 opacity-20" />
                <span className="text-sm">Select a template to preview</span>
              </div>
            )}
          </div>
        </div>
      )}

      {/* 3. TOOLS PANEL */}
      {activeTab === 'tools' && (
        <div className="bg-popover border border-border rounded-xl shadow-xl p-2 mb-2 grid grid-cols-2 gap-2 w-64 pointer-events-auto animate-in zoom-in-95 slide-in-from-bottom-2">
          <Button
            variant="ghost"
            className="justify-start gap-2 h-9 text-xs"
            onClick={() =>
              window.open(`${APP_CONFIG.apiBaseUrl}/sandbox/${sessionId}/scalar`, '_blank')
            }
          >
            <Terminal className="h-3.5 w-3.5 text-blue-500" /> API Docs
          </Button>
          <Button
            variant="ghost"
            className="justify-start gap-2 h-9 text-xs"
            onClick={() => {
              /* Trigger re-index */
            }}
          >
            <Zap className="h-3.5 w-3.5 text-amber-500" /> Re-index Search
          </Button>
        </div>
      )}

      {/* --- MAIN TOOLBAR --- */}
      <div className="flex items-center gap-2 p-1.5 bg-foreground/90 backdrop-blur-md text-background rounded-full shadow-2xl border border-white/10 pointer-events-auto hover:scale-[1.01] transition-transform duration-200">
        <div className="flex items-center gap-2 pl-3 pr-2 border-r border-white/20">
          <div className="w-2 h-2 bg-amber-400 rounded-full animate-pulse shadow-[0_0_8px_rgba(251,191,36,0.8)]" />
          <span className="text-xs font-bold font-mono tracking-tight">SANDBOX</span>
        </div>
        <div className="flex items-center gap-1">
          <ToolbarButton
            icon={<Sparkles className="h-4 w-4" />}
            label="Architect"
            active={activeTab === 'chat'}
            onClick={() => toggleTab('chat')}
          />
          <ToolbarButton
            icon={<Play className="h-4 w-4" />}
            label="Preview"
            active={activeTab === 'preview'}
            onClick={() => toggleTab('preview')}
          />
          <ToolbarButton
            icon={<LayoutTemplate className="h-4 w-4" />}
            label="Tools"
            active={activeTab === 'tools'}
            onClick={() => toggleTab('tools')}
          />
        </div>
        <div className="pl-2 pr-1 border-l border-white/20">
          <button
            onClick={() => setIsOpen(false)}
            className="p-1.5 rounded-full hover:bg-white/20 text-white/70 hover:text-white transition-colors"
          >
            <ChevronDown className="h-3.5 w-3.5" />
          </button>
        </div>
      </div>

      {!isOpen && (
        <Button
          className="absolute bottom-4 right-4 rounded-full h-12 w-12 shadow-xl pointer-events-auto"
          onClick={() => setIsOpen(true)}
        >
          <Sparkles className="h-5 w-5" />
        </Button>
      )}
    </div>
  );
};

const ToolbarButton = ({ icon, label, active, onClick }: any) => (
  <button
    onClick={onClick}
    className={`flex items-center gap-2 px-3 py-1.5 rounded-full text-xs font-medium transition-all ${active ? 'bg-white text-black shadow-md scale-105' : 'hover:bg-white/10 text-white/90 hover:text-white'}`}
  >
    {icon} <span>{label}</span>
  </button>
);
