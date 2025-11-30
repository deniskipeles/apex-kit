import { apiClient } from '../../../lib/apiClient';
import { AiAction } from '../../../types';

export const aiService = {
  list: async (): Promise<AiAction[]> => {
    // The backend returns snake_case, need to map if necessary or ensure types match
    const res = await apiClient.ai.getActions();
    return res.map((a: any) => ({
        ...a,
        id: a.id.toString(),
        // Handle potential nulls
        system_prompt: a.system_prompt || '', 
        config: a.config || {}
    }));
  },

  create: async (data: Partial<AiAction>) => {
    return await apiClient.ai.createAction(data);
  },

  delete: async (id: string) => {
    return await apiClient.ai.deleteAction(id);
  },

  run: async (slug: string, variables: Record<string, string>) => {
    return await apiClient.ai.run(slug, variables);
  }
};