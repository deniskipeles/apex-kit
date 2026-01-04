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
    diff_summary: string;
    last_error: string;
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
        const res = await apiClient.ai.listSessions();
        return res as AiSession[];
    },

    // Start a new project
    createSession: async (
        name: string, 
        initialPrompt?: string, 
        model?: string,
        cloneStrategy?: string,
        cloneRecordLimit?: number
    ): Promise<AiSession> => {
        const res = await apiClient.ai.createSession(
            name, 
            initialPrompt, 
            model,
            cloneStrategy,
            cloneRecordLimit
        );
        // SDK returns the object directly, or throws. 
        // If your SDK returns {id: ...}, this check is valid.
        if (!(res as any)?.id) throw new Error((res as any)?.message || "Failed to create session");
        return res as AiSession;
    },

    // Continue conversation
    chat: async (id: string, prompt: string, model: string): Promise<AiSession> => {
        const res = await apiClient.ai.chat(id, prompt, model);
        return res as AiSession;
    },

    // Code assistant edit AI
    codeEdit: async (prompt: string, currentCode: string, contextType: string, model: string): Promise<any> => {
        const res = await apiClient.ai.codeEdit(prompt, currentCode, contextType, model);
        return res;
    },

    // Publish as Plugin
    publish: async (id: string): Promise<any> => {
        const res = await apiClient.ai.publishSession(id);
        return res;
    },

    // List published plugins
    listPlugins: async (): Promise<Plugin[]> => {
        const res = await apiClient.ai.listPlugins();
        return res as Plugin[];
    },

    // Apply Changes
    applySessionChanges: async (id: string): Promise<any> => {
        return await apiClient.ai.applySessionChanges(id);
    }
};