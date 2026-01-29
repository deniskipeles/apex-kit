import { AppSettings } from '../../../types';
import { apiClient, pb } from '../../../lib/apiClient'; // Import pb

// We map the API response (snake_case keys from Rust) to Frontend (camelCase)
const mapToFrontend = (apiData: any): AppSettings => {
    return {
        appName: apiData.app_name || 'ApexKit',
        appUrl: apiData.app_url || 'http://localhost:3000',
        allowPublicRegistration: apiData.allow_public_registration || false,
        theme: apiData.theme || 'system',
        appLogo: apiData.app_logo || '',
        logoWidth: apiData.logo_width || '',
        logoHeight: apiData.logo_height || '',
        logRetentionDays: apiData.log_retention_days || 30,
        maxSiteSizeMb: apiData.max_site_size_mb || 5,
        smtp: {
            enabled: apiData.smtp?.enabled || false,
            host: apiData.smtp?.host || '',
            port: apiData.smtp?.port || 587,
            username: apiData.smtp?.username || '',
            password: apiData.smtp?.password || '', // Will be ******
            fromEmail: apiData.smtp?.from_email || '',
            template_welcome: apiData.smtp?.template_welcome || '',
            template_reset: apiData.smtp?.template_reset || '',
            template_verify: apiData.smtp?.template_verify || '',
        },
        storage: {
            activeDriver: apiData.storage?.active_driver || 'local',
            s3: {
                enabled: apiData.storage?.s3?.enabled || false,
                provider: apiData.storage?.s3?.provider || 'aws',
                bucket: apiData.storage?.s3?.bucket || '',
                region: apiData.storage?.s3?.region || '',
                endpoint: apiData.storage?.s3?.endpoint || '',
                accessKey: apiData.storage?.s3?.access_key || '',
                secretKey: apiData.storage?.s3?.secret_key || '' // Will be ******
            }
        },
        backups: { 
            enabled: apiData.backups?.enabled || false, 
            schedule: apiData.backups?.schedule || '0 0 * * *', 
            retention: apiData.backups?.retention || 7, 
            destination: apiData.backups?.destination || 'local',
            includeUploads: apiData.backups?.include_uploads || false,
            includeIndexes: apiData.backups?.include_indexes || false,
        },
        cronJobs: apiData.cron_jobs || [],
        apiTokens: [],
        security: {
            corsAllowAll: apiData.security?.cors_allow_all ?? true, // Default to true if missing
            corsOrigins: apiData.security?.cors_origins || ''
        },
        ai: {
            enabled: apiData.ai?.enabled || false,
            apiKey: apiData.ai?.api_key || '',
            provider: apiData.ai?.provider || ''
        },
    };
};

const mapToApi = (settings: Partial<AppSettings>): any => {
    const payload: any = {};
    if (settings.appName) payload.app_name = settings.appName;
    if (settings.appUrl) payload.app_url = settings.appUrl;
    if (settings.allowPublicRegistration !== undefined) payload.allow_public_registration = settings.allowPublicRegistration;
    if (settings.theme) payload.theme = settings.theme;
    // Map Logo Fields back to API
    if (settings.appLogo !== undefined) payload.app_logo = settings.appLogo;
    if (settings.logoWidth !== undefined) payload.logo_width = settings.logoWidth;
    if (settings.logoHeight !== undefined) payload.logo_height = settings.logoHeight;

    if (settings.logRetentionDays !== undefined) payload.log_retention_days = settings.logRetentionDays;
    if (settings.maxSiteSizeMb !== undefined) payload.max_site_size_mb = settings.maxSiteSizeMb;

    if (settings.smtp) {
        payload.smtp = {
            enabled: settings.smtp.enabled,
            host: settings.smtp.host,
            port: settings.smtp.port,
            username: settings.smtp.username,
            password: settings.smtp.password, // API handles masking
            from_email: settings.smtp.fromEmail
        };
    }

    if (settings.storage) {
        payload.storage = {
            active_driver: settings.storage.activeDriver,
            s3: {
                enabled: settings.storage.s3.enabled,
                provider: settings.storage.s3.provider,
                bucket: settings.storage.s3.bucket,
                region: settings.storage.s3.region,
                endpoint: settings.storage.s3.endpoint,
                access_key: settings.storage.s3.accessKey,
                secret_key: settings.storage.s3.secretKey
            }
        };
    }
    if (settings.security) {
        payload.security = {
            cors_allow_all: settings.security.corsAllowAll,
            cors_origins: settings.security.corsOrigins
        };
    }
    if (settings.cronJobs) {
        payload.cron_jobs = settings.cronJobs;
    }

    if (settings.ai) {
        payload.ai = {
            enabled: settings.ai.enabled,
            api_key: settings.ai.apiKey,
            provider: settings.ai.provider
        }
    }

    if (settings.backups) {
        payload.backups = {
            enabled: settings.backups.enabled,
            schedule: settings.backups.schedule,
            retention: settings.backups.retention,
            destination: settings.backups.destination,
            include_uploads: settings.backups.includeUploads,
            include_indexes: settings.backups.includeIndexes,
        };
    }

    return payload;
};

export const settingsService = {
    get: async (): Promise<AppSettings> => {
        try {
            const res = await pb.admins.getSettings();
            return mapToFrontend(res);
        } catch (e) {
            console.error("Failed to load settings", e);
            // Return default on failure to not crash UI
            return mapToFrontend({});
        }
    },

    update: async (settings: Partial<AppSettings>): Promise<AppSettings> => {
        const payload = mapToApi(settings);
        const res = await pb.admins.updateSettings(payload);
        return mapToFrontend(res);
    },

    // Keep mocks for untethered features
    testEmail: async (email: string) => true,
    generateToken: async (name: string, role: string = 'admin') => {
        // Use the new apiClient method
        const res = await apiClient.keys.create(name, role);
        // Map to frontend structure
        return {
            token: {
                id: res.info.id,
                name: res.info.name,
                key: res.key.substring(0, 8) + '...', // Masked prefix
                created: res.info.created
            },
            rawKey: res.key
        };
    },
};