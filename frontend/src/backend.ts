// Re-export everything from backend declarations
export * from './declarations/backend/index.ts';
export {
  idlFactory,
  GamePlan,
  DealerType
} from './declarations/backend/index.ts';

export type { DealerPosition, WalletTransaction } from './declarations/backend/index.ts';

// Re-export shenanigan types from shenanigans canister declarations
export {
  ShenaniganType,
  ShenaniganOutcome,
} from './declarations/shenanigans/index.ts';

export type {
  ShenaniganRecord,
  ShenaniganStats,
  ShenaniganConfig,
} from './declarations/shenanigans/index.ts';
