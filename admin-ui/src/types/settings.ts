import { AiAction } from ".";

export interface SecurityConfig {
  corsAllowAll: boolean;
  corsOrigins: string;
}

export interface SystemLog {
  id: string;
  level: 'info' | 'warning' | 'error' | 'success';
  message: string;
  timestamp: string;
  source: string;
}

export type ViewState =  'ai-architect' | 'ai-actions' | 'dashboard' | 'collections' | 'collections-create' | 'collections-edit' | 'records' | 'files' | 'settings' | 'logs' | 'users' | 'scripts' | 'templates';

export interface SmtpConfig {
  enabled: boolean;
  host: string;
  port: number;
  username?: string;
  password?: string; // usually not returned by API, only set
  fromEmail: string;
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

export interface AIProvider{
  enabled: Boolean;
  apiKey: string;
  provider: string;
}

export interface AppSettings {
  appName: string;
  appUrl: string;
  allowPublicRegistration: boolean;
  theme: 'light' | 'dark' | 'system';
  smtp: SmtpConfig;
  storage: StorageConfig;
  backups: BackupConfig;
  cronJobs: CronJob[];
  apiTokens: ApiToken[];
  security: SecurityConfig;
  logRetentionDays: Number;
  ai:AIProvider;
}

