
import { useAuthStore } from '../store/useAuthStore';

export const useAuth = () => {
  const { user, login, logout, isLoading, token } = useAuthStore();
  
  return {
    user,
    token,
    login,
    logout,
    isLoading,
    isAuthenticated: !!user
  };
};
