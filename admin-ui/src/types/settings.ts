import { AiAction } from '.';

export interface SecurityConfig {
  corsAllowAll: boolean;
  corsOrigins: string;
  tenantTransparency: boolean;
  globalRateLimit?: number; // Root API limit (reqs/min)
  tenantFreeRateLimit?: number; // Free tenant API limit
  tenantProRateLimit?: number; // Pro tenant API limit
}

export interface SystemLog {
  id: string;
  level: 'info' | 'warning' | 'error' | 'success';
  message: string;
  timestamp: string;
  source: string;
  meta?: Record<string, any>; // [NEW] Optional JSON metadata (IP, User Agent, Payload)
}

export type ViewState =
  | 'ai-architect'
  | 'ai-actions'
  | 'dashboard'
  | 'collections'
  | 'collections-create'
  | 'collections-edit'
  | 'records'
  | 'files'
  | 'settings'
  | 'logs'
  | 'users'
  | 'scripts'
  | 'templates';

export interface SmtpConfig {
  enabled: boolean;
  host: string;
  port: number;
  username?: string;
  password?: string; // usually not returned by API, only set
  fromEmail: string;
  template_welcome: string;
  template_reset: string;
  template_verify: string;
}

export interface S3Config {
  enabled: boolean;
  provider: 'aws' | 'gcs' | 'minio' | 'digitalocean' | 'other';
  bucket: string;
  region: string;
  endpoint: string;
  accessKey: string;
  secretKey: string;
}

export interface StorageConfig {
  activeDriver: 'local' | 's3';
  s3: S3Config;
}

export interface BackupConfig {
  enabled: boolean;
  schedule: string; // Cron expression
  retention: number; // Days to keep
  destination: 'local' | 's3';

  includeDatabases?: boolean;
  includeVectors?: boolean;
  includeUploads?: boolean;
  includeIndexes?: boolean;
  includeStaticSite?: boolean;
}

export interface CronJob {
  id: string;
  name: string;
  schedule: string;
  payload: string;
  active: boolean;
}

export interface ApiToken {
  id: string;
  name: string;
  key: string; // Partial key for display
  created: string;
}

export interface ApiKey {
  id: string;
  name: string;
  prefix: string;
  role: string;
  scope: string;
  bypass_cors: string | boolean;
  created: string;
}

export interface AIProvider {
  enabled: boolean;
  apiKey: string;
  provider: string;
}

export interface AppSettings {
  appName: string;
  appUrl: string;
  allowPublicRegistration: boolean;
  theme: 'light' | 'dark' | 'system';
  appLogo?: string;
  logoWidth?: string;
  logoHeight?: string;
  smtp: SmtpConfig;
  storage: StorageConfig;
  backups: BackupConfig;
  cronJobs: CronJob[];
  apiTokens: ApiToken[];
  security: SecurityConfig;
  logRetentionDays: number;
  maxSiteSizeMb?: number;
  ai: AIProvider;
}

export interface TenantStats {
  storage_mb: number;
  max_storage_mb: number;
  vector_count: number;
  max_vectors: number;
  ai_requests: number;
  max_ai_requests: number;
}

export interface Tenant {
  id: string;
  name?: string;
  status: string; // 'active', 'suspended'
  tier: string; // 'free', 'pro'
  stats: TenantStats;
  created_at: string;
}

export interface SandboxMetadata {
  id: string;
  name: string | null;
  status: string;
  expires_at: string | null;
  scope: string;
  tenant_id: string | null;
  current_storage_mb: number;
  max_storage_mb: number;
}
