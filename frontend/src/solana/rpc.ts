// Single source of truth for the Solana JSON-RPC endpoint.
//
// Kept in its own dependency-free module (no @solana/web3.js import) so it can be
// imported from app-wide code — e.g. useQueries' SOL balance hook — without
// dragging the web3.js bundle into the main chunk. web3.js-using callers
// (sendSolDeposit) import this same constant.
//
// Solana RPC endpoint for the browser. Used by the wallet-balance READ
// (useSolBalance) and the non-Phantom send fallback. The primary send path goes
// through Phantom's signAndSendTransaction and needs no RPC here.
//
// Sourced from VITE_SOLANA_RPC_URL (a gitignored .env.local, so the provider key
// never enters git) with the public endpoint as fallback. The public endpoint
// 403s browser blockhash/read calls, so set a domain-locked provider key
// (Helius/QuickNode) for reliable reads. Whatever host this resolves to MUST be
// in the asset-canister CSP connect-src (frontend/public/.ic-assets.json) or the
// browser fetch is blocked.
const ENV_RPC = process.env.VITE_SOLANA_RPC_URL;
export const SOLANA_RPC_ENDPOINT =
  ENV_RPC && ENV_RPC.trim() ? ENV_RPC.trim() : 'https://api.mainnet-beta.solana.com';
