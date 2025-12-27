import React, { useState } from 'react';
import { createPortal } from 'react-dom';
import { 
    X, Copy, Check, Database, Search, 
    BrainCircuit, Folder, Terminal, Server,
    Code2, Menu, AlertTriangle, ShieldAlert
} from 'lucide-react';
import { Button, Badge } from '../../../components/ui/Elements';
import { Collection } from '../../../types';
import { APP_CONFIG } from '../../../config/app.config';

interface ApiDocsModalProps {
  isOpen: boolean;
  onClose: () => void;
  collection?: Collection;
  context?: 'collection' | 'users';
}

// --- SUB-COMPONENTS ---

const CodeBlock = ({ label, code, className = "" }: { label: string, code: string, className?: string }) => {
  const [copied, setCopied] = useState(false);

  const handleCopy = () => {
    navigator.clipboard.writeText(code);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div className={`rounded-lg border border-border bg-[#0d1117] overflow-hidden my-4 group relative shadow-sm ${className}`}>
      <div className="flex items-center justify-between px-3 py-2 bg-[#161b22] border-b border-white/5">
        <span className="text-[10px] font-mono text-muted-foreground uppercase tracking-wider font-semibold">{label}</span>
        <button
            onClick={handleCopy}
            className="text-gray-400 hover:text-white transition-colors p-1 hover:bg-white/10 rounded"
            title="Copy"
        >
            {copied ? <Check className="h-3.5 w-3.5 text-emerald-400" /> : <Copy className="h-3.5 w-3.5" />}
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

const SectionHeader = ({ title, icon: Icon, description }: { title: string, icon: any, description: string }) => (
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

// --- MAIN COMPONENT ---

export const ApiDocsModal = ({ isOpen, onClose, collection }: ApiDocsModalProps) => {
  const [activeTab, setActiveTab] = useState('setup');
  const [isMobileMenuOpen, setIsMobileMenuOpen] = useState(false);

  if (!isOpen) return null;

  const colName = collection?.name || 'posts';
  
  // Detect Environment for Setup Params
  const path = window.location.pathname;
  let initCode = `import { ApexKit } from 'apexkit-sdk';\n\nconst pb = new ApexKit('${APP_CONFIG.apiBaseUrl}');`;
  
  if (path.includes('/tenant/')) {
      const tenantId = path.split('/tenant/')[1].split('/')[0];
      initCode += `\n\n// Target Specific Tenant\nconst client = pb.tenant('${tenantId}');`;
  } else if (path.includes('/sandbox/')) {
      const sandboxId = path.split('/sandbox/')[1].split('/')[0];
      initCode += `\n\n// Target Sandbox Session\nconst client = pb.sandbox('${sandboxId}');`;
  } else {
      initCode += `\nconst client = pb;`;
  }
  initCode += `\n\nawait client.auth.login('user@example.com', 'password');`;

  const sections = [
    { id: 'setup', label: 'Setup & Auth', icon: Server },
    { id: 'records', label: 'CRUD Records', icon: Database },
    { id: 'search', label: 'Search & Vectors', icon: Search },
    { id: 'files', label: 'File Storage', icon: Folder },
    { id: 'ai', label: 'AI Actions', icon: BrainCircuit },
    { id: 'scripts', label: 'Scripts', icon: Terminal },
    { id: 'errors', label: 'Error Handling', icon: ShieldAlert },
  ];

  const activeSection = sections.find(s => s.id === activeTab);

  return createPortal(
    <div className="fixed inset-0 z-[100] flex justify-center items-center isolate">
      {/* Backdrop */}
      <div className="absolute inset-0 bg-black/60 backdrop-blur-sm animate-in fade-in" onClick={onClose} />
      
      {/* Modal Container */}
      <div className="relative w-[95vw] md:w-[90vw] max-w-6xl h-[90vh] md:h-[85vh] bg-background border border-border shadow-2xl animate-in zoom-in-95 duration-200 flex flex-col md:flex-row rounded-xl overflow-hidden">
        
        {/* MOBILE HEADER (Visible only on small screens) */}
        <div className="md:hidden flex items-center justify-between p-4 border-b border-border bg-secondary/5">
            <button onClick={() => setIsMobileMenuOpen(!isMobileMenuOpen)} className="p-2 hover:bg-secondary rounded-md">
                <Menu className="h-5 w-5" />
            </button>
            <span className="font-bold text-sm flex items-center gap-2">
                <Code2 className="h-4 w-4 text-primary" /> SDK Guide
            </span>
            <button onClick={onClose} className="p-2 hover:bg-secondary rounded-md">
                <X className="h-5 w-5" />
            </button>
        </div>

        {/* SIDEBAR NAVIGATION */}
        <div className={`
            absolute md:relative inset-y-0 left-0 z-20 w-64 bg-background/95 md:bg-secondary/5 border-r border-border flex flex-col shrink-0 transition-transform duration-300 transform
            ${isMobileMenuOpen ? 'translate-x-0' : '-translate-x-full'} md:translate-x-0 backdrop-blur-md md:backdrop-blur-none
        `}>
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
                 {sections.map(s => (
                     <button
                        key={s.id}
                        onClick={() => { setActiveTab(s.id); setIsMobileMenuOpen(false); }}
                        className={`w-full text-left px-3 py-2.5 rounded-md text-sm font-medium flex items-center gap-3 transition-colors ${activeTab === s.id ? 'bg-primary/10 text-primary border border-primary/20' : 'hover:bg-secondary text-muted-foreground hover:text-foreground border border-transparent'}`}
                     >
                         <s.icon className={`h-4 w-4 ${activeTab === s.id ? 'text-primary' : 'opacity-70'}`} /> 
                         {s.label}
                     </button>
                 ))}
             </div>
        </div>

        {/* MOBILE OVERLAY BACKDROP */}
        {isMobileMenuOpen && (
            <div className="absolute inset-0 bg-black/50 z-10 md:hidden" onClick={() => setIsMobileMenuOpen(false)} />
        )}

        {/* MAIN CONTENT */}
        <div className="flex-1 flex flex-col relative bg-background overflow-hidden">
             {/* Desktop Close Button */}
             <div className="absolute top-4 right-4 z-10 hidden md:block">
                 <Button size="icon" variant="ghost" onClick={onClose} className="rounded-full h-8 w-8 hover:bg-secondary">
                     <X className="h-4 w-4" />
                 </Button>
             </div>
             
             <div className="flex-1 overflow-y-auto p-6 md:p-10 custom-scrollbar">
                 <div className="max-w-3xl mx-auto">
                 
                     {/* SETUP */}
                     {activeTab === 'setup' && (
                         <div className="animate-in fade-in slide-in-from-bottom-2 duration-300">
                             <SectionHeader title="Initialization" icon={Server} description="Install the SDK and initialize the client. It automatically handles context switching for Tenants." />
                             
                             <div className="space-y-6">
                                <CodeBlock label="Install via NPM" code="npm install apexkit-sdk" />
                                <CodeBlock label="main.ts" code={initCode} />
                             </div>
                         </div>
                     )}

                     {/* RECORDS */}
                     {activeTab === 'records' && (
                         <div className="animate-in fade-in slide-in-from-bottom-2 duration-300 space-y-10">
                             <SectionHeader title="Data Operations" icon={Database} description={`Basic CRUD operations for the '${colName}' collection.`} />
                             
                             <div className="grid grid-cols-1 gap-8">
                                 <div>
                                     <h5 className="font-bold text-sm mb-2 flex items-center gap-2"><div className="w-1.5 h-1.5 rounded-full bg-blue-500"></div> List & Filter</h5>
                                     <p className="text-xs text-muted-foreground mb-3">Fetch records with pagination, sorting, and mongo-style filtering.</p>
                                     <CodeBlock label="JS" code={`
// List with pagination and sort
const res = await client.collection('${colName}').list({
    page: 1,
    per_page: 20,
    sort: '-created',
    filter: { status: 'published' },
    expand: 'author,comments' // Optional relations
});

console.log(res.items); // Array of records
console.log(res.total); // Total count
                                     `} />
                                 </div>

                                 <div>
                                     <h5 className="font-bold text-sm mb-2 flex items-center gap-2"><div className="w-1.5 h-1.5 rounded-full bg-green-500"></div> Create & Update</h5>
                                     <CodeBlock label="JS" code={`
// Create
const newRecord = await client.collection('${colName}').create({
    title: "New Item",
    active: true
});

// Update
await client.collection('${colName}').update(newRecord.id, {
    title: "Updated Title"
});

// Delete
await client.collection('${colName}').delete(newRecord.id);
                                     `} />
                                 </div>
                             </div>
                         </div>
                     )}

                     {/* SEARCH & VECTORS */}
                     {activeTab === 'search' && (
                         <div className="animate-in fade-in slide-in-from-bottom-2 duration-300 space-y-10">
                             <SectionHeader title="Search Engine" icon={Search} description="High-performance full-text and semantic search capabilities." />

                             <div className="space-y-8">
                                <div className="p-4 rounded-xl border border-purple-500/20 bg-purple-500/5">
                                    <div className="flex items-center justify-between mb-3">
                                        <div className="flex items-center gap-2">
                                            <Badge variant="outline" className="bg-purple-500/10 text-purple-500 border-purple-500/20">Semantic</Badge>
                                            <h5 className="font-bold text-sm">Vector Search</h5>
                                        </div>
                                    </div>
                                    <p className="text-xs text-muted-foreground mb-4 leading-relaxed">
                                        Automatically converts query text to embeddings using the configured AI model (e.g., MiniLM, BGE) and finds conceptually similar records.
                                        Requires fields to have <code>vectorize: true</code> in schema.
                                    </p>
                                    <CodeBlock label="JS" code={`
// Find records similar to the MEANING of the query
const results = await client.collection('${colName}').searchTextVector(
    "stories about space exploration", 
    5 // Limit
);

results.forEach(rec => {
    // Results are sorted by similarity score (descending)
    console.log(rec.id, rec.data.title);
});
                                    `} />
                                </div>

                                <div className="p-4 rounded-xl border border-blue-500/20 bg-blue-500/5">
                                    <div className="flex items-center justify-between mb-3">
                                        <div className="flex items-center gap-2">
                                            <Badge variant="outline" className="bg-blue-500/10 text-blue-500 border-blue-500/20">Fast</Badge>
                                            <h5 className="font-bold text-sm">Instant Search (Tantivy)</h5>
                                        </div>
                                    </div>
                                    <p className="text-xs text-muted-foreground mb-4 leading-relaxed">
                                        Typo-tolerant, ultra-fast full text search. Returns snippets with HTML highlights <code>&lt;b&gt;match&lt;/b&gt;</code>.
                                        Requires fields to have <code>indexed: true</code>.
                                    </p>
                                    <CodeBlock label="JS" code={`
// "Harry Pottr" -> Matches "Harry Potter" (Typo tolerance)
const hits = await client.collection('${colName}').instantSearch("harry pottr", 10);

console.log(hits[0].snippet); 
// Output: { title: "<b>Harry Potter</b> and the Stone..." }
                                    `} />
                                </div>
                             </div>
                         </div>
                     )}

                     {/* FILES */}
                     {activeTab === 'files' && (
                         <div className="animate-in fade-in slide-in-from-bottom-2 duration-300 space-y-8">
                             <SectionHeader title="File Storage" icon={Folder} description="Upload and manage assets (Images, Documents)." />

                             <div className="grid grid-cols-1 gap-6">
                                 <div>
                                     <h5 className="font-semibold text-sm mb-2">Upload Workflow</h5>
                                     <p className="text-xs text-muted-foreground mb-3">Files are uploaded directly to the storage backend (Local/S3). The returned file object contains the public URL.</p>
                                     <CodeBlock label="JS" code={`
const fileInput = document.getElementById('my-file');
const file = fileInput.files[0];

// Upload
const uploaded = await client.files.upload(file);

console.log("File ID:", uploaded.id);
console.log("URL:", uploaded.url); // Use this url in your <img> tags

// Link to a record
await client.collection('${colName}').create({
    title: "Profile",
    avatar: uploaded.filename // Store the filename/key reference
});
                                     `} />
                                 </div>
                             </div>
                         </div>
                     )}

                     {/* AI ACTIONS */}
                     {activeTab === 'ai' && (
                         <div className="animate-in fade-in slide-in-from-bottom-2 duration-300 space-y-6">
                             <SectionHeader title="AI Actions" icon={BrainCircuit} description="Execute server-side LLM prompts securely without exposing API keys." />
                             
                             <CodeBlock label="JS" code={`
// Run a pre-defined prompt action (configured in Admin > AI Actions)
// e.g. Slug: 'summarize-content'
const result = await client.ai.run('summarize-content', {
    text: "Long article content here...",
    tone: "professional"
});

console.log(result.result); // The AI output text
console.log(result.metadata); // Grounding/Citation data
                             `} />
                         </div>
                     )}

                     {/* SCRIPTS */}
                     {activeTab === 'scripts' && (
                         <div className="animate-in fade-in slide-in-from-bottom-2 duration-300 space-y-6">
                             <SectionHeader title="Server Scripts" icon={Terminal} description="Run custom backend logic via API endpoints." />
                             
                             <CodeBlock label="JS" code={`
// Execute a script defined in Admin > Scripts
// e.g. Script Name: 'calculate-stats'
const response = await client.scripts.run('calculate-stats', {
    startDate: '2023-01-01',
    category: 'sales'
});

// The structure depends on what your script returns
console.log(response); 
                             `} />
                         </div>
                     )}

                     {/* ERRORS */}
                     {activeTab === 'errors' && (
                         <div className="animate-in fade-in slide-in-from-bottom-2 duration-300 space-y-8">
                             <SectionHeader title="Error Handling" icon={AlertTriangle} description="Standardized JSON error responses." />

                             <div className="grid gap-6">
                                <div>
                                    <h5 className="font-bold text-sm mb-3">Response Format</h5>
                                    <CodeBlock label="JSON" code={`
{
  "error": "not_found",           // Short code
  "message": "Record not found",  // Human readable
  "status": 404,                  // HTTP Status
  "details": { ... }              // Optional validation details
}
                                    `} />
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
                                                    <td className="px-4 py-3 text-muted-foreground">Invalid input/JSON syntax.</td>
                                                </tr>
                                                <tr>
                                                    <td className="px-4 py-3 font-mono text-xs">401</td>
                                                    <td className="px-4 py-3">unauthorized</td>
                                                    <td className="px-4 py-3 text-muted-foreground">Missing or invalid Auth Token.</td>
                                                </tr>
                                                <tr>
                                                    <td className="px-4 py-3 font-mono text-xs">403</td>
                                                    <td className="px-4 py-3">forbidden</td>
                                                    <td className="px-4 py-3 text-muted-foreground">Authenticated, but policy denied access.</td>
                                                </tr>
                                                <tr>
                                                    <td className="px-4 py-3 font-mono text-xs">404</td>
                                                    <td className="px-4 py-3">not_found</td>
                                                    <td className="px-4 py-3 text-muted-foreground">Resource (Collection/Record) does not exist.</td>
                                                </tr>
                                                <tr>
                                                    <td className="px-4 py-3 font-mono text-xs">422</td>
                                                    <td className="px-4 py-3">validation_error</td>
                                                    <td className="px-4 py-3 text-muted-foreground">Schema constraint violation (e.g. required field missing).</td>
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