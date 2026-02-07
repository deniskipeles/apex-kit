import React, { createContext, useContext, useState, useEffect, ReactNode, useCallback } from 'react';
import { authService } from './services/authService';
import { AuthUser } from '../../types';
import { APEX_TOKEN, APEX_USER } from '@/src/constants';
import { apiClient } from '@/src/lib/apiClient';

interface AuthContextType {
  user: AuthUser | null;
  login: (e: string, p: string) => Promise<void>;
  logout: () => void;
  checkAuth: () => Promise<void>; // <--- Added this
  isLoading: boolean;
}

const AuthContext = createContext<AuthContextType>(null!);

export const AuthProvider = ({ children }: { children?: ReactNode }) => {
  const [user, setUser] = useState<AuthUser | null>(null);
  const [isLoading, setIsLoading] = useState(true);

  // Helper to determine current URL scope
  const getCurrentUrlScope = () => {
    const path = window.location.pathname;
    if (path.includes('/tenant/')) {
        const id = path.split('/tenant/')[1].split('/')[0];
        return `tenant:${id}`;
    }
    if (path.includes('/sandbox/')) {
        const id = path.split('/sandbox/')[1].split('/')[0];
        return `sandbox:${id}`;
    }
    return 'root';
  };

  // Define checkAuth as a reusable useCallback
  const checkAuth = useCallback(async () => {
    const storedToken = localStorage.getItem(APEX_TOKEN);
    
    if (storedToken) {
      apiClient.setToken(storedToken);

      try {
        // 1. Fetch User (this validates the token with the backend)
        const user = await apiClient.auth.getMe();
        
        // 2. Validate Scope against URL
        const urlScope = getCurrentUrlScope();
        const userScope = user.scope || 'root'; 

        // Rule: 
        // - Root user can access everything.
        // - Tenant/Sandbox user must match URL exactly.
        const isAllowed = userScope === 'root' || userScope === urlScope;

        if (!isAllowed) {
            console.warn(`Scope Mismatch: User(${userScope}) cannot access URL(${urlScope}). Logging out.`);
            throw new Error("Scope Mismatch");
        }

        setUser(user);
      } catch (e) {
        console.error("Auth check failed", e);
        logout(); // Invalid token or scope mismatch -> logout
      }
    } else {
        setUser(null);
    }
    setIsLoading(false);
  }, []);

  // Initial Load
  useEffect(() => {
    checkAuth();
  }, [checkAuth]);

  const login = async (e: string, p: string) => {
    const res = await authService.login(e, p);
    
    // Validate returned user scope against URL immediately
    const urlScope = getCurrentUrlScope();
    const tokenScope = res.user.scope || 'root'; 
    
    if (tokenScope !== 'root' && tokenScope !== urlScope) {
        throw new Error(`Login restricted. This user belongs to ${tokenScope}, not ${urlScope}.`);
    }

    setUser(res.user);
    localStorage.setItem(APEX_USER, JSON.stringify(res.user));
  };

  const logout = async () => {
    try { await authService.logout(); } catch {}
    setUser(null);
    localStorage.removeItem(APEX_USER);
    localStorage.removeItem(APEX_TOKEN);
    // Force reload to clear any memory states/proxies
    window.location.href = window.location.pathname.includes('/_dashboard') ? window.location.pathname : '/_dashboard';
  };

  return (
    <AuthContext.Provider value={{ user, login, logout, checkAuth, isLoading }}>
      {children}
    </AuthContext.Provider>
  );
};

export const useAuth = () => useContext(AuthContext);