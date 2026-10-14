// Which Solana cluster the app is pointed at, for the wallet dropdown's cluster
// indicator (IMPL-46c). Derived from the RPC endpoint's HOST only: never its path
// or query, which carry provider API keys.

import { LOOPBACK_RPC_HOSTS } from './localModeFlag';

export type ClusterName = 'mainnet-beta' | 'devnet' | 'testnet' | 'localnet' | 'custom';

export interface ClusterInfo {
  cluster: ClusterName;
  /** What the indicator shows. */
  label: string;
  /** Host only (never a key), for a tooltip. */
  host: string;
}

export function clusterFromEndpoint(endpoint: string, opts: { localMode?: boolean } = {}): ClusterInfo {
  let host = '';
  try {
    host = new URL(endpoint).hostname.toLowerCase();
  } catch {
    host = '';
  }
  const local = opts.localMode === true || LOOPBACK_RPC_HOSTS.includes(host) || host.endsWith('.localhost');
  if (local) return { cluster: 'localnet', label: 'localnet', host };
  // Provider hostnames name the cluster: api.devnet.solana.com, devnet.helius-rpc.com,
  // x.solana-devnet.quiknode.pro, solana-mainnet.g.alchemy.com, api.mainnet-beta.solana.com ...
  if (/(^|[.-])devnet([.-]|$)/.test(host)) return { cluster: 'devnet', label: 'devnet', host };
  if (/(^|[.-])testnet([.-]|$)/.test(host)) return { cluster: 'testnet', label: 'testnet', host };
  if (/(^|[.-])mainnet([.-]|$)/.test(host)) return { cluster: 'mainnet-beta', label: 'mainnet-beta', host };
  return { cluster: 'custom', label: 'custom RPC', host };
}
