
export const APP_CONFIG = {
  name: 'Tinybase Admin',
  version: '1.0.0',
  apiBaseUrl: process.env.REACT_APP_API_URL || 'http://localhost:8090',
  defaultTheme: 'dark',
  pagination: {
    defaultPerPage: 20,
    options: [10, 20, 50, 100]
  }
};
