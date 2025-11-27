
export enum CollectionType {
  BASE = 'base',
  AUTH = 'auth',
  VIEW = 'view'
}

export interface SchemaField {
  name: string;
  type: 'text' | 'number' | 'bool' | 'email' | 'url' | 'date' | 'select' | 'json' | 'file' | 'relation';
  required: boolean;
  unique?: boolean;
  system?: boolean;
  options?: string[]; // For select
  min?: number | null;
  max?: number | null;
  pattern?: string;
  relationTo?: string; // ID or Name of the collection this field points to
  minLength?: number | null;
  maxLength?: number | null;
  maxSize?: number | null; // bytes
  mimeTypes?: string[];
  default?: any;
}

export interface CollectionRules {
  read?: string;
  create?: string;
  update?: string;
  delete?: string;
}

export interface Collection {
  id: string;
  name: string;
  type: CollectionType;
  schema: SchemaField[];
  rules?: CollectionRules;
  created: string;
  updated: string;
}