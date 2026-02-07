
import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import { AuthUser } from '../types';
import { apiClient } from '../lib/apiClient';
import { storage } from '../lib/storage';
import { APEX_AUTH } from '../constants';

interface AuthState {
  user: AuthUser | null;
  token: string | null;
  isLoading: boolean;
  login: (email: string, pass: string) => Promise<void>;
  logout: () => void;
  checkAuth: () => Promise<void>;
}

export const useAuthStore = create<AuthState>()(
  persist(
    (set) => ({
      user: null,
      token: null,
      isLoading: false,
      
      login: async (email, pass) => {
        set({ isLoading: true });
        try {
          const { user, token } = await apiClient.auth.login(email, pass);
          set({ user, token, isLoading: false });
        } catch (error) {
          set({ isLoading: false });
          throw error;
        }
      },
      
      logout: async () => {
        set({ isLoading: true });
        try {
            await apiClient.auth.logout();
        } finally {
            set({ user: null, token: null, isLoading: false });
        }
      },

      checkAuth: async () => {
        // Simplified check - in real app would verify token
        const token = storage.get<string>('auth-storage'); 
        if (!token) return;
        // validation logic here
      }
    }),
    {
      name: APEX_AUTH,
      partialize: (state) => ({ user: state.user, token: state.token }),
    }
  )
);
