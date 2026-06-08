import React, { useState, useEffect } from 'react';
import { createPortal } from 'react-dom';
import {
  X,
  Filter,
  Check,
  Copy,
  Code2,
  Terminal,
  Globe,
  BookOpen,
  Sparkles,
  RefreshCw,
} from 'lucide-react';
import { Button, Input, Select, Badge } from '../../../components/ui/Elements';
import { Collection } from '../../../types';
import { APP_CONFIG } from '../../../config/app.config';

interface RecordFiltersProps {
  isOpen: boolean;
  onClose: () => void;
  collection: Collection | null;
  onApplyFilters: (filters: any) => void;
}

const OPERATORS = [
  { label: 'Equals (=)', value: '$eq' },
  { label: 'Not Equals (!=)', value: '$neq' },
  { label: 'Contains (Text)', value: '$contains' },
  { label: 'Like (Wildcard %)', value: '$like' },
  { label: 'Greater Than (>)', value: '$gt' },
  { label: 'Greater/Equal (>=)', value: '$gte' },
  { label: 'Less Than (<)', value: '$lt' },
  { label: 'Less/Equal (<=)', value: '$lte' },
  { label: 'In List (comma sep)', value: '$in' },
  { label: 'Not In List', value: '$nin' },
];

type CodeFormat = 'sdk' | 'fetch' | 'curl';

export const RecordFilters = ({
  isOpen,
  onClose,
  collection,
  onApplyFilters,
}: RecordFiltersProps) => {
  const [activeFilters, setActiveFilters] = useState<Record<string, { op: string; val: string }>>(
    {}
  );
  const [activeTab, setActiveTab] = useState<'edit' | 'preview'>('edit');
  const [codeFormat, setCodeFormat] = useState<CodeFormat>('sdk');
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    setActiveFilters({});
    setActiveTab('edit');
  }, [collection?.id]);

  const updateFilter = (field: string, key: 'op' | 'val', value: string) => {
    setActiveFilters((prev) => ({
      ...prev,
      [field]: {
        op: key === 'op' ? value : prev[field]?.op || '$eq',
        val: key === 'val' ? value : prev[field]?.val || '',
      },
    }));
  };

  const clearFilter = (field: string) => {
    const next = { ...activeFilters };
    delete next[field];
    setActiveFilters(next);
  };

  const buildQueryObject = () => {
    const mongoQuery: any = {};

    Object.entries(activeFilters).forEach(([field, { op, val }]) => {
      if (val === '' || val === undefined) return;

      let processedVal: any = val;
      const fieldDef = collection?.schema.find((f) => f.name === field);

      if (fieldDef?.type === 'number') {
        processedVal = Number(val);
      } else if (fieldDef?.type === 'bool') {
        processedVal = val === 'true';
      }

      if (op === '$in' || op === '$nin') {
        const arr = String(val)
          .split(',')
          .map((s) => s.trim());
        if (fieldDef?.type === 'number') {
          processedVal = arr.map(Number).filter((n) => !isNaN(n));
        } else {
          processedVal = arr;
        }
      }

      if (op === '$eq') {
        mongoQuery[field] = processedVal;
      } else {
        mongoQuery[field] = { [op]: processedVal };
      }
    });

    return mongoQuery;
  };

  const handleApply = () => {
    onApplyFilters(buildQueryObject());
    onClose();
  };

  const handleCopyCode = (code: string) => {
    navigator.clipboard.writeText(code);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const generateCodeSnippet = () => {
    const colName = collection?.name || 'collection';
    const queryObj = buildQueryObject();
    const prettyQuery = JSON.stringify(queryObj, null, 2);

    switch (codeFormat) {
      case 'fetch':
        return `// Native JavaScript Fetch API Implementation
const token = localStorage.getItem('apex_token');
const queryParams = new URLSearchParams({
  page: '1',
  per_page: '20',
  filter: JSON.stringify(${prettyQuery.replace(/\n/g, '\n  ')})
});

const response = await fetch(\`\${APP_CONFIG.apiBaseUrl}/api/v1/collections/${colName}/records?\${queryParams}\`, {
  method: 'GET',
  headers: {
    'Authorization': \`Bearer \${token}\`,
    'Content-Type': 'application/json'
  }
});

if (!response.ok) {
  throw new Error('Network response was not ok');
}

const result = await response.json();
console.log(result.items); // Array of processed records`;

      case 'curl':
        return `curl -X GET "${APP_CONFIG.apiBaseUrl}/api/v1/collections/${colName}/records?page=1&per_page=20&filter=${encodeURIComponent(JSON.stringify(queryObj))}" \\
  -H "Authorization: Bearer YOUR_ACCESS_TOKEN" \\
  -H "Content-Type: application/json"`;

      case 'sdk':
      default:
        return `import { ApexKit } from '@apexkit/sdk';

const client = new ApexKit('${APP_CONFIG.apiBaseUrl}');

// Fetch paginated records matching active filter parameters
const result = await client.collection('${colName}').list({
  page: 1,
  per_page: 20,
  filter: ${prettyQuery.replace(/\n/g, '\n  ')}
});

console.log(result.items); // Type-safe list items
console.log(result.total); // Total matching item count`;
    }
  };

  if (!isOpen) return null;

  const activeCount = Object.keys(activeFilters).filter((k) => activeFilters[k].val).length;
  const generatedCode = generateCodeSnippet();

  return createPortal(
    <div className="fixed inset-0 z-[60] flex justify-end isolate">
      <div
        className="absolute inset-0 bg-black/40 backdrop-blur-[1px] animate-in fade-in"
        onClick={onClose}
      />
      <div className="relative w-full md:max-w-2xl lg:max-w-4xl h-full bg-background border-l border-border shadow-2xl animate-in slide-in-from-right flex flex-col">
        {/* Header */}
        <div className="flex items-center justify-between p-4 border-b bg-secondary/5 shrink-0">
          <div className="flex items-center gap-2">
            <h2 className="text-lg font-bold flex items-center gap-2">
              <Filter className="h-4 w-4" /> Filter Workspace
            </h2>
            {activeCount > 0 && (
              <Badge variant="primary" className="rounded-full px-2">
                {activeCount}
              </Badge>
            )}
          </div>
          <div className="flex items-center gap-2">
            {/* Mobile View Switcher */}
            <div className="flex md:hidden p-0.5 bg-secondary/30 rounded-lg">
              <button
                onClick={() => setActiveTab('edit')}
                className={`px-3 py-1 rounded text-xs font-semibold ${activeTab === 'edit' ? 'bg-background text-foreground' : 'text-muted-foreground'}`}
              >
                Filters
              </button>
              <button
                onClick={() => setActiveTab('preview')}
                className={`px-3 py-1 rounded text-xs font-semibold ${activeTab === 'preview' ? 'bg-background text-foreground' : 'text-muted-foreground'}`}
              >
                Code Code
              </button>
            </div>
            <Button size="icon" variant="ghost" onClick={onClose}>
              <X className="h-5 w-5" />
            </Button>
          </div>
        </div>

        {/* Workspace Panels */}
        <div className="flex-1 flex overflow-hidden min-h-0">
          {/* LEFT PANEL: Filters (Always visible on desktop, tabbed on mobile) */}
          <div
            className={`flex-1 flex flex-col h-full ${activeTab === 'edit' ? 'flex' : 'hidden md:flex'}`}
          >
            <div className="flex-1 overflow-y-auto p-6 space-y-4 custom-scrollbar border-r border-border/50">
              {collection?.schema.map((field) => {
                const filter = activeFilters[field.name] || { op: '$eq', val: '' };
                const isActive = !!activeFilters[field.name]?.val;

                return (
                  <div
                    key={field.name}
                    className={`p-3 rounded-lg border transition-all duration-200 ${isActive ? 'border-primary/50 bg-primary/5 shadow-sm' : 'border-border bg-card hover:border-primary/20'}`}
                  >
                    <div className="flex items-center justify-between mb-2">
                      <label className="text-sm font-semibold flex items-center gap-2 text-foreground">
                        {field.name}
                        <span
                          className={`text-[10px] font-mono uppercase px-1.5 py-0.5 rounded border ${isActive ? 'bg-background border-primary/20 text-primary' : 'bg-secondary border-transparent text-muted-foreground'}`}
                        >
                          {field.type}
                        </span>
                      </label>
                      {isActive && (
                        <button
                          onClick={() => clearFilter(field.name)}
                          className="text-muted-foreground hover:text-destructive transition-colors p-1 rounded hover:bg-destructive/10"
                        >
                          <X className="h-3 w-3" />
                        </button>
                      )}
                    </div>

                    <div className="flex gap-2">
                      <div className="w-[110px] shrink-0">
                        <Select
                          className="h-9 text-xs font-medium"
                          value={filter.op}
                          onChange={(e: any) => updateFilter(field.name, 'op', e.target.value)}
                        >
                          {OPERATORS.map((op) => (
                            <option key={op.value} value={op.value}>
                              {op.label}
                            </option>
                          ))}
                        </Select>
                      </div>
                      <div className="flex-1">
                        {field.type === 'bool' ? (
                          <Select
                            className="h-9 text-xs"
                            value={filter.val}
                            onChange={(e: any) => updateFilter(field.name, 'val', e.target.value)}
                          >
                            <option value="">-- Any --</option>
                            <option value="true">True</option>
                            <option value="false">False</option>
                          </Select>
                        ) : (
                          <Input
                            className="h-9 text-xs"
                            placeholder={
                              filter.op.includes('in') ? 'e.g. apple, banana' : 'Value...'
                            }
                            value={filter.val}
                            onChange={(e: any) => updateFilter(field.name, 'val', e.target.value)}
                            type={
                              field.type === 'number' && !filter.op.includes('in')
                                ? 'number'
                                : 'text'
                            }
                          />
                        )}
                      </div>
                    </div>
                  </div>
                );
              })}
            </div>
          </div>

          {/* RIGHT PANEL: Live Code Generation */}
          <div
            className={`flex-1 flex flex-col h-full bg-zinc-950 text-zinc-100 ${activeTab === 'preview' ? 'flex' : 'hidden md:flex'}`}
          >
            <div className="p-4 border-b border-white/10 flex items-center justify-between shrink-0 bg-white/5">
              <div className="flex items-center gap-1.5 p-0.5 bg-black/40 rounded-lg border border-white/5">
                <button
                  onClick={() => setCodeFormat('sdk')}
                  className={`px-3 py-1 rounded text-xs font-semibold flex items-center gap-1.5 transition-all ${codeFormat === 'sdk' ? 'bg-[#161b22] text-foreground' : 'text-zinc-400 hover:text-white'}`}
                >
                  <BookOpen className="h-3.5 w-3.5" /> SDK
                </button>
                <button
                  onClick={() => setCodeFormat('fetch')}
                  className={`px-3 py-1 rounded text-xs font-semibold flex items-center gap-1.5 transition-all ${codeFormat === 'fetch' ? 'bg-[#161b22] text-foreground' : 'text-zinc-400 hover:text-white'}`}
                >
                  <Globe className="h-3.5 w-3.5" /> Fetch
                </button>
                <button
                  onClick={() => setCodeFormat('curl')}
                  className={`px-3 py-1 rounded text-xs font-semibold flex items-center gap-1.5 transition-all ${codeFormat === 'curl' ? 'bg-[#161b22] text-foreground' : 'text-zinc-400 hover:text-white'}`}
                >
                  <Terminal className="h-3.5 w-3.5" /> cURL
                </button>
              </div>

              <Button
                variant="ghost"
                size="sm"
                onClick={() => handleCopyCode(generatedCode)}
                className="h-7 text-xs text-zinc-400 hover:text-white hover:bg-white/10"
              >
                {copied ? (
                  <Check className="h-3.5 w-3.5 mr-1 text-green-400" />
                ) : (
                  <Copy className="h-3.5 w-3.5 mr-1" />
                )}
                Copy Code
              </Button>
            </div>

            <div className="flex-1 overflow-auto p-6 font-mono text-xs leading-relaxed custom-scrollbar bg-[#0d1117] text-[#e6edf3]">
              <pre className="whitespace-pre">
                <code>{generatedCode}</code>
              </pre>
            </div>

            <div className="p-3 bg-black/40 border-t border-white/5 text-[10px] text-zinc-500 font-mono flex items-center gap-1.5 justify-end">
              <Sparkles className="h-3 w-3 text-primary animate-pulse" />
              <span>Query structures update in real-time</span>
            </div>
          </div>
        </div>

        {/* Global Footer Actions */}
        <div className="p-4 border-t flex gap-3 bg-background shrink-0 safe-bottom">
          <Button variant="outline" className="flex-1" onClick={() => setActiveFilters({})}>
            Reset All
          </Button>
          <Button className="flex-1" onClick={handleApply}>
            <Check className="mr-2 h-4 w-4" /> Apply Filters
          </Button>
        </div>
      </div>
    </div>,
    document.body
  );
};
