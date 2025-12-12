
export const APP_CONFIG = {
  name: 'Tinybase Admin',
  version: '1.0.0',
  apiBaseUrl: (import.meta as any).env.VITE_API_URL || 'http://localhost:5000',
  defaultTheme: 'dark',
  pagination: {
    defaultPerPage: 20,
    options: [10, 20, 50, 100]
  }
};
