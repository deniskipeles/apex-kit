import { apiClient } from '../../../lib/apiClient';
import { AdminUser } from '../../../types';

export const usersService = {
  list: () => {
    return apiClient.users.list();
  },
  create: (data: Partial<AdminUser>) => {
    return apiClient.users.create(data);
  },
  update: (id: string, data: Partial<AdminUser>) => {
    return apiClient.users.update(id, data);
  },
  delete: (id: string) => {
    return apiClient.users.delete(id);
  },
};
