import { apiClient } from '../../../lib/apiClient';
import { AuthUser } from '../../../types';

export const usersService = {
  list: (page = 1, perPage = 20, search = '') => {
    return apiClient.users.list(page, perPage, search);
  },
  create: (data: Partial<AuthUser>) => {
    return apiClient.users.create(data);
  },
  update: (id: string, data: Partial<AuthUser>) => {
    return apiClient.users.update(id, data);
  },
  delete: (id: string) => {
    return apiClient.users.delete(id);
  },
  getRoles: async (): Promise<string[]> => {
    try {
      const res = await apiClient.auth.listRoles();
      return res.roles || ['admin', 'user'];
    } catch (e) {
      console.warn('Failed to fetch roles, using defaults.', e);
      return ['admin', 'user'];
    }
  },
};
