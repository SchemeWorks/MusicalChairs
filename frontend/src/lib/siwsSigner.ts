import {
  Actor,
  HttpAgent,
  type ActorSubclass,
  type Identity,
  type Signature,
  type DerEncodedPublicKey,
} from '@dfinity/agent';
import {
  Delegation,
  DelegationChain,
  DelegationIdentity,
  Ed25519KeyIdentity,
} from '@dfinity/identity';
import bs58 from 'bs58';
import { idlFactory } from '../declarations/siws_provider';
import type {
  _SERVICE as SiwsProviderService,
  SiwsMessage,
  SignedDelegation as SiwsSignedDelegation,
  PublicKey as SiwsPublicKey,
} from '../declarations/siws_provider/siws_provider.did';

// The siws_provider canister IDs.
//
// Local matches `dfx deploy siws_provider` on a fresh local replica with the
// canisters in dfx.json declared in their current order (Task 6 deploy).
// Mainnet ID will be filled in at Task 13 once we install the canister on IC.
//
// Convention follows useWallet.tsx (lines 38-80): IDs are hardcoded per-hook
// (or per-module) rather than imported from declarations/index.ts, so the
// local-vs-mainnet switch is co-located with the code that needs it.
const SIWS_PROVIDER_CANISTER_ID_LOCAL = 'uxrrr-q7777-77774-qaaaq-cai';
const SIWS_PROVIDER_CANISTER_ID_MAINNET = 'tcm26-yqaaa-aaaac-qg2lq-cai';

const IS_LOCAL =
  typeof window !== 'undefined' && window.location.hostname === 'localhost';
const IC_HOST = IS_LOCAL ? 'http://localhost:4943' : 'https://icp0.io';
const SIWS_PROVIDER_CANISTER_ID = IS_LOCAL
  ? SIWS_PROVIDER_CANISTER_ID_LOCAL
  : SIWS_PROVIDER_CANISTER_ID_MAINNET;

// LocalStorage keys, namespaced so they don't collide with ic-siws-js's own
// `siwsIdentity` key (which we deliberately bypass — see top-of-file comment).
const LS_KEY_DELEGATION = 'musical-chairs-siws-delegation';
const LS_KEY_SESSION_KEY = 'musical-chairs-siws-session-key';
const LS_KEY_PUBKEY = 'musical-chairs-siws-pubkey';

// Contract shared with useWallet.tsx (Task 9 wires this in).
export interface SiwsConnection {
  identity: Identity;
  principal: string;
  solanaPubkey: string; // base58
}

// We can't use ic-siws-js's `SiwsManager.login()` because it requires a
// wallet-adapter with a `signIn` method baked into the wallet-standard. Our
// caller (useWallet.tsx) has only the generic `signMessage(bytes) => bytes`
// callback from @solana/wallet-adapter-react's useWallet(). So we drive the
// siws_provider canister directly via its actor, signing the canonical SIWS
// message text ourselves and bs58-encoding the signature before submission.
//
// The canonical SIWS message text format below is the one the ic_siws_provider
// canister reconstructs server-side from the same `SiwsMessage` record returned
// by `siws_prepare_login`. As long as the bytes we sign match the bytes the
// canister re-derives, signature verification passes.
function buildSiwsMessageText(msg: SiwsMessage): string {
  const issuedAtIso = new Date(Number(msg.issued_at / 1_000_000n)).toISOString();
  const expirationIso = new Date(
    Number(msg.expiration_time / 1_000_000n),
  ).toISOString();
  return (
    `${msg.domain} wants you to sign in with your Solana account:\n` +
    `${msg.address}\n` +
    `\n` +
    `${msg.statement}\n` +
    `\n` +
    `URI: ${msg.uri}\n` +
    `Version: ${msg.version}\n` +
    `Chain ID: ${msg.chain_id}\n` +
    `Nonce: ${msg.nonce}\n` +
    `Issued At: ${issuedAtIso}\n` +
    `Expiration Time: ${expirationIso}`
  );
}

// Tag a raw byte buffer as a Signature / DerEncodedPublicKey. The dfinity
// agent uses these brand types to distinguish public-key bytes from arbitrary
// buffers at compile time; the runtime representation is plain ArrayBuffer.
// Mirrored from ic-siws-js's internal `delegation.ts` (which isn't re-exported
// at the package root — see https://github.com/kristoferlund/ic-siws/blob/main/packages/ic_siws_js/src/delegation.ts).
function asSignature(signature: Uint8Array | number[]): Signature {
  const bytes = signature instanceof Uint8Array ? signature : Uint8Array.from(signature);
  const buf = bytes.buffer.slice(
    bytes.byteOffset,
    bytes.byteOffset + bytes.byteLength,
  ) as ArrayBuffer & { __signature__?: void };
  buf.__signature__ = undefined;
  return buf as Signature;
}

function asDerEncodedPublicKey(
  publicKey: Uint8Array | number[],
): DerEncodedPublicKey {
  const bytes = publicKey instanceof Uint8Array ? publicKey : Uint8Array.from(publicKey);
  const buf = bytes.buffer.slice(
    bytes.byteOffset,
    bytes.byteOffset + bytes.byteLength,
  ) as ArrayBuffer & { __derEncodedPublicKey__?: void };
  buf.__derEncodedPublicKey__ = undefined;
  return buf as DerEncodedPublicKey;
}

function buildDelegationChain(
  signedDelegation: SiwsSignedDelegation,
  canisterPublicKey: SiwsPublicKey,
): DelegationChain {
  const pubkeyBytes =
    signedDelegation.delegation.pubkey instanceof Uint8Array
      ? signedDelegation.delegation.pubkey
      : Uint8Array.from(signedDelegation.delegation.pubkey);
  return DelegationChain.fromDelegations(
    [
      {
        delegation: new Delegation(
          pubkeyBytes.buffer.slice(
            pubkeyBytes.byteOffset,
            pubkeyBytes.byteOffset + pubkeyBytes.byteLength,
          ),
          signedDelegation.delegation.expiration,
          signedDelegation.delegation.targets[0],
        ),
        signature: asSignature(signedDelegation.signature),
      },
    ],
    asDerEncodedPublicKey(canisterPublicKey),
  );
}

async function createAnonymousActor(): Promise<ActorSubclass<SiwsProviderService>> {
  const agent = await HttpAgent.create({
    host: IC_HOST,
    shouldFetchRootKey: IS_LOCAL,
  });
  return Actor.createActor<SiwsProviderService>(idlFactory, {
    agent,
    canisterId: SIWS_PROVIDER_CANISTER_ID,
  });
}

// Connects via SIWS. Runs the prepare_login → user-signs → login →
// get_delegation flow against the siws_provider canister, builds a
// DelegationIdentity from the returned chain, and persists everything to
// localStorage so `restoreSiwsSession` can resurrect the session on reload.
export async function connectSiws(
  signMessage: (msg: Uint8Array) => Promise<Uint8Array>,
  publicKey: string,
): Promise<SiwsConnection> {
  if (!SIWS_PROVIDER_CANISTER_ID) {
    throw new Error(
      'siws_provider canister ID is not configured for this environment.',
    );
  }

  const actor = await createAnonymousActor();

  // 1. Ask the canister to prepare a SIWS message keyed by the user's pubkey.
  const prepareResp = await actor.siws_prepare_login(publicKey);
  if ('Err' in prepareResp) {
    throw new Error(`siws_prepare_login failed: ${prepareResp.Err}`);
  }
  const siwsMessage = prepareResp.Ok;

  // 2. User signs the canonical SIWS message text with their Solana wallet.
  const messageBytes = new TextEncoder().encode(buildSiwsMessageText(siwsMessage));
  const signatureBytes = await signMessage(messageBytes);

  // 3. Generate a fresh session key (Ed25519) — its public key is what the
  // delegation will authorize the user to sign IC requests with.
  const sessionIdentity = Ed25519KeyIdentity.generate();
  const sessionPublicKey = sessionIdentity.getPublicKey().toDer();

  // 4. Submit the signed message to the canister, which verifies the signature
  // against the message it reconstructs from its own stored prepareLogin state.
  const loginResp = await actor.siws_login(
    bs58.encode(signatureBytes),
    publicKey,
    new Uint8Array(sessionPublicKey),
    siwsMessage.nonce,
  );
  if ('Err' in loginResp) {
    throw new Error(`siws_login failed: ${loginResp.Err}`);
  }

  // 5. Fetch the signed delegation that binds the session key to the
  // canister-derived principal (deterministic from pubkey + canister salt).
  const delegationResp = await actor.siws_get_delegation(
    publicKey,
    new Uint8Array(sessionPublicKey),
    loginResp.Ok.expiration,
  );
  if ('Err' in delegationResp) {
    throw new Error(`siws_get_delegation failed: ${delegationResp.Err}`);
  }

  // 6. Assemble the DelegationChain + DelegationIdentity. `buildDelegationChain`
  // handles the buffer-tagging dance that the dfinity Delegation/Signature
  // types require — see helper at top of file.
  const delegationChain = buildDelegationChain(
    delegationResp.Ok,
    loginResp.Ok.user_canister_pubkey,
  );
  const identity = DelegationIdentity.fromDelegation(
    sessionIdentity,
    delegationChain,
  );

  // 7. Persist for restoreSiwsSession. We store the three pieces separately
  // (rather than one blob) so future migrations can rev parts in isolation.
  localStorage.setItem(
    LS_KEY_SESSION_KEY,
    JSON.stringify(sessionIdentity.toJSON()),
  );
  localStorage.setItem(
    LS_KEY_DELEGATION,
    JSON.stringify(delegationChain.toJSON()),
  );
  localStorage.setItem(LS_KEY_PUBKEY, publicKey);

  return {
    identity,
    principal: identity.getPrincipal().toText(),
    solanaPubkey: publicKey,
  };
}

// Rehydrates a saved SIWS session without prompting the user. Returns null if
// nothing is saved, anything is malformed, or the delegation has expired —
// in any of those cases the caller should fall back to a fresh `connectSiws`.
export async function restoreSiwsSession(): Promise<SiwsConnection | null> {
  try {
    const sessionKeyJson = localStorage.getItem(LS_KEY_SESSION_KEY);
    const delegationJson = localStorage.getItem(LS_KEY_DELEGATION);
    const pubkey = localStorage.getItem(LS_KEY_PUBKEY);

    if (!sessionKeyJson || !delegationJson || !pubkey) {
      return null;
    }

    const delegationChain = DelegationChain.fromJSON(delegationJson);

    // Expiry check: a chain may contain multiple delegations; the chain is
    // valid only while ALL of them are still in the future. expiration is
    // ns-since-epoch as bigint; Date.now() is ms.
    const nowNs = BigInt(Date.now()) * 1_000_000n;
    const allValid = delegationChain.delegations.every(
      (d) => d.delegation.expiration > nowNs,
    );
    if (!allValid) {
      clearSiwsSession();
      return null;
    }

    const sessionIdentity = Ed25519KeyIdentity.fromJSON(sessionKeyJson);
    const identity = DelegationIdentity.fromDelegation(
      sessionIdentity,
      delegationChain,
    );

    return {
      identity,
      principal: identity.getPrincipal().toText(),
      solanaPubkey: pubkey,
    };
  } catch {
    // Anything wrong with the persisted state — wipe it so the next attempt
    // starts clean rather than looping on a poisoned localStorage entry.
    clearSiwsSession();
    return null;
  }
}

export function clearSiwsSession(): void {
  localStorage.removeItem(LS_KEY_SESSION_KEY);
  localStorage.removeItem(LS_KEY_DELEGATION);
  localStorage.removeItem(LS_KEY_PUBKEY);
}

// Display helper for the wallet dropdown — `A1b2…Z9y8`. Base58 Solana
// pubkeys are typically 43-44 chars, so the truncation always kicks in for
// real keys; the early-return is just defensive for tests / weird inputs.
export function truncateSolanaPubkey(pubkey: string): string {
  if (pubkey.length <= 12) return pubkey;
  return `${pubkey.slice(0, 4)}…${pubkey.slice(-4)}`;
}
