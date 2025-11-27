import path from 'path';
import { defineConfig, loadEnv } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig(({ mode }) => {
    // This loads the .env file based on the mode (dev/prod)
    const env = loadEnv(mode, process.cwd(), '');

    return {
      // 1. ADD THIS LINE: Set the base path for assets
      base: '/_dashboard/', 
      server: {
        port: 3000,
        host: '0.0.0.0',
        // Ensure this matches the actual URL you access in the browser
        allowedHosts: ["5173-01jp06r43r3zeenk9nb1sbyeyg.cloudspaces.litng.ai"], 
        proxy: {
          '/api': {
            // FIX: Use 'env' variable, provide a fallback if missing
            target: env.VITE_API_URL || 'http://127.0.0.1:5000', 
            changeOrigin: true,
            secure: false,
          },
          '/uploads': {
             // FIX: Use 'env' variable
             target: env.VITE_API_URL || 'http://127.0.0.1:5000',
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
      }
    };
});