// IMPL-46c: the wallet dropdown's cluster indicator, derived from the RPC endpoint host only.
import { describe, it, expect } from 'vitest';
import { clusterFromEndpoint } from './panelsCluster';

describe('cluster indicator', () => {
  it.each([
    ['https://api.mainnet-beta.solana.com', 'mainnet-beta'],
    ['https://mainnet.helius-rpc.com/?api-key=SECRET', 'mainnet-beta'],
    ['https://example.solana-mainnet.quiknode.pro/abc123/', 'mainnet-beta'],
    ['https://solana-mainnet.g.alchemy.com/v2/KEY', 'mainnet-beta'],
    ['https://api.devnet.solana.com', 'devnet'],
    ['https://devnet.helius-rpc.com/?api-key=SECRET', 'devnet'],
    ['https://example.solana-devnet.quiknode.pro/abc/', 'devnet'],
    ['https://api.testnet.solana.com', 'testnet'],
    ['http://127.0.0.1:8899', 'localnet'],
    ['http://localhost:8899', 'localnet'],
    ['http://[::1]:8899', 'localnet'],
    ['https://rpc.example.org/', 'custom'],
    ['not a url', 'custom'],
  ])('%s -> %s', (endpoint, cluster) => {
    expect(clusterFromEndpoint(endpoint).cluster).toBe(cluster);
  });

  it('reports "localnet" whenever the local-cluster mode is on', () => {
    expect(clusterFromEndpoint('https://api.mainnet-beta.solana.com', { localMode: true })).toMatchObject({ cluster: 'localnet', label: 'localnet' });
  });

  it('exposes the host for a tooltip and never the key in the path or query', () => {
    const info = clusterFromEndpoint('https://mainnet.helius-rpc.com/?api-key=SECRET-KEY-123');
    expect(info.host).toBe('mainnet.helius-rpc.com');
    expect(JSON.stringify(info)).not.toContain('SECRET');
    expect(info.label).toBe('mainnet-beta');
  });
});
