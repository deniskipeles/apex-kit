import React, { useState, useMemo } from 'react';
import { createPortal } from 'react-dom';
import { X, Copy, Check, Terminal, Code2, Box, ArrowDownLeft, AlertTriangle, Globe, Database } from 'lucide-react';
import { Button, Badge } from '../../../components/ui/Elements';
import { Collection } from '../../../types';
import { APP_CONFIG } from '../../../config/app.config';

interface ApiDocsModalProps {
  isOpen: boolean;
  onClose: () => void;
  collection: Collection;
}

const CodeBlock = ({ code, label, className = '' }: { code: string; label?: string; className?: string }) => {
  const [copied, setCopied] = useState(false);

  const handleCopy = () => {
    navigator.clipboard.writeText(code);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div className={`relative rounded-lg border border-border bg-[#0d1117] overflow-hidden my-4 group shadow-sm ${className}`}>
      {label && (
        <div className="flex items-center justify-between px-4 py-2 bg-[#161b22] border-b border-white/5">
          <span className="text-[10px] font-mono text-muted-foreground uppercase tracking-wider">{label}</span>
          <div className="flex gap-1.5">
              <div className="w-2.5 h-2.5 rounded-full bg-red-500/20"></div>
              <div className="w-2.5 h-2.5 rounded-full bg-yellow-500/20"></div>
              <div className="w-2.5 h-2.5 rounded-full bg-green-500/20"></div>
          </div>
        </div>
      )}
      <div className="p-4 overflow-x-auto custom-scrollbar">
        <pre className="text-xs sm:text-sm font-mono text-[#e6edf3] leading-relaxed whitespace-pre">
          <code>{code}</code>
        </pre>
      </div>
      <button
        onClick={handleCopy}
        className="absolute top-10 right-3 p-2 rounded-md bg-white/5 hover:bg-white/10 text-gray-400 hover:text-white transition-all opacity-0 group-hover:opacity-100 focus:opacity-100"
        title="Copy to clipboard"
      >
        {copied ? <Check className="h-3.5 w-3.5 text-emerald-400" /> : <Copy className="h-3.5 w-3.5" />}
      </button>
    </div>
  );
};

export const ApiDocsModal = ({ isOpen, onClose, collection }: ApiDocsModalProps) => {
  const [activeTab, setActiveTab] = useState<'js' | 'ts' | 'curl'>('js');

  // --- Context Detection ---
  const context = useMemo(() => {
    const path = window.location.pathname;
    const tenantMatch = path.match(/^\/_dashboard\/tenant\/([^/]+)/);
    const sandboxMatch = path.match(/^\/_dashboard\/sandbox\/([^/]+)/);
    
    if (tenantMatch) return { type: 'tenant', id: tenantMatch[1], label: 'Tenant' };
    if (sandboxMatch) return { type: 'sandbox', id: sandboxMatch[1], label: 'Sandbox' };
    return { type: 'root', id: null, label: 'Root' };
  }, []);

  if (!isOpen) return null;

  // --- Generators ---

  const generateBaseUrl = () => {
      let base = APP_CONFIG.apiBaseUrl;
      if (context.type === 'tenant') base += `/tenant/${context.id}/api/v1`;
      else if (context.type === 'sandbox') base += `/sandbox/${context.id}/api/v1`;
      else base += `/api/v1`;
      return base;
  };

  const generateSdkInit = () => {
    let init = `import { PowerBase } from 'tinybase-sdk';\n\nconst pb = new PowerBase('${APP_CONFIG.apiBaseUrl}');`;
    
    if (context.type === 'tenant') {
        init += `\n\n// Target Specific Tenant\nconst client = pb.tenant('${context.id}');`;
    } else if (context.type === 'sandbox') {
        init += `\n\n// Target Sandbox Session\nconst client = pb.sandbox('${context.id}');`;
    } else {
        init += `\nconst client = pb;`;
    }
    
    init += `\n\nawait client.auth.login('user@example.com', 'password');`;
    return init;
  };

  const generateTsInterface = (col: Collection) => {
    const typeMap: Record<string, string> = {
      text: 'string', number: 'number', bool: 'boolean', email: 'string',
      url: 'string', date: 'string', select: 'string', json: 'any',
      file: 'string', relation: 'string', owner: 'string'
    };

    const fields = col.schema.map(f => {
      const type = typeMap[f.type] || 'string';
      const optional = !f.required ? '?' : '';
      return `  ${f.name}${optional}: ${type};`;
    }).join('\n');

    return `export interface ${col.name.charAt(0).toUpperCase() + col.name.slice(1)} {\n  id: string;\n  created: string;\n  updated: string;\n${fields}\n}`;
  };

  const generateExampleData = (col: Collection) => {
    const data: any = {};
    col.schema.forEach(f => {
      if (f.type === 'bool') data[f.name] = true;
      else if (f.type === 'number') data[f.name] = 123;
      else if (f.type === 'json') data[f.name] = { key: "value" };
      else if (f.type === 'relation') data[f.name] = "REL_ID";
      else data[f.name] = `${f.name}_value`;
    });
    return JSON.stringify(data, null, 2); // .replace(/"([^"]+)":/g, '$1:');
  };

  const mockRecord = { id: "REC_123", ...JSON.parse(generateExampleData(collection)), created: "2023-10-27T10:00:00Z" };

  const tabs = [
    { 
        id: 'list', 
        label: 'List', 
        js: `const result = await client.collection('${collection.name}').list({\n  page: 1,\n  per_page: 20,\n  sort: '-created'\n});`,
        curl: `curl -X GET "${generateBaseUrl()}/collections/${collection.name}/records?page=1&sort=-created" \\\n  -H "Authorization: Bearer YOUR_TOKEN"`
    },
    { 
        id: 'create', 
        label: 'Create', 
        js: `const data = ${generateExampleData(collection)};\n\nconst record = await client.collection('${collection.name}').create(data);`,
        curl: `curl -X POST "${generateBaseUrl()}/collections/${collection.name}/records" \\\n  -H "Authorization: Bearer YOUR_TOKEN" \\\n  -H "Content-Type: application/json" \\\n  -d '{"data": ${generateExampleData(collection)}}'`
    },
    { 
        id: 'view', 
        label: 'Read', 
        js: `const record = await client.collection('${collection.name}').get('REC_123');`,
        curl: `curl -X GET "${generateBaseUrl()}/collections/${collection.name}/records/REC_123" \\\n  -H "Authorization: Bearer YOUR_TOKEN"`
    },
    { 
        id: 'update', 
        label: 'Update', 
        js: `const record = await client.collection('${collection.name}').update('REC_123', {\n  someField: 'newValue'\n});`,
        curl: `curl -X PATCH "${generateBaseUrl()}/collections/${collection.name}/records/REC_123" \\\n  -H "Authorization: Bearer YOUR_TOKEN" \\\n  -H "Content-Type: application/json" \\\n  -d '{"data": {"someField": "newValue"}}'`
    },
    { 
        id: 'delete', 
        label: 'Delete', 
        js: `await client.collection('${collection.name}').delete('REC_123');`,
        curl: `curl -X DELETE "${generateBaseUrl()}/collections/${collection.name}/records/REC_123" \\\n  -H "Authorization: Bearer YOUR_TOKEN"`
    },
  ];

  return createPortal(
    <div className="fixed inset-0 z-[100] flex justify-end isolate">
      <div className="absolute inset-0 bg-black/60 backdrop-blur-sm animate-in fade-in" onClick={onClose} />
      
      <div className="relative w-full h-full md:max-w-2xl bg-background border-l border-border shadow-2xl animate-in slide-in-from-right duration-300 flex flex-col">
        
        {/* Header */}
        <div className="flex items-center justify-between p-6 border-b border-border/50 bg-secondary/5 shrink-0">
          <div className="space-y-1">
            <div className="flex items-center gap-2">
              <div className={`p-1.5 rounded-md ${context.type === 'sandbox' ? 'bg-amber-500/10 text-amber-500' : 'bg-primary/10 text-primary'}`}>
                 <Terminal className="h-4 w-4" />
              </div>
              <h2 className="text-xl font-bold tracking-tight">API Documentation</h2>
            </div>
            <div className="flex items-center gap-2 text-sm text-muted-foreground">
               <Globe className="h-3 w-3" />
               <span>{context.label} Environment:</span>
               <code className="bg-secondary px-1.5 py-0.5 rounded text-xs font-mono text-foreground">
                   {context.id || 'Production'}
               </code>
            </div>
          </div>
          <Button size="icon" variant="ghost" onClick={onClose} className="rounded-full hover:bg-secondary"><X className="h-5 w-5" /></Button>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-y-auto p-6 space-y-8 custom-scrollbar">
          
          {/* Quick Setup */}
          <section className="space-y-4">
            <div className="flex items-center gap-2">
                <div className="h-6 w-1 bg-primary rounded-full"></div>
                <h3 className="text-lg font-semibold">SDK Setup</h3>
            </div>
            <p className="text-sm text-muted-foreground">Install the official JavaScript SDK to interact with your data.</p>
            <div className="grid gap-2">
                 <CodeBlock code="npm install tinybase-sdk" label="Terminal" />
                 <CodeBlock code={generateSdkInit()} label="Initialize Client" />
            </div>
          </section>

          {/* Type Definitions */}
          <section className="space-y-4">
             <div className="flex items-center justify-between">
                <div className="flex items-center gap-2">
                    <div className="h-6 w-1 bg-blue-500 rounded-full"></div>
                    <h3 className="text-lg font-semibold">Type Definitions</h3>
                </div>
                <div className="flex bg-secondary/30 p-1 rounded-lg">
                  <button onClick={() => setActiveTab('js')} className={`px-3 py-1 text-xs font-medium rounded-md transition-all ${activeTab === 'js' ? 'bg-background shadow text-foreground' : 'text-muted-foreground hover:text-foreground'}`}>JS</button>
                  <button onClick={() => setActiveTab('ts')} className={`px-3 py-1 text-xs font-medium rounded-md transition-all ${activeTab === 'ts' ? 'bg-background shadow text-blue-500' : 'text-muted-foreground hover:text-foreground'}`}>TS</button>
                  <button onClick={() => setActiveTab('curl')} className={`px-3 py-1 text-xs font-medium rounded-md transition-all ${activeTab === 'curl' ? 'bg-background shadow text-orange-500' : 'text-muted-foreground hover:text-foreground'}`}>cURL</button>
               </div>
             </div>
             
             {activeTab === 'ts' && (
                <CodeBlock code={generateTsInterface(collection)} label={`${collection.name}.d.ts`} />
             )}
             {activeTab === 'js' && (
                <CodeBlock 
                  code={`/**\n * @typedef {Object} ${collection.name}\n${collection.schema.map(f => ` * @property {${f.type}} ${f.name}`).join('\n')}\n */`} 
                  label="JSDoc"
                />
             )}
             {activeTab === 'curl' && (
                 <div className="p-4 rounded-lg bg-secondary/10 border border-border text-sm text-muted-foreground italic flex gap-2">
                     <AlertTriangle className="h-4 w-4" /> Types not applicable for raw HTTP requests. See examples below.
                 </div>
             )}
          </section>

          {/* CRUD Examples */}
          <section className="space-y-6">
            <div className="flex items-center gap-2">
                <div className="h-6 w-1 bg-emerald-500 rounded-full"></div>
                <h3 className="text-lg font-semibold">Endpoints & Examples</h3>
            </div>
            
            <div className="grid gap-8">
              {tabs.map((tab) => (
                <div key={tab.id} className="relative group">
                  <div className="flex items-center gap-2 mb-3">
                      <Badge variant="outline" className="font-mono text-xs uppercase bg-secondary/20">{tab.label}</Badge>
                      <div className="h-px bg-border flex-1"></div>
                  </div>
                  
                  {activeTab !== 'curl' ? (
                      <div className="grid gap-2">
                         <CodeBlock code={tab.js} className="my-0 border-l-2 border-l-primary/50" />
                      </div>
                  ) : (
                      <CodeBlock code={tab.curl} className="my-0 border-l-2 border-l-orange-500/50" />
                  )}
                </div>
              ))}
            </div>
          </section>

          {/* Response Format */}
          <section className="space-y-4">
             <div className="flex items-center gap-2">
                <div className="h-6 w-1 bg-purple-500 rounded-full"></div>
                <h3 className="text-lg font-semibold">Response Object</h3>
            </div>
            <CodeBlock code={JSON.stringify(mockRecord, null, 2)} label="JSON" />
          </section>

          {/* Error Handling */}
          <section className="space-y-6">
            <h3 className="text-lg font-semibold flex items-center gap-2"><AlertTriangle className="h-4 w-4 text-destructive" /> Error Handling</h3>
            <p className="text-sm text-muted-foreground">API errors are returned with a consistent JSON structure.</p>
            
            <div className="grid gap-6 md:grid-cols-2">
              <div>
                <h4 className="text-xs font-bold text-muted-foreground uppercase tracking-wider mb-2">Standard Error</h4>
                <CodeBlock 
                  code={`{\n  "code": 400,\n  "message": "Something went wrong.",\n  "data": {} \n}`} 
                  className="my-0"
                />
              </div>
              <div>
                <h4 className="text-xs font-bold text-muted-foreground uppercase tracking-wider mb-2">404 Not Found</h4>
                <CodeBlock 
                  code={`{\n  "code": 404,\n  "message": "The requested resource wasn't found.",\n  "data": {}\n}`} 
                  className="my-0"
                />
              </div>
              <div className="md:col-span-2">
                <h4 className="text-xs font-bold text-muted-foreground uppercase tracking-wider mb-2">400 Validation Error</h4>
                <CodeBlock 
                  code={`{\n  "code": 400,\n  "message": "Failed to create record.",\n  "data": {\n    "${collection.schema[0]?.name || 'field_name'}": {\n      "code": "validation_required",\n      "message": "This field is required."\n    }\n  }\n}`} 
                  className="my-0 border-l-4 border-l-destructive/50"
                />
              </div>
            </div>
          </section>

        </div>
        
        {/* Footer */}
        <div className="p-4 border-t border-border bg-background shrink-0">
           <Button variant="outline" className="w-full h-11" onClick={onClose}>Close Documentation</Button>
        </div>

      </div>
    </div>,
    document.body
  );
};