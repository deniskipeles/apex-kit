import { apiClient, pb } from '../../../lib/apiClient'; // Import pb instance directly if needed
import { AppRecord } from '../../../types';

export const recordsService = {
  list: (collectionId: string, page = 1, perPage = 20, expand = '', filter = {}, sort = '-id') => {
    return apiClient.records.list(collectionId, page, perPage, expand, filter);
  },
  create: (collectionId: string, data: any) => {
    return apiClient.records.create(collectionId, data);
  },
  update: (collectionId: string, recordId: string, data: any) => {
    return apiClient.records.update(collectionId, recordId, data);
  },
  delete: async (collectionId: string, recordId: string) => {
    await apiClient.records.delete(collectionId, recordId);
  },
  instantSearch: (collectionId: string, query: string) => {
    return apiClient.records.instantSearch(collectionId, query);
  },
  searchRecords: (collectionId: string, query: string) => {
    return apiClient.records.recordsSearchOSE(collectionId, query);
  },
  searchRecordsSQL: (collectionId: string, query: any) => {
    return apiClient.records.recordsSearchSQL(collectionId, query);
  },
  getOne: async (collectionId: string, recordId: string, expand = '') => {
    return await apiClient.records.getOne(collectionId, recordId, expand);
  },
};
