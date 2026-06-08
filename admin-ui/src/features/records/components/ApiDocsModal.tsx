import React, { useState } from 'react';
import { createPortal } from 'react-dom';
import {
  X,
  Copy,
  Check,
  Database,
  Search,
  BrainCircuit,
  Folder,
  Terminal,
  Server,
  Code2,
  Menu,
  ShieldAlert,
  Users,
  LayoutTemplate,
} from 'lucide-react';
import { Button, Badge } from '../../../components/ui/Elements';
import { Collection } from '../../../types';
import { APP_CONFIG } from '../../../config/app.config';

// Import modular documentation sections
import { getSetupDocs } from '../docs/SetupDocs';
import { AuthDocs } from '../docs/AuthDocs';
import { getCrudDocs } from '../docs/CrudDocs';
import { getSearchDocs } from '../docs/SearchDocs';
import { getFileDocs } from '../docs/FileDocs';
import { getAiDocs } from '../docs/AiDocs';
import { ScriptDocs } from '../docs/ScriptDocs';
import { SsrDocs } from '../docs/SsrDocs';
import { ErrorDocs } from '../docs/ErrorDocs';
import { QueryEngineDocs } from '../docs/QueryEngineDocs';

interface ApiDocsModalProps {
  isOpen: boolean;
  onClose: () => void;
  collection?: Collection;
  context?: 'collection' | 'users';
}

const CodeBlock = ({
  label,
  code,
  className = '',
}: {
  label: string;
  code: string;
  className?: string;
}) => {
  const [copied, setCopied] = useState(false);

  const handleCopy = () => {
    navigator.clipboard.writeText(code);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div
      className={`rounded-lg border border-border bg-[#0d1117] overflow-hidden my-4 group relative shadow-sm ${className}`}
    >
      <div className="flex items-center justify-between px-3 py-2 bg-[#161b22] border-b border-white/5">
        <span className="text-[10px] font-mono text-muted-foreground uppercase tracking-wider font-semibold">
          {label}
        </span>
        <button
          onClick={handleCopy}
          className="text-gray-400 hover:text-white transition-colors p-1 hover:bg-white/10 rounded"
          title="Copy"
        >
          {copied ? <Check className="h-3.5 w-3.5" /> : <Copy className="h-3.5 w-3.5" />}
        </button>
      </div>
      <div className="p-4 overflow-x-auto custom-scrollbar">
        <pre className="text-xs sm:text-sm font-mono text-[#e6edf3] leading-relaxed whitespace-pre">
          <code>{code.trim()}</code>
        </pre>
      </div>
    </div>
  );
};

const SectionHeader = ({
  title,
  icon: Icon,
  description,
}: {
  title: string;
  icon: any;
  description: string;
}) => (
  <div className="mb-6">
    <h4 className="text-xl font-bold flex items-center gap-2 text-foreground">
      <div className="p-2 rounded-md bg-primary/10 text-primary">
        <Icon className="h-5 w-5" />
      </div>
      {title}
    </h4>
    <p className="text-sm text-muted-foreground mt-1 ml-11">{description}</p>
  </div>
);

export const ApiDocsModal = ({ isOpen, onClose, collection, context }: ApiDocsModalProps) => {
  const [activeTab, setActiveTab] = useState(context === 'users' ? 'auth' : 'setup');
  const [isMobileMenuOpen, setIsMobileMenuOpen] = useState(false);

  if (!isOpen) return null;

  const colName = collection?.name || 'posts';
  const setupDocs = getSetupDocs(APP_CONFIG.apiBaseUrl, window.location.pathname);
  const crudDocs = getCrudDocs(colName);
  const searchDocs = getSearchDocs(colName);
  const fileDocs = getFileDocs(colName);
  const aiDocs = getAiDocs(colName);

  const sections = [
    { id: 'setup', label: 'Initialization', icon: Server },
    { id: 'auth', label: 'Users & Auth', icon: Users },
    { id: 'records', label: 'Records (CRUD)', icon: Database },
    { id: 'search', label: 'Search Engine', icon: Search },
    { id: 'files', label: 'File Storage', icon: Folder },
    { id: 'ai', label: 'AI Actions', icon: BrainCircuit },
    { id: 'scripts', label: 'Scripts', icon: Terminal },
    { id: 'ssr', label: 'SSR & Templates', icon: LayoutTemplate },
    { id: 'errors', label: 'Error Handling', icon: ShieldAlert },
  ];

  return createPortal(
    <div className="fixed inset-0 z-[100] flex justify-center items-center isolate">
      <div
        className="absolute inset-0 bg-black/60 backdrop-blur-sm animate-in fade-in"
        onClick={onClose}
      />

      <div className="relative w-[95vw] md:w-[90vw] max-w-6xl h-[90vh] md:h-[85vh] bg-background border border-border shadow-2xl animate-in zoom-in-95 duration-200 flex flex-col md:flex-row rounded-xl overflow-hidden">
        {/* Mobile Header */}
        <div className="md:hidden flex items-center justify-between p-4 border-b border-border bg-secondary/5">
          <button
            onClick={() => setIsMobileMenuOpen(!isMobileMenuOpen)}
            className="p-2 hover:bg-secondary rounded-md"
          >
            <Menu className="h-5 w-5" />
          </button>
          <span className="font-bold text-sm flex items-center gap-2">
            <Code2 className="h-4 w-4 text-primary" /> SDK Guide
          </span>
          <button onClick={onClose} className="p-2 hover:bg-secondary rounded-md">
            <X className="h-5 w-5" />
          </button>
        </div>

        {/* Sidebar Navigation */}
        <div
          className={`
            absolute md:relative inset-y-0 left-0 z-20 w-64 bg-background/95 md:bg-secondary/5 border-r border-border flex flex-col shrink-0 transition-transform duration-300 transform
            ${isMobileMenuOpen ? 'translate-x-0' : '-translate-x-full'} md:translate-x-0 backdrop-blur-md md:backdrop-blur-none
          `}
        >
          <div className="hidden md:flex p-5 border-b border-border/50 items-center gap-3">
            <div className="h-8 w-8 rounded-lg bg-primary/10 flex items-center justify-center text-primary">
              <Code2 className="h-5 w-5" />
            </div>
            <div>
              <h2 className="font-bold text-sm">Developer Guide</h2>
              <p className="text-[10px] text-muted-foreground">ApexKit SDK v0.1</p>
            </div>
          </div>

          <div className="flex-1 overflow-y-auto p-3 space-y-1">
            {sections.map((s) => (
              <button
                key={s.id}
                onClick={() => {
                  setActiveTab(s.id);
                  setIsMobileMenuOpen(false);
                }}
                className={`w-full text-left px-3 py-2.5 rounded-md text-sm font-medium flex items-center gap-3 transition-colors ${activeTab === s.id ? 'bg-primary/10 text-primary border border-primary/20' : 'hover:bg-secondary text-muted-foreground hover:text-foreground border border-transparent'}`}
              >
                <s.icon
                  className={`h-4 w-4 ${activeTab === s.id ? 'text-primary' : 'opacity-70'}`}
                />
                {s.label}
              </button>
            ))}
          </div>
        </div>

        {isMobileMenuOpen && (
          <div
            className="absolute inset-0 bg-black/50 z-10 md:hidden"
            onClick={() => setIsMobileMenuOpen(false)}
          />
        )}

        {/* Main Content */}
        <div className="flex-1 flex flex-col relative bg-background overflow-hidden">
          <div className="absolute top-4 right-4 z-10 hidden md:block">
            <Button
              size="icon"
              variant="ghost"
              onClick={onClose}
              className="rounded-full h-8 w-8 hover:bg-secondary"
            >
              <X className="h-4 w-4" />
            </Button>
          </div>

          <div className="flex-1 overflow-y-auto p-6 md:p-10 custom-scrollbar">
            <div className="max-w-3xl mx-auto">
              {/* Setup / Initialization */}
              {activeTab === 'setup' && (
                <div className="animate-in fade-in slide-in-from-bottom-2 duration-300">
                  <SectionHeader
                    title="Initialization"
                    icon={Server}
                    description="Install the SDK and initialize the client. It automatically handles context switching for Tenants and Sandboxes."
                  />
                  <div className="space-y-6">
                    <CodeBlock label="Install via NPM" code="npm install @apexkit/sdk" />
                    <CodeBlock label="main.ts" code={setupDocs.initCode} />
                  </div>
                </div>
              )}

              {/* Users & Auth */}
              {activeTab === 'auth' && (
                <div className="animate-in fade-in slide-in-from-bottom-2 duration-300 space-y-10">
                  <SectionHeader
                    title="Users & Authentication"
                    icon={Users}
                    description="Complete workflows for user identity, roles, sessions, and recovery."
                  />
                  <div className="space-y-12">
                    <div>
                      <h5 className="font-bold text-lg mb-2 flex items-center gap-2">
                        <div className="w-2 h-2 rounded-full bg-emerald-500" />
                        1. Registration & Login
                      </h5>
                      <p className="text-sm text-muted-foreground mb-4 leading-relaxed">
                        Standard email and password authentication. When logged in, the client SDK
                        automatically attaches the JWT token as a Bearer Header.
                      </p>
                      <CodeBlock
                        label="SDK (TypeScript / Javascript)"
                        code={AuthDocs.registrationAndLogin}
                      />
                    </div>

                    <div>
                      <h5 className="font-bold text-lg mb-2 flex items-center gap-2">
                        <div className="w-2 h-2 rounded-full bg-amber-500" />
                        2. Password Reset Flow
                      </h5>
                      <p className="text-sm text-muted-foreground mb-4 leading-relaxed">
                        If a user forgets their password, they can request a reset link. This
                        requires SMTP to be configured in the dashboard.
                      </p>
                      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
                        <CodeBlock
                          label="Step 1: Request Reset (Frontend)"
                          code={AuthDocs.passwordResetRequest}
                        />
                        <CodeBlock
                          label="Step 2: Confirm Reset (Frontend)"
                          code={AuthDocs.passwordResetConfirm}
                        />
                      </div>
                    </div>

                    <div>
                      <h5 className="font-bold text-lg mb-2 flex items-center gap-2">
                        <div className="w-2 h-2 rounded-full bg-blue-500" />
                        3. Email Verification Flow
                      </h5>
                      <p className="text-sm text-muted-foreground mb-4 leading-relaxed">
                        When a user registers, they are marked as unverified. You can restrict
                        database access using custom Scripts or manually prompt them.
                      </p>
                      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
                        <CodeBlock
                          label="Resend Verification Email"
                          code={AuthDocs.emailVerificationResend}
                        />
                        <CodeBlock
                          label="Verify User (From Email Link)"
                          code={AuthDocs.emailVerificationConfirm}
                        />
                      </div>
                    </div>

                    <div>
                      <h5 className="font-bold text-lg mb-2 flex items-center gap-2">
                        <div className="w-2 h-2 rounded-full bg-purple-500" />
                        4. OAuth (Google / GitHub)
                      </h5>
                      <p className="text-sm text-muted-foreground mb-4 leading-relaxed">
                        Allow users to sign in using third-party providers. The SDK will redirect
                        the user to the provider, and ApexKit will handle the callback.
                      </p>
                      <CodeBlock label="SDK (TypeScript / Javascript)" code={AuthDocs.oauthSetup} />
                    </div>
                  </div>
                </div>
              )}

              {/* Records CRUD */}
              {activeTab === 'records' && (
                <div className="animate-in fade-in slide-in-from-bottom-2 duration-300 space-y-10">
                  <div>
                    <SectionHeader
                      title="Data Operations"
                      icon={Database}
                      description={`Standard CRUD operations for the '${colName}' collection.`}
                    />
                    <div className="grid grid-cols-1 gap-8">
                      <div>
                        <h5 className="font-bold text-sm mb-2 flex items-center gap-2">
                          <div className="w-1.5 h-1.5 rounded-full bg-blue-500" /> List & Filter
                        </h5>
                        <p className="text-xs text-muted-foreground mb-3">
                          Fetch records with pagination, sorting, and MongoDB-style filtering.
                        </p>
                        <CodeBlock label="JS" code={crudDocs.listAndFilter} />
                      </div>
                      <div>
                        <h5 className="font-bold text-sm mb-2 flex items-center gap-2">
                          <div className="w-1.5 h-1.5 rounded-full bg-green-500" /> Create, Update,
                          Delete
                        </h5>
                        <CodeBlock label="JS" code={crudDocs.createUpdateDelete} />
                      </div>
                    </div>
                  </div>

                  <div>
                    <h5 className="font-bold text-base mb-2 text-foreground flex items-center gap-2">
                      <div className="w-2 h-2 rounded-full bg-purple-500" />
                      2. Advanced Queries ($query / SQL Engine)
                    </h5>
                    <p className="text-sm text-muted-foreground mb-4 leading-relaxed">
                      {QueryEngineDocs.intro}
                    </p>
                    <CodeBlock
                      label="Flow Guide: Feeding UI Filter Values into the SDK"
                      code={QueryEngineDocs.flowSnippet}
                    />
                    <CodeBlock
                      label="Aggregation & Grouping Example"
                      code={QueryEngineDocs.advancedSnippets.aggregates}
                    />
                    <CodeBlock
                      label="Compound Logical Filters"
                      code={QueryEngineDocs.advancedSnippets.operators}
                    />
                  </div>
                </div>
              )}

              {/* Search Engine */}
              {activeTab === 'search' && (
                <div className="animate-in fade-in slide-in-from-bottom-2 duration-300 space-y-10">
                  <SectionHeader
                    title="Search Engine"
                    icon={Search}
                    description="High-performance full-text and semantic search capabilities."
                  />
                  <div className="space-y-8">
                    <div className="p-4 rounded-xl border border-blue-500/20 bg-blue-500/5">
                      <div className="flex items-center justify-between mb-3">
                        <div className="flex items-center gap-2">
                          <Badge
                            variant="outline"
                            className="bg-blue-500/10 text-blue-500 border-blue-500/20"
                          >
                            Fast
                          </Badge>
                          <h5 className="font-bold text-sm">Instant Search (Tantivy/OSE)</h5>
                        </div>
                      </div>
                      <p className="text-xs text-muted-foreground mb-4 leading-relaxed">
                        Typo-tolerant, ultra-fast full text search. Returns snippets with HTML
                        highlights.
                      </p>
                      <CodeBlock label="JS" code={searchDocs.instantSearch} />
                    </div>

                    <div className="p-4 rounded-xl border border-purple-500/20 bg-purple-500/5">
                      <div className="flex items-center justify-between mb-3">
                        <div className="flex items-center gap-2">
                          <Badge
                            variant="outline"
                            className="bg-purple-500/10 text-purple-500 border-purple-500/20"
                          >
                            Semantic
                          </Badge>
                          <h5 className="font-bold text-sm">Text Vector Search</h5>
                        </div>
                      </div>
                      <p className="text-xs text-muted-foreground mb-4 leading-relaxed">
                        Converts query text to embeddings to find conceptually similar records.
                      </p>
                      <CodeBlock label="JS" code={searchDocs.textVectorSearch} />
                    </div>

                    <div className="p-4 rounded-xl border border-pink-500/20 bg-pink-500/5">
                      <div className="flex items-center justify-between mb-3">
                        <div className="flex items-center gap-2">
                          <Badge
                            variant="outline"
                            className="bg-pink-500/10 text-pink-500 border-pink-500/20"
                          >
                            Multimodal
                          </Badge>
                          <h5 className="font-bold text-sm">Image Vector Search</h5>
                        </div>
                      </div>
                      <p className="text-xs text-muted-foreground mb-4 leading-relaxed">
                        Find visually similar images by passing a Base64 image payload.
                      </p>
                      <CodeBlock label="JS" code={searchDocs.imageVectorSearch} />
                    </div>
                  </div>
                </div>
              )}

              {/* File Storage */}
              {activeTab === 'files' && (
                <div className="animate-in fade-in slide-in-from-bottom-2 duration-300 space-y-8">
                  <SectionHeader
                    title="File Storage"
                    icon={Folder}
                    description="Upload and manage assets (Images, Documents)."
                  />
                  <div className="grid grid-cols-1 gap-6">
                    <div>
                      <h5 className="font-semibold text-sm mb-2">Upload & Link Workflow</h5>
                      <p className="text-xs text-muted-foreground mb-3">
                        Files are uploaded directly to the storage backend. The returned file object
                        contains the public URL.
                      </p>
                      <CodeBlock label="JS" code={fileDocs.uploadAndLink} />
                    </div>
                  </div>
                </div>
              )}

              {/* AI Actions */}
              {activeTab === 'ai' && (
                <div className="animate-in fade-in slide-in-from-bottom-2 duration-300 space-y-6">
                  <SectionHeader
                    title="AI Actions"
                    icon={BrainCircuit}
                    description="Execute server-side LLM prompts securely without exposing API keys."
                  />
                  <CodeBlock label="JS" code={aiDocs.runAction} />
                </div>
              )}

              {/* Server Scripts */}
              {activeTab === 'scripts' && (
                <div className="animate-in fade-in slide-in-from-bottom-2 duration-300 space-y-6">
                  <SectionHeader
                    title="Server Scripts"
                    icon={Terminal}
                    description="Trigger custom backend JavaScript logic via API endpoints."
                  />
                  <CodeBlock label="JS" code={ScriptDocs.runScript} />
                </div>
              )}

              {/* SSR & Templates */}
              {activeTab === 'ssr' && (
                <div className="animate-in fade-in slide-in-from-bottom-2 duration-300 space-y-12">
                  <SectionHeader
                    title="Server-Side Rendering (SSR)"
                    icon={LayoutTemplate}
                    description="Build fast, secure, and dynamic pages using JavaScript Controllers, Tera HTML, and HTMX."
                  />

                  <div>
                    <h5 className="font-bold text-base mb-2 text-foreground border-b border-border pb-2">
                      1. Anatomy of a Template
                    </h5>
                    <p className="text-sm text-muted-foreground mb-4">
                      A template consists of a Server-Side JavaScript Controller and Tera HTML.
                    </p>
                    <CodeBlock label="HTML / TERA" code={SsrDocs.anatomy} />
                  </div>

                  <div>
                    <h5 className="font-bold text-base mb-2 text-foreground border-b border-border pb-2">
                      2. Routing, Request, and Auth
                    </h5>
                    <p className="text-sm text-muted-foreground mb-4">{SsrDocs.introText}</p>
                    <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
                      <CodeBlock label="Request Payload" code={SsrDocs.payload} />
                      <CodeBlock label="Protecting a Route" code={SsrDocs.routeProtection} />
                    </div>
                  </div>

                  <div>
                    <h5 className="font-bold text-base mb-2 text-foreground border-b border-border pb-2">
                      3. Client-Side: <code className="text-primary">apex.js</code> and HTMX
                    </h5>
                    <p className="text-sm text-muted-foreground mb-4">
                      Include the built-in `apex.js` script to manage JWT tokens and scope routing
                      automatically.
                    </p>
                    <CodeBlock label="base-layout template" code={SsrDocs.clientScript} />
                  </div>

                  <div>
                    <h5 className="font-bold text-base mb-2 text-foreground border-b border-border pb-2">
                      4. Components & Includes
                    </h5>
                    <p className="text-sm text-muted-foreground mb-4">
                      Break your UI into reusable components using standard inclusions.
                    </p>
                    <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
                      <CodeBlock
                        label="components/navbar template"
                        code={SsrDocs.navbarComponent}
                      />
                      <CodeBlock label="dashboard template" code={SsrDocs.dashboardComponent} />
                    </div>
                  </div>
                </div>
              )}

              {/* Error Handling */}
              {activeTab === 'errors' && (
                <div className="animate-in fade-in slide-in-from-bottom-2 duration-300 space-y-8">
                  <SectionHeader
                    title="Error Handling"
                    icon={ShieldAlert}
                    description="Standardized JSON error responses across all APIs."
                  />
                  <div className="grid gap-6">
                    <div>
                      <h5 className="font-bold text-sm mb-3">Response Format</h5>
                      <CodeBlock label="JSON" code={ErrorDocs.responseFormat} />
                    </div>
                    <div>
                      <h5 className="font-bold text-sm mb-3">Common Status Codes</h5>
                      <div className="rounded-lg border border-border overflow-hidden">
                        <table className="w-full text-sm text-left">
                          <thead className="bg-secondary/10 text-muted-foreground text-xs uppercase">
                            <tr>
                              <th className="px-4 py-3 font-medium">Code</th>
                              <th className="px-4 py-3 font-medium">Error</th>
                              <th className="px-4 py-3 font-medium">Description</th>
                            </tr>
                          </thead>
                          <tbody className="divide-y divide-border">
                            <tr>
                              <td className="px-4 py-3 font-mono text-xs">400</td>
                              <td className="px-4 py-3">input_validation</td>
                              <td className="px-4 py-3 text-muted-foreground">
                                Invalid input or bad JSON syntax.
                              </td>
                            </tr>
                            <tr>
                              <td className="px-4 py-3 font-mono text-xs">401</td>
                              <td className="px-4 py-3">unauthorized</td>
                              <td className="px-4 py-3 text-muted-foreground">
                                Missing or invalid Auth Token.
                              </td>
                            </tr>
                            <tr>
                              <td className="px-4 py-3 font-mono text-xs">403</td>
                              <td className="px-4 py-3">forbidden</td>
                              <td className="px-4 py-3 text-muted-foreground">
                                Policy denied access.
                              </td>
                            </tr>
                            <tr>
                              <td className="px-4 py-3 font-mono text-xs">404</td>
                              <td className="px-4 py-3">not_found</td>
                              <td className="px-4 py-3 text-muted-foreground">
                                Resource does not exist.
                              </td>
                            </tr>
                            <tr>
                              <td className="px-4 py-3 font-mono text-xs">422</td>
                              <td className="px-4 py-3">validation_error</td>
                              <td className="px-4 py-3 text-muted-foreground">
                                Schema constraint violation.
                              </td>
                            </tr>
                          </tbody>
                        </table>
                      </div>
                    </div>
                  </div>
                </div>
              )}
            </div>
          </div>
        </div>
      </div>
    </div>,
    document.body
  );
};
