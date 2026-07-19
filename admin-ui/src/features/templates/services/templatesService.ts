import { apiClient } from '../../../lib/apiClient';
import { Template } from '../../../types';

export const templatesService = {
  list: () => apiClient.templates.list(),
  create: (data: Partial<Template>) => apiClient.templates.create(data),
  update: (id: string, data: Partial<Template>) => apiClient.templates.update(id, data),
  delete: (id: string) => apiClient.templates.delete(id),
  export: (format: 'json' | 'txt' = 'json') => apiClient.templates.export(format),
  import: (file: File) => apiClient.templates.import(file),
};
