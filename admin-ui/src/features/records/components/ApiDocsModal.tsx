
import React, { useState } from 'react';
import { createPortal } from 'react-dom';
import { X, Copy, Check, Terminal, Code2, Box, ArrowDownLeft, AlertTriangle } from 'lucide-react';
import { Button, Badge } from '../../../components/form/FormPrimitives';
import { Collection, SchemaField } from '../../../types';
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
    <div className={`relative rounded-lg border border-border bg-[#1e1e1e] overflow-hidden my-4 group ${className}`}>
      {label && (
        <div className="flex items-center justify-between px-4 py-2 bg-[#2d2d2d] border-b border-white/10">
          <span className="text-xs font-mono text-muted-foreground">{label}</span>
        </div>
      )}
      <div className="p-4 overflow-x-auto custom-scrollbar">
        <pre className="text-sm font-mono text-[#d4d4d4] leading-relaxed">
          <code>{code}</code>
        </pre>
      </div>
      <button
        onClick={handleCopy}
        className="absolute top-3 right-3 p-1.5 rounded-md bg-white/10 hover:bg-white/20 text-white opacity-0 group-hover:opacity-100 transition-opacity"
        title="Copy to clipboard"
      >
        {copied ? <Check className="h-4 w-4 text-emerald-400" /> : <Copy className="h-4 w-4" />}
      </button>
    </div>
  );
};

export const ApiDocsModal = ({ isOpen, onClose, collection }: ApiDocsModalProps) => {
  const [activeTab, setActiveTab] = useState<'js' | 'ts'>('js');

  if (!isOpen) return null;

  // --- Generators ---

  const generateTsInterface = (col: Collection) => {
    const typeMap: Record<string, string> = {
      text: 'string',
      number: 'number',
      bool: 'boolean',
      email: 'string',
      url: 'string',
      date: 'string',
      select: 'string',
      json: 'any',
      file: 'string', // filename
      relation: 'string', // record ID
    };

    const fields = col.schema.map(f => {
      const type = typeMap[f.type] || 'string';
      const optional = !f.required ? '?' : '';
      return `  ${f.name}${optional}: ${type};`;
    }).join('\n');

    return `// Typescript Interface for ${col.name}
export interface ${col.name.charAt(0).toUpperCase() + col.name.slice(1)}Record {
  id: string;
  created: string;
  updated: string;
  collectionId: string;
  collectionName: string;
${fields}
}`;
  };

  const generateExampleData = (col: Collection) => {
    const data: any = {};
    col.schema.forEach(f => {
      if (f.type === 'bool') data[f.name] = true;
      else if (f.type === 'number') data[f.name] = 123;
      else if (f.type === 'json') data[f.name] = { key: "value" };
      else if (f.type === 'relation') data[f.name] = "RELATION_ID";
      else data[f.name] = `test_${f.name}`;
    });
    return JSON.stringify(data, null, 2).replace(/"([^"]+)":/g, '$1:');
  };

  const generateMockRecord = (col: Collection) => {
     const record: any = {
        id: "RECORD_ID_123",
        collectionId: col.id,
        collectionName: col.name,
        created: "2023-10-27 10:00:00.123Z",
        updated: "2023-10-27 10:00:00.123Z",
     };
     col.schema.forEach(f => {
        if (f.type === 'bool') record[f.name] = true;
        else if (f.type === 'number') record[f.name] = 123;
        else if (f.type === 'json') record[f.name] = { key: "value" };
        else if (f.type === 'relation') record[f.name] = "REL_123";
        else if (f.type === 'file') record[f.name] = "image.jpg";
        else record[f.name] = `test_${f.name}`;
     });
     return record;
  };

  const mockRecord = generateMockRecord(collection);

  const sdkInit = `import TinyBase from 'tinybase-sdk';

const client = new TinyBase('${APP_CONFIG.apiBaseUrl}');`;

  const tabs = [
    { 
        id: 'list', 
        label: 'List Records', 
        code: `// Fetch a paginated list of '${collection.name}'\nconst result = await client.collection('${collection.name}').getList(1, 20, {\n  sort: '-created',\n  filter: 'created > "2023-01-01"'\n});`,
        response: JSON.stringify({
            page: 1,
            perPage: 20,
            totalItems: 42,
            totalPages: 3,
            items: [mockRecord]
        }, null, 2)
    },
    { 
        id: 'view', 
        label: 'View Record', 
        code: `// Fetch a single record by ID\nconst record = await client.collection('${collection.name}').getOne('RECORD_ID_123');`,
        response: JSON.stringify(mockRecord, null, 2)
    },
    { 
        id: 'create', 
        label: 'Create Record', 
        code: `// Create a new record\nconst data = ${generateExampleData(collection)};\n\nconst record = await client.collection('${collection.name}').create(data);`,
        response: JSON.stringify(mockRecord, null, 2)
    },
    { 
        id: 'update', 
        label: 'Update Record', 
        code: `// Update an existing record\nconst record = await client.collection('${collection.name}').update('RECORD_ID_123', {\n  someField: 'newValue'\n});`,
        response: JSON.stringify(mockRecord, null, 2)
    },
    { 
        id: 'delete', 
        label: 'Delete Record', 
        code: `// Delete a record\nawait client.collection('${collection.name}').delete('RECORD_ID_123');`,
        response: "true"
    },
  ];

  return createPortal(
    <div className="fixed inset-0 z-[100] flex justify-end isolate">
      <div className="absolute inset-0 bg-black/40 backdrop-blur-[2px] animate-in fade-in" onClick={onClose} />
      <div className="relative w-full h-full md:max-w-2xl bg-background border-l border-border shadow-2xl animate-in slide-in-from-right duration-300 flex flex-col">
        
        {/* Header */}
        <div className="flex items-center justify-between p-6 border-b bg-secondary/5 safe-top">
          <div className="space-y-1">
            <div className="flex items-center gap-2">
              <Badge variant="outline" className="font-mono text-xs uppercase">API</Badge>
              <h2 className="text-xl font-bold">Documentation</h2>
            </div>
            <p className="text-sm text-muted-foreground flex items-center gap-2">
              Integration guide for <span className="font-bold text-foreground bg-secondary/50 px-1.5 rounded">{collection.name}</span>
            </p>
          </div>
          <Button size="icon" variant="ghost" onClick={onClose}><X className="h-5 w-5" /></Button>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-y-auto p-6 space-y-8 custom-scrollbar">
          
          {/* Quick Setup */}
          <section className="space-y-3">
            <h3 className="text-lg font-semibold flex items-center gap-2"><Terminal className="h-4 w-4 text-primary" /> Setup</h3>
            <p className="text-sm text-muted-foreground">Install the official JavaScript SDK to interact with your data.</p>
            <CodeBlock code="npm install tinybase-sdk" label="Terminal" />
            <CodeBlock code={sdkInit} label="Initialization" />
          </section>

          {/* Schema Definition */}
          <section className="space-y-3">
             <div className="flex items-center justify-between">
               <h3 className="text-lg font-semibold flex items-center gap-2"><Box className="h-4 w-4 text-primary" /> Type Definitions</h3>
               <div className="flex bg-secondary/20 p-1 rounded-lg">
                  <button onClick={() => setActiveTab('js')} className={`px-3 py-1 text-xs font-medium rounded-md transition-all ${activeTab === 'js' ? 'bg-background shadow text-foreground' : 'text-muted-foreground hover:text-foreground'}`}>JSDoc</button>
                  <button onClick={() => setActiveTab('ts')} className={`px-3 py-1 text-xs font-medium rounded-md transition-all ${activeTab === 'ts' ? 'bg-background shadow text-blue-500' : 'text-muted-foreground hover:text-foreground'}`}>TypeScript</button>
               </div>
             </div>
             
             {activeTab === 'ts' ? (
                <CodeBlock code={generateTsInterface(collection)} label={`${collection.name}.d.ts`} />
             ) : (
                <CodeBlock 
                  code={`/**\n * @typedef {Object} ${collection.name.charAt(0).toUpperCase() + collection.name.slice(1)}Record\n${collection.schema.map(f => ` * @property {${f.type === 'bool' ? 'boolean' : f.type === 'number' ? 'number' : 'string'}} ${!f.required ? '[' + f.name + ']' : f.name}`).join('\n')}\n */`} 
                  label="JSDoc"
                />
             )}
          </section>

          {/* CRUD Examples */}
          <section className="space-y-6">
            <h3 className="text-lg font-semibold flex items-center gap-2"><Code2 className="h-4 w-4 text-primary" /> Examples</h3>
            <div className="grid gap-10">
              {tabs.map((tab) => (
                <div key={tab.id} className="relative">
                  <div className="flex items-center gap-2 mb-2">
                      <h4 className="text-sm font-bold text-foreground">{tab.label}</h4>
                      <div className="h-px bg-border flex-1"></div>
                  </div>
                  
                  <div className="space-y-2">
                      <span className="text-[10px] uppercase font-bold text-muted-foreground tracking-wider ml-1">Request</span>
                      <CodeBlock code={tab.code} className="my-1 border-l-4 border-l-primary/50" />
                  </div>
                  
                  <div className="flex justify-center my-2">
                      <ArrowDownLeft className="h-4 w-4 text-muted-foreground opacity-50" />
                  </div>

                  <div className="space-y-2">
                      <span className="text-[10px] uppercase font-bold text-muted-foreground tracking-wider ml-1">Response</span>
                      <CodeBlock code={tab.response} className="my-1 border-l-4 border-l-emerald-500/50" />
                  </div>
                </div>
              ))}
            </div>
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
        <div className="p-4 border-t bg-background safe-bottom">
           <Button variant="outline" className="w-full" onClick={onClose}>Close Documentation</Button>
        </div>

      </div>
    </div>,
    document.body
  );
};
