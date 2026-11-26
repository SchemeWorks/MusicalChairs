// IMPL-46c: the allocation-run and round-close pipeline the "resume processing" button drives.
// The decision (which ONE step is next) is a pure function of Pool / Ticket / CloseRecord / clock.
import { describe, it, expect, vi } from 'vitest';
import {
  CANDIDATE_BATCH_ROWS,
  closingPayoutPlan,
  nextProcessingStep,
  processingOutcome,
  requestLooksFulfilled,
  runPayout,
  runProcessingStep,
  waitUntilLanded,
  type PayoutRunner,
} from './panelsClosing';
import { DRAW_STATE, TICKET_PHASE, type NativeCloseRecord, type NativeTxResult, type StatusUpdate } from './types';
import { OPENED_AT, testClose, testConfig, testPool } from './panelsTestFixtures';

const DEADLINE = OPENED_AT + 600n;
const EARLY = OPENED_AT + 30n;
const LATE = DEADLINE + 1n;
const closingPool = (overrides: Record<string, unknown> = {}) => testPool({ currentCloseId: 1n, nextCloseId: 2n, ...overrides });
const step = (close: NativeCloseRecord | null, overrides: { now?: bigint; ready?: boolean | null; pool?: ReturnType<typeof testPool> } = {}) =>
  nextProcessingStep({
    pool: overrides.pool ?? closingPool(),
    close,
    ticketPhase: TICKET_PHASE.Idle,
    now: overrides.now ?? EARLY,
    randomnessReady: overrides.ready ?? null,
  });

describe('allocation run', () => {
  it('names the phase and step, and is the first thing to push', () => {
    const s = nextProcessingStep({ pool: testPool({ activeTicket: 4n }), close: null, ticketPhase: TICKET_PHASE.Allocate, now: EARLY, randomnessReady: null });
    expect(s.kind).toBe('advance-allocation');
    expect(s.runnable).toBe(true);
    expect(s.detail).toMatch(/step 4 of 6 \(Allocate\)/);
    expect(s.label).toBe('Resume processing');
  });
  it('still runs the allocation run first when the round is also closing (cash barrier)', () => {
    const s = nextProcessingStep({
      pool: closingPool({ activeTicket: 2n }),
      close: testClose({ cashCommitted: false, closingTicketId: 2n }),
      ticketPhase: TICKET_PHASE.Senior,
      now: EARLY,
      randomnessReady: null,
    });
    expect(s.kind).toBe('advance-allocation');
  });
  it('is idle (nothing to show) when neither a run nor a close is pending', () => {
    expect(nextProcessingStep({ pool: testPool(), close: null, ticketPhase: TICKET_PHASE.Idle, now: EARLY, randomnessReady: null }).kind).toBe('done');
    expect(nextProcessingStep({ pool: null, close: null, ticketPhase: null, now: EARLY, randomnessReady: null }).kind).toBe('done');
  });
});

describe('round close: the next step from the CloseRecord', () => {
  it('provisions the candidate pages first (preferred, then fallback), then builds', () => {
    let s = step(testClose());
    expect(s).toMatchObject({ kind: 'provision-page', page: { setTag: 0, index: 0n }, runnable: true });
    s = step(testClose({ provisionedPreferredPages: 1n }));
    expect(s).toMatchObject({ kind: 'provision-page', page: { setTag: 1, index: 0n } });
    s = step(testClose({ provisionedPreferredPages: 1n, provisionedFallbackPages: 1n }));
    expect(s.kind).toBe('build-candidates');
    expect(s.detail).toMatch(/0 of 3 participants visited/);
  });
  it('provisions more than one page when the participants need it', () => {
    const s = step(testClose({ requiredPagesPerSet: 2n, provisionedPreferredPages: 1n, provisionedFallbackPages: 2n }));
    expect(s).toMatchObject({ kind: 'provision-page', page: { setTag: 0, index: 1n } });
  });
  it('requests the randomized selection once the candidates are sealed', () => {
    const sealed = { provisionedPreferredPages: 1n, provisionedFallbackPages: 1n, sealed: true, candidateCursor: 3n, fallbackCount: 3n };
    expect(step(testClose(sealed)).kind).toBe('request-selection');
  });
  it('waits for randomness while the provider has not answered, then captures it', () => {
    const requested = testClose({ sealed: true, drawState: BigInt(DRAW_STATE.Requested), fallbackCount: 3n });
    const waiting = step(requested, { ready: false });
    expect(waiting).toMatchObject({ kind: 'await-randomness', runnable: false });
    expect(waiting.detail).toMatch(/awaiting randomness/i);
    expect(step(requested, { ready: true }).kind).toBe('capture-randomness');
    expect(step(requested, { ready: null }).kind).toBe('capture-randomness'); // unknown: let the program decide
  });
  it('applies a captured result, even after the deadline', () => {
    const captured = testClose({ sealed: true, drawState: BigInt(DRAW_STATE.Captured), fallbackCount: 3n });
    expect(step(captured).kind).toBe('apply-promotion');
    expect(step(captured, { now: LATE }).kind).toBe('apply-promotion');
  });
  it('expires the selection once the 600 s window passes without a capture', () => {
    expect(step(testClose({ sealed: true, drawState: BigInt(DRAW_STATE.Requested) }), { now: LATE }).kind).toBe('expire-promotion');
    expect(step(testClose(), { now: LATE }).kind).toBe('expire-promotion');
    expect(step(testClose(), { now: DEADLINE }).kind).not.toBe('expire-promotion'); // strictly after
  });
  it('finalizes once the promotion is terminal and the reserve is settled', () => {
    const applied = testClose({ promotion: { kind: 'Skipped', reason: 1n, stage: 1n }, reserveSettled: true });
    expect(step(applied).kind).toBe('finalize-round');
    const unsettled = testClose({ promotion: { kind: 'Skipped', reason: 1n, stage: 1n }, reserveSettled: false });
    expect(step(unsettled)).toMatchObject({ kind: 'blocked', runnable: false });
  });
  it('is done once finalized, and waits while the closing cash is not committed', () => {
    expect(step(testClose({ finalized: true })).kind).toBe('done');
    expect(step(testClose({ cashCommitted: false, closingTicketId: 3n }))).toMatchObject({ kind: 'blocked', runnable: false });
    expect(step(null)).toMatchObject({ kind: 'blocked' });
  });
});

function fakeTx(signature = 'sig', statuses: StatusUpdate['status'][] = ['submitted', 'confirmed']): NativeTxResult {
  return {
    signature,
    nonce: null,
    status: statuses[0],
    status$: (async function* () {
      for (const status of statuses) yield { signature, status };
    })(),
    retry: async () => ({ signature, status: 'confirmed' }),
    safeToRebuild: false,
  } as unknown as NativeTxResult;
}

describe('runProcessingStep sends exactly the named step', () => {
  const client = () => ({
    advancePhase: vi.fn(async () => fakeTx('advance')),
    provisionPage: vi.fn(async () => fakeTx('page')),
    buildCandidates: vi.fn(async () => fakeTx('build')),
    requestDraw: vi.fn(async () => fakeTx('request')),
    captureDraw: vi.fn(async () => fakeTx('capture')),
    applyPromotion: vi.fn(async () => fakeTx('apply')),
    expirePromotion: vi.fn(async () => fakeTx('expire')),
    finalizeRound: vi.fn(async () => fakeTx('finalize')),
  });
  const base = { runnable: true, label: '', detail: '' };

  it('maps each step to its client method with the close id', async () => {
    const c = client();
    await runProcessingStep(c, { ...base, kind: 'advance-allocation', closeId: null });
    expect(c.advancePhase).toHaveBeenCalledWith({});
    await runProcessingStep(c, { ...base, kind: 'provision-page', closeId: 1n, page: { setTag: 1, index: 0n } });
    expect(c.provisionPage).toHaveBeenCalledWith({ page: { kind: 'Candidate', closeId: 1n, setTag: 1 }, index: 0n, funding: 'LockedClose' });
    await runProcessingStep(c, { ...base, kind: 'build-candidates', closeId: 1n });
    expect(c.buildCandidates).toHaveBeenCalledWith({ closeId: 1n, rows: CANDIDATE_BATCH_ROWS });
    expect(CANDIDATE_BATCH_ROWS).toBe(8);
    await runProcessingStep(c, { ...base, kind: 'request-selection', closeId: 1n });
    await runProcessingStep(c, { ...base, kind: 'capture-randomness', closeId: 1n });
    await runProcessingStep(c, { ...base, kind: 'apply-promotion', closeId: 1n });
    await runProcessingStep(c, { ...base, kind: 'expire-promotion', closeId: 1n });
    await runProcessingStep(c, { ...base, kind: 'finalize-round', closeId: 1n });
    for (const fn of [c.requestDraw, c.captureDraw, c.applyPromotion, c.expirePromotion, c.finalizeRound]) {
      expect(fn).toHaveBeenCalledWith({ closeId: 1n });
    }
  });
  it('refuses to send a waiting or blocked step', async () => {
    await expect(runProcessingStep(client(), { ...base, kind: 'await-randomness', closeId: 1n })).rejects.toThrow(/Nothing to send/);
    await expect(runProcessingStep(client(), { ...base, kind: 'build-candidates', closeId: null })).rejects.toThrow(/close id/);
  });
});

describe('randomness readiness hint', () => {
  it('compares the provider request account size with the config', () => {
    const config = testConfig(); // pending 200, fulfilled 300
    expect(requestLooksFulfilled(200, config)).toBe(false);
    expect(requestLooksFulfilled(300, config)).toBe(true);
    expect(requestLooksFulfilled(null, config)).toBeNull();
    expect(requestLooksFulfilled(300, testConfig({ pendingRequestBytes: 300n, fulfilledRequestBytes: 300n }))).toBeNull(); // no signal
    expect(requestLooksFulfilled(300, null)).toBeNull();
  });
});

describe('a payout that closes the round', () => {
  it('flags owed > pot and prepares the close record for the next close id', () => {
    const plan = closingPayoutPlan({ owed: 101n, pot: 100n, accrualPerSecondNumerator: 0n, closeRecordExists: false, nextCloseId: 7n });
    expect(plan).toEqual({ closesRound: true, prepareCloseId: 7n });
  });
  it('does not flag owed == pot, and skips the record when it exists', () => {
    expect(closingPayoutPlan({ owed: 100n, pot: 100n, accrualPerSecondNumerator: 0n, closeRecordExists: false, nextCloseId: 7n }).closesRound).toBe(false);
    expect(closingPayoutPlan({ owed: 200n, pot: 100n, accrualPerSecondNumerator: 0n, closeRecordExists: true, nextCloseId: 7n })).toEqual({
      closesRound: true,
      prepareCloseId: null,
    });
  });
  it('prepares the record early when accrual could tip the payout over the pot within two minutes', () => {
    const principal = 1_000_000_000n; // accrues 11 x 1e9 / 8.64e6 = ~1273 lamports a second
    const near = closingPayoutPlan({ owed: 1_000_000n - 1_000n, pot: 1_000_000n, accrualPerSecondNumerator: principal * 11n, closeRecordExists: false, nextCloseId: 3n });
    expect(near.closesRound).toBe(false);
    expect(near.prepareCloseId).toBe(3n);
    const far = closingPayoutPlan({ owed: 500_000n, pot: 1_000_000n, accrualPerSecondNumerator: principal * 11n, closeRecordExists: false, nextCloseId: 3n });
    expect(far.prepareCloseId).toBeNull();
  });
});

describe('runPayout: provision the CloseRecord, then pay out', () => {
  const request = { nonce: 9n, minimumReceive: 5n, maxWorkContribution: 0n };
  const instant = async () => undefined;
  function runner(opts: { recordAfterProvision?: boolean; existing?: boolean } = {}) {
    let provisioned = opts.existing ?? false;
    const order: string[] = [];
    const client: PayoutRunner & { order: string[] } = {
      order,
      readCloseRecord: vi.fn(async () => (provisioned ? testClose({ closeId: 2n }) : null)),
      provisionCloseRecord: vi.fn(async () => {
        order.push('provision');
        provisioned = opts.recordAfterProvision ?? true;
        return fakeTx('prov');
      }),
      withdrawSimple: vi.fn(async () => {
        order.push('withdraw');
        return fakeTx('withdraw');
      }),
      settleCompounding: vi.fn(async () => {
        order.push('settle');
        return fakeTx('settle');
      }),
    };
    return client;
  }

  it('provisions with Caller funding, waits for it to land, then sends the payout', async () => {
    const c = runner();
    const stages: string[] = [];
    const out = await runPayout(c, { kind: 'withdraw', request, prepareCloseId: 2n, onStage: (s) => stages.push(s), sleep: instant });
    expect(c.provisionCloseRecord).toHaveBeenCalledWith({ closeId: 2n, funding: 'Caller' });
    expect(c.order).toEqual(['provision', 'withdraw']);
    expect(stages).toEqual(['provisioning-close-record', 'sending-payout']);
    expect(c.withdrawSimple).toHaveBeenCalledWith(request);
    expect(out.provision?.signature).toBe('prov');
    expect(out.payout?.signature).toBe('withdraw');
  });
  it('uses settle_compounding for a compounding position', async () => {
    const c = runner({ existing: true });
    const out = await runPayout(c, { kind: 'settle', request, prepareCloseId: null, sleep: instant });
    expect(c.order).toEqual(['settle']);
    expect(out.provision).toBeNull();
  });
  it('skips provisioning when the record already exists', async () => {
    const c = runner({ existing: true });
    await runPayout(c, { kind: 'withdraw', request, prepareCloseId: 2n, sleep: instant });
    expect(c.provisionCloseRecord).not.toHaveBeenCalled();
    expect(c.order).toEqual(['withdraw']);
  });
  it('does not send the payout when provisioning failed or expired', async () => {
    const c = runner();
    await expect(
      runPayout(c, { kind: 'withdraw', request, prepareCloseId: 2n, sleep: instant, wait: async () => ({ signature: 'prov', status: 'failed' }) }),
    ).rejects.toThrow(/Nothing was withdrawn/);
    expect(c.withdrawSimple).not.toHaveBeenCalled();
  });
  it('returns without a payout when provisioning outcome is unknown (the caller shows its signature)', async () => {
    const c = runner();
    const out = await runPayout(c, { kind: 'withdraw', request, prepareCloseId: 2n, sleep: instant, wait: async () => ({ signature: 'prov', status: 'unknown' }) });
    expect(out.payout).toBeNull();
    expect(out.provision?.signature).toBe('prov');
    expect(c.withdrawSimple).not.toHaveBeenCalled();
  });
  it('does not send the payout while the record is still not visible after landing', async () => {
    const c = runner({ recordAfterProvision: false });
    const out = await runPayout(c, { kind: 'withdraw', request, prepareCloseId: 2n, sleep: instant });
    expect(out.payout).toBeNull();
    expect(c.withdrawSimple).not.toHaveBeenCalled();
  });
});

describe('waitUntilLanded', () => {
  it('stops at confirmed, at failure, and reports the last status when the window ends', async () => {
    expect((await waitUntilLanded(fakeTx('a', ['submitted', 'confirmed', 'finalized']))).status).toBe('confirmed');
    expect((await waitUntilLanded(fakeTx('b', ['submitted', 'failed']))).status).toBe('failed');
    expect((await waitUntilLanded(fakeTx('c', ['submitted', 'unknown']))).status).toBe('unknown');
  });
});

describe('what to say when processing goes idle (browser finding F3)', () => {
  it('"Open a Position in the new round" belongs only after finalize_round', () => {
    expect(processingOutcome('finalize-round')).toEqual({ message: 'The round is finalized. The next round is open.', opensNewRound: true });
  });
  it('a plain allocation-run commit just says payouts are available again', () => {
    const o = processingOutcome('advance-allocation');
    expect(o.opensNewRound).toBe(false);
    expect(o.message).toMatch(/allocation run finished/i);
    expect(o.message).toMatch(/Payouts are available again/);
  });
  it('every other step never opens a round', () => {
    for (const kind of ['provision-page', 'build-candidates', 'request-selection', 'capture-randomness', 'apply-promotion', 'expire-promotion']) {
      expect(processingOutcome(kind).opensNewRound, kind).toBe(false);
    }
  });
});
