// IMPL-46c: the panels' display math mirrors the on-chain handlers. Every payout
// figure is pinned to the Rust-side golden vectors in solana/fixtures/economics.json.
import { describe, it, expect } from 'vitest';
import {
  DEFAULT_PAYOUT_SLIPPAGE_BPS,
  allocationRunView,
  defaultMinimumReceive,
  describeNativeError,
  planKindFromInvestId,
  planOfPosition,
  positionAction,
  programErrorCopy,
  projectOpen,
  quoteBacker,
  quotePosition,
  seriesAEntitlement,
} from './panelsQuote';
import { projectSimpleDailyLamports } from './economics';
import { OPENED_AT, ROI_15_Q18, testBacker, testConfig, testPool, testPosition } from './panelsTestFixtures';
import { readFixtureJson } from './testFixtures';

interface Vector {
  principal: number;
  elapsed_seconds: number;
  owed: number;
  toll_bps: number;
  toll: number;
  net: number;
}
const vectors = readFixtureJson<Record<string, Vector & { pot?: number; paid_gross?: number }>>('economics.json');
const BIG_POT = 1_000_000_000_000n;

describe('open_position preview (gross interest, Front-End Load on top)', () => {
  const config = testConfig();

  it('Simple: 1 SOL earns 0.11 SOL a day on the GROSS deposit; Front-End Load is 4 %', () => {
    const p = projectOpen({ plan: 'Simple', gross: 1_000_000_000n, config });
    expect(p.frontEndLoad).toBe(40_000_000n);
    expect(p.potCredit).toBe(960_000_000n);
    expect(p.dailyLamports).toBe(110_000_000n);
    expect(p.dailyLamports).toBe(projectSimpleDailyLamports(1_000_000_000n));
    expect(p.maturityEarnings).toBe(BigInt(vectors.simple_maturity.owed)); // 21 days, golden vector
    expect(p.maturityFeeBps).toBe(300);
  });

  it('Compound15: maturity earnings use the pinned Q18 factor; the Jackpot Fee is 9 %', () => {
    const p = projectOpen({ plan: 'Compound15', gross: 1_000_000_000n, config });
    expect(p.maturityEarnings).toBe(BigInt(vectors.compound_15.owed));
    expect(p.maturityFeeBps).toBe(900);
    expect(projectOpen({ plan: 'Compound30', gross: 1_000_000_000n, config }).maturityEarnings).toBe(BigInt(vectors.compound_30.owed));
  });

  it('without a loaded config only the load is previewed (default 4 %)', () => {
    const p = projectOpen({ plan: 'Simple', gross: 500_000_000n, config: null });
    expect(p.frontEndLoad).toBe(20_000_000n);
    expect(p.maturityEarnings).toBeNull();
    expect(() => projectOpen({ plan: 'Simple', gross: -1n, config })).toThrow(/negative/);
  });

  it('Series A entitlement is 124 % of the funded amount, floored', () => {
    expect(seriesAEntitlement(50_000_000n, config)).toBe(62_000_000n);
    expect(seriesAEntitlement(1n, config)).toBe(1n); // floor(1.24)
    expect(ROI_15_Q18).toBe(4_473_565_759_257_037_859n);
  });
});

describe('Simple payout quote matches the golden vectors', () => {
  const config = testConfig();
  for (const name of ['simple_one_day', 'simple_before_day_7', 'simple_day_7', 'simple_before_day_14', 'simple_day_14', 'simple_maturity']) {
    it(name, () => {
      const v = vectors[name];
      const position = testPosition({ principal: BigInt(v.principal) });
      const quote = quotePosition({ position, config, pot: BIG_POT, now: OPENED_AT + BigInt(v.elapsed_seconds) })!;
      expect(quote.owed).toBe(BigInt(v.owed));
      expect(quote.feeBps).toBe(v.toll_bps);
      expect(quote.fee).toBe(BigInt(v.toll));
      expect(quote.net).toBe(BigInt(v.net));
      expect(quote.insolvent).toBe(false);
    });
  }

  it('lists the next, lower Carried Interest tier and when it starts', () => {
    const q = quotePosition({ position: testPosition(), config, pot: BIG_POT, now: OPENED_AT + 86_400n })!;
    expect(q.nextTier).toEqual({ bps: 750, inSeconds: 6n * 86_400n });
    const late = quotePosition({ position: testPosition(), config, pot: BIG_POT, now: OPENED_AT + 15n * 86_400n })!;
    expect(late.feeBps).toBe(300);
    expect(late.nextTier).toBeNull();
  });

  it('carries the remainder and subtracts what was already paid', () => {
    // accrue 1 day, pay it all, accrue another day: the second owed is a day of interest again.
    const afterFirstPayout = testPosition({
      accruedGross: 110_000_000n,
      paidGross: 110_000_000n,
      lastAccruedAt: OPENED_AT + 86_400n,
    });
    const q = quotePosition({ position: afterFirstPayout, config, pot: BIG_POT, now: OPENED_AT + 2n * 86_400n })!;
    expect(q.owed).toBe(110_000_000n);
    expect(q.payable).toBe(true);
  });

  it('caps accrual at maturity and flags a matured position with nothing owed as closable', () => {
    const matured = testPosition({ accruedGross: 2_310_000_000n, paidGross: 2_310_000_000n, lastAccruedAt: OPENED_AT + 1_814_400n });
    const q = quotePosition({ position: matured, config, pot: BIG_POT, now: OPENED_AT + 40n * 86_400n })!;
    expect(q.owed).toBe(0n);
    expect(q.matured).toBe(true);
    expect(q.closable).toBe(true);
    expect(q.payable).toBe(false);
  });

  it('insolvent (owed > pot) pays only the pot and charges the fee on what was paid', () => {
    const v = vectors.insolvent;
    const position = testPosition({ accruedGross: BigInt(v.owed), lastAccruedAt: OPENED_AT });
    const q = quotePosition({ position, config, pot: BigInt(v.pot!), now: OPENED_AT })!;
    expect(q.owed).toBe(100_000_003n);
    expect(q.insolvent).toBe(true);
    expect(q.paidGross).toBe(BigInt(v.paid_gross!));
    expect(q.fee).toBe(BigInt(v.toll));
    expect(q.net).toBe(BigInt(v.net));
  });

  it('owed == pot is solvent (strict test)', () => {
    const position = testPosition({ accruedGross: 50_000_000n, lastAccruedAt: OPENED_AT });
    const q = quotePosition({ position, config, pot: 50_000_000n, now: OPENED_AT })!;
    expect(q.insolvent).toBe(false);
    expect(q.paidGross).toBe(50_000_000n);
  });
});

describe('Compounding payout quote', () => {
  const config = testConfig();
  it('compound_15 / compound_30 settle at maturity at the golden-vector amounts', () => {
    for (const [name, kind, days] of [['compound_15', 1n, 15n], ['compound_30', 2n, 30n]] as const) {
      const v = vectors[name];
      const position = testPosition({ kind, maturityAt: OPENED_AT + days * 86_400n });
      const q = quotePosition({ position, config, pot: BIG_POT, now: OPENED_AT + BigInt(v.elapsed_seconds) })!;
      expect(q.matured).toBe(true);
      expect(q.owed).toBe(BigInt(v.owed));
      expect(q.feeBps).toBe(v.toll_bps);
      expect(q.fee).toBe(BigInt(v.toll));
      expect(q.net).toBe(BigInt(v.net));
      expect(q.payable).toBe(true);
    }
  });
  it('is not payable before maturity but still previews the at-maturity amount', () => {
    const position = testPosition({ kind: 1n, maturityAt: OPENED_AT + 15n * 86_400n });
    const q = quotePosition({ position, config, pot: BIG_POT, now: OPENED_AT + 86_400n })!;
    expect(q.matured).toBe(false);
    expect(q.payable).toBe(false);
    expect(q.owed).toBe(BigInt(vectors.compound_15.owed));
    expect(q.secondsToMaturity).toBe(14n * 86_400n);
  });
  it('maps position kinds and invest-tab plan ids', () => {
    expect(planOfPosition(0)).toBe('Simple');
    expect(planOfPosition(1)).toBe('Compound15');
    expect(planOfPosition(2)).toBe('Compound30');
    expect(planOfPosition(9)).toBeNull();
    expect(planKindFromInvestId('21-day-simple')).toBe('Simple');
    expect(planKindFromInvestId('15-day-compounding')).toBe('Compound15');
    expect(planKindFromInvestId('30-day-compounding')).toBe('Compound30');
    expect(planKindFromInvestId('anything-else')).toBe('Simple');
  });
});

describe('minimum_receive default', () => {
  it('is the quoted net less 1 % (floored); zero stays zero', () => {
    expect(DEFAULT_PAYOUT_SLIPPAGE_BPS).toBe(100n);
    expect(defaultMinimumReceive(96_800_000n)).toBe(95_832_000n);
    expect(defaultMinimumReceive(101n)).toBe(99n); // floor(99.99)
    expect(defaultMinimumReceive(0n)).toBe(0n);
    expect(defaultMinimumReceive(1_000n, 0n)).toBe(1_000n);
    expect(() => defaultMinimumReceive(1_000n, 10_001n)).toThrow(/slippage/);
  });
  it('never exceeds the quote, so an ordinary landing passes the on-chain floor', () => {
    for (const net of [1n, 7n, 12_345n, 96_800_000n, 2_240_700_000n]) {
      expect(defaultMinimumReceive(net)).toBeLessThanOrEqual(net);
    }
  });
});

describe('position action (what the button does)', () => {
  const config = testConfig();
  const pool = testPool({ roundId: 1n, pot: BIG_POT, activeTicket: 0n, currentCloseId: null });
  const at = (position = testPosition(), now = OPENED_AT + 86_400n) => ({
    position,
    quote: quotePosition({ position, config, pot: pool.pot, now })!,
  });

  it('withdraw is enabled for a Simple position with earnings', () => {
    const { position, quote } = at();
    expect(positionAction({ position, quote, pool })).toEqual({ kind: 'withdraw', enabled: true, reason: null });
  });
  it('is disabled with the allocation-run reason while a run is active', () => {
    const { position, quote } = at();
    const a = positionAction({ position, quote, pool: testPool({ ...{ roundId: 1n, pot: BIG_POT }, activeTicket: 4n }) });
    expect(a.enabled).toBe(false);
    expect(a.reason).toMatch(/allocation run/i);
  });
  it('is disabled while the round is closing', () => {
    const { position, quote } = at();
    const a = positionAction({ position, quote, pool: testPool({ roundId: 1n, pot: BIG_POT, currentCloseId: 1n }) });
    expect(a.enabled).toBe(false);
    expect(a.reason).toMatch(/closing/i);
  });
  it('compound is locked until maturity, then settles', () => {
    const early = at(testPosition({ kind: 1n, maturityAt: OPENED_AT + 15n * 86_400n }));
    expect(positionAction({ ...early, pool }).kind).toBe('locked');
    const matured = at(testPosition({ kind: 1n, maturityAt: OPENED_AT + 15n * 86_400n }), OPENED_AT + 15n * 86_400n);
    expect(positionAction({ ...matured, pool })).toEqual({ kind: 'settle', enabled: true, reason: null });
  });
  it('matured Simple with owed == 0 offers Close', () => {
    const paid = testPosition({ accruedGross: 2_310_000_000n, paidGross: 2_310_000_000n, lastAccruedAt: OPENED_AT + 1_814_400n });
    const x = at(paid, OPENED_AT + 30n * 86_400n);
    expect(positionAction({ ...x, pool })).toEqual({ kind: 'close', enabled: true, reason: null });
  });
  it('a position from a closed round, or one already claimed, is read-only', () => {
    const old = at(testPosition({ roundId: 0n }));
    expect(positionAction({ ...old, pool }).kind).toBe('none');
    expect(positionAction({ ...old, pool }).enabled).toBe(false);
    const claimed = at(testPosition({ claimed: true }));
    expect(positionAction({ ...claimed, pool }).kind).toBe('none');
  });
  it('a brand-new Simple position has no earnings yet', () => {
    const fresh = at(testPosition(), OPENED_AT);
    const a = positionAction({ ...fresh, pool });
    expect(a.enabled).toBe(false);
    expect(a.reason).toBe('No earnings yet');
  });
});

describe('Backer quote', () => {
  it('claimable = funded + pending credit whose allocation run has finalized', () => {
    const backer = testBacker({ funded: 100n, pending: 20n, pendingTicket: 5n, lifetime: 120n, entitlement: 1_000n });
    const running = quoteBacker(backer, 4n);
    expect(running.claimable).toBe(100n);
    expect(running.inProgress).toBe(20n);
    const finalized = quoteBacker(backer, 5n);
    expect(finalized.claimable).toBe(120n);
    expect(finalized.inProgress).toBe(0n);
    expect(finalized.remaining).toBe(880n);
    expect(finalized.creditedPct).toBe(12);
  });
  it('a claim never lowers lifetime credit, and an empty row quotes zero', () => {
    const q = quoteBacker(testBacker({ entitlement: 0n }), 0n);
    expect(q.claimable).toBe(0n);
    expect(q.creditedPct).toBe(0);
  });
});

describe('allocation run view', () => {
  it('is active exactly when pool.activeTicket is nonzero; the phase decorates it', () => {
    expect(allocationRunView({ pool: testPool({ activeTicket: 0n }), ticketPhase: 6 }).active).toBe(false);
    const v = allocationRunView({ pool: testPool({ activeTicket: 3n }), ticketPhase: 3 });
    expect(v).toEqual({ active: true, phase: 'Allocate', step: 4, steps: 6 });
    expect(allocationRunView({ pool: testPool({ activeTicket: 3n }), ticketPhase: null }).phase).toBeNull();
  });
});

describe('error copy', () => {
  it('names the fix for the errors a user can act on', () => {
    expect(programErrorCopy('AllocationPending')).toMatch(/resume processing/i);
    expect(programErrorCopy('ReserveUnavailable')).toMatch(/owner contribution/i);
    expect(programErrorCopy('MinimumReceive')).toMatch(/minimum receive/i);
    expect(programErrorCopy('SomethingNew')).toContain('SomethingNew');
    expect(programErrorCopy(undefined)).toMatch(/rejected/);
  });
  it('turns the client errors into one sentence', () => {
    expect(describeNativeError(new Error('User rejected the request.'))).toBe('Wallet request was cancelled.');
    const rejected = Object.assign(new Error('Transaction rejected before broadcast: simulation failed'), {
      name: 'NativeSendRejectedError',
      programError: { code: 6010, name: 'AllocationPending' },
    });
    expect(describeNativeError(rejected)).toBe(programErrorCopy('AllocationPending'));
    const lamports = Object.assign(new Error('Transaction rejected before broadcast: Transfer: insufficient lamports 5, need 9'), {
      name: 'NativeSendRejectedError',
      programError: null,
    });
    expect(describeNativeError(lamports)).toMatch(/doesn't have enough SOL/);
    expect(describeNativeError(new Error('Connected wallet does not match your session — switch back.'))).toMatch(/does not match your session/);
    expect(describeNativeError(null)).toMatch(/went wrong/);
  });

  const named = (name: string, message: string, extra: Record<string, unknown> = {}) => Object.assign(new Error(message), { name }, extra);

  it('maps the adapter\'s pre-sign simulation rejections by kind and says nothing was signed', () => {
    const sim = (kind: string, program: { code: number; name: string } | null) =>
      named('NativeSendRejectedError', 'Transaction would fail: x. Nothing was signed.', { phase: 'simulation', kind, programError: program });
    expect(describeNativeError(sim('insufficient-funds', null))).toMatch(/doesn't have enough SOL.*Nothing was signed/);
    expect(describeNativeError(sim('missing-account', null))).toMatch(/missing or not set up.*Nothing was signed/);
    expect(describeNativeError(sim('program', { code: 6010, name: 'AllocationPending' }))).toMatch(/resume processing.*Nothing was signed/i);
    for (const kind of ['page', 'budget', 'size', 'batch']) {
      expect(describeNativeError(sim(kind, null)), kind).toMatch(/too large for one transaction.*smaller batch/);
    }
    // a node preflight rejection happens after signing: no "Nothing was signed" claim
    const preflight = named('NativeSendRejectedError', 'Transaction rejected before broadcast: x', { phase: 'preflight', programError: { code: 6014, name: 'NotMature' } });
    expect(describeNativeError(preflight)).toBe(programErrorCopy('NotMature'));
  });

  it('keeps the unfulfilled-randomness rejection identifiable (the capture step shows "awaiting randomness" for it)', () => {
    const e = named('NativeSendRejectedError', 'x', { phase: 'simulation', kind: 'program', programError: { code: 6042, name: 'RequestNotFulfilled' } });
    expect((e as unknown as { programError: { name: string } }).programError.name).toBe('RequestNotFulfilled');
    expect(describeNativeError(e)).toMatch(/RequestNotFulfilled/);
  });

  it('maps a retryable simulation outage, cluster errors and a tampered signed transaction', () => {
    expect(describeNativeError(named('NativeSimulationUnavailableError', 'raw'))).toMatch(/Nothing was signed; try again/);
    expect(describeNativeError(named('NativeClusterMismatchError', 'raw', { reason: 'genesis-hash' }))).toMatch(/genesis hash differs/);
    expect(describeNativeError(named('NativeClusterMismatchError', 'raw', { reason: 'cluster-domain' }))).toMatch(/different deployment/);
    expect(describeNativeError(named('NativeClusterMismatchError', 'raw', { reason: 'program-id' }))).toMatch(/different program/);
    expect(describeNativeError(named('NativeClusterNotConfiguredError', 'raw'))).toMatch(/isn't available on this cluster yet/);
    expect(describeNativeError(named('SignedTransactionMismatchError', 'raw', { reason: 'fee-payer' }))).toMatch(/changed the fee payer.*Nothing was sent/);
    expect(describeNativeError(named('SignedTransactionMismatchError', 'raw', { reason: 'message-mutated' }))).toMatch(/different transaction/);
    expect(describeNativeError(named('SignedTransactionMismatchError', 'raw', { reason: 'signature' }))).toMatch(/valid signature/);
  });

  it('a cluster switch: a re-send is refused with the origin cluster; an action says nothing was signed', () => {
    const origin = { cluster: { expectedClusterDomain: 'localnet', programId: 'p' }, rpcEndpoint: 'http://127.0.0.1:8899', generation: 0, clusterKey: 'k' };
    const retry = named('NativeClusterSwitchedError', 'raw', { step: 'retry', origin });
    expect(describeNativeError(retry)).toMatch(/was not re-sent.*sent on localnet \(127\.0\.0\.1:8899\).*Check status/);
    const action = named('NativeClusterSwitchedError', 'raw', { step: 'action', origin });
    expect(describeNativeError(action)).toMatch(/Nothing was signed or sent/);
  });

  it('a wallet rejection that merely contains the word does not hide a typed error', () => {
    // NativeClusterMismatchError text mentions neither, but SignedTransactionMismatchError messages include "rejected"-like words in some wallets
    expect(describeNativeError(named('SignedTransactionMismatchError', 'The wallet rejected the message', { reason: 'signature' }))).toMatch(/valid signature/);
  });
});
