// IMPL-46b step 1, tests 4 and 5 (+ the send path around them): the NativeClient
// signs only for the session wallet, and an unknown outcome never rebuilds.
import { describe, it, expect, vi } from 'vitest';
import { Keypair, PublicKey, SystemProgram, Transaction, VersionedTransaction } from '@solana/web3.js';
import bs58 from 'bs58';
import {
  NativeSimulationRejectedError,
  NativeSimulationUnavailableError,
  NativeStateError,
  createNativeClient,
  simulateUnsigned,
  NativeSendRejectedError,
} from './client';
import { SignedTransactionMismatchError, WalletMismatchError } from './wallet';
import {
  CLUSTER_DOMAINS,
  PUBLIC_GENESIS_HASHES,
  NativeClusterMismatchError,
  NativeClusterNotConfiguredError,
  NativeClusterSwitchedError,
  clusterCacheKey,
  type NativeClusterConfig,
  type NativeClusterIdentity,
} from './cluster';
import { getNativeClusterGeneration, setNativeClusterIdentity } from './queries';
import { poolAddress } from './accounts';
import { SIM_OK, buildAccount, fakeConnection, fakeWallet, simCustom, simFailure, type FakeAccount } from './testFixtures';
import { bytesToHex, encodeInstructionData, idl } from './codecs';
import type { StatusUpdate } from './types';

const PROGRAM = new PublicKey(idl.address);
const noSleep = async () => {};

function world() {
  const accounts = new Map<string, FakeAccount>();
  accounts.set(poolAddress().toBase58(), {
    owner: PROGRAM,
    data: buildAccount('Pool', { roundId: 1n, activeConfigVersion: 1, participantCount: 0n }),
  });
  return accounts;
}

const confirmedStatus = { slot: 5, confirmations: 1, err: null, confirmationStatus: 'confirmed' as const };
const finalizedStatus = { slot: 6, confirmations: null, err: null, confirmationStatus: 'finalized' as const };

describe('wallet / session binding', () => {
  it('rejectsSendWhenSelectedWalletPubkeyDiffers', async () => {
    const sessionKey = Keypair.generate().publicKey.toBase58(); // what the SIWS session says
    const wallet = fakeWallet(); // a DIFFERENT wallet is selected in the extension
    const connection = fakeConnection(world());
    const client = createNativeClient({ connection, wallet, sessionPubkey: sessionKey, sleep: noSleep });

    await expect(client.openPosition({ plan: 'Simple', gross: 1_000_000_000n })).rejects.toBeInstanceOf(WalletMismatchError);
    await expect(client.openPosition({ plan: 'Simple', gross: 1_000_000_000n })).rejects.toThrow(/does not match your session/i);

    // Nothing was signed, broadcast, or even read: the check runs before any I/O.
    expect(wallet.signTransaction).not.toHaveBeenCalled();
    expect(connection.sendRawTransaction).not.toHaveBeenCalled();
    expect(connection.getLatestBlockhash).not.toHaveBeenCalled();
    expect(connection.getAccountInfo).not.toHaveBeenCalled();
    expect(connection.getMultipleAccountsInfo).not.toHaveBeenCalled();
  });

  it('every state-changing method refuses a mismatched wallet', async () => {
    const wallet = fakeWallet();
    const connection = fakeConnection(world());
    const client = createNativeClient({ connection, wallet, sessionPubkey: Keypair.generate().publicKey.toBase58(), sleep: noSleep });
    const calls: Array<[string, () => Promise<unknown>]> = [
      ['openPosition', () => client.openPosition({ plan: 'Simple', gross: 10_000_000n })],
      ['fundSeriesA', () => client.fundSeriesA({ gross: 50_000_000n })],
      ['withdrawSimple', () => client.withdrawSimple({ nonce: 1n, minimumReceive: 0n })],
      ['settleCompounding', () => client.settleCompounding({ nonce: 1n, minimumReceive: 0n })],
      ['closeSimplePosition', () => client.closeSimplePosition({ nonce: 1n })],
      ['claimBacker', () => client.claimBacker({})],
      ['advancePhase', () => client.advancePhase({})],
      ['discoverRows', () => client.discoverRows({})],
      ['applySenior', () => client.applySenior()],
      ['sealWeights', () => client.sealWeights()],
      ['allocateRows', () => client.allocateRows({})],
      ['pruneRows', () => client.pruneRows({})],
      ['commitTicket', () => client.commitTicket()],
      ['provisionPage', () => client.provisionPage({ page: { kind: 'Registry' }, index: 0n, funding: 'Caller' })],
      ['provisionCloseRecord', () => client.provisionCloseRecord({ closeId: 1n, funding: 'LockedClose' })],
      ['increaseProviderBudget', () => client.increaseProviderBudget({ closeId: 1n, extra: 1n })],
      ['buildCandidates', () => client.buildCandidates({ closeId: 1n })],
      ['requestDraw', () => client.requestDraw({ closeId: 1n })],
      ['captureDraw', () => client.captureDraw({ closeId: 1n })],
      ['applyPromotion', () => client.applyPromotion({ closeId: 1n })],
      ['expirePromotion', () => client.expirePromotion({ closeId: 1n })],
      ['finalizeRound', () => client.finalizeRound({ closeId: 1n })],
      ['composeExpireFinalizeOpen', () => client.composeExpireFinalizeOpen({ closeId: 1n, plan: 'Simple', gross: 10_000_000n })],
    ];
    for (const [name, call] of calls) {
      await expect(call(), name).rejects.toBeInstanceOf(WalletMismatchError);
    }
    expect(wallet.signTransaction).not.toHaveBeenCalled();
    expect(connection.sendRawTransaction).not.toHaveBeenCalled();
    expect(connection.getAccountInfo).not.toHaveBeenCalled();
    expect(connection.getLatestBlockhash).not.toHaveBeenCalled();
  });

  it('refuses a wallet that is not connected (no public key) and a session without a pubkey', async () => {
    const wallet = { ...fakeWallet(), publicKey: null };
    const client = createNativeClient({ connection: fakeConnection(world()), wallet, sessionPubkey: Keypair.generate().publicKey.toBase58(), sleep: noSleep });
    await expect(client.openPosition({ plan: 'Simple', gross: 1n })).rejects.toBeInstanceOf(WalletMismatchError);

    const connected = fakeWallet();
    const noSession = createNativeClient({ connection: fakeConnection(world()), wallet: connected, sessionPubkey: null, sleep: noSleep });
    await expect(noSession.openPosition({ plan: 'Simple', gross: 1n })).rejects.toBeInstanceOf(WalletMismatchError);
  });
});

describe('send path and unknown outcomes', () => {
  it('signs once, broadcasts the signed bytes and walks submitted -> confirmed -> finalized', async () => {
    const wallet = fakeWallet();
    const connection = fakeConnection(world(), {
      getSignatureStatuses: vi
        .fn()
        .mockResolvedValueOnce({ value: [null] })
        .mockResolvedValueOnce({ value: [confirmedStatus] })
        .mockResolvedValue({ value: [finalizedStatus] }),
    });
    const client = createNativeClient({ connection, wallet, sessionPubkey: wallet.publicKey!, sleep: noSleep, maxPolls: 6 });

    const result = await client.openPosition({ plan: 'Simple', gross: 1_000_000_000n, nonce: 7n });
    expect(result.nonce).toBe(7n);
    expect(result.status).toBe('submitted');
    expect(wallet.signTransaction).toHaveBeenCalledTimes(1);

    const signed = wallet.signTransaction.mock.calls[0][0] as Transaction;
    const raw = connection.sendRawTransaction.mock.calls[0][0] as Uint8Array;
    expect(Buffer.from(raw).equals(signed.serialize())).toBe(true);
    expect(result.signature).toBe(bs58.encode(signed.signature!));

    // open_position carries the encoded args (plan, gross, nonce, expected round/config).
    const ix = signed.instructions[signed.instructions.length - 1];
    expect(ix.programId.equals(PROGRAM)).toBe(true);
    expect(bytesToHex(ix.data)).toBe(
      bytesToHex(encodeInstructionData('open_position', { plan: 'Simple', gross: 1_000_000_000n, clientNonce: 7n, expectedRoundId: 1n, expectedConfig: 1 })),
    );

    const seen: StatusUpdate[] = [];
    for await (const update of result.status$) seen.push(update);
    expect(seen.map((u) => u.status)).toEqual(['submitted', 'confirmed', 'finalized']);
  });

  it('unknownOutcomeKeepsSignatureAndNonceAndDoesNotRebuild', async () => {
    const wallet = fakeWallet();
    const connection = fakeConnection(world(), {
      // The RPC dies AFTER the wallet signed: we cannot tell whether the cluster got the transaction.
      sendRawTransaction: vi.fn().mockRejectedValueOnce(new Error('Request timed out')).mockResolvedValue('ignored'),
      getSignatureStatuses: vi
        .fn()
        .mockResolvedValueOnce({ value: [null] }) // retry's first poll: still not visible
        .mockResolvedValue({ value: [confirmedStatus] }),
    });
    const client = createNativeClient({ connection, wallet, sessionPubkey: wallet.publicKey!, sleep: noSleep, maxPolls: 4 });

    const result = await client.openPosition({ plan: 'Simple', gross: 1_000_000_000n, nonce: 42n });

    // Unknown is a result, not an exception: the caller keeps the handle.
    expect(result.status).toBe('unknown');
    expect(result.nonce).toBe(42n);
    expect(result.safeToRebuild).toBe(false);
    const signed = wallet.signTransaction.mock.calls[0][0] as Transaction;
    expect(result.signature).toBe(bs58.encode(signed.signature!));

    const after = await result.retry();

    expect(after.status).toBe('confirmed');
    expect(after.signature).toBe(result.signature);
    expect(result.nonce).toBe(42n);
    // The retry re-polled the SAME signature; it never rebuilt or re-signed.
    expect(wallet.signTransaction).toHaveBeenCalledTimes(1);
    expect(connection.getLatestBlockhash).toHaveBeenCalledTimes(1);
    expect(connection.getSignatureStatuses.mock.calls.every((call: unknown[]) => (call[0] as string[]).length === 1 && (call[0] as string[])[0] === result.signature)).toBe(true);
    // The only extra network write is a re-broadcast of the IDENTICAL signed bytes.
    expect(connection.sendRawTransaction).toHaveBeenCalledTimes(2);
    const [first, second] = connection.sendRawTransaction.mock.calls.map((call: unknown[]) => Buffer.from(call[0] as Uint8Array));
    expect(second.equals(first)).toBe(true);
    // The Pool was read once for the one instruction ever built.
    expect(connection.getAccountInfo.mock.calls.length).toBe(1);
  });

  it('a retry that finds the signature already landed does not re-broadcast', async () => {
    const wallet = fakeWallet();
    const connection = fakeConnection(world(), {
      sendRawTransaction: vi.fn().mockRejectedValue(new Error('socket hang up')),
      getSignatureStatuses: vi.fn().mockResolvedValue({ value: [finalizedStatus] }),
    });
    const client = createNativeClient({ connection, wallet, sessionPubkey: wallet.publicKey!, sleep: noSleep });
    const result = await client.openPosition({ plan: 'Compound15', gross: 10_000_000n, nonce: 1n });
    expect(result.status).toBe('unknown');
    const after = await result.retry();
    expect(after.status).toBe('finalized');
    expect(connection.sendRawTransaction).toHaveBeenCalledTimes(1);
  });

  it('a preflight rejection is a definite failure (exception, nothing to track), not unknown', async () => {
    const wallet = fakeWallet();
    const rejection = Object.assign(new Error('Transaction simulation failed: Error processing Instruction 1: custom program error: 0x1771'), {
      name: 'SendTransactionError',
      logs: ['Program log: AnchorError ... Error Number: 6001. Error Message: InvalidState'],
    });
    const connection = fakeConnection(world(), { sendRawTransaction: vi.fn().mockRejectedValue(rejection) });
    const client = createNativeClient({ connection, wallet, sessionPubkey: wallet.publicKey!, sleep: noSleep });
    const error = await client.openPosition({ plan: 'Simple', gross: 1_000_000_000n, nonce: 3n }).catch((e: unknown) => e);
    expect(error).toBeInstanceOf(NativeSendRejectedError);
    expect((error as NativeSendRejectedError).programError).toEqual({ code: 6001, name: 'InvalidState' });
    expect(connection.getSignatureStatuses).not.toHaveBeenCalled();
  });

  it('generates a fresh random u64 client nonce when none is given and returns it', async () => {
    const wallet = fakeWallet();
    const connection = fakeConnection(world());
    const client = createNativeClient({ connection, wallet, sessionPubkey: wallet.publicKey!, sleep: noSleep });
    const a = await client.openPosition({ plan: 'Simple', gross: 1_000_000_000n });
    const b = await client.openPosition({ plan: 'Simple', gross: 1_000_000_000n });
    expect(typeof a.nonce).toBe('bigint');
    expect(a.nonce).not.toBe(b.nonce);
    expect(a.nonce! >= 0n && a.nonce! < 1n << 64n).toBe(true);
  });

  it('refuses a wallet that returns a transaction without the fee-payer signature', async () => {
    const wallet = fakeWallet();
    wallet.signTransaction.mockImplementation(async (tx: Transaction) => tx); // "signs" nothing
    const connection = fakeConnection(world());
    const client = createNativeClient({ connection, wallet, sessionPubkey: wallet.publicKey!, sleep: noSleep });
    await expect(client.openPosition({ plan: 'Simple', gross: 1n, nonce: 1n })).rejects.toBeInstanceOf(WalletMismatchError);
    expect(connection.sendRawTransaction).not.toHaveBeenCalled();
  });
});


// ---------------------------------------------------------------------------
// R1: cluster identity is verified before the first read or action
// ---------------------------------------------------------------------------

function poolWorld(clusterDomain: Uint8Array) {
  const accounts = new Map<string, FakeAccount>();
  accounts.set(poolAddress().toBase58(), {
    owner: PROGRAM,
    data: buildAccount('Pool', { roundId: 1n, activeConfigVersion: 1, participantCount: 0n, clusterDomain }),
  });
  return accounts;
}
const DEVNET_CLUSTER: NativeClusterConfig = { expectedClusterDomain: 'devnet', programId: idl.address };
const LOCALNET_CLUSTER: NativeClusterConfig = { expectedClusterDomain: 'localnet', programId: idl.address };
const OPEN = { plan: 'Simple' as const, gross: 1_000_000_000n };

function untouched(connection: ReturnType<typeof fakeConnection>, wallet: ReturnType<typeof fakeWallet>) {
  expect(wallet.signTransaction).not.toHaveBeenCalled();
  expect(connection.sendRawTransaction).not.toHaveBeenCalled();
  expect(connection.simulateTransaction).not.toHaveBeenCalled();
  expect(connection.getLatestBlockhash).not.toHaveBeenCalled();
}

describe('cluster identity', () => {
  const clientFor = (connection: ReturnType<typeof fakeConnection>, cluster?: NativeClusterConfig) => {
    const wallet = fakeWallet();
    const client = createNativeClient({ connection, wallet, sessionPubkey: wallet.publicKey!, sleep: noSleep, cluster });
    return { wallet, client };
  };

  it('wrongClusterDomainRejectsBeforeAnySignTransaction', async () => {
    const connection = fakeConnection(poolWorld(CLUSTER_DOMAINS.localnet), { getGenesisHash: vi.fn(async () => PUBLIC_GENESIS_HASHES.devnet) });
    const { wallet, client } = clientFor(connection, DEVNET_CLUSTER);
    const error = await client.openPosition(OPEN).catch((e: unknown) => e);
    expect(error).toBeInstanceOf(NativeClusterMismatchError);
    expect((error as NativeClusterMismatchError).reason).toBe('cluster-domain');
    untouched(connection, wallet);
  });

  it('a wrong genesis hash rejects before any account is read or signed', async () => {
    const connection = fakeConnection(poolWorld(CLUSTER_DOMAINS.devnet), { getGenesisHash: vi.fn(async () => PUBLIC_GENESIS_HASHES['mainnet-beta']) });
    const { wallet, client } = clientFor(connection, DEVNET_CLUSTER);
    const error = await client.openPosition(OPEN).catch((e: unknown) => e);
    expect((error as NativeClusterMismatchError).reason).toBe('genesis-hash');
    untouched(connection, wallet);
    expect(connection.getAccountInfo).not.toHaveBeenCalled();
  });

  it('every state-changing method is refused on a wrong cluster, not just openPosition', async () => {
    const connection = fakeConnection(poolWorld(CLUSTER_DOMAINS.localnet), { getGenesisHash: vi.fn(async () => PUBLIC_GENESIS_HASHES.devnet) });
    const { wallet, client } = clientFor(connection, DEVNET_CLUSTER);
    const calls: Array<() => Promise<unknown>> = [
      () => client.fundSeriesA({ gross: 50_000_000n }),
      () => client.withdrawSimple({ nonce: 1n, minimumReceive: 0n }),
      () => client.claimBacker({}),
      () => client.advancePhase({}),
      () => client.buildCandidates({ closeId: 1n }),
      () => client.requestDraw({ closeId: 1n }),
      () => client.provisionPage({ page: { kind: 'Registry' }, index: 0n, funding: 'Caller' }),
      () => client.composeExpireFinalizeOpen({ closeId: 1n, plan: 'Simple', gross: 10_000_000n }),
    ];
    for (const call of calls) await expect(call()).rejects.toBeInstanceOf(NativeClusterMismatchError);
    untouched(connection, wallet);
  });

  it('reads are guarded too: a mismatched cluster returns an error, never data', async () => {
    const connection = fakeConnection(poolWorld(CLUSTER_DOMAINS.localnet), { getGenesisHash: vi.fn(async () => PUBLIC_GENESIS_HASHES.devnet) });
    const { client } = clientFor(connection, DEVNET_CLUSTER);
    await expect(client.readPool()).rejects.toBeInstanceOf(NativeClusterMismatchError);
    await expect(client.listPositions(Keypair.generate().publicKey.toBase58())).rejects.toBeInstanceOf(NativeClusterMismatchError);
    expect(connection.getProgramAccounts).not.toHaveBeenCalled();
  });

  it('a matching cluster proceeds, and is verified once per connection across clients and calls', async () => {
    const connection = fakeConnection(poolWorld(CLUSTER_DOMAINS.devnet), { getGenesisHash: vi.fn(async () => PUBLIC_GENESIS_HASHES.devnet) });
    const a = clientFor(connection, DEVNET_CLUSTER);
    const b = clientFor(connection, DEVNET_CLUSTER);
    await a.client.openPosition({ ...OPEN, nonce: 1n });
    await b.client.openPosition({ ...OPEN, nonce: 2n });
    await a.client.readPool();
    expect(a.wallet.signTransaction).toHaveBeenCalledTimes(1);
    expect(b.wallet.signTransaction).toHaveBeenCalledTimes(1);
    expect(connection.getGenesisHash).toHaveBeenCalledTimes(1);
  });

  it('two clusters never share verification: the same program on a verified connection does not vouch for another', async () => {
    const good = fakeConnection(poolWorld(CLUSTER_DOMAINS.devnet), { getGenesisHash: vi.fn(async () => PUBLIC_GENESIS_HASHES.devnet) });
    const bad = fakeConnection(poolWorld(CLUSTER_DOMAINS.localnet), { getGenesisHash: vi.fn(async () => PUBLIC_GENESIS_HASHES.devnet) });
    await expect(clientFor(good, DEVNET_CLUSTER).client.readPool()).resolves.toMatchObject({ roundId: 1n });
    await expect(clientFor(bad, DEVNET_CLUSTER).client.readPool()).rejects.toBeInstanceOf(NativeClusterMismatchError);
  });

  it('an uninitialized program reads as null and refuses actions with a state error (not a cluster error)', async () => {
    const connection = fakeConnection(new Map(), { getGenesisHash: vi.fn(async () => PUBLIC_GENESIS_HASHES.devnet) });
    const { wallet, client } = clientFor(connection, DEVNET_CLUSTER);
    await expect(client.readPool()).resolves.toBeNull();
    await expect(client.openPosition(OPEN)).rejects.toBeInstanceOf(NativeStateError);
    expect(wallet.signTransaction).not.toHaveBeenCalled();
  });

  it('no explicit cluster: a real-looking connection is identified from its endpoint (loopback = localnet)', async () => {
    const wrong = fakeConnection(poolWorld(CLUSTER_DOMAINS.devnet), { rpcEndpoint: 'http://127.0.0.1:8899' });
    const w1 = clientFor(wrong);
    await expect(w1.client.openPosition(OPEN)).rejects.toBeInstanceOf(NativeClusterMismatchError);
    untouched(wrong, w1.wallet);

    const right = fakeConnection(poolWorld(CLUSTER_DOMAINS.localnet), { rpcEndpoint: 'http://127.0.0.1:8899' });
    const w2 = clientFor(right);
    await w2.client.openPosition(OPEN);
    expect(w2.wallet.signTransaction).toHaveBeenCalledTimes(1);
  });

  it('no explicit cluster and an endpoint that names none fails closed on first use (construction never throws)', async () => {
    const connection = fakeConnection(poolWorld(CLUSTER_DOMAINS.localnet), { rpcEndpoint: 'https://rpc.example.com' });
    const { wallet, client } = clientFor(connection);
    await expect(client.openPosition(OPEN)).rejects.toBeInstanceOf(NativeClusterNotConfiguredError);
    await expect(client.readPool()).rejects.toBeInstanceOf(NativeClusterNotConfiguredError);
    untouched(connection, wallet);
  });

  it('mainnet-beta fails closed until its domain is chosen', async () => {
    const connection = fakeConnection(poolWorld(CLUSTER_DOMAINS.devnet), { getGenesisHash: vi.fn(async () => PUBLIC_GENESIS_HASHES['mainnet-beta']) });
    const { wallet, client } = clientFor(connection, { expectedClusterDomain: 'mainnet-beta', programId: idl.address });
    await expect(client.openPosition(OPEN)).rejects.toBeInstanceOf(NativeClusterNotConfiguredError);
    untouched(connection, wallet);
    expect(connection.getGenesisHash).not.toHaveBeenCalled();
  });

  it('a bare test double (no explicit cluster, no endpoint) is unverified: the genesis hash is never requested', async () => {
    const connection = fakeConnection(poolWorld(CLUSTER_DOMAINS.localnet));
    const { client } = clientFor(connection);
    await client.openPosition(OPEN);
    expect(connection.getGenesisHash).not.toHaveBeenCalled();
  });

  it('localnet identity is accepted on a private genesis hash and refused on a public cluster', async () => {
    const ok = fakeConnection(poolWorld(CLUSTER_DOMAINS.localnet));
    await expect(clientFor(ok, LOCALNET_CLUSTER).client.readPool()).resolves.toMatchObject({ roundId: 1n });
    const publicCluster = fakeConnection(poolWorld(CLUSTER_DOMAINS.localnet), { getGenesisHash: vi.fn(async () => PUBLIC_GENESIS_HASHES.devnet) });
    await expect(clientFor(publicCluster, LOCALNET_CLUSTER).client.readPool()).rejects.toBeInstanceOf(NativeClusterMismatchError);
  });
});

// ---------------------------------------------------------------------------
// R2: simulate the UNSIGNED transaction before the wallet prompt
// ---------------------------------------------------------------------------

describe('pre-sign simulation', () => {
  const setup = (extra: Parameters<typeof fakeConnection>[1] = {}) => {
    const connection = fakeConnection(world(), extra);
    const wallet = fakeWallet();
    const client = createNativeClient({ connection, wallet, sessionPubkey: wallet.publicKey!, sleep: noSleep });
    return { connection, wallet, client };
  };

  it('simulationFailureNeverCallsSignTransaction', async () => {
    const { connection, wallet, client } = setup({ simulateTransaction: vi.fn(async () => simCustom(6017)) });
    const error = await client.openPosition({ ...OPEN, nonce: 9n }).catch((e: unknown) => e);
    expect(error).toBeInstanceOf(NativeSimulationRejectedError);
    expect(error).toBeInstanceOf(NativeSendRejectedError);
    expect((error as NativeSimulationRejectedError).programError).toEqual({ code: 6017, name: 'Replay' });
    expect((error as NativeSimulationRejectedError).kind).toBe('program');
    expect((error as NativeSimulationRejectedError).phase).toBe('simulation');
    expect((error as Error).name).toBe('NativeSendRejectedError'); // name-based error copy (panelsQuote.describeNativeError) keeps working
    expect((error as Error).message).toMatch(/Replay/);
    expect((error as Error).message).toMatch(/nothing was signed/i);
    expect(wallet.signTransaction).toHaveBeenCalledTimes(0);
    expect(connection.sendRawTransaction).toHaveBeenCalledTimes(0);
    expect(connection.simulateTransaction).toHaveBeenCalledTimes(1);
  });

  it('simulates the unsigned transaction it is about to sign: sigVerify off, recent blockhash replaced, same message bytes', async () => {
    const { connection, wallet, client } = setup();
    await client.openPosition({ ...OPEN, nonce: 1n });
    const [simulated, config] = connection.simulateTransaction.mock.calls[0] as [VersionedTransaction, Record<string, unknown>];
    expect(simulated).toBeInstanceOf(VersionedTransaction);
    expect(simulated.signatures.every((sig) => sig.every((byte) => byte === 0))).toBe(true); // unsigned
    expect(config).toMatchObject({ sigVerify: false, replaceRecentBlockhash: true });
    const signed = wallet.signTransaction.mock.calls[0][0] as Transaction;
    expect(Buffer.from(simulated.message.serialize()).equals(signed.serializeMessage())).toBe(true);
  });

  it('simulates before the wallet prompt, and the prompt happens once on success', async () => {
    const { connection, wallet, client } = setup();
    await client.openPosition({ ...OPEN, nonce: 1n });
    expect(connection.simulateTransaction.mock.invocationCallOrder[0]).toBeLessThan(wallet.signTransaction.mock.invocationCallOrder[0]);
    expect(wallet.signTransaction.mock.invocationCallOrder[0]).toBeLessThan(connection.sendRawTransaction.mock.invocationCallOrder[0]);
    expect(connection.simulateTransaction).toHaveBeenCalledTimes(1);
    expect(wallet.signTransaction).toHaveBeenCalledTimes(1);
  });

  it('classifies insufficient funds and missing accounts (decoded, nothing signed)', async () => {
    const funds = setup({ simulateTransaction: vi.fn(async () => simFailure('InsufficientFundsForFee')) });
    const e1 = await funds.client.openPosition(OPEN).catch((e: unknown) => e);
    expect((e1 as NativeSimulationRejectedError).kind).toBe('insufficient-funds');
    expect((e1 as Error).message).toMatch(/insufficient funds/i);
    expect(funds.wallet.signTransaction).not.toHaveBeenCalled();

    const missing = setup({ simulateTransaction: vi.fn(async () => simFailure('AccountNotFound')) });
    const e2 = await missing.client.openPosition(OPEN).catch((e: unknown) => e);
    expect((e2 as NativeSimulationRejectedError).kind).toBe('missing-account');
    expect(missing.wallet.signTransaction).not.toHaveBeenCalled();
  });

  it('an unavailable simulation RPC is a retryable pre-sign failure, never a skipped check', async () => {
    const { connection, wallet, client } = setup({ simulateTransaction: vi.fn(async () => { throw new Error('socket hang up'); }) });
    const error = await client.openPosition(OPEN).catch((e: unknown) => e);
    expect(error).toBeInstanceOf(NativeSimulationUnavailableError);
    expect((error as Error).message).toMatch(/nothing was signed/i);
    expect(wallet.signTransaction).not.toHaveBeenCalled();
    expect(connection.sendRawTransaction).not.toHaveBeenCalled();
  });

  it('a transaction that does not fit the packet is a size failure decided locally, before any RPC call', async () => {
    const payer = Keypair.generate();
    const tx = new Transaction({ feePayer: payer.publicKey, blockhash: '4vJ9JU1bJJE96FWSJKvHsmmFADCg4gpZQff4P3bkLKi', lastValidBlockHeight: 1 });
    for (let i = 0; i < 40; i += 1) tx.add(SystemProgram.transfer({ fromPubkey: payer.publicKey, toPubkey: Keypair.generate().publicKey, lamports: 1 }));
    const simulate = vi.fn(async () => SIM_OK);
    const outcome = await simulateUnsigned({ simulateTransaction: simulate } as never, tx, { commitment: 'confirmed', timeoutMs: 1_000 });
    expect(outcome).toMatchObject({ ok: false, kind: 'size' });
    expect(simulate).not.toHaveBeenCalled();
  });

  it('post-broadcast reconciliation is unchanged: a send that dies after a passing simulation keeps signature and nonce', async () => {
    const { connection, wallet, client } = setup({
      sendRawTransaction: vi.fn().mockRejectedValueOnce(new Error('Request timed out')).mockResolvedValue('ignored'),
      getSignatureStatuses: vi.fn().mockResolvedValueOnce({ value: [null] }).mockResolvedValue({ value: [confirmedStatus] }),
    });
    const result = await client.openPosition({ ...OPEN, nonce: 42n });
    expect(result.status).toBe('unknown');
    expect(result.nonce).toBe(42n);
    const after = await result.retry();
    expect(after.status).toBe('confirmed');
    expect(wallet.signTransaction).toHaveBeenCalledTimes(1);
    expect(connection.simulateTransaction).toHaveBeenCalledTimes(1); // retry never simulates, rebuilds or re-signs
  });
});

// ---------------------------------------------------------------------------
// R4: the signed transaction is exactly the one we asked to sign
// ---------------------------------------------------------------------------

describe('signed transaction binding', () => {
  const setup = (sign: (wallet: ReturnType<typeof fakeWallet>) => (tx: Transaction) => Promise<Transaction>) => {
    const connection = fakeConnection(world());
    const wallet = fakeWallet();
    wallet.signTransaction.mockImplementation(sign(wallet));
    const client = createNativeClient({ connection, wallet, sessionPubkey: wallet.publicKey!, sleep: noSleep });
    return { connection, wallet, client };
  };

  it('rejectsWalletThatChangesTheFeePayer', async () => {
    const attacker = Keypair.generate();
    const { connection, wallet, client } = setup(() => async (tx) => {
      const swapped = new Transaction({ feePayer: attacker.publicKey, blockhash: tx.recentBlockhash!, lastValidBlockHeight: 1 });
      swapped.add(...tx.instructions);
      swapped.partialSign(attacker);
      return swapped;
    });
    const error = await client.openPosition(OPEN).catch((e: unknown) => e);
    expect(error).toBeInstanceOf(SignedTransactionMismatchError);
    expect(error).toBeInstanceOf(WalletMismatchError);
    expect((error as SignedTransactionMismatchError).reason).toBe('fee-payer');
    expect(wallet.signTransaction).toHaveBeenCalledTimes(1);
    expect(connection.sendRawTransaction).not.toHaveBeenCalled();
  });

  it('rejectsWalletThatSwapsTheMessageButKeepsTheFeePayer', async () => {
    const { connection, client } = setup((wallet) => async (tx) => {
      const swapped = new Transaction({ feePayer: wallet.keypair.publicKey, blockhash: tx.recentBlockhash!, lastValidBlockHeight: 1 });
      swapped.add(SystemProgram.transfer({ fromPubkey: wallet.keypair.publicKey, toPubkey: Keypair.generate().publicKey, lamports: 4_000_000_000 }));
      swapped.partialSign(wallet.keypair);
      return swapped;
    });
    const error = await client.openPosition(OPEN).catch((e: unknown) => e);
    expect((error as SignedTransactionMismatchError).reason).toBe('message-mutated');
    expect(connection.sendRawTransaction).not.toHaveBeenCalled();
  });

  it('rejects a wallet that mutates the transaction in place before signing it', async () => {
    const { connection, client } = setup((wallet) => async (tx) => {
      tx.add(SystemProgram.transfer({ fromPubkey: wallet.keypair.publicKey, toPubkey: Keypair.generate().publicKey, lamports: 1 }));
      tx.partialSign(wallet.keypair);
      return tx;
    });
    const error = await client.openPosition(OPEN).catch((e: unknown) => e);
    expect(error).toBeInstanceOf(SignedTransactionMismatchError);
    expect((error as SignedTransactionMismatchError).reason).toBe('message-mutated');
    expect(connection.sendRawTransaction).not.toHaveBeenCalled();
  });

  it('rejects a wrong or corrupted signature even on an intact message', async () => {
    const { connection, client } = setup((wallet) => async (tx) => {
      tx.partialSign(wallet.keypair);
      const bytes = Uint8Array.from(tx.signature!);
      bytes[3] ^= 0x55;
      tx.signatures = [{ publicKey: wallet.keypair.publicKey, signature: Buffer.from(bytes) }];
      return tx;
    });
    await expect(client.openPosition(OPEN)).rejects.toBeInstanceOf(WalletMismatchError);
    expect(connection.sendRawTransaction).not.toHaveBeenCalled();
  });

  it('an honest wallet that returns a re-deserialized copy is accepted and broadcast byte for byte', async () => {
    const { connection, wallet, client } = setup((w) => async (tx) => {
      tx.partialSign(w.keypair);
      return Transaction.from(tx.serialize());
    });
    const result = await client.openPosition({ ...OPEN, nonce: 5n });
    expect(connection.sendRawTransaction).toHaveBeenCalledTimes(1);
    const signed = await wallet.signTransaction.mock.results[0].value;
    expect(Buffer.from(connection.sendRawTransaction.mock.calls[0][0] as Uint8Array).equals(signed.serialize())).toBe(true);
    expect(result.signature).toBe(bs58.encode(signed.signature!));
  });
});


// ---------------------------------------------------------------------------
// A result and a client are bound to the cluster they were created on
// ---------------------------------------------------------------------------

describe('cluster switch and retry', () => {
  const ID_A: NativeClusterIdentity = { cluster: LOCALNET_CLUSTER, rpcEndpoint: 'http://127.0.0.1:8899' };
  const ID_B: NativeClusterIdentity = { cluster: DEVNET_CLUSTER, rpcEndpoint: 'https://api.devnet.solana.com' };

  /** Cluster A (a local validator) and cluster B (devnet), each a distinct fake connection. */
  function twoClusters(extraA: Parameters<typeof fakeConnection>[1] = {}) {
    const a = fakeConnection(poolWorld(CLUSTER_DOMAINS.localnet), { rpcEndpoint: ID_A.rpcEndpoint, ...extraA });
    const b = fakeConnection(poolWorld(CLUSTER_DOMAINS.devnet), { rpcEndpoint: ID_B.rpcEndpoint, getGenesisHash: vi.fn(async () => PUBLIC_GENESIS_HASHES.devnet) });
    return { a, b };
  }
  const clientOn = (connection: ReturnType<typeof fakeConnection>, identity: NativeClusterIdentity, wallet = fakeWallet()) =>
    ({ wallet, client: createNativeClient({ connection, wallet, sessionPubkey: wallet.publicKey!, sleep: noSleep, maxPolls: 4, cluster: identity.cluster }) });
  const allUntouched = (connection: ReturnType<typeof fakeConnection>) => {
    for (const name of ['sendRawTransaction', 'simulateTransaction', 'getLatestBlockhash', 'getSignatureStatuses', 'getAccountInfo', 'getGenesisHash'] as const) {
      expect(connection[name], name).not.toHaveBeenCalled();
    }
  };
  /** The send dies after signing on A: the outcome is unknown. */
  const unknownOnA = () =>
    twoClusters({
      sendRawTransaction: vi.fn().mockRejectedValueOnce(new Error('Request timed out')).mockResolvedValue('ignored'),
      getSignatureStatuses: vi.fn().mockResolvedValue({ value: [null] }),
    });

  it('retryAfterClusterSwitchFailsClosedBeforeAnyBroadcast', async () => {
    setNativeClusterIdentity(ID_A);
    const { a, b } = unknownOnA();
    const { client, wallet } = clientOn(a, ID_A);
    const result = await client.openPosition({ ...OPEN, nonce: 42n });
    expect(result.status).toBe('unknown');
    const originalSignature = result.signature;
    expect(a.sendRawTransaction).toHaveBeenCalledTimes(1);

    setNativeClusterIdentity(ID_B); // the app now points at cluster B

    const error = await result.retry().catch((e: unknown) => e);
    expect(error).toBeInstanceOf(NativeClusterSwitchedError);
    expect((error as NativeClusterSwitchedError).signature).toBe(originalSignature);
    expect((error as NativeClusterSwitchedError).nonce).toBe(42n);
    expect((error as NativeClusterSwitchedError).origin.clusterKey).toBe(clusterCacheKey(ID_A));

    // no packet went anywhere: A still saw exactly its one original broadcast, B saw nothing at all
    expect(a.sendRawTransaction).toHaveBeenCalledTimes(1);
    expect(a.getSignatureStatuses).not.toHaveBeenCalled(); // the refused retry made no network call at all
    expect(b.sendRawTransaction).not.toHaveBeenCalled();
    allUntouched(b);
    // and nothing was rebuilt or re-signed
    expect(wallet.signTransaction).toHaveBeenCalledTimes(1);
    expect(a.getLatestBlockhash).toHaveBeenCalledTimes(1);
    expect(a.simulateTransaction).toHaveBeenCalledTimes(1);

    // the result still carries the original signature, nonce and origin metadata
    expect(result.signature).toBe(originalSignature);
    expect(result.nonce).toBe(42n);
    expect(result.origin).toMatchObject({ clusterKey: clusterCacheKey(ID_A), rpcEndpoint: ID_A.rpcEndpoint, cluster: ID_A.cluster });
    expect(result.origin.generation).toBeLessThan(getNativeClusterGeneration());
    expect(result.status).toBe('unknown');
    setNativeClusterIdentity(null);
  });

  it('originStatusCanStillBeReadWithoutAnyRetransmission', async () => {
    setNativeClusterIdentity(ID_A);
    const { a, b } = twoClusters({
      sendRawTransaction: vi.fn().mockRejectedValueOnce(new Error('Request timed out')).mockResolvedValue('ignored'),
      getSignatureStatuses: vi.fn().mockResolvedValueOnce({ value: [null] }).mockResolvedValue({ value: [confirmedStatus] }),
    });
    const { client } = clientOn(a, ID_A);
    const result = await client.openPosition({ ...OPEN, nonce: 7n });
    setNativeClusterIdentity(ID_B);

    const reconciled = await result.reconcile();

    expect(reconciled).toMatchObject({ signature: result.signature, status: 'confirmed' });
    expect(a.getSignatureStatuses).toHaveBeenCalled();
    expect(a.getSignatureStatuses.mock.calls.every((call: unknown[]) => (call[0] as string[]).length === 1 && (call[0] as string[])[0] === result.signature)).toBe(true);
    expect(a.sendRawTransaction).toHaveBeenCalledTimes(1); // read-only: nothing retransmitted
    allUntouched(b);

    // the status stream keeps following the origin too
    const seen: string[] = [];
    for await (const update of result.status$) seen.push(update.status);
    expect(seen[seen.length - 1]).toBe('confirmed');
    expect(a.sendRawTransaction).toHaveBeenCalledTimes(1);
    allUntouched(b);
    setNativeClusterIdentity(null);
  });

  it('sameClusterRetryStillRebroadcastsTheIdenticalBytes', async () => {
    setNativeClusterIdentity(ID_A);
    const { a } = twoClusters({
      sendRawTransaction: vi.fn().mockRejectedValueOnce(new Error('Request timed out')).mockResolvedValue('ignored'),
      getSignatureStatuses: vi.fn().mockResolvedValueOnce({ value: [null] }).mockResolvedValue({ value: [confirmedStatus] }),
    });
    const { client, wallet } = clientOn(a, ID_A);
    const result = await client.openPosition({ ...OPEN, nonce: 8n });

    const after = await result.retry();

    expect(after).toMatchObject({ signature: result.signature, status: 'confirmed' });
    expect(a.sendRawTransaction).toHaveBeenCalledTimes(2);
    const [first, second] = a.sendRawTransaction.mock.calls.map((call: unknown[]) => Buffer.from(call[0] as Uint8Array));
    expect(second.equals(first)).toBe(true);
    expect(a.getSignatureStatuses.mock.calls.every((call: unknown[]) => (call[0] as string[])[0] === result.signature)).toBe(true);
    expect(wallet.signTransaction).toHaveBeenCalledTimes(1);
    expect(result.nonce).toBe(8n);
    setNativeClusterIdentity(null);
  });

  it('a switch between the retry’s poll and its rebroadcast still stops the packet', async () => {
    setNativeClusterIdentity(ID_A);
    const { a, b } = twoClusters({
      sendRawTransaction: vi.fn().mockRejectedValueOnce(new Error('Request timed out')).mockResolvedValue('ignored'),
      getSignatureStatuses: vi.fn(async () => {
        setNativeClusterIdentity(ID_B); // the user switches while the poll is in flight
        return { value: [null] };
      }),
    });
    const { client } = clientOn(a, ID_A);
    const result = await client.openPosition({ ...OPEN, nonce: 9n });
    await expect(result.retry()).rejects.toBeInstanceOf(NativeClusterSwitchedError);
    expect(a.sendRawTransaction).toHaveBeenCalledTimes(1);
    allUntouched(b);
    setNativeClusterIdentity(null);
  });

  it('a client created before a switch cannot start a new action afterwards (nothing read, simulated or signed)', async () => {
    setNativeClusterIdentity(ID_A);
    const { a } = twoClusters();
    const { client, wallet } = clientOn(a, ID_A);
    expect(client.origin).toMatchObject({ clusterKey: clusterCacheKey(ID_A), rpcEndpoint: ID_A.rpcEndpoint });
    setNativeClusterIdentity(ID_B);
    await expect(client.openPosition(OPEN)).rejects.toBeInstanceOf(NativeClusterSwitchedError);
    await expect(client.finalizeRound({ closeId: 1n })).rejects.toBeInstanceOf(NativeClusterSwitchedError);
    expect(wallet.signTransaction).not.toHaveBeenCalled();
    allUntouched(a);

    // a client created after the switch is bound to the new generation and works
    const { b } = twoClusters();
    const fresh = clientOn(b, ID_B);
    await fresh.client.openPosition({ ...OPEN, nonce: 1n });
    expect(fresh.wallet.signTransaction).toHaveBeenCalledTimes(1);
    setNativeClusterIdentity(null);
  });

  it('a switch during the wallet prompt stops the signed transaction from being broadcast', async () => {
    setNativeClusterIdentity(ID_A);
    const { a, b } = twoClusters();
    const wallet = fakeWallet();
    wallet.signTransaction.mockImplementation(async (tx: Transaction) => {
      tx.partialSign(wallet.keypair);
      setNativeClusterIdentity(ID_B); // the user switched cluster while the wallet popup was open
      return tx;
    });
    const { client } = clientOn(a, ID_A, wallet);
    const error = await client.openPosition(OPEN).catch((e: unknown) => e);
    expect(error).toBeInstanceOf(NativeClusterSwitchedError);
    expect((error as NativeClusterSwitchedError).signature).toBeNull(); // signed but never broadcast: nothing to track
    expect(a.sendRawTransaction).not.toHaveBeenCalled();
    allUntouched(b);
    setNativeClusterIdentity(null);
  });

  it('origin metadata: pinned clients carry their cluster key and endpoint; a bare test double carries none', async () => {
    const pinned = twoClusters().a;
    const withCluster = clientOn(pinned, ID_A);
    const result = await withCluster.client.openPosition({ ...OPEN, nonce: 3n });
    expect(result.origin.clusterKey).toBe(clusterCacheKey(ID_A));
    expect(result.origin.generation).toBe(withCluster.client.origin.generation);

    const bare = fakeConnection(world());
    const wallet = fakeWallet();
    const unpinned = createNativeClient({ connection: bare, wallet, sessionPubkey: wallet.publicKey!, sleep: noSleep });
    expect(unpinned.origin).toMatchObject({ clusterKey: null, cluster: null, rpcEndpoint: null });
    setNativeClusterIdentity(null);
  });
});
