export { idlFactory, init } from './backend.did.js';
export * from './backend.did.d.ts';

// Re-export types for convenience (only types that exist in backend.did.d.ts)
export type {
  UserRole,
  UserProfile,
  GamePlan,
  GameRecord,
  PlatformStats,
  GameResetRecord,
  DealerType,
  DealerPosition,
  ShenaniganType,
  ShenaniganOutcome,
  ShenaniganRecord,
  ShenaniganStats,
  ShenaniganConfig,
  HouseLedgerRecord,
  WalletTransaction,
  _SERVICE,
} from './backend.did.d.ts';

// GamePlan enum values for frontend use
export const GamePlan = {
  simple21Day: { 'simple21Day': null } as const,
  compounding15Day: { 'compounding15Day': null } as const,
  compounding30Day: { 'compounding30Day': null } as const,
};

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

// DealerType enum values for frontend use
export const DealerType = {
  upstream: { 'upstream': null } as const,
  downstream: { 'downstream': null } as const,
};

// ShenaniganOutcome enum values for frontend use
export const ShenaniganOutcome = {
  success: { 'success': null } as const,
  fail: { 'fail': null } as const,
  backfire: { 'backfire': null } as const,
};
