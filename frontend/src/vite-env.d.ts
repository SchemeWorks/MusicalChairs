/// <reference types="vite/client" />

declare const process: {
  env: {
    DFX_NETWORK: string;
    CANISTER_ID_BACKEND: string;
    CANISTER_ID_PONZI_MATH: string;
    VITE_SOLANA_RPC_URL: string;
  };
};
