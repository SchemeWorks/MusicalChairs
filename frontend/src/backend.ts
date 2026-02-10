// Re-export everything from declarations for easier imports
// Import from index.ts explicitly to get the const values
export * from './declarations/backend/index.ts';
export { 
  idlFactory, 
  GamePlan, 
  ShenaniganType, 
  ShenaniganOutcome,
  DealerType
} from './declarations/backend/index.ts';

// Re-export types explicitly (excluding DealerType which is now exported as a const above)
export type { DealerPosition, WalletTransaction } from './declarations/backend/index.ts';
