import React, {
  createContext,
  useContext,
  useState,
  useEffect,
  ReactNode,
  useCallback,
} from 'react';
import { authService } from './services/authService';
import { AuthUser } from '../../types';
import { APEX_TOKEN, APEX_USER } from '@/src/constants';
import { apiClient } from '@/src/lib/apiClient';
import { ApexKit } from '@/src/lib/sdk';
import { APP_CONFIG } from '@/src/config/app.config';

interface AuthContextType {
  user: AuthUser | null;
  login: (e: string, p: string) => Promise<void>;
  logout: () => void;
  checkAuth: () => Promise<void>;
  isLoading: boolean;
}

const AuthContext = createContext<AuthContextType>(null!);

export const AuthProvider = ({ children }: { children?: ReactNode }) => {
  const [user, setUser] = useState<AuthUser | null>(null);
  const [isLoading, setIsLoading] = useState(true);

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

  const checkAuth = useCallback(async () => {
    const storedToken = localStorage.getItem(APEX_TOKEN);

    if (storedToken) {
      apiClient.setToken(storedToken);

      const urlScope = getCurrentUrlScope();

      try {
        // --- ATTEMPT 1: Context-Aware Check ---
        const user = await apiClient.auth.getMe();
        const userScope = user.scope || 'root';

        const isAllowed = userScope === 'root' || userScope === urlScope;

        if (isAllowed && (user.role + '').toLowerCase() === 'admin') {
          setUser(user);
        } else {
          throw new Error('Scope Mismatch');
        }
      } catch (e) {
        // --- ATTEMPT 2: Scope-Aware Fallback ---
        if (urlScope !== 'root') {
          try {
            const storedUserStr = localStorage.getItem(APEX_USER);
            let fallbackClient = new ApexKit(APP_CONFIG.apiBaseUrl); // Default to Root

            // [FIXED] Detect if the logged-in user belongs to a Tenant,
            // and target the fallback client directly to that Tenant space instead of Root.
            if (storedUserStr) {
              const storedUser = JSON.parse(storedUserStr);
              if (storedUser.scope && storedUser.scope.startsWith('tenant:')) {
                const tenantId = storedUser.scope.replace('tenant:', '');
                fallbackClient = fallbackClient.tenant(tenantId);
              }
            }

            fallbackClient.setToken(storedToken);
            const parentUser = await fallbackClient.auth.getMe();

            const parentUserScope = parentUser.scope || 'root';

            // Allow if Root, or if Tenant Admin is accessing a Sandbox (the backend verifies ownership)
            const isAuthorized =
              parentUserScope === 'root' ||
              (parentUserScope.startsWith('tenant:') && urlScope.startsWith('sandbox:'));

            if (isAuthorized && (parentUser.role + '').toLowerCase() === 'admin') {
              setUser(parentUser);
            } else {
              logout();
            }
          } catch (rootErr) {
            logout();
          }
        } else {
          logout();
        }
      }
    } else {
      setUser(null);
    }
    setIsLoading(false);
  }, []);

  useEffect(() => {
    checkAuth();
  }, [checkAuth]);

  const login = async (e: string, p: string) => {
    const res = await authService.login(e, p);
    setUser(res.user);
    localStorage.setItem(APEX_USER, JSON.stringify(res.user));
    checkAuth();
  };

  const logout = async () => {
    try {
      await authService.logout();
    } catch {}
    setUser(null);
    localStorage.removeItem(APEX_USER);
    localStorage.removeItem(APEX_TOKEN);
    if (window.location.pathname !== '/_dashboard/login') {
      window.location.href = window.location.pathname;
    }
  };

  return (
    <AuthContext.Provider value={{ user, login, logout, checkAuth, isLoading }}>
      {children}
    </AuthContext.Provider>
  );
};

export const useAuth = () => {
  const context = useContext(AuthContext);
  if (!context) {
    throw new Error('useAuth must be used within an AuthProvider');
  }
  return context;
};