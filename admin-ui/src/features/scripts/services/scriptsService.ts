import { apiClient } from '../../../lib/apiClient';
import { Script } from '../../../types';

export const scriptsService = {
  list: () => apiClient.scripts.list(),
  create: (data: Partial<Script>) => apiClient.scripts.create(data),
  delete: (id: string) => apiClient.scripts.delete(id),
  run: (name: string, variables: any) => apiClient.scripts.run(name, variables),
  export: (format: 'json' | 'txt' = 'json') => apiClient.scripts.export(format),
  import: (file: File) => apiClient.scripts.import(file),
};
