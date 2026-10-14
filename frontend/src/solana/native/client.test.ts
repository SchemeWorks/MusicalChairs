// IMPL-46b step 1, tests 4 and 5 (+ the send path around them): the NativeClient
// signs only for the session wallet, and an unknown outcome never rebuilds.
import { describe, it, expect, vi } from 'vitest';
import { Keypair, PublicKey, Transaction } from '@solana/web3.js';
import bs58 from 'bs58';
import { createNativeClient, NativeSendRejectedError } from './client';
import { WalletMismatchError } from './wallet';
import { poolAddress } from './accounts';
import { buildAccount, fakeConnection, fakeWallet, type FakeAccount } from './testFixtures';
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
