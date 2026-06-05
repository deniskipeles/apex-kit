export type FieldType =
  | 'text'
  | 'string'
  | 'number'
  | 'bool'
  | 'email'
  | 'url'
  | 'date'
  | 'select'
  | 'json'
  | 'file'
  | 'blob'
  | 'relation'
  | 'vector'
  | 'owner';

export interface SchemaField {
  name: string;
  type: FieldType;
  required: boolean;
  unique?: boolean;
  ose_indexed?: boolean;
  sql_indexed?: boolean;
  auto?: boolean;
  system?: boolean;
  default?: any;
  uid: string;
  position: number;
  vectorize?: boolean;
  // --- Dynamic Validation ---

  // Number
  min?: number | null;
  max?: number | null;

  // String / Text / Blob
  minLength?: number | null; // Maps to min_length
  maxLength?: number | null; // Maps to max_length
  pattern?: string;

  // Select
  options?: string[];

  // File / Blob
  mimeTypes?: string[]; // Maps to mime_types
  maxSize?: number | null; // Maps to max_size (bytes)

  // Vector
  dimension?: number | null;

  // Relation / Owner
  relationTo?: string; // Target Collection ID/Name

  // Frontend Internal (for renaming tracking)
  originalName?: string;
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
  type: 'base' | 'auth' | 'view';
  schema: SchemaField[];
  rules?: CollectionRules;
  fieldHistory?: Record<string, string[]>; // "current_name" -> ["old1", "old2"]
  created: string;
  updated: string;
  compositeUnique: string[];
}
