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
    height = "400px", 
    readOnly = false,
    label,
    withTypes = false,
    collections = [] 
}: CodeEditorProps) => {
    const editorRef = useRef<any>(null);
    const monacoRef = useRef<any>(null);
    
    // FIX: Store the disposable returned by addExtraLib here
    const dynamicLibDisposable = useRef<any>(null);

    const [copied, setCopied] = useState(false);
    const [cursorPos, setCursorPos] = useState({ line: 1, column: 1 });

    const handleEditorDidMount: OnMount = (editor, monacoInstance) => {
        editorRef.current = editor;
        monacoRef.current = monacoInstance;

        // Fix layout issues in modals
        setTimeout(() => {
            editor.layout();
        }, 300); 

        editor.onDidChangeCursorPosition((e) => {
            setCursorPos({
                line: e.position.lineNumber,
                column: e.position.column
            });
        });

        // Configure TypeScript/JavaScript settings
        if (language === 'javascript' || language === 'typescript') {
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
                // 1. Load Static System Types
                // We don't need to track this one as it never changes
                const baseLibSource = `
                    // --- Primitives ---
                    type JsonValue = string | number | boolean | null | { [key: string]: JsonValue } | JsonValue[];
                    
                    interface Request {
                        method: string;
                        headers: Headers;
                        body: any; // Raw parsed body
                        args: any; // Alias for body
                        json(): Promise<any>;
                        text(): Promise<string>;
                    }

                    interface ResponseInit {
                        status?: number;
                        headers?: Record<string, string>;
                    }

                    declare class Response {
                        constructor(body: any, init?: ResponseInit);
                    }

                    interface Headers {
                        get(name: string): string | null;
                        set(name: string, value: string): void;
                    }

                    // --- Global Tools ---
                    declare const console: {
                        log(...args: any[]): void;
                        error(...args: any[]): void;
                    };

                    declare const $http: {
                        get(url: string): Promise<string>;
                        post(url: string, body: object): Promise<string>;
                    };

                    declare const $util: {
                        uuid(): string;
                        slugify(text: string): string;
                        hash(text: string, alg: 'sha256' | 'sha512'): string;
                        hmac(text: string, key: string): string;
                        sleep(ms: number): Promise<void>;
                    };

                    declare const $ai: {
                        embed(text: string): Promise<number[]>;
                    };

                    declare const $env: {
                        get(key: string): Promise<string>;
                        APP_URL: string;
                    };
                    
                    declare const $fs: {
                        readText(filename: string): Promise<string>;
                    };
                    
                    declare const $archive: {
                        create(jsonTree: object, filename: string): Promise<string>;
                    };
                    
                    declare const $realtime: {
                        send(channel: string, event: string, data: object): Promise<boolean>;
                    };
                    
                    declare const $mail: {
                        send(to: string, subject: string, body: string): Promise<boolean>;
                    };

                    // --- ApexKit SDK ---
                    
                    interface QueryOptions {
                        filter?: string | object;
                        sort?: string;
                        page?: number;
                        per_page?: number;
                        expand?: string;
                    }

                    interface ApexQuery {
                        from: string;
                        select?: string[];
                        where?: object;
                        sort?: string;
                        limit?: number;
                        offset?: number;
                        populate?: string[];
                        aggregate?: Record<string, { $sum?: string, $count?: string, $avg?: string, $min?: string, $max?: string }>;
                        group_by?: string;
                    }

                    interface CollectionAPI {
                        list(options?: QueryOptions): Promise<{ items: any[], total: number }>;
                        get(id: number | string, options?: { expand?: string }): Promise<any>;
                        create(data: object): Promise<{ id: number }>;
                        update(id: number | string, data: object): Promise<any>;
                        delete(id: number | string): Promise<boolean>;
                        search(query: string): Promise<any[]>;
                        searchVector(field: string, vector: number[], limit?: number): Promise<any[]>;
                        getVector(id: number | string): Promise<any[]>;
                    }

                    interface UsersAPI {
                        list(query?: string, limit?: number, offset?: number): Promise<any[]>;
                        get(email: string): Promise<any>;
                        create(email: string, password?: string, role?: string): Promise<any>;
                    }
                    
                    interface FilesAPI {
                        list(limit?: number, offset?: number): Promise<any[]>;
                    }
                    
                    interface CollectionsAPI {
                        list(): Promise<any[]>;
                        create(name: string, schema?: object): Promise<{ id: number }>;
                    }

                    declare class ApexKit {
                        constructor(contextId?: string | null);
                        tenant(id: string): ApexKit;
                        sandbox(id: string): ApexKit;
                        
                        collection(name: string): CollectionAPI;
                        
                        readonly users: UsersAPI;
                        readonly files: FilesAPI;
                        readonly collections: CollectionsAPI;
                        
                        query(q: ApexQuery): Promise<any>;
                    }
                    
                    // --- Entry Points ---
                    declare const $apex: ApexKit;
                    declare const pb: ApexKit; // Alias

                    // Low-level $db (internal use)
                    declare const $db: {
                        records: {
                            list(ctx: string | null, col: string, opts: any): Promise<any>;
                            // ... other low level methods
                        };
                        query(ctx: string | null, q: any): Promise<any>;
                    };
                    
                    // --- Hook Event Context ---
                    // e is passed to hook scripts
                    declare const e: {
                        trigger: string;
                        auth?: { id: number, email: string, role: string };
                        record?: { id: number, data: any };
                        collection?: { id: number, name: string };
                        data?: any; // For list hooks
                    };
                    
                    // Helper for log
                    declare function log(msg: any): void;
                `;
                const baseLibUri = 'ts:filename/apexkit_base.d.ts';
                // Add if not exists (addExtraLib overwrites if same URI, but best to be safe)
                monacoInstance.languages.typescript.javascriptDefaults.addExtraLib(baseLibSource, baseLibUri);

                // 2. Load Initial Dynamic Types
                updateDynamicTypes(monacoInstance, collections);
            }
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

    // Helper to inject types
    const updateDynamicTypes = (monaco: any, cols: Collection[]) => {
        if (!withTypes || !monaco) return;
        
        const dynamicSource = generateTypeScriptDefs(cols);
        const dynamicUri = 'ts:filename/apexkit_dynamic.d.ts';

        // FIX: Dispose of the previous definition using the stored reference
        if (dynamicLibDisposable.current) {
            dynamicLibDisposable.current.dispose();
        }

        // FIX: Add new definition and store the disposable reference
        dynamicLibDisposable.current = monaco.languages.typescript.javascriptDefaults.addExtraLib(
            dynamicSource, 
            dynamicUri
        );
    };

    // Cleanup on unmount
    useEffect(() => {
        return () => {
            if (dynamicLibDisposable.current) {
                dynamicLibDisposable.current.dispose();
            }
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
        <div className="flex flex-col border border-border rounded-lg overflow-hidden bg-[#1e1e1e] shadow-sm" style={{ height }}>
            {/* Header */}
            <div className="flex items-center justify-between px-3 py-2 bg-[#252526] border-b border-white/10 flex-shrink-0">
                <div className="flex items-center gap-2">
                    {language === 'html' ? <FileCode className="h-4 w-4 text-orange-400" /> : 
                     language === 'json' ? <FileJson className="h-4 w-4 text-yellow-400" /> :
                     <FileCode className="h-4 w-4 text-blue-400" />}
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
                <span>Ln {cursorPos.line}, Col {cursorPos.column}</span>
            </div>
        </div>
    );
};