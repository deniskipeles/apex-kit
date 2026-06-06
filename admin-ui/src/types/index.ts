export * from './collection';
export * from './record';
export * from './user';
export * from './settings';
export * from './api';
export * from './files';

export interface AppVersions {
  root: string;
  api: string;
  core: string;
  vector: string;
}

// Script Types
export interface Script {
  id: string;
  name: string;
  trigger_type:
    | 'manual'
    | 'before_create'
    | 'after_create'
    | 'before_update'
    | 'after_update'
    | 'before_delete'
    | 'after_delete'
    | 'cron';
  code: string;
  target_collection: string;
  active: boolean;
  visibility: 'public' | 'private';
}

// Templates
export interface Template {
  id: string;
  slug: string;
  content: string;
  script_id: string | null;
  created_at: string;
}

export interface AiAction {
  id: string;
  slug: string;
  name: string;
  model: string;
  system_prompt?: string;
  template: string;
  config?: any;
}

export interface AiSession {
  id: string;
  name: string;
  messages: Array<{ role: 'user' | 'assistant'; content: string }>;
  current_manifest?: any;
  pending_manifest?: any;
  diff_summary?: string | null;
  last_error?: string;
  created_at: string;
}

export interface ChatMessage {
  role: 'user' | 'assistant';
  content: string;
}

export interface AppManifest {
  app_name: string;
  collections: Array<any>;
  scripts: Array<any>;
  templates: Array<any>;
}

export interface Plugin {
  id: string;
  name: string;
  version: string;
  description?: string;
  manifest: any;
  created_at: string;
}

export interface SiteFile {
  path: string;
  size: number;
}
