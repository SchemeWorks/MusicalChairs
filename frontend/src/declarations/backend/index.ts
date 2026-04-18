import type {
  GamePlan as GamePlanT,
  DealerType as DealerTypeT,
} from './backend.did';

export { idlFactory, init } from './backend.did.js';

export type {
  UserRole,
  UserProfile,
  GameRecord,
  PlatformStats,
  GameResetRecord,
  DealerPosition,
  HouseLedgerRecord,
  CoverChargeEntry,
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
} from './backend.did';

export type GamePlan = GamePlanT;
export const GamePlan = {
  simple21Day: { simple21Day: null } as GamePlanT,
  compounding15Day: { compounding15Day: null } as GamePlanT,
  compounding30Day: { compounding30Day: null } as GamePlanT,
};

export type DealerType = DealerTypeT;
export const DealerType = {
  upstream: { upstream: null } as DealerTypeT,
  downstream: { downstream: null } as DealerTypeT,
};
