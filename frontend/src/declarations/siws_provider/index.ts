// Hand-written wrapper around the dfx-generated siws_provider declarations.
// Mirrors the convention used by ./shenanigans/index.ts, ./backend/index.ts, etc.:
// re-export idlFactory + init for actor construction, plus the types the frontend
// consumes. Canister IDs are hardcoded in the consuming hooks (see useWallet.tsx),
// not exported from here.

export { idlFactory, init } from './siws_provider.did.js';

export type {
  _SERVICE,
  Address,
  CanisterPublicKey,
  Delegation,
  GetAddressResponse,
  GetDelegationResponse,
  GetPrincipalResponse,
  LoginDetails,
  LoginResponse,
  Nonce,
  PrepareLoginResponse,
  PublicKey,
  RuntimeFeature,
  SessionKey,
  SettingsInput,
  SignedDelegation,
  SiwsMessage,
  SiwsSignature,
  Timestamp,
} from './siws_provider.did';
