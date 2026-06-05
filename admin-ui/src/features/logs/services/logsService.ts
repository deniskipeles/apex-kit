import { SystemLog } from '../../../types';
import { apiClient } from '../../../lib/apiClient';
import { MOCK_LOGS, CHART_DATA } from '../../../constants';

// Mock delay helper
const delay = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

export const logsService = {
  list: async (): Promise<SystemLog[]> => {
    // In a real app, this would call apiClient.get('/logs')
    return apiClient.logs.list();
  },

  getStats: async () => {
    await delay(300);
    return CHART_DATA;
  },

  clearLogs: async () => {
    await delay(500);
    // Mock clearing logs
    return true;
  },
};
