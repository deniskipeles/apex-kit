import { APP_CONFIG } from '../../../../config/app.config';

export type Method = 'GET' | 'POST' | 'PATCH' | 'DELETE';

export interface ApiEndpointDef {
    id: string;
    label: string;
    method: Method;
    path: string; // e.g. "/collections/:id/records"
    description: string;
    // Parameters for URL path or Query string
    params?: { name: string; type: 'path' | 'query'; description?: string; default?: string }[];
    // Default JSON body for POST/PATCH
    body?: any; 
    category: 'data' | 'search' | 'storage' | 'compute' | 'ai';
}

export const getBaseUrl = () => {
    const path = window.location.pathname;
    const tenantMatch = path.match(/^\/_dashboard\/tenant\/([^/]+)/);
    const sandboxMatch = path.match(/^\/_dashboard\/sandbox\/([^/]+)/);
    
    let base = APP_CONFIG.apiBaseUrl; // Default root
    let envPrefix = '';

    if (tenantMatch) {
        base += `/tenant/${tenantMatch[1]}/api/v1`;
        envPrefix = `Tenant (${tenantMatch[1]})`;
    } else if (sandboxMatch) {
        base += `/sandbox/${sandboxMatch[1]}/api/v1`;
        envPrefix = `Sandbox (${sandboxMatch[1]})`;
    } else {
        base += `/api/v1`;
        envPrefix = 'Root';
    }
    return { url: base, env: envPrefix };
};

// Dynamic generator based on current context
export const getEndpoints = (collectionName = '{collection}'): ApiEndpointDef[] => [
    // --- DATA ---
    {
        id: 'list_records', category: 'data', label: 'List Records', method: 'GET',
        path: `/collections/${collectionName}/records`,
        description: 'Fetch paginated records with filtering and sorting.',
        params: [
            { name: 'page', type: 'query', default: '1' },
            { name: 'per_page', type: 'query', default: '20' },
            { name: 'sort', type: 'query', default: '-created' },
            { name: 'filter', type: 'query', description: 'JSON e.g. {"status":"active"}' }
        ]
    },
    {
        id: 'get_record', category: 'data', label: 'Get Record', method: 'GET',
        path: `/collections/${collectionName}/records/{id}`,
        description: 'Retrieve a single record by ID.',
        params: [{ name: 'id', type: 'path', default: '1' }]
    },
    {
        id: 'create_record', category: 'data', label: 'Create Record', method: 'POST',
        path: `/collections/${collectionName}/records`,
        description: 'Create a new record.',
        body: { data: { title: "New Item", status: "draft" } }
    },
    
    // --- SEARCH ---
    {
        id: 'sql_search', category: 'search', label: 'SQL Search', method: 'GET',
        path: `/collections/${collectionName}/search`,
        description: 'Standard database search (LIKE query).',
        params: [{ name: 'q', type: 'query', default: 'search term' }]
    },
    {
        id: 'instant_search', category: 'search', label: 'Instant Search', method: 'GET',
        path: `/collections/${collectionName}/instant-search`,
        description: 'High-performance full-text search using Tantivy index. Returns snippets.',
        params: [{ name: 'q', type: 'query', default: 'search term' }]
    },
    {
        id: 'vector_search', category: 'search', label: 'Vector Search', method: 'POST',
        path: `/collections/${collectionName}/search-text-vector`,
        description: 'Semantic search using AI embeddings.',
        body: { query_text: "Find items similar to this concept", limit: 5 }
    },

    // --- STORAGE ---
    {
        id: 'list_files', category: 'storage', label: 'List Files', method: 'GET',
        path: `/storage/files`,
        description: 'List uploaded files.',
        params: [{ name: 'page', type: 'query', default: '1' }]
    },
    // (Upload is multipart, hard to demo in JSON playground, skipping for now)

    // --- COMPUTE ---
    {
        id: 'run_script', category: 'compute', label: 'Run Script', method: 'POST',
        path: `/run/{script_name}`,
        description: 'Execute a server-side script.',
        params: [{ name: 'script_name', type: 'path', default: 'my-script' }],
        body: { some_input: "value" }
    },

    // --- AI ---
    {
        id: 'run_ai', category: 'ai', label: 'Run AI Action', method: 'POST',
        path: `/ai/run/{slug}`,
        description: 'Execute a Generative AI prompt template.',
        params: [{ name: 'slug', type: 'path', default: 'summarize-text' }],
        body: { variables: { input: "Text to process..." } }
    }
];