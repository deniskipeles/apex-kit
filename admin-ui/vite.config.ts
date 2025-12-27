import path from 'path';
import { defineConfig, loadEnv } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig(({ mode }) => {
    // This loads the .env file based on the mode (dev/prod)
    const env = loadEnv(mode, process.cwd(), '');
    const isDev = mode === 'development';
    const apiUrl = isDev
      ? env.VITE_API_URL?.trim() || 'http://127.0.0.1:5000'
      : '/';

    return {
      // 1. ADD THIS LINE: Set the base path for assets
      base: '/_dashboard/', 
      server: {
        port: 5000,
        host: '0.0.0.0',
        // Ensure this matches the actual URL you access in the browser
        allowedHosts: ["5173-01jp06r43r3zeenk9nb1sbyeyg.cloudspaces.litng.ai"], 
        proxy: {
          '/api': {
            // FIX: Use 'env' variable, provide a fallback if missing
            target: apiUrl, 
            changeOrigin: true,
            secure: false,
          },
          '/uploads': {
             // FIX: Use 'env' variable
             target: apiUrl,
             changeOrigin: true
          }
        }
      },
      plugins: [react()],
      define: {
        // This makes these variables available to your Client-side React code
        'process.env.API_KEY': JSON.stringify(env.GEMINI_API_KEY),
        'process.env.GEMINI_API_KEY': JSON.stringify(env.GEMINI_API_KEY)
      },
      resolve: {
        alias: {
          '@': path.resolve(__dirname, '.'),
        }
      },
      build: {
        chunkSizeWarningLimit: 1600,
        rollupOptions: {
            output: {
                manualChunks: {
                    // 1. React & Routing
                    'vendor-react': ['react', 'react-dom', 'react-router-dom', 'zustand'],
                    // 2. UI Components & Icons
                    'vendor-ui': ['lucide-react', 'recharts'],
                    // 3. Utilities (Markdown, Sanitization)
                    'vendor-utils': ['dompurify', 'marked', 'turndown', 'turndown-plugin-gfm'],
                    // 4. Monaco Wrapper (The heavy editor logic)
                    'monaco-wrapper': ['@monaco-editor/react']
                }
            }
        }
      }
    };
});