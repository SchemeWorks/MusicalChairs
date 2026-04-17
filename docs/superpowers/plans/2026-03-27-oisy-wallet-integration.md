# Oisy Wallet Integration — Fix Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix broken Oisy wallet integration so canister calls use the real Oisy principal (not anonymous) with proper ICRC-112 batching for approve+operation flows.

**Architecture:** Backend gets ICRC-21 consent messages (Motoko), ICRC-28 trusted origins, and ICRC-10 standards declaration. Frontend replaces `@dfinity/oisy-wallet-signer` (account-discovery-only) with `@slide-computer/signer` v4 + `signer-agent` v4, creating a proper SignerAgent that routes canister calls through the Oisy signer popup. The createGame flow uses ICRC-112 sequential batching (approve → createGame in one popup).

**Tech Stack:** Motoko (backend), React/TypeScript (frontend), `@slide-computer/signer` v4, `@slide-computer/signer-agent` v4, `@slide-computer/signer-web` v4

**Reference:** Working Rumi Protocol v2 implementation at `/Users/robertripley/coding/rumi-protocol-v2/src/vault_frontend/src/lib/services/oisySigner.ts` and `/Users/robertripley/coding/rumi-protocol-v2/src/rumi_protocol_backend/src/icrc21.rs`

---

## File Structure

### Backend (new files)
- **Create:** `backend/icrc21.mo` — ICRC-21 consent messages, ICRC-28 trusted origins, ICRC-10 standards
- **Modify:** `backend/main.mo` — Import icrc21 module, add 3 public entry points

### Frontend (new + modified files)
- **Create:** `frontend/src/lib/oisySigner.ts` — Oisy SignerAgent creation + caching + actor factory
- **Modify:** `package.json` — Add `@slide-computer/signer`, `signer-agent`, `signer-web`; remove `@dfinity/oisy-wallet-signer`
- **Modify:** `frontend/src/hooks/useWallet.tsx` — Replace `connectOisy` to store Oisy principal (no AnonymousIdentity); clean up disconnect
- **Modify:** `frontend/src/hooks/useActor.ts` — Add Oisy branch that creates actor via SignerAgent
- **Modify:** `frontend/src/hooks/useQueries.ts` — ICRC-112 batching in `useCreateGame` mutation

---

## Task 1: Backend ICRC-21/28/10 Module

**Files:**
- Create: `backend/icrc21.mo`
- Modify: `backend/main.mo`

- [ ] **Step 1: Create `backend/icrc21.mo`**

Implement a Motoko module with:
- ICRC-21 types: `ConsentMessageRequest`, `ConsentMessageSpec`, `ConsentMessageMetadata`, `DeviceSpec`, `ConsentMessage`, `ConsentInfo`, `Icrc21Error`
- `consentMessage(request)` — generates human-readable consent messages for each update method:
  - `saveCallerUserProfile` → "Set Display Name"
  - `createGame` → "Open Investment Position" (include plan type + ICP amount if decodable)
  - `withdrawEarnings` → "Withdraw Earnings"
  - `addDealerMoney` → "Fund as Backer" (include ICP amount if decodable)
  - `addDownstreamDealer` → "Fund as Series B Backer"
  - `initializeAccessControl` → "Initialize Account"
  - All other update methods → `UnsupportedCanisterCall` error
- `trustedOrigins()` — returns Musical Chairs origins:
  - `https://5qu42-fqaaa-aaaac-qecla-cai.icp0.io`
  - `https://5qu42-fqaaa-aaaac-qecla-cai.raw.icp0.io`
  - `https://musicalchairs.fun`
  - `https://www.musicalchairs.fun`
- `supportedStandards()` — returns ICRC-10, ICRC-21, ICRC-28

**CRITICAL Motoko note:** The `ConsentMessageSpec` type MUST be nested (contains `metadata: ConsentMessageMetadata` + `device_spec: ?DeviceSpec`), NOT flat. Oisy sends the nested structure. Getting this wrong causes Candid decoding failures.

- [ ] **Step 2: Wire entry points in `backend/main.mo`**

Add three public functions to the actor:
```motoko
import Icrc21 "icrc21";

// ICRC-21 Consent Messages
public shared func icrc21_canister_call_consent_message(request : Icrc21.ConsentMessageRequest) : async Icrc21.ConsentMessageResponse {
    Icrc21.consentMessage(request);
};

// ICRC-28 Trusted Origins
public query func icrc28_trusted_origins() : async Icrc21.TrustedOriginsResponse {
    Icrc21.trustedOrigins();
};

// ICRC-10 Supported Standards
public query func icrc10_supported_standards() : async [Icrc21.StandardRecord] {
    Icrc21.supportedStandards();
};
```

- [ ] **Step 3: Update Candid declarations**

Run: `dfx generate backend`

This regenerates `frontend/src/declarations/backend/` with the new ICRC-21/28/10 methods.

- [ ] **Step 4: Verify ICRC-21 locally**

```bash
dfx deploy backend --network local
dfx canister call backend icrc21_canister_call_consent_message \
  '(record { method = "saveCallerUserProfile"; arg = blob "\44\49\44\4C\00\00"; user_preferences = record { metadata = record { language = "en"; utc_offset_minutes = null }; device_spec = null } })' \
  --network local
```

Expected: `Ok` variant with "Set Display Name" consent message.

```bash
dfx canister call backend icrc28_trusted_origins --network local
dfx canister call backend icrc10_supported_standards --network local
```

- [ ] **Step 5: Commit**

```bash
git add backend/icrc21.mo backend/main.mo
git commit -m "feat: ICRC-21 consent messages, ICRC-28 trusted origins, ICRC-10 standards"
```

---

## Task 2: Frontend — Install Signer Packages

**Files:**
- Modify: `package.json`

- [ ] **Step 1: Install @slide-computer packages, remove old Oisy signer**

```bash
npm install @slide-computer/signer@^4.2.2 @slide-computer/signer-agent@^4.2.2 @slide-computer/signer-web@^4.2.2
npm uninstall @dfinity/oisy-wallet-signer
```

- [ ] **Step 2: Commit**

```bash
git add package.json package-lock.json
git commit -m "deps: replace @dfinity/oisy-wallet-signer with @slide-computer/signer v4"
```

---

## Task 3: Frontend — Create Oisy Signer Module

**Files:**
- Create: `frontend/src/lib/oisySigner.ts`

- [ ] **Step 1: Create `frontend/src/lib/oisySigner.ts`**

Port from Rumi's implementation (`/Users/robertripley/coding/rumi-protocol-v2/src/vault_frontend/src/lib/services/oisySigner.ts`). The module:

1. Creates a module-level `Signer` with `PostMessageTransport` pointing to `https://oisy.com/sign`
2. Exports `getOisySignerAgent(principal)` — creates/caches a v4 `SignerAgent`
3. Exports `createOisyActor(canisterId, idlFactory, signerAgent)` — creates an actor routed through the signer
4. Exports `clearOisySigner()` — clears cache on disconnect

Key: Let `SignerAgent.create()` build its own internal HttpAgent. Do NOT pass `@dfinity/agent`'s HttpAgent — causes ArrayBuffer vs Uint8Array rootKey mismatch.

- [ ] **Step 2: Commit**

```bash
git add frontend/src/lib/oisySigner.ts
git commit -m "feat: Oisy signer module with v4 SignerAgent + ICRC-112 batching"
```

---

## Task 4: Frontend — Fix Oisy Connection in useWallet

**Files:**
- Modify: `frontend/src/hooks/useWallet.tsx`

- [ ] **Step 1: Update `connectOisy`**

Replace the current implementation that uses `@dfinity/oisy-wallet-signer` + `AnonymousIdentity`. The new flow:
1. Use `@slide-computer/signer` v4's `Signer` to get accounts (via `signer.accounts()`)
2. Store the Oisy principal
3. Store `walletType = 'oisy'`
4. Do NOT set `identity` to `AnonymousIdentity` — leave it `null` and let `useActor` handle the Oisy path separately

Key change: the Oisy path no longer pretends to have an identity. Instead, `useActor` detects `walletType === 'oisy'` and creates a SignerAgent-backed actor.

- [ ] **Step 2: Update `disconnect` to clear Oisy signer**

```typescript
import { clearOisySigner } from '../lib/oisySigner';
// In disconnect:
clearOisySigner();
```

- [ ] **Step 3: Remove Oisy session restoration from `initializeWallet`**

The current code restores Oisy sessions using AuthClient (`client.isAuthenticated()`), which is wrong — Oisy doesn't use AuthClient. Oisy sessions can't be persisted across page reloads (signer protocol limitation). Remove the `'oisy'` check from the `savedWalletType === 'internet-identity' || savedWalletType === 'oisy'` branch.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/hooks/useWallet.tsx
git commit -m "fix: Oisy connection uses signer v4 accounts, no AnonymousIdentity"
```

---

## Task 5: Frontend — Fix Oisy Actor Creation in useActor

**Files:**
- Modify: `frontend/src/hooks/useActor.ts`

- [ ] **Step 1: Add Oisy branch to actor creation**

After the Plug branch, before the II/anonymous branch:

```typescript
import { getOisySignerAgent, createOisyActor } from '../lib/oisySigner';
import { Principal } from '@dfinity/principal';

// In createActor():
if (walletType === 'oisy' && principal) {
  const signerAgent = await getOisySignerAgent(Principal.fromText(principal));
  const newActor = createOisyActor(BACKEND_CANISTER_ID, idlFactory, signerAgent);
  setActor(newActor);
  return;
}
```

This creates an actor where every call routes through the Oisy SignerAgent, which opens a popup for signing. The `caller` in the canister will be the Oisy principal.

- [ ] **Step 2: Commit**

```bash
git add frontend/src/hooks/useActor.ts
git commit -m "feat: Oisy actor creation via SignerAgent — real principal for canister calls"
```

---

## Task 6: Frontend — ICRC-112 Batching for createGame

**Files:**
- Modify: `frontend/src/hooks/useQueries.ts`

- [ ] **Step 1: Update `useCreateGame` mutation for Oisy batching**

The current flow: `ledger.approve()` → `actor.createGame()` (two separate calls). For Oisy, this needs ICRC-112 batching (one popup).

In the `useCreateGame` mutation, detect Oisy and batch:

```typescript
import { getOisySignerAgent, createOisyActor } from '../lib/oisySigner';
import { idlFactory as ledgerIdlFactory } from '../declarations/ledger'; // or inline

// Inside mutationFn:
if (walletType === 'oisy' && principal) {
  const signerAgent = await getOisySignerAgent(Principal.fromText(principal));

  // Create actors that route through the signer
  const ledgerActor = createOisyActor(ICP_LEDGER_CANISTER_ID, ledgerIdlFactory, signerAgent);
  const backendActor = createOisyActor(BACKEND_CANISTER_ID, idlFactory, signerAgent);

  // Sequence 0: approve
  signerAgent.batch();
  const approvePromise = ledgerActor.icrc2_approve({
    amount: approveAmount,
    spender: { owner: Principal.fromText(BACKEND_CANISTER_ID), subaccount: [] },
    expires_at: [], expected_allowance: [], memo: [], fee: [],
    from_subaccount: [], created_at_time: [],
  });

  // Sequence 1: createGame
  signerAgent.batch();
  const gamePromise = backendActor.createGame(plan, amount, isCompounding, referrer);

  // Fire single ICRC-112 request — ONE signer popup
  await signerAgent.execute();
  const [_, gameId] = await Promise.all([approvePromise, gamePromise]);
  return { gameId, amount, plan: planId };
}

// Non-Oisy path: existing approve + createGame flow (unchanged)
```

**CRITICAL:** No `await` between the user's click and `signerAgent.execute()`. The `batch()` calls and actor method calls are synchronous queue operations. Only `execute()` is async (opens the popup). Any async work between click and execute burns the browser gesture context and the popup gets blocked.

- [ ] **Step 2: Commit**

```bash
git add frontend/src/hooks/useQueries.ts
git commit -m "feat: ICRC-112 batching for Oisy createGame (approve + game in one popup)"
```

---

## Task 7: Build, Test, Deploy

- [ ] **Step 1: Build frontend**

```bash
npm run build
```

Fix any TypeScript errors.

- [ ] **Step 2: Deploy backend** (includes ICRC-21/28/10 + daysActive fix)

```bash
dfx deploy backend --network ic
```

Verify: `dfx canister call backend icrc10_supported_standards --network ic`

- [ ] **Step 3: Deploy frontend**

```bash
dfx deploy frontend --network ic
```

- [ ] **Step 4: Manual verification**

1. Log in with II → verify "rob" profile + 3 games visible
2. Log out → log in with Plug → verify "steve" profile
3. Log out → log in with Oisy → verify Oisy principal shown, can save profile
4. Check splash page → daysActive should show ~11, not 20539

- [ ] **Step 5: Commit any remaining fixes + tag**

```bash
git add -A
git commit -m "fix: Oisy wallet integration, query scoping, daysActive"
```
