import React, { useState, useEffect, useMemo } from 'react';
import { Play, Copy, Check, Loader2, AlertCircle } from 'lucide-react';
import { Button, Input, Label, Badge } from '@/src/components/ui/Elements';
import { JSONEditor } from '@/src/components/form/JsonEditor';
import { ApiEndpointDef, getBaseUrl } from './ApiDefinitions';
import { APEX_TOKEN } from '@/src/constants';

interface Props {
    endpoint: ApiEndpointDef;
}

export const EndpointPlayground = ({ endpoint }: Props) => {
    const { url: baseUrl, env } = getBaseUrl();
    
    // State for inputs
    const [pathParams, setPathParams] = useState<Record<string, string>>({});
    const [queryParams, setQueryParams] = useState<Record<string, string>>({});
    const [body, setBody] = useState<string>('{}');
    
    // Execution State
    const [isLoading, setIsLoading] = useState(false);
    const [response, setResponse] = useState<any>(null);
    const [status, setStatus] = useState<number | null>(null);
    const [copied, setCopied] = useState(false);

    // Initialize defaults when endpoint changes
    useEffect(() => {
        const initialPath: any = {};
        const initialQuery: any = {};
        endpoint.params?.forEach(p => {
            if (p.type === 'path') initialPath[p.name] = p.default || '';
            if (p.type === 'query') initialQuery[p.name] = p.default || '';
        });
        setPathParams(initialPath);
        setQueryParams(initialQuery);
        setBody(endpoint.body ? JSON.stringify(endpoint.body, null, 2) : '');
        setResponse(null);
        setStatus(null);
    }, [endpoint]);

    // Construct the full URL
    const fullUrl = useMemo(() => {
        let path = endpoint.path;
        // Replace path params
        Object.entries(pathParams).forEach(([key, val]) => {
            path = path.replace(`{${key}}`, val || `:${key}`);
        });

        // Append Query Params
        const searchParams = new URLSearchParams();
        Object.entries(queryParams).forEach(([key, val]) => {
            if (val) searchParams.append(key, val);
        });
        const qs = searchParams.toString();
        
        return `${baseUrl}${path}${qs ? '?' + qs : ''}`;
    }, [endpoint, pathParams, queryParams, baseUrl]);

    // Generate Code Snippets
    const curlCommand = useMemo(() => {
        let cmd = `curl -X ${endpoint.method} "${fullUrl}" \\\n  -H "Authorization: Bearer YOUR_TOKEN"`;
        if (endpoint.method !== 'GET' && body) {
            cmd += ` \\\n  -H "Content-Type: application/json" \\\n  -d '${body.replace(/\n/g, '')}'`;
        }
        return cmd;
    }, [fullUrl, endpoint.method, body]);

    const jsCode = useMemo(() => {
        const hasBody = endpoint.method !== 'GET' && body && body !== '{}';
        return `
const response = await fetch("${fullUrl}", {
  method: "${endpoint.method}",
  headers: {
    "Authorization": "Bearer " + token${hasBody ? ',\n    "Content-Type": "application/json"' : ''}
  }${hasBody ? `,\n  body: JSON.stringify(${body})` : ''}
});

const data = await response.json();
console.log(data);
`.trim();
    }, [fullUrl, endpoint.method, body]);

    const handleRun = async () => {
        setIsLoading(true);
        setResponse(null);
        setStatus(null);

        try {
            const token = localStorage.getItem(APEX_TOKEN);
            const options: RequestInit = {
                method: endpoint.method,
                headers: {
                    'Authorization': `Bearer ${token}`,
                    'Content-Type': 'application/json'
                }
            };

            if (endpoint.method !== 'GET' && body) {
                // Validate JSON before sending
                try {
                    JSON.parse(body);
                    options.body = body;
                } catch (e) {
                    throw new Error("Invalid JSON in request body");
                }
            }

            const res = await fetch(fullUrl, options);
            setStatus(res.status);
            
            const contentType = res.headers.get("content-type");
            if (contentType && contentType.includes("application/json")) {
                const json = await res.json();
                setResponse(json);
            } else {
                const text = await res.text();
                setResponse(text);
            }
        } catch (e: any) {
            setResponse({ error: e.message });
        } finally {
            setIsLoading(false);
        }
    };

    const copyToClipboard = (text: string) => {
        navigator.clipboard.writeText(text);
        setCopied(true);
        setTimeout(() => setCopied(false), 2000);
    };

    return (
        <div className="flex flex-col h-full overflow-hidden">
            {/* Header */}
            <div className="flex-none p-6 border-b border-border bg-secondary/5">
                <div className="flex items-center gap-3 mb-2">
                    <Badge variant={endpoint.method === 'GET' ? 'primary' : endpoint.method === 'POST' ? 'success' : 'warning'} className="text-xs font-mono">
                        {endpoint.method}
                    </Badge>
                    <h2 className="text-xl font-bold font-mono truncate">{endpoint.path}</h2>
                </div>
                <p className="text-muted-foreground">{endpoint.description}</p>
                <div className="mt-2 text-xs text-muted-foreground flex gap-2">
                    <span>Environment: <strong className="text-primary">{env}</strong></span>
                </div>
            </div>

            <div className="flex-1 flex flex-col lg:flex-row overflow-hidden">
                {/* LEFT: Inputs */}
                <div className="flex-1 overflow-y-auto p-6 space-y-6 border-r border-border">
                    
                    {/* Params Form */}
                    {endpoint.params && endpoint.params.length > 0 && (
                        <div className="space-y-4">
                            <h3 className="text-sm font-bold uppercase text-muted-foreground">Parameters</h3>
                            <div className="grid gap-4">
                                {endpoint.params.map(param => (
                                    <div key={param.name} className="space-y-1">
                                        <Label className="font-mono text-xs text-primary">
                                            {param.name} 
                                            <span className="ml-2 text-muted-foreground opacity-50 uppercase text-[10px]">{param.type}</span>
                                        </Label>
                                        <Input 
                                            value={param.type === 'path' ? pathParams[param.name] : queryParams[param.name]}
                                            onChange={(e: any) => {
                                                const val = e.target.value;
                                                if (param.type === 'path') setPathParams(prev => ({...prev, [param.name]: val}));
                                                else setQueryParams(prev => ({...prev, [param.name]: val}));
                                            }}
                                            placeholder={param.description || `Value for ${param.name}`}
                                        />
                                    </div>
                                ))}
                            </div>
                        </div>
                    )}

                    {/* Body Editor */}
                    {endpoint.method !== 'GET' && (
                        <div className="space-y-2 flex-1 flex flex-col">
                            <h3 className="text-sm font-bold uppercase text-muted-foreground">Request Body</h3>
                            <div className="border border-border rounded-md overflow-hidden flex-1 min-h-[200px]">
                                <JSONEditor value={body} onChange={setBody} height="100%" />
                            </div>
                        </div>
                    )}

                    {/* Code Snippets */}
                    <div className="space-y-2">
                        <div className="flex items-center justify-between">
                             <h3 className="text-sm font-bold uppercase text-muted-foreground">Generated Code</h3>
                             <Button variant="ghost" size="sm" onClick={() => copyToClipboard(jsCode)} className="h-6 text-xs gap-1">
                                {copied ? <Check className="h-3 w-3" /> : <Copy className="h-3 w-3" />} Copy JS
                             </Button>
                        </div>
                        <div className="bg-[#0d1117] p-3 rounded-md overflow-x-auto text-xs font-mono text-blue-100 border border-border">
                            <pre>{curlCommand}</pre>
                        </div>
                    </div>
                </div>

                {/* RIGHT: Execution & Response */}
                <div className="flex-1 flex flex-col bg-secondary/5 min-w-[40%]">
                    <div className="p-4 border-b border-border flex justify-between items-center bg-background">
                         <div className="text-sm font-semibold">Response</div>
                         <Button onClick={handleRun} disabled={isLoading}>
                             {isLoading ? <Loader2 className="animate-spin mr-2 h-4 w-4" /> : <Play className="mr-2 h-4 w-4" />}
                             Run Request
                         </Button>
                    </div>
                    
                    <div className="flex-1 overflow-auto p-4 relative">
                        {status !== null && (
                            <div className={`absolute top-4 right-4 z-10 px-2 py-1 rounded text-xs font-bold ${status >= 200 && status < 300 ? 'bg-emerald-500/20 text-emerald-500' : 'bg-destructive/20 text-destructive'}`}>
                                Status: {status}
                            </div>
                        )}
                        
                        {response ? (
                            <div className="h-full bg-background border border-border rounded-md overflow-hidden">
                                <JSONEditor value={typeof response === 'string' ? response : JSON.stringify(response, null, 2)} onChange={() => {}} readOnly height="100%" />
                            </div>
                        ) : (
                            <div className="h-full flex flex-col items-center justify-center text-muted-foreground">
                                <div className="p-4 rounded-full bg-secondary mb-3">
                                    <AlertCircle className="h-8 w-8 opacity-20" />
                                </div>
                                <p className="text-sm">Click "Run Request" to execute</p>
                            </div>
                        )}
                    </div>
                </div>
            </div>
        </div>
    );
};