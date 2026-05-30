# Buy PP with SOL — Founder's Allocation Desk (Backend) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an admin-operated OTC desk to `ponzi_math_sol` so SIWS users can buy loose `pp_ledger` PP with SOL: Charles escrows PP, sets laddered PP-per-0.1-SOL tiers, and a buyer's SOL payment (matched by the existing deposit observer) atomically releases escrowed PP to the buyer's IC principal.

**Architecture:** Pure-additive state and functions on the existing `ponzi_math_sol` persistent actor — new top-level stable `var`s (`deskTiers`, `pendingBuyIntents`, accrual counter) and new variant arms on `GeneralLedgerEvent`, all of which flow through orthogonal persistence with **no migration module** (there is none; only *modifying existing stored records* would need one). Settlement reuses the per-user deposit-address + RPC observer pipeline: a new `#buyPP`-style `BuyIntent` is matched in `creditDeposit`'s existing "no deposit intent" branch, and PP is released via a newly-wired `pp_ledger` ICRC-1 actor.

**Tech Stack:** Motoko (`mo:base`, `persistent actor`, `OrderedMap`), a copied `PpLedger` ICRC-1/2 client module, the existing threshold-Ed25519 SOL pipeline (`SolSigner`/`SolTx`/`SolRpc`), `dfx` for local verification.

**Scope:** Backend engine + queries ONLY. The frontend (buyer flyout replacing `BuySOLFlyout`, admin "PP Desk" panel, `useBuyPpDesk` hook) is a **separate follow-up plan**, written after this backend's candid interface lands. This backend is independently verifiable via `dfx canister call`.

**Testing reality:** There is **no `mops.toml`** — `ponzi_math_sol` has no Motoko unit-test harness (confirmed: `find` shows no `*.test.mo`, no mops config). Verification is a `dfx canister call` matrix on a local replica, matching the `2026-05-29-exit-liquidity-backend.md` precedent. Deterministic logic (tier CRUD, quote walk, reservation, inventory) is asserted directly via calls against a **locally-deployed `pp_ledger`**. The SOL-in→PP-out settlement path cannot run locally (it needs Solana devnet RPC), so its credit/fill logic is exercised through a `TEST_ADMIN`-gated shim (`adminTestSettleBuyIntent`) that bypasses the RPC and drives `settleBuyIntent` directly. `vitest` exists but is for the frontend plan.

---

## Conventions to follow (verified against the live code)

- **Self principal:** the actor class has no `this`/`self` binding today. Add `= self` to the class header (Task 1) and use `Principal.fromActor(self)` for the escrow account owner. Do **not** hardcode the mainnet canister id — the local replica id differs and would break local tests.
- **Admin gate:** `requireAdmin(caller)` traps on non-admins (`ponzi_math_sol/main.mo:378`); `isAdmin(caller) : Bool` (`:370`). All `desk*` admin methods call `requireAdmin(caller)` first.
- **Auth gate:** `requireAuthenticated(caller)` traps anonymous (`:352`).
- **Concurrency:** wrap state-mutating update calls in `acquireCallerLock(caller)` / `releaseCallerLock(caller)` via `try/finally` (pattern at `prepareSolDeposit`, `:2419-2467`).
- **Maps:** `natMap` (Nat keys, `:216`), `principalMapNat` (Principal keys, `:217`), `textMap` (Text keys, `:263`). Use `natMap` for `pendingBuyIntents`.
- **Intent template:** `prepareSolDeposit` (`:2407-2468`) is the exact shape to mirror for `createBuyIntent` — auth, bootstrapped check, lock, derive-or-reuse deposit address, create record with `expiresAt = now + TTL`, return `{ intentId; depositAddress }`.
- **Deposit address:** `principalMapNat.get(depositAddresses, caller)` then derive via `SolSigner.deriveAddress(keyId, derivationPathForPrincipal(caller))` if absent (`:2439-2449`).
- **Observer settlement hook:** `creditDeposit` (`:2177`) computes `inboundLamports` then matches an open `DepositIntent`. Its `case (null)` arm (`:2249-2262`) — "confirmed inbound, no matching deposit intent" — is where buy-intent matching slots in, **before** the unmatched-log + cursor advance.
- **Amount tolerance:** `bpsApply(expected, 500)` = ±5% (`:2239`, helper `:2063`).
- **Detection scan set:** `runDetectionForOpenIntents` (`:2513`) builds `toScan : textMap<Principal>` from open `pendingIntents`. Buy-intent addresses MUST be added to the same set or buy payments are never polled.
- **SOL out of pool:** `sendSolPayout(toAddress, lamports) : async {#Ok:Text; #Err:Text}` (`:875`) builds+broadcasts a pool→address transfer. Used for `adminWithdrawDeskProceeds` and admin refunds.
- **Sweep in:** `sweepToPool(address, derivationPathForPrincipal(p), lamports)` (`:2079`) moves a buyer's deposit-address balance to the pool after settlement.
- **Accrual counter pattern:** `coverChargeAccrualLamports : Nat64` (`:285`) — mirror as `deskProceedsAccrualLamports` so desk revenue is tracked separately from the game pot even though both physically live on the pool address.
- **Ledger events:** `recordLedger(#variant({...}))` (`:403`); `GeneralLedgerEvent` is a stored variant — adding arms is additive/safe (no migration).
- **Units:** PP = 8 decimals (`PpLedger.PP_UNIT_SCALE = 100_000_000`); SOL = 9 decimals, so **0.1 SOL = 100_000_000 lamports**. This coincidence makes "PP-units per 0.1 SOL" give exact integer quote math.

---

## File structure

- **Create:** `ponzi_math_sol/PpLedger.mo` — verbatim copy of `shenanigans/PpLedger.mo` (ICRC-1/2 client; no shenanigans-specific deps). Keeps `ponzi_math_sol` self-contained.
- **Modify:** `ponzi_math_sol/main.mo` — all desk state, admin methods, quote, buy-intent, settlement hook (additive; one surgical edit inside `creditDeposit` and one inside `runDetectionForOpenIntents`).
- **Regenerate:** `frontend/src/declarations/ponzi_math_sol/*` via `npm run generate` (Task 9) so the frontend plan has the candid.
- No `migration` module (none exists; not needed).

---

## Quote & reservation math (single source of truth — reused in Tasks 5, 6, 7)

Rate is stored as **PP-units per 0.1 SOL** (`ratePpUnitsPer0_1Sol : Nat`). With `S = 100_000_000`:

- PP out for `lamports` at one tier: `ppUnits = lamports * rate / S` (floor; house-favorable).
- Lamports to buy `ppUnits` at one tier: `lamports = ppUnits * S / rate` (floor).

A multi-tier quote walks tiers **top-down in Charles's list order**, spending `lamports` against each tier's *available* PP (`ppUnitsTotal − ppUnitsSold − ppUnitsReserved`) until the SOL is exhausted or tiers run out (`cappedByInventory`). Settlement walks the intent's **locked** reservation legs (each carrying its own `ratePpUnitsPer0_1Sol`) so later admin tier edits never change an open quote.

---

## Starting config (illustrative — Charles tunes live via admin methods)

- `DESK_BUY_INTENT_TTL_NS = 15 * 60 * 1_000_000_000` (15 min).
- `MIN_BUY_LAMPORTS : Nat64 = 10_000_000` (0.01 SOL).
- Example ladder (set via `deskAddTier` after stocking): `[{rate=300_000 * S; total=...}, {rate=250_000 * S; ...}, {rate=200_000 * S; ...}]` i.e. 300k → 250k → 200k PP per 0.1 SOL as tiers sell out.

---

## Task 1: Copy the ledger module, wire the actor, expose the escrow account

**Files:**
- Create: `ponzi_math_sol/PpLedger.mo`
- Modify: `ponzi_math_sol/main.mo` (imports, actor header `= self`, ledger actor, escrow constant + account helper + query)

- [ ] **Step 1: Create `ponzi_math_sol/PpLedger.mo`** as a verbatim copy of `shenanigans/PpLedger.mo` (same `module { ... }` body: `PP_LEDGER_CANISTER_ID = "5xv2o-iiaaa-aaaac-qeclq-cai"`, `PP_UNIT_SCALE = 100_000_000`, `Account`, `TransferArg`, `TransferResult`, `ApproveArg`, `TransferFromArg`, `TransferFromResult`, and `LedgerActor` with `icrc1_balance_of`, `icrc1_fee`, `icrc1_transfer`, `icrc2_transfer_from`).

- [ ] **Step 2: Add imports + ledger actor near the other `transient let` actors** (after `solRpc`/`ic` at `ponzi_math_sol/main.mo:36-37`). Ensure `import Array "mo:base/Array";` and `import Blob "mo:base/Blob";` exist at the top (add if missing).

```motoko
import PpLedger "PpLedger";
// ...
transient let ppLedger : PpLedger.LedgerActor = actor (PpLedger.PP_LEDGER_CANISTER_ID);
```

- [ ] **Step 3: Bind `self` on the actor class header.** Change the class header's closing `}) {` to `}) = self {` (the line that closes the `initArgs` record type and opens the actor body, just before `transient let BACKEND_PRINCIPAL`).

```motoko
// before:  }) {
// after:
}) = self {
```

- [ ] **Step 4: Add the escrow subaccount constant + account helper + query** (near the deposit-address state, after `ponzi_math_sol/main.mo:265`).

```motoko
// Desk PP inventory lives in a fixed subaccount of THIS canister on pp_ledger.
// Bytes spell "PPDESK" then zero-padded to 32. Distinct from the default
// (null) subaccount so desk inventory never mixes with any other PP the
// canister might hold.
transient let DESK_ESCROW_SUBACCOUNT : Blob = Blob.fromArray([
    0x50, 0x50, 0x44, 0x45, 0x53, 0x4b, // "PPDESK"
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0
]);

func deskEscrowAccount() : PpLedger.Account {
    { owner = Principal.fromActor(self); subaccount = ?DESK_ESCROW_SUBACCOUNT };
};

public query func getDeskEscrowAccount() : async { owner : Principal; subaccount : Blob } {
    { owner = Principal.fromActor(self); subaccount = DESK_ESCROW_SUBACCOUNT };
};
```

- [ ] **Step 5: Compile-deploy on a fresh local replica.**

Run:
```bash
dfx start --background --clean
dfx deploy ponzi_math_sol --argument '(record { backendPrincipal = principal "2vxsx-fae"; testAdmin = principal "2vxsx-fae"; solTreasuryAddress = "11111111111111111111111111111111"; solRpcProvider = variant { Devnet }; keyId = record { name = "dfx_test_key"; curve = variant { ed25519 } }; solRpcCanisterId = principal "tghme-zyaaa-aaaar-qarca-cai" })'
```
Expected: deploy succeeds (compiles). If the `--argument` record shape mismatches, copy the exact init record from `dfx.json`/existing deploy scripts — the point of this step is that **it compiles with `= self` and the new `PpLedger` import**.

- [ ] **Step 6: Verify the escrow account query.**

Run: `dfx canister call ponzi_math_sol getDeskEscrowAccount '()'`
Expected: a record with `owner = <local ponzi_math_sol principal>` and a 32-byte `subaccount` blob beginning `\50\50\44\45\53\4b`.

- [ ] **Step 7: Commit.**

```bash
git add ponzi_math_sol/PpLedger.mo ponzi_math_sol/main.mo
git commit -m "feat(ponzi_math_sol): wire pp_ledger ICRC actor + desk escrow account"
```

---

## Task 2: Desk state — tier/intent types, stable vars, ledger event arms

**Files:**
- Modify: `ponzi_math_sol/main.mo` (types + stable vars near `:280`; `GeneralLedgerEvent` arms near `:140`)

- [ ] **Step 1: Add the desk types + stable vars** immediately after the `pendingIntents`/`nextIntentId` block (`ponzi_math_sol/main.mo:281`).

```motoko
// ===== Founder's Allocation Desk (buy PP with SOL) =====
public type DeskTier = {
    ratePpUnitsPer0_1Sol : Nat; // PP-units per 0.1 SOL (=1e8 lamports)
    ppUnitsTotal : Nat;
    ppUnitsSold : Nat;
    ppUnitsReserved : Nat;      // held against open buy intents
};
var deskTiers : [DeskTier] = [];

public type BuyReservation = {
    tierIndex : Nat;
    ppUnits : Nat;
    ratePpUnitsPer0_1Sol : Nat; // LOCKED at quote time
};
public type BuyIntent = {
    id : Nat;
    principal : Principal;
    reserved : [BuyReservation];
    ppUnitsReservedTotal : Nat;
    quotedLamports : Nat64;
    createdAt : Int;
    expiresAt : Int;
    fulfilled : Bool;
};
var pendingBuyIntents = natMap.empty<BuyIntent>();
var nextBuyIntentId : Nat = 0;

// Desk SOL revenue, tracked separately from the game pot (mirrors
// coverChargeAccrualLamports). Physically lives on the pool address after
// sweep; this counter bounds adminWithdrawDeskProceeds.
var deskProceedsAccrualLamports : Nat64 = 0;

transient let DESK_BUY_INTENT_TTL_NS : Int = 15 * 60 * 1_000_000_000;
transient let MIN_BUY_LAMPORTS : Nat64 = 10_000_000; // 0.01 SOL
transient let PP_S : Nat = 100_000_000; // PP unit scale; 0.1 SOL = 1e8 lamports
```

- [ ] **Step 2: Add ledger event arms.** In the `GeneralLedgerEvent` variant (`ponzi_math_sol/main.mo:140`), add:

```motoko
        #deskSale : { buyer : Principal; ppUnitsCredited : Nat; lamportsReceived : Nat; intentId : Nat };
        #deskProceedsWithdrawal : { toAddress : Text; lamports : Nat; txSig : Text };
```

- [ ] **Step 3: Add a desk-state query** (place near `getMyPendingIntents`, `:2470`).

```motoko
public query func deskListTiers() : async [DeskTier] { deskTiers };
```

- [ ] **Step 4: Deploy + verify it compiles and tiers start empty.**

Run:
```bash
dfx deploy ponzi_math_sol --mode upgrade --yes
dfx canister call ponzi_math_sol deskListTiers '()'
```
Expected: deploy/upgrade succeeds; call returns `(vec {})`.

- [ ] **Step 5: Commit.**

```bash
git add ponzi_math_sol/main.mo
git commit -m "feat(ponzi_math_sol): add desk tier/buy-intent state + ledger events"
```

---

## Task 3: Admin tier management

**Files:**
- Modify: `ponzi_math_sol/main.mo` (new admin methods near the other admin functions, e.g. after `:2483`)

- [ ] **Step 1: Add the tier admin methods.**

```motoko
// Append a tier. Charles lists best-deal-first (highest rate at the top).
public shared ({ caller }) func deskAddTier(ratePpUnitsPer0_1Sol : Nat, ppUnitsTotal : Nat) : async { #Ok : Nat; #Err : Text } {
    requireAdmin(caller);
    if (ratePpUnitsPer0_1Sol == 0) { return #Err("Rate must be > 0") };
    if (ppUnitsTotal == 0) { return #Err("Quantity must be > 0") };
    let tier : DeskTier = { ratePpUnitsPer0_1Sol; ppUnitsTotal; ppUnitsSold = 0; ppUnitsReserved = 0 };
    deskTiers := Array.append(deskTiers, [tier]);
    #Ok(deskTiers.size() - 1 : Nat);
};

// Update a tier's rate and/or total. Total cannot drop below sold+reserved.
public shared ({ caller }) func deskUpdateTier(index : Nat, ratePpUnitsPer0_1Sol : Nat, ppUnitsTotal : Nat) : async { #Ok : (); #Err : Text } {
    requireAdmin(caller);
    if (index >= deskTiers.size()) { return #Err("No such tier") };
    if (ratePpUnitsPer0_1Sol == 0) { return #Err("Rate must be > 0") };
    let t = deskTiers[index];
    if (ppUnitsTotal < t.ppUnitsSold + t.ppUnitsReserved) {
        return #Err("Total cannot be below already sold+reserved");
    };
    let updated = { t with ratePpUnitsPer0_1Sol; ppUnitsTotal };
    deskTiers := Array.tabulate<DeskTier>(deskTiers.size(), func(i) { if (i == index) { updated } else { deskTiers[i] } });
    #Ok();
};

// Remove a tier. Blocked while it (or, for index-shift safety, ANY tier)
// has reserved PP, so open intents' tierIndex stays valid.
public shared ({ caller }) func deskRemoveTier(index : Nat) : async { #Ok : (); #Err : Text } {
    requireAdmin(caller);
    if (index >= deskTiers.size()) { return #Err("No such tier") };
    for (t in deskTiers.vals()) {
        if (t.ppUnitsReserved > 0) { return #Err("Cannot restructure tiers while buy intents are open; wait for them to settle or expire (max 15 min)") };
    };
    deskTiers := Array.tabulate<DeskTier>(deskTiers.size() - 1 : Nat, func(i) { if (i < index) { deskTiers[i] } else { deskTiers[i + 1] } });
    #Ok();
};

// Replace the full ordered list (reorder/bulk edit). Blocked while reservations exist.
public shared ({ caller }) func deskReorderTiers(newOrder : [DeskTier]) : async { #Ok : (); #Err : Text } {
    requireAdmin(caller);
    for (t in deskTiers.vals()) {
        if (t.ppUnitsReserved > 0) { return #Err("Cannot reorder while buy intents are open") };
    };
    for (t in newOrder.vals()) {
        if (t.ratePpUnitsPer0_1Sol == 0 or t.ppUnitsTotal < t.ppUnitsSold) { return #Err("Invalid tier in new order") };
    };
    deskTiers := newOrder;
    #Ok();
};
```

- [ ] **Step 2: Deploy + verify tier CRUD via dfx.**

Run:
```bash
dfx deploy ponzi_math_sol --mode upgrade --yes
dfx canister call ponzi_math_sol deskAddTier '(30000000000000 : nat, 100000000000000 : nat)'   # 300k PP/0.1SOL, 1,000,000 PP
dfx canister call ponzi_math_sol deskAddTier '(20000000000000 : nat, 100000000000000 : nat)'   # 200k PP/0.1SOL
dfx canister call ponzi_math_sol deskListTiers '()'
dfx canister call ponzi_math_sol deskUpdateTier '(1 : nat, 25000000000000 : nat, 100000000000000 : nat)'
dfx canister call ponzi_math_sol deskRemoveTier '(0 : nat)'
dfx canister call ponzi_math_sol deskListTiers '()'
```
Expected: first `deskAddTier` → `(variant { Ok = 0 })`; list shows two tiers; update → `Ok`; remove → `Ok`; final list shows one tier with `ratePpUnitsPer0_1Sol = 25_000_000_000_000`.

- [ ] **Step 3: Verify non-admin is rejected.**

Run: `dfx identity new desk_tester 2>/dev/null; dfx --identity desk_tester canister call ponzi_math_sol deskAddTier '(1 : nat, 1 : nat)'`
Expected: trap "Unauthorized: admin only". (The default `dfx` identity is `testAdmin` from Task 1's init arg, so it passes; `desk_tester` is not.)

- [ ] **Step 4: Commit.**

```bash
git add ponzi_math_sol/main.mo
git commit -m "feat(ponzi_math_sol): desk admin tier management"
```

---

## Task 4: Inventory — escrow deposit, withdraw, and inventory query

**Files:**
- Modify: `ponzi_math_sol/main.mo` (inventory methods; reserved-total helper)

- [ ] **Step 1: Add a reserved-total helper + inventory methods.**

```motoko
func deskReservedTotal() : Nat {
    var sum : Nat = 0;
    for (t in deskTiers.vals()) { sum += t.ppUnitsReserved };
    sum;
};

// Charles must icrc2_approve THIS canister as spender on pp_ledger first,
// then call this to pull `ppUnits` from his wallet into the escrow subaccount.
public shared ({ caller }) func deskDepositInventory(ppUnits : Nat) : async { #Ok : Nat; #Err : Text } {
    requireAdmin(caller);
    if (ppUnits == 0) { return #Err("Amount must be > 0") };
    try {
        let res = await ppLedger.icrc2_transfer_from({
            spender_subaccount = null;
            from = { owner = caller; subaccount = null };
            to = deskEscrowAccount();
            amount = ppUnits;
            fee = null; // pp_ledger fee is 0
            memo = null;
            created_at_time = null;
        });
        switch (res) {
            case (#Ok(block)) { #Ok(block) };
            case (#Err(e)) { #Err("transfer_from failed: " # debug_show (e)) };
        };
    } catch (e) { #Err("ppLedger call failed: " # Error.message(e)) };
};

// Return unsold, unreserved PP from escrow to `toOwner`.
public shared ({ caller }) func deskWithdrawInventory(ppUnits : Nat, toOwner : Principal) : async { #Ok : Nat; #Err : Text } {
    requireAdmin(caller);
    if (ppUnits == 0) { return #Err("Amount must be > 0") };
    let bal = await ppLedger.icrc1_balance_of(deskEscrowAccount());
    let reserved = deskReservedTotal();
    let available : Nat = if (bal > reserved) { bal - reserved } else { 0 };
    if (ppUnits > available) { return #Err("Only " # Nat.toText(available) # " PP available (rest is reserved)") };
    try {
        let res = await ppLedger.icrc1_transfer({
            from_subaccount = ?DESK_ESCROW_SUBACCOUNT;
            to = { owner = toOwner; subaccount = null };
            amount = ppUnits;
            fee = null;
            memo = null;
            created_at_time = null;
        });
        switch (res) {
            case (#Ok(block)) { #Ok(block) };
            case (#Err(e)) { #Err("transfer failed: " # debug_show (e)) };
        };
    } catch (e) { #Err("ppLedger call failed: " # Error.message(e)) };
};

public func deskInventory() : async { balanceUnits : Nat; reservedUnits : Nat; availableUnits : Nat } {
    let bal = await ppLedger.icrc1_balance_of(deskEscrowAccount());
    let reserved = deskReservedTotal();
    { balanceUnits = bal; reservedUnits = reserved; availableUnits = if (bal > reserved) { bal - reserved } else { 0 } };
};
```

- [ ] **Step 2: Deploy a local `pp_ledger` and mint test PP to the default (admin) identity.**

Run:
```bash
dfx deploy pp_ledger   # uses the init_arg in dfx.json (minting account = shenanigans; default identity gets initial_balances)
dfx deploy ponzi_math_sol --mode upgrade --yes
# Approve ponzi_math_sol as spender, then deposit 500,000 PP (50,000,000,000,000 units) into escrow:
PMS=$(dfx canister id ponzi_math_sol)
dfx canister call pp_ledger icrc2_approve "(record { spender = record { owner = principal \"$PMS\"; subaccount = null }; amount = 50000000000000 : nat; fee = opt (0 : nat); expected_allowance = null; expires_at = null; memo = null; from_subaccount = null; created_at_time = null })"
dfx canister call ponzi_math_sol deskDepositInventory '(50000000000000 : nat)'
dfx canister call ponzi_math_sol deskInventory '()'
```
Expected: `icrc2_approve` → `(variant { Ok = ... })`; `deskDepositInventory` → `(variant { Ok = ... })`; `deskInventory` → `balanceUnits = 50_000_000_000_000`, `reservedUnits = 0`, `availableUnits = 50_000_000_000_000`.
(If the default identity has no PP balance locally, the `initial_balances` principal in `dfx.json` differs from your dfx identity; mint by calling `icrc1_transfer` from the minting account, or temporarily set `initial_balances` to your `dfx identity get-principal` for local testing.)

- [ ] **Step 3: Verify withdraw respects availability.**

Run:
```bash
dfx canister call ponzi_math_sol deskWithdrawInventory "(10000000000000 : nat, principal \"$(dfx identity get-principal)\")"
dfx canister call ponzi_math_sol deskInventory '()'
```
Expected: withdraw `Ok`; inventory now `balanceUnits = 40_000_000_000_000`.

- [ ] **Step 4: Commit.**

```bash
git add ponzi_math_sol/main.mo
git commit -m "feat(ponzi_math_sol): desk PP inventory deposit/withdraw + query"
```

---

## Task 5: Quote query (`quoteBuyPP`)

**Files:**
- Modify: `ponzi_math_sol/main.mo` (pure quote walker + query)

- [ ] **Step 1: Add the quote walker + types + query.**

```motoko
public type QuoteLeg = { tierIndex : Nat; ppUnits : Nat; lamports : Nat64; ratePpUnitsPer0_1Sol : Nat };
public type DeskQuote = { ppUnitsOut : Nat; legs : [QuoteLeg]; cappedByInventory : Bool };

// Pure: walk tiers top-down spending `lamports` against each tier's available PP.
func computeQuote(lamports : Nat64) : DeskQuote {
    var remaining : Nat = Nat64.toNat(lamports);
    var totalPp : Nat = 0;
    var legs = List.nil<QuoteLeg>();
    var i : Nat = 0;
    while (i < deskTiers.size() and remaining > 0) {
        let t = deskTiers[i];
        let availablePp : Nat = if (t.ppUnitsTotal > t.ppUnitsSold + t.ppUnitsReserved) {
            t.ppUnitsTotal - t.ppUnitsSold - t.ppUnitsReserved : Nat;
        } else { 0 };
        if (availablePp > 0) {
            let maxLamportsForTier : Nat = availablePp * PP_S / t.ratePpUnitsPer0_1Sol;
            let spend : Nat = Nat.min(remaining, maxLamportsForTier);
            let ppFromTier : Nat = spend * t.ratePpUnitsPer0_1Sol / PP_S;
            if (ppFromTier > 0) {
                totalPp += ppFromTier;
                legs := List.push({ tierIndex = i; ppUnits = ppFromTier; lamports = Nat64.fromNat(spend); ratePpUnitsPer0_1Sol = t.ratePpUnitsPer0_1Sol }, legs);
                remaining -= spend;
            };
        };
        i += 1;
    };
    { ppUnitsOut = totalPp; legs = List.toArray(List.reverse(legs)); cappedByInventory = remaining > 0 };
};

public query func quoteBuyPP(lamports : Nat64) : async DeskQuote { computeQuote(lamports) };
```

- [ ] **Step 2: Deploy + verify the quote against the tiers from Task 4.**

Run (with one tier 250k PP/0.1SOL = `25_000_000_000_000` from Task 3, ample inventory):
```bash
dfx deploy ponzi_math_sol --mode upgrade --yes
dfx canister call ponzi_math_sol deskAddTier '(25000000000000 : nat, 100000000000000 : nat)'
dfx canister call ponzi_math_sol quoteBuyPP '(100000000 : nat64)'   # 0.1 SOL
```
Expected: `ppUnitsOut = 25_000_000_000_000` (= 250,000 PP), one leg, `cappedByInventory = false`. (0.1 SOL × rate / 1e8 = 1e8 × 25e12 / 1e8 = 25e12. ✓)

- [ ] **Step 3: Verify multi-tier spill + inventory cap.**

Run:
```bash
# Shrink tier 0 to only 10,000 PP (1_000_000_000_000 units) so a 0.1 SOL buy spills/caps:
dfx canister call ponzi_math_sol deskUpdateTier '(0 : nat, 25000000000000 : nat, 1000000000000 : nat)'
dfx canister call ponzi_math_sol quoteBuyPP '(100000000 : nat64)'
```
Expected: `ppUnitsOut = 1_000_000_000_000` (only 10,000 PP available), `cappedByInventory = true`.

- [ ] **Step 4: Commit.**

```bash
git add ponzi_math_sol/main.mo
git commit -m "feat(ponzi_math_sol): desk quoteBuyPP tier walk"
```

---

## Task 6: `createBuyIntent` + reservation + buy-intent queries

**Files:**
- Modify: `ponzi_math_sol/main.mo` (reservation helper, `createBuyIntent`, queries)

- [ ] **Step 1: Add a reservation applier + `createBuyIntent` + queries.** Mirrors `prepareSolDeposit` (`:2407`).

```motoko
// Add (+delta) or release (-delta) reserved PP on the named tiers.
func applyReservation(legs : [QuoteLeg], release : Bool) {
    deskTiers := Array.tabulate<DeskTier>(deskTiers.size(), func(i) {
        var t = deskTiers[i];
        for (leg in legs.vals()) {
            if (leg.tierIndex == i) {
                if (release) {
                    t := { t with ppUnitsReserved = (if (t.ppUnitsReserved > leg.ppUnits) { t.ppUnitsReserved - leg.ppUnits } else { 0 }) };
                } else {
                    t := { t with ppUnitsReserved = t.ppUnitsReserved + leg.ppUnits };
                };
            };
        };
        t;
    });
};

public shared ({ caller }) func createBuyIntent(lamports : Nat64) : async {
    #Ok : { intentId : Nat; depositAddress : Text; ppUnitsReserved : Nat; legs : [QuoteLeg]; expiresAt : Int };
    #Err : Text;
} {
    requireAuthenticated(caller);
    if (lamports < MIN_BUY_LAMPORTS) { return #Err("Minimum buy is 0.01 SOL (10,000,000 lamports)") };
    if (not bootstrapped) { return #Err("Canister not bootstrapped yet") };
    acquireCallerLock(caller);
    try {
        let quote = computeQuote(lamports);
        if (quote.ppUnitsOut == 0) { return #Err("Desk has no inventory available") };

        // Ensure escrow can actually back this reservation (balance ≥ reserved + new).
        let bal = await ppLedger.icrc1_balance_of(deskEscrowAccount());
        if (bal < deskReservedTotal() + quote.ppUnitsOut) {
            return #Err("Insufficient desk inventory to reserve this buy");
        };

        // Deposit address (derive if absent), same as prepareSolDeposit.
        let depositAddr = switch (principalMapNat.get(depositAddresses, caller)) {
            case (?a) { a };
            case (null) {
                let addr = await SolSigner.deriveAddress(keyId, derivationPathForPrincipal(caller));
                depositAddresses := principalMapNat.put(depositAddresses, caller, addr);
                addressToPrincipal := textMap.put(addressToPrincipal, addr, caller);
                addr;
            };
        };

        applyReservation(quote.legs, false);
        let now = Time.now();
        let reserved : [BuyReservation] = Array.map<QuoteLeg, BuyReservation>(quote.legs, func(l) {
            { tierIndex = l.tierIndex; ppUnits = l.ppUnits; ratePpUnitsPer0_1Sol = l.ratePpUnitsPer0_1Sol };
        });
        let intent : BuyIntent = {
            id = nextBuyIntentId;
            principal = caller;
            reserved;
            ppUnitsReservedTotal = quote.ppUnitsOut;
            quotedLamports = lamports;
            createdAt = now;
            expiresAt = now + DESK_BUY_INTENT_TTL_NS;
            fulfilled = false;
        };
        pendingBuyIntents := natMap.put(pendingBuyIntents, nextBuyIntentId, intent);
        let intentId = nextBuyIntentId;
        nextBuyIntentId += 1;
        #Ok({ intentId; depositAddress = depositAddr; ppUnitsReserved = quote.ppUnitsOut; legs = quote.legs; expiresAt = intent.expiresAt });
    } finally {
        releaseCallerLock(caller);
    };
};

public query ({ caller }) func getMyPendingBuyIntents() : async [BuyIntent] {
    var out = List.nil<BuyIntent>();
    for (intent in natMap.vals(pendingBuyIntents)) {
        if (intent.principal == caller and not intent.fulfilled) { out := List.push(intent, out) };
    };
    List.toArray(out);
};

public query ({ caller }) func adminGetAllBuyIntents() : async [BuyIntent] {
    requireAdmin(caller);
    Iter.toArray(natMap.vals(pendingBuyIntents));
};
```

- [ ] **Step 2: Deploy + verify reservation decrements available inventory.**

Run (fresh tier with plenty of inventory; needs `bootstrapped = true` — if bootstrap isn't run locally, temporarily relax the `bootstrapped` guard for this test or run the local bootstrap path):
```bash
dfx deploy ponzi_math_sol --mode upgrade --yes
dfx canister call ponzi_math_sol deskReorderTiers '(vec { record { ratePpUnitsPer0_1Sol = 25000000000000 : nat; ppUnitsTotal = 100000000000000 : nat; ppUnitsSold = 0 : nat; ppUnitsReserved = 0 : nat } })'
dfx canister call ponzi_math_sol createBuyIntent '(100000000 : nat64)'      # 0.1 SOL → reserves 250k PP
dfx canister call ponzi_math_sol deskInventory '()'
dfx canister call ponzi_math_sol deskListTiers '()'
dfx canister call ponzi_math_sol getMyPendingBuyIntents '()'
```
Expected: `createBuyIntent` → `Ok` with `ppUnitsReserved = 25_000_000_000_000`, a `depositAddress`, and `expiresAt`; `deskInventory` shows `reservedUnits = 25_000_000_000_000` and `availableUnits` reduced; tier shows `ppUnitsReserved = 25_000_000_000_000`; one pending buy intent.

- [ ] **Step 3: Commit.**

```bash
git add ponzi_math_sol/main.mo
git commit -m "feat(ponzi_math_sol): createBuyIntent reserves desk inventory"
```

---

## Task 7: Settlement — match + proportional fill + PP release

**Files:**
- Modify: `ponzi_math_sol/main.mo` (`settleBuyIntent`, hook in `creditDeposit`, scan-set in `runDetectionForOpenIntents`, test shim)

- [ ] **Step 1: Add `settleBuyIntent` (transfer-first-then-record).**

```motoko
// Credit PP for a matched buy payment. Pure-computes the fill, transfers PP
// from escrow, and ONLY on transfer success mutates tier/intent/accrual + sweeps.
func settleBuyIntent(intent : BuyIntent, inboundLamports : Nat64) : async { #Ok : Nat; #Err : Text } {
    // 1. Pure: walk locked legs, consume inboundLamports → creditPp + per-leg filled.
    var remaining : Nat = Nat64.toNat(inboundLamports);
    var creditPp : Nat = 0;
    let filled = Array.map<BuyReservation, Nat>(intent.reserved, func(leg) {
        let legLamports : Nat = leg.ppUnits * PP_S / leg.ratePpUnitsPer0_1Sol;
        let spend : Nat = Nat.min(remaining, legLamports);
        let ppFilled : Nat = if (spend >= legLamports) { leg.ppUnits } else { spend * leg.ratePpUnitsPer0_1Sol / PP_S };
        remaining -= spend;
        creditPp += ppFilled;
        ppFilled;
    });
    if (creditPp == 0) { return #Err("Zero fill") };

    // 2. Transfer PP escrow → buyer FIRST.
    let xfer = try {
        await ppLedger.icrc1_transfer({
            from_subaccount = ?DESK_ESCROW_SUBACCOUNT;
            to = { owner = intent.principal; subaccount = null };
            amount = creditPp;
            fee = null; memo = null; created_at_time = null;
        });
    } catch (e) { #Err(#GenericError({ error_code = 0; message = Error.message(e) })) };
    switch (xfer) {
        case (#Err(e)) { return #Err("PP transfer failed: " # debug_show (e)) };
        case (#Ok(_)) {};
    };

    // 3. On success: release each leg's full reservation, add filled to sold.
    deskTiers := Array.tabulate<DeskTier>(deskTiers.size(), func(i) {
        var t = deskTiers[i];
        var j : Nat = 0;
        for (leg in intent.reserved.vals()) {
            if (leg.tierIndex == i) {
                let rel = if (t.ppUnitsReserved > leg.ppUnits) { t.ppUnitsReserved - leg.ppUnits } else { 0 };
                t := { t with ppUnitsReserved = rel; ppUnitsSold = t.ppUnitsSold + filled[j] };
            };
            j += 1;
        };
        t;
    });
    pendingBuyIntents := natMap.put(pendingBuyIntents, intent.id, { intent with fulfilled = true });
    deskProceedsAccrualLamports += inboundLamports;
    recordLedger(#deskSale({ buyer = intent.principal; ppUnitsCredited = creditPp; lamportsReceived = Nat64.toNat(inboundLamports); intentId = intent.id }));
    #Ok(creditPp);
};
```

- [ ] **Step 2: Hook buy-intent matching into `creditDeposit`'s no-deposit-match branch.** In `ponzi_math_sol/main.mo`, the `case (null)` arm at `:2249` currently logs the unmatched deposit and advances the cursor. Replace its body's *start* (before the `Debug.print` + cursor advance) with a buy-intent match attempt:

```motoko
            case (null) {
                // Before treating as unmatched: try an open buy intent for this
                // principal, amount within ±5%, not expired.
                var matchedBuy : ?BuyIntent = null;
                for (bi in natMap.vals(pendingBuyIntents)) {
                    if (bi.principal == sig.principal and not bi.fulfilled) {
                        let tol = bpsApply(bi.quotedLamports, 500);
                        let lo : Nat64 = if (bi.quotedLamports > tol) { bi.quotedLamports - tol } else { 0 };
                        let hi : Nat64 = bi.quotedLamports + tol;
                        if (inboundLamports >= lo and inboundLamports <= hi and Time.now() <= bi.expiresAt) {
                            matchedBuy := ?bi;
                        };
                    };
                };
                switch (matchedBuy) {
                    case (?bi) {
                        switch (await settleBuyIntent(bi, inboundLamports)) {
                            case (#Ok(_pp)) {
                                // Sweep buyer's address → pool, accrue handled in settle.
                                switch (await sweepToPool(sig.address, derivationPathForPrincipal(sig.principal), inboundLamports)) {
                                    case (#Err(e)) { Debug.print("Desk sweep failed " # sig.signature # ": " # e) };
                                    case (#Ok(_)) {};
                                };
                                lastSeenSignature := textMap.put(lastSeenSignature, sig.address, sig.signature);
                                return #Ok(null); // not a game; cursor advanced
                            };
                            case (#Err(e)) {
                                // Leave cursor UNADVANCED → retry next tick (e.g. transient ledger error).
                                Debug.print("Desk settle failed " # sig.signature # ": " # e);
                                return #Err(e);
                            };
                        };
                    };
                    case (null) {};
                };
                // ...existing unmatched-deposit handling (Debug.print + cursor advance + return #Ok(null)) unchanged below...
```
(Keep the original `Debug.print(...)`, `lastSeenSignature := ...`, `return #Ok(null)` lines that already exist in this arm — the new block sits above them and only `return`s when a buy intent matched.)

- [ ] **Step 3: Add open buy-intent addresses to the detection scan set.** In `runDetectionForOpenIntents` (`:2513`), after the loop that fills `toScan` from `pendingIntents` (`:2518-2525`), add:

```motoko
        for (bi in natMap.vals(pendingBuyIntents)) {
            if (not bi.fulfilled and now <= bi.expiresAt) {
                switch (principalMapNat.get(depositAddresses, bi.principal)) {
                    case (?addr) { toScan := textMap.put(toScan, addr, bi.principal) };
                    case (null) {};
                };
            };
        };
```

- [ ] **Step 4: Add the `TEST_ADMIN`-gated settlement shim** (so settlement is dfx-testable without devnet RPC).

```motoko
// TEST/diagnostic: drive settleBuyIntent directly with a simulated inbound
// amount, bypassing the Solana observer. Gated to TEST_ADMIN.
public shared ({ caller }) func adminTestSettleBuyIntent(intentId : Nat, inboundLamports : Nat64) : async { #Ok : Nat; #Err : Text } {
    if (caller != TEST_ADMIN) { return #Err("Unauthorized: testAdmin only") };
    switch (natMap.get(pendingBuyIntents, intentId)) {
        case (null) { #Err("No such buy intent") };
        case (?bi) { if (bi.fulfilled) { #Err("Already fulfilled") } else { await settleBuyIntent(bi, inboundLamports) } };
    };
};
```

- [ ] **Step 5: Deploy + verify full settlement (reserve → settle → PP delivered).**

Run (continuing from Task 6's open intent `intentId = 0`, escrow funded, buyer = default identity):
```bash
dfx deploy ponzi_math_sol --mode upgrade --yes
BUYER=$(dfx identity get-principal)
dfx canister call pp_ledger icrc1_balance_of "(record { owner = principal \"$BUYER\"; subaccount = null })"   # before
dfx canister call ponzi_math_sol adminTestSettleBuyIntent '(0 : nat, 100000000 : nat64)'   # full pay 0.1 SOL
dfx canister call pp_ledger icrc1_balance_of "(record { owner = principal \"$BUYER\"; subaccount = null })"   # after
dfx canister call ponzi_math_sol deskInventory '()'
dfx canister call ponzi_math_sol deskListTiers '()'
```
Expected: `adminTestSettleBuyIntent` → `(variant { Ok = 25_000_000_000_000 })`; buyer PP balance increases by 25_000_000_000_000; `deskInventory` `reservedUnits = 0` and `balanceUnits` down by the credited amount; tier `ppUnitsSold = 25_000_000_000_000`, `ppUnitsReserved = 0`.

- [ ] **Step 6: Verify partial fill (underpay credits proportionally).**

Run:
```bash
dfx canister call ponzi_math_sol createBuyIntent '(100000000 : nat64)'     # new intent id=1, reserves 250k PP
dfx canister call ponzi_math_sol adminTestSettleBuyIntent '(1 : nat, 50000000 : nat64)'   # pay only 0.05 SOL
```
Expected: `Ok = 12_500_000_000_000` (half the SOL → half the PP at the locked rate). Inventory reserved returns to 0 (full reservation released; only half sold).

- [ ] **Step 7: Commit.**

```bash
git add ponzi_math_sol/main.mo
git commit -m "feat(ponzi_math_sol): desk settlement via observer + test shim"
```

---

## Task 8: Reservation expiry, proceeds withdrawal, refund, stats

**Files:**
- Modify: `ponzi_math_sol/main.mo` (expiry sweep, proceeds withdraw, refund, stats query; call expiry from detection tick)

- [ ] **Step 1: Add expired-reservation release + call it from the detection pass.**

```motoko
// Release reservations for expired, unfulfilled buy intents (returns PP to the pool).
func releaseExpiredBuyIntents() {
    let now = Time.now();
    for (bi in natMap.vals(pendingBuyIntents)) {
        if (not bi.fulfilled and now > bi.expiresAt) {
            let legs = Array.map<BuyReservation, QuoteLeg>(bi.reserved, func(r) {
                { tierIndex = r.tierIndex; ppUnits = r.ppUnits; lamports = 0 : Nat64; ratePpUnitsPer0_1Sol = r.ratePpUnitsPer0_1Sol };
            });
            applyReservation(legs, true);
            pendingBuyIntents := natMap.put(pendingBuyIntents, bi.id, { bi with fulfilled = true });
        };
    };
};
```
In `runDetectionForOpenIntents` (`:2513`), add `releaseExpiredBuyIntents();` as the first statement (before building `toScan`).

- [ ] **Step 2: Add proceeds withdrawal + admin refund + stats.**

```motoko
// Withdraw accrued desk SOL revenue from the pool to Charles's address.
public shared ({ caller }) func adminWithdrawDeskProceeds(toAddress : Text) : async { #Ok : Text; #Err : Text } {
    requireAdmin(caller);
    if (deskProceedsAccrualLamports == 0) { return #Err("No desk proceeds to withdraw") };
    let amount = deskProceedsAccrualLamports;
    switch (await sendSolPayout(toAddress, amount)) {
        case (#Ok(txSig)) {
            deskProceedsAccrualLamports := 0;
            recordLedger(#deskProceedsWithdrawal({ toAddress; lamports = Nat64.toNat(amount); txSig }));
            #Ok(txSig);
        };
        case (#Err(e)) { #Err(e) };
    };
};

// Refund SOL from the pool to a buyer-provided address (excess/late payments).
// The observer cannot extract the sender, so Charles supplies the address
// (from support contact). Bounded by accrued proceeds.
public shared ({ caller }) func adminRefundDeskSol(toAddress : Text, lamports : Nat64) : async { #Ok : Text; #Err : Text } {
    requireAdmin(caller);
    if (lamports == 0 or lamports > deskProceedsAccrualLamports) { return #Err("Amount exceeds accrued desk proceeds") };
    switch (await sendSolPayout(toAddress, lamports)) {
        case (#Ok(txSig)) {
            deskProceedsAccrualLamports -= lamports;
            recordLedger(#deskProceedsWithdrawal({ toAddress; lamports = Nat64.toNat(lamports); txSig }));
            #Ok(txSig);
        };
        case (#Err(e)) { #Err(e) };
    };
};

public func deskStats() : async {
    inventoryUnits : Nat; reservedUnits : Nat; availableUnits : Nat;
    proceedsLamports : Nat; openBuyIntents : Nat; tierCount : Nat; totalSoldUnits : Nat;
} {
    let bal = await ppLedger.icrc1_balance_of(deskEscrowAccount());
    let reserved = deskReservedTotal();
    var open : Nat = 0;
    for (bi in natMap.vals(pendingBuyIntents)) { if (not bi.fulfilled) { open += 1 } };
    var sold : Nat = 0;
    for (t in deskTiers.vals()) { sold += t.ppUnitsSold };
    {
        inventoryUnits = bal; reservedUnits = reserved;
        availableUnits = if (bal > reserved) { bal - reserved } else { 0 };
        proceedsLamports = Nat64.toNat(deskProceedsAccrualLamports);
        openBuyIntents = open; tierCount = deskTiers.size(); totalSoldUnits = sold;
    };
};
```

- [ ] **Step 3: Deploy + verify proceeds accrual + stats.**

Run (after Task 7, proceeds accrued from the two settlements = 0.1 + 0.05 SOL = 150_000_000 lamports):
```bash
dfx deploy ponzi_math_sol --mode upgrade --yes
dfx canister call ponzi_math_sol deskStats '()'
```
Expected: `proceedsLamports = 150_000_000`, `totalSoldUnits = 37_500_000_000_000` (25e12 + 12.5e12), `openBuyIntents = 0`.
(`adminWithdrawDeskProceeds` broadcasts a real Solana tx and can't complete on a local replica without the SOL pipeline bootstrapped; verify it returns a structured `#Err` from `sendSolPayout` rather than trapping. Full proceeds withdrawal is covered by the live devnet smoke test in Task 9.)

- [ ] **Step 4: Commit.**

```bash
git add ponzi_math_sol/main.mo
git commit -m "feat(ponzi_math_sol): desk reservation expiry, proceeds withdrawal, refund, stats"
```

---

## Task 9: Regenerate declarations + full verification matrix + commit

**Files:**
- Regenerate: `frontend/src/declarations/ponzi_math_sol/*`

- [ ] **Step 1: Regenerate candid/TS declarations.**

Run: `npm run generate`
Expected: succeeds; `frontend/src/declarations/ponzi_math_sol/ponzi_math_sol.did` now contains `deskAddTier`, `quoteBuyPP`, `createBuyIntent`, `deskInventory`, `deskStats`, `getMyPendingBuyIntents`, etc.

- [ ] **Step 2: Run the consolidated local verification matrix** (fresh replica) and confirm each expected output from Tasks 3–8 in sequence: tier CRUD → inventory deposit → quote (single, multi-tier cap) → createBuyIntent reserve → adminTestSettleBuyIntent full + partial → deskStats. Capture the outputs.

- [ ] **Step 3: (Optional, operator-run) Devnet smoke test.** Against the deployed devnet `ponzi_math_sol`: stock a small inventory, add one tier, `createBuyIntent` for 0.01 SOL, send devnet SOL to the returned address, wait ≤60s for the detection timer, confirm PP arrives at the buyer principal and `deskStats.proceedsLamports` increases, then `adminWithdrawDeskProceeds` to a Phantom address. (Backend deploy to devnet/mainnet only with explicit operator permission — see migration note below.)

- [ ] **Step 4: Commit the regenerated declarations.**

```bash
git add frontend/src/declarations/ponzi_math_sol
git commit -m "chore(ponzi_math_sol): regenerate declarations for desk methods"
```

---

## Deploy / migration note

These changes are **pure-additive** (new stable `var`s `deskTiers`/`pendingBuyIntents`/`nextBuyIntentId`/`deskProceedsAccrualLamports`, new `GeneralLedgerEvent` arms, new methods) — no existing stored record is modified, so **no migration module is required** and `ponzi_math_sol` has none. Per the project's deploy-safety rule, **deploy to devnet/mainnet only with explicit operator permission**; `ponzi_math_sol` is live at `spc6q-xyaaa-aaaac-qg2ma-cai`, so rebase onto the current deployed commit before any upgrade.

## Self-review

- **Spec coverage:** escrow custody (Tasks 1,4) ✓; laddered PP-per-0.1-SOL tiers + admin management (Tasks 2,3) ✓; quote walk (Task 5) ✓; rate-locked reservation + buy intent (Task 6) ✓; observer settlement with proportional under-pay fill (Task 7) ✓; reservation TTL release, proceeds withdrawal, refund, stats (Task 8) ✓; declarations for the frontend plan (Task 9) ✓. Sell-only/no-oracle/PP-per-SOL all honored.
- **Deviations from spec (discovered during planning, intentional):** (1) **No migration** — additive state, contradicting the spec's "requires a migration" note (spec corrected). (2) **Refunds are admin-directed to a provided address**, not auto-to-sender — the observer deliberately does not decode Solana `accountKeys`, so the sender pubkey isn't available (spec corrected). (3) Buy intents are a **separate `pendingBuyIntents` map**, not a new arm on `DepositIntent`, which is what keeps it migration-free.
- **Type consistency:** `ratePpUnitsPer0_1Sol`, `ppUnitsTotal/Sold/Reserved`, `QuoteLeg`, `BuyReservation`, `BuyIntent`, `ppUnitsReservedTotal`, `deskProceedsAccrualLamports`, `PP_S` used identically across Tasks 2–8. `computeQuote`/`applyReservation`/`settleBuyIntent`/`deskReservedTotal` signatures match their call sites.
- **Placeholder scan:** none — every step carries complete Motoko and exact `dfx` commands with expected output. The two "operator-run / optional" steps (devnet smoke, proceeds-withdrawal broadcast) are explicitly out of local-replica reach, not placeholders.

## Frontend follow-up (separate plan, after this lands)

`useBuyPpDesk.ts` (quote/createBuyIntent hooks mirroring `usePartyDexBuy.ts`), `BuyPpDeskFlyout.tsx` replacing `BuySOLFlyout` in the `BuySOLWidget`/`BuySOLFab` slot, and the `PpDeskPanel.tsx` admin UI in Charles's office — all written against the candid generated in Task 9.
