import React, { useState, useRef, useEffect } from 'react';
import {
  Send,
  Bot,
  User,
  Code,
  ExternalLink,
  RefreshCw,
  UploadCloud,
  X,
  History,
  Box,
  Calendar,
  FileJson,
} from 'lucide-react';
import { Button, Input, Textarea, Badge } from '../../../components/ui/Elements';
import { PreviewPanel } from '../../../components/preview/PreviewPanel';
import { Overlay } from '../../../components/overlay/Overlay';
import { Dialog } from '../../../components/ui/Dialog'; // Import Dialog for centered views
import { architectService, AiSession, Plugin } from '../services/architectService';
import { useToast } from '../../../components/feedback/Toast';
import { APP_CONFIG } from '../../../config/app.config';

import { AI_MODELS, DEFAULT_AI_MODEL } from '../../../config/ai-models';
import { Select } from '../../../components/ui/Elements';

interface AiSessionPanelProps {
  session: AiSession | null;
  onClose: () => void;
  onUpdate: (session: AiSession) => void;
}

export const AiSessionPanel = ({ session, onClose, onUpdate }: AiSessionPanelProps) => {
  const [input, setInput] = useState('');
  const [isThinking, setIsThinking] = useState(false);
  const [model, setModel] = useState(DEFAULT_AI_MODEL);
  const scrollRef = useRef<HTMLDivElement>(null);
  const { toast } = useToast();

  // Manifest Overlay State (Anchored to button)
  const [showManifest, setShowManifest] = useState(false);
  const manifestBtnRef = useRef<HTMLButtonElement>(null);

  // Versions Modal State (Centered)
  const [showVersions, setShowVersions] = useState(false);
  const [versions, setVersions] = useState<Plugin[]>([]);
  const [isLoadingVersions, setIsLoadingVersions] = useState(false);

  // Version Preview State (Stacked on top of Versions)
  const [viewVersion, setViewVersion] = useState<Plugin | null>(null);

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [session?.messages]);

  const handleSend = async () => {
    if (!session || !input.trim()) return;
    const prompt = input;
    setInput('');
    setIsThinking(true);

    const optimisticSession = {
      ...session,
      messages: [...session.messages, { role: 'user' as const, content: prompt }],
    };
    onUpdate(optimisticSession);

    try {
      const updated = await architectService.chat(session.id, prompt, model);
      onUpdate(updated);
      toast('Architect has applied changes.', 'success');
    } catch (e: any) {
      toast(e.message, 'error');
    } finally {
      setIsThinking(false);
    }
  };

  const handlePublish = async () => {
    if (!session) return;
    try {
      await architectService.publish(session.id);
      toast('Plugin published successfully!', 'success');
      if (showVersions) loadVersions();
    } catch (e: any) {
      toast(e.message, 'error');
    }
  };

  const loadVersions = async () => {
    setIsLoadingVersions(true);
    try {
      const allPlugins = await architectService.listPlugins();
      const appName = session?.current_manifest?.app_name;
      const relevant = appName ? allPlugins.filter((p) => p.name === appName) : [];
      setVersions(relevant);
    } catch (e) {
      console.error(e);
    } finally {
      setIsLoadingVersions(false);
    }
  };

  const toggleVersions = () => {
    if (!showVersions) {
      loadVersions();
    }
    setShowVersions(!showVersions);
    setShowManifest(false);
  };

  const startUrl = session?.current_manifest?.templates[0]?.slug
    ? `${APP_CONFIG.apiBaseUrl}/sandbox/${session.id}/render/${session.current_manifest.templates[0].slug}`
    : null;

  if (!session) return null;

  return (
    <PreviewPanel
      isOpen={!!session}
      onClose={onClose}
      title={session.name}
      actions={
        <div className="flex gap-2 w-full">
          {/* View Current Schema (Anchored Overlay) */}
          <Button
            ref={manifestBtnRef}
            variant="outline"
            onClick={() => {
              setShowManifest(!showManifest);
              setShowVersions(false);
            }}
            className="flex-1"
            disabled={!session.current_manifest}
          >
            <Code className="mr-2 h-4 w-4" /> Schema
          </Button>

          {/* View History (Centered Dialog) */}
          <Button
            variant="outline"
            onClick={toggleVersions}
            className="flex-1"
            disabled={!session.current_manifest}
          >
            <History className="mr-2 h-4 w-4" /> Versions
          </Button>

          <Button onClick={handlePublish} className="flex-1" disabled={!session.current_manifest}>
            <UploadCloud className="mr-2 h-4 w-4" /> Publish
          </Button>
        </div>
      }
    >
      <div className="flex flex-col h-[calc(100vh-200px)]">
        {/* Chat Stream */}
        <div className="flex-1 overflow-y-auto p-4 space-y-4" ref={scrollRef}>
          <div className="flex gap-3 text-sm text-muted-foreground p-4 bg-secondary/10 rounded-lg border border-border border-dashed">
            <Bot className="h-5 w-5 shrink-0" />
            <div>
              <p>I am your ApexKit Architect. Describe what you want to build.</p>
              {startUrl && (
                <div className="mt-2">
                  <a
                    href={startUrl}
                    target="_blank"
                    rel="noreferrer"
                    className="inline-flex items-center text-primary hover:underline"
                  >
                    <ExternalLink className="mr-1 h-3 w-3" /> Open Live App
                  </a>
                </div>
              )}
            </div>
          </div>

          {session.messages.map((msg, idx) => (
            <div
              key={idx}
              className={`flex gap-3 ${msg.role === 'user' ? 'flex-row-reverse' : ''}`}
            >
              <div
                className={`h-8 w-8 rounded-full flex items-center justify-center shrink-0 ${msg.role === 'user' ? 'bg-primary text-primary-foreground' : 'bg-secondary text-secondary-foreground'}`}
              >
                {msg.role === 'user' ? <User className="h-4 w-4" /> : <Bot className="h-4 w-4" />}
              </div>
              <div
                className={`rounded-lg p-3 max-w-[85%] text-sm whitespace-pre-wrap ${msg.role === 'user' ? 'bg-primary/10 border border-primary/20' : 'bg-card border border-border'}`}
              >
                {msg.content}
              </div>
            </div>
          ))}

          {isThinking && (
            <div className="flex gap-3">
              <div className="h-8 w-8 rounded-full bg-secondary flex items-center justify-center shrink-0 animate-pulse">
                <Bot className="h-4 w-4" />
              </div>
              <div className="flex items-center gap-2 text-xs text-muted-foreground animate-pulse mt-2">
                <RefreshCw className="h-3 w-3 animate-spin" /> Thinking...
              </div>
            </div>
          )}
        </div>

        {/* Input Area */}
        <div className="p-4 border-t border-border mt-auto bg-background/50 backdrop-blur-sm">
          {/* Model Selector Bar */}
          <div className="flex justify-between items-center px-1">
            <span className="text-[10px] font-bold uppercase text-muted-foreground tracking-wider">
              AI Model
            </span>
            <div className="w-48">
              <Select
                value={model}
                onChange={(e: any) => setModel(e.target.value)}
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

          <div className="relative">
            <Textarea
              value={input}
              onChange={(e: any) => setInput(e.target.value)}
              placeholder="Type your request..."
              className="min-h-[80px] pr-12 resize-none font-sans"
              onKeyDown={(e) => {
                if (e.key === 'Enter' && !e.shiftKey) {
                  e.preventDefault();
                  handleSend();
                }
              }}
              autoFocus
            />
            <button
              onClick={handleSend}
              disabled={!input.trim() || isThinking}
              className="absolute bottom-3 right-3 p-2 bg-primary text-primary-foreground rounded-full hover:opacity-90 disabled:opacity-50 transition-all"
            >
              <Send className="h-4 w-4" />
            </button>
          </div>
        </div>

        {/* --- OVERLAY 1: CURRENT MANIFEST (Anchored to button) --- */}
        <Overlay
          isOpen={showManifest}
          onClose={() => setShowManifest(false)}
          anchorRef={manifestBtnRef}
          align="end"
          width={400}
          className="bg-popover text-popover-foreground border border-border shadow-2xl rounded-lg flex flex-col max-h-[500px] z-[60]"
        >
          <div className="flex items-center justify-between p-3 border-b border-border bg-secondary/10">
            <h4 className="text-sm font-semibold flex items-center gap-2">
              <Code className="h-4 w-4" /> Current Schema
            </h4>
            <button
              onClick={() => setShowManifest(false)}
              className="hover:bg-secondary rounded p-1"
            >
              <X className="h-4 w-4" />
            </button>
          </div>
          <div className="flex-1 overflow-auto p-0 bg-[#1e1e1e]">
            <pre className="text-[10px] font-mono text-blue-100 p-4 leading-relaxed">
              {session.current_manifest
                ? JSON.stringify(session.current_manifest, null, 2)
                : '// No manifest'}
            </pre>
          </div>
        </Overlay>

        {/* --- DIALOG 1: VERSIONS LIST (Centered) --- */}
        <Dialog
          isOpen={showVersions}
          onClose={() => setShowVersions(false)}
          title="Published Versions"
          size="md"
          zIndex={70} // Higher than panel
        >
          <div className="space-y-4">
            <p className="text-sm text-muted-foreground">
              Select a version to view its code manifest.
            </p>
            <div className="border rounded-md divide-y divide-border max-h-[400px] overflow-y-auto">
              {isLoadingVersions ? (
                <div className="p-8 text-center text-xs text-muted-foreground">
                  <RefreshCw className="h-4 w-4 animate-spin inline mr-2" /> Loading history...
                </div>
              ) : versions.length === 0 ? (
                <div className="p-8 text-center text-xs text-muted-foreground">
                  No published versions found.
                </div>
              ) : (
                versions.map((v) => (
                  <button
                    key={v.id}
                    onClick={() => setViewVersion(v)}
                    className="w-full text-left p-4 hover:bg-accent/50 transition-colors flex items-start gap-4 group"
                  >
                    <div className="h-8 w-8 rounded-full bg-primary/10 flex items-center justify-center shrink-0 text-primary group-hover:bg-primary group-hover:text-primary-foreground transition-colors">
                      <FileJson className="h-4 w-4" />
                    </div>
                    <div className="flex-1 min-w-0">
                      <div className="flex justify-between items-center mb-1">
                        <span className="font-medium text-sm truncate">{v.name}</span>
                        <Badge variant="secondary" className="text-[10px] font-mono">
                          v{v.version}
                        </Badge>
                      </div>
                      <div className="text-xs text-muted-foreground flex items-center gap-2 mb-1">
                        <Calendar className="h-3 w-3" />
                        {new Date(v.created_at).toLocaleString()}
                      </div>
                      {v.description && (
                        <p className="text-[11px] text-muted-foreground line-clamp-1">
                          {v.description}
                        </p>
                      )}
                    </div>
                  </button>
                ))
              )}
            </div>
          </div>
        </Dialog>

        {/* --- DIALOG 2: VERSION CODE PREVIEW (Centered, Stacked) --- */}
        <Dialog
          isOpen={!!viewVersion}
          onClose={() => setViewVersion(null)}
          title={`Manifest: ${viewVersion?.name} (v${viewVersion?.version})`}
          size="lg"
          zIndex={80} // Stacked on top of Versions list
        >
          <div className="flex flex-col h-[60vh]">
            <div className="flex-1 bg-[#1e1e1e] rounded-md border border-border overflow-hidden flex flex-col">
              <div className="flex items-center justify-between px-4 py-2 border-b border-white/10 bg-white/5">
                <span className="text-xs font-mono text-muted-foreground">manifest.json</span>
                <Button
                  variant="ghost"
                  size="sm"
                  className="h-6 text-xs"
                  onClick={() => {
                    navigator.clipboard.writeText(JSON.stringify(viewVersion?.manifest, null, 2));
                    toast('Copied to clipboard', 'success');
                  }}
                >
                  Copy
                </Button>
              </div>
              <div className="flex-1 overflow-auto p-4">
                <pre className="text-xs font-mono text-blue-100 leading-relaxed">
                  {viewVersion ? JSON.stringify(viewVersion.manifest, null, 2) : ''}
                </pre>
              </div>
            </div>
            <div className="pt-4 flex justify-end">
              <Button onClick={() => setViewVersion(null)}>Close Preview</Button>
            </div>
          </div>
        </Dialog>
      </div>
    </PreviewPanel>
  );
};
