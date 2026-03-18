# Shenanigans Canister Migration Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract all shenanigans logic from the main backend canister into a standalone shenanigans canister so the main backend can be blackholed while shenanigans remain updatable.

**Architecture:** Create a new Motoko canister (`shenanigans`) that owns all shenanigan state (configs, records, stats) and logic (casting, outcomes, backfires). The shenanigans canister makes inter-canister calls to the main backend to read/write ponzi points and dealer repayments — the main backend exposes thin API endpoints for these mutations. The frontend creates a second actor for the shenanigans canister and routes shenanigan queries/mutations there.

**Tech Stack:** Motoko (IC canister), React/TypeScript (frontend), dfx (deployment)

---

## Key Design Decision: Cross-Canister Point Mutations

`castShenanigan` currently mutates three pieces of state owned by the main backend:
1. `ponziPoints` — deducts cost from caster, transfers points on backfire
2. `ponziPointsBurned` — tracks burned PP
3. `dealerRepayments` — 10% of shenanigan cost goes to dealers

**Approach:** The main backend exposes three new public functions callable only by the shenanigans canister:
- `deductPonziPoints(user, amount)` — deduct PP from a user
- `transferPonziPoints(from, to, amount)` — move PP between users
- `distributeDealerCut(amount)` — distribute amount among active dealers
- `getPonziPointsBalance(user)` — read a user's PP balance

The shenanigans canister is authorized by principal check (its canister ID is hardcoded or configured in the main backend).

---

## File Structure

### New Files
| File | Responsibility |
|------|---------------|
| `shenanigans/main.mo` | Shenanigans canister — all shenanigan state, configs, casting logic, outcome determination, stats tracking, admin config management |
| `frontend/src/hooks/useShenaniganActor.ts` | Actor hook for the shenanigans canister (parallel to existing `useActor` for backend) |
| `frontend/src/declarations/shenanigans/` | Auto-generated Candid declarations (created by `dfx generate`) |

### Modified Files
| File | Changes |
|------|---------|
| `dfx.json` | Add `shenanigans` canister entry |
| `backend/main.mo` | Remove all shenanigan logic; add thin PP mutation API for cross-canister calls; add shenanigans canister authorization |
| `frontend/src/hooks/useQueries.ts` | Repoint shenanigan hooks to new shenanigans actor |
| `frontend/src/backend.ts` | Export shenanigans types from new declarations |
| `frontend/src/components/Shenanigans.tsx` | No changes needed (hooks handle routing) |
| `frontend/src/components/ShenanigansAdminPanel.tsx` | No changes needed (hooks handle routing) |

---

## Chunk 1: Backend — New Shenanigans Canister + Backend API

### Task 1: Add shenanigans canister to dfx.json

**Files:**
- Modify: `dfx.json`

- [ ] **Step 1: Add the shenanigans canister entry**

Add to `dfx.json` canisters object:

```json
"shenanigans": {
  "main": "shenanigans/main.mo",
  "type": "motoko"
}
```

- [ ] **Step 2: Create the shenanigans directory**

```bash
mkdir -p shenanigans
```

- [ ] **Step 3: Commit**

```bash
git add dfx.json
git commit -m "chore: add shenanigans canister to dfx.json"
```

---

### Task 2: Create the shenanigans canister

**Files:**
- Create: `shenanigans/main.mo`

This is the bulk of the migration. The canister contains:
- All shenanigan types (ShenaniganType, ShenaniganOutcome, ShenaniganRecord, ShenaniganStats, ShenaniganConfig)
- All shenanigan state (configs, records, stats, nextId)
- Default config initialization
- `castShenanigan` — the main casting function (makes inter-canister calls to backend for PP mutations)
- `determineOutcome` — RNG-based outcome logic
- `updateShenaniganStats` — per-user stat tracking
- Query functions: `getShenaniganStats`, `getRecentShenanigans`, `getShenaniganConfigs`
- Admin functions: `updateShenaniganConfig`, `resetShenaniganConfig`, `saveAllShenaniganConfigs`
- Its own access control (admin = first caller to initialize, same pattern as backend)

- [ ] **Step 1: Write the shenanigans canister**

```motoko
import OrderedMap "mo:base/OrderedMap";
import Principal "mo:base/Principal";
import Debug "mo:base/Debug";
import Time "mo:base/Time";
import Float "mo:base/Float";
import Int "mo:base/Int";
import Iter "mo:base/Iter";
import List "mo:base/List";

persistent actor {

    // ================================================================
    // Types
    // ================================================================

    public type ShenaniganType = {
        #moneyTrickster;
        #aoeSkim;
        #renameSpell;
        #mintTaxSiphon;
        #downlineHeist;
        #magicMirror;
        #ppBoosterAura;
        #purseCutter;
        #whaleRebalance;
        #downlineBoost;
        #goldenName;
    };

    public type ShenaniganOutcome = {
        #success;
        #fail;
        #backfire;
    };

    public type ShenaniganRecord = {
        id : Nat;
        user : Principal;
        shenaniganType : ShenaniganType;
        target : ?Principal;
        outcome : ShenaniganOutcome;
        timestamp : Int;
        cost : Float;
    };

    public type ShenaniganStats = {
        totalSpent : Float;
        totalCast : Nat;
        goodOutcomes : Nat;
        badOutcomes : Nat;
        backfires : Nat;
        dealerCut : Float;
    };

    public type ShenaniganConfig = {
        id : Nat;
        name : Text;
        description : Text;
        cost : Float;
        successOdds : Nat;
        failureOdds : Nat;
        backfireOdds : Nat;
        duration : Nat;
        cooldown : Nat;
        effectValues : [Float];
        castLimit : Nat;
        backgroundColor : Text;
    };

    // ================================================================
    // Backend canister interface (for cross-canister calls)
    // ================================================================

    type BackendActor = actor {
        deductPonziPoints : shared (user : Principal, amount : Float) -> async ();
        transferPonziPoints : shared (from : Principal, to : Principal, amount : Float) -> async ();
        distributeDealerCut : shared (amount : Float) -> async ();
        getPonziPointsBalance : shared query (user : Principal) -> async Float;
        burnPonziPoints : shared (user : Principal, amount : Float) -> async ();
    };

    // ================================================================
    // State
    // ================================================================

    transient let natMap = OrderedMap.Make<Nat>(Nat.compare);
    transient let principalMap = OrderedMap.Make<Principal>(Principal.compare);

    transient var shenanigans = natMap.empty<ShenaniganRecord>();
    transient var shenaniganStats = principalMap.empty<ShenaniganStats>();
    transient var nextShenaniganId = 0;
    transient var shenaniganConfigs = natMap.empty<ShenaniganConfig>();

    // Admin state
    transient var adminPrincipal : ?Principal = null;

    // Backend canister principal (set by admin after deployment)
    transient var backendPrincipal : ?Principal = null;

    // ================================================================
    // Initialization
    // ================================================================

    public shared ({ caller }) func initialize(backendCanisterId : Principal) : async () {
        switch (adminPrincipal) {
            case (null) {
                adminPrincipal := ?caller;
                backendPrincipal := ?backendCanisterId;
                initializeDefaultShenanigans();
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
                if (caller != admin) {
                    Debug.trap("Unauthorized: admin only");
                };
            };
        };
    };

    func getBackend() : BackendActor {
        switch (backendPrincipal) {
            case (null) { Debug.trap("Backend canister not configured") };
            case (?p) { actor (Principal.toText(p)) : BackendActor };
        };
    };

    // ================================================================
    // Default configs (identical to current backend)
    // ================================================================

    func initializeDefaultShenanigans() {
        let defaultConfigs : [ShenaniganConfig] = [
            { id = 0; name = "Money Trickster"; description = "Steals 2–8% of target's Ponzi Points (max 250 PP)."; cost = 120.0; successOdds = 60; failureOdds = 25; backfireOdds = 15; duration = 0; cooldown = 2; effectValues = [2.0, 8.0, 250.0]; castLimit = 0; backgroundColor = "#fff9e6" },
            { id = 1; name = "AOE Skim"; description = "Siphons 1–3% from each player (max 60 PP per player)."; cost = 600.0; successOdds = 40; failureOdds = 40; backfireOdds = 20; duration = 0; cooldown = 0; effectValues = [1.0, 3.0, 60.0]; castLimit = 1; backgroundColor = "#e6f7ff" },
            { id = 2; name = "Rename Spell"; description = "Changes target's display name for 7 days."; cost = 200.0; successOdds = 90; failureOdds = 5; backfireOdds = 5; duration = 168; cooldown = 0; effectValues = [7.0]; castLimit = 0; backgroundColor = "#ffe6f7" },
            { id = 3; name = "Mint Tax Siphon"; description = "Skims 5% of target's new PP for 7 days (max 1000 PP)."; cost = 1200.0; successOdds = 70; failureOdds = 20; backfireOdds = 10; duration = 168; cooldown = 0; effectValues = [5.0, 1000.0]; castLimit = 0; backgroundColor = "#f3e6ff" },
            { id = 4; name = "Downline Heist"; description = "Steals one downline member (favor L3)."; cost = 500.0; successOdds = 30; failureOdds = 60; backfireOdds = 10; duration = 0; cooldown = 0; effectValues = []; castLimit = 1; backgroundColor = "#e6fff2" },
            { id = 5; name = "Magic Mirror"; description = "Equips shield (blocks one hostile shenanigan)."; cost = 200.0; successOdds = 100; failureOdds = 0; backfireOdds = 0; duration = 0; cooldown = 0; effectValues = []; castLimit = 2; backgroundColor = "#fff4e6" },
            { id = 6; name = "PP Booster Aura"; description = "Earn +5–15% additional PP for rest of round."; cost = 300.0; successOdds = 100; failureOdds = 0; backfireOdds = 0; duration = 0; cooldown = 0; effectValues = [5.0, 15.0]; castLimit = 1; backgroundColor = "#e6f2ff" },
            { id = 7; name = "Purse Cutter"; description = "Target loses 25–50% PP (max 800 PP)."; cost = 900.0; successOdds = 20; failureOdds = 50; backfireOdds = 30; duration = 0; cooldown = 0; effectValues = [25.0, 50.0, 800.0]; castLimit = 0; backgroundColor = "#ffe6e6" },
            { id = 8; name = "Whale Rebalance"; description = "Takes 20% from top 3 holders (max 300 PP/whale)."; cost = 800.0; successOdds = 50; failureOdds = 30; backfireOdds = 20; duration = 0; cooldown = 0; effectValues = [20.0, 300.0]; castLimit = 0; backgroundColor = "#f0e6ff" },
            { id = 9; name = "Downline Boost"; description = "Downline referrals kick up 1.3x PP for rest of round."; cost = 400.0; successOdds = 100; failureOdds = 0; backfireOdds = 0; duration = 0; cooldown = 0; effectValues = [1.3]; castLimit = 1; backgroundColor = "#e6fffa" },
            { id = 10; name = "Golden Name"; description = "Gives gold name on leaderboard (24h or 7d)."; cost = 100.0; successOdds = 100; failureOdds = 0; backfireOdds = 0; duration = 24; cooldown = 0; effectValues = [24.0, 168.0]; castLimit = 1; backgroundColor = "#fff0e6" },
        ];
        for (config in defaultConfigs.vals()) {
            shenaniganConfigs := natMap.put(shenaniganConfigs, config.id, config);
        };
    };

    // ================================================================
    // Core Logic
    // ================================================================

    public shared ({ caller }) func castShenanigan(shenaniganType : ShenaniganType, target : ?Principal) : async ShenaniganOutcome {
        let backend = getBackend();

        let cost = switch (shenaniganType) {
            case (#moneyTrickster) { 120.0 };
            case (#aoeSkim) { 600.0 };
            case (#renameSpell) { 200.0 };
            case (#mintTaxSiphon) { 1200.0 };
            case (#downlineHeist) { 500.0 };
            case (#magicMirror) { 200.0 };
            case (#ppBoosterAura) { 300.0 };
            case (#purseCutter) { 900.0 };
            case (#whaleRebalance) { 800.0 };
            case (#downlineBoost) { 400.0 };
            case (#goldenName) { 100.0 };
        };

        // Check balance via cross-canister call
        let userPoints = await backend.getPonziPointsBalance(caller);
        if (userPoints < cost) {
            Debug.trap("Insufficient Ponzi Points to cast this shenanigan");
        };

        // Deduct cost
        await backend.deductPonziPoints(caller, cost);

        // Track burn
        await backend.burnPonziPoints(caller, cost);

        // Determine outcome
        let outcome = determineOutcome(shenaniganType);

        // Apply backfire effects
        if (outcome == #backfire) {
            switch (shenaniganType) {
                case (#moneyTrickster) {
                    switch (target) {
                        case (null) {};
                        case (?targetPrincipal) {
                            let casterPoints = await backend.getPonziPointsBalance(caller);
                            let lossPercentage = 0.02 + (Float.fromInt(Int.abs(Time.now()) % 7) / 100.0);
                            let lossAmount = casterPoints * lossPercentage;
                            let cappedLoss = Float.min(lossAmount, 250.0);
                            await backend.transferPonziPoints(caller, targetPrincipal, cappedLoss);
                        };
                    };
                };
                case (#aoeSkim) {
                    let casterPoints = await backend.getPonziPointsBalance(caller);
                    let lossPercentage = 0.01 + (Float.fromInt(Int.abs(Time.now()) % 3) / 100.0);
                    let lossAmount = casterPoints * lossPercentage;
                    // For AoE backfire, just deduct from caster (distributing to all players
                    // requires iterating all PP holders which is complex cross-canister;
                    // simplify to a burn for now)
                    await backend.deductPonziPoints(caller, lossAmount);
                };
                case (#downlineHeist) {
                    switch (target) {
                        case (null) {};
                        case (?targetPrincipal) {
                            Debug.print("Backfire: " # Principal.toText(caller) # " loses L3 downline to " # Principal.toText(targetPrincipal));
                        };
                    };
                };
                case (_) {};
            };
        };

        // Record shenanigan
        let shenaniganId = nextShenaniganId;
        nextShenaniganId += 1;

        let newShenanigan : ShenaniganRecord = {
            id = shenaniganId;
            user = caller;
            shenaniganType;
            target;
            outcome;
            timestamp = Time.now();
            cost;
        };
        shenanigans := natMap.put(shenanigans, shenaniganId, newShenanigan);

        // Update stats
        updateShenaniganStats(caller, cost, outcome);

        // Dealer cut (10% of cost)
        let dealerCut = cost * 0.1;
        await backend.distributeDealerCut(dealerCut);

        outcome;
    };

    func determineOutcome(shenaniganType : ShenaniganType) : ShenaniganOutcome {
        let randomValue = Int.abs(Time.now()) % 100;
        switch (shenaniganType) {
            case (#moneyTrickster) { if (randomValue < 60) #success else if (randomValue < 85) #fail else #backfire };
            case (#aoeSkim) { if (randomValue < 40) #success else if (randomValue < 80) #fail else #backfire };
            case (#renameSpell) { if (randomValue < 90) #success else if (randomValue < 95) #fail else #backfire };
            case (#mintTaxSiphon) { if (randomValue < 70) #success else if (randomValue < 90) #fail else #backfire };
            case (#downlineHeist) { if (randomValue < 30) #success else if (randomValue < 90) #fail else #backfire };
            case (#magicMirror) { #success };
            case (#ppBoosterAura) { #success };
            case (#purseCutter) { if (randomValue < 20) #success else if (randomValue < 70) #fail else #backfire };
            case (#whaleRebalance) { if (randomValue < 50) #success else if (randomValue < 80) #fail else #backfire };
            case (#downlineBoost) { #success };
            case (#goldenName) { #success };
        };
    };

    func updateShenaniganStats(user : Principal, cost : Float, outcome : ShenaniganOutcome) {
        let currentStats = switch (principalMap.get(shenaniganStats, user)) {
            case (null) { { totalSpent = 0.0; totalCast = 0; goodOutcomes = 0; badOutcomes = 0; backfires = 0; dealerCut = 0.0 } };
            case (?stats) { stats };
        };
        let updatedStats = {
            currentStats with
            totalSpent = currentStats.totalSpent + cost;
            totalCast = currentStats.totalCast + 1;
            goodOutcomes = currentStats.goodOutcomes + (if (outcome == #success) 1 else 0);
            badOutcomes = currentStats.badOutcomes + (if (outcome == #fail) 1 else 0);
            backfires = currentStats.backfires + (if (outcome == #backfire) 1 else 0);
            dealerCut = currentStats.dealerCut + (cost * 0.1);
        };
        shenaniganStats := principalMap.put(shenaniganStats, user, updatedStats);
    };

    // ================================================================
    // Query Functions
    // ================================================================

    public query ({ caller }) func getShenaniganStats() : async ShenaniganStats {
        switch (principalMap.get(shenaniganStats, caller)) {
            case (null) { { totalSpent = 0.0; totalCast = 0; goodOutcomes = 0; badOutcomes = 0; backfires = 0; dealerCut = 0.0 } };
            case (?stats) { stats };
        };
    };

    public query func getRecentShenanigans() : async [ShenaniganRecord] {
        let allShenanigans = Iter.toArray(natMap.vals(shenanigans));
        let sorted = List.fromArray(allShenanigans);
        let recent = List.take(sorted, 12);
        List.toArray(recent);
    };

    public query func getShenaniganConfigs() : async [ShenaniganConfig] {
        Iter.toArray(natMap.vals(shenaniganConfigs));
    };

    // ================================================================
    // Admin Functions
    // ================================================================

    public shared ({ caller }) func updateShenaniganConfig(config : ShenaniganConfig) : async () {
        requireAdmin(caller);
        if (config.successOdds + config.failureOdds + config.backfireOdds != 100) {
            Debug.trap("Success, failure, and backfire odds must sum to 100");
        };
        if (config.cost < 0.0 or config.duration < 0 or config.cooldown < 0 or config.castLimit < 0) {
            Debug.trap("Cost, duration, cooldown, and cast limit must be non-negative");
        };
        shenaniganConfigs := natMap.put(shenaniganConfigs, config.id, config);
    };

    public shared ({ caller }) func resetShenaniganConfig(id : Nat) : async () {
        requireAdmin(caller);
        // Re-initialize all defaults and pick the one matching id
        initializeDefaultShenanigans();
    };

    public shared ({ caller }) func saveAllShenaniganConfigs(configs : [ShenaniganConfig]) : async () {
        requireAdmin(caller);
        for (config in configs.vals()) {
            if (config.successOdds + config.failureOdds + config.backfireOdds != 100) {
                Debug.trap("Success, failure, and backfire odds must sum to 100");
            };
            if (config.cost < 0.0 or config.duration < 0 or config.cooldown < 0 or config.castLimit < 0) {
                Debug.trap("Cost, duration, cooldown, and cast limit must be non-negative");
            };
            shenaniganConfigs := natMap.put(shenaniganConfigs, config.id, config);
        };
    };
};
```

- [ ] **Step 2: Verify the canister compiles**

```bash
dfx build shenanigans
```

Expected: Successful build with no errors.

- [ ] **Step 3: Commit**

```bash
git add shenanigans/main.mo
git commit -m "feat: create standalone shenanigans canister"
```

---

### Task 3: Add cross-canister API to the main backend

**Files:**
- Modify: `backend/main.mo`

The main backend needs to expose thin endpoints that the shenanigans canister calls to mutate ponzi points and dealer repayments. These must be secured so only the shenanigans canister can call them.

- [ ] **Step 1: Add shenanigans canister authorization and API endpoints**

Add a new state variable near the top of the actor (after `canisterPrincipal`):

```motoko
// Authorized shenanigans canister principal
transient var shenanigansPrincipal : ?Principal = null;
```

Add an admin function to set it:

```motoko
// Set the shenanigans canister principal (admin only)
public shared ({ caller }) func setShenanigansPrincipal(p : Principal) : async () {
    if (not (AccessControl.hasPermission(accessControlState, caller, #admin))) {
        Debug.trap("Unauthorized: Only admins can set shenanigans principal");
    };
    shenanigansPrincipal := ?p;
};
```

Add a helper to enforce shenanigans-only access:

```motoko
func requireShenanigansCanister(caller : Principal) {
    switch (shenanigansPrincipal) {
        case (null) { Debug.trap("Shenanigans canister not configured") };
        case (?p) {
            if (caller != p) {
                Debug.trap("Unauthorized: only shenanigans canister can call this");
            };
        };
    };
};
```

Add the cross-canister API endpoints:

```motoko
// === Cross-canister API for Shenanigans canister ===

public shared ({ caller }) func deductPonziPoints(user : Principal, amount : Float) : async () {
    requireShenanigansCanister(caller);
    let current = switch (principalMapNat.get(ponziPoints, user)) {
        case (null) { 0.0 };
        case (?points) { points };
    };
    if (current < amount) { Debug.trap("Insufficient points") };
    ponziPoints := principalMapNat.put(ponziPoints, user, current - amount);
};

public shared ({ caller }) func transferPonziPoints(from : Principal, to : Principal, amount : Float) : async () {
    requireShenanigansCanister(caller);
    let fromBalance = switch (principalMapNat.get(ponziPoints, from)) {
        case (null) { 0.0 };
        case (?points) { points };
    };
    if (fromBalance < amount) { Debug.trap("Insufficient points") };
    ponziPoints := principalMapNat.put(ponziPoints, from, fromBalance - amount);
    let toBalance = switch (principalMapNat.get(ponziPoints, to)) {
        case (null) { 0.0 };
        case (?points) { points };
    };
    ponziPoints := principalMapNat.put(ponziPoints, to, toBalance + amount);
};

public shared ({ caller }) func distributeDealerCut(amount : Float) : async () {
    requireShenanigansCanister(caller);
    updateDealerCut(amount);
};

public shared query ({ caller }) func getPonziPointsBalance(user : Principal) : async Float {
    switch (principalMapNat.get(ponziPoints, user)) {
        case (null) { 0.0 };
        case (?points) { points };
    };
};

public shared ({ caller }) func burnPonziPoints(user : Principal, amount : Float) : async () {
    requireShenanigansCanister(caller);
    let burned = switch (principalMapNat.get(ponziPointsBurned, user)) {
        case (null) { 0.0 };
        case (?existing) { existing };
    };
    ponziPointsBurned := principalMapNat.put(ponziPointsBurned, user, burned + amount);
};
```

Note: `getPonziPointsBalance` does NOT have the `requireShenanigansCanister` guard — it's a read-only query that's fine to be public. But it does need to not require `caller` auth since the shenanigans canister passes a `user` param. The existing `getPonziPoints` function uses `caller` directly, so this is a separate endpoint.

- [ ] **Step 2: Remove all shenanigan logic from the backend**

Remove these from `backend/main.mo`:
1. The `ShenaniganType`, `ShenaniganOutcome`, `ShenaniganRecord`, `ShenaniganStats`, `ShenaniganConfig` type definitions (lines ~125-182)
2. State variables: `shenanigans`, `shenaniganStats`, `nextShenaniganId`, `shenaniganConfigs` (lines ~214-216, ~283)
3. `initializeDefaultShenanigans()` function and its call (lines ~286-450)
4. `castShenanigan` function (lines ~1457-1594)
5. `determineOutcome` function (lines ~1597-1643)
6. `updateShenaniganStats` function (lines ~1646-1672)
7. `getShenaniganStats` function (lines ~1692-1708)
8. `getRecentShenanigans` function (lines ~1709-1717)
9. `getShenaniganConfigs` function (lines ~1873-1875)
10. `updateShenaniganConfig` function (lines ~1878-1894)
11. `resetShenaniganConfig` function (lines ~1897-2069)
12. `saveAllShenaniganConfigs` function (lines ~2072-2090)

**Keep** `updateDealerCut` — it's still used by the cross-canister API and may be used elsewhere.

- [ ] **Step 3: Verify the backend compiles**

```bash
dfx build backend
```

Expected: Successful build with no errors.

- [ ] **Step 4: Commit**

```bash
git add backend/main.mo
git commit -m "refactor: strip shenanigan logic from backend, add cross-canister PP API"
```

---

## Chunk 2: Frontend — Rewire to New Canister

### Task 4: Generate declarations and create shenanigans actor hook

**Files:**
- Create: `frontend/src/hooks/useShenaniganActor.ts`
- Modify: `frontend/src/backend.ts`
- Auto-generated: `frontend/src/declarations/shenanigans/`

- [ ] **Step 1: Deploy shenanigans canister to generate declarations**

```bash
dfx deploy --network ic shenanigans
```

This creates the canister and generates `src/declarations/shenanigans/`.

- [ ] **Step 2: Initialize the shenanigans canister**

After deploying, call `initialize` with the backend canister ID:

```bash
dfx canister call --network ic shenanigans initialize '(principal "5zxxg-tyaaa-aaaac-qeckq-cai")'
```

- [ ] **Step 3: Set the shenanigans principal in the backend**

Get the shenanigans canister ID and tell the backend about it:

```bash
SHEN_ID=$(dfx canister id --network ic shenanigans)
dfx canister call --network ic backend setShenanigansPrincipal "(principal \"$SHEN_ID\")"
```

- [ ] **Step 4: Create the shenanigans actor hook**

```typescript
// frontend/src/hooks/useShenaniganActor.ts
import { useState, useEffect } from 'react';
import { Actor, HttpAgent } from '@dfinity/agent';
import { useAuth } from './useAuth';

// Import the generated IDL factory and canister ID
// These will be created by `dfx generate shenanigans`
import { idlFactory } from '../declarations/shenanigans';

const SHENANIGANS_CANISTER_ID = import.meta.env.VITE_SHENANIGANS_CANISTER_ID
  || process.env.CANISTER_ID_SHENANIGANS
  || ''; // Will be set after first deploy

export function useShenaniganActor() {
  const { identity, isAuthenticated } = useAuth();
  const [actor, setActor] = useState<any>(null);
  const [isFetching, setIsFetching] = useState(true);

  useEffect(() => {
    async function createActor() {
      if (!isAuthenticated || !identity || !SHENANIGANS_CANISTER_ID) {
        setActor(null);
        setIsFetching(false);
        return;
      }

      try {
        const agent = await HttpAgent.create({
          identity,
          host: import.meta.env.VITE_IC_HOST || 'https://icp0.io',
        });

        const shenaniganActor = Actor.createActor(idlFactory, {
          agent,
          canisterId: SHENANIGANS_CANISTER_ID,
        });

        setActor(shenaniganActor);
      } catch (err) {
        console.error('Failed to create shenanigans actor:', err);
        setActor(null);
      } finally {
        setIsFetching(false);
      }
    }

    createActor();
  }, [identity, isAuthenticated]);

  return { actor, isFetching };
}
```

Note: Check how the existing `useActor` hook works and mirror its pattern exactly (it may use `@dfinity/agent` differently, or use a context provider). Adapt this file to match.

- [ ] **Step 5: Update frontend/src/backend.ts to export shenanigan types from new declarations**

The existing `backend.ts` re-exports types. Add shenanigan types from the new declarations:

```typescript
// Add to backend.ts
export type {
  ShenaniganType,
  ShenaniganOutcome,
  ShenaniganRecord,
  ShenaniganStats,
  ShenaniganConfig,
} from './declarations/shenanigans/shenanigans.did.d.ts';
```

Remove the shenanigan type exports that currently come from the backend declarations (if they exist there).

- [ ] **Step 6: Commit**

```bash
git add frontend/src/hooks/useShenaniganActor.ts frontend/src/backend.ts frontend/src/declarations/shenanigans/
git commit -m "feat: add shenanigans actor hook and declarations"
```

---

### Task 5: Rewire shenanigan query hooks to new actor

**Files:**
- Modify: `frontend/src/hooks/useQueries.ts`

- [ ] **Step 1: Update all shenanigan hooks to use the shenanigans actor**

Replace the shenanigan hook implementations. Each one needs to import and use `useShenaniganActor` instead of `useActor`:

```typescript
import { useShenaniganActor } from './useShenaniganActor';

export function useGetShenaniganStats() {
  const { actor, isFetching: actorFetching } = useShenaniganActor();

  return useQuery<ShenaniganStats>({
    queryKey: ['shenaniganStats'],
    queryFn: async () => {
      if (!actor) throw new Error('Shenanigans actor not available');
      return actor.getShenaniganStats();
    },
    enabled: !!actor && !actorFetching,
    refetchInterval: 5000,
  });
}

export function useGetRecentShenanigans() {
  const { actor, isFetching: actorFetching } = useShenaniganActor();

  return useQuery<ShenaniganRecord[]>({
    queryKey: ['recentShenanigans'],
    queryFn: async () => {
      if (!actor) throw new Error('Shenanigans actor not available');
      return actor.getRecentShenanigans();
    },
    enabled: !!actor && !actorFetching,
    refetchInterval: 3000,
  });
}

export function useCastShenanigan() {
  const { actor } = useShenaniganActor();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async ({ shenaniganType, target }: { shenaniganType: ShenaniganType; target: Principal | null }) => {
      if (!actor) throw new Error('Shenanigans actor not available');
      return actor.castShenanigan(shenaniganType, target ? [target] : []);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['shenaniganStats'] });
      queryClient.invalidateQueries({ queryKey: ['recentShenanigans'] });
      queryClient.invalidateQueries({ queryKey: ['ponziPoints'] });
      queryClient.invalidateQueries({ queryKey: ['ponziPointsBalance'] });
      queryClient.invalidateQueries({ queryKey: ['houseRepaymentBalance'] });
    },
  });
}

export function useGetShenaniganConfigs() {
  const { actor, isFetching: actorFetching } = useShenaniganActor();

  return useQuery<ShenaniganConfig[]>({
    queryKey: ['shenaniganConfigs'],
    queryFn: async () => {
      if (!actor) throw new Error('Shenanigans actor not available');
      return actor.getShenaniganConfigs();
    },
    enabled: !!actor && !actorFetching,
    refetchInterval: 10000,
  });
}

export function useUpdateShenaniganConfig() {
  const { actor } = useShenaniganActor();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (config: ShenaniganConfig) => {
      if (!actor) throw new Error('Shenanigans actor not available');
      return actor.updateShenaniganConfig(config);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['shenaniganConfigs'] });
    },
  });
}

export function useResetShenaniganConfig() {
  const { actor } = useShenaniganActor();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (id: number) => {
      if (!actor) throw new Error('Shenanigans actor not available');
      return actor.resetShenaniganConfig(BigInt(id));
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['shenaniganConfigs'] });
    },
  });
}

export function useSaveAllShenaniganConfigs() {
  const { actor } = useShenaniganActor();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (configs: ShenaniganConfig[]) => {
      if (!actor) throw new Error('Shenanigans actor not available');
      return actor.saveAllShenaniganConfigs(configs);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['shenaniganConfigs'] });
    },
  });
}
```

- [ ] **Step 2: Verify the frontend builds**

```bash
cd frontend && npx vite build
```

Expected: Successful build.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/hooks/useQueries.ts
git commit -m "refactor: rewire shenanigan hooks to standalone canister"
```

---

## Chunk 3: Deploy and Verify

### Task 6: Deploy everything and smoke test

**Files:** None (deployment steps)

- [ ] **Step 1: Deploy the updated backend**

```bash
dfx deploy --network ic backend
```

- [ ] **Step 2: Deploy the frontend**

```bash
dfx deploy --network ic frontend
```

- [ ] **Step 3: Verify shenanigans canister initialization**

```bash
dfx canister call --network ic shenanigans getShenaniganConfigs '()'
```

Expected: Returns array of 11 default configs.

- [ ] **Step 4: Smoke test in browser**

1. Navigate to the shenanigans tab — configs should load
2. Cast a shenanigan — should deduct PP and show outcome
3. Check stats tab — stats should update
4. Check profit center — dealer cuts should still flow
5. Admin panel — config changes should save

- [ ] **Step 5: Final commit**

```bash
git add -A
git commit -m "chore: deploy shenanigans migration, verify end-to-end"
```

---

## Notes

### What this does NOT migrate
- **Ponzi Points state** stays in the main backend (it's financial — needs to be in the blackholed canister)
- **Dealer repayment state** stays in the main backend (same reason)
- The shenanigans canister is a consumer of these via cross-canister calls

### Post-migration cleanup candidates
- The `updateDealerCut` function in the backend should be renamed to something clearer since it's now a public API
- Consider making `getPonziPointsBalance` a public query on the backend anyway (useful for other future canisters)
- The AoE Skim backfire was simplified (deducts from caster instead of distributing to all players) to avoid complex cross-canister iteration — can be enhanced later

### Pre-blackhole checklist (separate task)
- Remove `seedGame`
- Remove `setTestMode`, hardcode `testMode = false`
- Audit `addHouseMoney` (require real ICP transfer?)
- Verify `shenanigansPrincipal` is set correctly
- Verify admin principal is set correctly
- Test all cross-canister calls work
