import { apiClient } from '../../../lib/apiClient';
import { AiSession, Plugin } from '../../../types';

export const architectService = {
  listSessions: async (): Promise<any[]> => {
    try {
      const list = await apiClient.root.listSandboxes();

      if (!Array.isArray(list)) {
        console.warn('apiClient.root.listSandboxes did not return an array:', list);
        return [];
      }

      // [FIXED] Pass down container metadata from the DB metrics
      return list.map((s: any) => ({
        id: s.id,
        name: s.name || s.id,
        messages: [],
        current_manifest: null, // Kept secure inside isolated sandbox DB
        created_at: s.created_at || new Date().toISOString(),
        status: s.status,
        expires_at: s.expires_at,
        current_storage_mb: s.current_storage_mb,
        max_storage_mb: s.max_storage_mb,
        current_vectors: s.current_vectors,
        max_vectors: s.max_vectors,
        current_ai_requests: s.current_ai_requests,
        max_ai_requests: s.max_ai_requests,
      })) as any;
    } catch (e) {
      console.error('Failed to load listSessions:', e);
      return [];
    }
  },

  createSession: async (
    name: string,
    initialPrompt?: string,
    model?: string,
    cloneStrategy?: string,
    cloneRecordLimit?: number,
    collections?: string[],
    scripts?: string[],
    templates?: string[]
  ): Promise<any> => {
    return await apiClient.root.createSandbox({
      name,
      clone_strategy: cloneStrategy || 'none',
      clone_record_limit: cloneRecordLimit,
      model,
      initial_prompt: initialPrompt,
      collections,
      scripts,
      templates,
    });
  },

  chat: async (prompt: string, model: string): Promise<AiSession> => {
    return await apiClient.ai.chat(prompt, model);
  },

  applySessionChanges: async (): Promise<AiSession> => {
    return await apiClient.ai.applySessionChanges();
  },

  publish: async (id: string): Promise<Plugin> => {
    return await apiClient.root.publishSandbox(id);
  },

  listPlugins: async (): Promise<Plugin[]> => {
    return await apiClient.ai.listPlugins();
  },

  codeEdit: async (prompt: string, currentCode: string, contextType: string, model: string) => {
    return await apiClient.ai.codeEdit(prompt, currentCode, contextType, model);
  },
};
