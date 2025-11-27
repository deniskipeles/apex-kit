import React, { createContext, useContext, useState, useEffect, ReactNode } from 'react';
import { authService } from './services/authService';
import { AdminUser } from '../../types';

interface AuthContextType {
  user: AdminUser | null;
  login: (e: string, p: string) => Promise<void>;
  logout: () => void;
  isLoading: boolean;
}

const AuthContext = createContext<AuthContextType>(null!);

export const AuthProvider = ({ children }: { children?: ReactNode }) => {
  const [user, setUser] = useState<AdminUser | null>(null);
  const [isLoading, setIsLoading] = useState(true);

  useEffect(() => {
    const storedUser = localStorage.getItem('tinybase_user');
    const storedToken = localStorage.getItem('tinybase_token');
    
    if (storedUser && storedToken) {
        setUser(JSON.parse(storedUser));
        // Token is set in apiClient.ts on load, but good to double check if moving logic
    }
    setIsLoading(false);
  }, []);

  const login = async (e: string, p: string) => {
    const res = await authService.login(e, p);
    setUser(res.user);
    localStorage.setItem('tinybase_user', JSON.stringify(res.user));
    // Token setting handled in apiClient
  };

  const logout = async () => {
    await authService.logout();
    setUser(null);
    localStorage.removeItem('tinybase_user');
  };

  return (
    <AuthContext.Provider value={{ user, login, logout, isLoading }}>
      {children}
    </AuthContext.Provider>
  );
};

export const useAuth = () => useContext(AuthContext);
