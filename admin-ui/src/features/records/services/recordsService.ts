import { apiClient, pb } from '../../../lib/apiClient'; // Import pb instance directly if needed
import { AppRecord } from '../../../types';

export const recordsService = {
  list: (collectionId: string, page = 1, perPage = 20, expand = '') => {
    return apiClient.records.list(collectionId, page, perPage, expand);
  },
  create: (collectionId: string, data: any) => {
    return apiClient.records.create(collectionId, data);
  },
  update: (collectionId: string, recordId: string, data: any) => {
    return apiClient.records.update(collectionId, recordId, data);
  },
  // UPDATE THIS FUNCTION:
  delete: async (collectionId: string, recordId: string) => {
    // Direct SDK usage or update apiClient to accept 2 args
    await pb.collection(collectionId).delete(recordId);
  },
  instantSearch: (collectionId: string, query: string) => {
    return apiClient.records.instantSearch(collectionId, query);
  },
};