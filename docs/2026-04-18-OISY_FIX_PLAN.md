# Oisy Wallet Integration Fix — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Make Oisy wallet work end-to-end — queries load without popups, updates sign correctly under the user's real principal, and sessions survive page reloads.

**Architecture:** Two-actor model. A **read actor** built on anonymous `HttpAgent` handles every query call from every wallet type. A **write actor** is wallet-specific: Plug uses its injected agent, II uses `DelegationIdentity` + `HttpAgent`, Oisy uses `SignerAgent` from `@slide-computer/signer-agent`, always invoked inside a click handler, using ICRC-112 batching for multi-call flows (approve + op). All caller-scoped queries are converted to principal-parameterized queries so reads don't need a signing identity.

**Tech Stack:** `@dfinity/agent`, `@slide-computer/signer`, `@slide-computer/signer-agent` v4.2.2, `@slide-computer/signer-web`, Motoko (mo:core), TanStack Query, React.

---

## Background Context (read before starting)

This project is a ponzi game dapp on the Internet Computer. Three wallet types: Plug (`window.ic.plug`), Internet Identity (via `@dfinity/auth-client`), Oisy (via slide-computer signer).

**The bug we're fixing:** Oisy's `SignerAgent` requires a user click to open the Oisy popup for every canister call. Queries fire on render (no click), so they throw. A race during wallet connect causes some queries to hit the *anonymous* HttpAgent fallback — those succeed but as caller `2vxsx-fae` (the anonymous principal). One such query (`saveCallerUserProfile`) was made before `requireAuthenticated` was added to the backend, leaving a stale `{ name = "andy" }` entry keyed by the anonymous principal. Every unauthenticated or broken-Oisy visitor sees "Welcome, andy".

**Current state on-chain** (verified via `dfx canister --network ic call backend getUserProfile '(principal "...")'`):
- `stzp3-...-bqe` → `"steve"` (Plug user, correct)
- `gcbfr-...-xqe` → `"rob"` (II user, correct)
- `zegjz-...-tae` → `null` (Andy's real Oisy principal — never successfully wrote anything)
- `2vxsx-fae` → `"andy"` (anonymous principal — stale, needs deletion)

**Why the fix works:** Queries don't need to be signed on IC — the canister accepts anonymous queries and the app just tells it which user to look up via a `Principal` argument. Only update calls (writes) need the wallet signature, and those are always triggered by a click so the Oisy popup is allowed.

---

## File Structure

**Created:**
- `frontend/src/hooks/useReadActor.ts` — returns an anonymous-HttpAgent actor. Single instance, no wallet dependency.
- `frontend/src/lib/oisyDelegation.ts` — REMOVED from earlier draft. Not needed; signer session is enough.

**Modified:**
- `frontend/src/hooks/useActor.ts` — becomes the *write* actor (wallet-specific, for updates only). Delete query-path fallbacks.
- `frontend/src/hooks/useWallet.tsx` — add Oisy restore path in `initializeWallet`. Oisy session is cached by the signer itself; we just re-read `accounts()` on load if `savedWalletType === 'oisy'`.
- `frontend/src/lib/oisySigner.ts` — expose a helper to check if the signer has a cached session.
- `frontend/src/hooks/useQueries.ts` — every query switches from `useActor` to `useReadActor`. Caller-scoped queries switch to their principal-parameterized backend equivalents.
- `frontend/src/components/WalletDropdown.tsx` — remove the `__diagActor` + `whoAmI` diagnostic block. Use the read actor pattern.
- `backend/main.mo` — add principal-parameterized query variants for every caller-scoped query (see Task 1). Add one-time admin cleanup endpoint for the stale anonymous profile (see Task 2). Remove `whoAmI` diagnostic.
- `frontend/src/declarations/backend/backend.did`, `.did.js`, `.did.d.ts` — add new Candid methods, remove `whoAmI`.
- `frontend/src/hooks/useActor.ts` — remove the diagnostic `console.log`s added in the investigation phase.

**Reviewed, not modified (verification only):**
- `backend/icrc21.mo` — already has correct ICRC-21/28/10 with nested `ConsentMessageSpec` and all update method labels. No change required.

---

## Task 1: Add principal-parameterized query variants (backend)

The backend's caller-scoped queries (`getCallerUserProfile`, `getPonziPoints`, etc.) only work when the caller is the target principal. We need parallel queries that take a `Principal` parameter so anonymous queries work for any wallet.

**Files:**
- Modify: `backend/main.mo` (lines 81-87, 1306-1459 approx.)

**Caller queries in play** (found via `grep -n 'public query (\{ caller \})' backend/main.mo`):
- `getCallerUserProfile()` — already has `getUserProfile(user)`. No-op.
- `getCallerUserRole()` — add `getUserRole(user)`.
- `isCallerAdmin()` — add `isAdmin(user)`.
- `getCoverChargeBalance()` — add `getCoverChargeBalanceFor(user)`.
- `getCoverChargeTransactions()` — add `getCoverChargeTransactionsFor(user)`.
- `getPonziPoints()` — add `getPonziPointsFor(user)`.
- `getPonziPointsBalance()` — add `getPonziPointsBalanceFor(user)`. NOTE: there's already an update-style `getPonziPointsBalanceFor(principal)`; rename if collision.
- `getReferralTierPoints()` — add `getReferralTierPointsFor(user)`.
- `getUserGames()` — add `getUserGamesFor(user)`.
- `getDealerRepaymentBalance()` — add `getDealerRepaymentBalanceFor(user)`.
- `checkDepositRateLimit()` — leave as-is (this is a rate check, it has to be from the caller).

- [ ] **Step 1: Verify existing caller queries and their return types**

Run: `grep -n 'public query ({ caller })' backend/main.mo`

Expected output: ~10 matches. Write the list down — each one needs a sibling query that takes a `Principal` and returns the same type.

- [ ] **Step 2: Add the sibling queries. Example for `getPonziPoints`:**

```motoko
// Existing — keep.
public query ({ caller }) func getPonziPoints() : async Float {
    ponziPoints.get(caller);
};

// New — principal-parameterized variant.
public query func getPonziPointsFor(user : Principal) : async Float {
    ponziPoints.get(user);
};
```

Mechanically apply this pattern to each caller query in the list above. Copy the body, replace `caller` with `user`, name the new function with `For` suffix (e.g., `getUserGamesFor`).

Before renaming `getPonziPointsBalanceFor`, check the existing one:
```bash
grep -n 'getPonziPointsBalanceFor' backend/main.mo
```
If it already exists and takes a Principal, skip adding it. Otherwise add `getPonziPointsBalanceFor` to match the new naming convention.

- [ ] **Step 3: Remove the `whoAmI` diagnostic endpoint**

In `backend/main.mo`, delete the block added during debugging:
```motoko
public query ({ caller }) func whoAmI() : async Principal {
    caller;
};
```

- [ ] **Step 4: Compile to catch typos**

Run: `cd /Users/robertripley/coding/musicalchairs && dfx build backend 2>&1 | tail -20`

Expected: build succeeds, may show pre-existing `M0155`/`M0218` warnings, no new errors.

- [ ] **Step 5: Commit**

```bash
git add backend/main.mo
git commit -m "feat(backend): principal-parameterized query variants, remove whoAmI diagnostic"
```

---

## Task 2: Clean up stale anonymous profile

The `{ name = "andy" }` record at `2vxsx-fae` leaks to every unauthenticated visitor. Add an admin-only cleanup method that deletes a profile by principal, call it once for the anonymous principal, then remove the cleanup method.

Rationale for "add then remove": generic admin "delete any profile" is a footgun. One-shot endpoint, used once, removed, keeps the surface area minimal.

**Files:**
- Modify: `backend/main.mo` (near `saveCallerUserProfile`, line 89)

- [ ] **Step 1: Add the cleanup endpoint**

```motoko
// One-shot admin cleanup for the stale anonymous profile bug. Remove after use.
public shared ({ caller }) func adminDeleteProfile(user : Principal) : async () {
    assert AccessControl.isAdmin(accessControlState, caller);
    userProfiles := principalMap.delete(userProfiles, user);
};
```

- [ ] **Step 2: Deploy, delete, verify**

```bash
dfx deploy backend --network ic
dfx canister --network ic call backend adminDeleteProfile "(principal \"2vxsx-fae\")"
dfx --identity anonymous canister --network ic call backend getCallerUserProfile
# Expected: (null)
```

- [ ] **Step 3: Remove the admin endpoint from `main.mo`**

Delete the `adminDeleteProfile` function and its definition.

- [ ] **Step 4: Redeploy**

```bash
dfx deploy backend --network ic
```

- [ ] **Step 5: Commit**

```bash
git add backend/main.mo
git commit -m "fix(backend): delete stale anonymous user profile"
```

---

## Task 3: Frontend Candid declarations

Update the three Candid files in `frontend/src/declarations/backend/` to mirror the backend changes. These files are normally regenerated by `dfx generate`, but we edit them manually here to keep the build fast.

**Files:**
- Modify: `frontend/src/declarations/backend/backend.did`
- Modify: `frontend/src/declarations/backend/backend.did.js`
- Modify: `frontend/src/declarations/backend/backend.did.d.ts`

- [ ] **Step 1: Remove `whoAmI` entries from all three files**

Find and delete in each file:
- `.did`: `whoAmI: () -> (principal) query;`
- `.did.js`: `'whoAmI' : IDL.Func([], [IDL.Principal], ['query']),`
- `.did.d.ts`: `'whoAmI' : ActorMethod<[], Principal>,`

- [ ] **Step 2: Add each new `*For` query method to all three files**

Mirror the signatures in Task 1. Example for `getPonziPointsFor`:
- `.did`: `getPonziPointsFor: (user: principal) -> (float64) query;`
- `.did.js`: `'getPonziPointsFor' : IDL.Func([IDL.Principal], [IDL.Float64], ['query']),`
- `.did.d.ts`: `'getPonziPointsFor' : ActorMethod<[Principal], number>,`

Apply the same pattern for every method added in Task 1.

- [ ] **Step 3: TypeScript sanity check**

```bash
cd frontend && npx tsc --noEmit 2>&1 | tail -20
```

Expected: no errors related to backend types. Other errors may exist from Task 4 work — focus on `declarations/backend/`.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/declarations/backend/
git commit -m "feat(frontend): Candid decls for principal-parameterized queries"
```

---

## Task 4: Add `useReadActor` hook

Every query in the app will use this actor. No wallet dependency. One HttpAgent per page load.

**Files:**
- Create: `frontend/src/hooks/useReadActor.ts`

- [ ] **Step 1: Write the hook**

```typescript
import { useMemo } from 'react';
import { Actor, HttpAgent, ActorSubclass } from '@dfinity/agent';
import { idlFactory } from '../declarations/backend';
import type { _SERVICE } from '../declarations/backend';

const BACKEND_CANISTER_ID = '5zxxg-tyaaa-aaaac-qeckq-cai';
const HOST = 'https://icp0.io';

let cachedActor: ActorSubclass<_SERVICE> | null = null;

export function useReadActor(): ActorSubclass<_SERVICE> {
  return useMemo(() => {
    if (cachedActor) return cachedActor;
    const agent = new HttpAgent({ host: HOST });
    cachedActor = Actor.createActor<_SERVICE>(idlFactory, {
      agent,
      canisterId: BACKEND_CANISTER_ID,
    });
    return cachedActor;
  }, []);
}
```

No `fetchRootKey()` call — this is a mainnet-only actor. Local development uses a different setup.

- [ ] **Step 2: Commit**

```bash
git add frontend/src/hooks/useReadActor.ts
git commit -m "feat(frontend): useReadActor hook for anonymous queries"
```

---

## Task 5: Migrate queries to `useReadActor` and principal-parameterized calls

Every query in `useQueries.ts` that used `useActor` now uses `useReadActor`. Caller-scoped queries get the principal from `useWallet` and call the new `*For` backend methods.

**Files:**
- Modify: `frontend/src/hooks/useQueries.ts`

- [ ] **Step 1: Replace `useGetCallerUserProfile`**

```typescript
export function useGetCallerUserProfile() {
  const actor = useReadActor();
  const { principal } = useWallet();

  const query = useQuery<UserProfile | null>({
    queryKey: ['currentUserProfile', principal],
    queryFn: async (): Promise<UserProfile | null> => {
      if (!principal) throw new Error('No principal');
      const result = await actor.getUserProfile(Principal.fromText(principal));
      return result[0] ?? null;
    },
    enabled: !!principal,
    retry: false,
  });

  return {
    ...query,
    isLoading: query.isLoading,
    isFetched: query.isFetched,
  };
}
```

Note: no more `actorFetching` — the read actor is synchronous via `useMemo`.

- [ ] **Step 2: Mechanically migrate every other caller-scoped query**

For each of:
- `useGetPonziPoints`
- `useGetPonziPointsBalance`
- `useGetCoverChargeBalance`
- `useGetCoverChargeTransactions`
- `useGetReferralTierPoints`
- `useGetUserGames`
- `useGetDealerRepaymentBalance`
- `useGetCallerUserRole`
- `useIsCallerAdmin`

Apply this template:
```typescript
export function useFoo() {
  const actor = useReadActor();
  const { principal } = useWallet();
  return useQuery({
    queryKey: ['foo', principal],
    queryFn: async () => {
      if (!principal) throw new Error('No principal');
      return actor.getFooFor(Principal.fromText(principal));
    },
    enabled: !!principal,
    // keep existing refetchInterval / retry / select settings
  });
}
```

- [ ] **Step 3: Migrate non-caller queries too**

Queries like `useGetGameStats`, `useGetAllDealerRepayments`, `useGetPlatformStats`, etc. — these already took no caller argument, but they still went through `useActor`. Switch them to `useReadActor` and drop the `actorFetching` gating.

- [ ] **Step 4: Type check**

```bash
cd frontend && npx tsc --noEmit 2>&1 | tail -30
```

Expected: zero errors. If a hook's `enabled` condition changed shape (e.g., no more `!!actor`), update callers that destructured `isLoading` or `isFetched` from the old return.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/hooks/useQueries.ts
git commit -m "refactor(frontend): queries use anonymous read actor with principal arg"
```

---

## Task 6: Clean up WalletDropdown diagnostic

Remove the `whoAmI` + `__diagActor` + `useActor()` diagnostic block.

**Files:**
- Modify: `frontend/src/components/WalletDropdown.tsx`

- [ ] **Step 1: Delete the diagnostic**

Remove these lines (exact text):
```typescript
import { useActor } from '../hooks/useActor';
```
And:
```typescript
  // DIAGNOSTIC: expose whoAmI + displayed principal to devtools.
  // Remove once principal-mismatch investigation concludes.
  const { actor: __diagActor } = useActor();
  useEffect(() => {
    if (!__diagActor) return;
    (async () => {
      try {
        const who = await __diagActor.whoAmI();
        (window as any).__whoAmI = { /* ... */ };
        console.log('[whoAmI]', (window as any).__whoAmI);
      } catch (e) {
        console.error('[whoAmI] failed', e);
      }
    })();
  }, [__diagActor, principal, walletType]);
```

- [ ] **Step 2: Remove console.logs from `useActor.ts`**

In `frontend/src/hooks/useActor.ts`, delete:
```typescript
console.log('[useActor] Creating Oisy SignerAgent for', principal);
// and
console.log('[useActor] SignerAgent created, getPrincipal:', /* ... */);
// and
(window as any).__oisyActor = newActor;
// and
console.log('[useActor] Falling through to HttpAgent path. walletType=', /* ... */);
```

- [ ] **Step 3: Verify TypeScript**

```bash
cd frontend && npx tsc --noEmit 2>&1 | tail -10
```

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/WalletDropdown.tsx frontend/src/hooks/useActor.ts
git commit -m "chore(frontend): remove whoAmI diagnostic"
```

---

## Task 7: Add Oisy restore path to `useWallet`

On page load, if `savedWalletType === 'oisy'`, try to rehydrate the Oisy session. The `@slide-computer/signer` package maintains session state inside its transport — re-creating the signer + calling `accounts()` during an already-authorized 7-day window succeeds without a popup.

Important: `accounts()` requires the signer channel. Per the slide-computer docs, the channel can only be opened in a click handler... EXCEPT when restoring an established session, the transport already has a handle. Test this behavior first before committing.

**Files:**
- Modify: `frontend/src/hooks/useWallet.tsx`
- Modify: `frontend/src/lib/oisySigner.ts`

- [ ] **Step 1: Add a `hasActiveSession()` helper to oisySigner**

In `frontend/src/lib/oisySigner.ts`, append:

```typescript
export async function restoreOisySession(): Promise<Principal | null> {
  // accounts() will throw if the signer session has expired AND we're not
  // inside a click handler. We try once — if it throws, return null and the
  // user sees the signin button instead of a broken state.
  try {
    const accounts = await oisySigner.accounts();
    if (!accounts || accounts.length === 0) return null;
    return accounts[0].owner;
  } catch {
    return null;
  }
}
```

- [ ] **Step 2: Add the restore call in `initializeWallet`**

In `frontend/src/hooks/useWallet.tsx`, extend the switch:

```typescript
if (savedWalletType === 'plug') {
  await restorePlugConnection();
} else if (savedWalletType === 'internet-identity') {
  // ... existing II restore ...
} else if (savedWalletType === 'oisy') {
  const { restoreOisySession } = await import('../lib/oisySigner');
  const owner = await restoreOisySession();
  if (owner) {
    setPrincipal(owner.toText());
    setWalletType('oisy');
  } else {
    localStorage.removeItem('musical-chairs-wallet-type');
  }
}
```

- [ ] **Step 3: Manual verification**

```bash
# Build + deploy + reload the browser tab
cd frontend && npm run build && cd .. && dfx deploy frontend --network ic
```

Then in the browser:
1. Sign in with Oisy. Confirm "Welcome, andy" shows with zegjz principal.
2. Reload the page. Observe that `walletType` is `oisy` on page load and the dropdown still says "Welcome, andy" with zegjz principal.
3. If Oisy session expired (after 7 days), reload should silently downgrade to signed-out instead of error-looping.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/hooks/useWallet.tsx frontend/src/lib/oisySigner.ts
git commit -m "feat(frontend): restore Oisy session on page reload"
```

---

## Task 8: Wire Oisy writes with ICRC-112 batching

Updates (state-changing calls) are the only calls that need Oisy signing. Each happens inside a click handler, so the signer popup opens correctly. Multi-step writes (deposit = `icrc2_approve` + `createGame`) use ICRC-112 batching so the user sees one popup instead of two.

**Files:**
- Modify: `frontend/src/hooks/useActor.ts`
- Modify: `frontend/src/hooks/useQueries.ts` (mutations only — queries already done in Task 5)
- Modify: `frontend/src/components/GamePlans.tsx` or wherever `createGame`/`icrc2_approve` are called together (find via grep).

- [ ] **Step 1: Find the approve+call flows**

```bash
grep -rn "icrc2_approve" frontend/src --include="*.ts" --include="*.tsx"
```

For each location, the approve is currently followed by a separate mutation call — this is the two-popup path. Check every one.

- [ ] **Step 2: For Oisy, wrap the pair in an ICRC-112 batch**

Pattern (inside the click handler, no awaits above it except inputs that come from the click itself):

```typescript
import { useWallet } from './useWallet';

async function depositOisy(amount: bigint, plan: GamePlan) {
  const signerAgent = await getOisySignerAgent(Principal.fromText(principal));
  // begin sequence 0
  signerAgent.batch();
  const approvePromise = ledgerActor.icrc2_approve({
    amount,
    spender: { owner: Principal.fromText(BACKEND_CANISTER_ID), subaccount: [] },
    expires_at: [], expected_allowance: [], memo: [], fee: [],
    from_subaccount: [], created_at_time: [],
  });
  // begin sequence 1
  signerAgent.batch();
  const depositPromise = backendActor.createGame(plan, Number(amount) / 1e8, isCompounding, referrer);
  // fire
  await signerAgent.execute();
  return Promise.all([approvePromise, depositPromise]);
}
```

For Plug / II, keep the existing separate-call path.

- [ ] **Step 3: Keep single-update flows simple**

Actions like save display name (`saveCallerUserProfile`) or withdraw (`withdrawEarnings`) are one call each. No batching needed — the click-handler invariant is enough.

- [ ] **Step 4: Verify — with preview running**

1. Sign in with Oisy.
2. Deposit 0.01 ICP into a simple21Day plan.
3. Expected: one Oisy popup showing both the approve and the createGame consent, user approves, both calls succeed.
4. Verify game record: `dfx canister --network ic call backend getAllGames | grep zegjz`.
5. Expected: a record with `player = principal "zegjz-..."`.

- [ ] **Step 5: Commit**

```bash
git add frontend/
git commit -m "feat(frontend): ICRC-112 batching for Oisy multi-call updates"
```

---

## Task 9: Save flow and end-to-end verification

- [ ] **Step 1: From Andy's Oisy session, set display name to "Andy"**

1. Sign in with Oisy.
2. Open wallet dropdown, click pencil next to the name.
3. Type "Andy", hit save. Oisy popup appears (this is a click-triggered update), approve.
4. Dropdown refreshes showing "Welcome, Andy".

- [ ] **Step 2: Verify backend state**

```bash
dfx canister --network ic call backend getUserProfile "(principal \"zegjz-jpi6k-qkand-c2bgf-qw6za-xk4si-nz3gx-qzzia-fk6fg-snepb-tae\")"
# Expected: (opt record { name = "Andy" })

dfx --identity anonymous canister --network ic call backend getCallerUserProfile
# Expected: (null)  — the stale anonymous profile stays deleted.
```

- [ ] **Step 3: Reload the tab. Confirm "Welcome, Andy" persists without requiring sign-in.**

- [ ] **Step 4: Open in an incognito window (unauthenticated). Confirm no profile name appears / login prompt shows. Not "Welcome, andy" to a stranger.**

- [ ] **Step 5: Commit any cleanup from verification**

```bash
git status
# If anything changed during verification, add + commit with a "chore: verification notes" style message.
# Otherwise skip.
```

---

## Self-Review Notes

**Spec coverage:**
- Queries no longer require signer popups → Task 4, 5.
- Andy's real principal (zegjz) can make backend writes → Task 8.
- Anonymous "andy" profile deleted → Task 2.
- Oisy survives reload → Task 7.
- Diagnostic whoAmI cleanup → Tasks 3, 6.
- No backend deploys without explicit permission — this plan requires two (Tasks 1+2 bundled, and Task 2 removal). Both are flagged before deploy steps.

**Types:**
- Every backend `*For` method returns the same type as its caller counterpart.
- Frontend `useQuery` keys include `principal` so cache invalidates on wallet switch.

**Known risks:**
- Task 7 assumes `oisySigner.accounts()` works without a click on an established session. Docs are ambiguous. If Step 3 verification fails, the fallback is to always require the user to click "Sign in with Oisy" even on reload — not worse than today.
- ICRC-112 batch in Task 8 requires the signer to support ICRC-112. v4 does — verified via `agent.js` source inspection during investigation.
