import type {
  GamePlan as GamePlanT,
  BackerType as BackerTypeT,
} from './ponzi_math_sol.did';

export { idlFactory, init } from './ponzi_math_sol.did.js';

export type {
  ActivePlanSnapshot,
  BackerKey,
  BackerPosition,
  BuyIntent,
  BuyReservation,
  ConsentInfo,
  ConsentMessage,
  ConsentMessageMetadata,
  ConsentMessageRequest,
  ConsentMessageResponse,
  ConsentMessageSpec,
  DepositIntent,
  DeskQuote,
  DeskTier,
  DeviceSpec,
  GameRecord,
  GameResetRecord,
  GeneralLedgerEntry,
  GeneralLedgerEvent,
  Icrc21Error,
  KeyId,
  LineDisplayPage,
  PlatformStats,
  PonziMathSol,
  Provider,
  QuoteLeg,
  RoundSummary,
  SchnorrAlgorithm,
  StandardRecord,
  TrustedOriginsResponse,
  _SERVICE,
} from './ponzi_math_sol.did';

export type GamePlan = GamePlanT;
export const GamePlan = {
  simple21Day: { simple21Day: null } as GamePlanT,
  compounding15Day: { compounding15Day: null } as GamePlanT,
  compounding30Day: { compounding30Day: null } as GamePlanT,
};

export type BackerType = BackerTypeT;
export const BackerType = {
  seriesA: { seriesA: null } as BackerTypeT,
  seriesB: { seriesB: null } as BackerTypeT,
};
