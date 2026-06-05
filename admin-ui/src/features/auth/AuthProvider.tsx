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
import { ApexKit } from '@/src/lib/sdk'; // Import Class for fallback client
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

  const checkAuth = useCallback(async () => {
    const storedToken = localStorage.getItem(APEX_TOKEN);

    if (storedToken) {
      // 1. Ensure the main proxy client has the token
      apiClient.setToken(storedToken);

      const urlScope = getCurrentUrlScope();

      try {
        // --- ATTEMPT 1: Context-Aware Check ---
        // This hits /tenant/xyz/api/v1/auth/me if we are on a tenant URL.
        const user = await apiClient.auth.getMe();
        const userScope = user.scope || 'root';

        // Valid if:
        // A. User is Root (can access anything)
        // B. User's scope matches URL scope exactly
        const isAllowed = userScope === 'root' || userScope === urlScope;

        if (isAllowed && (user.role + '').toLowerCase() === 'admin') {
          setUser(user);
        } else {
          throw new Error('Scope Mismatch'); // Trigger fallback
        }
      } catch (e) {
        // --- ATTEMPT 2: Root Fallback ---
        // If the Context Check failed (e.g. Root Admin doesn't exist in Tenant DB yet),
        // or if we threw Scope Mismatch, we verify against the Root API.

        console.warn('Primary auth check failed or mismatched. Attempting Root Fallback...', e);

        // Only retry if we aren't already at root (prevent infinite loop)
        if (urlScope !== 'root') {
          try {
            // Create a FRESH client that points strictly to Root URL, bypassing the Proxy logic
            const rootClient = new ApexKit(APP_CONFIG.apiBaseUrl);
            rootClient.setToken(storedToken);

            const rootUser = await rootClient.auth.getMe();

            // If this token belongs to a Root Admin, allow access to Tenant/Sandbox
            if (rootUser.scope === 'root' && (rootUser.role + '').toLowerCase() === 'admin') {
              console.log('Root Admin authorized via fallback.');
              // We set the user in state.
              // Note: This user object comes from Root DB, so ID/Metadata matches Root identity.
              setUser(rootUser);
            } else {
              // Token is valid, but it's a different Tenant user trying to access wrong Tenant
              console.error('Access Denied: Valid token, but not Root and scope mismatch.');
              logout();
            }
          } catch (rootErr) {
            // Both checks failed. Token is invalid or expired.
            console.error('Root fallback failed. Token invalid.', rootErr);
            logout();
          }
        } else {
          // We were already at Root and it failed. Game over.
          logout();
        }
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
    setUser(res.user);
    localStorage.setItem(APEX_USER, JSON.stringify(res.user));
    // After login, ensure checkAuth logic runs to validate scope immediately
    checkAuth();
  };

  const logout = async () => {
    try {
      await authService.logout();
    } catch {}
    setUser(null);
    localStorage.removeItem(APEX_USER);
    localStorage.removeItem(APEX_TOKEN);
    // Force reload to clear any memory states/proxies if we are deep in a view
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
