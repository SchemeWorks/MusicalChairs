// Single source of truth for the Solana JSON-RPC endpoint.
//
// Kept in its own dependency-free module (no @solana/web3.js import) so it can be
// imported from app-wide code — e.g. useQueries' SOL balance hook — without
// dragging the web3.js bundle into the main chunk. web3.js-using callers
// (sendSolDeposit) import this same constant.
//
// Devnet for now; flips to a mainnet endpoint at the M3 cutover. The M3 flip must
// update this constant AND the asset-canister CSP `connect-src` together (see
// frontend/public/.ic-assets.json), or browser fetches to the new host are blocked.
export const SOLANA_RPC_ENDPOINT = 'https://api.devnet.solana.com';
