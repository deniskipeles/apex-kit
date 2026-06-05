import { apiClient } from '../../../lib/apiClient';
import { StoredFile } from '../../../types';

export const filesService = {
  list: (page?: number, perPage?: number, search?: string) => {
    return apiClient.files.list(page, perPage, search);
  },
  upload: (file: File) => {
    return apiClient.files.upload(file);
  },
  delete: (id: string) => {
    return apiClient.files.delete(id);
  },
};
