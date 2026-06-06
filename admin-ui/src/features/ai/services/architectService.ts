import { apiClient } from '../../../lib/apiClient';
import { AiSession, Plugin } from '../../../types';

export const architectService = {
  listSessions: async (): Promise<AiSession[]> => {
    try {
      const list = await apiClient.root.listSandboxes();

      // [FIXED] Defensive Array Guard
      if (!Array.isArray(list)) {
        console.warn('apiClient.root.listSandboxes did not return an array:', list);
        return [];
      }

      return list.map((s: any) => ({
        id: s.id,
        name: s.name || s.id,
        messages: [],
        current_manifest: s.current_manifest || null,
        created_at: s.expires_at || new Date().toISOString(),
      })) as any;
    } catch (e) {
      console.error('Failed to load listSessions:', e);
      return [];
    }
  },

  // Provision sandbox directly
  createSession: async (
    name: string,
    initialPrompt?: string,
    model?: string,
    cloneStrategy?: string,
    cloneRecordLimit?: number
  ): Promise<any> => {
    return await apiClient.root.createSandbox({
      name,
      clone_strategy: cloneStrategy,
      clone_record_limit: cloneRecordLimit,
      model,
      initial_prompt: initialPrompt,
    });
  },

  // Scoped chat methods (executed inside Sandbox context)
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
};
