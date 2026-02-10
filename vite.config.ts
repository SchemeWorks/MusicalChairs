import { defineConfig, loadEnv } from 'vite';
import react from '@vitejs/plugin-react';
import path from 'path';

export default defineConfig(({ mode }) => {
  // Load env file based on `mode` in the current working directory.
  const env = loadEnv(mode, process.cwd(), '');
  
  const DFX_NETWORK = env.DFX_NETWORK || 'local';
  const CANISTER_ID_BACKEND = env.CANISTER_ID_BACKEND || 'uxrrr-q7777-77774-qaaaq-cai';
  
  return {
    root: 'frontend',
    plugins: [
      react(),
    ],
    resolve: {
      alias: {
        '@': path.resolve(__dirname, 'frontend/src'),
      },
    },
    build: {
      outDir: 'dist',
      emptyOutDir: true,
    },
    server: {
      proxy: {
        '/api': {
          target: 'http://localhost:4943',
          changeOrigin: true,
        },
      },
    },
    define: {
      'process.env.DFX_NETWORK': JSON.stringify(DFX_NETWORK),
      'process.env.CANISTER_ID_BACKEND': JSON.stringify(CANISTER_ID_BACKEND),
      global: 'globalThis',
    },
    optimizeDeps: {
      esbuildOptions: {
        define: {
          global: 'globalThis',
        },
      },
    },
  };
});
