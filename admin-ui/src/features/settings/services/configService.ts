import { apiClient } from '@/src/lib/apiClient';

export interface ConfigItem {
  key: string;
  value?: string;
  encrypted: boolean;
  updated_at: string;
}

export const configService = {
  list: async (): Promise<ConfigItem[]> => {
    const res = await apiClient.configs.list();
    return res.map((item: any) => ({
      key: item.key,
      value: item.value,
      encrypted: item.encrypted,
      updated_at: item.updated_at,
    }));
  },

  set: async (key: string, value: string, encrypt: boolean = false): Promise<void> => {
    await apiClient.configs.set(key, value, encrypt);
  },

  delete: async (key: string): Promise<void> => {
    await apiClient.configs.delete(key);
  },
};
