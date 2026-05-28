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

export {
  idlFactory as ponziMathSolIdlFactory,
  GamePlan as SolGamePlan,
  BackerType as SolBackerType,
} from './declarations/ponzi_math_sol';
export type {
  GameRecord as SolGameRecord,
  PlatformStats as SolPlatformStats,
  GameResetRecord as SolGameResetRecord,
  BackerPosition as SolBackerPosition,
  BackerKey as SolBackerKey,
  GeneralLedgerEntry as SolGeneralLedgerEntry,
  GeneralLedgerEvent as SolGeneralLedgerEvent,
  ActivePlanSnapshot as SolActivePlanSnapshot,
  RoundSummary as SolRoundSummary,
  DepositIntent as SolDepositIntent,
  _SERVICE as PonziMathSolService,
} from './declarations/ponzi_math_sol';

export { ShenaniganType, ShenaniganOutcome } from './declarations/shenanigans';
export type {
  ShenaniganRecord,
  ShenaniganStats,
  ShenaniganConfig,
  ActiveSpellEffects,
  ShieldState,
} from './declarations/shenanigans';
