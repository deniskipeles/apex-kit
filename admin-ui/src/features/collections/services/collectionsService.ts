
import { apiClient } from '../../../lib/apiClient';
import { Collection } from '../../../types';

export const collectionsService = {
  list: () => {
    return apiClient.collections.list();
  },
  get: (id: string) => {
    return apiClient.collections.get(id);
  },
  create: (data: Partial<Collection>) => {
    return apiClient.collections.create(data);
  },
  update: (id: string, data: Partial<Collection>) => {
    return apiClient.collections.update(id, data);
  },
  delete: (id: string) => {
    return apiClient.collections.delete(id);
  },
};