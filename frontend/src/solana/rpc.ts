// Single source of truth for the Solana JSON-RPC endpoint.
//
// Kept in its own dependency-free module (no @solana/web3.js import) so it can be
// imported from app-wide code — e.g. useQueries' SOL balance hook — without
// dragging the web3.js bundle into the main chunk. web3.js-using callers
// (sendSolDeposit) import this same constant.
//
// Mainnet-beta as of the M3 cutover (2026-06-01). This constant AND the
// asset-canister CSP `connect-src` (frontend/public/.ic-assets.json) must stay
// in sync, or browser fetches to this host are blocked. The public endpoint is
// rate-limited; swap to a dedicated provider (Helius/QuickNode) if volume grows
// — remember to update the CSP connect-src to match the new host.
export const SOLANA_RPC_ENDPOINT = 'https://api.mainnet-beta.solana.com';
