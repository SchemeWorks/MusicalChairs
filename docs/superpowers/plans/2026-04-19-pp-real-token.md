# Ponzi Points as a Real ICRC-1 Token — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `pp_ledger` the system of record for Ponzi Points, move all PP logic (mint / custody / spells / leaderboard / tunables) into `shenanigans`, and strip PP knowledge from `backend` so it can eventually be blackholed.

**Architecture:** Three canisters with clean responsibilities. `backend` keeps ICP + ponzi math and becomes ignorant of PP. `pp_ledger` (re-initialized with decimals = 8 and shenanigans as minting account) is the only source of truth for PP balances. `shenanigans` (mutable) runs an observer timer against backend queries, mints PP via `icrc1_transfer` from its default subaccount, holds all in-game chips under deterministic per-player subaccounts, handles deposit/cash-out and spells as ledger transfers, and tracks the leaderboard + admin tunables locally. Existing mainnet canister IDs are preserved (shenanigans is upgraded, pp_ledger is reinstalled, backend is upgraded).

**Tech Stack:** Motoko (`mo:base`), dfx + dfx.json, React/TypeScript frontend with `@dfinity/agent`, ICRC-1/ICRC-2 ledger from `ledger-suite-icrc-2025-09-01`.

---

## Units & Conversions (reference — used throughout)

- `PP_DECIMALS = 8`. One whole PP = `100_000_000` ledger units ("PP-e8s").
- Backend stores ICP amounts as `Float` (e.g. `1.0` = 1 ICP = `100_000_000` ICP-e8s).
- **Mint conversion:** `pp_units = icp_float * rate_pp_per_icp * 10^8`. Example: deposit of `1.0` ICP on Simple plan (1000 PP/ICP) → `1.0 * 1000 * 10^8 = 100_000_000_000` PP-units (= 1000 whole PP).
- **MIN_DEPOSIT** in whole PP: 5,000 → `500_000_000_000` PP-units.
- **Referral cuts** stay percentages: L1 = 8%, L2 = 5%, L3 = 2%.
- **CASH_OUT_DELAY_SECONDS** = 604,800 (7 days).
- **Observer poll interval** = 10 seconds.

Mints (from `(shenanigans, null)`) and burns (to `(shenanigans, null)`) MUST set `fee = null` (see ICRC pitfall #10). Transfers between chip subaccounts use `fee = ?0` (the configured `transfer_fee`).

---

## File Structure

### New files
| File | Responsibility |
|---|---|
| `shenanigans/PpLedger.mo` | ICRC-1/ICRC-2 types + `actor` reference for `pp_ledger`. Mirrors the structure of `backend/ledger.mo`. |
| `shenanigans/Subaccount.mo` | Deterministic `principalToChipSubaccount(p : Principal) : Blob` (32-byte zero-padded principal bytes). Default subaccount helper. |
| `frontend/src/declarations/pp_ledger/` | Auto-generated candid bindings for pp_ledger (via `dfx generate pp_ledger`). |
| `frontend/src/hooks/usePpLedger.ts` | Actor hook for pp_ledger — balance reads, icrc2_approve from the user's wallet. |
| `frontend/src/components/ChipWallet.tsx` | UI for wallet PP balance, chip PP balance, deposit/approve, cash-out request + claim, pending queue list. |

### Modified files
| File | Changes |
|---|---|
| `dfx.json` | Rewrite `pp_ledger.init_arg`: `decimals = opt (8 : nat8)`, `minting_account = shenanigans principal`, drop the 1M genesis (owner can be re-minted manually post-reinstall). |
| `shenanigans/main.mo` | Add mint config + cash-out queue + leaderboard + observer cursors. Timer + `observerTick`. `depositChips`, `requestCashOut`, `claimCashOut`. Rewrite `castShenanigan` to use ppLedger transfers. Admin getters/setters for every tunable. Drop obsolete stable state (`shenanigans`, `shenaniganStats`, `nextShenaniganId`). |
| `backend/main.mo` | Delete `ponziPoints`, `ponziPointsBurned`, `referralEarnings`, `shenanigansPrincipal` state. Delete every public PP method (list in Task 28). Delete `awardPonziPoints`, `creditPonziPointsDirect`, `awardReferralPP`, `creditReferralEarnings`, `updateDealerCut`. Delete `distributeDealerCutFromShenanigans`, `deductPonziPoints`, `transferPonziPoints`, `burnPonziPoints`, `getPonziPointsBalanceFor`, `setShenanigansPrincipal`. Add public query `getReferrer(user : Principal) : async ?Principal`. Stop calling `awardPonziPoints` inside `createGame` and `addDealerMoney`. Keep `registerReferral` and `referralChain`. Delete `ReferralEarnings` type. Delete `getReferralEarnings`, `getReferralTierPoints*`, `getPonziPointsBalance`, `getPonziPointsBreakdownFor`. |
| `frontend/src/declarations/shenanigans/` | Regenerated via `dfx generate shenanigans`. |
| `frontend/src/declarations/backend/` | Regenerated. |
| `frontend/src/hooks/useQueries.ts` | Replace `useGetPonziPoints` with chip-balance + wallet-balance queries against pp_ledger. Replace `useGetTopPonziPointsHolders` with deletion (leaderboard spec removes holders). Replace `useGetTopPonziPointsBurners` to hit new shenanigans leaderboard query. Add `useDepositChips`, `useRequestCashOut`, `useClaimCashOut`, `usePendingCashOuts`. Invalidate queries across pp_ledger + shenanigans. |
| `frontend/src/components/PonziPointsDashboard.tsx` | Show chip balance (spendable) + wallet balance (external) + "Bring chips to the table" / "Cash out" CTAs linking to new `ChipWallet`. |
| `frontend/src/components/HallOfFame.tsx` | Remove top-holders list. Keep top-burners list (now sourced from shenanigans leaderboard). |
| `frontend/src/components/ShenanigansAdminPanel.tsx` | New "Mint Rules & Economy" tab with rows for every tunable (see Task 26). |

---

## Chunk 1 — Scaffolding (shared modules)

### Task 1: Add `shenanigans/PpLedger.mo` module

**Files:**
- Create: `shenanigans/PpLedger.mo`

- [ ] **Step 1: Write the module**

```motoko
/**
 * PP Ledger integration module (ICRC-1 + ICRC-2).
 * Mirrors backend/ledger.mo shape but points at pp_ledger on mainnet.
 */

import Principal "mo:base/Principal";
import Blob "mo:base/Blob";
import Nat "mo:base/Nat";
import Nat64 "mo:base/Nat64";

module {
    /// PP Ledger canister ID on mainnet (unchanged across reinstall)
    public let PP_LEDGER_CANISTER_ID : Text = "5xv2o-iiaaa-aaaac-qeclq-cai";

    /// PP uses 8 decimals post-reinstall; 1 whole PP = 10^8 units
    public let PP_DECIMALS : Nat8 = 8;
    public let PP_UNIT_SCALE : Nat = 100_000_000;

    public type Account = {
        owner : Principal;
        subaccount : ?Blob;
    };

    public type TransferArg = {
        from_subaccount : ?Blob;
        to : Account;
        amount : Nat;
        fee : ?Nat;
        memo : ?Blob;
        created_at_time : ?Nat64;
    };

    public type TransferError = {
        #BadFee : { expected_fee : Nat };
        #BadBurn : { min_burn_amount : Nat };
        #InsufficientFunds : { balance : Nat };
        #TooOld;
        #CreatedInFuture : { ledger_time : Nat64 };
        #Duplicate : { duplicate_of : Nat };
        #TemporarilyUnavailable;
        #GenericError : { error_code : Nat; message : Text };
    };

    public type TransferResult = { #Ok : Nat; #Err : TransferError };

    public type ApproveArg = {
        from_subaccount : ?Blob;
        spender : Account;
        amount : Nat;
        expected_allowance : ?Nat;
        expires_at : ?Nat64;
        fee : ?Nat;
        memo : ?Blob;
        created_at_time : ?Nat64;
    };

    public type ApproveError = {
        #BadFee : { expected_fee : Nat };
        #InsufficientFunds : { balance : Nat };
        #AllowanceChanged : { current_allowance : Nat };
        #Expired : { ledger_time : Nat64 };
        #TooOld;
        #CreatedInFuture : { ledger_time : Nat64 };
        #Duplicate : { duplicate_of : Nat };
        #TemporarilyUnavailable;
        #GenericError : { error_code : Nat; message : Text };
    };

    public type ApproveResult = { #Ok : Nat; #Err : ApproveError };

    public type TransferFromArg = {
        spender_subaccount : ?Blob;
        from : Account;
        to : Account;
        amount : Nat;
        fee : ?Nat;
        memo : ?Blob;
        created_at_time : ?Nat64;
    };

    public type TransferFromError = {
        #BadFee : { expected_fee : Nat };
        #BadBurn : { min_burn_amount : Nat };
        #InsufficientFunds : { balance : Nat };
        #InsufficientAllowance : { allowance : Nat };
        #TooOld;
        #CreatedInFuture : { ledger_time : Nat64 };
        #Duplicate : { duplicate_of : Nat };
        #TemporarilyUnavailable;
        #GenericError : { error_code : Nat; message : Text };
    };

    public type TransferFromResult = { #Ok : Nat; #Err : TransferFromError };

    public type LedgerActor = actor {
        icrc1_balance_of : shared query Account -> async Nat;
        icrc1_fee : shared query () -> async Nat;
        icrc1_transfer : shared TransferArg -> async TransferResult;
        icrc2_transfer_from : shared TransferFromArg -> async TransferFromResult;
    };
}
```

- [ ] **Step 2: Verify it parses**

Run: `dfx build shenanigans` (from project root, local replica stopped is fine — this is just a compile check).

If the module is referenced nowhere yet, `dfx build` will still type-check it once imported in Task 3. Skip to Step 3 if the build fails solely due to it being unused.

- [ ] **Step 3: Commit**

```bash
git add shenanigans/PpLedger.mo
git commit -m "feat(shenanigans): add PpLedger module for ICRC-1/ICRC-2 types and actor interface"
```

### Task 2: Add `shenanigans/Subaccount.mo` — deterministic chip subaccounts

**Files:**
- Create: `shenanigans/Subaccount.mo`

Design: deterministic 32-byte subaccount = principal bytes left-padded with `0x00` bytes. Principal serialization is ≤29 bytes and globally unique, so zero-padding produces globally unique 32-byte subaccounts. This avoids a sha256 dependency while meeting the spec's "deterministic derivation from principal" requirement.

- [ ] **Step 1: Write the module**

```motoko
import Principal "mo:base/Principal";
import Blob "mo:base/Blob";
import Array "mo:base/Array";
import Nat8 "mo:base/Nat8";

module {
    /// Default (all-zero) 32-byte subaccount.
    public func defaultSubaccount() : Blob {
        Blob.fromArray(Array.tabulate<Nat8>(32, func(_) = 0));
    };

    /// Map a player principal to a deterministic 32-byte chip subaccount.
    /// Encoding: principal bytes, left-padded on the right with 0x00 to 32 bytes.
    /// Principal byte representations are ≤29 bytes and globally unique, so
    /// two distinct principals always yield distinct subaccounts.
    public func principalToChipSubaccount(p : Principal) : Blob {
        let bytes = Blob.toArray(Principal.toBlob(p));
        let size = bytes.size();
        // Principals are always <= 29 bytes in practice, but guard defensively.
        assert (size <= 32);
        let padded = Array.tabulate<Nat8>(32, func(i) {
            if (i < size) { bytes[i] } else { 0 : Nat8 }
        });
        Blob.fromArray(padded);
    };
}
```

- [ ] **Step 2: Commit**

```bash
git add shenanigans/Subaccount.mo
git commit -m "feat(shenanigans): add Subaccount module for deterministic chip subaccount derivation"
```

---

## Chunk 2 — Shenanigans state, init, types

### Task 3: Rewrite `shenanigans/main.mo` header — imports, types, state

**Files:**
- Modify: `shenanigans/main.mo` (top of file)

This task replaces the imports, type declarations, and state block. Subsequent tasks fill in logic bodies. After this task the file will not compile until Task 4-5 are done — that's expected and we commit after Task 5.

- [ ] **Step 1: Replace imports (lines 1–10)**

```motoko
import OrderedMap "mo:base/OrderedMap";
import Principal "mo:base/Principal";
import Debug "mo:base/Debug";
import Time "mo:base/Time";
import Float "mo:base/Float";
import Int "mo:base/Int";
import Iter "mo:base/Iter";
import List "mo:base/List";
import Nat "mo:base/Nat";
import Nat64 "mo:base/Nat64";
import Blob "mo:base/Blob";
import Error "mo:base/Error";
import Timer "mo:base/Timer";
import Array "mo:base/Array";
import Text "mo:base/Text";

import PpLedger "PpLedger";
import Subaccount "Subaccount";
```

- [ ] **Step 2: Extend the types section (replace the existing `Types` block)**

Add these types after the existing `ShenaniganConfig` type declaration:

```motoko
/// A queued cash-out. Stays in the chip subaccount until claimed so
/// hostile spells can still drain it during the delay window.
public type CashOutEntry = {
    id : Nat;
    player : Principal;
    amount : Nat;              // PP-units requested
    claimableAfter : Int;      // nanoseconds (Time.now() + delay)
    claimed : Bool;            // set true after claimCashOut succeeds
};

/// Mutable mint + economy configuration. All fields admin-tunable.
public type MintConfig = {
    simple21DayPpPerIcp : Nat;    // initial 1000 (whole PP per ICP)
    compounding15DayPpPerIcp : Nat; // initial 2000
    compounding30DayPpPerIcp : Nat; // initial 3000
    dealerPpPerIcp : Nat;          // initial 4000
    referralL1Bps : Nat;           // basis points; initial 800 (= 8%)
    referralL2Bps : Nat;           // initial 500
    referralL3Bps : Nat;           // initial 200
    minDepositPp : Nat;            // initial 5000 (whole PP)
    cashOutDelaySeconds : Nat;     // initial 604_800
    observerIntervalSeconds : Nat; // initial 10
};

/// Per-dealer cumulative ICP seen by the observer. Used to mint only
/// on deltas when dealers top up.
public type DealerSeen = Float;
```

- [ ] **Step 3: Replace the `BackendActor` interface**

The new observer calls query methods only. No more `deduct/transfer/burn/getBalance` — those are being deleted from backend in Chunk 6.

```motoko
type BackendGamePlan = {
    #simple21Day;
    #compounding15Day;
    #compounding30Day;
};

type BackendGameRecord = {
    id : Nat;
    player : Principal;
    plan : BackendGamePlan;
    amount : Float;
    startTime : Int;
    isCompounding : Bool;
    isActive : Bool;
    lastUpdateTime : Int;
    accumulatedEarnings : Float;
    totalWithdrawn : Float;
};

type BackendDealerType = { #upstream; #downstream };

type BackendDealerPosition = {
    owner : Principal;
    amount : Float;
    entitlement : Float;
    startTime : Int;
    isActive : Bool;
    name : Text;
    dealerType : BackendDealerType;
    firstDepositDate : ?Int;
};

type BackendActor = actor {
    getAllGames : shared query () -> async [BackendGameRecord];
    getDealerPositions : shared query () -> async [BackendDealerPosition];
    getReferrer : shared query (Principal) -> async ?Principal;
};
```

- [ ] **Step 4: Replace the state block (was lines 84–100)**

```motoko
transient let natMap = OrderedMap.Make<Nat>(Nat.compare);
transient let principalMap = OrderedMap.Make<Principal>(Principal.compare);

// Spell configs — PRESERVED across migration (admin-tunable spell definitions)
var shenaniganConfigs = natMap.empty<ShenaniganConfig>();

// Spell cast history — reset at migration; bounded to last 500 entries
var shenanigans = natMap.empty<ShenaniganRecord>();
var shenaniganStats = principalMap.empty<ShenaniganStats>();
var nextShenaniganId : Nat = 0;

// Admin state
var adminPrincipal : ?Principal = null;
var backendPrincipal : ?Principal = null;

// Mint + economy configuration (mutable, admin-tunable)
var mintConfig : MintConfig = defaultMintConfig();

// Observer cursors
var gameIdCursor : Nat = 0;                         // next unprocessed game id
var dealerSeen = principalMap.empty<DealerSeen>();  // cumulative ICP minted-for per dealer

// Observer lock to prevent concurrent ticks
transient var observerRunning : Bool = false;
var observerTimerId : ?Timer.TimerId = null;

// Cash-out queue
var cashOuts = natMap.empty<CashOutEntry>();
var nextCashOutId : Nat = 0;

// Leaderboard (local state — not derived from ledger)
var ppBurnedPerPlayer = principalMap.empty<Nat>();  // cumulative PP units burned
var spellsCastPerPlayer = principalMap.empty<Nat>(); // successful casts only

// PP ledger actor reference
transient let ppLedger : PpLedger.LedgerActor = actor (PpLedger.PP_LEDGER_CANISTER_ID);
```

- [ ] **Step 5: Do not yet build — wait for Task 5**

### Task 4: Add default mint config helper

**Files:**
- Modify: `shenanigans/main.mo`

- [ ] **Step 1: Add `defaultMintConfig` helper below state block**

```motoko
func defaultMintConfig() : MintConfig {
    {
        simple21DayPpPerIcp = 1000;
        compounding15DayPpPerIcp = 2000;
        compounding30DayPpPerIcp = 3000;
        dealerPpPerIcp = 4000;
        referralL1Bps = 800;
        referralL2Bps = 500;
        referralL3Bps = 200;
        minDepositPp = 5000;
        cashOutDelaySeconds = 604_800;
        observerIntervalSeconds = 10;
    };
};
```

### Task 5: Update `initialize`, `requireAdmin`, `getBackend`; add migration wipe

**Files:**
- Modify: `shenanigans/main.mo`

- [ ] **Step 1: Replace `initialize` / `requireAdmin` / `getBackend` and add migration post-upgrade hook**

```motoko
public shared ({ caller }) func initialize(backendCanisterId : Principal) : async () {
    switch (adminPrincipal) {
        case (null) {
            adminPrincipal := ?caller;
            backendPrincipal := ?backendCanisterId;
            if (natMap.size(shenaniganConfigs) == 0) {
                initializeDefaultShenanigans();
            };
            startObserver();
        };
        case (?admin) {
            if (caller != admin) {
                Debug.trap("Already initialized. Only admin can reconfigure.");
            };
            backendPrincipal := ?backendCanisterId;
        };
    };
};

func requireAdmin(caller : Principal) {
    switch (adminPrincipal) {
        case (null) { Debug.trap("Not initialized") };
        case (?admin) {
            if (caller != admin) { Debug.trap("Unauthorized: admin only") };
        };
    };
};

public shared ({ caller }) func rotateAdmin(newAdmin : Principal) : async () {
    requireAdmin(caller);
    adminPrincipal := ?newAdmin;
};

func getBackend() : BackendActor {
    switch (backendPrincipal) {
        case (null) { Debug.trap("Backend canister not configured") };
        case (?p) { actor (Principal.toText(p)) : BackendActor };
    };
};
```

- [ ] **Step 2: Build to verify the scaffolding**

Run: `dfx build shenanigans` (from repo root; you may need `dfx start --clean --background` first).

Expected: `Building canister 'shenanigans'` succeeds. Timer-related code in `startObserver` is added in Task 8 — if build complains that `startObserver` is unresolved, add a stub:

```motoko
func startObserver() {
    // Implemented in Task 8
};
```

- [ ] **Step 3: Commit**

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): add mint config, cash-out queue, leaderboard state and typed backend interface"
```

---

## Chunk 3 — Observer (polling + minting)

### Task 6: Add mint helpers

**Files:**
- Modify: `shenanigans/main.mo`

Mints require `fee = null` because shenanigans' default subaccount is the minting account.

- [ ] **Step 1: Add helpers in a new `// Mint engine` section**

```motoko
// ================================================================
// Mint engine (internal helpers)
// ================================================================

func nowNat64() : Nat64 {
    Nat64.fromNat(Int.abs(Time.now()));
};

/// Convert whole PP → PP-units.
func ppToUnits(pp : Nat) : Nat { pp * PpLedger.PP_UNIT_SCALE };

/// ICP-float * (PP per ICP) → PP-units.
/// Example: 1.0 ICP * 1000 PP/ICP = 1000 whole PP = 10^11 PP-units.
func icpFloatToPpUnits(icp : Float, ppPerIcp : Nat) : Nat {
    if (icp <= 0.0) return 0;
    // icp * 10^8 = ICP-e8s; multiply by ppPerIcp; divide by 1 (we want units).
    // pp_units = (icp_e8s) * ppPerIcp, because ppPerIcp is whole-PP per ICP
    // and there are 10^8 PP-units per whole PP, so the 10^8 factors cancel.
    let icpE8s : Nat = Int.abs(Float.toInt(icp * 100_000_000.0));
    icpE8s * ppPerIcp;
};

/// Mint PP-units to a player's chip subaccount.
/// Returns #Ok(blockIndex) or #Err(text).
func mintTo(player : Principal, amount : Nat, memoText : Text) : async { #Ok : Nat; #Err : Text } {
    if (amount == 0) { return #Ok(0) };
    let memo = ?Text.encodeUtf8(memoText);
    try {
        let res = await ppLedger.icrc1_transfer({
            from_subaccount = null;
            to = {
                owner = Principal.fromActor(Self);
                subaccount = ?Subaccount.principalToChipSubaccount(player);
            };
            amount;
            fee = null;              // mint — must be null
            memo;
            created_at_time = ?nowNat64();
        });
        switch (res) {
            case (#Ok(idx)) { #Ok(idx) };
            case (#Err(#Duplicate { duplicate_of })) { #Ok(duplicate_of) }; // idempotent retry
            case (#Err(e)) { #Err(describeTransferErr(e)) };
        };
    } catch (e) {
        #Err("ppLedger call failed: " # Error.message(e));
    };
};

func describeTransferErr(err : PpLedger.TransferError) : Text {
    switch (err) {
        case (#BadFee({ expected_fee })) { "BadFee expected=" # Nat.toText(expected_fee) };
        case (#BadBurn({ min_burn_amount })) { "BadBurn min=" # Nat.toText(min_burn_amount) };
        case (#InsufficientFunds({ balance })) { "InsufficientFunds balance=" # Nat.toText(balance) };
        case (#TooOld) { "TooOld" };
        case (#CreatedInFuture(_)) { "CreatedInFuture" };
        case (#Duplicate({ duplicate_of })) { "Duplicate of=" # Nat.toText(duplicate_of) };
        case (#TemporarilyUnavailable) { "TemporarilyUnavailable" };
        case (#GenericError({ message; _ })) { "GenericError: " # message };
    };
};
```

The `Self` reference requires the actor header to be renamed. Update line 11 from `persistent actor {` to `persistent actor Self {`.

- [ ] **Step 2: Commit**

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): add mint helpers and actor self-reference"
```

### Task 7: Add referral cascade helper

**Files:**
- Modify: `shenanigans/main.mo`

- [ ] **Step 1: Add `cascadeReferralMint`**

Referral minting walks up the chain (L1 → L2 → L3). Each level mints `base * bps / 10_000`. The cascade reads the chain via backend query. If any mint fails, we log and continue — the player's own mint already landed, and referral PP is a bonus.

```motoko
/// For each of L1/L2/L3, mint referral PP-units derived from the base mint.
/// Memo tags `referral-LN-<eventId>` so dedup works per-level per-event.
func cascadeReferralMint(originUser : Principal, baseUnits : Nat, eventId : Text) : async () {
    if (baseUnits == 0) return;
    let backend = getBackend();
    let l1Maybe = try { await backend.getReferrer(originUser) } catch (_) { null };
    switch (l1Maybe) {
        case (null) {};
        case (?l1) {
            let l1Units = baseUnits * mintConfig.referralL1Bps / 10_000;
            let _ = await mintTo(l1, l1Units, "referral-L1-" # eventId);
            let l2Maybe = try { await backend.getReferrer(l1) } catch (_) { null };
            switch (l2Maybe) {
                case (null) {};
                case (?l2) {
                    let l2Units = baseUnits * mintConfig.referralL2Bps / 10_000;
                    let _ = await mintTo(l2, l2Units, "referral-L2-" # eventId);
                    let l3Maybe = try { await backend.getReferrer(l2) } catch (_) { null };
                    switch (l3Maybe) {
                        case (null) {};
                        case (?l3) {
                            let l3Units = baseUnits * mintConfig.referralL3Bps / 10_000;
                            let _ = await mintTo(l3, l3Units, "referral-L3-" # eventId);
                        };
                    };
                };
            };
        };
    };
};
```

- [ ] **Step 2: Commit**

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): add referral cascade mint helper"
```

### Task 8: Implement the observer tick + timer

**Files:**
- Modify: `shenanigans/main.mo`

- [ ] **Step 1: Replace the `startObserver` stub and add `observerTick`**

```motoko
// ================================================================
// Observer (polling timer)
// ================================================================

func startObserver() {
    // Cancel any existing timer first
    switch (observerTimerId) {
        case (?tid) { Timer.cancelTimer(tid) };
        case (null) {};
    };
    let interval : Nat = mintConfig.observerIntervalSeconds;
    let tid = Timer.recurringTimer<system>(#seconds(interval), observerTick);
    observerTimerId := ?tid;
};

/// One observer pass. Mints PP for new deposits and dealer top-ups.
/// Advances cursors only after successful mint to guarantee at-least-once
/// minting with ledger-level dedup (via created_at_time + memo) preventing
/// duplicates.
func observerTick() : async () {
    if (observerRunning) return;
    observerRunning := true;
    try {
        await processNewGames();
        await processDealerDeltas();
    } catch (e) {
        Debug.print("Observer tick error: " # Error.message(e));
    };
    observerRunning := false;
};

func processNewGames() : async () {
    let backend = getBackend();
    let games = try { await backend.getAllGames() } catch (_) { [] };
    // Order by id ascending so cursor advances monotonically
    let sorted = Array.sort<BackendGameRecord>(games, func(a, b) = Nat.compare(a.id, b.id));
    for (game in sorted.vals()) {
        if (game.id >= gameIdCursor) {
            let ppPerIcp = switch (game.plan) {
                case (#simple21Day) { mintConfig.simple21DayPpPerIcp };
                case (#compounding15Day) { mintConfig.compounding15DayPpPerIcp };
                case (#compounding30Day) { mintConfig.compounding30DayPpPerIcp };
            };
            let units = icpFloatToPpUnits(game.amount, ppPerIcp);
            let eventId = "game-" # Nat.toText(game.id);
            let res = await mintTo(game.player, units, eventId);
            switch (res) {
                case (#Ok(_)) {
                    await cascadeReferralMint(game.player, units, eventId);
                    gameIdCursor := game.id + 1;
                };
                case (#Err(msg)) {
                    Debug.print("Mint failed for " # eventId # ": " # msg);
                    return; // stop at first failure; retry next tick
                };
            };
        };
    };
};

func processDealerDeltas() : async () {
    let backend = getBackend();
    let dealers = try { await backend.getDealerPositions() } catch (_) { [] };
    for (dealer in dealers.vals()) {
        let seen : Float = switch (principalMap.get(dealerSeen, dealer.owner)) {
            case (null) { 0.0 };
            case (?v) { v };
        };
        if (dealer.amount > seen) {
            let delta : Float = dealer.amount - seen;
            let units = icpFloatToPpUnits(delta, mintConfig.dealerPpPerIcp);
            let eventId = "dealer-" # Principal.toText(dealer.owner) # "-"
                # Float.toText(dealer.amount);
            let res = await mintTo(dealer.owner, units, eventId);
            switch (res) {
                case (#Ok(_)) {
                    await cascadeReferralMint(dealer.owner, units, eventId);
                    dealerSeen := principalMap.put(dealerSeen, dealer.owner, dealer.amount);
                };
                case (#Err(msg)) {
                    Debug.print("Dealer mint failed: " # msg);
                };
            };
        };
    };
};
```

- [ ] **Step 2: Add admin method to set cursor at cutover**

At mainnet cutover we must set `gameIdCursor` to one past the last existing game id and seed `dealerSeen` with current dealer amounts so historical events are not re-minted (per Spec §Migration step 4).

```motoko
/// One-shot catch-up primer. Admin only. Call immediately after the
/// cutover upgrade completes, before unpausing user traffic.
public shared ({ caller }) func primeObserverCursors() : async () {
    requireAdmin(caller);
    let backend = getBackend();
    let games = await backend.getAllGames();
    var maxId : Nat = 0;
    for (g in games.vals()) { if (g.id >= maxId) { maxId := g.id + 1 } };
    gameIdCursor := maxId;

    let dealers = await backend.getDealerPositions();
    for (d in dealers.vals()) {
        dealerSeen := principalMap.put(dealerSeen, d.owner, d.amount);
    };
};
```

- [ ] **Step 3: Build and commit**

Run: `dfx build shenanigans`. Expected success.

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): implement observer tick, game/dealer delta processing, cursor primer"
```

---

## Chunk 4 — Chip custody: deposit, cash-out

### Task 9: Implement `depositChips`

**Files:**
- Modify: `shenanigans/main.mo`

Deposit = `icrc2_transfer_from` pulling from the user's external wallet `(caller, null)` into their chip subaccount `(shenanigans, chip_subaccount(caller))`. Requires the user previously signed `icrc2_approve` giving shenanigans a spend allowance on pp_ledger. Validates `amount >= minDepositPp * 10^8`. Uses fee = ?0 (pp_ledger's configured fee is 0).

- [ ] **Step 1: Add to a `// Chip custody` section**

```motoko
// ================================================================
// Chip custody — deposit / cash-out
// ================================================================

/// Pull `amountUnits` PP-units from the caller's wallet into their
/// chip subaccount. Caller must have signed icrc2_approve on pp_ledger
/// beforehand.
public shared ({ caller }) func depositChips(amountUnits : Nat) : async { #Ok : Nat; #Err : Text } {
    if (Principal.isAnonymous(caller)) { return #Err("Authentication required") };
    let minUnits = ppToUnits(mintConfig.minDepositPp);
    if (amountUnits < minUnits) {
        return #Err("Minimum deposit is " # Nat.toText(mintConfig.minDepositPp) # " PP");
    };
    try {
        let res = await ppLedger.icrc2_transfer_from({
            spender_subaccount = null;
            from = { owner = caller; subaccount = null };
            to = {
                owner = Principal.fromActor(Self);
                subaccount = ?Subaccount.principalToChipSubaccount(caller);
            };
            amount = amountUnits;
            fee = ?0;
            memo = ?Text.encodeUtf8("chip-deposit");
            created_at_time = ?nowNat64();
        });
        switch (res) {
            case (#Ok(idx)) { #Ok(idx) };
            case (#Err(#InsufficientAllowance(_))) {
                #Err("Approve shenanigans on pp_ledger first");
            };
            case (#Err(#InsufficientFunds({ balance }))) {
                #Err("Wallet balance too low (" # Nat.toText(balance) # " units)");
            };
            case (#Err(e)) { #Err(describeTransferFromErr(e)) };
        };
    } catch (e) {
        #Err("ppLedger call failed: " # Error.message(e));
    };
};

func describeTransferFromErr(err : PpLedger.TransferFromError) : Text {
    switch (err) {
        case (#BadFee({ expected_fee })) { "BadFee expected=" # Nat.toText(expected_fee) };
        case (#BadBurn({ min_burn_amount })) { "BadBurn min=" # Nat.toText(min_burn_amount) };
        case (#InsufficientFunds({ balance })) { "InsufficientFunds balance=" # Nat.toText(balance) };
        case (#InsufficientAllowance({ allowance })) { "InsufficientAllowance=" # Nat.toText(allowance) };
        case (#TooOld) { "TooOld" };
        case (#CreatedInFuture(_)) { "CreatedInFuture" };
        case (#Duplicate({ duplicate_of })) { "Duplicate of=" # Nat.toText(duplicate_of) };
        case (#TemporarilyUnavailable) { "TemporarilyUnavailable" };
        case (#GenericError({ message; _ })) { "GenericError: " # message };
    };
};
```

- [ ] **Step 2: Build**

```bash
dfx build shenanigans
```

- [ ] **Step 3: Commit**

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): add depositChips (icrc2_transfer_from → chip subaccount)"
```

### Task 10: Implement cash-out queue + claim

**Files:**
- Modify: `shenanigans/main.mo`

Design: `requestCashOut` validates the player's *current* chip balance is ≥ (sum of unclaimed queued + new request) — i.e. the queued amounts cannot exceed what they hold right now. The amount stays in the chip subaccount and is exposed to spells during the delay. `claimCashOut` checks elapsed time and transfers the lesser of the queued amount or the current chip balance.

- [ ] **Step 1: Add `requestCashOut` / `claimCashOut` / queries**

```motoko
public shared ({ caller }) func requestCashOut(amountUnits : Nat) : async { #Ok : Nat; #Err : Text } {
    if (Principal.isAnonymous(caller)) { return #Err("Authentication required") };
    if (amountUnits == 0) { return #Err("Amount must be positive") };

    // Sum of unclaimed cash-outs already queued for caller
    var pending : Nat = 0;
    for (entry in natMap.vals(cashOuts)) {
        if (entry.player == caller and not entry.claimed) {
            pending += entry.amount;
        };
    };

    let chipBalance = await ppLedger.icrc1_balance_of({
        owner = Principal.fromActor(Self);
        subaccount = ?Subaccount.principalToChipSubaccount(caller);
    });
    if (pending + amountUnits > chipBalance) {
        return #Err("Requested amount exceeds unqueued chip balance");
    };

    let id = nextCashOutId;
    nextCashOutId += 1;
    let claimableAfter : Int = Time.now() + (mintConfig.cashOutDelaySeconds * 1_000_000_000);
    let entry : CashOutEntry = {
        id;
        player = caller;
        amount = amountUnits;
        claimableAfter;
        claimed = false;
    };
    cashOuts := natMap.put(cashOuts, id, entry);
    #Ok(id);
};

public shared ({ caller }) func claimCashOut(id : Nat) : async { #Ok : Nat; #Err : Text } {
    let entry = switch (natMap.get(cashOuts, id)) {
        case (null) { return #Err("No such cash-out") };
        case (?e) { e };
    };
    if (entry.player != caller) { return #Err("Not your cash-out") };
    if (entry.claimed) { return #Err("Already claimed") };
    if (Time.now() < entry.claimableAfter) {
        return #Err("Claim not yet unlocked");
    };

    let chipBalance = await ppLedger.icrc1_balance_of({
        owner = Principal.fromActor(Self);
        subaccount = ?Subaccount.principalToChipSubaccount(caller);
    });
    let payable : Nat = if (chipBalance < entry.amount) { chipBalance } else { entry.amount };
    if (payable == 0) {
        // Mark claimed so the queue entry goes away even when balance was
        // fully drained by a spell during the window.
        cashOuts := natMap.put(cashOuts, id, { entry with claimed = true });
        return #Err("No chips left to cash out");
    };

    try {
        let res = await ppLedger.icrc1_transfer({
            from_subaccount = ?Subaccount.principalToChipSubaccount(caller);
            to = { owner = caller; subaccount = null };
            amount = payable;
            fee = ?0;
            memo = ?Text.encodeUtf8("cash-out-" # Nat.toText(id));
            created_at_time = ?nowNat64();
        });
        switch (res) {
            case (#Ok(idx)) {
                cashOuts := natMap.put(cashOuts, id, { entry with claimed = true });
                #Ok(idx);
            };
            case (#Err(e)) { #Err(describeTransferErr(e)) };
        };
    } catch (e) {
        #Err("ppLedger call failed: " # Error.message(e));
    };
};

/// Pending and recently claimed cash-outs for the caller.
public query ({ caller }) func getCashOutsFor(user : Principal) : async [CashOutEntry] {
    let all = Iter.toArray(natMap.vals(cashOuts));
    Array.filter<CashOutEntry>(all, func(e) { e.player == user });
};

public query ({ caller }) func getMyCashOuts() : async [CashOutEntry] {
    let all = Iter.toArray(natMap.vals(cashOuts));
    Array.filter<CashOutEntry>(all, func(e) { e.player == caller });
};
```

- [ ] **Step 2: Commit**

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): add cash-out queue with 7-day delay and claim flow"
```

---

## Chunk 5 — Spells on the ledger

### Task 11: Rewrite `castShenanigan` to use pp_ledger

**Files:**
- Modify: `shenanigans/main.mo`

Cost burns (chip → minting account, fee = null). Backfires transfer chip-to-chip (fee = ?0). Balance reads come from `icrc1_balance_of` on the chip subaccount. The cost in `ShenaniganConfig` stays a `Float` (whole PP) — convert to PP-units with `ppToUnits(Int.abs(Float.toInt(config.cost)))`.

- [ ] **Step 1: Add burn and chip-transfer helpers**

```motoko
/// Burn PP-units from a chip subaccount (transfer to minting account).
func burnFrom(player : Principal, units : Nat, memoText : Text) : async { #Ok : Nat; #Err : Text } {
    if (units == 0) { return #Ok(0) };
    try {
        let res = await ppLedger.icrc1_transfer({
            from_subaccount = ?Subaccount.principalToChipSubaccount(player);
            to = { owner = Principal.fromActor(Self); subaccount = null };
            amount = units;
            fee = null; // burn — must be null
            memo = ?Text.encodeUtf8(memoText);
            created_at_time = ?nowNat64();
        });
        switch (res) {
            case (#Ok(idx)) { #Ok(idx) };
            case (#Err(e)) { #Err(describeTransferErr(e)) };
        };
    } catch (e) {
        #Err("ppLedger call failed: " # Error.message(e));
    };
};

/// Chip-to-chip transfer (between two player subaccounts).
func chipTransfer(from : Principal, to : Principal, units : Nat, memoText : Text) : async { #Ok : Nat; #Err : Text } {
    if (units == 0) { return #Ok(0) };
    try {
        let res = await ppLedger.icrc1_transfer({
            from_subaccount = ?Subaccount.principalToChipSubaccount(from);
            to = {
                owner = Principal.fromActor(Self);
                subaccount = ?Subaccount.principalToChipSubaccount(to);
            };
            amount = units;
            fee = ?0;
            memo = ?Text.encodeUtf8(memoText);
            created_at_time = ?nowNat64();
        });
        switch (res) {
            case (#Ok(idx)) { #Ok(idx) };
            case (#Err(e)) { #Err(describeTransferErr(e)) };
        };
    } catch (e) {
        #Err("ppLedger call failed: " # Error.message(e));
    };
};

func getChipBalance(player : Principal) : async Nat {
    await ppLedger.icrc1_balance_of({
        owner = Principal.fromActor(Self);
        subaccount = ?Subaccount.principalToChipSubaccount(player);
    });
};
```

- [ ] **Step 2: Rewrite `castShenanigan`**

Replace the existing body. Behavior preserved: determineOutcome unchanged, backfire semantics preserved. Change: no more backend calls — all PP movement is on pp_ledger. Dealer cut removed per Open Question 1 resolution (Task 16).

```motoko
public shared ({ caller }) func castShenanigan(shenaniganType : ShenaniganType, target : ?Principal) : async ShenaniganOutcome {
    if (Principal.isAnonymous(caller)) { Debug.trap("Authentication required") };

    let config = switch (getConfigForType(shenaniganType)) {
        case (null) { Debug.trap("Unknown shenanigan type") };
        case (?c) { c };
    };
    let costUnits = ppToUnits(Int.abs(Float.toInt(config.cost)));

    let balance = await getChipBalance(caller);
    if (balance < costUnits) { Debug.trap("Insufficient chips to cast this shenanigan") };

    // Burn the cost. If it fails we trap — nothing else has run yet.
    let castId = nextShenaniganId;
    let burnMemo = "cast-" # Nat.toText(castId);
    switch (await burnFrom(caller, costUnits, burnMemo)) {
        case (#Err(msg)) { Debug.trap("Burn failed: " # msg) };
        case (#Ok(_)) {};
    };

    // Track burn in the leaderboard
    let priorBurn = switch (principalMap.get(ppBurnedPerPlayer, caller)) {
        case (null) { 0 };
        case (?n) { n };
    };
    ppBurnedPerPlayer := principalMap.put(ppBurnedPerPlayer, caller, priorBurn + costUnits);

    let outcome = determineOutcome(shenaniganType);

    // Apply backfire effects via chip-to-chip transfers
    if (outcome == #backfire) {
        switch (shenaniganType) {
            case (#moneyTrickster) {
                switch (target) {
                    case (null) {};
                    case (?targetP) {
                        let casterBal = await getChipBalance(caller);
                        let pct = 2 + (Int.abs(Time.now()) % 7);          // 2%-8%
                        let raw = casterBal * pct / 100;
                        let capped = if (raw > ppToUnits(250)) { ppToUnits(250) } else { raw };
                        let _ = await chipTransfer(caller, targetP, capped, "backfire-" # Nat.toText(castId));
                    };
                };
            };
            case (#aoeSkim) {
                let casterBal = await getChipBalance(caller);
                let pct = 1 + (Int.abs(Time.now()) % 3);                 // 1%-3%
                let loss = casterBal * pct / 100;
                let _ = await burnFrom(caller, loss, "backfire-aoe-" # Nat.toText(castId));
            };
            case (#downlineHeist) {
                switch (target) {
                    case (null) {};
                    case (?t) {
                        Debug.print("Backfire: " # Principal.toText(caller) # " loses L3 downline to " # Principal.toText(t));
                    };
                };
            };
            case (_) {};
        };
    };

    // Record the cast
    nextShenaniganId += 1;
    let newShenanigan : ShenaniganRecord = {
        id = castId;
        user = caller;
        shenaniganType;
        target;
        outcome;
        timestamp = Time.now();
        cost = config.cost;
    };
    shenanigans := natMap.put(shenanigans, castId, newShenanigan);
    updateShenaniganStats(caller, config.cost, outcome);
    if (outcome == #success or outcome == #backfire) {
        let prior = switch (principalMap.get(spellsCastPerPlayer, caller)) {
            case (null) { 0 };
            case (?n) { n };
        };
        spellsCastPerPlayer := principalMap.put(spellsCastPerPlayer, caller, prior + 1);
    };

    outcome;
};

func getConfigForType(t : ShenaniganType) : ?ShenaniganConfig {
    // Map variant → config id (matches initializeDefaultShenanigans ordering)
    let id : Nat = switch (t) {
        case (#moneyTrickster) { 0 };
        case (#aoeSkim) { 1 };
        case (#renameSpell) { 2 };
        case (#mintTaxSiphon) { 3 };
        case (#downlineHeist) { 4 };
        case (#magicMirror) { 5 };
        case (#ppBoosterAura) { 6 };
        case (#purseCutter) { 7 };
        case (#whaleRebalance) { 8 };
        case (#downlineBoost) { 9 };
        case (#goldenName) { 10 };
    };
    natMap.get(shenaniganConfigs, id);
};
```

- [ ] **Step 3: Build and commit**

```bash
dfx build shenanigans
git add shenanigans/main.mo
git commit -m "feat(shenanigans): cast spells via pp_ledger transfers; burn cost from chip subaccount"
```

---

## Chunk 6 — Leaderboard + admin tunables

### Task 12: Add leaderboard queries

**Files:**
- Modify: `shenanigans/main.mo`

- [ ] **Step 1: Add `// Leaderboard` section**

```motoko
// ================================================================
// Leaderboard
// ================================================================

/// Top-N players by cumulative PP burned. Returns (principal, PP-units).
public query func getTopPpBurners(limit : Nat) : async [(Principal, Nat)] {
    let entries = Iter.toArray(principalMap.entries(ppBurnedPerPlayer));
    let sorted = Array.sort<(Principal, Nat)>(
        entries,
        func(a, b) = Nat.compare(b.1, a.1),
    );
    let cap = if (limit < sorted.size()) { limit } else { sorted.size() };
    Array.subArray(sorted, 0, cap);
};

/// Top-N players by number of spells cast (success + backfire).
public query func getTopSpellCasters(limit : Nat) : async [(Principal, Nat)] {
    let entries = Iter.toArray(principalMap.entries(spellsCastPerPlayer));
    let sorted = Array.sort<(Principal, Nat)>(
        entries,
        func(a, b) = Nat.compare(b.1, a.1),
    );
    let cap = if (limit < sorted.size()) { limit } else { sorted.size() };
    Array.subArray(sorted, 0, cap);
};

public query func getPpBurnedFor(user : Principal) : async Nat {
    switch (principalMap.get(ppBurnedPerPlayer, user)) {
        case (null) { 0 };
        case (?n) { n };
    };
};
```

- [ ] **Step 2: Commit**

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): add burn + cast leaderboard queries"
```

### Task 13: Add admin getter/setter for every tunable

**Files:**
- Modify: `shenanigans/main.mo`

- [ ] **Step 1: Add `// Admin tunables` section**

```motoko
// ================================================================
// Admin tunables
// ================================================================

public query func getMintConfig() : async MintConfig { mintConfig };

public shared ({ caller }) func setSimple21DayPpPerIcp(v : Nat) : async () {
    requireAdmin(caller);
    mintConfig := { mintConfig with simple21DayPpPerIcp = v };
};
public shared ({ caller }) func setCompounding15DayPpPerIcp(v : Nat) : async () {
    requireAdmin(caller);
    mintConfig := { mintConfig with compounding15DayPpPerIcp = v };
};
public shared ({ caller }) func setCompounding30DayPpPerIcp(v : Nat) : async () {
    requireAdmin(caller);
    mintConfig := { mintConfig with compounding30DayPpPerIcp = v };
};
public shared ({ caller }) func setDealerPpPerIcp(v : Nat) : async () {
    requireAdmin(caller);
    mintConfig := { mintConfig with dealerPpPerIcp = v };
};
public shared ({ caller }) func setReferralBps(l1 : Nat, l2 : Nat, l3 : Nat) : async () {
    requireAdmin(caller);
    mintConfig := {
        mintConfig with
        referralL1Bps = l1;
        referralL2Bps = l2;
        referralL3Bps = l3;
    };
};
public shared ({ caller }) func setMinDepositPp(v : Nat) : async () {
    requireAdmin(caller);
    mintConfig := { mintConfig with minDepositPp = v };
};
public shared ({ caller }) func setCashOutDelaySeconds(v : Nat) : async () {
    requireAdmin(caller);
    mintConfig := { mintConfig with cashOutDelaySeconds = v };
};
public shared ({ caller }) func setObserverIntervalSeconds(v : Nat) : async () {
    requireAdmin(caller);
    if (v < 1) { Debug.trap("Interval must be >= 1 second") };
    mintConfig := { mintConfig with observerIntervalSeconds = v };
    startObserver(); // restart with new interval
};
```

- [ ] **Step 2: Commit**

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): add admin getter/setter for every mint tunable"
```

### Task 14: Re-run or cancel observer at admin will

**Files:**
- Modify: `shenanigans/main.mo`

- [ ] **Step 1: Add pause/resume controls**

```motoko
public shared ({ caller }) func stopObserver() : async () {
    requireAdmin(caller);
    switch (observerTimerId) {
        case (?tid) { Timer.cancelTimer(tid); observerTimerId := null };
        case (null) {};
    };
};

public shared ({ caller }) func resumeObserver() : async () {
    requireAdmin(caller);
    startObserver();
};

/// Manual one-shot observer tick (admin debug).
public shared ({ caller }) func runObserverOnce() : async () {
    requireAdmin(caller);
    await observerTick();
};
```

- [ ] **Step 2: Commit**

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): add admin pause/resume/run-once controls for observer"
```

---

## Chunk 7 — Shenanigans wrap-up

### Task 15: Remove obsolete shenanigans behavior

**Files:**
- Modify: `shenanigans/main.mo`

The old `distributeDealerCutFromShenanigans` backend call and the old `getPonziPointsBalanceFor` pathway are already removed by Task 11's rewrite. Verify nothing else references the deleted `BackendActor` methods.

- [ ] **Step 1: Grep for dead references**

Run: `Grep deductPonziPoints|transferPonziPoints|burnPonziPoints|getPonziPointsBalanceFor|distributeDealerCutFromShenanigans shenanigans/main.mo` — expected: zero matches.

If any remain, delete them.

- [ ] **Step 2: Verify old stats comment block**

The `updateShenaniganStats` function still references `cost * 0.1` for `dealerCut`. Per Task 11 / Open Question 1 we are no longer crediting a dealer pool from spell cost in backend. Keep the stat field so the existing admin UI keeps working (it is now effectively a "virtual" number not tied to any dealer pool), but add a comment:

```motoko
// dealerCut is kept in stats for UI continuity, but since backend no
// longer tracks PP dealer pools (Open Question 1 resolution: dealer
// repayment is ICP-only) this number is purely informational.
```

- [ ] **Step 3: Commit**

```bash
git add shenanigans/main.mo
git commit -m "chore(shenanigans): confirm dead backend PP calls removed, document dealerCut stat"
```

### Task 16: Regenerate shenanigans candid declarations

**Files:**
- Regenerate: `frontend/src/declarations/shenanigans/`

- [ ] **Step 1: Build + generate**

```bash
dfx build shenanigans
dfx generate shenanigans
```

- [ ] **Step 2: Commit the regenerated bindings**

```bash
git add frontend/src/declarations/shenanigans
git commit -m "chore(frontend): regenerate shenanigans candid bindings"
```

---

## Chunk 8 — Backend PP strip

### Task 17: Add `getReferrer` query to backend

**Files:**
- Modify: `backend/main.mo`

Shenanigans' referral cascade needs read access to `referralChain`. Add a plain query.

- [ ] **Step 1: Add after the existing `getReferralEarnings` query (~line 1375)**

```motoko
/// One-hop lookup — returns the caller's immediate referrer (L1) or null.
/// Used by shenanigans for referral PP cascades.
public query func getReferrer(user : Principal) : async ?Principal {
    principalMapNat.get(referralChain, user);
};
```

- [ ] **Step 2: Commit**

```bash
git add backend/main.mo
git commit -m "feat(backend): add getReferrer query for shenanigans referral cascade"
```

### Task 18: Remove PP state + methods from backend (part 1 — state + internal helpers)

**Files:**
- Modify: `backend/main.mo`

The state variables `ponziPoints`, `ponziPointsBurned`, `referralEarnings`, `shenanigansPrincipal` are dropped. Motoko preserves other stable variables across upgrade; removing a stable `var` drops its data.

Deletion order matters to avoid compile errors mid-edit: delete callers first, then helpers, then state.

- [ ] **Step 1: Delete internal helpers**

Remove these functions from `backend/main.mo`:
- `awardPonziPoints` (~line 788)
- `creditPonziPointsDirect` (~line 794)
- `awardReferralPP` (~line 803)
- `creditReferralEarnings` (~line 836)
- `updateDealerCut` (~line 1764)
- `requireShenanigansCanister` (~line 1695)

- [ ] **Step 2: Delete call sites for `awardPonziPoints`**

In `createGame` (around line 684) remove:
```
let points = switch (plan) { ... };
awardPonziPoints(caller, points);
```

In `seedGame` (around line 490-495) remove:
```
let points = switch (plan) { ... };
awardPonziPoints(player, points);
```

In `addDealerMoney` (around line 770) remove:
```
// Award 4,000 Ponzi Points per ICP deposited
awardPonziPoints(caller, amount * 4000.0);
```

- [ ] **Step 3: Commit**

```bash
git add backend/main.mo
git commit -m "refactor(backend): remove PP mint call sites and internal PP helpers"
```

### Task 19: Remove PP public methods from backend

**Files:**
- Modify: `backend/main.mo`

- [ ] **Step 1: Delete the following public methods**

| Method | Approx. line |
|---|---|
| `getPonziPoints` | 1509 |
| `getPonziPointsFor` | 1517 |
| `getPonziPointsBalance` | 1525 |
| `getPonziPointsBreakdownFor` | 1567 |
| `getReferralTierPoints` | 1609 |
| `getReferralTierPointsFor` | 1629 |
| `setShenanigansPrincipal` | 1706 |
| `deductPonziPoints` | 1713 |
| `transferPonziPoints` | 1723 |
| `distributeDealerCutFromShenanigans` | 1738 |
| `getPonziPointsBalanceFor` | 1744 |
| `burnPonziPoints` | 1752 |
| `getTopPonziPointsHolders` | 1783 |
| `getTopPonziPointsBurners` | 1791 |
| `getReferralEarnings` | 1369 |

- [ ] **Step 2: Delete PP state + type**

Delete:
- `ponziPoints` var (~line 203)
- `ponziPointsBurned` var (~line 206)
- `referralEarnings` var (~line 182)
- `shenanigansPrincipal` var (~line 321)
- `ReferralEarnings` type (~line 127)

Keep:
- `referralChain` var (used for ICP-denominated dealer referral cuts)
- `registerReferral` function (called inside `createGame` / `seedReferral`)

- [ ] **Step 3: Build**

```bash
dfx build backend
```

If any references remain (to removed fields/functions/types), the compiler will point them out — delete or repoint them.

- [ ] **Step 4: Commit**

```bash
git add backend/main.mo
git commit -m "refactor(backend): remove PP public methods, PP state, and shenanigansPrincipal authorization"
```

### Task 20: Regenerate backend candid

**Files:**
- Regenerate: `frontend/src/declarations/backend/`

- [ ] **Step 1: Run**

```bash
dfx generate backend
```

- [ ] **Step 2: Commit**

```bash
git add frontend/src/declarations/backend
git commit -m "chore(frontend): regenerate backend candid bindings after PP strip"
```

---

## Chunk 9 — `pp_ledger` reinstall prep

### Task 21: Update `dfx.json` pp_ledger init args

**Files:**
- Modify: `dfx.json`

Change `decimals` from 0 to 8 and set `minting_account` to the shenanigans principal. Drop the genesis mint to owner (the 1M can be re-minted post-reinstall via a `dfx canister call pp_ledger icrc1_transfer` from the minting account — done manually in Task 24 step 5, not in init).

Shenanigans mainnet principal: `j56tm-oaaaa-aaaac-qf34q-cai`.

- [ ] **Step 1: Replace the `init_arg` string**

```
(variant { Init = record {
  token_symbol = "PP";
  token_name = "Ponzi Points";
  minting_account = record {
    owner = principal "j56tm-oaaaa-aaaac-qf34q-cai";
    subaccount = null;
  };
  transfer_fee = 0 : nat;
  feature_flags = opt record { icrc2 = true };
  initial_balances = vec {};
  metadata = vec {};
  decimals = opt (8 : nat8);
  archive_options = record {
    num_blocks_to_archive = 1_000 : nat64;
    trigger_threshold = 2_000 : nat64;
    controller_id = principal "ft3ml-xex6k-ppiwj-ie6tc-zwkgb-ybm2x-eat4a-5p2jg-auzl3-latf4-aae";
    cycles_for_archive_creation = opt (100_000_000_000 : nat64);
    max_transactions_per_response = null;
    max_message_size_bytes = null;
    node_max_memory_size_bytes = null;
    more_controller_ids = null;
  };
} })
```

- [ ] **Step 2: Commit**

```bash
git add dfx.json
git commit -m "chore(pp_ledger): reinit config — decimals=8, minting_account=shenanigans, drop genesis"
```

### Task 22: Generate pp_ledger frontend declarations

**Files:**
- Create: `frontend/src/declarations/pp_ledger/`

- [ ] **Step 1: Run**

```bash
dfx generate pp_ledger
```

This downloads the candid file from the release URL and generates TS bindings.

- [ ] **Step 2: Commit**

```bash
git add frontend/src/declarations/pp_ledger
git commit -m "chore(frontend): generate pp_ledger candid bindings"
```

---

## Chunk 10 — Frontend: read PP from ledger

### Task 23: Add `usePpLedger` hook

**Files:**
- Create: `frontend/src/hooks/usePpLedger.ts`

- [ ] **Step 1: Write the hook**

```typescript
import { useMemo } from 'react';
import { Actor, HttpAgent, ActorSubclass } from '@dfinity/agent';
import { Principal } from '@dfinity/principal';
import { useWallet } from './useWallet';
import { idlFactory } from '../declarations/pp_ledger';
import type { _SERVICE } from '../declarations/pp_ledger';

export const PP_LEDGER_CANISTER_ID = '5xv2o-iiaaa-aaaac-qeclq-cai';
const HOST = 'https://icp0.io';

const SHENANIGANS_CANISTER_ID = 'j56tm-oaaaa-aaaac-qf34q-cai';

function makeAnonActor(): ActorSubclass<_SERVICE> {
  const agent = new HttpAgent({ host: HOST });
  return Actor.createActor<_SERVICE>(idlFactory, {
    agent,
    canisterId: PP_LEDGER_CANISTER_ID,
  });
}

export function useReadPpLedger(): ActorSubclass<_SERVICE> {
  return useMemo(() => makeAnonActor(), []);
}

export function useAuthPpLedger(): ActorSubclass<_SERVICE> | null {
  const { identity, walletType, isInitializing } = useWallet();
  return useMemo(() => {
    if (isInitializing) return null;
    if (walletType === 'plug' && (window as any).ic?.plug?.agent) {
      return Actor.createActor<_SERVICE>(idlFactory, {
        agent: (window as any).ic.plug.agent,
        canisterId: PP_LEDGER_CANISTER_ID,
      });
    }
    if (!identity) return null;
    const agent = new HttpAgent({ host: HOST, identity });
    return Actor.createActor<_SERVICE>(idlFactory, {
      agent,
      canisterId: PP_LEDGER_CANISTER_ID,
    });
  }, [identity, walletType, isInitializing]);
}

/** Build the 32-byte chip subaccount for a principal (mirrors shenanigans/Subaccount.mo). */
export function principalToChipSubaccount(principal: Principal): Uint8Array {
  const bytes = principal.toUint8Array();
  const out = new Uint8Array(32);
  out.set(bytes);
  return out;
}

export function shenanigansOwner(): Principal {
  return Principal.fromText(SHENANIGANS_CANISTER_ID);
}

export const PP_DECIMALS = 8;
export const PP_UNIT_SCALE = 100_000_000n;

/** Format PP-units as a whole-number string (no decimals shown by default). */
export function ppUnitsToWhole(units: bigint): number {
  return Number(units / PP_UNIT_SCALE);
}

/** Parse a whole-number PP amount into PP-units. */
export function wholePpToUnits(whole: number | bigint): bigint {
  if (typeof whole === 'number') return BigInt(Math.trunc(whole)) * PP_UNIT_SCALE;
  return whole * PP_UNIT_SCALE;
}
```

- [ ] **Step 2: Commit**

```bash
git add frontend/src/hooks/usePpLedger.ts
git commit -m "feat(frontend): add usePpLedger hook + subaccount helpers"
```

### Task 24: Repoint `useGetPonziPoints` to the ledger

**Files:**
- Modify: `frontend/src/hooks/useQueries.ts`

- [ ] **Step 1: Replace `useGetPonziPoints`**

```typescript
import { useReadPpLedger, shenanigansOwner, principalToChipSubaccount, ppUnitsToWhole } from './usePpLedger';

export function useGetPonziPoints() {
  const ledger = useReadPpLedger();
  const { principal } = useWallet();

  return useQuery({
    queryKey: ['ppBalances', principal],
    queryFn: async () => {
      if (!principal) throw new Error('No principal');
      const p = Principal.fromText(principal);
      const [chipUnits, walletUnits] = await Promise.all([
        ledger.icrc1_balance_of({
          owner: shenanigansOwner(),
          subaccount: [principalToChipSubaccount(p)],
        }),
        ledger.icrc1_balance_of({ owner: p, subaccount: [] }),
      ]);
      return {
        chipPoints: ppUnitsToWhole(chipUnits),
        walletPoints: ppUnitsToWhole(walletUnits),
        totalPoints: ppUnitsToWhole(chipUnits + walletUnits),
      };
    },
    enabled: !!principal,
    refetchInterval: 5000,
  });
}
```

Note: the returned shape changed — old code read `totalPoints`, `depositPoints`, `referralPoints`. Update consumers in Task 27. For now, introducing the new hook may break existing call sites until Task 27 updates them. Build will catch them.

- [ ] **Step 2: Replace `useGetTopPonziPointsHolders`** with a deletion (return null / throw) — spec removes this leaderboard.

```typescript
export function useGetTopPonziPointsHolders() {
  return useQuery({
    queryKey: ['topPonziPointsHolders'],
    queryFn: async () => [] as { rank: number; name: string; ponziPoints: number; principal: string }[],
  });
}
```

- [ ] **Step 3: Repoint `useGetTopPonziPointsBurners`**

```typescript
export function useGetTopPonziPointsBurners() {
  const actor = useShenaniganActor().actor;
  return useQuery({
    queryKey: ['topPpBurners'],
    queryFn: async () => {
      if (!actor) return [];
      const burners = await actor.getTopPpBurners(50n);
      return burners.map(([principal, unitsBig], index) => ({
        rank: index + 1,
        name: `User ${principal.toString().slice(-8)}`,
        ponziPointsBurned: Number(unitsBig / 100_000_000n),
        principal: principal.toString(),
      }));
    },
    refetchInterval: 30000,
  });
}
```

- [ ] **Step 4: Commit**

```bash
git add frontend/src/hooks/useQueries.ts
git commit -m "feat(frontend): read PP balances from pp_ledger; repoint burner leaderboard to shenanigans"
```

### Task 25: Add deposit / cash-out hooks

**Files:**
- Modify: `frontend/src/hooks/useQueries.ts`

- [ ] **Step 1: Add mutations**

```typescript
import { useAuthPpLedger, wholePpToUnits } from './usePpLedger';

const SHENANIGANS_PRINCIPAL = Principal.fromText('j56tm-oaaaa-aaaac-qf34q-cai');

/** One-time approve for chip deposits. Amount in whole PP. */
export function useApproveForDeposits() {
  const ppLedger = useAuthPpLedger();
  return useMutation({
    mutationFn: async (wholePp: number) => {
      if (!ppLedger) throw new Error('No pp_ledger actor');
      const res = await ppLedger.icrc2_approve({
        from_subaccount: [],
        spender: { owner: SHENANIGANS_PRINCIPAL, subaccount: [] },
        amount: wholePpToUnits(wholePp),
        expected_allowance: [],
        expires_at: [],
        fee: [],
        memo: [],
        created_at_time: [],
      });
      if ('Err' in res) throw new Error(JSON.stringify(res.Err));
      return res.Ok;
    },
  });
}

export function useDepositChips() {
  const { actor } = useShenaniganActor();
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (wholePp: number) => {
      if (!actor) throw new Error('No shenanigans actor');
      const res = await actor.depositChips(wholePpToUnits(wholePp));
      if ('Err' in res) throw new Error(res.Err);
      return res.Ok;
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['ppBalances'] });
      qc.invalidateQueries({ queryKey: ['pendingCashOuts'] });
    },
  });
}

export function useRequestCashOut() {
  const { actor } = useShenaniganActor();
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (wholePp: number) => {
      if (!actor) throw new Error('No shenanigans actor');
      const res = await actor.requestCashOut(wholePpToUnits(wholePp));
      if ('Err' in res) throw new Error(res.Err);
      return res.Ok;
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ['pendingCashOuts'] }),
  });
}

export function useClaimCashOut() {
  const { actor } = useShenaniganActor();
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (id: bigint) => {
      if (!actor) throw new Error('No shenanigans actor');
      const res = await actor.claimCashOut(id);
      if ('Err' in res) throw new Error(res.Err);
      return res.Ok;
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['pendingCashOuts'] });
      qc.invalidateQueries({ queryKey: ['ppBalances'] });
    },
  });
}

export function usePendingCashOuts() {
  const { actor } = useShenaniganActor();
  const { principal } = useWallet();
  return useQuery({
    queryKey: ['pendingCashOuts', principal],
    queryFn: async () => {
      if (!actor) return [];
      const entries = await actor.getMyCashOuts();
      return entries.map((e: any) => ({
        id: e.id as bigint,
        amount: Number(e.amount) / 1e8,
        claimableAfter: new Date(Number(e.claimableAfter) / 1_000_000),
        claimed: e.claimed as boolean,
      }));
    },
    enabled: !!actor && !!principal,
    refetchInterval: 10000,
  });
}
```

- [ ] **Step 2: Commit**

```bash
git add frontend/src/hooks/useQueries.ts
git commit -m "feat(frontend): add approve/deposit/request-cash-out/claim/pending hooks"
```

### Task 26: Update `PonziPointsDashboard` to show chip vs wallet

**Files:**
- Modify: `frontend/src/components/PonziPointsDashboard.tsx`

The old `depositPoints` / `referralPoints` breakdown is gone (backend no longer tracks the provenance split). The dashboard now shows chip balance, wallet balance, total, plus a CTA to the new `ChipWallet` page.

- [ ] **Step 1: Replace the balance block**

```tsx
const chipPoints = ponziData?.chipPoints || 0;
const walletPoints = ponziData?.walletPoints || 0;
const totalPoints = ponziData?.totalPoints || 0;

return (
  <div className="space-y-6">
    <div className="mc-card-elevated">
      <div className="text-center mb-6">
        <div className="mc-label mb-2">Your Ponzi Points</div>
        <div className="text-2xl sm:text-4xl mc-text-purple mc-glow-purple font-display">
          {totalPoints.toLocaleString()} PP
        </div>
      </div>
      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        <div className="mc-card mc-accent-green p-4 text-center">
          <div className="mc-label mb-1">Chips (spendable)</div>
          <div className="text-xl font-bold mc-text-green">{chipPoints.toLocaleString()}</div>
        </div>
        <div className="mc-card mc-accent-cyan p-4 text-center">
          <div className="mc-label mb-1">Wallet (external)</div>
          <div className="text-xl font-bold mc-text-cyan">{walletPoints.toLocaleString()}</div>
        </div>
      </div>
      <div className="flex gap-3 mt-4 justify-center">
        <button onClick={() => window.location.hash = '#chips'} className="mc-btn mc-btn-primary">
          Manage chips
        </button>
      </div>
    </div>
    {/* keep "How to earn" block as-is */}
  </div>
);
```

- [ ] **Step 2: Commit**

```bash
git add frontend/src/components/PonziPointsDashboard.tsx
git commit -m "feat(frontend): show chip vs wallet PP balances with chip-wallet CTA"
```

### Task 27: Build `ChipWallet` component

**Files:**
- Create: `frontend/src/components/ChipWallet.tsx`

- [ ] **Step 1: Write the component**

```tsx
import React, { useState } from 'react';
import { toast } from 'sonner';
import { useGetPonziPoints, useApproveForDeposits, useDepositChips, useRequestCashOut, useClaimCashOut, usePendingCashOuts } from '../hooks/useQueries';

export default function ChipWallet() {
  const { data } = useGetPonziPoints();
  const { data: pending } = usePendingCashOuts();
  const approve = useApproveForDeposits();
  const deposit = useDepositChips();
  const request = useRequestCashOut();
  const claim = useClaimCashOut();

  const [depositAmount, setDepositAmount] = useState<number>(5000);
  const [cashOutAmount, setCashOutAmount] = useState<number>(5000);

  const wallet = data?.walletPoints ?? 0;
  const chips = data?.chipPoints ?? 0;

  return (
    <div className="space-y-6 p-4">
      <section className="mc-card p-4">
        <h2 className="font-display text-xl mb-2">Bring chips to the table</h2>
        <p className="text-sm mc-text-muted mb-3">
          Wallet: <b>{wallet.toLocaleString()} PP</b> · Chips: <b>{chips.toLocaleString()} PP</b>
        </p>
        <input
          type="number"
          min={5000}
          value={depositAmount}
          onChange={(e) => setDepositAmount(Number(e.target.value))}
          className="mc-input w-40 mr-2"
        />
        <button
          className="mc-btn mc-btn-secondary mr-2"
          onClick={async () => {
            try {
              await approve.mutateAsync(depositAmount * 10);
              toast.success('Approved pp_ledger spend');
            } catch (e: any) { toast.error(e.message); }
          }}
        >Approve (one-time)</button>
        <button
          className="mc-btn mc-btn-primary"
          disabled={deposit.isPending}
          onClick={async () => {
            try {
              await deposit.mutateAsync(depositAmount);
              toast.success(`Deposited ${depositAmount} PP`);
            } catch (e: any) { toast.error(e.message); }
          }}
        >Deposit</button>
      </section>

      <section className="mc-card p-4">
        <h2 className="font-display text-xl mb-2">Cash out</h2>
        <p className="text-sm mc-text-muted mb-3">
          7-day lockup. Chips stay exposed to spells during the window.
        </p>
        <input
          type="number"
          min={1}
          value={cashOutAmount}
          onChange={(e) => setCashOutAmount(Number(e.target.value))}
          className="mc-input w-40 mr-2"
        />
        <button
          className="mc-btn mc-btn-primary"
          disabled={request.isPending}
          onClick={async () => {
            try {
              await request.mutateAsync(cashOutAmount);
              toast.success('Cash-out queued');
            } catch (e: any) { toast.error(e.message); }
          }}
        >Request cash-out</button>
      </section>

      <section className="mc-card p-4">
        <h2 className="font-display text-xl mb-2">Pending cash-outs</h2>
        {!pending || pending.length === 0 ? (
          <p className="text-sm mc-text-muted">None.</p>
        ) : (
          <ul className="space-y-2">
            {pending.map((p) => {
              const now = new Date();
              const ready = !p.claimed && p.claimableAfter <= now;
              return (
                <li key={String(p.id)} className="flex items-center justify-between">
                  <span>
                    {p.amount} PP · {p.claimed ? 'claimed' : ready ? 'ready' : `unlocks ${p.claimableAfter.toLocaleString()}`}
                  </span>
                  {ready && !p.claimed && (
                    <button
                      className="mc-btn mc-btn-success"
                      onClick={async () => {
                        try { await claim.mutateAsync(p.id); toast.success('Claimed'); }
                        catch (e: any) { toast.error(e.message); }
                      }}
                    >Claim</button>
                  )}
                </li>
              );
            })}
          </ul>
        )}
      </section>
    </div>
  );
}
```

- [ ] **Step 2: Wire the component into `App.tsx` routing**

Add a route/hash case for `#chips` that renders `<ChipWallet />`. Match existing app routing style.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/components/ChipWallet.tsx frontend/src/App.tsx
git commit -m "feat(frontend): add ChipWallet component for approve/deposit/cash-out"
```

### Task 28: Extend admin panel with Mint Rules tab

**Files:**
- Modify: `frontend/src/components/ShenanigansAdminPanel.tsx`

- [ ] **Step 1: Add the mint config section**

Add a collapsible section at the top with rows for each tunable, wired to new hooks:

```tsx
import { useGetMintConfig, useSetSimple21, useSetCompounding15, useSetCompounding30, useSetDealerMultiplier, useSetReferralBps, useSetMinDeposit, useSetCashOutDelay, useSetObserverInterval } from '../hooks/useQueries';
```

(Each hook wraps the corresponding shenanigans setter, mirroring the existing `useUpdateShenaniganConfig` pattern. Add them in Task 25's file where the other mutations live.)

Render rows for:
- `simple21DayPpPerIcp` (initial 1000)
- `compounding15DayPpPerIcp` (initial 2000)
- `compounding30DayPpPerIcp` (initial 3000)
- `dealerPpPerIcp` (initial 4000)
- `referralL1Bps` / `L2Bps` / `L3Bps` (initial 800 / 500 / 200)
- `minDepositPp` (initial 5000)
- `cashOutDelaySeconds` (initial 604800)
- `observerIntervalSeconds` (initial 10)

Each row: label + current value + input + Save button that calls the setter.

- [ ] **Step 2: Commit**

```bash
git add frontend/src/components/ShenanigansAdminPanel.tsx frontend/src/hooks/useQueries.ts
git commit -m "feat(frontend): mint rules & economy tab with admin setters"
```

### Task 29: Update Hall of Fame

**Files:**
- Modify: `frontend/src/components/HallOfFame.tsx`

- [ ] **Step 1: Remove the top-holders section**

Delete any component blocks that render the output of `useGetTopPonziPointsHolders`. Keep the burners section, which is now sourced from shenanigans' `getTopPpBurners`.

- [ ] **Step 2: Commit**

```bash
git add frontend/src/components/HallOfFame.tsx
git commit -m "refactor(frontend): remove top-PP-holders list; keep burners from shenanigans"
```

---

## Chunk 11 — Local verification

### Task 30: Local deploy + integration smoke test

**Files:**
- None (script)

- [ ] **Step 1: Start local replica fresh**

```bash
dfx start --clean --background
```

- [ ] **Step 2: Deploy in dependency order**

```bash
dfx deploy pp_ledger
dfx deploy backend
dfx deploy shenanigans
SHENANIGANS_ID=$(dfx canister id shenanigans)
BACKEND_ID=$(dfx canister id backend)
dfx canister call shenanigans initialize "(principal \"$BACKEND_ID\")"
```

- [ ] **Step 3: Smoke test — deposit & balance**

Note: the local pp_ledger init args must match mainnet for this to work — update `dfx.json`'s `init_arg` to use `$(dfx canister id shenanigans)` as the minting account before deploying locally. The `init_arg` value contains a literal shenanigans principal — swap it out for local testing OR use `--argument` on the command line where shell substitution works.

```bash
# Mint 100_000 PP directly from pp_ledger (minting account = shenanigans)
dfx canister call shenanigans runObserverOnce '()'
# or call icrc1_transfer directly via a test admin principal

PRINCIPAL=$(dfx identity get-principal)
# Check wallet balance
dfx canister call pp_ledger icrc1_balance_of "(record { owner = principal \"$PRINCIPAL\"; subaccount = null })"
```

Expected: `(0 : nat)` initially, then post-mint balance reflects the event.

- [ ] **Step 4: Smoke test — full flow**

1. Call `backend.createGame` with 1 ICP Simple plan (mock the ICP transfer with a test ledger — or skip ICP enforcement in local mode).
2. Wait 10 seconds.
3. Query `pp_ledger.icrc1_balance_of` on the depositor's chip subaccount. Expected: 1000 whole PP = `100_000_000_000` units.
4. Cast a spell: `dfx canister call shenanigans castShenanigan '(variant { moneyTrickster }, null)'`. Expected: outcome variant returned; chip balance decreases by 120 PP.

- [ ] **Step 5: Commit test notes (optional)**

If any scripts are added, commit them under `scripts/smoke/` — no new files in this task otherwise.

### Task 31: Type-check frontend

**Files:**
- None

- [ ] **Step 1: Run**

```bash
cd frontend && npm run build
```

Fix any TS errors (most likely in `BackerMoneyToast.tsx`, `GameStatusBar.tsx`, `useQueries.ts` references to removed backend types).

- [ ] **Step 2: Commit fixes**

```bash
git add frontend/src
git commit -m "chore(frontend): fix type errors from backend PP strip"
```

---

## Chunk 12 — Mainnet cutover

**WARNING: These steps hit mainnet canisters. The user has given go-ahead to wipe existing PP balances (including the 1M owner balance). Do NOT run without explicit user confirmation on the day of cutover.**

### Task 32: Cutover runbook

**Files:**
- None (operational)

- [ ] **Step 1: Freeze traffic**

Ask user to put the frontend into a maintenance banner (short manual edit). Not strictly required but good practice.

- [ ] **Step 2: Upgrade shenanigans (no reinstall — preserves shenaniganConfigs)**

```bash
dfx deploy shenanigans --network ic
```

- [ ] **Step 3: Reinstall pp_ledger (wipes state)**

```bash
dfx canister install pp_ledger --mode reinstall --network ic --yes
```

This wipes all balances including the 1M owner genesis. Confirm the install output shows new init args with `decimals = 8` and the shenanigans principal as minting account.

- [ ] **Step 4: Re-mint owner's 1M PP (optional, per spec non-goal #3)**

If the user wants it kept:

```bash
dfx canister call pp_ledger icrc1_transfer --network ic "(record {
  from_subaccount = null;
  to = record { owner = principal \"ft3ml-xex6k-ppiwj-ie6tc-zwkgb-ybm2x-eat4a-5p2jg-auzl3-latf4-aae\"; subaccount = null };
  amount = 100_000_000_000_000 : nat;
  fee = null;
  memo = null;
  created_at_time = null;
})"
```

(1,000,000 PP × 10^8 units = 100_000_000_000_000 units.)

This must be called by the minting account, which is the shenanigans canister. Since only shenanigans can mint, you need a shenanigans admin method. Add to `shenanigans/main.mo`:

```motoko
public shared ({ caller }) func mintTo(to : Principal, wholePp : Nat) : async { #Ok : Nat; #Err : Text } {
    requireAdmin(caller);
    let units = ppToUnits(wholePp);
    switch (await mintTo(to, units, "admin-mint")) {
        case (#Ok(idx)) { #Ok(idx) };
        case (#Err(msg)) { #Err(msg) };
    };
};
```

(Rename the internal helper `mintTo` → `mintInternal` to avoid the collision, or use a distinct public name like `adminMint`.)

Add this helper as an additional task below (Task 35).

- [ ] **Step 5: Upgrade backend (strips PP)**

```bash
dfx deploy backend --network ic
```

- [ ] **Step 6: Prime observer cursors**

```bash
dfx canister call shenanigans primeObserverCursors '()' --network ic
```

This prevents retroactive minting for pre-cutover deposits. From here forward, only new `createGame`/`addDealerMoney` events mint PP.

- [ ] **Step 7: Deploy frontend**

```bash
cd frontend && npm run build && cd ..
dfx deploy frontend --network ic
```

- [ ] **Step 8: Smoke test on mainnet**

- Admin wallet: check balance via `pp_ledger.icrc1_balance_of` — should match re-minted 1M (if re-minted) or 0.
- Random user: create a game with 0.1 ICP, wait 15 seconds, confirm chip balance shows 100 PP.

- [ ] **Step 9: Commit any migration runbook notes (optional)**

If anything was adjusted during cutover, capture it in `docs/superpowers/plans/2026-04-19-pp-real-token.md` as a "Cutover log" section and commit.

### Task 33 (inserted between Task 14 and Task 15 during implementation): Admin `adminMint` method

**Files:**
- Modify: `shenanigans/main.mo`

- [ ] **Step 1: Rename internal helper and add public admin-gated wrapper**

Rename the internal `func mintTo(...)` added in Task 6 to `mintInternal`. Add at the admin section:

```motoko
public shared ({ caller }) func adminMint(to : Principal, wholePp : Nat) : async { #Ok : Nat; #Err : Text } {
    requireAdmin(caller);
    await mintInternal(to, ppToUnits(wholePp), "admin-mint-" # Principal.toText(to));
};
```

Update all internal callers of `mintTo` (in `processNewGames`, `processDealerDeltas`, `cascadeReferralMint`) to call `mintInternal`.

- [ ] **Step 2: Commit**

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): add adminMint for admin-triggered manual PP issuance"
```

---

## Chunk 13 — Observability & follow-ups

### Task 34: Add observer status query

**Files:**
- Modify: `shenanigans/main.mo`

- [ ] **Step 1: Add status query**

```motoko
public query func getObserverStatus() : async {
    running : Bool;
    gameIdCursor : Nat;
    dealerSeenCount : Nat;
    intervalSeconds : Nat;
} {
    {
        running = observerTimerId != null;
        gameIdCursor;
        dealerSeenCount = principalMap.size(dealerSeen);
        intervalSeconds = mintConfig.observerIntervalSeconds;
    };
};
```

- [ ] **Step 2: Commit**

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): add getObserverStatus for admin dashboard"
```

### Task 35: Surface observer status in admin panel

**Files:**
- Modify: `frontend/src/components/ShenanigansAdminPanel.tsx`

- [ ] **Step 1: Render observer status row at the top of the admin panel**

Label: "Observer". Show running/paused, current game cursor, dealers tracked, interval. Add Pause/Resume buttons wired to `stopObserver` / `resumeObserver`.

- [ ] **Step 2: Commit**

```bash
git add frontend/src/components/ShenanigansAdminPanel.tsx
git commit -m "feat(frontend): observer status row with pause/resume"
```

---

## Post-implementation verification checklist

Run on mainnet after Task 32:

- [ ] A fresh principal calling `backend.createGame(#simple21Day, 0.1 ICP)` sees its pp_ledger chip balance rise to `10_000_000_000` units (100 PP) within 20 seconds.
- [ ] That principal's referrer (if any) sees a cascade: L1 receives 8% of 100 PP = 8 PP-worth of units.
- [ ] Casting `moneyTrickster` (120 PP cost) drops chip balance by `12_000_000_000` units. `getTopPpBurners(10)` shows the caster at rank 1.
- [ ] `requestCashOut(5000)` queues an entry; `claimCashOut(id)` before 7 days returns `#Err "Claim not yet unlocked"`.
- [ ] Setting `setObserverIntervalSeconds(30)` via admin call and calling `getObserverStatus()` reflects the new interval.
- [ ] Backend's candid no longer exposes any method containing `PonziPoints` in its name (check via `dfx canister metadata backend candid:service --network ic`).
- [ ] pp_ledger `icrc1_metadata()` returns decimals = 8 and the shenanigans principal as minting account.

## Out-of-scope reminders

- Blackholing backend (removing controllers) is NOT part of this plan. A separate cleanup for `setTestMode`, `setCanisterPrincipal`, `assignCallerUserRole`, seed methods is tracked separately per spec §Trust properties.
- PP inflation modeling, DAO controls for shenanigans upgrades, PP trading venues — all explicitly deferred.

---

## Self-review (performed after writing)

**Spec coverage:** Every numbered section in the spec maps to at least one task:
- backend PP strip → Tasks 17-20
- pp_ledger re-init → Tasks 21-22
- Minting (observer pattern) → Tasks 6-8
- Chip custody + subaccount layout → Task 2, Task 11 helpers
- Deposit flow → Task 9, Task 25
- Cash-out flow → Task 10, Task 25, Task 27
- Spell casting → Task 11
- Leaderboard → Task 12
- Admin dashboard → Tasks 13, 14, 28, 34-35
- Tunable parameters initial values → Task 4 (`defaultMintConfig`)
- Trust properties (blackhole-readiness) → Task 19 deletes `shenanigansPrincipal` & PP methods
- Migration runbook → Task 32
- Open Question 1 (dealer cut from shenanigans) → resolved in Task 11 by removing the dealer-pool call; `dealerCut` stat kept purely informational per Task 15
- Open Question 2 (referral chain ownership) → backend keeps `referralChain`, exposes `getReferrer` in Task 17
- Open Question 3 (observer lag UX) → addressed in Task 26 dashboard (chip vs wallet distinction makes pending PP obvious; further polish deferred)
- Open Question 4 (pp_ledger reinit procedure) → Task 32 step 3 uses `dfx canister install --mode reinstall`

**Placeholder scan:** No "TBD", "implement later", or bare "add error handling" sentinels remain. Every code step ships runnable Motoko/TypeScript.

**Type consistency:** `mintTo` was renamed to `mintInternal` in Task 33 to free up the public `adminMint` — callers in Tasks 6-8 reference `mintTo` and must be updated when Task 33 runs. Flagged inline.

**Execution handoff** — see top of file.
