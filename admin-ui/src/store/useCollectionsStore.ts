
import { create } from 'zustand';
import { Collection } from '../types';
import { apiClient } from '../lib/apiClient';

interface CollectionsState {
  collections: Collection[];
  isLoading: boolean;
  error: string | null;
  activeCollection: Collection | null;
  
  fetchCollections: () => Promise<void>;
  createCollection: (data: Partial<Collection>) => Promise<void>;
  updateCollection: (id: string, data: Partial<Collection>) => Promise<void>;
  deleteCollection: (id: string) => Promise<void>;
  setActiveCollection: (col: Collection | null) => void;
}

export const useCollectionsStore = create<CollectionsState>((set, get) => ({
  collections: [],
  isLoading: false,
  error: null,
  activeCollection: null,

  setActiveCollection: (col) => set({ activeCollection: col }),

  fetchCollections: async () => {
    set({ isLoading: true, error: null });
    try {
      const collections = await apiClient.collections.list();
      set({ collections, isLoading: false });
    } catch (err) {
      set({ isLoading: false, error: (err as Error).message });
    }
  },

  createCollection: async (data) => {
    set({ isLoading: true });
    try {
      const newCol = await apiClient.collections.create(data);
      set((state) => ({ 
        collections: [...state.collections, newCol],
        isLoading: false 
      }));
    } catch (err) {
      set({ isLoading: false, error: (err as Error).message });
      throw err;
    }
  },

  updateCollection: async (id, data) => {
    set({ isLoading: true });
    try {
      const updated = await apiClient.collections.update(id, data);
      set((state) => ({
        collections: state.collections.map(c => c.id === id ? updated : c),
        activeCollection: updated,
        isLoading: false
      }));
    } catch (err) {
      set({ isLoading: false, error: (err as Error).message });
      throw err;
    }
  },

  deleteCollection: async (id) => {
    set({ isLoading: true });
    try {
      await apiClient.collections.delete(id);
      set((state) => ({
        collections: state.collections.filter(c => c.id !== id),
        isLoading: false
      }));
    } catch (err) {
      set({ isLoading: false, error: (err as Error).message });
      throw err;
    }
  }
}));