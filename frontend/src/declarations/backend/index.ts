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

// DealerType enum values for frontend use
export const DealerType = {
  upstream: { 'upstream': null } as const,
  downstream: { 'downstream': null } as const,
};
