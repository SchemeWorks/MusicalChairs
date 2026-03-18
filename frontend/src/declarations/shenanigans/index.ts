export { idlFactory, init } from './shenanigans.did.js';
export * from './shenanigans.did.d.ts';

export type {
  ShenaniganType,
  ShenaniganOutcome,
  ShenaniganRecord,
  ShenaniganStats,
  ShenaniganConfig,
  _SERVICE,
} from './shenanigans.did.d.ts';

// ShenaniganType enum values for frontend use
export const ShenaniganType = {
  moneyTrickster: { 'moneyTrickster': null } as const,
  aoeSkim: { 'aoeSkim': null } as const,
  renameSpell: { 'renameSpell': null } as const,
  mintTaxSiphon: { 'mintTaxSiphon': null } as const,
  downlineHeist: { 'downlineHeist': null } as const,
  magicMirror: { 'magicMirror': null } as const,
  ppBoosterAura: { 'ppBoosterAura': null } as const,
  purseCutter: { 'purseCutter': null } as const,
  whaleRebalance: { 'whaleRebalance': null } as const,
  downlineBoost: { 'downlineBoost': null } as const,
  goldenName: { 'goldenName': null } as const,
};

// ShenaniganOutcome enum values for frontend use
export const ShenaniganOutcome = {
  success: { 'success': null } as const,
  fail: { 'fail': null } as const,
  backfire: { 'backfire': null } as const,
};
