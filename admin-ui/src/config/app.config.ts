const apiUrl = (import.meta as any).env.DEV
  ? (import.meta as any).env.VITE_API_URL?.trim() || 'http://127.0.0.1:5000'
  : (typeof window !== 'undefined' ? window.origin : 'http://127.0.0.1:5000').trim();

export const APP_CONFIG = {
  name: 'ApexKit Admin',
  version: '1.0.0',
  apiBaseUrl: apiUrl,
  defaultTheme: 'dark',
  pagination: {
    defaultPerPage: 20,
    options: [10, 20, 50, 100],
  },
};
