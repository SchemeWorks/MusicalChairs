# Series B Emergency Equity Conversion — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When a round ends with the pot empty, automatically promote one randomly-selected unprofitable player to Series B backer with entitlement = `(amount − totalWithdrawn) × 1.16`. Refactor the backer schema so a single player can hold both a Series A and a Series B position simultaneously.

**Architecture:**

- **Schema change**: re-key `backerPositions` and `backerRepayments` from `Principal` to compound `BackerKey = (Principal, BackerType)`. A one-shot migration module re-keys existing entries by reading each position's `backerType` field. Repayment buckets that have no matching position (orphans) are dropped.
- **Promotion path**: new `promoteAndReset(reason) : async ()` wrapper replaces direct `triggerGameReset(reason)` calls at four sites in `withdrawEarnings`/`settleCompoundingGame`. It calls `selectPromotionCandidate()` (which does `raw_rand`-based selection with eligibility filtering), then `applySeriesBPromotion()`, then `triggerGameReset()`. If no candidate, just resets.
- **Eligibility (phase 1)**: candidates are players whose principal has zero existing entries in `backerPositions` AND who are underwater (`Σ amount − Σ totalWithdrawn > 0` across active games). If no such candidates exist (phase 2 — everyone has at least one position), fall back to all underwater players.
- **Merge rule**: when the winner already has a Series B position, sum `amount` and `entitlement` into it. Never merge into an existing Series A. A player with only Series A will gain a separate Series B row.
- **Audit**: new `#seriesBPromotion` ledger event captures `{ owner; underwater; entitlement }` at promotion time, recorded before the reset event.
- **Frontend**: per-position repayment tracking requires updating `useGetAllBackerRepayments` consumers from `Map<Principal, Float>` to `Map<BackerKey, Float>`. Lookup keyed by `${principal}-${'seriesA' in type ? 'A' : 'B'}`.

**Tech Stack:**

- Motoko (`persistent actor class` with enhanced orthogonal persistence)
- IC management canister `raw_rand` for randomness
- React/TypeScript frontend with `@dfinity/agent`
- dfx for build/deploy on the IC mainnet

**Important context:**

- The promised behavior is documented at [frontend/src/components/DocsPage.tsx:240](frontend/src/components/DocsPage.tsx:240) — "A random unprofitable player becomes a Series B Backer. Their entitlement equals their losses plus a 16% bonus." This plan implements that promise.
- The constant `DOWNSTREAM_BACKER_BONUS = 0.16` exists at [frontend/src/lib/gameConstants.ts:47](frontend/src/lib/gameConstants.ts:47) but is currently only read by `DocsPage.tsx`. The backend math uses `1.16` inline; we won't add a backend constant unless it's reused elsewhere.
- Mainnet ponzi_math canister ID: `guy42-yqaaa-aaaaj-qr5pq-cai`. Currently has 1 backer position (Rob's Series A: amount 0.6, entitlement 0.744) and 0 repayment entries.
- All ponzi_math upgrades on mainnet require `--wasm-memory-persistence keep` and `--argument '(record { backendPrincipal = principal "5zxxg-tyaaa-aaaac-qeckq-cai"; testAdmin = principal "6pwpo-d5iaw-mfjrn-owfb3-v4oz6-72woh-pc5t2-cwn73-zrzeq-4bjeh-tqe"; })'` (per [icp.yaml](icp.yaml) / cutover precedent).
- Current `BackerPosition` type at [ponzi_math/main.mo:159-167](ponzi_math/main.mo:159). Fields: `owner; amount; entitlement; startTime; isActive; backerType; firstDepositDate`. **Unchanged by this plan** — only the keying changes.

**Rollback strategy:** before deploying the upgrade in Task 11, snapshot current backer state to a file. If the upgrade fails or produces wrong state, revert the source to `main` (or the parent commit of this work) and re-deploy with the same persistence flag — Motoko will see the same stable shape on both sides and skip migration. The snapshot exists as evidence in case manual state restoration is needed.

---

## Task 1: Add BackerKey type, compare function, and map ops binding

**Files:**
- Modify: `ponzi_math/main.mo` — add type + function alongside existing types (around line 152, after BackerType definition) and add ops binding alongside existing map ops (around line 158)

- [ ] **Step 1: Open the file and locate the BackerType definition**

Run: `grep -n "type BackerType" ponzi_math/main.mo`
Expected output: `152:    public type BackerType = {` (or similar line near 152).

- [ ] **Step 2: Add BackerKey type alias immediately after the BackerType declaration block**

Find the lines:

```motoko
    public type BackerType = {
        #seriesA;
        #seriesB;
    };
```

Add immediately after them:

```motoko
    public type BackerKey = (Principal, BackerType);

    func backerKeyCompare(a : BackerKey, b : BackerKey) : { #less; #equal; #greater } {
        switch (Principal.compare(a.0, b.0)) {
            case (#less) #less;
            case (#greater) #greater;
            case (#equal) {
                switch (a.1, b.1) {
                    case (#seriesA, #seriesA) #equal;
                    case (#seriesB, #seriesB) #equal;
                    case (#seriesA, #seriesB) #less;
                    case (#seriesB, #seriesA) #greater;
                };
            };
        };
    };
```

- [ ] **Step 3: Add the backerKeyMap transient ops binding alongside the other map ops**

Find the existing block:

```motoko
    transient let natMap = OrderedMap.Make<Nat>(Nat.compare);
    transient let principalMapNat = OrderedMap.Make<Principal>(Principal.compare);
    transient let intMap = OrderedMap.Make<Int>(Int.compare);
```

Add immediately after:

```motoko
    transient let backerKeyMap = OrderedMap.Make<BackerKey>(backerKeyCompare);
```

- [ ] **Step 4: Build to confirm syntax**

Run: `dfx build --network ic ponzi_math 2>&1 | tail -10`
Expected: `Finished building canisters.` (warnings OK; no errors).

- [ ] **Step 5: Commit**

```bash
git add ponzi_math/main.mo
git commit -m "feat(ponzi_math): add BackerKey compound key type and ops"
```

---

## Task 2: Re-type `backerPositions` and `backerRepayments` to compound key

**Files:**
- Modify: `ponzi_math/main.mo` — change the two stable var type annotations

- [ ] **Step 1: Locate the two var declarations**

Run: `grep -n "var backerPositions\|var backerRepayments" ponzi_math/main.mo`
Expected: two lines like `172:    var backerPositions = ...` and `173:    var backerRepayments = ...`.

- [ ] **Step 2: Change `backerPositions` initialization**

Replace this exact line:

```motoko
    var backerPositions = principalMapNat.empty<BackerPosition>();
```

With:

```motoko
    var backerPositions = backerKeyMap.empty<BackerPosition>();
```

- [ ] **Step 3: Change `backerRepayments` initialization**

Replace this exact line:

```motoko
    var backerRepayments = principalMapNat.empty<Float>();
```

With:

```motoko
    var backerRepayments = backerKeyMap.empty<Float>();
```

- [ ] **Step 4: Build — expect errors at access sites**

Run: `dfx build --network ic ponzi_math 2>&1 | tail -30`
Expected: type errors at every site that uses `principalMapNat.get(backerPositions, …)` or `principalMapNat.put(backerPositions, …)` or similar with `backerRepayments`. **Do not fix yet** — Tasks 3–8 fix each site. The errors are the worklist.

- [ ] **Step 5: Capture the worklist**

Run: `dfx build --network ic ponzi_math 2>&1 | grep -E "principalMapNat\.(get|put|delete|entries|vals) .*\(backer(Positions|Repayments)" | sort -u`

Save the output mentally or to a scratch buffer — every line is a site we'll fix.

- [ ] **Step 6: Commit (yes, broken state — Tasks 3–8 follow immediately)**

```bash
git add ponzi_math/main.mo
git commit -m "feat(ponzi_math): re-type backer maps to compound key (WIP — accessors broken)"
```

---

## Task 3: Write the migration module

**Files:**
- Create: `ponzi_math/migration.mo`

- [ ] **Step 1: Create the file with the migration function**

Write file contents:

```motoko
import OrderedMap "mo:base/OrderedMap";
import Principal "mo:base/Principal";

module {

    type BackerType = { #seriesA; #seriesB };

    type BackerPosition = {
        owner : Principal;
        amount : Float;
        entitlement : Float;
        startTime : Int;
        isActive : Bool;
        backerType : BackerType;
        firstDepositDate : ?Int;
    };

    type BackerKey = (Principal, BackerType);

    func backerKeyCompare(a : BackerKey, b : BackerKey) : { #less; #equal; #greater } {
        switch (Principal.compare(a.0, b.0)) {
            case (#less) #less;
            case (#greater) #greater;
            case (#equal) {
                switch (a.1, b.1) {
                    case (#seriesA, #seriesA) #equal;
                    case (#seriesB, #seriesB) #equal;
                    case (#seriesA, #seriesB) #less;
                    case (#seriesB, #seriesA) #greater;
                };
            };
        };
    };

    // Re-key both backer maps from Principal to (Principal, BackerType).
    // Each old backerPositions entry carries its own backerType field.
    // Each old backerRepayments entry is re-keyed using the matching
    // position's type; orphaned entries (repayment with no position) are
    // dropped — they would never have been paid out anyway.
    public func run(old : {
        var backerPositions : OrderedMap.Map<Principal, BackerPosition>;
        var backerRepayments : OrderedMap.Map<Principal, Float>;
    }) : {
        var backerPositions : OrderedMap.Map<BackerKey, BackerPosition>;
        var backerRepayments : OrderedMap.Map<BackerKey, Float>;
    } {
        let oldOps = OrderedMap.Make<Principal>(Principal.compare);
        let newOps = OrderedMap.Make<BackerKey>(backerKeyCompare);

        var newPositions = newOps.empty<BackerPosition>();
        for ((p, pos) in oldOps.entries(old.backerPositions)) {
            newPositions := newOps.put(newPositions, (p, pos.backerType), pos);
        };

        var newRepayments = newOps.empty<Float>();
        for ((p, r) in oldOps.entries(old.backerRepayments)) {
            switch (oldOps.get(old.backerPositions, p)) {
                case (?pos) {
                    newRepayments := newOps.put(newRepayments, (p, pos.backerType), r);
                };
                case (null) { /* orphan — drop */ };
            };
        };

        {
            var backerPositions = newPositions;
            var backerRepayments = newRepayments;
        };
    };
};
```

- [ ] **Step 2: Wire the migration into the actor**

Open `ponzi_math/main.mo` and locate the actor declaration:

Run: `grep -n "persistent actor class" ponzi_math/main.mo`
Expected: one line like `41:persistent actor class PonziMath(initArgs : {`.

Above that line, add the import (near the other imports at the top of the file):

```motoko
import Migration "migration";
```

Then change the actor declaration line from:

```motoko
persistent actor class PonziMath(initArgs : {
```

To:

```motoko
(with migration = Migration.run)
persistent actor class PonziMath(initArgs : {
```

- [ ] **Step 3: Build to confirm migration module syntax (accessor errors still present, that's fine)**

Run: `dfx build --network ic ponzi_math 2>&1 | tail -5`
Expected: still type errors from Task 2's accessor sites — but **no errors mentioning migration.mo**. If migration.mo has errors, fix before continuing.

- [ ] **Step 4: Commit**

```bash
git add ponzi_math/migration.mo ponzi_math/main.mo
git commit -m "feat(ponzi_math): migration module to re-key backer maps"
```

---

## Task 4: Refactor `addBackerMoney` accessors

**Files:**
- Modify: `ponzi_math/main.mo` — `addBackerMoney` function body (around line 685)

- [ ] **Step 1: Locate the function**

Run: `grep -n "func addBackerMoney" ponzi_math/main.mo`
Expected: one match around line 685.

- [ ] **Step 2: Read the existing lookup/put block to understand context**

Run: `sed -n '680,720p' ponzi_math/main.mo`

You should see a pattern like:

```motoko
switch (principalMapNat.get(backerPositions, caller)) {
    case (null) { /* create new */ ... };
    case (?existing) { /* merge into existing */ ... };
};
```

- [ ] **Step 3: Change `principalMapNat.get` to compound-key lookup**

Replace this exact line:

```motoko
            switch (principalMapNat.get(backerPositions, caller)) {
```

With:

```motoko
            switch (backerKeyMap.get(backerPositions, (caller, #seriesA))) {
```

- [ ] **Step 4: Change both `principalMapNat.put` sites in this function**

Find the line (around 696):

```motoko
                    backerPositions := principalMapNat.put(backerPositions, caller, newBacker);
```

Replace with:

```motoko
                    backerPositions := backerKeyMap.put(backerPositions, (caller, #seriesA), newBacker);
```

Find the line (around 704):

```motoko
                    backerPositions := principalMapNat.put(backerPositions, caller, updated);
```

Replace with:

```motoko
                    backerPositions := backerKeyMap.put(backerPositions, (caller, #seriesA), updated);
```

- [ ] **Step 5: Build — expect remaining errors at other sites, but `addBackerMoney` should be clean**

Run: `dfx build --network ic ponzi_math 2>&1 | grep "addBackerMoney\|line.*685\|line.*696\|line.*704"`
Expected: no errors referencing these line numbers.

- [ ] **Step 6: Commit**

```bash
git add ponzi_math/main.mo
git commit -m "refactor(ponzi_math): addBackerMoney uses compound key for series A"
```

---

## Task 5: Refactor `creditBackerRepayment` to take `BackerKey`, and update `distributeExitToll` call sites

**Files:**
- Modify: `ponzi_math/main.mo` — `creditBackerRepayment` definition (around line 326), `distributeExitToll` body (around line 336)

- [ ] **Step 1: Change `creditBackerRepayment` signature and body**

Find the function definition (line 326):

```motoko
    func creditBackerRepayment(backer : Principal, amount : Float) {
        let current = switch (principalMapNat.get(backerRepayments, backer)) {
            case (null) { 0.0 };
            case (?balance) { balance };
        };
        backerRepayments := principalMapNat.put(backerRepayments, backer, current + amount);
    };
```

Replace the entire function with:

```motoko
    func creditBackerRepayment(key : BackerKey, amount : Float) {
        let current = switch (backerKeyMap.get(backerRepayments, key)) {
            case (null) { 0.0 };
            case (?balance) { balance };
        };
        backerRepayments := backerKeyMap.put(backerRepayments, key, current + amount);
    };
```

- [ ] **Step 2: Update the three callsites in `distributeExitToll`**

Find these three lines (398, 405, 410):

```motoko
            case (?b) { creditBackerRepayment(b.owner, toOldest) };
```

```motoko
            for (b in otherSeriesA.vals()) { creditBackerRepayment(b.owner, perBacker) };
```

```motoko
        for (b in allBackers.vals()) { creditBackerRepayment(b.owner, perAll) };
```

Replace with respectively:

```motoko
            case (?b) { creditBackerRepayment((b.owner, b.backerType), toOldest) };
```

```motoko
            for (b in otherSeriesA.vals()) { creditBackerRepayment((b.owner, b.backerType), perBacker) };
```

```motoko
        for (b in allBackers.vals()) { creditBackerRepayment((b.owner, b.backerType), perAll) };
```

- [ ] **Step 3: Update `allBackers` iteration source**

Find this line in `distributeExitToll` (line 341):

```motoko
        let allBackers = Iter.toArray(principalMapNat.vals(backerPositions));
```

Replace with:

```motoko
        let allBackers = Iter.toArray(backerKeyMap.vals(backerPositions));
```

- [ ] **Step 4: Build**

Run: `dfx build --network ic ponzi_math 2>&1 | tail -15`
Expected: errors only at remaining unmigrated sites (claimBackerRepayment, adminMergeBackerPosition, getBackerPositions, getAllBackerRepayments, total-entitlement iterations).

- [ ] **Step 5: Commit**

```bash
git add ponzi_math/main.mo
git commit -m "refactor(ponzi_math): creditBackerRepayment takes BackerKey"
```

---

## Task 6: Refactor `claimBackerRepayment` for compound key

**Files:**
- Modify: `ponzi_math/main.mo` — `claimBackerRepayment` function (around line 983)

**Design note:** the function currently claims the caller's full repayment balance keyed by principal. After the schema change, a caller may have separate buckets for Series A and Series B. We sum them, transfer once, zero both. This preserves the user-facing behavior ("one claim button") while internally tracking per-type. The corresponding ledger event records the total claimed; if you need per-type breakdown later, it can be added.

- [ ] **Step 1: Read the function to understand current structure**

Run: `sed -n '980,1030p' ponzi_math/main.mo`

You should see the pattern: lookup `caller` in `backerRepayments`, transfer, zero the bucket, restore on transfer failure.

- [ ] **Step 2: Refactor lookups, summation, and writes**

Find this block (around line 988):

```motoko
            let balanceOpt : ?Float = switch (principalMapNat.get(backerRepayments, caller)) {
                case (null) { null };
                case (?b) { ?b };
            };
            let balance = switch (balanceOpt) {
                case (null) { return #Err("No repayment balance") };
                case (?b) {
                    if (b <= 0.0) { return #Err("No repayment balance") };
                    b;
                };
            };
            backerRepayments := principalMapNat.put(backerRepayments, caller, 0.0);
```

Replace with:

```motoko
            let aBalance = switch (backerKeyMap.get(backerRepayments, (caller, #seriesA))) {
                case (null) { 0.0 };
                case (?b) { b };
            };
            let bBalance = switch (backerKeyMap.get(backerRepayments, (caller, #seriesB))) {
                case (null) { 0.0 };
                case (?b) { b };
            };
            let balance = aBalance + bBalance;
            if (balance <= 0.0) { return #Err("No repayment balance") };
            backerRepayments := backerKeyMap.put(backerRepayments, (caller, #seriesA), 0.0);
            backerRepayments := backerKeyMap.put(backerRepayments, (caller, #seriesB), 0.0);
```

- [ ] **Step 3: Update the rollback sites (transfer failure paths)**

Find both lines (around 1010 and 1016):

```motoko
                backerRepayments := principalMapNat.put(backerRepayments, caller, balance);
```

Replace each with a two-line restore:

```motoko
                backerRepayments := backerKeyMap.put(backerRepayments, (caller, #seriesA), aBalance);
                backerRepayments := backerKeyMap.put(backerRepayments, (caller, #seriesB), bBalance);
```

- [ ] **Step 4: Build**

Run: `dfx build --network ic ponzi_math 2>&1 | tail -10`
Expected: errors only at sites in subsequent tasks (adminMergeBackerPosition, queries, total-entitlement iteration).

- [ ] **Step 5: Commit**

```bash
git add ponzi_math/main.mo
git commit -m "refactor(ponzi_math): claimBackerRepayment sums A+B buckets for compound key"
```

---

## Task 7: Refactor `adminMergeBackerPosition` for compound key (Series A only)

**Files:**
- Modify: `ponzi_math/main.mo` — `adminMergeBackerPosition` function (around line 1375)

**Design note:** this test hatch was originally for consolidating Series A smoke-test positions. We keep it Series A-only (it's hard to define a sensible "merge B with A" semantic). If you need to merge B positions later, add a separate `adminMergeSeriesBPosition`.

- [ ] **Step 1: Locate and read the function**

Run: `grep -n "func adminMergeBackerPosition" ponzi_math/main.mo`
Expected: one match around line 1375.

Run: `sed -n '1375,1420p' ponzi_math/main.mo`

- [ ] **Step 2: Update the function body to use Series A compound key**

Find the body — every `principalMapNat.{get,put,delete}(backerPositions, …)` and `principalMapNat.{get,put,delete}(backerRepayments, …)` call. Apply the rule: replace `principalMapNat` with `backerKeyMap`, and replace the bare `Principal` argument with `(principal, #seriesA)`.

Specifically:

Line ~1378:

```motoko
        let fromPos = switch (principalMapNat.get(backerPositions, from)) {
```

becomes:

```motoko
        let fromPos = switch (backerKeyMap.get(backerPositions, (from, #seriesA))) {
```

Line ~1383:

```motoko
        switch (principalMapNat.get(backerPositions, to)) {
```

becomes:

```motoko
        switch (backerKeyMap.get(backerPositions, (to, #seriesA))) {
```

Lines ~1385 and ~1397 (the two `put` sites):

```motoko
                backerPositions := principalMapNat.put(backerPositions, to, …);
```

becomes:

```motoko
                backerPositions := backerKeyMap.put(backerPositions, (to, #seriesA), …);
```

Find the `delete` on `backerPositions`:

```motoko
        backerPositions := principalMapNat.delete(backerPositions, from);
```

becomes:

```motoko
        backerPositions := backerKeyMap.delete(backerPositions, (from, #seriesA));
```

Find the `get`/`put`/`delete` on `backerRepayments` inside this function. Each should change `principalMapNat` → `backerKeyMap` and the principal arg → `(principal, #seriesA)`. For example:

```motoko
        let fromRepay = switch (principalMapNat.get(backerRepayments, from)) {
```

becomes:

```motoko
        let fromRepay = switch (backerKeyMap.get(backerRepayments, (from, #seriesA))) {
```

…and so on for every `principalMapNat` access in this function. There should be 7 total accesses (4 on positions, 3 on repayments).

- [ ] **Step 3: Build**

Run: `dfx build --network ic ponzi_math 2>&1 | tail -10`
Expected: errors only at remaining sites (queries + total-entitlement loop).

- [ ] **Step 4: Commit**

```bash
git add ponzi_math/main.mo
git commit -m "refactor(ponzi_math): adminMergeBackerPosition uses series-A compound key"
```

---

## Task 8: Refactor query functions and total-entitlement iterations

**Files:**
- Modify: `ponzi_math/main.mo` — five sites: two iterations, two lookups, one return type

- [ ] **Step 1: Update `getBackerPositions` iteration**

Find (around line 1180):

```motoko
        Iter.toArray(principalMapNat.vals(backerPositions));
```

Replace with:

```motoko
        Iter.toArray(backerKeyMap.vals(backerPositions));
```

- [ ] **Step 2: Update the two per-caller repayment query lookups**

Find both (around lines 1184 and 1191):

```motoko
        switch (principalMapNat.get(backerRepayments, caller)) {
```

```motoko
        switch (principalMapNat.get(backerRepayments, user)) {
```

For each, change the function to sum both series:

Locate the entire `getBackerRepaymentBalance` function (around line 1183). It currently looks like:

```motoko
    public query ({ caller }) func getBackerRepaymentBalance() : async Float {
        switch (principalMapNat.get(backerRepayments, caller)) {
            case (null) { 0.0 };
            case (?b) { b };
        };
    };
```

Replace with:

```motoko
    public query ({ caller }) func getBackerRepaymentBalance() : async Float {
        let a = switch (backerKeyMap.get(backerRepayments, (caller, #seriesA))) {
            case (null) { 0.0 };
            case (?b) { b };
        };
        let b = switch (backerKeyMap.get(backerRepayments, (caller, #seriesB))) {
            case (null) { 0.0 };
            case (?v) { v };
        };
        a + b;
    };
```

Do the same restructure for `getBackerRepaymentBalanceFor(user : Principal)` (around line 1190).

- [ ] **Step 3: Update `getAllBackerRepayments` return type**

Find (around line 1197):

```motoko
    public query func getAllBackerRepayments() : async [(Principal, Float)] {
        Iter.toArray(principalMapNat.entries(backerRepayments));
    };
```

Replace with:

```motoko
    public query func getAllBackerRepayments() : async [(BackerKey, Float)] {
        Iter.toArray(backerKeyMap.entries(backerRepayments));
    };
```

- [ ] **Step 4: Update total-entitlement iteration (around line 1203)**

Find:

```motoko
        for (b in principalMapNat.vals(backerPositions)) { total += b.entitlement };
```

Replace with:

```motoko
        for (b in backerKeyMap.vals(backerPositions)) { total += b.entitlement };
```

- [ ] **Step 5: Update the other vals iteration (around line 1210)**

Find:

```motoko
        for (b in principalMapNat.vals(backerPositions)) {
```

Replace with:

```motoko
        for (b in backerKeyMap.vals(backerPositions)) {
```

- [ ] **Step 6: Verify there are no remaining backer-map accesses through `principalMapNat`**

Run: `grep -nE "principalMapNat\.(get|put|delete|entries|vals) *\(backer" ponzi_math/main.mo`
Expected: **no output** (empty result).

- [ ] **Step 7: Build — should now compile cleanly**

Run: `dfx build --network ic ponzi_math 2>&1 | tail -10`
Expected: `Finished building canisters.` with no errors. Warnings about unused identifiers or operator-may-trap are pre-existing and OK.

- [ ] **Step 8: Commit**

```bash
git add ponzi_math/main.mo
git commit -m "refactor(ponzi_math): queries + entitlement iterations use compound key"
```

---

## Task 9: Add `#seriesBPromotion` ledger event variant

**Files:**
- Modify: `ponzi_math/main.mo` — `GeneralLedgerEvent` type (around line 83)

- [ ] **Step 1: Locate the GeneralLedgerEvent variant block**

Run: `grep -n "public type GeneralLedgerEvent" ponzi_math/main.mo`
Expected: one match around line 83.

- [ ] **Step 2: Add the new variant**

Find the last variant in the type block (it's `#backdatedGameCreated` around line 144). After its closing `};` and before the type-closing `};`, add:

```motoko
        #seriesBPromotion : {
            owner : Principal;
            underwater : Float;
            entitlement : Float;
        };
```

- [ ] **Step 3: Update the ledger summary statistics (if applicable)**

Run: `grep -n "case (#tollDistribution\|case (#backerRepaymentClaim\|getGeneralLedgerStats" ponzi_math/main.mo`

Find the function that switches on event variants for stats (around line 1245). The new event doesn't represent a cash flow (just an audit row), so add a no-op case at the bottom of the switch:

```motoko
                case (#seriesBPromotion(_)) { /* no cash flow, audit only */ };
```

- [ ] **Step 4: Build**

Run: `dfx build --network ic ponzi_math 2>&1 | tail -5`
Expected: `Finished building canisters.`

- [ ] **Step 5: Commit**

```bash
git add ponzi_math/main.mo
git commit -m "feat(ponzi_math): #seriesBPromotion ledger event variant"
```

---

## Task 10: Implement `selectPromotionCandidate` and `applySeriesBPromotion`

**Files:**
- Modify: `ponzi_math/main.mo` — add two new functions above `triggerGameReset` (around line 430)

- [ ] **Step 1: Add the IC management actor binding (if not already present)**

Run: `grep -n "raw_rand\|aaaaa-aa" ponzi_math/main.mo`

If no match, add the binding near the top-of-actor transient declarations (e.g., after `transient let icpLedger`):

```motoko
    transient let ic : actor { raw_rand : () -> async Blob } = actor "aaaaa-aa";
```

If a match exists, skip this step.

- [ ] **Step 2: Add `selectPromotionCandidate` immediately above `triggerGameReset`**

Run: `grep -n "func triggerGameReset" ponzi_math/main.mo`
Expected: one match around line 430.

Above that line, add:

```motoko
    // Pick a Series B promotion candidate from the current round's losers.
    // Eligibility (phase 1): underwater players who currently have ZERO entries
    // in backerPositions. If none qualify (phase 2 — every underwater player
    // already has a position), fall back to all underwater players. Uses
    // raw_rand for selection — caller must be in an async update context.
    // Returns null if no one is underwater.
    func selectPromotionCandidate() : async ?{ owner : Principal; underwater : Float } {
        // Aggregate underwater (amount - totalWithdrawn) per active player.
        var underwaterByPlayer = principalMapNat.empty<Float>();
        for (g in natMap.vals(gameRecords)) {
            if (g.isActive) {
                let loss = g.amount - g.totalWithdrawn;
                if (loss > 0.0) {
                    let prev = switch (principalMapNat.get(underwaterByPlayer, g.player)) {
                        case (null) { 0.0 };
                        case (?v) { v };
                    };
                    underwaterByPlayer := principalMapNat.put(underwaterByPlayer, g.player, prev + loss);
                };
            };
        };

        let allLosers = Iter.toArray(principalMapNat.entries(underwaterByPlayer));
        if (allLosers.size() == 0) { return null };

        // Phase 1: keep only players who have zero backer positions.
        let withoutBacker = List.toArray(
            List.filter(
                List.fromArray(allLosers),
                func((p, _) : (Principal, Float)) : Bool {
                    let aHas = switch (backerKeyMap.get(backerPositions, (p, #seriesA))) {
                        case (null) { false };
                        case (?_) { true };
                    };
                    let bHas = switch (backerKeyMap.get(backerPositions, (p, #seriesB))) {
                        case (null) { false };
                        case (?_) { true };
                    };
                    not aHas and not bHas;
                },
            )
        );

        // Phase 2: if everyone has a position, fall back to all losers.
        let pool = if (withoutBacker.size() > 0) { withoutBacker } else { allLosers };

        // raw_rand returns a 32-byte blob. Use the first 8 bytes as a nat.
        let entropy = await ic.raw_rand();
        let bytes = Blob.toArray(entropy);
        var seed : Nat = 0;
        var i = 0;
        while (i < 8 and i < bytes.size()) {
            seed := seed * 256 + Nat8.toNat(bytes[i]);
            i += 1;
        };
        let idx = seed % pool.size();
        let (chosen, loss) = pool[idx];
        ?{ owner = chosen; underwater = loss };
    };
```

(Note: `Nat8` and `Blob` are imported. Verify with `grep -n "^import Nat8\|^import Blob" ponzi_math/main.mo`. If `Nat8` is missing, add `import Nat8 "mo:base/Nat8";` at the top with the other imports. `Blob` should already be imported per the file header.)

- [ ] **Step 3: Add `applySeriesBPromotion` immediately after `selectPromotionCandidate`**

```motoko
    // Apply a Series B promotion. If the player already has a Series B
    // position, merge: sum amount and entitlement. Otherwise create a new
    // Series B row alongside any existing Series A.
    func applySeriesBPromotion(owner : Principal, underwater : Float) {
        let entitlement = underwater * 1.16;
        let now = Time.now();
        let key : BackerKey = (owner, #seriesB);
        switch (backerKeyMap.get(backerPositions, key)) {
            case (null) {
                let fresh : BackerPosition = {
                    owner;
                    amount = underwater;
                    entitlement;
                    startTime = now;
                    isActive = true;
                    backerType = #seriesB;
                    firstDepositDate = ?now;
                };
                backerPositions := backerKeyMap.put(backerPositions, key, fresh);
            };
            case (?existing) {
                let merged : BackerPosition = {
                    existing with
                    amount = existing.amount + underwater;
                    entitlement = existing.entitlement + entitlement;
                };
                backerPositions := backerKeyMap.put(backerPositions, key, merged);
            };
        };

        recordLedger(#seriesBPromotion({ owner; underwater; entitlement }));
    };
```

- [ ] **Step 4: Build**

Run: `dfx build --network ic ponzi_math 2>&1 | tail -10`
Expected: `Finished building canisters.`

- [ ] **Step 5: Commit**

```bash
git add ponzi_math/main.mo
git commit -m "feat(ponzi_math): Series B promotion candidate selection + apply helpers"
```

---

## Task 11: Implement `promoteAndReset` and wire the four callsites

**Files:**
- Modify: `ponzi_math/main.mo` — new wrapper above `triggerGameReset`, then four callsite edits

- [ ] **Step 1: Add the `promoteAndReset` wrapper**

Above the `triggerGameReset` definition, add:

```motoko
    // Async wrapper that performs the Series B promotion (if any eligible
    // candidate exists) before zeroing the round state. Used by all four
    // pot-empty paths in withdrawEarnings and settleCompoundingGame.
    func promoteAndReset(reason : Text) : async () {
        switch (await selectPromotionCandidate()) {
            case (?c) { applySeriesBPromotion(c.owner, c.underwater) };
            case (null) { /* nobody underwater — straight reset */ };
        };
        triggerGameReset(reason);
    };
```

- [ ] **Step 2: Find the four callsites**

Run: `grep -n "triggerGameReset(" ponzi_math/main.mo`
Expected: 4 callsites in update-call functions (lines around 769, 909, plus two more inside `settleCompoundingGame`) and 1 in `promoteAndReset` itself (don't touch that one).

- [ ] **Step 3: Replace each external callsite**

For each of the 4 sites (NOT the one inside `promoteAndReset`), change:

```motoko
                    triggerGameReset(actualToll);
```

…wait, that's not the pattern. The actual pattern from the source is:

```motoko
                    triggerGameReset("Insufficient funds for payout (pot empty)");
```

or similar. Replace each with:

```motoko
                    await promoteAndReset("Insufficient funds for payout (pot empty)");
```

(Preserve the original reason text — copy it verbatim.)

Verify after each replacement: `grep -n "triggerGameReset\|promoteAndReset" ponzi_math/main.mo`. The expected end state: 1 reference to `triggerGameReset` inside `promoteAndReset`, and 4 references to `promoteAndReset(...)` in the calling functions.

- [ ] **Step 4: Build**

Run: `dfx build --network ic ponzi_math 2>&1 | tail -10`
Expected: `Finished building canisters.` with no new errors.

- [ ] **Step 5: Commit**

```bash
git add ponzi_math/main.mo
git commit -m "feat(ponzi_math): wire promoteAndReset into pot-empty paths"
```

---

## Task 12: Build, snapshot state, deploy with migration, verify

**Files:** none modified — this task verifies on mainnet.

- [ ] **Step 1: Final clean build**

Run: `dfx build --network ic ponzi_math 2>&1 | tail -5`
Expected: `Finished building canisters.`

- [ ] **Step 2: Snapshot pre-deploy state**

Run:
```bash
dfx canister --network ic call ponzi_math getBackerPositions > /tmp/pre_deploy_positions.txt
dfx canister --network ic call ponzi_math getAllBackerRepayments > /tmp/pre_deploy_repayments.txt
dfx canister --network ic call ponzi_math getPlatformStats > /tmp/pre_deploy_stats.txt
cat /tmp/pre_deploy_positions.txt
```
Expected: shows current backer positions (single Rob Series A position if no new activity since session start).

- [ ] **Step 3: Deploy the upgrade with migration**

Run:
```bash
dfx canister --network ic install ponzi_math --mode upgrade --wasm-memory-persistence keep \
  --argument '(record { backendPrincipal = principal "5zxxg-tyaaa-aaaac-qeckq-cai"; testAdmin = principal "6pwpo-d5iaw-mfjrn-owfb3-v4oz6-72woh-pc5t2-cwn73-zrzeq-4bjeh-tqe"; })' \
  --yes 2>&1 | tail -5
```
Expected: `Upgraded code for canister ponzi_math, with canister ID guy42-yqaaa-aaaaj-qr5pq-cai`.

- [ ] **Step 4: Verify positions survived migration**

Run: `dfx canister --network ic call ponzi_math getBackerPositions`
Expected: same set of positions as in `/tmp/pre_deploy_positions.txt` (same owners, amounts, entitlements, types).

- [ ] **Step 5: Verify repayments survived migration with new shape**

Run: `dfx canister --network ic call ponzi_math getAllBackerRepayments`
Expected: candid output now shows `vec { record { record { principal "..."; variant { seriesA } }; FLOAT } }` (tuple key with type tag) instead of `vec { record { principal "..."; FLOAT } }`. Empty result is also valid if there were no repayments pre-deploy.

- [ ] **Step 6: Verify platformStats unchanged**

Run: `dfx canister --network ic call ponzi_math getPlatformStats`
Expected: same potBalance, activeGames, totalDeposits, totalWithdrawals as `/tmp/pre_deploy_stats.txt`.

- [ ] **Step 7: Commit (no source change — this commit is the verification record)**

```bash
git commit --allow-empty -m "deploy(ponzi_math): Series B refactor live on mainnet"
```

---

## Task 13: Update frontend candid declarations and repayment lookup

**Files:**
- Regenerate: `frontend/src/declarations/ponzi_math/ponzi_math.did.d.ts` (via dfx generate)
- Modify: `frontend/src/hooks/useQueries.ts` — `useGetAllBackerRepayments` consumer
- Modify: `frontend/src/components/HouseDashboard.tsx` — `repaidByOwner` → `repaidByKey`

- [ ] **Step 1: Regenerate TypeScript declarations**

Run: `dfx generate ponzi_math 2>&1 | tail -5`
Expected: succeeds, no errors. (If `dfx generate` is not configured for the project, manually edit `frontend/src/declarations/ponzi_math/ponzi_math.did.d.ts` to update the `getAllBackerRepayments` return type to `Array<[[Principal, BackerType], number]>`.)

Run: `grep -A 3 "getAllBackerRepayments" frontend/src/declarations/ponzi_math/ponzi_math.did.d.ts`
Expected: return type involves a tuple key `[Principal, BackerType]`.

- [ ] **Step 2: Locate frontend consumers**

Run: `grep -rEn "useGetAllBackerRepayments|repaidByOwner|getAllBackerRepayments" frontend/src --include="*.ts" --include="*.tsx"`
Expected output: a hook definition in `useQueries.ts` and consumers in `HouseDashboard.tsx` (and possibly others).

- [ ] **Step 3: Update `HouseDashboard.tsx` repayment lookup**

Find this block (around line 270):

```tsx
  const repaidByOwner = new Map<string, number>(
    repaymentEntries.map(([p, v]) => [p.toString(), v])
  );
```

Replace with:

```tsx
  const backerKeyId = (principal: string, type: { seriesA?: null; seriesB?: null }) =>
    `${principal}-${'seriesA' in type ? 'A' : 'B'}`;

  const repaidByKey = new Map<string, number>(
    repaymentEntries.map(([[p, t], v]) => [backerKeyId(p.toString(), t), v])
  );
```

Then find the per-backer usage (around line 296):

```tsx
  const totalRepaid = backers.reduce(
    (s, d) => s + (repaidByOwner.get(d.owner.toString()) || 0),
    0,
  );
```

Replace with:

```tsx
  const totalRepaid = backers.reduce(
    (s, d) => s + (repaidByKey.get(backerKeyId(d.owner.toString(), d.backerType)) || 0),
    0,
  );
```

And the per-row usage (around line 332):

```tsx
            const repaid = repaidByOwner.get(backer.owner.toString()) || 0;
```

Replace with:

```tsx
            const repaid = repaidByKey.get(backerKeyId(backer.owner.toString(), backer.backerType)) || 0;
```

- [ ] **Step 4: Build the frontend**

Run: `npm run build 2>&1 | tail -10`
Expected: `✓ built in N.NNs` with no TypeScript errors.

- [ ] **Step 5: Deploy the frontend**

Run:
```bash
dfx build --network ic frontend 2>&1 | tail -5
dfx canister --network ic install frontend --mode upgrade --yes 2>&1 | tail -5
```
Expected: `Upgraded code for canister frontend, with canister ID 5qu42-fqaaa-aaaac-qecla-cai`.

- [ ] **Step 6: Verify live frontend serves the new bundle**

Run:
```bash
NEW_HASH=$(grep -oE 'index-[A-Za-z0-9_-]+\.js' frontend/dist/index.html | head -1)
LIVE_HASH=$(curl -sS "https://5qu42-fqaaa-aaaac-qecla-cai.icp0.io/" | grep -oE 'index-[A-Za-z0-9_-]+\.js' | head -1)
echo "local=$NEW_HASH live=$LIVE_HASH"
```
Expected: `local` and `live` match.

- [ ] **Step 7: Open https://musicalchairs.fun in a browser, navigate to the SEED ROUND tab, and confirm**

- Your Series A row still shows with the correct amount/entitlement
- No console errors mentioning `repaidByOwner` or undefined property access on a tuple
- The "repaid" / "remaining" numbers render

(This is the one manual step. Take a screenshot if anything looks off.)

- [ ] **Step 8: Commit**

```bash
git add frontend/src
git commit -m "feat(frontend): repayment lookup uses BackerKey for per-position tracking"
```

---

## Task 14: Open the PR

**Files:** none modified — this task opens the PR.

- [ ] **Step 1: Confirm the branch is up to date with all commits**

Run: `git log --oneline origin/main..HEAD | head -20`
Expected: ~10–14 commits covering Tasks 1–13.

- [ ] **Step 2: Push the branch**

Run: `git push 2>&1 | tail -5`
Expected: branch pushed.

- [ ] **Step 3: Open the PR**

Run:
```bash
gh pr create --title "Series B emergency equity conversion" --body "$(cat <<'EOF'
## Summary

Implements the long-promised "random unprofitable player gets a Series B backer position" mechanic. Three structural pieces:

- **Schema refactor**: `backerPositions` and `backerRepayments` are now keyed by `(Principal, BackerType)` instead of `Principal`. A migration module re-keys existing entries using each position's stored `backerType`. Repayment entries that have no matching position (orphans) are dropped.
- **Promotion path**: a new async `promoteAndReset(reason)` wrapper replaces the four direct `triggerGameReset(reason)` callsites in `withdrawEarnings` and `settleCompoundingGame`. It calls `raw_rand()` to pick one underwater player, applying eligibility filtering — players with zero existing backer positions get priority; if everyone has a position, the pool opens to all losers. The winner gets a Series B row with `entitlement = (amount − totalWithdrawn) × 1.16`. Existing Series B holders merge; existing Series A holders gain a separate row.
- **Audit**: new `#seriesBPromotion` ledger event records `{ owner; underwater; entitlement }` at each promotion.

## Frontend changes

- `useGetAllBackerRepayments` candid shape changed to `Array<[[Principal, BackerType], Float]>`.
- `HouseDashboard.tsx` repayment lookup is now keyed by `${principal}-${A|B}` so a player with both A and B rows sees the correct repaid/remaining numbers on each.

## Test plan

- [x] `dfx build` clean
- [x] Mainnet upgrade with migration succeeded; pre/post snapshots match
- [ ] Manual: SEED ROUND tab renders without console errors and shows correct repaid/remaining per row
- [ ] Manual: simulate a pot-empty event with a backdated unprofitable game and confirm a Series B position appears in `getBackerPositions` and a `#seriesBPromotion` entry in the general ledger

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```
Expected: PR URL printed.

---

## Self-review notes (author)

- **Spec coverage**: every numbered design rule from the user's spec (underwater definition, uniform random selection, ×1.16 entitlement, eligibility-then-fallback, merge B-into-B but never B-into-A, skip when no one underwater) maps to a task. The phase-2 "everyone has a position" branch is handled by Task 10 Step 2's `pool` fallback.
- **No placeholders**: every code block contains exact code. No "implement appropriate error handling" gestures.
- **Type consistency**: `BackerKey` defined in Task 1 is used identically in Tasks 3, 4, 5, 6, 7, 8, 10, 11. The `backerKeyCompare` function appears once in main.mo (Task 1) and once in migration.mo (Task 3) — duplicated intentionally because migrations cannot import from main code paths (per the migrating-motoko skill rule).
- **Migration safety**: Task 12 snapshots state before deploying. If migration fails, the canister stays on old code and the snapshot is still valid for a manual restore via a reverse migration. The `--wasm-memory-persistence keep` flag is required (per the cutover precedent).
- **Frontend deployment ordering**: Task 12 (backend deploy) happens before Task 13 (frontend deploy). There's a window (~30s of build + deploy time) where the frontend still expects the old candid shape but ponzi_math emits the new one. For a single-user system this is acceptable; if it becomes a concern, frontend changes can be made defensive (handle both shapes) and deployed first in a follow-up.
