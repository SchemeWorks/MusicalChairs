// IMPL-46b step 1, test 6 (+ the unknown / expired edges): the status ladder is
// driven by injected getSignatureStatuses responses; no websocket anywhere.
import { describe, it, expect, vi } from 'vitest';
import { StatusTracker, describeTransactionError, type StatusRpc, type SignatureStatusValue } from './status';
import type { StatusUpdate } from './types';

const SIG = '5VERv8NMvzbJMEkV8xnrLkEaWRtSz9CosKDYjCJjBRnbJLgp8uirBgmQpjKhoR4tjF3ZpRzrFmBV6UjKdiSZkQUW';

const processed: SignatureStatusValue = { slot: 10, confirmations: 0, err: null, confirmationStatus: 'processed' };
const confirmed: SignatureStatusValue = { slot: 11, confirmations: 1, err: null, confirmationStatus: 'confirmed' };
const finalized: SignatureStatusValue = { slot: 12, confirmations: null, err: null, confirmationStatus: 'finalized' };

/** An RPC whose getSignatureStatuses answers from `script` in order, then repeats the last answer. */
function scriptedRpc(script: Array<SignatureStatusValue | null | Error>, blockHeight?: number) {
  let i = 0;
  const rpc = {
    getSignatureStatuses: vi.fn(async (_sigs: string[], _cfg?: { searchTransactionHistory?: boolean }) => {
      const step = script[Math.min(i, script.length - 1)];
      i += 1;
      if (step instanceof Error) throw step;
      return { value: [step] };
    }),
    ...(blockHeight === undefined ? {} : { getBlockHeight: vi.fn(async () => blockHeight) }),
  };
  return rpc as StatusRpc & { getSignatureStatuses: ReturnType<typeof vi.fn> } & { getBlockHeight?: ReturnType<typeof vi.fn> };
}

async function collect(stream: AsyncIterable<StatusUpdate>): Promise<StatusUpdate[]> {
  const seen: StatusUpdate[] = [];
  for await (const update of stream) seen.push(update);
  return seen;
}

const noSleep = async () => {};

describe('StatusTracker', () => {
  it('statusLadderSubmittedConfirmedFinalized', async () => {
    const rpc = scriptedRpc([null, processed, confirmed, finalized]);
    const tracker = new StatusTracker({ rpc, signature: SIG, initial: 'submitted', sleep: noSleep, maxPolls: 10 });
    const seen = await collect(tracker.stream());
    expect(seen.map((u) => u.status)).toEqual(['submitted', 'confirmed', 'finalized']);
    expect(seen.every((u) => u.signature === SIG)).toBe(true);
    expect(seen[1].slot).toBe(11);
    expect(seen[2].slot).toBe(12);
    // History search is on (a settled transaction may have left the recent cache), and one signature per call.
    expect(rpc.getSignatureStatuses).toHaveBeenCalledWith([SIG], { searchTransactionHistory: true });
    // It stopped at finalized: 4 polls, not the 10-poll window.
    expect(rpc.getSignatureStatuses).toHaveBeenCalledTimes(4);
    expect(tracker.latest.status).toBe('finalized');
  });

  it('never steps backwards (a lagging node reporting processed after confirmed keeps confirmed)', async () => {
    const rpc = scriptedRpc([confirmed, processed, finalized]);
    const tracker = new StatusTracker({ rpc, signature: SIG, initial: 'submitted', sleep: noSleep, maxPolls: 5 });
    expect((await collect(tracker.stream())).map((u) => u.status)).toEqual(['submitted', 'confirmed', 'finalized']);
  });

  it('reports a program error as failed (terminal) and names the IDL error', async () => {
    const err = { InstructionError: [1, { Custom: 6001 }] };
    const rpc = scriptedRpc([null, { slot: 9, confirmations: 0, err, confirmationStatus: 'confirmed' }]);
    const tracker = new StatusTracker({ rpc, signature: SIG, initial: 'submitted', sleep: noSleep, maxPolls: 5 });
    const seen = await collect(tracker.stream());
    expect(seen.map((u) => u.status)).toEqual(['submitted', 'failed']);
    expect(seen[1].error).toEqual(err);
    expect(describeTransactionError(err)).toEqual({ code: 6001, name: 'InvalidState' });
    expect(tracker.safeToRebuild).toBe(true);
  });

  it('a signature that never shows up in the window is unknown, not failed, and is not safe to rebuild', async () => {
    const rpc = scriptedRpc([null]);
    const tracker = new StatusTracker({ rpc, signature: SIG, initial: 'submitted', sleep: noSleep, maxPolls: 3 });
    const seen = await collect(tracker.stream());
    expect(seen.map((u) => u.status)).toEqual(['submitted', 'unknown']);
    expect(rpc.getSignatureStatuses).toHaveBeenCalledTimes(3);
    expect(tracker.safeToRebuild).toBe(false);
  });

  it('starts at unknown when the send itself timed out, then lands on confirmed when the signature appears', async () => {
    const rpc = scriptedRpc([null, confirmed]);
    const tracker = new StatusTracker({ rpc, signature: SIG, initial: 'unknown', initialError: new Error('timeout'), sleep: noSleep, maxPolls: 5 });
    const seen = await collect(tracker.stream());
    expect(seen.map((u) => u.status)).toEqual(['unknown', 'confirmed']);
    expect(seen[0].error).toBeInstanceOf(Error);
  });

  it('survives transient RPC errors and reports unknown (with the error) if every poll fails', async () => {
    const flaky = scriptedRpc([new Error('503'), finalized]);
    const t1 = new StatusTracker({ rpc: flaky, signature: SIG, initial: 'submitted', sleep: noSleep, maxPolls: 5 });
    expect((await collect(t1.stream())).map((u) => u.status)).toEqual(['submitted', 'finalized']);

    const dead = scriptedRpc([new Error('connection refused')]);
    const t2 = new StatusTracker({ rpc: dead, signature: SIG, initial: 'submitted', sleep: noSleep, maxPolls: 3 });
    const seen = await collect(t2.stream());
    expect(seen.map((u) => u.status)).toEqual(['submitted', 'unknown']);
    expect((seen[1].error as Error).message).toBe('connection refused');
  });

  it('expires only after the blockhash height has passed AND the signature is still unseen, reading the height first', async () => {
    const rpc = scriptedRpc([null], 101);
    const tracker = new StatusTracker({ rpc, signature: SIG, initial: 'submitted', sleep: noSleep, maxPolls: 3, lastValidBlockHeight: 100 });
    const seen = await collect(tracker.stream());
    expect(seen.map((u) => u.status)).toEqual(['submitted', 'expired']);
    expect(tracker.safeToRebuild).toBe(true);
    // Height is read BEFORE the status, so a transaction cannot slip in between the two reads.
    expect(rpc.getBlockHeight!.mock.invocationCallOrder[0]).toBeLessThan(rpc.getSignatureStatuses.mock.invocationCallOrder[0]);

    // Height not yet past -> still just unknown at the end of the window.
    const early = scriptedRpc([null], 100);
    const t2 = new StatusTracker({ rpc: early, signature: SIG, initial: 'submitted', sleep: noSleep, maxPolls: 2, lastValidBlockHeight: 100 });
    expect((await collect(t2.stream())).map((u) => u.status)).toEqual(['submitted', 'unknown']);

    // A signature seen on chain is never expired, even past the height.
    const landed = scriptedRpc([confirmed], 500);
    const t3 = new StatusTracker({ rpc: landed, signature: SIG, initial: 'submitted', sleep: noSleep, maxPolls: 2, lastValidBlockHeight: 100 });
    expect((await collect(t3.stream())).map((u) => u.status)).toEqual(['submitted', 'confirmed']);
  });

  it('sleeps between polls using the injected sleeper (no wall-clock timers, no wss)', async () => {
    const sleep = vi.fn(async () => {});
    const rpc = scriptedRpc([null, null, finalized]);
    const tracker = new StatusTracker({ rpc, signature: SIG, initial: 'submitted', sleep, pollIntervalMs: 1500, maxPolls: 5 });
    await collect(tracker.stream());
    expect(sleep).toHaveBeenCalledWith(1500);
    expect(sleep).toHaveBeenCalledTimes(2);
  });
});
