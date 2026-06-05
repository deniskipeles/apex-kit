import { create } from 'zustand';
import { persist } from 'zustand/middleware';

interface AppSettings {
  appName: string;
  appUrl: string;
  theme: 'light' | 'dark' | 'system';
}

interface SettingsState {
  settings: AppSettings;
  updateSettings: (settings: Partial<AppSettings>) => void;
}

export const useSettingsStore = create<SettingsState>()(
  persist(
    (set) => ({
      settings: {
        appName: 'ApexKit Admin',
        appUrl: 'http://localhost:8090',
        theme: 'system',
      },
      updateSettings: (newSettings) =>
        set((state) => ({ settings: { ...state.settings, ...newSettings } })),
    }),
    { name: 'apexkit-settings' }
  )
);
