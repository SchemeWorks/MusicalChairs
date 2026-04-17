/// <reference types="vite/client" />

declare const process: {
  env: {
    DFX_NETWORK: string;
    CANISTER_ID_BACKEND: string;
  };
};
