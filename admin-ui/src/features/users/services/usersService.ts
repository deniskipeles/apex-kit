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
  // method to fetch roles via the script engine
  getRoles: async (): Promise<string[]> => {
    try {
        // Runs the server-side script 'apex-auth-roles'
        // Expecting response: { roles: ["admin", "user", "editor"] }
        const res = await apiClient.scripts.run('apex-auth-roles', {});
        return res?.roles || ['admin', 'user']; // Fallback defaults
    } catch (e) {
        console.warn("Failed to fetch roles from script, using defaults.", e);
        return ['admin', 'user'];
    }
  }
};
