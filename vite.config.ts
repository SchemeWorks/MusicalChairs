/// <reference types="vitest/config" />
import { defineConfig, loadEnv } from 'vite';
import react from '@vitejs/plugin-react';
import path from 'path';

export default defineConfig(({ mode }) => {
  // Load env file based on `mode` in the current working directory.
  const env = loadEnv(mode, process.cwd(), '');
  
  const DFX_NETWORK = env.DFX_NETWORK || 'local';
  const CANISTER_ID_BACKEND = env.CANISTER_ID_BACKEND || 'uxrrr-q7777-77774-qaaaq-cai';
  // Browser Solana RPC (domain-locked Helius key in .env.local). Exposed via
  // define because Vite's native import.meta.env loads from envDir=root('frontend'),
  // but our env files live at the repo root where this loadEnv reads.
  const VITE_SOLANA_RPC_URL = env.VITE_SOLANA_RPC_URL || '';
  
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
      'process.env.VITE_SOLANA_RPC_URL': JSON.stringify(VITE_SOLANA_RPC_URL),
      global: 'globalThis',
    },
    optimizeDeps: {
      esbuildOptions: {
        define: {
          global: 'globalThis',
        },
      },
    },
    test: {
      // @solana/web3.js pulls in rpc-websockets, whose CJS build `require()`s an
      // ESM-only `uuid` — fatal under Vitest's default Node externalization
      // (ERR_REQUIRE_ESM). Pre-bundling these with esbuild (the deps optimizer)
      // resolves the nested ESM/CJS interop so the deposit-builder tests
      // (sendSolDeposit.test.ts) can load @solana/web3.js.
      deps: {
        optimizer: {
          ssr: {
            enabled: true,
            include: ['@solana/web3.js', 'rpc-websockets'],
          },
        },
      },
    },
  };
});
