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
  GeneralLedgerEntry,
  GeneralLedgerEvent,
  _SERVICE as PonziMathService,
} from './declarations/ponzi_math';

export { ShenaniganType, ShenaniganOutcome } from './declarations/shenanigans';
export type {
  ShenaniganRecord,
  ShenaniganStats,
  ShenaniganConfig,
} from './declarations/shenanigans';
