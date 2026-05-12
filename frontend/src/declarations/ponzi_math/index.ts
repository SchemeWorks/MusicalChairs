import type {
  GamePlan as GamePlanT,
  BackerType as BackerTypeT,
} from './ponzi_math.did';

export { idlFactory, init } from './ponzi_math.did.js';

export type {
  GameRecord,
  PlatformStats,
  GameResetRecord,
  BackerPosition,
  GeneralLedgerEntry,
  GeneralLedgerEvent,
  ConsentInfo,
  ConsentMessage,
  ConsentMessageMetadata,
  ConsentMessageRequest,
  ConsentMessageResponse,
  ConsentMessageSpec,
  DeviceSpec,
  Icrc21Error,
  LineDisplayPage,
  StandardRecord,
  TrustedOriginsResponse,
  _SERVICE,
} from './ponzi_math.did';

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
