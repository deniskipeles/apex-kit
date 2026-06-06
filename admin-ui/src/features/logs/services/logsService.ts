import { SystemLog } from '../../../types';
import { apiClient } from '../../../lib/apiClient';

export const logsService = {
  list: async (
    page = 1,
    perPage = 50,
    level = '',
    source = '',
    search = '',
    type = 'system'
  ): Promise<{ items: SystemLog[]; total: number }> => {
    return apiClient.logs.list(page, perPage, level, source, search, type);
  },
};
