import type {
  ShenaniganType as ShenaniganTypeT,
  ShenaniganOutcome as ShenaniganOutcomeT,
} from './shenanigans.did';

export { idlFactory, init } from './shenanigans.did.js';

export type {
  ShenaniganRecord,
  ShenaniganStats,
  ShenaniganConfig,
  _SERVICE,
} from './shenanigans.did';

export type ShenaniganType = ShenaniganTypeT;
export const ShenaniganType = {
  moneyTrickster: { moneyTrickster: null } as ShenaniganTypeT,
  aoeSkim: { aoeSkim: null } as ShenaniganTypeT,
  renameSpell: { renameSpell: null } as ShenaniganTypeT,
  mintTaxSiphon: { mintTaxSiphon: null } as ShenaniganTypeT,
  downlineHeist: { downlineHeist: null } as ShenaniganTypeT,
  magicMirror: { magicMirror: null } as ShenaniganTypeT,
  ppBoosterAura: { ppBoosterAura: null } as ShenaniganTypeT,
  purseCutter: { purseCutter: null } as ShenaniganTypeT,
  whaleRebalance: { whaleRebalance: null } as ShenaniganTypeT,
  downlineBoost: { downlineBoost: null } as ShenaniganTypeT,
  goldenName: { goldenName: null } as ShenaniganTypeT,
};

export type ShenaniganOutcome = ShenaniganOutcomeT;
export const ShenaniganOutcome = {
  success: { success: null } as ShenaniganOutcomeT,
  fail: { fail: null } as ShenaniganOutcomeT,
  backfire: { backfire: null } as ShenaniganOutcomeT,
};
