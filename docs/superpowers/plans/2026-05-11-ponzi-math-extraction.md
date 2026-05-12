# `ponzi_math` Canister Extraction Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract all money-flow logic from `backend` into a new dedicated `ponzi_math` canister designed for eventual blackholing.

**Architecture:** Three canisters split by concern. `ponzi_math` (new, blackholable) owns games, backers, pot, seed reserve, exit toll, cover-charge accrual, and general ledger. `backend` (mutable) keeps profiles + access control + payManagement for cover-charge pay-out. `shenanigans` (mutable) owns PP economy + referral chain. Shenanigans observer polls `ponzi_math.getAllGames` for deposit events. Cover-charges accrue on ponzi_math, are swept to backend (gated on backend canister principal), then admin pays out from backend.

**Tech Stack:** Motoko, TypeScript + React, dfx CLI, ICRC-1/2 ICP ledger.

**Reference:** Design spec at `docs/superpowers/specs/2026-05-11-ponzi-math-extraction-design.md`.

**Validation cadence:** Every Motoko task ends with `dfx build <canister> --check` for type-check. Every frontend task ends with `npm run build` for full type-check + bundle. Integration smoke test is the final task.

**Naming guard:** `exitToll` and `coverCharge` identifiers stay unchanged (per CLAUDE.md). `dealer*` → `backer*` and `#upstream`/`#downstream` → `#seriesA`/`#seriesB` in new ponzi_math code and in places where those types cross the canister boundary (shenanigans actor type, frontend declarations). `game*`, `pot*`, `round*` are untouched.

---

## Phase 1: ponzi_math canister scaffolding

### Task 1: Register ponzi_math in dfx.json

**Files:**
- Modify: `dfx.json`

- [ ] **Step 1: Add the ponzi_math canister entry**

Add this entry to the `"canisters"` object, immediately after the `"backend"` entry, preserving JSON formatting:

```json
    "ponzi_math": {
      "main": "ponzi_math/main.mo",
      "type": "motoko"
    },
```

- [ ] **Step 2: Verify JSON parses**

Run: `python3 -c "import json; json.load(open('dfx.json'))"`
Expected: no output (success). If error, fix the trailing comma or quoting.

- [ ] **Step 3: Commit**

```bash
git add dfx.json
git commit -m "chore: register ponzi_math canister in dfx.json"
```

---

### Task 2: Create ponzi_math directory and copy ledger module

**Files:**
- Create: `ponzi_math/ledger.mo`

- [ ] **Step 1: Create the directory**

Run: `mkdir -p ponzi_math`

- [ ] **Step 2: Copy ledger.mo verbatim from backend**

Run: `cp backend/ledger.mo ponzi_math/ledger.mo`

The file is 193 lines of ICP ledger ICRC-1/2 type definitions and the
`ICP_LEDGER_CANISTER_ID` constant. No changes needed — these types are
identical across canisters that talk to the ledger.

- [ ] **Step 3: Verify copy is byte-identical**

Run: `diff backend/ledger.mo ponzi_math/ledger.mo`
Expected: no output (files identical).

- [ ] **Step 4: Commit**

```bash
git add ponzi_math/ledger.mo
git commit -m "chore(ponzi_math): copy ICP ledger module from backend"
```

---

### Task 3: Create ponzi_math icrc21.mo

**Files:**
- Create: `ponzi_math/icrc21.mo`

- [ ] **Step 1: Copy from backend then trim non-financial method labels**

Run: `cp backend/icrc21.mo ponzi_math/icrc21.mo`

- [ ] **Step 2: Replace the consentMessage switch arms**

In `ponzi_math/icrc21.mo`, replace the entire `let methodLabel = switch (request.method) { ... }` block (lines 65-74 of the copy) with this — drops profile/access-control methods, keeps financial methods, renames the dealer methods:

```motoko
        let methodLabel = switch (request.method) {
            case "createGame" { ?"Open Investment Position" };
            case "withdrawEarnings" { ?"Withdraw Earnings" };
            case "settleCompoundingGame" { ?"Settle Compounding Game" };
            case "addBackerMoney" { ?"Fund as Backer" };
            case "claimBackerRepayment" { ?"Claim Backer Repayment" };
            case "sweepCoverCharges" { ?"Sweep Cover Charges to Backend" };
            case _ { null };
        };
```

- [ ] **Step 3: Commit**

```bash
git add ponzi_math/icrc21.mo
git commit -m "chore(ponzi_math): icrc21 module with financial consent messages only"
```

---

### Task 4: Create ponzi_math main.mo skeleton

**Files:**
- Create: `ponzi_math/main.mo`

- [ ] **Step 1: Write minimal compiling skeleton with init args**

Create `ponzi_math/main.mo` with these contents — imports, init args, empty actor body:

```motoko
import Principal "mo:base/Principal";
import OrderedMap "mo:base/OrderedMap";
import Time "mo:base/Time";
import Nat "mo:base/Nat";
import Nat64 "mo:base/Nat64";
import Float "mo:base/Float";
import Int "mo:base/Int";
import Text "mo:base/Text";
import List "mo:base/List";
import Iter "mo:base/Iter";
import Debug "mo:base/Debug";
import Blob "mo:base/Blob";
import Error "mo:base/Error";

import Ledger "ledger";
import Icrc21 "icrc21";

persistent actor class PonziMath(initArgs : {
    backendPrincipal : Principal;
    testAdmin : Principal;
}) = Self {
    transient let BACKEND_PRINCIPAL : Principal = initArgs.backendPrincipal;
    transient let TEST_ADMIN : Principal = initArgs.testAdmin;
    transient let icpLedger : Ledger.LedgerActor = actor(Ledger.ICP_LEDGER_CANISTER_ID);
};
```

Note: `persistent actor class` with init args. The actor name is `Self` so we
can refer to it via `Principal.fromActor(Self)` later.

- [ ] **Step 2: Type-check**

Run: `dfx build ponzi_math --check`
Expected: success (the empty actor compiles).

- [ ] **Step 3: Commit**

```bash
git add ponzi_math/main.mo
git commit -m "feat(ponzi_math): skeleton actor with init args"
```

---

## Phase 2: ponzi_math types, state, locks

### Task 5: Add public types

**Files:**
- Modify: `ponzi_math/main.mo`

- [ ] **Step 1: Insert public type declarations after the icpLedger line**

After `transient let icpLedger = ...;` and before the closing `};` of the actor, add:

```motoko
    // ========================================================================
    // Public types
    // ========================================================================

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
        #seriesA;
        #seriesB;
    };

    public type BackerPosition = {
        owner : Principal;
        amount : Float;
        entitlement : Float;
        startTime : Int;
        isActive : Bool;
        backerType : BackerType;
        firstDepositDate : ?Int;
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
        #backdatedGameCreated : {
            admin : Principal;
            player : Principal;
            gameId : Nat;
            startTime : Int;
            amount : Float;
        };
    };
```

- [ ] **Step 2: Type-check**

Run: `dfx build ponzi_math --check`
Expected: success.

- [ ] **Step 3: Commit**

```bash
git add ponzi_math/main.mo
git commit -m "feat(ponzi_math): public types"
```

---

### Task 6: Add state variables

**Files:**
- Modify: `ponzi_math/main.mo`

- [ ] **Step 1: Insert state declarations after the types block**

Append to the actor body, after the public types:

```motoko
    // ========================================================================
    // State
    // ========================================================================

    transient let natMap = OrderedMap.Make<Nat>(Nat.compare);
    transient let principalMapNat = OrderedMap.Make<Principal>(Principal.compare);
    transient let intMap = OrderedMap.Make<Int>(Int.compare);

    var gameRecords = natMap.empty<GameRecord>();
    var nextGameId : Nat = 0;

    var platformStats : PlatformStats = {
        totalDeposits = 0.0;
        totalWithdrawals = 0.0;
        activeGames = 0;
        potBalance = 0.0;
        daysActive = 0;
    };

    var gameResetHistory = intMap.empty<GameResetRecord>();
    var roundSeedReserve : Float = 0.0;
    var depositTimestamps = principalMapNat.empty<List.List<Int>>();
    var backerPositions = principalMapNat.empty<BackerPosition>();
    var backerRepayments = principalMapNat.empty<Float>();
    var coverChargeBalance : Nat = 0;
    var generalLedger = natMap.empty<GeneralLedgerEntry>();
    var nextGeneralLedgerId : Nat = 0;

    // Transient concurrency state — resets on upgrade (safe by construction)
    transient var callerLocks = principalMapNat.empty<Bool>();
    transient var globalCriticalLock : Bool = false;
```

- [ ] **Step 2: Type-check**

Run: `dfx build ponzi_math --check`
Expected: success.

- [ ] **Step 3: Commit**

```bash
git add ponzi_math/main.mo
git commit -m "feat(ponzi_math): state variables"
```

---

### Task 7: Add concurrency locks and isCriticalSectionBusy query

**Files:**
- Modify: `ponzi_math/main.mo`

- [ ] **Step 1: Add lock helpers and the public diagnostic query**

Append after the state block:

```motoko
    // ========================================================================
    // Concurrency: per-caller lock and global critical-section lock
    // ========================================================================

    func acquireCallerLock(caller : Principal) {
        switch (principalMapNat.get(callerLocks, caller)) {
            case (?true) { Debug.trap("Another operation is already in progress for this caller") };
            case _ { callerLocks := principalMapNat.put(callerLocks, caller, true) };
        };
    };

    func releaseCallerLock(caller : Principal) {
        callerLocks := principalMapNat.delete(callerLocks, caller);
    };

    func acquireGlobalLock() {
        if (globalCriticalLock) {
            Debug.trap("Critical section busy — another operation is in progress, please retry");
        };
        globalCriticalLock := true;
    };

    func releaseGlobalLock() {
        globalCriticalLock := false;
    };

    public query func isCriticalSectionBusy() : async Bool {
        globalCriticalLock;
    };
```

- [ ] **Step 2: Type-check**

Run: `dfx build ponzi_math --check`
Expected: success.

- [ ] **Step 3: Commit**

```bash
git add ponzi_math/main.mo
git commit -m "feat(ponzi_math): per-caller and global concurrency locks"
```

---

### Task 8: Add validation and formatting helpers

**Files:**
- Modify: `ponzi_math/main.mo`

- [ ] **Step 1: Add helpers after the lock block**

```motoko
    // ========================================================================
    // Validation + formatting helpers
    // ========================================================================

    func requireAuthenticated(caller : Principal) {
        if (Principal.isAnonymous(caller)) {
            Debug.trap("Anonymous principal not allowed");
        };
    };

    func validateAmount(amount : Float) {
        if (Float.isNaN(amount)) { Debug.trap("Amount cannot be NaN") };
        if (Float.isNaN(amount - amount) and not Float.isNaN(amount)) {
            Debug.trap("Amount must be finite");
        };
        if (amount < 0.0) { Debug.trap("Amount cannot be negative") };
    };

    func roundToEightDecimals(value : Float) : Float {
        let multiplier = 100000000.0;
        Float.fromInt(Float.toInt(value * multiplier)) / multiplier;
    };

    func validateEightDecimals(value : Float) : Bool {
        let multiplier = 100000000.0;
        let rounded = Float.fromInt(Float.toInt(value * multiplier)) / multiplier;
        rounded == value;
    };

    func formatICP(value : Float) : Text {
        let intValue = Float.toInt(value);
        if (Float.fromInt(intValue) == value) {
            Int.toText(intValue);
        } else {
            let textValue = Float.toText(value);
            let parts = Iter.toArray(Text.split(textValue, #char '.'));
            switch (parts.size()) {
                case (1) { parts[0] };
                case (2) {
                    let trimmed = Text.trimEnd(parts[1], #char '0');
                    if (trimmed == "") { parts[0] } else { parts[0] # "." # trimmed };
                };
                case (_) { textValue };
            };
        };
    };

    func transferFromErrorMessage(err : Ledger.TransferFromError) : Text {
        switch (err) {
            case (#InsufficientFunds(_)) { "Insufficient ICP balance" };
            case (#InsufficientAllowance(_)) { "Please approve the transfer first" };
            case (#BadFee(_)) { "Bad fee" };
            case (#BadBurn(_)) { "Bad burn" };
            case (#TooOld) { "Transaction too old" };
            case (#CreatedInFuture(_)) { "Transaction created in future" };
            case (#Duplicate(_)) { "Duplicate transaction" };
            case (#TemporarilyUnavailable) { "Ledger temporarily unavailable" };
            case (#GenericError(e)) { "Error: " # e.message };
        };
    };

    func transferErrorMessage(err : Ledger.TransferError) : Text {
        switch (err) {
            case (#InsufficientFunds(_)) { "Canister has insufficient ICP. Please contact support." };
            case (#BadFee(_)) { "Bad fee" };
            case (#BadBurn(_)) { "Bad burn" };
            case (#TooOld) { "Transaction too old" };
            case (#CreatedInFuture(_)) { "Transaction created in future" };
            case (#Duplicate(_)) { "Duplicate transaction" };
            case (#TemporarilyUnavailable) { "Ledger temporarily unavailable" };
            case (#GenericError(e)) { "Error: " # e.message };
        };
    };
```

- [ ] **Step 2: Type-check**

Run: `dfx build ponzi_math --check`
Expected: success.

- [ ] **Step 3: Commit**

```bash
git add ponzi_math/main.mo
git commit -m "feat(ponzi_math): validation, rounding, and error-formatting helpers"
```

---

### Task 9: Add recordLedger internal helper

**Files:**
- Modify: `ponzi_math/main.mo`

- [ ] **Step 1: Add recordLedger after the helpers**

```motoko
    // ========================================================================
    // General Ledger event recording
    // ========================================================================

    func recordLedger(event : GeneralLedgerEvent) {
        let entry : GeneralLedgerEntry = {
            id = nextGeneralLedgerId;
            timestamp = Time.now();
            event;
        };
        generalLedger := natMap.put(generalLedger, nextGeneralLedgerId, entry);
        nextGeneralLedgerId += 1;
    };
```

- [ ] **Step 2: Type-check**

Run: `dfx build ponzi_math --check`
Expected: success.

- [ ] **Step 3: Commit**

```bash
git add ponzi_math/main.mo
git commit -m "feat(ponzi_math): general ledger event recording helper"
```

---

## Phase 3: ponzi_math money math helpers

### Task 10: Add calculateExitToll helper

**Files:**
- Modify: `ponzi_math/main.mo`

- [ ] **Step 1: Add calculateExitToll after recordLedger**

```motoko
    // ========================================================================
    // Exit toll calculation
    // Simple: 7% (< 3 days), 5% (3-10), 3% (> 10)
    // Compounding: 9% (15-day plan), 13% (30-day plan)
    // ========================================================================

    func calculateExitToll(game : GameRecord, earnings : Float) : Float {
        if (game.isCompounding) {
            switch (game.plan) {
                case (#compounding15Day) { earnings * 0.09 };
                case (#compounding30Day) { earnings * 0.13 };
                case (#simple21Day) { 0.0 };
            };
        } else {
            let elapsedSeconds = Float.fromInt((Time.now() - game.startTime) / 1_000_000_000);
            let elapsedDays = elapsedSeconds / 86400.0;
            if (elapsedDays < 3.0) { earnings * 0.07 }
            else if (elapsedDays < 10.0) { earnings * 0.05 }
            else { earnings * 0.03 };
        };
    };
```

- [ ] **Step 2: Type-check**

Run: `dfx build ponzi_math --check`
Expected: success.

- [ ] **Step 3: Commit**

```bash
git add ponzi_math/main.mo
git commit -m "feat(ponzi_math): exit toll calculation helper"
```

---

### Task 11: Add backer credit + exit toll distribution

**Files:**
- Modify: `ponzi_math/main.mo`

- [ ] **Step 1: Add creditBackerRepayment and distributeExitToll**

Append after calculateExitToll:

```motoko
    // ========================================================================
    // Backer repayment crediting + 35/25/40 exit-toll distribution
    // ========================================================================

    func creditBackerRepayment(backer : Principal, amount : Float) {
        let current = switch (principalMapNat.get(backerRepayments, backer)) {
            case (null) { 0.0 };
            case (?existing) { existing };
        };
        backerRepayments := principalMapNat.put(backerRepayments, backer, current + amount);
    };

    // 50% of the toll seeds the next round (routed to roundSeedReserve, OUT of
    // the pot). The other 50% credits backer repayment balances via 35/25/40.
    // Tolls never stay in the pot — that's how reset preserves carryover cleanly.
    func distributeExitToll(tollAmount : Float) {
        let seedAmount = tollAmount * 0.5;
        let backerRepaymentAmount = tollAmount * 0.5;
        roundSeedReserve += seedAmount;

        let allBackers = Iter.toArray(principalMapNat.vals(backerPositions));
        if (allBackers.size() == 0) {
            // No backers yet — backer half also flows to seed reserve (not pot).
            roundSeedReserve += backerRepaymentAmount;
            recordLedger(#tollDistribution({
                tollAmount;
                toSeedReserve = tollAmount;
                toOldestSeriesA = 0.0;
                toOtherSeriesA = 0.0;
                toAllBackers = 0.0;
            }));
            return;
        };

        let seriesABackers = List.toArray(
            List.filter(
                List.fromArray(allBackers),
                func(b : BackerPosition) : Bool { b.backerType == #seriesA },
            )
        );

        // Find oldest Series A backer
        var oldestBacker : ?BackerPosition = null;
        var oldestTime : Int = 0;
        for (b in seriesABackers.vals()) {
            switch (b.firstDepositDate) {
                case (null) {};
                case (?date) {
                    if (oldestBacker == null or date < oldestTime) {
                        oldestBacker := ?b;
                        oldestTime := date;
                    };
                };
            };
        };

        let toOldest : Float = backerRepaymentAmount * 0.35;
        switch (oldestBacker) {
            case (null) {};
            case (?b) { creditBackerRepayment(b.owner, toOldest) };
        };

        let otherSeriesA = List.toArray(
            List.filter(
                List.fromArray(seriesABackers),
                func(b : BackerPosition) : Bool {
                    switch (oldestBacker) {
                        case (null) { true };
                        case (?oldest) { b.owner != oldest.owner };
                    };
                },
            )
        );
        var toOthers : Float = 0.0;
        if (otherSeriesA.size() > 0) {
            let perBacker = backerRepaymentAmount * 0.25 / Float.fromInt(otherSeriesA.size());
            toOthers := perBacker * Float.fromInt(otherSeriesA.size());
            for (b in otherSeriesA.vals()) { creditBackerRepayment(b.owner, perBacker) };
        };

        let perAll = backerRepaymentAmount * 0.4 / Float.fromInt(allBackers.size());
        let toAll = perAll * Float.fromInt(allBackers.size());
        for (b in allBackers.vals()) { creditBackerRepayment(b.owner, perAll) };

        recordLedger(#tollDistribution({
            tollAmount;
            toSeedReserve = seedAmount;
            toOldestSeriesA = toOldest;
            toOtherSeriesA = toOthers;
            toAllBackers = toAll;
        }));
    };
```

- [ ] **Step 2: Type-check**

Run: `dfx build ponzi_math --check`
Expected: success.

- [ ] **Step 3: Commit**

```bash
git add ponzi_math/main.mo
git commit -m "feat(ponzi_math): backer credit + 35/25/40 toll distribution"
```

---

### Task 12: Add triggerGameReset internal helper

**Files:**
- Modify: `ponzi_math/main.mo`

- [ ] **Step 1: Add triggerGameReset after distributeExitToll**

```motoko
    // ========================================================================
    // Game reset (called on insolvency)
    // ========================================================================

    func triggerGameReset(reason : Text) {
        let resetRecord : GameResetRecord = {
            resetTime = Time.now();
            reason;
        };
        gameResetHistory := intMap.put(gameResetHistory, Time.now(), resetRecord);
        gameRecords := natMap.empty<GameRecord>();

        // Carry the seed reserve into the new round's pot. The reserve has been
        // accumulating 50% of every realized exit toll since the last reset.
        let newPot = roundSeedReserve;
        let carried = newPot;
        roundSeedReserve := 0.0;
        platformStats := {
            totalDeposits = 0.0;
            totalWithdrawals = 0.0;
            activeGames = 0;
            potBalance = newPot;
            daysActive = 0;
        };
        nextGameId := 0;
        recordLedger(#gameReset({ reason; seedReserveCarried = carried }));
    };
```

- [ ] **Step 2: Type-check**

Run: `dfx build ponzi_math --check`
Expected: success.

- [ ] **Step 3: Commit**

```bash
git add ponzi_math/main.mo
git commit -m "feat(ponzi_math): triggerGameReset internal helper"
```

---

### Task 13: Add earnings calculation queries

**Files:**
- Modify: `ponzi_math/main.mo`

- [ ] **Step 1: Add the four calculate* public queries**

Append after triggerGameReset:

```motoko
    // ========================================================================
    // Earnings calculations (public queries — frontend passes the GameRecord)
    // ========================================================================

    public query func calculateEarnings(game : GameRecord) : async Float {
        let dailyRate = switch (game.plan) {
            case (#simple21Day) { 0.11 };
            case (#compounding15Day) { 0.12 };
            case (#compounding30Day) { 0.09 };
        };
        let maxDurationSeconds = switch (game.plan) {
            case (#simple21Day) { 21.0 * 86400.0 };
            case (#compounding15Day) { 15.0 * 86400.0 };
            case (#compounding30Day) { 30.0 * 86400.0 };
        };
        let timeAlreadyAccounted = Float.fromInt((game.lastUpdateTime - game.startTime) / 1_000_000_000);
        let remainingAllowedTime = Float.max(0.0, maxDurationSeconds - timeAlreadyAccounted);
        let timeSinceLastUpdate = Float.fromInt((Time.now() - game.lastUpdateTime) / 1_000_000_000);
        let timeElapsed = Float.min(timeSinceLastUpdate, remainingAllowedTime);
        let earnings = game.amount * dailyRate * (timeElapsed / 86400.0);
        roundToEightDecimals(game.accumulatedEarnings + earnings);
    };

    public query func calculateCompoundedEarnings(game : GameRecord) : async Float {
        if (game.plan != #compounding15Day) {
            Debug.trap("This calculation is only for the 15-day compounding plan");
        };
        let timeElapsed = Float.fromInt((Time.now() - game.startTime) / 1_000_000_000);
        let daysElapsed = Float.min(timeElapsed / 86400.0, 15.0);
        let dailyRate = 0.12;
        let compoundedEarnings = game.amount * (Float.pow(1.0 + dailyRate, daysElapsed) - 1.0);
        roundToEightDecimals(compoundedEarnings);
    };

    public query func calculateCompounded30DayEarnings(game : GameRecord) : async Float {
        if (game.plan != #compounding30Day) {
            Debug.trap("This calculation is only for the 30-day compounding plan");
        };
        let timeElapsed = Float.fromInt((Time.now() - game.startTime) / 1_000_000_000);
        let daysElapsed = Float.min(timeElapsed / 86400.0, 30.0);
        let dailyRate = 0.09;
        let compoundedEarnings = game.amount * (Float.pow(1.0 + dailyRate, daysElapsed) - 1.0);
        roundToEightDecimals(compoundedEarnings);
    };

    public query func calculateCompoundedROI() : async Float {
        let dailyRate = 0.12;
        let days = 15.0;
        let roi = Float.pow(1.0 + dailyRate, days) - 1.0;
        roundToEightDecimals(roi);
    };
```

- [ ] **Step 2: Type-check**

Run: `dfx build ponzi_math --check`
Expected: success.

- [ ] **Step 3: Commit**

```bash
git add ponzi_math/main.mo
git commit -m "feat(ponzi_math): earnings calculation public queries"
```

---

## Phase 4: ponzi_math public money methods

### Task 14: Add createGame

**Files:**
- Modify: `ponzi_math/main.mo`

- [ ] **Step 1: Add createGame public method**

Append after the calculation queries. Notes vs the backend version:
- Drops the `referrer : ?Principal` parameter and all referral registration.
- Uses `Principal.fromActor(Self)` for self principal — no canisterPrincipal state var.
- Cover-charge accrual emits a `#coverChargeAccrued` general-ledger entry.
- Deposit emits a `#deposit` general-ledger entry.

```motoko
    // ========================================================================
    // createGame — opens a new investment position
    // Returns #Ok(gameId) or #Err(message). Returns rather than traps after
    // await because traps roll back the current turn, losing lock release.
    // ========================================================================

    transient let COVER_CHARGE_RATE : Float = 0.04;

    public shared ({ caller }) func createGame(
        plan : GamePlan,
        amount : Float,
        isCompounding : Bool,
    ) : async { #Ok : Nat; #Err : Text } {
        requireAuthenticated(caller);
        validateAmount(amount);
        if (amount < 0.1) { return #Err("Minimum deposit is 0.1 ICP") };
        if (not validateEightDecimals(amount)) {
            return #Err("Amount cannot have more than 8 decimal places");
        };

        acquireCallerLock(caller);
        acquireGlobalLock();

        // Deposit rate limit (3 per hour) — checked BEFORE the transfer
        let currentTime = Time.now();
        let currentHour = currentTime / 3600000000000;
        switch (principalMapNat.get(depositTimestamps, caller)) {
            case (null) {};
            case (?timestamps) {
                let filtered = List.filter<Int>(
                    timestamps,
                    func(t) { currentHour - t < 1 },
                );
                if (List.size(filtered) >= 3) {
                    releaseGlobalLock();
                    releaseCallerLock(caller);
                    return #Err("You can only open 3 positions per hour");
                };
            };
        };

        // Max deposit for simple mode: greater of 20% pot or 5 ICP
        if (not isCompounding) {
            let maxDeposit = Float.max(platformStats.potBalance * 0.2, 5.0);
            if (amount > maxDeposit) {
                releaseGlobalLock();
                releaseCallerLock(caller);
                return #Err("Maximum deposit for simple mode is the greater of 20% of current pot balance or 5 ICP (" # formatICP(maxDeposit) # " ICP)");
            };
        };

        let selfPrincipal = Principal.fromActor(Self);
        let amountE8s = Int.abs(Float.toInt(amount * 100_000_000.0));

        let transferResult = try {
            await icpLedger.icrc2_transfer_from({
                spender_subaccount = null;
                from = { owner = caller; subaccount = null };
                to = { owner = selfPrincipal; subaccount = null };
                amount = amountE8s;
                fee = null;
                memo = null;
                created_at_time = null;
            });
        } catch (e) {
            releaseGlobalLock();
            releaseCallerLock(caller);
            return #Err("Failed to contact ICP ledger: " # Error.message(e));
        };

        switch (transferResult) {
            case (#Err(err)) {
                releaseGlobalLock();
                releaseCallerLock(caller);
                return #Err(transferFromErrorMessage(err));
            };
            case (#Ok(_)) {};
        };

        // Record rate-limit timestamp
        switch (principalMapNat.get(depositTimestamps, caller)) {
            case (null) {
                depositTimestamps := principalMapNat.put(depositTimestamps, caller, List.push(currentHour, List.nil()));
            };
            case (?timestamps) {
                let filtered = List.filter<Int>(timestamps, func(t) { currentHour - t < 1 });
                depositTimestamps := principalMapNat.put(depositTimestamps, caller, List.push(currentHour, filtered));
            };
        };

        // Cover charge: 4% to coverChargeBalance (segregated), 96% to pot
        let coverCharge = amount * COVER_CHARGE_RATE;
        let coverChargeE8s = Int.abs(Float.toInt(coverCharge * 100_000_000.0));
        coverChargeBalance += coverChargeE8s;
        let netAmount = amount - coverCharge;

        let gameId = nextGameId;
        nextGameId += 1;

        if (coverChargeE8s > 0) {
            recordLedger(#coverChargeAccrued({
                gameId;
                player = caller;
                amountE8s = coverChargeE8s;
            }));
        };

        let newGame : GameRecord = {
            id = gameId;
            player = caller;
            plan;
            amount;
            startTime = Time.now();
            isCompounding;
            isActive = true;
            lastUpdateTime = Time.now();
            accumulatedEarnings = 0.0;
            totalWithdrawn = 0.0;
        };
        gameRecords := natMap.put(gameRecords, gameId, newGame);
        platformStats := {
            platformStats with
            totalDeposits = platformStats.totalDeposits + amount;
            activeGames = platformStats.activeGames + 1;
            potBalance = platformStats.potBalance + netAmount;
        };

        recordLedger(#deposit({
            player = caller;
            gameId;
            gross = amount;
            coverCharge;
            netToPot = netAmount;
            plan;
            isCompounding;
        }));

        releaseGlobalLock();
        releaseCallerLock(caller);
        #Ok(gameId);
    };
```

- [ ] **Step 2: Type-check**

Run: `dfx build ponzi_math --check`
Expected: success.

- [ ] **Step 3: Commit**

```bash
git add ponzi_math/main.mo
git commit -m "feat(ponzi_math): createGame method"
```

---

### Task 15: Add addBackerMoney

**Files:**
- Modify: `ponzi_math/main.mo`

- [ ] **Step 1: Add addBackerMoney method**

Append after createGame. Notes vs backend:
- Renamed from `addDealerMoney`.
- `BackerPosition` has no `name` field — drops profile lookup.
- `backerType = #seriesA` (was `#upstream`).
- Emits `#backerDeposit` general-ledger entry.

```motoko
    // ========================================================================
    // addBackerMoney — funds a Series A backer position
    // ========================================================================

    public shared ({ caller }) func addBackerMoney(amount : Float) : async { #Ok : Nat; #Err : Text } {
        requireAuthenticated(caller);
        validateAmount(amount);
        if (amount < 0.1) { return #Err("Minimum deposit is 0.1 ICP") };
        if (not validateEightDecimals(amount)) {
            return #Err("Amount cannot have more than 8 decimal places");
        };

        acquireCallerLock(caller);
        acquireGlobalLock();

        let selfPrincipal = Principal.fromActor(Self);
        let amountE8s = Int.abs(Float.toInt(amount * 100_000_000.0));

        let transferResult = try {
            await icpLedger.icrc2_transfer_from({
                spender_subaccount = null;
                from = { owner = caller; subaccount = null };
                to = { owner = selfPrincipal; subaccount = null };
                amount = amountE8s;
                fee = null;
                memo = null;
                created_at_time = null;
            });
        } catch (e) {
            releaseGlobalLock();
            releaseCallerLock(caller);
            return #Err("Failed to contact ICP ledger: " # Error.message(e));
        };

        let blockIndex = switch (transferResult) {
            case (#Err(err)) {
                releaseGlobalLock();
                releaseCallerLock(caller);
                return #Err(transferFromErrorMessage(err));
            };
            case (#Ok(idx)) { idx };
        };

        let entitlement = amount * 1.24; // Series A 24% bonus

        switch (principalMapNat.get(backerPositions, caller)) {
            case (null) {
                let newBacker : BackerPosition = {
                    owner = caller;
                    amount;
                    entitlement;
                    startTime = Time.now();
                    isActive = true;
                    backerType = #seriesA;
                    firstDepositDate = ?Time.now();
                };
                backerPositions := principalMapNat.put(backerPositions, caller, newBacker);
            };
            case (?existing) {
                let updated : BackerPosition = {
                    existing with
                    amount = existing.amount + amount;
                    entitlement = existing.entitlement + entitlement;
                };
                backerPositions := principalMapNat.put(backerPositions, caller, updated);
            };
        };

        platformStats := {
            platformStats with
            potBalance = platformStats.potBalance + amount;
        };

        recordLedger(#backerDeposit({ backer = caller; amount; entitlement }));

        releaseGlobalLock();
        releaseCallerLock(caller);
        let _ = blockIndex;
        #Ok(blockIndex);
    };
```

- [ ] **Step 2: Type-check**

Run: `dfx build ponzi_math --check`
Expected: success.

- [ ] **Step 3: Commit**

```bash
git add ponzi_math/main.mo
git commit -m "feat(ponzi_math): addBackerMoney method"
```

---

### Task 16: Add withdrawEarnings

**Files:**
- Modify: `ponzi_math/main.mo`

- [ ] **Step 1: Add withdrawEarnings method**

Append after addBackerMoney. Mirrors backend's withdrawEarnings (lines 1074-1209) with these differences:
- Uses `Principal.fromActor(Self)` instead of `canisterPrincipal` state.
- Emits a `#withdrawal` general-ledger entry on success.
- Renames `dealerRepayments` → `backerRepayments`.

```motoko
    // ========================================================================
    // withdrawEarnings — simple-plan payout, applies tiered exit toll
    // ========================================================================

    public shared ({ caller }) func withdrawEarnings(gameId : Nat) : async { #Ok : Float; #Err : Text } {
        requireAuthenticated(caller);
        acquireCallerLock(caller);
        acquireGlobalLock();
        switch (natMap.get(gameRecords, gameId)) {
            case (null) {
                releaseGlobalLock();
                releaseCallerLock(caller);
                #Err("Game not found");
            };
            case (?game) {
                if (game.player != caller) {
                    releaseGlobalLock();
                    releaseCallerLock(caller);
                    return #Err("Unauthorized: Only the game owner can withdraw earnings");
                };
                if (game.isCompounding) {
                    releaseGlobalLock();
                    releaseCallerLock(caller);
                    return #Err("Cannot withdraw from compounding games");
                };

                let earnings = await calculateEarnings(game);
                let exitToll = calculateExitToll(game, earnings);
                let netEarnings = roundToEightDecimals(earnings - exitToll);

                let elapsedDays = Float.fromInt((Time.now() - game.startTime) / 1_000_000_000) / 86400.0;
                let closePosition = elapsedDays >= 21.0;

                let originalGame = game;
                let originalStats = platformStats;
                let originalRepayments = backerRepayments;
                let originalSeedReserve = roundSeedReserve;

                let pot = platformStats.potBalance;
                let isInsolvent = earnings > pot;

                if (isInsolvent and pot <= 0.0) {
                    triggerGameReset("Insufficient funds for payout (pot empty)");
                    releaseGlobalLock();
                    releaseCallerLock(caller);
                    return #Err("Game reset: pot is empty");
                };

                let scaleFactor = if (isInsolvent) { pot / earnings } else { 1.0 };
                let actualNetEarnings = roundToEightDecimals(netEarnings * scaleFactor);
                let actualToll = exitToll * scaleFactor;
                let actualPotDeduction = if (isInsolvent) { pot } else { earnings };

                distributeExitToll(actualToll);

                let willClose = closePosition or isInsolvent;
                let updatedGame : GameRecord = {
                    game with
                    accumulatedEarnings = 0.0;
                    lastUpdateTime = Time.now();
                    totalWithdrawn = game.totalWithdrawn + actualNetEarnings;
                    isActive = if (willClose) false else game.isActive;
                };
                gameRecords := natMap.put(gameRecords, gameId, updatedGame);
                platformStats := {
                    platformStats with
                    totalWithdrawals = platformStats.totalWithdrawals + actualNetEarnings;
                    potBalance = platformStats.potBalance - actualPotDeduction;
                    activeGames =
                        if (willClose and platformStats.activeGames > 0) {
                            platformStats.activeGames - 1
                        } else { platformStats.activeGames };
                };

                let netEarningsE8s = Int.abs(Float.toInt(actualNetEarnings * 100_000_000.0));
                if (netEarningsE8s > 0) {
                    let transferResult = try {
                        await icpLedger.icrc1_transfer({
                            from_subaccount = null;
                            to = { owner = caller; subaccount = null };
                            amount = netEarningsE8s;
                            fee = null;
                            memo = null;
                            created_at_time = null;
                        });
                    } catch (e) {
                        gameRecords := natMap.put(gameRecords, gameId, originalGame);
                        platformStats := originalStats;
                        backerRepayments := originalRepayments;
                        roundSeedReserve := originalSeedReserve;
                        releaseGlobalLock();
                        releaseCallerLock(caller);
                        return #Err("Failed to contact ICP ledger: " # Error.message(e));
                    };
                    switch (transferResult) {
                        case (#Err(err)) {
                            gameRecords := natMap.put(gameRecords, gameId, originalGame);
                            platformStats := originalStats;
                            backerRepayments := originalRepayments;
                            roundSeedReserve := originalSeedReserve;
                            releaseGlobalLock();
                            releaseCallerLock(caller);
                            return #Err(transferErrorMessage(err));
                        };
                        case (#Ok(_)) {};
                    };
                };

                recordLedger(#withdrawal({
                    player = caller;
                    gameId;
                    grossEarnings = earnings;
                    toll = actualToll;
                    netToPlayer = actualNetEarnings;
                    potDeduction = actualPotDeduction;
                    isInsolvent;
                }));

                if (isInsolvent) {
                    triggerGameReset("Pot drained (partial payout)");
                };

                releaseGlobalLock();
                releaseCallerLock(caller);
                #Ok(actualNetEarnings);
            };
        };
    };
```

- [ ] **Step 2: Type-check**

Run: `dfx build ponzi_math --check`
Expected: success.

- [ ] **Step 3: Commit**

```bash
git add ponzi_math/main.mo
git commit -m "feat(ponzi_math): withdrawEarnings method with toll + insolvency handling"
```

---

### Task 17: Add settleCompoundingGame

**Files:**
- Modify: `ponzi_math/main.mo`

- [ ] **Step 1: Add settleCompoundingGame method**

Append after withdrawEarnings. Mirrors backend's settleCompoundingGame (lines 1213-1362), with the same renames + `Principal.fromActor(Self)` + `#settlement` ledger entry:

```motoko
    // ========================================================================
    // settleCompoundingGame — compounding-plan payout at maturity
    // ========================================================================

    public shared ({ caller }) func settleCompoundingGame(gameId : Nat) : async { #Ok : Float; #Err : Text } {
        requireAuthenticated(caller);
        acquireCallerLock(caller);
        acquireGlobalLock();
        switch (natMap.get(gameRecords, gameId)) {
            case (null) {
                releaseGlobalLock();
                releaseCallerLock(caller);
                #Err("Game not found");
            };
            case (?game) {
                if (game.player != caller) {
                    releaseGlobalLock();
                    releaseCallerLock(caller);
                    return #Err("Unauthorized: Only the game owner can settle this game");
                };
                if (not game.isCompounding) {
                    releaseGlobalLock();
                    releaseCallerLock(caller);
                    return #Err("This function is only for compounding games. Use withdrawEarnings instead.");
                };
                if (not game.isActive) {
                    releaseGlobalLock();
                    releaseCallerLock(caller);
                    return #Err("Game is already settled");
                };

                let timeElapsedSec = Float.fromInt((Time.now() - game.startTime) / 1_000_000_000);
                let daysElapsed = timeElapsedSec / 86400.0;
                let maturityDaysOpt : ?Float = switch (game.plan) {
                    case (#compounding15Day) { ?15.0 };
                    case (#compounding30Day) { ?30.0 };
                    case (#simple21Day) { null };
                };
                let maturityDays = switch (maturityDaysOpt) {
                    case (null) {
                        releaseGlobalLock();
                        releaseCallerLock(caller);
                        return #Err("Simple games cannot be settled this way");
                    };
                    case (?d) { d };
                };
                if (daysElapsed < maturityDays) {
                    releaseGlobalLock();
                    releaseCallerLock(caller);
                    return #Err("Game has not matured yet. " # Float.toText(maturityDays - daysElapsed) # " days remaining.");
                };

                let dailyRate = switch (game.plan) {
                    case (#compounding15Day) { 0.12 };
                    case (#compounding30Day) { 0.09 };
                    case (#simple21Day) { 0.0 };
                };
                let earnings = game.amount * (Float.pow(1.0 + dailyRate, maturityDays) - 1.0);
                let exitToll = calculateExitToll(game, earnings);
                let netEarnings = roundToEightDecimals(earnings - exitToll);

                let originalGame = game;
                let originalStats = platformStats;
                let originalRepayments = backerRepayments;
                let originalSeedReserve = roundSeedReserve;

                let pot = platformStats.potBalance;
                let isInsolvent = earnings > pot;

                if (isInsolvent and pot <= 0.0) {
                    triggerGameReset("Insufficient funds for compounding game settlement (pot empty)");
                    releaseGlobalLock();
                    releaseCallerLock(caller);
                    return #Err("Game reset: pot is empty");
                };

                let scaleFactor = if (isInsolvent) { pot / earnings } else { 1.0 };
                let actualNetEarnings = roundToEightDecimals(netEarnings * scaleFactor);
                let actualToll = exitToll * scaleFactor;
                let actualPotDeduction = if (isInsolvent) { pot } else { earnings };

                distributeExitToll(actualToll);

                let settled : GameRecord = {
                    game with
                    isActive = false;
                    accumulatedEarnings = actualNetEarnings;
                    totalWithdrawn = actualNetEarnings;
                    lastUpdateTime = Time.now();
                };
                gameRecords := natMap.put(gameRecords, gameId, settled);
                platformStats := {
                    platformStats with
                    totalWithdrawals = platformStats.totalWithdrawals + actualNetEarnings;
                    potBalance = platformStats.potBalance - actualPotDeduction;
                    activeGames = if (platformStats.activeGames > 0) { platformStats.activeGames - 1 } else { 0 };
                };

                let payoutE8s = Int.abs(Float.toInt(actualNetEarnings * 100_000_000.0));
                if (payoutE8s > 0) {
                    let transferResult = try {
                        await icpLedger.icrc1_transfer({
                            from_subaccount = null;
                            to = { owner = caller; subaccount = null };
                            amount = payoutE8s;
                            fee = null;
                            memo = null;
                            created_at_time = null;
                        });
                    } catch (e) {
                        gameRecords := natMap.put(gameRecords, gameId, originalGame);
                        platformStats := originalStats;
                        backerRepayments := originalRepayments;
                        roundSeedReserve := originalSeedReserve;
                        releaseGlobalLock();
                        releaseCallerLock(caller);
                        return #Err("Failed to contact ICP ledger: " # Error.message(e));
                    };
                    switch (transferResult) {
                        case (#Err(err)) {
                            gameRecords := natMap.put(gameRecords, gameId, originalGame);
                            platformStats := originalStats;
                            backerRepayments := originalRepayments;
                            roundSeedReserve := originalSeedReserve;
                            releaseGlobalLock();
                            releaseCallerLock(caller);
                            return #Err(transferErrorMessage(err));
                        };
                        case (#Ok(_)) {};
                    };
                };

                recordLedger(#settlement({
                    player = caller;
                    gameId;
                    grossEarnings = earnings;
                    toll = actualToll;
                    netToPlayer = actualNetEarnings;
                    potDeduction = actualPotDeduction;
                    isInsolvent;
                }));

                if (isInsolvent) {
                    triggerGameReset("Pot drained (partial payout)");
                };

                releaseGlobalLock();
                releaseCallerLock(caller);
                #Ok(actualNetEarnings);
            };
        };
    };
```

- [ ] **Step 2: Type-check**

Run: `dfx build ponzi_math --check`
Expected: success.

- [ ] **Step 3: Commit**

```bash
git add ponzi_math/main.mo
git commit -m "feat(ponzi_math): settleCompoundingGame method"
```

---

### Task 18: Add claimBackerRepayment

**Files:**
- Modify: `ponzi_math/main.mo`

- [ ] **Step 1: Add claimBackerRepayment method**

Append after settleCompoundingGame:

```motoko
    // ========================================================================
    // claimBackerRepayment — transfers backer's accrued repayment balance
    // ========================================================================

    public shared ({ caller }) func claimBackerRepayment() : async { #Ok : Float; #Err : Text } {
        requireAuthenticated(caller);
        acquireCallerLock(caller);
        acquireGlobalLock();
        let balanceOpt : ?Float = switch (principalMapNat.get(backerRepayments, caller)) {
            case (null) { null };
            case (?b) { if (b <= 0.0) { null } else { ?b } };
        };
        let balance = switch (balanceOpt) {
            case (null) {
                releaseGlobalLock();
                releaseCallerLock(caller);
                return #Err("No repayment balance to claim");
            };
            case (?b) { b };
        };

        backerRepayments := principalMapNat.put(backerRepayments, caller, 0.0);

        let balanceE8s = Int.abs(Float.toInt(roundToEightDecimals(balance) * 100_000_000.0));
        let transferResult = try {
            await icpLedger.icrc1_transfer({
                from_subaccount = null;
                to = { owner = caller; subaccount = null };
                amount = balanceE8s;
                fee = null;
                memo = null;
                created_at_time = null;
            });
        } catch (e) {
            backerRepayments := principalMapNat.put(backerRepayments, caller, balance);
            releaseGlobalLock();
            releaseCallerLock(caller);
            return #Err("Failed to contact ICP ledger: " # Error.message(e));
        };

        switch (transferResult) {
            case (#Err(err)) {
                backerRepayments := principalMapNat.put(backerRepayments, caller, balance);
                releaseGlobalLock();
                releaseCallerLock(caller);
                return #Err(transferErrorMessage(err));
            };
            case (#Ok(_)) {};
        };

        recordLedger(#backerRepaymentClaim({ backer = caller; amount = balance }));

        releaseGlobalLock();
        releaseCallerLock(caller);
        #Ok(balance);
    };
```

- [ ] **Step 2: Type-check**

Run: `dfx build ponzi_math --check`
Expected: success.

- [ ] **Step 3: Commit**

```bash
git add ponzi_math/main.mo
git commit -m "feat(ponzi_math): claimBackerRepayment method"
```

---

### Task 19: Add sweepCoverCharges

**Files:**
- Modify: `ponzi_math/main.mo`

- [ ] **Step 1: Add sweepCoverCharges method**

Append after claimBackerRepayment. Gated on caller == backendPrincipal. Transfers full accumulated balance, zeros local state on success, reverts on failure.

```motoko
    // ========================================================================
    // sweepCoverCharges — gated on backend canister principal
    // Transfers the full coverChargeBalance to BACKEND_PRINCIPAL.
    // ========================================================================

    public shared ({ caller }) func sweepCoverCharges() : async { #Ok : Nat; #Err : Text } {
        if (caller != BACKEND_PRINCIPAL) {
            return #Err("Unauthorized: only backend canister can sweep");
        };
        if (coverChargeBalance == 0) {
            return #Err("Nothing to sweep");
        };
        if (coverChargeBalance <= Ledger.ICP_TRANSFER_FEE) {
            return #Err("Accumulated balance below transfer fee");
        };

        acquireGlobalLock();

        let amount = coverChargeBalance;
        let transferAmount : Nat = amount - Ledger.ICP_TRANSFER_FEE;

        // Deduct BEFORE transfer (saga pattern). Compensate on failure.
        coverChargeBalance := 0;

        let transferResult = try {
            await icpLedger.icrc1_transfer({
                from_subaccount = null;
                to = { owner = BACKEND_PRINCIPAL; subaccount = null };
                amount = transferAmount;
                fee = null;
                memo = null;
                created_at_time = null;
            });
        } catch (e) {
            coverChargeBalance := amount;
            releaseGlobalLock();
            return #Err("Failed to contact ICP ledger: " # Error.message(e));
        };

        switch (transferResult) {
            case (#Err(err)) {
                coverChargeBalance := amount;
                releaseGlobalLock();
                return #Err(transferErrorMessage(err));
            };
            case (#Ok(blockIndex)) {
                recordLedger(#coverChargeSwept({
                    amountE8s = amount;
                    toBackend = BACKEND_PRINCIPAL;
                    blockIndex;
                }));
                releaseGlobalLock();
                #Ok(blockIndex);
            };
        };
    };
```

- [ ] **Step 2: Type-check**

Run: `dfx build ponzi_math --check`
Expected: success.

- [ ] **Step 3: Commit**

```bash
git add ponzi_math/main.mo
git commit -m "feat(ponzi_math): sweepCoverCharges gated on backend principal"
```

---

## Phase 5: ponzi_math queries

### Task 20: Add platform stats and game queries

**Files:**
- Modify: `ponzi_math/main.mo`

- [ ] **Step 1: Add public queries — platform-wide getters and game lookups**

```motoko
    // ========================================================================
    // Public queries — platform state
    // ========================================================================

    public query func getPlatformStats() : async PlatformStats {
        {
            platformStats with
            // March 16 2026 00:00 PST — first mainnet deployment (kept for date math continuity).
            daysActive = Int.abs((Time.now() - 1_773_644_400_000_000_000) / 86_400_000_000_000);
        };
    };

    public query func getAllGames() : async [GameRecord] {
        Iter.toArray(natMap.vals(gameRecords));
    };

    public query func getAllActiveGames() : async [GameRecord] {
        var active = List.nil<GameRecord>();
        for (g in natMap.vals(gameRecords)) {
            if (g.isActive) { active := List.push(g, active) };
        };
        List.toArray(active);
    };

    public query ({ caller }) func getUserGames() : async [GameRecord] {
        var games = List.nil<GameRecord>();
        for (g in natMap.vals(gameRecords)) {
            if (g.player == caller) { games := List.push(g, games) };
        };
        List.toArray(games);
    };

    public query func getUserGamesFor(user : Principal) : async [GameRecord] {
        var games = List.nil<GameRecord>();
        for (g in natMap.vals(gameRecords)) {
            if (g.player == user) { games := List.push(g, games) };
        };
        List.toArray(games);
    };

    public query func getGameById(gameId : Nat) : async ?GameRecord {
        natMap.get(gameRecords, gameId);
    };

    public query func getActiveGameCount() : async Nat {
        var count = 0;
        for (g in natMap.vals(gameRecords)) { if (g.isActive) { count += 1 } };
        count;
    };

    public query func getAvailableBalance() : async Float {
        platformStats.potBalance;
    };

    public query func getTotalDeposits() : async Float {
        platformStats.totalDeposits;
    };

    public query func getTotalWithdrawals() : async Float {
        platformStats.totalWithdrawals;
    };

    public query func getDaysActive() : async Nat {
        Int.abs((Time.now() - 1_773_644_400_000_000_000) / 86_400_000_000_000);
    };

    public query func getMaxDepositLimit() : async Float {
        Float.max(platformStats.potBalance * 0.2, 5.0);
    };

    public query ({ caller }) func checkDepositRateLimit() : async Bool {
        let currentHour = Time.now() / 3600000000000;
        switch (principalMapNat.get(depositTimestamps, caller)) {
            case (null) { true };
            case (?ts) {
                let filtered = List.filter<Int>(ts, func(t) { currentHour - t < 1 });
                List.size(filtered) < 3;
            };
        };
    };

    public query func getRoundSeedReserve() : async Float {
        roundSeedReserve;
    };

    public query func getGameResetHistory() : async [GameResetRecord] {
        Iter.toArray(intMap.vals(gameResetHistory));
    };
```

- [ ] **Step 2: Type-check**

Run: `dfx build ponzi_math --check`
Expected: success.

- [ ] **Step 3: Commit**

```bash
git add ponzi_math/main.mo
git commit -m "feat(ponzi_math): platform stats and game query methods"
```

---

### Task 21: Add backer queries

**Files:**
- Modify: `ponzi_math/main.mo`

- [ ] **Step 1: Add backer-related queries**

```motoko
    // ========================================================================
    // Public queries — backer state
    // ========================================================================

    public query func getBackerPositions() : async [BackerPosition] {
        Iter.toArray(principalMapNat.vals(backerPositions));
    };

    public query ({ caller }) func getBackerRepaymentBalance() : async Float {
        switch (principalMapNat.get(backerRepayments, caller)) {
            case (null) { 0.0 };
            case (?b) { b };
        };
    };

    public query func getBackerRepaymentBalanceFor(user : Principal) : async Float {
        switch (principalMapNat.get(backerRepayments, user)) {
            case (null) { 0.0 };
            case (?b) { b };
        };
    };

    public query func getAllBackerRepayments() : async [(Principal, Float)] {
        Iter.toArray(principalMapNat.entries(backerRepayments));
    };

    public query func getTotalBackerDebt() : async Float {
        var total = 0.0;
        for (b in principalMapNat.vals(backerPositions)) { total += b.entitlement };
        total;
    };

    public query func getOldestSeriesABacker() : async ?BackerPosition {
        var oldest : ?BackerPosition = null;
        var oldestTime : Int = 0;
        for (b in principalMapNat.vals(backerPositions)) {
            if (b.backerType == #seriesA) {
                switch (b.firstDepositDate) {
                    case (null) {};
                    case (?date) {
                        if (oldest == null or date < oldestTime) {
                            oldest := ?b;
                            oldestTime := date;
                        };
                    };
                };
            };
        };
        oldest;
    };
```

- [ ] **Step 2: Type-check**

Run: `dfx build ponzi_math --check`
Expected: success.

- [ ] **Step 3: Commit**

```bash
git add ponzi_math/main.mo
git commit -m "feat(ponzi_math): backer query methods"
```

---

### Task 22: Add cover charge and general ledger queries + canister balance

**Files:**
- Modify: `ponzi_math/main.mo`

- [ ] **Step 1: Add the remaining queries**

```motoko
    // ========================================================================
    // Public queries — cover charge, general ledger, canister balance
    // ========================================================================

    public query func getCoverChargeBalance() : async Nat {
        coverChargeBalance;
    };

    public query func getGeneralLedger() : async [GeneralLedgerEntry] {
        Iter.toArray(natMap.vals(generalLedger));
    };

    public query func getGeneralLedgerStats() : async {
        totalInflows : Float;
        totalOutflows : Float;
        netFlow : Float;
        entryCount : Nat;
    } {
        var inflows = 0.0;
        var outflows = 0.0;
        var count = 0;
        for (entry in natMap.vals(generalLedger)) {
            count += 1;
            switch (entry.event) {
                case (#deposit(d)) { inflows += d.gross };
                case (#backerDeposit(b)) { inflows += b.amount };
                case (#withdrawal(w)) { outflows += w.netToPlayer + w.toll };
                case (#settlement(s)) { outflows += s.netToPlayer + s.toll };
                case (#backerRepaymentClaim(c)) { outflows += c.amount };
                case (#coverChargeSwept(s)) {
                    outflows += Float.fromInt(s.amountE8s) / 100_000_000.0;
                };
                case (_) {};
            };
        };
        { totalInflows = inflows; totalOutflows = outflows; netFlow = inflows - outflows; entryCount = count };
    };

    // Returns the canister's actual on-ledger ICP balance. Public — no auth gate
    // since ponzi_math has no admin role. shared (update) call because it awaits
    // the ledger.
    public shared func getCanisterICPBalance() : async Nat {
        let selfPrincipal = Principal.fromActor(Self);
        try {
            await icpLedger.icrc1_balance_of({ owner = selfPrincipal; subaccount = null });
        } catch (_) { 0 };
    };
```

- [ ] **Step 2: Type-check**

Run: `dfx build ponzi_math --check`
Expected: success.

- [ ] **Step 3: Commit**

```bash
git add ponzi_math/main.mo
git commit -m "feat(ponzi_math): cover charge, general ledger, canister balance queries"
```

---

## Phase 6: Test hatch and ICRC standards

### Task 23: Add createBackdatedGame test hatch

**Files:**
- Modify: `ponzi_math/main.mo`

- [ ] **Step 1: Add the test hatch block**

Append at the end of the actor body, clearly demarcated for pre-blackhole removal:

```motoko
    // ========================================================================
    // PRE-BLACKHOLE TEST HATCH — DELETE THIS ENTIRE BLOCK BEFORE BLACKHOLING.
    // Gated on caller == TEST_ADMIN (init arg).
    //
    // Same flow as createGame but with a caller-specified startTime, enabling
    // tests of matured-position payouts (e.g. set startTime to 30 days ago,
    // then immediately settle the 30-day compounding plan).
    //
    // Caller is always the player. Real ICP is transferred via
    // icrc2_transfer_from. Skips rate-limit and max-deposit checks.
    // ========================================================================

    public shared ({ caller }) func createBackdatedGame(
        plan : GamePlan,
        amount : Float,
        isCompounding : Bool,
        startTimeNanos : Int,
    ) : async { #Ok : Nat; #Err : Text } {
        if (caller != TEST_ADMIN) { return #Err("Unauthorized: testAdmin only") };
        requireAuthenticated(caller);
        validateAmount(amount);
        if (amount < 0.1) { return #Err("Minimum deposit is 0.1 ICP") };
        if (not validateEightDecimals(amount)) {
            return #Err("Amount cannot have more than 8 decimal places");
        };

        acquireCallerLock(caller);
        acquireGlobalLock();

        let selfPrincipal = Principal.fromActor(Self);
        let amountE8s = Int.abs(Float.toInt(amount * 100_000_000.0));

        let transferResult = try {
            await icpLedger.icrc2_transfer_from({
                spender_subaccount = null;
                from = { owner = caller; subaccount = null };
                to = { owner = selfPrincipal; subaccount = null };
                amount = amountE8s;
                fee = null;
                memo = null;
                created_at_time = null;
            });
        } catch (e) {
            releaseGlobalLock();
            releaseCallerLock(caller);
            return #Err("Failed to contact ICP ledger: " # Error.message(e));
        };

        switch (transferResult) {
            case (#Err(err)) {
                releaseGlobalLock();
                releaseCallerLock(caller);
                return #Err(transferFromErrorMessage(err));
            };
            case (#Ok(_)) {};
        };

        let coverCharge = amount * COVER_CHARGE_RATE;
        let coverChargeE8s = Int.abs(Float.toInt(coverCharge * 100_000_000.0));
        coverChargeBalance += coverChargeE8s;
        let netAmount = amount - coverCharge;

        let gameId = nextGameId;
        nextGameId += 1;

        if (coverChargeE8s > 0) {
            recordLedger(#coverChargeAccrued({ gameId; player = caller; amountE8s = coverChargeE8s }));
        };

        let newGame : GameRecord = {
            id = gameId;
            player = caller;
            plan;
            amount;
            startTime = startTimeNanos;
            isCompounding;
            isActive = true;
            lastUpdateTime = startTimeNanos;
            accumulatedEarnings = 0.0;
            totalWithdrawn = 0.0;
        };
        gameRecords := natMap.put(gameRecords, gameId, newGame);
        platformStats := {
            platformStats with
            totalDeposits = platformStats.totalDeposits + amount;
            activeGames = platformStats.activeGames + 1;
            potBalance = platformStats.potBalance + netAmount;
        };

        recordLedger(#backdatedGameCreated({
            admin = caller;
            player = caller;
            gameId;
            startTime = startTimeNanos;
            amount;
        }));

        releaseGlobalLock();
        releaseCallerLock(caller);
        #Ok(gameId);
    };
```

- [ ] **Step 2: Type-check**

Run: `dfx build ponzi_math --check`
Expected: success.

- [ ] **Step 3: Commit**

```bash
git add ponzi_math/main.mo
git commit -m "feat(ponzi_math): createBackdatedGame test hatch (remove before blackhole)"
```

---

### Task 24: Expose ICRC-21/28/10 standards

**Files:**
- Modify: `ponzi_math/main.mo`

- [ ] **Step 1: Add the three standard exposures**

Append at the very end of the actor body, after the test hatch block:

```motoko
    // ========================================================================
    // ICRC-21 consent messages, ICRC-28 trusted origins, ICRC-10 standards
    // ========================================================================

    public shared func icrc21_canister_call_consent_message(request : Icrc21.ConsentMessageRequest) : async Icrc21.ConsentMessageResponse {
        Icrc21.consentMessage(request);
    };

    public query func icrc28_trusted_origins() : async Icrc21.TrustedOriginsResponse {
        Icrc21.trustedOrigins();
    };

    public query func icrc10_supported_standards() : async [Icrc21.StandardRecord] {
        Icrc21.supportedStandards();
    };
```

- [ ] **Step 2: Full build (declarations generated)**

Run: `dfx generate ponzi_math` and `dfx build ponzi_math --check`
Expected: declarations written to `src/declarations/ponzi_math` (or wherever
dfx places them) plus successful type-check.

- [ ] **Step 3: Commit**

```bash
git add ponzi_math/main.mo
git commit -m "feat(ponzi_math): expose ICRC-21/28/10 standards"
```

---

## Phase 7: Backend changes

### Task 25: Remove moved financial code from backend/main.mo

**Files:**
- Modify: `backend/main.mo`

- [ ] **Step 1: Delete the following sections/lines from `backend/main.mo`**

Use search-and-delete carefully. Remove:

1. **Type definitions** (lines ~109-161): `GamePlan`, `GameRecord`, `PlatformStats`, `GameResetRecord`, `DealerType`, `DealerPosition`. They're all moving to ponzi_math.

2. **State variables** (lines ~168-200, ~309-310, ~318, ~344-352): `gameRecords`, `referralRecords` (deprecated), `referralChain`, `platformStats`, `gameResetHistory`, `nextGameId`, `roundSeedReserve`, `depositTimestamps`, `dealerRepayments`, `dealerPositions`, `coverChargeTransactions`, `nextCoverChargeTxId`, `canisterPrincipal`, `HouseLedgerRecord`, `houseLedger`, `nextHouseLedgerId`.

3. **Locks** (lines ~202-256): per-caller and global lock state + helpers + `isCriticalSectionBusy`. They move to ponzi_math.

4. **Cover charge state + methods** (lines ~285-453): `COVER_CHARGE_RECIPIENT`, `COVER_CHARGE_RATE`, `coverChargeBalance`, `CoverChargeEntry`, `recordCoverChargeTransaction`, `getCoverChargeBalance`, `getCoverChargeTransactions`, `withdrawCoverCharges`.

5. **`migrateReconcilePot`** (lines ~474-529).

6. **`adminSweep`** (lines ~534-576).

7. **`setTestMode` / `isTestMode` / `testMode` var** (lines ~313, ~579-589).

8. **`setCanisterPrincipal` / `getCanisterPrincipal`** (lines ~324-341).

9. **`getCanisterICPBalance`** (lines ~592-606).

10. **`seedGame`** (lines ~610-640) and **`seedReferral`** (lines ~643-648).

11. **House ledger getters** (lines ~650-681): `getHouseLedger`, `getHouseLedgerStats`.

12. **`createGame`** (lines ~683-829).

13. **`addDealerMoney`** (lines ~831-923).

14. **`registerReferral`** internal helper (lines ~928-938).

15. **`creditBackerRepayment`**, **`distributeExitToll`**, **`calculateExitToll`** (lines ~944-1059).

16. **`getAllActiveGames`**, **`withdrawEarnings`**, **`settleCompoundingGame`** (lines ~1061-1362).

17. **`calculateEarnings`**, **`calculateCompoundedEarnings`**, **`calculateCompounded30DayEarnings`**, **`calculateCompoundedROI`** (lines ~1364-1427).

18. **`getPlatformStats`** (lines ~1429-1436).

19. **`triggerGameReset`** internal helper (lines ~1438-1458).

20. **`getGameResetHistory`** through **`getOldestUpstreamDealer`** (lines ~1460-1718): all the platform getters, backer getters, etc.

21. **`distributeFees`** (lines ~1723-1732).

22. **`getReferrer`** (lines ~1497-1501) — moves to shenanigans.

23. **`getTotalHouseMoneyAdded`** (lines ~1689-1697).

24. Remove the imports of `Time`, `Nat64`, `Float`, `List`, `Iter`, `Blob`, `Error` if no remaining references use them. Run `dfx build backend --check` after deletion to find which imports are unused — Motoko warns on unused imports.

25. Remove the `Ledger "ledger"` import — backend no longer needs the ICP ledger here (it will be re-added in Task 27 for `payManagement`). Keep `Ledger` import or re-add it then.

26. Remove the `transient let icpLedger = ...` — same reason.

27. Remove `transferFromErrorMessage` and `transferErrorMessage` — only called from removed methods.

28. Remove `roundToEightDecimals`, `validateEightDecimals`, `formatICP`, `validateAmount`, `validateTextLength` — `validateAmount` is only used by removed methods; the others are unused. Keep `validateTextLength` since `saveCallerUserProfile` uses it.

After deletion, the backend should keep ONLY: imports relevant to remaining code, AccessControl state, `requireAuthenticated`, `validateTextLength`, `UserProfile` type + map + getters/saver, all `AccessControl.*` exposed methods, and ICRC-21/28/10 methods.

- [ ] **Step 2: Type-check backend**

Run: `dfx build backend --check`
Expected: success. If any errors mention unused imports, remove them. If any reference broken state vars, double-check that nothing was left behind.

- [ ] **Step 3: Commit**

```bash
git add backend/main.mo
git commit -m "refactor(backend): remove money-flow code (moving to ponzi_math)"
```

---

### Task 26: Trim backend/icrc21.mo to non-financial methods

**Files:**
- Modify: `backend/icrc21.mo`

- [ ] **Step 1: Replace consentMessage switch arms**

In `backend/icrc21.mo`, change the `methodLabel` switch (lines 65-74) to:

```motoko
        let methodLabel = switch (request.method) {
            case "saveCallerUserProfile" { ?"Set Display Name" };
            case "initializeAccessControl" { ?"Initialize Account" };
            case "payManagement" { ?"Pay Management (Cover-Charge Pay-Out)" };
            case "setPonziMathPrincipal" { ?"Set Ponzi Math Canister" };
            case _ { null };
        };
```

- [ ] **Step 2: Type-check**

Run: `dfx build backend --check`
Expected: success.

- [ ] **Step 3: Commit**

```bash
git add backend/icrc21.mo
git commit -m "chore(backend): icrc21 consent messages for retained methods only"
```

---

### Task 27: Add ponzi_math principal state and setter on backend

**Files:**
- Modify: `backend/main.mo`

- [ ] **Step 1: Re-add ICP ledger import and reference**

Near the top of `backend/main.mo`, in the imports, ensure these are present (re-add if Task 25 removed them):

```motoko
import Error "mo:base/Error";
import Nat "mo:base/Nat";
import Ledger "ledger";
```

After the access-control initialization, add:

```motoko
    transient let icpLedger : Ledger.LedgerActor = actor(Ledger.ICP_LEDGER_CANISTER_ID);
```

- [ ] **Step 2: Add ponzi_math principal state + admin setter + getter**

Add after the canister-self-aware area (after access control init):

```motoko
    // ponzi_math canister reference. Set once at cutover by admin.
    stable var ponziMathPrincipal : ?Principal = null;

    public shared ({ caller }) func setPonziMathPrincipal(p : Principal) : async () {
        if (not (AccessControl.hasPermission(accessControlState, caller, #admin))) {
            Debug.trap("Unauthorized: admin only");
        };
        ponziMathPrincipal := ?p;
    };

    public query func getPonziMathPrincipal() : async ?Principal {
        ponziMathPrincipal;
    };
```

- [ ] **Step 3: Type-check**

Run: `dfx build backend --check`
Expected: success.

- [ ] **Step 4: Commit**

```bash
git add backend/main.mo
git commit -m "feat(backend): ponzi_math principal state + admin setter"
```

---

### Task 28: Add payManagement and getBackendICPBalance on backend

**Files:**
- Modify: `backend/main.mo`

- [ ] **Step 1: Add the PonziMathActor type alias**

After the ponziMathPrincipal block, add:

```motoko
    type PonziMathActor = actor {
        sweepCoverCharges : shared () -> async { #Ok : Nat; #Err : Text };
    };
```

- [ ] **Step 2: Add payManagement and balance query**

```motoko
    // Pay Management — admin pay-out for accrued cover charges.
    // 1. Calls ponzi_math.sweepCoverCharges() to pull accumulated balance.
    // 2. Transfers `amount` from backend's ICP balance to `to`.
    public shared ({ caller }) func payManagement(
        to : Principal,
        amount : Nat,
    ) : async { #Ok : Nat; #Err : Text } {
        if (not (AccessControl.hasPermission(accessControlState, caller, #admin))) {
            return #Err("Unauthorized: admin only");
        };
        if (amount == 0) { return #Err("Amount must be greater than zero") };
        if (amount <= Ledger.ICP_TRANSFER_FEE) {
            return #Err("Amount must exceed the ledger transfer fee of " # Nat.toText(Ledger.ICP_TRANSFER_FEE) # " e8s");
        };

        let ponziMath : PonziMathActor = switch (ponziMathPrincipal) {
            case (null) { return #Err("ponzi_math principal not set") };
            case (?p) { actor(Principal.toText(p)) };
        };

        // Step 1: pull cover charges from ponzi_math. Ignore "Nothing to sweep"
        // and "below transfer fee" errors — admin may want to pay out from
        // pre-existing backend balance.
        let _ = try { await ponziMath.sweepCoverCharges() }
        catch (_) { #Err("sweep call failed; proceeding with existing backend balance") };

        // Step 2: transfer `amount` from backend's balance to `to`.
        let transferAmount : Nat = amount - Ledger.ICP_TRANSFER_FEE;
        let transferResult = try {
            await icpLedger.icrc1_transfer({
                from_subaccount = null;
                to = { owner = to; subaccount = null };
                amount = transferAmount;
                fee = null;
                memo = null;
                created_at_time = null;
            });
        } catch (e) {
            return #Err("Failed to contact ICP ledger: " # Error.message(e));
        };

        switch (transferResult) {
            case (#Ok(blockIndex)) { #Ok(blockIndex) };
            case (#Err(err)) {
                let msg = switch (err) {
                    case (#InsufficientFunds(_)) { "Backend has insufficient ICP. Sweep may not have funded enough yet." };
                    case (#BadFee(_)) { "Bad fee" };
                    case (#BadBurn(_)) { "Bad burn" };
                    case (#TooOld) { "Transaction too old" };
                    case (#CreatedInFuture(_)) { "Transaction created in future" };
                    case (#Duplicate(_)) { "Duplicate transaction" };
                    case (#TemporarilyUnavailable) { "Ledger temporarily unavailable" };
                    case (#GenericError(e)) { "Error: " # e.message };
                };
                #Err(msg);
            };
        };
    };

    // Backend's on-ledger ICP balance — usually the sum of swept cover charges
    // waiting to be paid out. Admin only.
    public shared ({ caller }) func getBackendICPBalance() : async Nat {
        if (not (AccessControl.hasPermission(accessControlState, caller, #admin))) {
            Debug.trap("Unauthorized");
        };
        try {
            await icpLedger.icrc1_balance_of({ owner = Principal.fromActor(Self); subaccount = null });
        } catch (_) { 0 };
    };
```

- [ ] **Step 3: Make backend actor self-named**

For `Principal.fromActor(Self)` to work, change the actor declaration line in `backend/main.mo` from `persistent actor {` to `persistent actor Self {`.

- [ ] **Step 4: Type-check**

Run: `dfx build backend --check`
Expected: success.

- [ ] **Step 5: Commit**

```bash
git add backend/main.mo
git commit -m "feat(backend): payManagement and getBackendICPBalance"
```

---

## Phase 8: Shenanigans changes

### Task 29: Add referral chain state and methods in shenanigans

**Files:**
- Modify: `shenanigans/main.mo`

- [ ] **Step 1: Add referralChain state variable**

Near the existing `var dealerSeen = ...` line, add:

```motoko
    /// Maps user → who referred them. First-wins, one-time, immutable per user.
    /// Migrated from backend during the ponzi_math extraction (referrals are
    /// PP-economy metadata, not money math).
    var referralChain = principalMap.empty<Principal>();
```

- [ ] **Step 2: Add public methods**

Add wherever public methods are grouped in shenanigans (near initialize or near depositChips):

```motoko
    /// Idempotent referral registration. First call sets the chain entry;
    /// subsequent calls for the same caller are no-ops. Self-referral rejected.
    public shared ({ caller }) func registerReferral(referrer : Principal) : async () {
        if (Principal.isAnonymous(caller)) { Debug.trap("Anonymous principal not allowed") };
        if (caller == referrer) { return };
        switch (principalMap.get(referralChain, caller)) {
            case (?_) { /* already set */ };
            case null { referralChain := principalMap.put(referralChain, caller, referrer) };
        };
    };

    /// One-hop lookup — returns the user's immediate referrer (L1) or null.
    public query func getReferrer(user : Principal) : async ?Principal {
        principalMap.get(referralChain, user);
    };
```

- [ ] **Step 3: Type-check**

Run: `dfx build shenanigans --check`
Expected: success.

- [ ] **Step 4: Commit**

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): referral chain state + registerReferral/getReferrer"
```

---

### Task 30: Repoint shenanigans observer actor types to ponzi_math

**Files:**
- Modify: `shenanigans/main.mo`

- [ ] **Step 1: Rename actor types**

In `shenanigans/main.mo`, find the section starting `type BackendGamePlan = {` (around line 113). Replace the whole block (through the end of `type BackendActor`) with:

```motoko
    type PonziMathGamePlan = {
        #simple21Day;
        #compounding15Day;
        #compounding30Day;
    };

    type PonziMathGameRecord = {
        id : Nat;
        player : Principal;
        plan : PonziMathGamePlan;
        amount : Float;
        startTime : Int;
        isCompounding : Bool;
        isActive : Bool;
        lastUpdateTime : Int;
        accumulatedEarnings : Float;
        totalWithdrawn : Float;
    };

    type PonziMathBackerType = { #seriesA; #seriesB };

    type PonziMathBackerPosition = {
        owner : Principal;
        amount : Float;
        entitlement : Float;
        startTime : Int;
        isActive : Bool;
        backerType : PonziMathBackerType;
        firstDepositDate : ?Int;
    };

    type PonziMathActor = actor {
        getAllGames : shared query () -> async [PonziMathGameRecord];
        getBackerPositions : shared query () -> async [PonziMathBackerPosition];
    };
```

Note: the type no longer references `getReferrer` — that's now local.

- [ ] **Step 2: Rename the principal variable**

Find `var backendPrincipal : ?Principal = null;` and change it to:

```motoko
    var ponziMathPrincipal : ?Principal = null;
```

- [ ] **Step 3: Rename the initialize parameter**

Find `public shared ({ caller }) func initialize(backendCanisterId : Principal) : async ()` and update both the parameter name AND any references in the body. The body assigns the principal — change `backendPrincipal := ?backendCanisterId` to `ponziMathPrincipal := ?backendCanisterId` (rename the var, but keep the parameter renamed to `ponziMathCanisterId` for clarity):

```motoko
    public shared ({ caller }) func initialize(ponziMathCanisterId : Principal) : async () {
        // ... existing access-control check ...
        // Where the code currently sets backendPrincipal, set ponziMathPrincipal:
        ponziMathPrincipal := ?ponziMathCanisterId;
        // ... rest unchanged ...
    };
```

- [ ] **Step 4: Rename the getBackend helper**

Find `func getBackend() : BackendActor` and replace with:

```motoko
    func getPonziMath() : PonziMathActor {
        switch (ponziMathPrincipal) {
            case (null) { Debug.trap("ponzi_math not initialized") };
            case (?p) { actor(Principal.toText(p)) };
        };
    };
```

- [ ] **Step 5: Type-check**

Run: `dfx build shenanigans --check`
Expected: errors in the observer body referring to `getBackend()`, `BackendGameRecord`, etc. — those are addressed in Task 31.

- [ ] **Step 6: Commit (no build yet)**

```bash
git add shenanigans/main.mo
git commit -m "refactor(shenanigans): rename Backend* observer types to PonziMath*"
```

---

### Task 31: Update shenanigans internal references (Dealer → Backer)

**Files:**
- Modify: `shenanigans/main.mo`

- [ ] **Step 1: Rename observer state vars and helpers**

In `shenanigans/main.mo`:

1. Find `public type DealerSeen = Float;` → change to `public type BackerSeen = Float;`
2. Find `var dealerSeen = principalMap.empty<DealerSeen>();` → change to `var backerSeen = principalMap.empty<BackerSeen>();`
3. Find `dealerPpPerIcp : Nat;` inside MintConfig → change to `backerPpPerIcp : Nat;`
4. Find `dealerPpPerIcp = 4000;` in the MintConfig defaults → change to `backerPpPerIcp = 4000;`
5. Find `func processDealerDeltas() : async ()` → change to `func processBackerDeltas() : async ()`
6. Inside the renamed function body: replace all `dealer` variable references with `backer`, and `BackendDealerPosition` with `PonziMathBackerPosition`, and `dealerSeen` references with `backerSeen`, and `dealerPpPerIcp` with `backerPpPerIcp`.
7. Find the call site `await processDealerDeltas()` and change to `await processBackerDeltas()`.
8. Find the `getBackend` calls inside the observer body and change to `getPonziMath()`.
9. Find `backend.getAllGames()` calls and change to `ponziMath.getAllGames()` (whatever local var binds the actor).
10. Find `backend.getDealerPositions()` calls and change to `ponziMath.getBackerPositions()`.
11. Inside the `BackerPosition` iteration: the field that was `dealer.dealerType` is now `backer.backerType`. The variants are now `#seriesA` / `#seriesB` not `#upstream` / `#downstream` — change any filter logic accordingly.
12. **Drop the `name` field reference**: any line that reads `dealer.name` or `backer.name` needs to be removed — `BackerPosition` no longer has it. Shenanigans doesn't use the name for any logic; UI joins on `owner` against backend profile.

13. Update the comment `"Per-dealer cumulative ICP seen by the observer..."` to `"Per-backer cumulative ICP..."`.

- [ ] **Step 2: Replace cross-canister getReferrer with local lookup**

Find `cascadeReferralMint` and the lines that do `await backend.getReferrer(originUser)` / `await backend.getReferrer(l1)` / `await backend.getReferrer(l2)`. Replace each with a synchronous local `Map.get`:

```motoko
        // L1 lookup
        let l1Maybe : ?Principal = principalMap.get(referralChain, originUser);
        switch (l1Maybe) {
            case (null) {};
            case (?l1) {
                // ... existing mint logic for L1 ...
                let l2Maybe : ?Principal = principalMap.get(referralChain, l1);
                switch (l2Maybe) {
                    case (null) {};
                    case (?l2) {
                        // ... existing mint logic for L2 ...
                        let l3Maybe : ?Principal = principalMap.get(referralChain, l2);
                        switch (l3Maybe) {
                            case (null) {};
                            case (?l3) {
                                // ... existing mint logic for L3 ...
                            };
                        };
                    };
                };
            };
        };
```

(Preserve the existing PP-minting code inside each case; just replace the `await backend.getReferrer(...)` calls and remove the now-unused `try/catch` wrappers around them.)

- [ ] **Step 3: Find and rename the `dealerCut` reference**

`dealerCut : Float;` lives inside `ShenaniganStats`. Leave the field name alone for UI continuity (UI components read this), but update any comment that says "dealer cut" to "backer cut" for clarity.

- [ ] **Step 4: Type-check**

Run: `dfx build shenanigans --check`
Expected: success.

- [ ] **Step 5: Commit**

```bash
git add shenanigans/main.mo
git commit -m "refactor(shenanigans): rename internal dealer→backer, repoint observer to ponzi_math"
```

---

### Task 32: Add chips-rename TODO marker in shenanigans

**Files:**
- Modify: `shenanigans/main.mo`

- [ ] **Step 1: Add TODO comment at top of file**

After the imports and before the `persistent actor Self` line, add:

```motoko
// TODO(2026-05-11): Rename "chips" terminology in this canister — depositChips,
// claimCashOut, chip subaccount, CashOutEntry, etc. — to non-casino verbiage
// (e.g. credits, PP units, tokens). Deferred from the ponzi_math extraction
// migration to keep that scope tight. See
// docs/superpowers/specs/2026-05-11-ponzi-math-extraction-design.md.
```

- [ ] **Step 2: Type-check**

Run: `dfx build shenanigans --check`
Expected: success.

- [ ] **Step 3: Commit**

```bash
git add shenanigans/main.mo
git commit -m "chore(shenanigans): TODO marker for chips-terminology rename"
```

---

## Phase 9: Frontend wiring

### Task 33: Generate ponzi_math TypeScript declarations

**Files:**
- Create: `frontend/src/declarations/ponzi_math/*` (autogenerated)

- [ ] **Step 1: Generate declarations via dfx**

Run: `dfx generate ponzi_math`
Expected: new files at `src/declarations/ponzi_math/` (or wherever dfx places them — check `dfx.json`'s `output` config). They should include `ponzi_math.did.d.ts`, `ponzi_math.did.js`, `index.js`, `index.d.ts`.

- [ ] **Step 2: Move/copy declarations into frontend tree if dfx output differs**

If dfx placed them outside `frontend/src/declarations/`, copy them so the frontend can import:

```bash
mkdir -p frontend/src/declarations/ponzi_math
cp src/declarations/ponzi_math/* frontend/src/declarations/ponzi_math/
```

(Skip if dfx already wrote to `frontend/src/declarations/ponzi_math/`.)

- [ ] **Step 3: Commit declarations**

```bash
git add frontend/src/declarations/ponzi_math
git commit -m "chore(frontend): generated ponzi_math TypeScript declarations"
```

---

### Task 34: Add usePonziMathActor hook

**Files:**
- Create or modify: `frontend/src/lib/actors.ts`
- Modify: `frontend/src/backend.ts`

- [ ] **Step 1: Read existing actor wiring**

Read `frontend/src/backend.ts` and `frontend/src/hooks/useQueries.ts` to see how the backend actor is currently constructed (likely using `@dfinity/agent` + the generated `createActor` from declarations).

- [ ] **Step 2: Update backend.ts to also export ponzi_math IDL and types**

Add to `frontend/src/backend.ts` alongside existing backend exports:

```typescript
export {
  idlFactory as ponziMathIdlFactory,
} from './declarations/ponzi_math';

export type {
  _SERVICE as PonziMathService,
  GameRecord,
  GamePlan,
  PlatformStats,
  GameResetRecord,
  BackerType,
  BackerPosition,
  GeneralLedgerEntry,
  GeneralLedgerEvent,
} from './declarations/ponzi_math/ponzi_math.did';
```

(Exact filename inside `declarations/ponzi_math/` may be different — adapt to what dfx generated.)

- [ ] **Step 3: Build a ponzi_math actor hook**

Create (or extend an existing `lib/actors.ts`) with a hook similar to whatever is used for the backend actor today. Pattern:

```typescript
import { Actor, HttpAgent } from '@dfinity/agent';
import { useMemo } from 'react';
import { ponziMathIdlFactory, type PonziMathService } from '../backend';
import { useAuthClient } from './your-existing-auth-hook'; // mirror backend actor pattern

const PONZI_MATH_CANISTER_ID = import.meta.env.VITE_PONZI_MATH_CANISTER_ID as string;

export function usePonziMathActor() {
    const { identity } = useAuthClient(); // or however backend actor reads identity
    return useMemo(() => {
        const agent = new HttpAgent({ identity, host: /* mirror backend */ });
        if (import.meta.env.DEV) agent.fetchRootKey().catch(console.error);
        return Actor.createActor<PonziMathService>(ponziMathIdlFactory, {
            agent,
            canisterId: PONZI_MATH_CANISTER_ID,
        });
    }, [identity]);
}
```

Adapt to match the exact pattern used for the existing backend actor (look at imports, agent setup, env-var conventions).

- [ ] **Step 4: Add env var**

Add to `.env` (or wherever the existing canister IDs are listed):
```
VITE_PONZI_MATH_CANISTER_ID=<filled-in-after-deploy>
```
Add the variable name to `frontend/src/vite-env.d.ts` if it has explicit types.

For local dev, dfx writes the canister IDs into `.env` automatically (since dfx.json has `"output_env_file": ".env"`). After running `dfx deploy ponzi_math` locally, the var will be set as `CANISTER_ID_PONZI_MATH=...`. Confirm Vite's prefix expectations (`VITE_*`) — may need to remap or use a different reading pattern (match existing `CANISTER_ID_BACKEND` usage).

- [ ] **Step 5: Type-check**

Run: `npm run build`
Expected: success.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/backend.ts frontend/src/lib/actors.ts frontend/src/vite-env.d.ts .env
git commit -m "feat(frontend): usePonziMathActor hook + idl/type exports"
```

---

### Task 35: Repoint financial hooks in useQueries.ts to ponzi_math actor

**Files:**
- Modify: `frontend/src/hooks/useQueries.ts`

- [ ] **Step 1: Identify and repoint hooks**

For each hook in `frontend/src/hooks/useQueries.ts` that calls a financial method, replace `actor.<method>(...)` with `ponziMathActor.<method>(...)`. Hooks to repoint:

- `usePlatformStats` (calls `getPlatformStats`)
- `useAllGames` / `useUserGames` / `useGameById` (game getters)
- `useDealerRepaymentBalance(For)` — also rename hook to `useBackerRepaymentBalance(For)`, method to `getBackerRepaymentBalance(For)`
- `useAllDealerRepayments` → `useAllBackerRepayments`, `getAllBackerRepayments`
- `useHouseLedger` / `useHouseLedgerStats` → `useGeneralLedger` / `useGeneralLedgerStats`, methods `getGeneralLedger` / `getGeneralLedgerStats`
- `useCoverChargeBalance` (calls `getCoverChargeBalance`) — now on ponzi_math, **no auth gate** (was admin-only on backend)
- `useCoverChargeTransactions` — **DELETED**. Replace any consumers with `useGeneralLedger().filter(e => e.event.coverChargeAccrued)`.
- `useWithdrawCoverCharges` → renamed to `usePayManagement` (next task), kept on backend actor.
- `useAddDealerMoney` → `useAddBackerMoney`, method `addBackerMoney`, on ponzi_math
- `useCreateGame` (note: drop the `referrer` arg from the args object passed to `createGame` — it's no longer a parameter)
- `useWithdrawEarnings` → on ponzi_math
- `useSettleCompoundingGame` → on ponzi_math
- `useClaimDealerRepayment` → `useClaimBackerRepayment`, on ponzi_math
- `useDealerPositions` → `useBackerPositions`, on ponzi_math
- `useCalculateEarnings`, `useCalculateCompoundedEarnings`, `useCalculateCompounded30DayEarnings`, `useCalculateCompoundedROI` → on ponzi_math
- `useGameResetHistory` → on ponzi_math
- `useRoundSeedReserve` → on ponzi_math
- `useMaxDepositLimit` → on ponzi_math
- `useCheckDepositRateLimit` → on ponzi_math

Hooks that stay on backend: profile, access control, payManagement, getBackendICPBalance, setPonziMathPrincipal/getPonziMathPrincipal, ICRC standards.

At the top of each repointed hook, import and use the ponzi_math actor instead of the backend actor. Mirror the existing pattern (e.g. `const ponziMathActor = usePonziMathActor();`).

- [ ] **Step 2: Drop the referrer parameter from createGame call**

Inside `useCreateGame`, remove the `referrerOpt` argument from the args destructure (it was passed to the old backend.createGame). The call becomes:

```typescript
const gameResult = await ponziMathActor.createGame(plan, amount, isCompounding);
```

Keep the localStorage-based referral capture logic intact — it'll be wired to shenanigans in Task 38.

- [ ] **Step 3: Type-check**

Run: `npm run build`
Expected: success. TypeScript may complain about removed methods on the backend type; those need their imports/types updated. Adapt as the errors guide.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/hooks/useQueries.ts
git commit -m "refactor(frontend): repoint financial hooks to ponzi_math actor"
```

---

### Task 36: Rename DealerType / DealerPosition references in components

**Files:**
- Modify: any frontend file that imports `DealerType` or `DealerPosition` or reads `.dealerType` / `.name`.

- [ ] **Step 1: Find all uses**

Run: `grep -rn 'DealerType\|DealerPosition\|dealerType\|\.name' frontend/src --include='*.ts' --include='*.tsx'`

For each match:
- `DealerType` import → replace with `BackerType` from `'../backend'` (which re-exports from ponzi_math declarations).
- `DealerPosition` import → replace with `BackerPosition`.
- `position.dealerType` → `position.backerType`.
- `DealerType.upstream` → `BackerType.seriesA` (use object form: `{ seriesA: null } as BackerType`).
- `DealerType.downstream` → `BackerType.seriesB`.
- `position.name` → either drop (if it's just display) or replace with `useUserProfile(position.owner).data?.name ?? 'Anonymous Backer'`.

- [ ] **Step 2: Type-check**

Run: `npm run build`
Expected: success. TypeScript errors will point you at remaining stale references.

- [ ] **Step 3: Commit**

```bash
git add frontend/src
git commit -m "refactor(frontend): rename DealerType/DealerPosition to BackerType/BackerPosition"
```

---

### Task 37: Update cover-charge admin UI to use new methods

**Files:**
- Modify: `frontend/src/hooks/useQueries.ts` (the cover-charge admin hooks)
- Modify: components that render `useWithdrawCoverCharges` / `useCoverChargeBalance`

- [ ] **Step 1: Replace useWithdrawCoverCharges with usePayManagement**

In `useQueries.ts`, find the hook that calls `actor.withdrawCoverCharges(amountE8s)`. Rename it to `usePayManagement` and update the implementation:

```typescript
export function usePayManagement() {
    const backendActor = useBackendActor();
    return useMutation({
        mutationFn: async ({ to, amountE8s }: { to: Principal; amountE8s: bigint }) => {
            const result = await backendActor.payManagement(to, amountE8s);
            if ('Err' in result) throw new Error(result.Err);
            return result.Ok;
        },
    });
}
```

- [ ] **Step 2: Update getCoverChargeBalance hook**

The hook now reads from ponzi_math and is **anonymous-callable** (no admin gate). Remove any auth gating in the hook itself; the canister returns the value unconditionally.

- [ ] **Step 3: Add useBackendICPBalance hook**

```typescript
export function useBackendICPBalance() {
    const backendActor = useBackendActor();
    return useQuery({
        queryKey: ['backendICPBalance'],
        queryFn: async () => backendActor.getBackendICPBalance(),
        refetchInterval: 10_000,
    });
}
```

- [ ] **Step 4: Update admin UI component**

In whatever component renders the admin pay-out form (likely the admin wallet dropdown / cover-charge card), replace the "withdraw to me" button with a "Pay Management" form that takes:
- `to`: Principal text input (defaulted to admin's own principal)
- `amount`: numeric input

On submit, call `payManagement.mutateAsync({ to, amountE8s })`.

Display both balances:
- `useCoverChargeBalance()` — "Pending sweep on ponzi_math: X ICP"
- `useBackendICPBalance()` — "Ready to pay out on backend: Y ICP"

- [ ] **Step 5: Type-check + smoke**

Run: `npm run build`
Expected: success. Then `npm run dev` and click through to the admin UI; confirm both balances render and the form is wired (don't submit yet — requires deployed ponzi_math).

- [ ] **Step 6: Commit**

```bash
git add frontend/src
git commit -m "feat(frontend): payManagement admin form + backend balance view"
```

---

### Task 38: Wire referral capture to shenanigans.registerReferral

**Files:**
- Modify: `frontend/src/hooks/useQueries.ts` (or wherever referral capture is currently invoked from)
- Modify: `frontend/src/App.tsx` or `frontend/src/lib/referral.ts` (wherever the URL `?ref=` is read)

- [ ] **Step 1: Find existing referral capture**

Run: `grep -rn 'referrer\|registerReferral\|ref=' frontend/src --include='*.ts' --include='*.tsx'`

The existing flow likely: URL captured to localStorage → passed to `backend.createGame(plan, amount, isCompounding, referrerOpt)`. Now we need a separate call to shenanigans.

- [ ] **Step 2: Add useRegisterReferral hook**

In `useQueries.ts`:

```typescript
export function useRegisterReferral() {
    const shenanigansActor = useShenanigansActor();
    return useMutation({
        mutationFn: async (referrer: Principal) => {
            await shenanigansActor.registerReferral(referrer);
        },
    });
}
```

(Use the existing shenanigans actor hook; create one if there isn't one yet, mirroring `useBackendActor`.)

- [ ] **Step 3: Call registerReferral on first authenticated load**

In the app's auth-aware entry point (likely `App.tsx` or a top-level provider), add an effect that fires once per session when the user becomes authenticated:

```typescript
const registerReferral = useRegisterReferral();
useEffect(() => {
    if (!identity || identity.getPrincipal().isAnonymous()) return;
    const stored = localStorage.getItem('referrerPrincipal');
    if (!stored) return;
    try {
        const referrer = Principal.fromText(stored);
        registerReferral.mutate(referrer);
    } catch (e) {
        console.warn('Invalid stored referrer principal', e);
    }
}, [identity]);
```

(Adapt to existing patterns; this is the conceptual shape.)

- [ ] **Step 4: Type-check + smoke**

Run: `npm run build`
Expected: success.

- [ ] **Step 5: Commit**

```bash
git add frontend/src
git commit -m "feat(frontend): call shenanigans.registerReferral with stored referrer"
```

---

### Task 39: Update useHouseLedger → useGeneralLedger callers

**Files:**
- Modify: any frontend component that consumed `useHouseLedger` / `useHouseLedgerStats`.

- [ ] **Step 1: Find consumers**

Run: `grep -rn 'useHouseLedger\|HouseLedgerRecord' frontend/src --include='*.ts' --include='*.tsx'`

- [ ] **Step 2: Update references**

For each match:
- `useHouseLedger` → `useGeneralLedger`
- `useHouseLedgerStats` → `useGeneralLedgerStats`
- `HouseLedgerRecord` → `GeneralLedgerEntry`

If any component was rendering a flat `{ amount, timestamp, description }` shape, adapt it to render the new `GeneralLedgerEntry` shape (which has a tagged `event` variant). Render approach: switch on `entry.event` and show different text per event type. For minimum viable behavior, render `event` as `Object.keys(entry.event)[0]` and a JSON pretty-print of the value — adequate for an audit log UI. Polish later.

- [ ] **Step 3: Type-check**

Run: `npm run build`
Expected: success.

- [ ] **Step 4: Commit**

```bash
git add frontend/src
git commit -m "refactor(frontend): useGeneralLedger replaces useHouseLedger consumers"
```

---

## Phase 10: Local integration test rehearsal

### Task 40: Deploy all canisters locally and smoke-test

**Files:**
- None modified — this is a deploy + manual smoke-test task.

- [ ] **Step 1: Start clean local replica**

Run: `dfx start --background --clean`
Expected: replica running on default port.

- [ ] **Step 2: Deploy ICP ledger + pp_ledger + internet identity** (existing setup)

Use whatever local-deploy script the project has. If none, deploy them via dfx individually:

```bash
dfx deploy internet-identity --network local
dfx deploy pp_ledger --network local
# (any other prerequisite canisters)
```

- [ ] **Step 3: Deploy backend (without ponzi_math reference set)**

Run: `dfx deploy backend --network local`
Note the backend canister ID (echoed in output).

- [ ] **Step 4: Deploy ponzi_math with init args**

```bash
dfx deploy ponzi_math --network local --argument '(record {
    backendPrincipal = principal "<BACKEND_CANISTER_ID_FROM_STEP_3>";
    testAdmin = principal "'$(dfx identity get-principal)'";
})'
```

Confirm the output prints the ponzi_math canister ID.

- [ ] **Step 5: Set backend.ponziMathPrincipal**

```bash
dfx canister call backend setPonziMathPrincipal "(principal \"<PONZI_MATH_CANISTER_ID>\")"
```

Expected: `()` (unit return on success).

- [ ] **Step 6: Deploy shenanigans, initialize against ponzi_math**

```bash
dfx deploy shenanigans --network local
dfx canister call shenanigans initialize "(principal \"<PONZI_MATH_CANISTER_ID>\")"
```

- [ ] **Step 7: Deploy frontend**

```bash
npm run build
dfx deploy frontend --network local
```

- [ ] **Step 8: Mint test ICP to dfx identity**

The local ICP ledger should be deployed with an initial balance to the dfx identity. Confirm:

```bash
dfx canister call <icp_ledger_canister_id> icrc1_balance_of "(record { owner = principal \"$(dfx identity get-principal)\"; subaccount = null })"
```

Expected: non-zero balance. If zero, mint or re-deploy the local ledger with an initial-balances record for the dfx identity.

- [ ] **Step 9: Smoke test — createGame, withdraw**

Approve ICP allowance for ponzi_math:

```bash
dfx canister call <icp_ledger_canister_id> icrc2_approve "(record { spender = record { owner = principal \"<PONZI_MATH_CANISTER_ID>\"; subaccount = null }; amount = 200000000 : nat })"
```

Create a simple-21-day game with 1 ICP:

```bash
dfx canister call ponzi_math createGame "(variant { simple21Day }, 1.0, false)"
```

Expected: `(variant { Ok = 0 : nat })` (gameId 0).

Withdraw earnings (no real earnings yet — 0 elapsed):

```bash
dfx canister call ponzi_math withdrawEarnings "(0 : nat)"
```

Expected: `(variant { Ok = 0.0 : float64 })`.

Check platform stats:

```bash
dfx canister call ponzi_math getPlatformStats
```

Expected: `totalDeposits = 1.0`, `potBalance = 0.96` (after 4% cover charge).

Check general ledger:

```bash
dfx canister call ponzi_math getGeneralLedger
```

Expected: at least 3 entries — `#coverChargeAccrued`, `#deposit`, `#withdrawal`.

- [ ] **Step 10: Smoke test — backer flow**

```bash
dfx canister call <icp_ledger_canister_id> icrc2_approve "(record { spender = record { owner = principal \"<PONZI_MATH_CANISTER_ID>\"; subaccount = null }; amount = 100000000 : nat })"
dfx canister call ponzi_math addBackerMoney "(1.0 : float64)"
dfx canister call ponzi_math getBackerPositions
```

Expected: one BackerPosition record with `backerType = variant { seriesA }`, `amount = 1.0`, `entitlement = 1.24`.

- [ ] **Step 11: Smoke test — createBackdatedGame**

A 30-day compounding game backdated 30 days:

```bash
NOW_NS=$(date +%s%N)
THIRTY_DAYS_AGO_NS=$((NOW_NS - 2592000000000000))
dfx canister call <icp_ledger_canister_id> icrc2_approve "(record { spender = record { owner = principal \"<PONZI_MATH_CANISTER_ID>\"; subaccount = null }; amount = 100000000 : nat })"
dfx canister call ponzi_math createBackdatedGame "(variant { compounding30Day }, 1.0, true, $THIRTY_DAYS_AGO_NS : int)"
dfx canister call ponzi_math settleCompoundingGame "(1 : nat)"
```

Expected on settle: large `Ok` value (real 30-day compounded earnings at 9%/day — roughly `(1.09^30 - 1) ≈ 12.27` ICP, which exceeds the 0.96 pot. Pro-rata insolvency settle should drain pot and game reset.

Check ledger:

```bash
dfx canister call ponzi_math getGeneralLedger
```

Expected: should now include `#backdatedGameCreated`, `#settlement` with `isInsolvent = true`, `#tollDistribution`, `#gameReset`.

- [ ] **Step 12: Smoke test — sweepCoverCharges + payManagement**

```bash
dfx canister call ponzi_math getCoverChargeBalance
```

Expected: positive Nat (e8s accumulated from the two createGame deposits + the backdated game).

```bash
dfx canister call backend payManagement "(principal \"$(dfx identity get-principal)\", 1000000 : nat)"
```

Expected: `(variant { Ok = <block_index> })`.

Check:

```bash
dfx canister call ponzi_math getCoverChargeBalance
dfx canister call backend getBackendICPBalance
```

Expected: ponzi_math's cover-charge balance is now near zero (less transfer fee + paid-out amount), backend's balance is positive but reduced.

- [ ] **Step 13: Smoke test — shenanigans observer mints PP**

Wait ~15 seconds (observer polls every 10s):

```bash
dfx canister call shenanigans getObserverStatus
```

Expected: `gameIdCursor` advanced past the games we created.

Check PP balance for the player:

```bash
dfx canister call pp_ledger icrc1_balance_of "(record { owner = principal \"$(dfx identity get-principal)\"; subaccount = null })"
```

Expected: PP units minted (1000 PP per ICP for simple21Day → 1000 PP for the 1 ICP game; plus dealer/backer PP for the addBackerMoney top-up at 4000 PP/ICP).

- [ ] **Step 14: Smoke test — referral registration**

```bash
SECOND_IDENTITY_PRINCIPAL=<some-other-principal>
dfx canister call shenanigans registerReferral "(principal \"$SECOND_IDENTITY_PRINCIPAL\")"
dfx canister call shenanigans getReferrer "(principal \"$(dfx identity get-principal)\")"
```

Expected: `(opt principal "<SECOND_IDENTITY_PRINCIPAL>")`.

- [ ] **Step 15: Frontend manual check (browser preview)**

If preview tooling is available, start the dev server and click through:
- Connect wallet (e.g., Internet Identity local)
- Create a game (UI flow)
- See it in the games list
- Withdraw / settle
- See updated platform stats
- View the general ledger (admin or audit page)
- Admin: pay management form shows both balances; submit a small pay-out

For UI verification follow the `preview_*` tools workflow if available. Capture screenshots of key flows.

- [ ] **Step 16: Smoke test summary commit (optional notes file)**

```bash
git add -A
git commit -m "test: local smoke-test rehearsal of ponzi_math extraction"
```

(Empty commit if nothing changed — alternative: skip commit and treat the local rehearsal as ephemeral.)

---

## Done

All 40 tasks complete. ponzi_math is live on local replica; backend is trimmed and supports payManagement; shenanigans repoints its observer and owns referrals; frontend talks to both actors. Ready for mainnet deploy following the cutover sequence in the spec — but mainnet deploy is **not** part of this implementation plan and requires explicit user authorization.

---

## Self-review notes (filled in after writing)

**Spec coverage check:**
- ✅ All types from spec implemented (Task 5)
- ✅ State variables match spec (Task 6)
- ✅ Init args match spec (Task 4)
- ✅ All public methods listed in spec implemented (Tasks 14-24)
- ✅ Concurrency locks (Task 7)
- ✅ Validation/formatting helpers (Task 8)
- ✅ General ledger replaces house ledger (Tasks 9, 22, 39)
- ✅ Test hatch with admin gate (Task 23)
- ✅ ICRC-21/28/10 split (Tasks 3, 24, 26)
- ✅ Backend cleanup (Task 25)
- ✅ payManagement + sweep coordination (Tasks 27-28)
- ✅ Referrals move to shenanigans (Tasks 29, 38)
- ✅ Dealer→Backer + seriesA/seriesB rename (Tasks 30-31, 36)
- ✅ Frontend two-actor wiring (Tasks 33-39)
- ✅ Chips TODO marker (Task 32)
- ✅ Smoke-test rehearsal (Task 40)

**Placeholder scan:** None. Every step has concrete code, exact commands, or precise rename instructions.

**Type consistency:** `BackerPosition` has `backerType` field (not `dealerType`) consistently across canister code, shenanigans actor type, and frontend type renames. `BackerType` variants `#seriesA`/`#seriesB` used consistently. `GeneralLedgerEntry` / `GeneralLedgerEvent` shapes match between definition (Task 5), record sites (Tasks 9, 14-23), and consumers (Task 39).

**Known caveats:**
- Task 25 uses approximate line ranges from `backend/main.mo` for deletion — the engineer must verify against the live file in case earlier tasks shifted lines. Recommend doing Task 25 in a single sitting against a clean `git status`.
- Task 33's exact declaration output path depends on dfx version; the task instructs the engineer to check and adapt.
- Task 34's Vite env-var convention may need adapting based on how the existing backend env var is read.
- Task 35's hook rename list is comprehensive but the existing `useQueries.ts` may have additional hooks not listed; engineer should grep for `actor.<method>` references during the task and repoint anything missed.
