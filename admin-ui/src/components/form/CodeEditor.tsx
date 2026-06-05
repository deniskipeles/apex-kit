import React, { useRef, useState, useEffect } from 'react';
import Editor, { OnMount } from '@monaco-editor/react';
import { Copy, Check, FileJson, FileCode, Wand2 } from 'lucide-react';
import { Button } from '../ui/Elements';
import { Collection } from '../../types';
import { generateTypeScriptDefs } from '../../lib/schemaToTs';

interface CodeEditorProps {
  value: string;
  onChange: (value: string) => void;
  language?: 'javascript' | 'html' | 'json' | 'typescript';
  height?: string;
  readOnly?: boolean;
  label?: string;
  withTypes?: boolean;
  collections?: Collection[];
}

export const CodeEditor = ({
  value,
  onChange,
  language = 'javascript',
  height = '400px',
  readOnly = false,
  label,
  withTypes = false,
  collections = [],
}: CodeEditorProps) => {
  const editorRef = useRef<any>(null);
  const monacoRef = useRef<any>(null);

  // Store disposables for cleanup
  const dynamicLibDisposable = useRef<any>(null);
  const htmlCompletionDisposable = useRef<any>(null); // [NEW] HTML Completion

  const [copied, setCopied] = useState(false);
  const [cursorPos, setCursorPos] = useState({ line: 1, column: 1 });

  const handleEditorDidMount: OnMount = (editor, monacoInstance) => {
    editorRef.current = editor;
    monacoRef.current = monacoInstance;

    // Fix layout issues in modals
    setTimeout(() => {
      editor.layout();
    }, 300);

    editor.onDidChangeCursorPosition((e: any) => {
      setCursorPos({
        line: e.position.lineNumber,
        column: e.position.column,
      });
    });

    // Configure TypeScript/JavaScript settings globally
    monacoInstance.languages.typescript.javascriptDefaults.setDiagnosticsOptions({
      noSemanticValidation: true,
      noSyntaxValidation: false,
    });

    monacoInstance.languages.typescript.typescriptDefaults.setCompilerOptions({
      target: monacoInstance.languages.typescript.ScriptTarget.ES2020,
      allowNonTsExtensions: true,
      moduleResolution: monacoInstance.languages.typescript.ModuleResolutionKind.NodeJs,
    });

    if (withTypes) {
      // 1. Load Static System Types (For JS/TS Mode)
      const baseLibSource = `
                type JsonValue = string | number | boolean | null | { [key: string]: JsonValue } | JsonValue[];
                interface Request { method: string; headers: Headers; body: any; args: any; json(): Promise<any>; text(): Promise<string>; }
                interface ResponseInit { status?: number; headers?: Record<string, string>; }
                declare class Response { constructor(body: any, init?: ResponseInit); }
                interface Headers { get(name: string): string | null; set(name: string, value: string): void; }
                declare const console: { log(...args: any[]): void; error(...args: any[]): void; };
                declare const $http: { get(url: string): Promise<string>; post(url: string, body: object): Promise<string>; };
                declare const $util: { uuid(): string; slugify(text: string): string; hash(text: string, alg: 'sha256' | 'sha512'): string; hmac(text: string, key: string): string; sleep(ms: number): Promise<void>; };
                declare const $ai: { embed(text: string): Promise<number[]>; };
                declare const $env: { get(key: string): Promise<string>; APP_URL: string; };
                declare const $fs: { readText(filename: string): Promise<string>; };
                interface QueryOptions { filter?: string | object; sort?: string; page?: number; per_page?: number; expand?: string; }
                interface CollectionAPI { list(options?: QueryOptions): Promise<{ items: any[], total: number }>; get(id: number | string, options?: { expand?: string }): Promise<any>; create(data: object): Promise<{ id: number }>; update(id: number | string, data: object): Promise<any>; delete(id: number | string): Promise<boolean>; search(query: string): Promise<any[]>; searchVector(field: string, vector: number[], limit?: number): Promise<any[]>; getVector(id: number | string): Promise<any[]>; instantSearch(query: string, limit?: number): Promise<Array<{id: number, score: number, snippet: any}>>; }
                declare const $db: { records: { list(col: string, opts: any): Promise<any>; create(col: string, data: any): Promise<any>; update(col: string, id: number, data: any): Promise<any>; delete(col: string, id: number): Promise<any>; get(col: string, id: number): Promise<any>; search(col: string, query: string): Promise<any>; }; query(q: any): Promise<any>; };
                declare function log(msg: any): void;
            `;
      const baseLibUri = 'ts:filename/apexkit_base.d.ts';
      monacoInstance.languages.typescript.javascriptDefaults.addExtraLib(baseLibSource, baseLibUri);

      // 2. Load Initial Dynamic Types
      updateDynamicTypes(monacoInstance, collections);
    }

    if (language === 'json') {
      monacoInstance.languages.json.jsonDefaults.setDiagnosticsOptions({
        validate: true,
        allowComments: true,
      });
      monacoInstance.languages.json.jsonDefaults.setModeConfiguration({
        ...monacoInstance.languages.json.jsonDefaults.modeConfiguration,
        documentFormattingEdits: true,
      });
    }
  };

  // Helper to inject types & HTML autocompletions
  const updateDynamicTypes = (monaco: any, cols: Collection[]) => {
    if (!withTypes || !monaco) return;

    // A. Inject TS definitions for JS/TS mode
    const dynamicSource = generateTypeScriptDefs(cols);
    const dynamicUri = 'ts:filename/apexkit_dynamic.d.ts';

    if (dynamicLibDisposable.current) {
      dynamicLibDisposable.current.dispose();
    }
    dynamicLibDisposable.current = monaco.languages.typescript.javascriptDefaults.addExtraLib(
      dynamicSource,
      dynamicUri
    );

    // B. Inject Custom Completion Provider for HTML mode <script> tags
    if (htmlCompletionDisposable.current) {
      htmlCompletionDisposable.current.dispose();
    }

    htmlCompletionDisposable.current = monaco.languages.registerCompletionItemProvider('html', {
      triggerCharacters: ['.', '$', "'", '"'],
      provideCompletionItems: function (model: any, position: any) {
        // Check if we are inside a <script> block
        const textUntilPosition = model.getValueInRange({
          startLineNumber: 1,
          startColumn: 1,
          endLineNumber: position.lineNumber,
          endColumn: position.column,
        });
        const lastScriptOpen = textUntilPosition.lastIndexOf('<script');
        const lastScriptClose = textUntilPosition.lastIndexOf('</script>');

        if (lastScriptOpen === -1 || lastScriptOpen < lastScriptClose) {
          return { suggestions: [] }; // Not inside a script tag
        }

        const word = model.getWordUntilPosition(position);
        const range = {
          startLineNumber: position.lineNumber,
          endLineNumber: position.lineNumber,
          startColumn: word.startColumn,
          endColumn: word.endColumn,
        };

        const linePrefix = model
          .getLineContent(position.lineNumber)
          .substring(0, position.column - 1);

        // Suggest Collection Names if typing inside methods
        if (
          linePrefix.match(
            /\$db\.records\.(list|get|create|update|delete|searchVector|getVector|instantSearch)\(['"]$/
          )
        ) {
          return {
            suggestions: cols.map((c) => ({
              label: c.name,
              kind: monaco.languages.CompletionItemKind.EnumMember,
              insertText: c.name,
              detail: 'Collection',
              range,
            })),
          };
        }

        // Suggest $db properties
        if (linePrefix.endsWith('$db.')) {
          return {
            suggestions: [
              {
                label: 'records',
                kind: monaco.languages.CompletionItemKind.Property,
                insertText: 'records',
                range,
              },
              {
                label: 'query',
                kind: monaco.languages.CompletionItemKind.Method,
                insertText: 'query({ from: "${1:collection}", select: [], where: {} })',
                insertTextRules: monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
                range,
              },
              {
                label: 'users',
                kind: monaco.languages.CompletionItemKind.Property,
                insertText: 'users',
                range,
              },
              {
                label: 'collections',
                kind: monaco.languages.CompletionItemKind.Property,
                insertText: 'collections',
                range,
              },
              {
                label: 'files',
                kind: monaco.languages.CompletionItemKind.Property,
                insertText: 'files',
                range,
              },
            ],
          };
        }

        // Suggest $db.records methods
        if (linePrefix.endsWith('$db.records.')) {
          return {
            suggestions: [
              {
                label: 'list',
                kind: monaco.languages.CompletionItemKind.Method,
                insertText: "list('${1:collection}', { filter: {} })",
                insertTextRules: monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
                detail: 'List records',
                range,
              },
              {
                label: 'get',
                kind: monaco.languages.CompletionItemKind.Method,
                insertText: "get('${1:collection}', ${2:id})",
                insertTextRules: monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
                detail: 'Get one record',
                range,
              },
              {
                label: 'create',
                kind: monaco.languages.CompletionItemKind.Method,
                insertText: "create('${1:collection}', { ${2:data} })",
                insertTextRules: monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
                detail: 'Create record',
                range,
              },
              {
                label: 'update',
                kind: monaco.languages.CompletionItemKind.Method,
                insertText: "update('${1:collection}', ${2:id}, { ${3:data} })",
                insertTextRules: monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
                detail: 'Update record',
                range,
              },
              {
                label: 'delete',
                kind: monaco.languages.CompletionItemKind.Method,
                insertText: "delete('${1:collection}', ${2:id})",
                insertTextRules: monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
                detail: 'Delete record',
                range,
              },
            ],
          };
        }

        // Suggest $http methods
        if (linePrefix.endsWith('$http.')) {
          return {
            suggestions: [
              {
                label: 'get',
                kind: monaco.languages.CompletionItemKind.Method,
                insertText: "get('${1:url}')",
                insertTextRules: monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
                range,
              },
              {
                label: 'post',
                kind: monaco.languages.CompletionItemKind.Method,
                insertText: "post('${1:url}', ${2:body})",
                insertTextRules: monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
                range,
              },
            ],
          };
        }

        // Global Script Scope suggestions
        return {
          suggestions: [
            {
              label: '$db',
              kind: monaco.languages.CompletionItemKind.Variable,
              insertText: '$db',
              detail: 'ApexKit Database',
              range,
            },
            {
              label: '$http',
              kind: monaco.languages.CompletionItemKind.Variable,
              insertText: '$http',
              detail: 'ApexKit HTTP Client',
              range,
            },
            {
              label: '$ai',
              kind: monaco.languages.CompletionItemKind.Variable,
              insertText: '$ai',
              detail: 'ApexKit AI Tools',
              range,
            },
            {
              label: '$fs',
              kind: monaco.languages.CompletionItemKind.Variable,
              insertText: '$fs',
              detail: 'ApexKit File System',
              range,
            },
            {
              label: '$env',
              kind: monaco.languages.CompletionItemKind.Variable,
              insertText: '$env',
              detail: 'ApexKit Secrets',
              range,
            },
            {
              label: 'ApexKit',
              kind: monaco.languages.CompletionItemKind.Class,
              insertText: 'ApexKit',
              detail: 'SDK Client',
              range,
            },
          ],
        };
      },
    });
  };

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      if (dynamicLibDisposable.current) dynamicLibDisposable.current.dispose();
      if (htmlCompletionDisposable.current) htmlCompletionDisposable.current.dispose();
    };
  }, []);

  // React to collections prop changes
  useEffect(() => {
    if (monacoRef.current && withTypes) {
      updateDynamicTypes(monacoRef.current, collections);
    }
  }, [collections, withTypes]);

  // Resize Observer
  useEffect(() => {
    const handleResize = () => {
      if (editorRef.current) {
        editorRef.current.layout();
      }
    };
    window.addEventListener('resize', handleResize);
    return () => window.removeEventListener('resize', handleResize);
  }, []);

  const handleFormat = () => {
    if (editorRef.current) {
      try {
        editorRef.current.focus();
        editorRef.current.trigger('format', 'editor.action.formatDocument', {});
      } catch (error) {
        console.error('Format failed:', error);
      }
    }
  };

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(value);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch (err) {
      console.error('Failed to copy:', err);
    }
  };

  return (
    <div
      className="flex flex-col border border-border rounded-lg overflow-hidden bg-[#1e1e1e] shadow-sm"
      style={{ height }}
    >
      {/* Header */}
      <div className="flex items-center justify-between px-3 py-2 bg-[#252526] border-b border-white/10 flex-shrink-0">
        <div className="flex items-center gap-2">
          {language === 'html' ? (
            <FileCode className="h-4 w-4 text-orange-400" />
          ) : language === 'json' ? (
            <FileJson className="h-4 w-4 text-yellow-400" />
          ) : (
            <FileCode className="h-4 w-4 text-blue-400" />
          )}
          <span className="text-xs font-medium text-gray-300 font-mono uppercase">
            {label || language}
          </span>
        </div>

        <div className="flex items-center gap-1">
          <Button
            size="sm"
            variant="ghost"
            className="h-6 text-[10px] px-2 text-gray-400 hover:text-white"
            onClick={handleFormat}
            title="Format Code"
          >
            <Wand2 className="h-3 w-3 mr-1" /> Format
          </Button>
          <div className="w-px h-3 bg-white/20 mx-1"></div>
          <Button
            size="sm"
            variant="ghost"
            className="h-6 px-2 text-gray-400 hover:text-white"
            onClick={handleCopy}
          >
            {copied ? <Check className="h-3 w-3 text-green-400" /> : <Copy className="h-3 w-3" />}
          </Button>
        </div>
      </div>

      {/* Editor container */}
      <div className="relative group flex-1 min-h-0">
        <Editor
          language={language}
          value={value}
          theme="vs-dark"
          onChange={(val) => onChange(val || '')}
          onMount={handleEditorDidMount}
          height="100%"
          options={{
            minimap: { enabled: false },
            fontSize: 13,
            lineNumbers: 'on',
            scrollBeyondLastLine: false,
            automaticLayout: true,
            tabSize: 2,
            wordWrap: 'on',
            formatOnPaste: true,
            formatOnType: true,
            readOnly: readOnly,
            fontFamily: "'JetBrains Mono', 'Fira Code', Consolas, monospace",
            padding: { top: 16, bottom: 16 },
            wrappingIndent: 'indent',
            fontLigatures: true,
          }}
        />
      </div>

      {/* Footer */}
      <div className="px-3 py-1 bg-[#252526] border-t border-white/10 text-[10px] text-gray-500 flex justify-between flex-shrink-0">
        <span>{language.toUpperCase()}</span>
        <span>
          Ln {cursorPos.line}, Col {cursorPos.column}
        </span>
      </div>
    </div>
  );
};
