
import { AppRecord } from './record';

export interface ListResult<T> {
  items: T[];
  totalItems: number;
  page?: number;
  perPage?: number;
}

export interface AuthResponse {
  token: string;
  user: any;
}

export interface InstantResult {
  id: number;
  score: number;
  snippet: Record<string, any>; // The stored fields from Tantivy
}