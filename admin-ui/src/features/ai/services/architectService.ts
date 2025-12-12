import { APEX_TOKEN } from '@/src/constants';
import { apiClient } from '../../../lib/apiClient';

export interface ChatMessage {
    role: 'user' | 'assistant';
    content: string;
}

export interface AppManifest {
    app_name: string;
    collections: Array<any>;
    scripts: Array<any>;
    templates: Array<any>;
}

export interface AiSession {
    id: string;
    name: string;
    messages: ChatMessage[];
    current_manifest: AppManifest | null;
    created_at: string;
}

export interface Plugin {
    id: string;
    name: string;
    version: string;
    description: string;
    created_at: string;
}

export const architectService = {
    // List all past sessions
    listSessions: async (): Promise<AiSession[]> => {
        // Note: You need to ensure the Rust endpoint list_ai_sessions is exposed in lib.rs router
        // If not exposed in sdk.js yet, we use raw fetch for the admin namespace
        const token = localStorage.getItem(APEX_TOKEN);
        const res = await fetch(`${apiClient.apiUrl}/api/v1/admin/ai/sessions`, {
            headers: { 'Authorization': `Bearer ${token}` }
        });
        return await res.json();
    },

    // Start a new project
    createSession: async (name: string, initialPrompt?: string, model?: string): Promise<AiSession> => {
        const token = localStorage.getItem(APEX_TOKEN);
        const res = await fetch(`${apiClient.apiUrl}/api/v1/admin/ai/sessions`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json', 'Authorization': `Bearer ${token}` },
            body: JSON.stringify({ name, initial_prompt: initialPrompt, model }) 
        });
        if (!res.ok) throw new Error((await res.json()).message);
        return await res.json();
    },

    // Continue conversation
    chat: async (id: string, prompt: string, model: string): Promise<AiSession> => {
        const token = localStorage.getItem(APEX_TOKEN);
        const res = await fetch(`${apiClient.apiUrl}/api/v1/admin/ai/sessions/${id}/chat`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json', 'Authorization': `Bearer ${token}` },
            body: JSON.stringify({ prompt, model })
        });
        if (!res.ok) throw new Error((await res.json()).message);
        return await res.json();
    },

    // code assistant edit AI
    codeEdit: async (prompt: string, currentCode: string, contextType: string, model: string): Promise<any> => {
        const token = localStorage.getItem(APEX_TOKEN);
        const res = await fetch(`${apiClient.apiUrl}/api/v1/admin/ai/edit-code`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
                'Authorization': `Bearer ${token}`
            },
            body: JSON.stringify({
                prompt,
                current_code: currentCode,
                context_type: contextType,
                model: model // Pass selected model
            })
        });
        if (!res.ok) throw new Error((await res.json()).message);
        return await res.json();
    },

    // Publish as Plugin
    publish: async (id: string): Promise<any> => {
        const token = localStorage.getItem(APEX_TOKEN);
        const res = await fetch(`${apiClient.apiUrl}/api/v1/admin/ai/sessions/${id}/publish`, {
            method: 'POST',
            headers: { 'Authorization': `Bearer ${token}` }
        });
        if (!res.ok) throw new Error((await res.json()).message);
        return await res.json();
    },
    // List published plugins
    listPlugins: async (): Promise<Plugin[]> => {
        const token = localStorage.getItem(APEX_TOKEN);
        const res = await fetch(`${apiClient.apiUrl}/api/v1/admin/ai/plugins`, {
            headers: { 'Authorization': `Bearer ${token}` }
        });
        if (!res.ok) throw new Error("Failed to load plugins");
        return await res.json();
    }
};