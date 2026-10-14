// Test-only builders for the panels' view-models (IMPL-46c): decoded Pool /
// ConfigVersion / Position / Backer / CloseRecord values with sane defaults.
// Imported by *.test.ts only, never by app code.

import { decodeAccountData } from './codecs';
import { buildAccount } from './testFixtures';
import type { NativeBacker, NativeCloseRecord, NativeConfigVersion, NativePool, NativePosition } from './types';

type Rec = Record<string, unknown>;

export const ROI_15_Q18 = 4_473_565_759_257_037_859n;
export const ROI_30_Q18 = 12_267_678_469_131_246_220n;
export const OWNER = 'Bswb3UyeD1pUTaGiE6WvqwFpJZsQSEY1xhJePCDTHdvp';
export const OPENED_AT = 1_800_000_000n;

export function testConfig(overrides: Rec = {}): NativeConfigVersion {
  return decodeAccountData<NativeConfigVersion>(
    'ConfigVersion',
    buildAccount('ConfigVersion', {
      version: 1n,
      planRatesBpsPerDay: [1_100n, 1_200n, 900n],
      planDurationsS: [1_814_400n, 1_296_000n, 2_592_000n],
      compoundFactorsQ18: [ROI_15_Q18, ROI_30_Q18],
      simpleTollBps: [1_200n, 750n, 300n],
      simpleTollDayBounds: [7n, 14n],
      jackpotBps: [900n, 1_300n],
      frontEndLoadBps: 400n,
      minGame: 10_000_000n,
      minA: 50_000_000n,
      seniorBps: 3_500n,
      aBps: 4_500n,
      bBps: 2_000n,
      aEntitlementBps: 12_400n,
      bEntitlementBps: 11_600n,
      pendingRequestBytes: 200n,
      fulfilledRequestBytes: 300n,
      publicSeriesAEnabled: true,
      ...overrides,
    }),
  );
}

export function testPool(overrides: Rec = {}): NativePool {
  return decodeAccountData<NativePool>(
    'Pool',
    buildAccount('Pool', { roundId: 1n, pot: 10_000_000_000n, nextCloseId: 1n, ...overrides }),
  );
}

export function testPosition(overrides: Rec = {}): NativePosition {
  return decodeAccountData<NativePosition>(
    'Position',
    buildAccount('Position', {
      owner: OWNER,
      nonce: 7n,
      roundId: 1n,
      configVersion: 1n,
      kind: 0n,
      principal: 1_000_000_000n,
      openedAt: OPENED_AT,
      maturityAt: OPENED_AT + 1_814_400n,
      lastAccruedAt: OPENED_AT,
      ...overrides,
    }),
  );
}

export function testBacker(overrides: Rec = {}): NativeBacker {
  return decodeAccountData<NativeBacker>(
    'Backer',
    buildAccount('Backer', { owner: OWNER, series: 0n, entitlement: 1_240_000_000n, active: true, ...overrides }),
  );
}

export function testClose(overrides: Rec = {}): NativeCloseRecord {
  return decodeAccountData<NativeCloseRecord>(
    'CloseRecord',
    buildAccount('CloseRecord', {
      closeId: 1n,
      roundId: 1n,
      configVersion: 1n,
      closeStartedAt: OPENED_AT,
      promotionDeadline: OPENED_AT + 600n,
      cashCommitted: true,
      participantCount: 3n,
      requiredPagesPerSet: 1n,
      ...overrides,
    }),
  );
}
