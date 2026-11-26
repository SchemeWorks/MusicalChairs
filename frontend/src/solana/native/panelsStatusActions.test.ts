// IMPL-46c P2: the status card's "Check status" is READ-ONLY (reconcile on the ORIGIN cluster) and its
// "Resend the same transaction" is a separate, guarded action. These tests run the card's two handlers
// (checkTxStatus / resendSameTx) against a result made by the REAL NativeClient on fake connections:
// A unknown -> the app switches its active identity to B -> Check status reads A's original signature
// with no send, no rebuild and no re-sign.
import { afterEach, describe, expect, it, vi } from 'vitest';
import { PublicKey } from '@solana/web3.js';
import { createNativeClient } from './client';
import { CLUSTER_DOMAINS, PUBLIC_GENESIS_HASHES, type NativeClusterConfig, type NativeClusterIdentity } from './cluster';
import { poolAddress } from './accounts';
import { buildAccount, fakeConnection, fakeWallet, type FakeAccount } from './testFixtures';
import { idl } from './codecs';
import { setNativeClusterIdentity } from './queries';
import { checkTxStatus, resendSameTx } from './panelsStatus';
import { describeOrigin } from './panelsCluster';
import type { NativeTxResult } from './types';

const PROGRAM = new PublicKey(idl.address);
const noSleep = async () => {};
const LOCALNET: NativeClusterConfig = { expectedClusterDomain: 'localnet', programId: idl.address };
const DEVNET: NativeClusterConfig = { expectedClusterDomain: 'devnet', programId: idl.address };
const ID_A: NativeClusterIdentity = { cluster: LOCALNET, rpcEndpoint: 'http://127.0.0.1:8899' };
const ID_B: NativeClusterIdentity = { cluster: DEVNET, rpcEndpoint: 'https://api.devnet.solana.com' };
const confirmed = { slot: 5, confirmations: 1, err: null, confirmationStatus: 'confirmed' as const };

function poolWorld(clusterDomain: Uint8Array) {
  const accounts = new Map<string, FakeAccount>();
  accounts.set(poolAddress().toBase58(), {
    owner: PROGRAM,
    data: buildAccount('Pool', { roundId: 1n, activeConfigVersion: 1, participantCount: 0n, clusterDomain }),
  });
  return accounts;
}

/** An open_position whose send died after signing on cluster A: the outcome is unknown. */
async function unknownOnA(statuses: ReturnType<typeof vi.fn>) {
  setNativeClusterIdentity(ID_A);
  const a = fakeConnection(poolWorld(CLUSTER_DOMAINS.localnet), {
    rpcEndpoint: ID_A.rpcEndpoint,
    sendRawTransaction: vi.fn().mockRejectedValueOnce(new Error('Request timed out')).mockResolvedValue('ignored'),
    getSignatureStatuses: statuses,
  });
  const b = fakeConnection(poolWorld(CLUSTER_DOMAINS.devnet), { rpcEndpoint: ID_B.rpcEndpoint, getGenesisHash: vi.fn(async () => PUBLIC_GENESIS_HASHES.devnet) });
  const wallet = fakeWallet();
  const client = createNativeClient({ connection: a, wallet, sessionPubkey: wallet.publicKey!, sleep: noSleep, maxPolls: 4, cluster: ID_A.cluster });
  const tx = await client.openPosition({ plan: 'Simple', gross: 1_000_000_000n, nonce: 42n });
  expect(tx.status).toBe('unknown');
  return { a, b, wallet, tx };
}

afterEach(() => setNativeClusterIdentity(null));

describe('Check status after the active cluster switched', () => {
  it('reads A\'s ORIGINAL signature on A: no send, no rebuild, no re-sign, B untouched', async () => {
    const { a, b, wallet, tx } = await unknownOnA(vi.fn().mockResolvedValueOnce({ value: [null] }).mockResolvedValue({ value: [confirmed] }));
    setNativeClusterIdentity(ID_B); // the app now points at cluster B

    const out = await checkTxStatus(tx);

    expect(out).toMatchObject({ ok: true, update: { signature: tx.signature, status: 'confirmed' }, note: null });
    expect(a.getSignatureStatuses).toHaveBeenCalled();
    expect(a.getSignatureStatuses.mock.calls.every((call: unknown[]) => (call[0] as string[]).length === 1 && (call[0] as string[])[0] === tx.signature)).toBe(true);
    expect(a.sendRawTransaction).toHaveBeenCalledTimes(1); // only the original broadcast
    expect(wallet.signTransaction).toHaveBeenCalledTimes(1);
    expect(a.getLatestBlockhash).toHaveBeenCalledTimes(1);
    expect(a.simulateTransaction).toHaveBeenCalledTimes(1);
    for (const name of ['sendRawTransaction', 'simulateTransaction', 'getLatestBlockhash', 'getSignatureStatuses', 'getAccountInfo', 'getGenesisHash'] as const) {
      expect(b[name], `cluster B ${name}`).not.toHaveBeenCalled();
    }
    expect(tx.nonce).toBe(42n);
  });

  it('a status that stays unknown says so, names the origin cluster and the last RPC error, and re-sends nothing', async () => {
    const { a, tx } = await unknownOnA(vi.fn().mockRejectedValue(new Error('RPC unreachable')));
    setNativeClusterIdentity(ID_B);

    const out = await checkTxStatus(tx);

    expect(out.ok).toBe(true);
    if (!out.ok) return;
    expect(out.update.status).toBe('unknown');
    expect(out.note).toMatch(/Still can't tell on localnet \(127\.0\.0\.1:8899\)/);
    expect(out.note).toMatch(/Last RPC error: Request timed out/);
    expect(out.note).toMatch(/Nothing was re-sent/);
    expect(a.sendRawTransaction).toHaveBeenCalledTimes(1);
  });

  it('a read that throws is surfaced as an error naming the origin, never swallowed', async () => {
    const broken = {
      origin: describeOriginSource(),
      reconcile: vi.fn(async () => {
        throw new Error('socket hang up');
      }),
    } as unknown as Pick<NativeTxResult, 'reconcile' | 'origin'>;
    const out = await checkTxStatus(broken);
    expect(out).toEqual({ ok: false, error: 'Could not read the status on localnet (127.0.0.1:8899): socket hang up' });
  });
});

describe('Resend the same transaction', () => {
  it('before any switch it re-sends the IDENTICAL signed bytes (same signature, no rebuild, no re-sign)', async () => {
    const { a, wallet, tx } = await unknownOnA(vi.fn().mockResolvedValue({ value: [null] }));
    const out = await resendSameTx(tx);
    expect(out.ok).toBe(true);
    expect(a.sendRawTransaction).toHaveBeenCalledTimes(2);
    const [first, second] = a.sendRawTransaction.mock.calls.map((call: unknown[]) => Buffer.from(call[0] as Uint8Array));
    expect(second.equals(first)).toBe(true);
    expect(wallet.signTransaction).toHaveBeenCalledTimes(1);
    expect(a.getLatestBlockhash).toHaveBeenCalledTimes(1);
    expect(tx.signature).toBe(out.ok ? out.update.signature : '');
    expect(tx.nonce).toBe(42n);
  });

  it('after a switch it is refused with the reason and the ORIGIN cluster, and nothing is sent anywhere', async () => {
    const { a, b, wallet, tx } = await unknownOnA(vi.fn().mockResolvedValue({ value: [null] }));
    setNativeClusterIdentity(ID_B);

    const out = await resendSameTx(tx);

    expect(out.ok).toBe(false);
    if (out.ok) return;
    expect(out.error).toMatch(/changed since this transaction was sent, so it was not re-sent/);
    expect(out.error).toMatch(/sent on localnet \(127\.0\.0\.1:8899\)/);
    expect(out.error).toMatch(/Check status/);
    expect(a.sendRawTransaction).toHaveBeenCalledTimes(1);
    expect(a.getSignatureStatuses).not.toHaveBeenCalled(); // the refused retry made no network call at all
    expect(b.sendRawTransaction).not.toHaveBeenCalled();
    expect(wallet.signTransaction).toHaveBeenCalledTimes(1);
  });
});

describe('origin description', () => {
  it('names the cluster and the RPC host only (never a path or API key)', () => {
    expect(describeOrigin({ cluster: LOCALNET, rpcEndpoint: 'http://127.0.0.1:8899' }).label).toBe('localnet (127.0.0.1:8899)');
    const keyed = describeOrigin({ cluster: DEVNET, rpcEndpoint: 'https://devnet.helius-rpc.com/?api-key=SECRET-123' });
    expect(keyed.label).toBe('devnet (devnet.helius-rpc.com)');
    expect(JSON.stringify(keyed)).not.toContain('SECRET');
  });
  it('falls back to the endpoint when the origin is unpinned, and to "unknown cluster" with nothing', () => {
    expect(describeOrigin({ cluster: null, rpcEndpoint: 'https://api.devnet.solana.com' }).cluster).toBe('devnet');
    expect(describeOrigin(undefined).label).toBe('unknown cluster');
    expect(describeOrigin({ cluster: { expectedClusterDomain: new Uint8Array(32) }, rpcEndpoint: null }).cluster).toBe('custom deployment');
  });
});

function describeOriginSource() {
  return { clusterKey: 'k', cluster: LOCALNET, rpcEndpoint: ID_A.rpcEndpoint, generation: 0 };
}
