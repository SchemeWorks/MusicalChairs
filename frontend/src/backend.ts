export { idlFactory } from './declarations/backend';
export type {
  UserRole,
  UserProfile,
  _SERVICE,
} from './declarations/backend';

export {
  idlFactory as ponziMathIdlFactory,
  GamePlan,
  BackerType,
} from './declarations/ponzi_math';
export type {
  GameRecord,
  PlatformStats,
  GameResetRecord,
  BackerPosition,
  BackerKey,
  GeneralLedgerEntry,
  GeneralLedgerEvent,
  ActivePlanSnapshot,
  RoundSummary,
  _SERVICE as PonziMathService,
} from './declarations/ponzi_math';

export { ShenaniganType, ShenaniganOutcome } from './declarations/shenanigans';
export type {
  ShenaniganRecord,
  ShenaniganStats,
  ShenaniganConfig,
  ActiveSpellEffects,
  ShieldState,
} from './declarations/shenanigans';
