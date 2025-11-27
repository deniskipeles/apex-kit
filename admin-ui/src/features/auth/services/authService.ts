import { apiClient } from '../../../lib/apiClient';

export const authService = {
  login: (email: string, password: string) => {
    return apiClient.auth.login(email, password);
  },
  logout: () => {
    return apiClient.auth.logout();
  },
};
