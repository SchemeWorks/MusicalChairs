# `ponzi_math` Canister Extraction — Design Spec

**Date:** 2026-05-11
**Status:** Design — implementation pending review.

## Goal

Extract all money-flow logic from the `backend` canister into a new dedicated
`ponzi_math` canister whose Candid surface is the entirety of "the rules". The
new canister is designed to be eventually blackholed (controller removed,
immutable forever). `backend` remains mutable for profiles, access control,
admin tooling, and downstream pay-out distribution.

This is an extraction, not a state migration. The currently-deployed `backend`
canister's Ponzi state will be wound down (withdraws/claims/sweep) and
`ponzi_math` starts from zero. No programmatic state transfer.

## Architecture summary

```
┌────────────────────────────────────────────────────────────────────────┐
│  USER                                                                  │
└──────┬─────────────────────────────────────┬───────────────────────────┘
       │ deposit, withdraw, settle,          │ profile, login
       │ addBackerMoney, claimBackerRepayment│
       │ (financial)                         │ (non-financial)
       ↓                                     ↓
┌──────────────────┐                ┌──────────────────┐
│   ponzi_math     │                │     backend      │
│  (blackholable)  │                │    (mutable)     │
│                  │                │                  │
│ - games          │                │ - profiles       │
│ - backers        │                │ - access control │
│ - pot            │  cover-charge  │ - admin tooling  │
│ - seed reserve   │  sweep (gated  │ - payManagement  │
│ - exit toll      │  on backend    │   to admin       │
│ - cover charge   │  principal)    │   wallet         │
│   (accrual)      │ ─────────────→ │                  │
│ - generalLedger  │                │                  │
└──────┬───────────┘                └──────────────────┘
       │ getAllGames (poll, query)
       │ getBackerPositions (poll, query)
       ↓
┌──────────────────┐
│   shenanigans    │
│ (PP economy)     │
│                  │
│ - PP mint        │
│ - referralChain  │
│   (MOVED FROM    │
│   backend)       │
│ - cashOut queue  │
│ - spell casts    │
└──────────────────┘
```

Money never enters or leaves shenanigans. Math never enters or leaves
shenanigans. Shenanigans is purely PP-economy, cosmetic, and observational.

## What moves to `ponzi_math`

All of the following code, currently in `backend/main.mo`:

### Public methods (renamed where noted)

**Game lifecycle:**
- `createGame(plan, amount, isCompounding)` — drops the `referrer` parameter (moved to shenanigans).
- `withdrawEarnings(gameId)`
- `settleCompoundingGame(gameId)`

**Backer (was Dealer) system:**
- `addBackerMoney(amount)` (was `addDealerMoney`)
- `claimBackerRepayment()` (was `claimDealerRepayment`)
- `getBackerPositions()` (was `getDealerPositions`)
- `getAllBackerRepayments()` (was `getAllDealerRepayments`)
- `getBackerRepaymentBalance()` / `getBackerRepaymentBalanceFor(user)` (was `getDealerRepaymentBalance` / `getDealerRepaymentBalanceFor`)
- `getTotalBackerDebt()` (was `getTotalDealerDebt`)
- `getOldestSeriesABacker()` (was `getOldestUpstreamDealer`)
- `distributeFees(totalFees)` — **DROPPED**. Admin-only manual variant, no longer needed.

**Calculations (signatures unchanged):**
- `calculateEarnings(game : GameRecord)`
- `calculateCompoundedEarnings(game : GameRecord)`
- `calculateCompounded30DayEarnings(game : GameRecord)`
- `calculateCompoundedROI()`

**Platform state getters:**
- `getPlatformStats()`
- `getAllGames()`, `getAllActiveGames()`, `getActiveGameCount()`
- `getUserGames()`, `getUserGamesFor(user)`, `getGameById(gameId)`
- `getAvailableBalance()`, `getMaxDepositLimit()`
- `getTotalDeposits()`, `getTotalWithdrawals()`, `getDaysActive()`
- `getRoundSeedReserve()`
- `getGameResetHistory()`
- `checkDepositRateLimit()`
- `getCanisterICPBalance()` — was admin-only, becomes **public** (no admin role exists on ponzi_math). Still a `shared` (update) call rather than a query because it awaits `icpLedger.icrc1_balance_of`.

**Cover charge (accrual only, no pay-out here):**
- `getCoverChargeBalance() : Nat` — public, no auth, returns accumulated e8s not yet swept.
- `sweepCoverCharges() : Result<Nat, Text>` — gated on `caller == backendPrincipal`. Transfers full accumulated balance to backend canister principal. Returns block index on success.
- `withdrawCoverCharges` — **DROPPED**. Pay-out moves to backend.
- `getCoverChargeTransactions` — **DROPPED**. Cover-charge accruals are now captured as `#coverChargeAccrued` events in the general ledger; the dedicated transactions map is redundant.

**Concurrency / diagnostics:**
- `isCriticalSectionBusy() : Bool` — public query.

**General Ledger (new — replaces dead `houseLedger`):**
- `getGeneralLedger() : [GeneralLedgerEntry]`
- `getGeneralLedgerStats() : { totalInflows : Float; totalOutflows : Float; netFlow : Float; entryCount : Nat }`

### Internal helpers (private, moved verbatim where noted)

- `distributeExitToll(tollAmount)` — uses `#seriesA` filter (was `#upstream`).
- `calculateExitToll(game, earnings)`
- `creditBackerRepayment(backer, amount)` — already correctly named.
- `triggerGameReset(reason)` — appends a `#gameReset` ledger entry.
- `acquireCallerLock` / `releaseCallerLock` / `acquireGlobalLock` / `releaseGlobalLock`
- `recordLedger(event)` — new internal helper. Called at every money-flow event
  (deposit, withdrawal, settlement, toll distribution, backer claim,
  cover-charge accrual, sweep, reset, test backdated-game create).

### State (all `var`, all stable by default since no `transient` keyword)

```motoko
var gameRecords        : Map<Nat, GameRecord>
var nextGameId         : Nat
var platformStats      : PlatformStats
var gameResetHistory   : Map<Int, GameResetRecord>
var roundSeedReserve   : Float
var depositTimestamps  : Map<Principal, List<Int>>     // rate limit
var backerPositions    : Map<Principal, BackerPosition> // was dealerPositions
var backerRepayments   : Map<Principal, Float>          // was dealerRepayments
var coverChargeBalance : Nat                            // e8s, fully segregated from pot
var generalLedger      : Map<Nat, GeneralLedgerEntry>   // new, replaces houseLedger
var nextGeneralLedgerId : Nat

// transient (resets on upgrade — safe by construction)
transient var callerLocks       : Map<Principal, Bool>
transient var globalCriticalLock : Bool
```

### Types (public Candid surface)

```motoko
public type GamePlan = {
    #simple21Day;
    #compounding15Day;
    #compounding30Day;
};

public type GameRecord = {
    id : Nat;
    player : Principal;
    plan : GamePlan;
    amount : Float;
    startTime : Int;
    isCompounding : Bool;
    isActive : Bool;
    lastUpdateTime : Int;
    accumulatedEarnings : Float;
    totalWithdrawn : Float;
};

public type PlatformStats = {
    totalDeposits : Float;
    totalWithdrawals : Float;
    activeGames : Nat;
    potBalance : Float;
    daysActive : Nat;
};

public type GameResetRecord = {
    resetTime : Int;
    reason : Text;
};

public type BackerType = {
    #seriesA;   // gets 24% bonus, paid first
    #seriesB;   // not yet wired into addBackerMoney; reserved
};

public type BackerPosition = {
    owner : Principal;
    amount : Float;
    entitlement : Float;
    startTime : Int;
    isActive : Bool;
    backerType : BackerType;
    firstDepositDate : ?Int;
    // NB: no `name` field. Frontend joins on `owner` against backend profile.
};

public type GeneralLedgerEntry = {
    id : Nat;
    timestamp : Int;
    event : GeneralLedgerEvent;
};

public type GeneralLedgerEvent = {
    #deposit : {
        player : Principal;
        gameId : Nat;
        gross : Float;
        coverCharge : Float;
        netToPot : Float;
        plan : GamePlan;
        isCompounding : Bool;
    };
    #backerDeposit : {
        backer : Principal;
        amount : Float;
        entitlement : Float;
    };
    #withdrawal : {
        player : Principal;
        gameId : Nat;
        grossEarnings : Float;
        toll : Float;
        netToPlayer : Float;
        potDeduction : Float;
        isInsolvent : Bool;
    };
    #settlement : {
        player : Principal;
        gameId : Nat;
        grossEarnings : Float;
        toll : Float;
        netToPlayer : Float;
        potDeduction : Float;
        isInsolvent : Bool;
    };
    #tollDistribution : {
        tollAmount : Float;
        toSeedReserve : Float;
        toOldestSeriesA : Float;
        toOtherSeriesA : Float;
        toAllBackers : Float;
    };
    #backerRepaymentClaim : {
        backer : Principal;
        amount : Float;
    };
    #coverChargeAccrued : {
        gameId : Nat;
        player : Principal;
        amountE8s : Nat;
    };
    #coverChargeSwept : {
        amountE8s : Nat;
        toBackend : Principal;
        blockIndex : Nat;
    };
    #gameReset : {
        reason : Text;
        seedReserveCarried : Float;
    };
    #backdatedGameCreated : {     // TEST HATCH — removed pre-blackhole
        admin : Principal;
        player : Principal;
        gameId : Nat;
        startTime : Int;
        amount : Float;
    };
};
```

### Init args (immutable post-install)

```motoko
persistent actor (initArgs : {
    backendPrincipal : Principal;   // gates sweepCoverCharges, sweep destination
    testAdmin : Principal;          // gates createBackdatedGame test hatch
}) Self { ... }
```

### Test hatch block (clearly marked, removed before blackhole)

```motoko
// ========================================================================
// PRE-BLACKHOLE TEST HATCH — DELETE THIS ENTIRE BLOCK BEFORE BLACKHOLING.
// Gated on caller == initArgs.testAdmin.
// ========================================================================
public shared ({ caller }) func createBackdatedGame(
    plan : GamePlan,
    amount : Float,
    isCompounding : Bool,
    startTimeNanos : Int,
) : async { #Ok : Nat; #Err : Text }
```

Same flow as `createGame` (icrc2_transfer_from from caller, real ICP), but:
- Gated on `caller == initArgs.testAdmin`.
- `startTime` is caller-specified instead of `Time.now()`.
- Skips deposit-rate-limit and max-deposit checks.
- Caller is the player (no separate `player` argument).
- Emits a `#backdatedGameCreated` general-ledger entry so test activity is auditable.

### File structure

```
ponzi_math/
  main.mo        — actor entry, types, all public methods, state, internal helpers
  ledger.mo      — ICP ledger interface (copy of backend/ledger.mo)
  icrc21.mo      — ICRC-21/28/10 consent messages for financial methods only
```

`main.mo` is single-file. Backend's main.mo is 1752 lines; ponzi_math's will be
smaller (~1100 lines) since profile, access-control, and dropped admin code are
gone. Refactor into modules later if it grows past ~1500.

## What stays in `backend`

### Existing, retained:

- User profiles (`UserProfile`, `getCallerUserProfile`, `getUserProfile`, `saveCallerUserProfile`)
- Access control (`initializeAccessControl`, `assignCallerUserRole`, `getCallerUserRole`, `getUserRole`, `isCallerAdmin`, `isAdmin`)
- ICRC-21 consent messages (for profile methods only — most consent messages move to ponzi_math)
- ICRC-28 trusted origins
- ICRC-10 supported standards
- `authorization/access-control.mo` module

### New, added:

```motoko
// ICP ledger reference (backend can now also hold ICP, from cover-charge sweeps)
transient let icpLedger : Ledger.LedgerActor = actor(Ledger.ICP_LEDGER_CANISTER_ID);

// ponzi_math canister reference, set once at cutover by admin.
stable var ponziMathPrincipal : ?Principal = null;

public shared ({ caller }) func setPonziMathPrincipal(p : Principal) : async () {
    // Admin only (via AccessControl). No first-caller-wins escape — backend
    // already has an admin role from the original deploy.
};

public query func getPonziMathPrincipal() : async ?Principal;

// Pay management — sweep + transfer in one admin op.
public shared ({ caller }) func payManagement(
    to : Principal,
    amount : Nat,    // e8s; must exceed transfer fee
) : async { #Ok : Nat; #Err : Text } {
    // Admin only.
    // 1. Call ponzi_math.sweepCoverCharges() — pulls accumulated charges to backend.
    // 2. Transfer `amount` from backend's ICP balance to `to`.
    // 3. Remainder stays on backend for next call.
};

// Diagnostic — how much ICP backend is holding (post-sweep, pre-distribution).
public shared ({ caller }) func getBackendICPBalance() : async Nat;
// Admin only.
```

### Removed entirely from backend:

All money-flow code listed under "What moves" above. Plus:
- `migrateReconcilePot` — one-shot recovery, irrelevant to fresh ponzi_math.
- `adminSweep` — replaced by ponzi_math's pre-blackhole test hatch model.
- `setTestMode` / `isTestMode` — fake-ICP testing no longer used.
- `seedGame` / `seedReferral` — superseded by `createBackdatedGame` on ponzi_math; referrals move to shenanigans.
- `setCanisterPrincipal` / `getCanisterPrincipal` — backend uses `Principal.fromActor(Self)` like ponzi_math.
- `houseLedger`, `nextHouseLedgerId`, `HouseLedgerRecord`, `getHouseLedger`, `getHouseLedgerStats`, `getTotalHouseMoneyAdded` — dead state; replaced by `generalLedger` on ponzi_math.
- `referralChain`, `registerReferral`, `referralRecords` (deprecated), `getReferrer` — referrals move to shenanigans.

## What changes in `shenanigans`

### Repoint observer to `ponzi_math`

`shenanigans.initialize(backendCanisterId)` becomes `shenanigans.initialize(ponziMathCanisterId)`. The observer polls `ponzi_math.getAllGames()` and `ponzi_math.getBackerPositions()` instead of backend's equivalents.

Type renames in shenanigans's hand-rolled actor type:
- `BackendActor` → `PonziMathActor`
- `BackendGamePlan` → `PonziMathGamePlan`
- `BackendGameRecord` → `PonziMathGameRecord`
- `BackendDealerType` → `PonziMathBackerType` (variants: `#seriesA`, `#seriesB`)
- `BackendDealerPosition` → `PonziMathBackerPosition` (`backerType` field, no `name` field)
- `getDealerPositions` → `getBackerPositions`
- The `getReferrer` reference is **removed** (referrals now live locally).

Variable renames in shenanigans internal state:
- `dealerSeen : Map<Principal, Float>` → `backerSeen`
- `dealerPpPerIcp : Nat` → `backerPpPerIcp` (in MintConfig)
- `dealerCut : Float` (in ShenaniganStats) → leave alone for UI continuity; comment marks it deprecated.
- `processDealerDeltas()` → `processBackerDeltas()`

### Referral chain moves here (new state + new method)

```motoko
var referralChain : Map<Principal, Principal> = ... // user → who referred them, first-wins

public shared ({ caller }) func registerReferral(referrer : Principal) : async () {
    requireAuthenticated(caller);
    if (caller == referrer) return;
    // First registration wins; subsequent calls are no-ops.
    switch (Map.get(referralChain, caller)) {
        case (?_) { /* already set */ };
        case null { referralChain := Map.put(referralChain, caller, referrer); };
    };
};

public query func getReferrer(user : Principal) : async ?Principal;
```

The PP cascade (`cascadeReferralMint`) inside the observer changes from
`await backend.getReferrer(user)` to a local `Map.get(referralChain, user)`.
No cross-canister call needed.

### TODO marker (deferred — separate cleanup pass)

```motoko
// TODO(2026-05-11): Rename "chips" terminology — chips, depositChips,
// claimCashOut, chip subaccount — to non-casino verbiage (e.g. credits,
// PP units, tokens). Deferred from the ponzi_math extraction migration
// to keep that scope tight. See docs/superpowers/specs/2026-05-11-ponzi-math-extraction-design.md.
```

## What changes in `frontend`

### Add `ponzi_math` actor + declarations

```
frontend/src/declarations/ponzi_math/    # autogenerated from .did
frontend/src/backend.ts                  # additionally exports ponziMath idl
frontend/src/lib/actors.ts (new)         # builds both actors, exports useBackendActor + usePonziMathActor hooks
```

### Repoint financial calls

`frontend/src/hooks/useQueries.ts` — hooks that call financial methods (create
game, withdraw, settle, addBackerMoney, claimBackerRepayment, getPlatformStats,
getAllGames, getBackerPositions, getAllBackerRepayments, getDealerRepaymentBalance*,
getMaxDepositLimit, checkDepositRateLimit, all calculate* hooks, getRoundSeedReserve,
getCoverChargeBalance, getGameResetHistory, etc.) switch from `actor` to `ponziMathActor`.

Hooks that stay on backend: profile, access control, payManagement (new),
icrc21/28/10 standards stubs.

### Type renames

- `DealerType` → `BackerType` (variants `seriesA` / `seriesB`)
- `DealerPosition` → `BackerPosition` (`dealerType` → `backerType`, **`name` field removed**)
- `HouseLedgerRecord` → `GeneralLedgerEntry` + `GeneralLedgerEvent` variant
- `useHouseLedger` hook → `useGeneralLedger`

Components that rendered the backer's `name` from the position now do a
profile join: read `name` from `useUserProfile(position.owner)` instead.

### New referral wiring

Frontend already captures `?ref=<principal>` from URLs into localStorage.
Currently it passes the referrer to `backend.createGame(referrer)`. After the
split:

- On user's first authenticated load (or on first deposit), call
  `shenanigans.registerReferral(referrerFromLocalStorage)`. Idempotent — safe
  to call repeatedly; only the first call lands.
- `ponzi_math.createGame` no longer accepts a `referrer` argument.

### Deprecated cover-charge admin UI

The admin wallet page currently calls `backend.getCoverChargeBalance` and
`backend.withdrawCoverCharges`. After the split:

- `ponzi_math.getCoverChargeBalance` returns the e8s accrued and not-yet-swept on
  ponzi_math (visible to all; previously admin-only).
- `backend.getBackendICPBalance` returns the e8s sitting on backend, ready to
  pay out (admin-only).
- `backend.payManagement(to, amount)` replaces `withdrawCoverCharges`.

Admin UI rebinds to these new methods. Single screen showing both balances +
"Pay" form.

## Coordination plan

### Deploy order (for cutover)

1. **Deploy `ponzi_math` to mainnet** with init args. Both principal IDs need
   confirmation before mainnet deploy — `backendPrincipal` is the existing
   backend canister; `testAdmin` is whichever principal will be used for
   `createBackdatedGame` test calls:
   ```
   dfx deploy ponzi_math --network ic --argument '(record {
       backendPrincipal = principal "5zxxg-tyaaa-aaaac-qeckq-cai";   // ← confirm
       testAdmin        = principal "ft3ml-xex6k-...latf4-aae";       // ← confirm
   })'
   ```
2. **Upgrade `backend`**: removes money-flow code, adds `payManagement` and
   `setPonziMathPrincipal`. Profile and access-control state preserved.
3. **Set `backend.ponziMathPrincipal`** to the newly-deployed `ponzi_math`
   canister ID.
4. **Upgrade `shenanigans`**: repoint observer types, add referralChain state,
   add `registerReferral` method. Observer continues running against backend
   until step 5.
5. **Call `shenanigans.initialize(ponziMathPrincipal)`** to switch the
   observer's polled canister.
6. **Build and deploy frontend** with both actors, repointed hooks.

Step 5 is the cutover. Between steps 1-4 and step 5, the old backend's Ponzi
state is still running and the observer is still watching it. Once step 5
flips, new games happen on ponzi_math (which is empty); old games stay open
on backend until manually wound down.

### Cutover and wind-down (operational, outside this spec)

Wind-down of existing open games on the old backend is a separate operational
decision after ponzi_math is live and proven. Open questions deferred:

- Whether to drop the financial-method Candid surface on backend immediately
  (forcing all existing-game holders to withdraw against a previous canister
  version), or leave it as orphaned dead code until games close naturally.
- How the frontend distinguishes pre-cutover games (old backend) from
  post-cutover games (ponzi_math) in the UI.
- Final `adminSweep` of residual ICP from old backend after wind-down.

Out of scope for this design — handled when ponzi_math is stable.

### Test rehearsal before mainnet

Full deploy + smoke test on a local replica:
- Deploy backend + ponzi_math + shenanigans + pp_ledger locally.
- Run through createGame → withdraw → settle, addBackerMoney →
  claimBackerRepayment, sweepCoverCharges via payManagement,
  createBackdatedGame → settle, triggerGameReset (via insolvency forcing).
- Verify general-ledger entries for every flow.
- Verify shenanigans observer mints PP correctly against ponzi_math.

## Naming conventions (reaffirmed)

Per `CLAUDE.md` and decisions in this design:

| User-facing       | Internal identifier (unchanged) |
|-------------------|---------------------------------|
| Carried Interest  | `exitToll`, `EXIT_TOLL_*`       |
| Front-End Load    | `coverCharge`, `COVER_CHARGE_*` |
| Jackpot Fee       | `exitToll` (compounding path)   |

New renames in this migration:

| Old (casino)      | New                             |
|-------------------|---------------------------------|
| Dealer            | Backer                          |
| House Ledger      | General Ledger                  |
| upstream          | seriesA                         |
| downstream        | seriesB                         |

Untouched (low value, high churn):
- `game*`, `gameRecords`, `getAllGames`
- `pot*`, `potBalance`
- `round*`, `roundSeedReserve`

Deferred (separate cleanup):
- shenanigans `chip*` terminology

## Explicitly NOT doing

- State migration from old backend → ponzi_math. Fresh start.
- Closing existing open games on the old backend automatically (game-0 etc.).
- Frontend UI behavior for pre-cutover games — deferred to operational
  decisions after ponzi_math is live.
- Removing the backend's financial-method Candid surface in the same deploy
  as cutover. Leave as orphaned dead code; remove in a follow-up cleanup
  once the old games are fully closed.
- Stable variable upgrade choreography on ponzi_math — initial deploy is
  fresh state, no prior storage to migrate. Future ponzi_math upgrades use
  regular Motoko `persistent actor` upgrade rules; if a future field rename
  or restructure is needed, the `migrating-motoko-enhanced` skill applies.

## Deferred / open items

- **Chips rename in shenanigans** — separate scope, TODO marker added.
- **Removing pre-blackhole test hatch** — `createBackdatedGame` and
  `testAdmin` init arg deleted via a code-rm commit before blackhole.
- **General-ledger pagination** — current design returns the full ledger
  array. Add `getGeneralLedger(offset, limit)` if entry count grows past
  a few thousand. Not blocking initial deploy.
- **Old backend cleanup deploy** — final removal of orphaned financial
  Candid methods after all pre-cutover games close.
- **Backend storing ICP** — backend currently never holds ICP. After this
  migration it holds cover-charge sweep funds between sweep and payout.
  This means backend's controller has the ability to drain it via canister
  reinstall. Acceptable for the current trust model (admin is trusted for
  pay-out anyway); revisit if trust model changes.

## Out-of-scope decisions explicitly settled

These were discussed and locked during brainstorming:

- **`DealerPosition.name` field**: dropped. Frontend joins on `owner`.
- **`Principal.fromActor(Self)`** used instead of `setCanisterPrincipal`
  pattern. No setter, no state, no admin gate needed.
- **Calculate function signatures**: `calculateEarnings(game : GameRecord)`
  shape preserved. No frontend churn.
- **Single-file `main.mo`** for ponzi_math, not pre-emptively split into
  modules. Refactor later if size warrants.
- **Sweep trigger**: gated on `caller == backendPrincipal`. Spam-safe.
  Combined into `backend.payManagement` so admin sees one method.
- **Cover-charge recipient stored state**: none on backend. Admin picks
  recipient on each `payManagement` call.
