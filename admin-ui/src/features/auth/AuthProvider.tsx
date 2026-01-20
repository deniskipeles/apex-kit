import React, { createContext, useContext, useState, useEffect, ReactNode } from 'react';
import { authService } from './services/authService';
import { AuthUser } from '../../types';
import { APEX_TOKEN, APEX_USER } from '@/src/constants';
import { apiClient } from '@/src/lib/apiClient';

interface AuthContextType {
  user: AuthUser | null;
  login: (e: string, p: string) => Promise<void>;
  logout: () => void;
  isLoading: boolean;
}

const AuthContext = createContext<AuthContextType>(null!);

export const AuthProvider = ({ children }: { children?: ReactNode }) => {
  const [user, setUser] = useState<AuthUser | null>(null);
  const [isLoading, setIsLoading] = useState(true);

  useEffect(() => {
    const initAuth = async () => {
      const storedToken = localStorage.getItem(APEX_TOKEN);
      if (storedToken) {
        // Set token first so the request is authenticated
        // We don't have user object yet, so scope isn't set in SDK yet
        apiClient.setToken(storedToken);

        try {
          // Fetch fresh user data + Authoritative Scope
          const user = await apiClient.auth.getMe();
          setUser(user);
          // SDK scope is updated inside getMe() via _request response handling
        } catch (e) {
          console.error("Token invalid or expired", e);
          localStorage.removeItem(APEX_TOKEN);
          localStorage.removeItem(APEX_USER);
          setUser(null);
        }
      }
      setIsLoading(false);
    };
    initAuth();
  }, []);

  const login = async (e: string, p: string) => {
    const res = await authService.login(e, p);
    setUser(res.user);
    localStorage.setItem(APEX_USER, JSON.stringify(res.user));
    // Token setting handled in apiClient
  };

  const logout = async () => {
    await authService.logout();
    setUser(null);
    localStorage.removeItem(APEX_USER);
    localStorage.removeItem(APEX_TOKEN);
  };

  return (
    <AuthContext.Provider value={{ user, login, logout, isLoading }}>
      {children}
    </AuthContext.Provider>
  );
};

export const useAuth = () => useContext(AuthContext);
