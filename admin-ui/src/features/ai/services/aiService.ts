import { apiClient } from '../../../lib/apiClient';
import { AiAction } from '../../../types';

export const aiService = {
  list: async (): Promise<AiAction[]> => {
    return await apiClient.ai.getActions();
  },

  create: async (data: Partial<AiAction>) => {
    return await apiClient.ai.createAction(data);
  },

  delete: async (id: string) => {
    return await apiClient.ai.deleteAction(id);
  },

  run: async (
    slug: string,
    variables: Record<string, string>,
    onChunk?: (text: string) => void
  ) => {
    return await apiClient.ai.run(slug, variables, onChunk);
  },

  exportActions: async () => {
    return await apiClient.ai.exportActions();
  },

  importActions: async (file: File) => {
    return await apiClient.ai.importActions(file);
  },
};
