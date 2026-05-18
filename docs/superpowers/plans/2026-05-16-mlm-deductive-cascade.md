# MLM Deductive Cascade — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the additive referral cascade in `shenanigans` with a deductive 10%/50%-passthrough cascade per [the spec](../specs/2026-05-16-mlm-deductive-cascade-design.md). Add a 500 PP signup gift gated on first deposit. Expose `recentSignups` for the frontend toast/badge.

**Architecture:** All work lives in `shenanigans/main.mo` (one canister, one file) plus a one-shot extension to `shenanigans/migration.mo` for the `MintConfig` shape change. New stable state is declared in `main.mo`; seeding (grandfather signupGiftClaimed, backfill referrerToDownline, seed lastQualifyingDeposit) runs in an admin-callable one-shot function `seedMigrationV2`. Frontend changes are deferred per the spec.

**Tech Stack:** Motoko (persistent actor + enhanced orthogonal persistence), dfx, candid. No tests directory exists in this project — verification is `dfx build` (typecheck) + local-replica deploy + `dfx canister call` smoke tests.

---

## File map

| File | Action | Responsibility |
|---|---|---|
| `shenanigans/main.mo` | Modify | Add new stable state, new `MintConfig` fields, helpers (`house`, `getPayoutTarget`, `isActive`), `distributeDeductiveCascade` walker, signup-gift logic, 5 admin setters, extended `getReferralStats`, one-shot `seedMigrationV2`. Remove old `cascadeReferralMint` once unused. |
| `shenanigans/migration.mo` | Modify | Extend with `OldMintConfigV3` → `NewMintConfigV3` migration to add 5 new fields with safe defaults during the upgrade. |

No changes to `ponzi_math/`, `backend/`, frontend, or `dfx.json`.

## Verification approach

The project has no test suite. Each task ends in a build/smoke check:

- **Typecheck:** `dfx build shenanigans` from the project root. Must exit 0.
- **Local deploy (final task only):** `dfx start --background --clean` then `dfx deploy shenanigans` + admin `seedMigrationV2` + functional canister calls.
- **Live ID check:** mainnet canister ID is `j56tm-oaaaa-aaaac-qf34q-cai` (per `canister_ids.json`). DO NOT deploy to mainnet from this plan — that requires explicit user permission per the project's memory.

After every task: `git add <files> && git commit -m "<message>"`. Commit per task, not in batches.

---

## Task 1: Extend `migration.mo` to handle the new `MintConfig` shape

**Files:**
- Modify: `shenanigans/migration.mo`

The existing migration handled a prior Dealer→Backer rename. We add a fresh, parallel V3 migration that extends `MintConfig` with the 5 new admin-tunable fields. The old `run` function is preserved for any environments still running the V2 schema — the actor wires up V3.

- [ ] **Step 1: Replace `migration.mo` with the V3-aware version**

Open `shenanigans/migration.mo` and replace its full contents with:

```motoko
import OrderedMap "mo:base/OrderedMap";
import Principal "mo:base/Principal";

module {

    // ================================================================
    // V2 types — Dealer→Backer rename (already deployed; left for reference)
    // ================================================================

    type V2OldMintConfig = {
        simple21DayPpPerIcp : Nat;
        compounding15DayPpPerIcp : Nat;
        compounding30DayPpPerIcp : Nat;
        dealerPpPerIcp : Nat;
        referralL1Bps : Nat;
        referralL2Bps : Nat;
        referralL3Bps : Nat;
        minDepositPp : Nat;
        cashOutDelaySeconds : Nat;
        observerIntervalSeconds : Nat;
    };

    type V2NewMintConfig = {
        simple21DayPpPerIcp : Nat;
        compounding15DayPpPerIcp : Nat;
        compounding30DayPpPerIcp : Nat;
        backerPpPerIcp : Nat;
        referralL1Bps : Nat;
        referralL2Bps : Nat;
        referralL3Bps : Nat;
        minDepositPp : Nat;
        cashOutDelaySeconds : Nat;
        observerIntervalSeconds : Nat;
    };

    type SeenMap = OrderedMap.Map<Principal, Float>;

    public func runV2(old : {
        var mintConfig : V2OldMintConfig;
        var backendPrincipal : ?Principal;
        var dealerSeen : SeenMap;
    }) : {
        var mintConfig : V2NewMintConfig;
        var ponziMathPrincipal : ?Principal;
        var backerSeen : SeenMap;
    } {
        let oldCfg = old.mintConfig;
        {
            var mintConfig = {
                simple21DayPpPerIcp = oldCfg.simple21DayPpPerIcp;
                compounding15DayPpPerIcp = oldCfg.compounding15DayPpPerIcp;
                compounding30DayPpPerIcp = oldCfg.compounding30DayPpPerIcp;
                backerPpPerIcp = oldCfg.dealerPpPerIcp;
                referralL1Bps = oldCfg.referralL1Bps;
                referralL2Bps = oldCfg.referralL2Bps;
                referralL3Bps = oldCfg.referralL3Bps;
                minDepositPp = oldCfg.minDepositPp;
                cashOutDelaySeconds = oldCfg.cashOutDelaySeconds;
                observerIntervalSeconds = oldCfg.observerIntervalSeconds;
            };
            var ponziMathPrincipal = old.backendPrincipal;
            var backerSeen = old.dealerSeen;
        };
    };

    // ================================================================
    // V3 — Deductive cascade rollout
    //
    // Extends MintConfig with 5 admin-tunable fields:
    //   cascadeInitialBps       (10% deduction off the top)
    //   cascadePassthroughBps   (50% kept by each active upline)
    //   signupGiftPp            (500 PP signup gift; 0 disables)
    //   activityRequiresDeposit (cascade skips inactive uplines)
    //   activityWindowDays      (null = lifetime; ?n = last n days)
    //
    // Old MintConfig fields are preserved verbatim. Old referralL[1-3]Bps
    // remain on the record (deprecated, unused by the new cascade) so the
    // candid signature stays stable for admin tooling that reads them.
    // ================================================================

    type V3OldMintConfig = V2NewMintConfig;

    type V3NewMintConfig = {
        simple21DayPpPerIcp : Nat;
        compounding15DayPpPerIcp : Nat;
        compounding30DayPpPerIcp : Nat;
        backerPpPerIcp : Nat;
        referralL1Bps : Nat;       // deprecated; unused by new cascade
        referralL2Bps : Nat;       // deprecated; unused by new cascade
        referralL3Bps : Nat;       // deprecated; unused by new cascade
        minDepositPp : Nat;
        cashOutDelaySeconds : Nat;
        observerIntervalSeconds : Nat;
        cascadeInitialBps : Nat;
        cascadePassthroughBps : Nat;
        signupGiftPp : Nat;
        activityRequiresDeposit : Bool;
        activityWindowDays : ?Nat;
    };

    public func runV3(old : { var mintConfig : V3OldMintConfig }) : { var mintConfig : V3NewMintConfig } {
        let o = old.mintConfig;
        {
            var mintConfig = {
                simple21DayPpPerIcp = o.simple21DayPpPerIcp;
                compounding15DayPpPerIcp = o.compounding15DayPpPerIcp;
                compounding30DayPpPerIcp = o.compounding30DayPpPerIcp;
                backerPpPerIcp = o.backerPpPerIcp;
                referralL1Bps = o.referralL1Bps;
                referralL2Bps = o.referralL2Bps;
                referralL3Bps = o.referralL3Bps;
                minDepositPp = o.minDepositPp;
                cashOutDelaySeconds = o.cashOutDelaySeconds;
                observerIntervalSeconds = o.observerIntervalSeconds;
                cascadeInitialBps = 1000;        // 10%
                cascadePassthroughBps = 5000;    // 50%
                signupGiftPp = 500;
                activityRequiresDeposit = true;
                activityWindowDays = null;       // lifetime
            };
        };
    };
};
```

- [ ] **Step 2: Build to typecheck**

Run from project root:
```bash
dfx build shenanigans
```
Expected: build succeeds, output ends with `WARNING` lines (existing) but no errors. If you see a candid mismatch or syntax error, fix it before proceeding.

- [ ] **Step 3: Commit**

```bash
git add shenanigans/migration.mo
git commit -m "$(cat <<'EOF'
feat(shenanigans): add Migration.runV3 for deductive-cascade MintConfig

Extends MintConfig with cascadeInitialBps/cascadePassthroughBps,
signupGiftPp, activityRequiresDeposit, activityWindowDays. Defaults
mirror the design spec (10%/50%/500/true/null). Old referralL[1-3]Bps
fields stay on the record (deprecated) so candid stays stable.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Wire `Migration.runV3` into the `persistent actor`

**Files:**
- Modify: `shenanigans/main.mo` lines 27-30 (the `persistent actor Self {` declaration)

We hook `runV3` via the inline migration syntax so the next upgrade rebuilds `mintConfig` with the 5 new fields populated.

- [ ] **Step 1: Add the Migration import**

In `shenanigans/main.mo`, locate the import block (lines 1-19). After `import Subaccount "Subaccount";` add:

```motoko
import Migration "migration";
```

- [ ] **Step 2: Decorate the actor with the V3 migration**

Find line 27: `persistent actor Self {`. Replace with:

```motoko
(with migration = Migration.runV3)
persistent actor Self {
```

- [ ] **Step 3: Build to typecheck**

```bash
dfx build shenanigans
```
Expected: build succeeds.

- [ ] **Step 4: Commit**

```bash
git add shenanigans/main.mo
git commit -m "$(cat <<'EOF'
feat(shenanigans): hook Migration.runV3 onto persistent actor

Upgrade now extends MintConfig with the 5 deductive-cascade fields.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: Add the 5 new `MintConfig` fields to the actor's local type + defaults

**Files:**
- Modify: `shenanigans/main.mo` — the `MintConfig` type declaration (around lines 92-110) and the `var mintConfig : MintConfig = {...}` initializer (around lines 225-235)

The migration rebuilds existing canister state; the actor's local type must match the new shape, with default values for the case of a fresh deploy (no migration runs).

- [ ] **Step 1: Locate and update the `MintConfig` type**

In `shenanigans/main.mo`, find the public type declaration:
```motoko
    public type MintConfig = {
        simple21DayPpPerIcp : Nat;
        compounding15DayPpPerIcp : Nat;
        compounding30DayPpPerIcp : Nat;
        backerPpPerIcp : Nat;
        referralL1Bps : Nat;
        referralL2Bps : Nat;
        referralL3Bps : Nat;
        minDepositPp : Nat;
        cashOutDelaySeconds : Nat;
        observerIntervalSeconds : Nat;
    };
```

Replace with:
```motoko
    public type MintConfig = {
        simple21DayPpPerIcp : Nat;
        compounding15DayPpPerIcp : Nat;
        compounding30DayPpPerIcp : Nat;
        backerPpPerIcp : Nat;
        referralL1Bps : Nat;       // deprecated; unused by deductive cascade
        referralL2Bps : Nat;       // deprecated; unused by deductive cascade
        referralL3Bps : Nat;       // deprecated; unused by deductive cascade
        minDepositPp : Nat;
        cashOutDelaySeconds : Nat;
        observerIntervalSeconds : Nat;
        cascadeInitialBps : Nat;
        cascadePassthroughBps : Nat;
        signupGiftPp : Nat;
        activityRequiresDeposit : Bool;
        activityWindowDays : ?Nat;
    };
```

- [ ] **Step 2: Update the initializer**

Find the `var mintConfig : MintConfig = { ... };` declaration. Add the 5 new fields. The full initializer should read:

```motoko
    var mintConfig : MintConfig = {
        simple21DayPpPerIcp = 1000;
        compounding15DayPpPerIcp = 2000;
        compounding30DayPpPerIcp = 3000;
        backerPpPerIcp = 4000;
        referralL1Bps = 800;
        referralL2Bps = 500;
        referralL3Bps = 200;
        minDepositPp = 5000;
        cashOutDelaySeconds = 604_800;
        observerIntervalSeconds = 10;
        cascadeInitialBps = 1000;
        cascadePassthroughBps = 5000;
        signupGiftPp = 500;
        activityRequiresDeposit = true;
        activityWindowDays = null;
    };
```

(Preserve the existing values for the original 10 fields — don't change them. Only add the 5 new ones with the defaults shown.)

- [ ] **Step 3: Build to typecheck**

```bash
dfx build shenanigans
```
Expected: build succeeds.

- [ ] **Step 4: Commit**

```bash
git add shenanigans/main.mo
git commit -m "$(cat <<'EOF'
feat(shenanigans): add 5 new MintConfig fields for deductive cascade

cascadeInitialBps/cascadePassthroughBps/signupGiftPp/
activityRequiresDeposit/activityWindowDays land on the live MintConfig
type. Defaults: 10%/50%/500 PP/true/null (lifetime). Old referralL*Bps
fields stay (deprecated; unused by new cascade).

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: Declare new stable state — `housePrincipal`, `signupGiftClaimed`, `lastQualifyingDeposit`

**Files:**
- Modify: `shenanigans/main.mo` — state declarations section (around lines 205-275)

Three new stable vars. They start empty (`null` / empty maps); seeding happens later via `seedMigrationV2`. Motoko's enhanced OP handles new-var declarations automatically on upgrade — no migration code needed for these.

- [ ] **Step 1: Locate state-vars section**

Find the existing state block. Look for `var referralChain = principalMap.empty<Principal>();` and the cluster of declarations around it (roughly lines 205-275).

- [ ] **Step 2: Add the three new vars near the referral-related state**

Just after the existing `var referralEarnings = principalMap.empty<ReferralEarnings>();` declaration, add:

```motoko
    // ────────────────────────────────────────────────────────────────
    // Deductive-cascade state (added 2026-05-16)
    // ────────────────────────────────────────────────────────────────

    // Catch-all upline + residual destination ("Charles"). Initialized
    // to the deploying admin on first init via seedMigrationV2 when null;
    // admin can override via setHousePrincipal.
    var housePrincipal : ?Principal = null;

    // Per-principal signup-gift claim time. Empty = never claimed.
    // Doubles as the "join time" surfaced via getReferralStats.recentSignups.
    var signupGiftClaimed = principalMap.empty<Int>();

    // Per-principal time of last qualifying deposit (≥ 0.1 ICP). Drives
    // isActive() without per-cascade inter-canister calls. Populated by
    // the observer on every qualifying mint event.
    var lastQualifyingDeposit = principalMap.empty<Int>();
```

- [ ] **Step 3: Build to typecheck**

```bash
dfx build shenanigans
```
Expected: build succeeds.

- [ ] **Step 4: Commit**

```bash
git add shenanigans/main.mo
git commit -m "$(cat <<'EOF'
feat(shenanigans): add housePrincipal + signupGiftClaimed + lastQualifyingDeposit

Three new stable vars for the deductive cascade. housePrincipal is the
catch-all/residual destination. signupGiftClaimed gates the 500 PP gift
and doubles as join-time for toast/badge. lastQualifyingDeposit feeds
isActive() so the cascade can skip dormant uplines.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: Add `CASCADE_DEPTH_CAP` constant + `house()` helper

**Files:**
- Modify: `shenanigans/main.mo` — near other module-level transient constants (look for `transient let REFERRAL_CODE_LEN : Nat = 6;` around line 366)

- [ ] **Step 1: Add the depth-cap constant**

Just below the existing `transient let REFERRAL_CODE_LEN : Nat = 6;` line, add:

```motoko
    transient let CASCADE_DEPTH_CAP : Nat = 10;
```

- [ ] **Step 2: Add the `house()` helper**

Find a sensible location after the state-vars section but before the public methods (after the `bumpReferralEarnings` function is a good neighborhood, but anywhere in the helpers band is fine). Add:

```motoko
    // Resolve the house (catch-all) principal. Falls back to admin if
    // housePrincipal hasn't been seeded yet (defensive — seedMigrationV2
    // initializes it).
    func house() : Principal {
        switch (housePrincipal) {
            case (?p) { p };
            case (null) {
                switch (adminPrincipal) {
                    case (?p) { p };
                    case (null) { Debug.trap("housePrincipal not initialized and no admin set") };
                };
            };
        };
    };
```

- [ ] **Step 3: Build to typecheck**

```bash
dfx build shenanigans
```
Expected: build succeeds. `house()` is unused for now; that's fine — the compiler will let it pass.

- [ ] **Step 4: Commit**

```bash
git add shenanigans/main.mo
git commit -m "$(cat <<'EOF'
feat(shenanigans): add CASCADE_DEPTH_CAP and house() helper

Depth cap is hardcoded 10 (DoS guard, not a tuning lever). house()
resolves the catch-all destination with a defensive fallback to admin.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: Add `getPayoutTarget()` and `isActive()` helpers

**Files:**
- Modify: `shenanigans/main.mo` — same helpers band as `house()`

`getPayoutTarget` is the v2-swap point: v1 returns the referralChain entry or Charles. v2 will rewrite this to traverse NFT ownership. `isActive` consults `lastQualifyingDeposit` against the configured activity window.

- [ ] **Step 1: Add both helpers**

Place these immediately below the `house()` function added in Task 5:

```motoko
    // v1: referralChain.get(current) ?? house(). v2 will swap this to
    // NFT-ownership lookup — keep the function signature stable.
    func getPayoutTarget(current : Principal) : Principal {
        switch (principalMap.get(referralChain, current)) {
            case (?p) { p };
            case (null) { house() };
        };
    };

    // True when the principal meets the configured activity bar.
    // Hot-path: called once per cascade hop. Reads lastQualifyingDeposit
    // (populated by observer) — no inter-canister call here.
    func isActive(p : Principal) : Bool {
        if (not mintConfig.activityRequiresDeposit) { return true };
        switch (principalMap.get(lastQualifyingDeposit, p)) {
            case (null) { false };
            case (?ts) {
                switch (mintConfig.activityWindowDays) {
                    case (null) { true };
                    case (?days) {
                        let now = Time.now();
                        let windowNs : Int = days * 86_400 * 1_000_000_000;
                        (now - ts) <= windowNs;
                    };
                };
            };
        };
    };
```

- [ ] **Step 2: Build to typecheck**

```bash
dfx build shenanigans
```
Expected: build succeeds.

- [ ] **Step 3: Commit**

```bash
git add shenanigans/main.mo
git commit -m "$(cat <<'EOF'
feat(shenanigans): add getPayoutTarget and isActive cascade helpers

getPayoutTarget is the v2-swap point — v1 returns the referralChain
entry or Charles; v2 will rewrite to NFT-ownership lookup. isActive
gates cascade payouts on the configured activity window.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 7: Implement `distributeDeductiveCascade` (new walker)

**Files:**
- Modify: `shenanigans/main.mo` — add the new function alongside the existing `cascadeReferralMint` (do NOT remove the old one yet)

Adding the new function first, leaving the old one untouched, lets each subsequent task switch a single call site over without breaking the canister mid-task.

- [ ] **Step 1: Add the function**

Place this immediately below the existing `cascadeReferralMint` function (around line 888). Use the existing local `principalMap` ordered-map alias and the existing `mintInternal`. **Note:** Motoko's `OrderedMap` doesn't have a `Set` analog conveniently available, so we use `OrderedMap<Principal, ()>` for the visited tracker — same operations, dummy value.

```motoko
    // Deductive cascade: 10% off the top (cascadeInitialBps) distributed
    // up the chain at 50% passthrough (cascadePassthroughBps) per active
    // upline. Inactive uplines are skipped (flow-around). Cycles detected
    // via visited-set. Residual after depth cap → house.
    //
    // Caller is responsible for minting the player's NET (base - cascadeUnits)
    // before invoking. This function only handles the cascade share.
    func distributeDeductiveCascade(originUser : Principal, cascadeUnits : Nat, eventId : Text) : async () {
        if (cascadeUnits == 0) return;

        var remaining : Nat = cascadeUnits;
        var visited = principalMap.empty<()>();
        visited := principalMap.put(visited, originUser, ());

        var depth : Nat = 0;
        var activeRank : Nat = 0;
        var current : Principal = originUser;

        label walk loop {
            if (remaining == 0 or depth >= CASCADE_DEPTH_CAP) { break walk };

            let next = getPayoutTarget(current);
            switch (principalMap.get(visited, next)) {
                case (?_) { break walk }; // cycle — bail to residual
                case (null) {};
            };
            visited := principalMap.put(visited, next, ());
            depth += 1;

            if (not isActive(next)) { current := next; continue walk };

            activeRank += 1;
            let payout = remaining * mintConfig.cascadePassthroughBps / 10_000;
            if (payout == 0) { break walk };

            switch (await mintInternal(next, payout, "cascade-A" # Nat.toText(activeRank) # "-" # eventId)) {
                case (#Ok(_)) {
                    // Display buckets are L1/L2/L3 only. activeRank ≥ 4 still
                    // receives the payout via the mint above; we just don't
                    // inflate the L3 bucket with their share.
                    if (activeRank <= 3) { bumpReferralEarnings(next, activeRank, payout) };
                };
                case (#Err(_)) {};
            };

            remaining -= payout;
            current := next;
        };

        // Residual to house: covers depth cap, cycle break, exhausted chain.
        if (remaining > 0) {
            let _ = await mintInternal(house(), remaining, "cascade-residual-" # eventId);
        };
    };
```

- [ ] **Step 2: Build to typecheck**

```bash
dfx build shenanigans
```
Expected: build succeeds. The new function is currently unused — that's fine.

- [ ] **Step 3: Commit**

```bash
git add shenanigans/main.mo
git commit -m "$(cat <<'EOF'
feat(shenanigans): add distributeDeductiveCascade walker

Implements the deductive cascade: 10% off the top, 50% passthrough per
active upline, depth cap 10 (counts all hops as DoS guard), residual to
house. Active-rank ≤ 3 bumps display buckets; ≥ 4 still receives payout
but doesn't inflate L3 stats.

Caller is responsible for minting playerNet; this function distributes
the cascadeUnits share only. Conservation: caller_net + sum(cascade_payouts)
+ residual = baseUnits.

Left cascadeReferralMint in place; call sites switch over in follow-up
tasks.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 8: Switch `processNewGames` to deductive split — game mints

**Files:**
- Modify: `shenanigans/main.mo` — `processNewGames` function (around lines 487-513)

This task changes the math at the game-mint call site. After this task, every new game record processed by the observer triggers a deductive cascade instead of the additive one. Backer mints and signup gifts come in subsequent tasks.

- [ ] **Step 1: Locate the current `processNewGames` function**

You're looking for the function whose body matches:
```motoko
    func processNewGames() : async () {
        let ponziMath = getPonziMath();
        let games = try { await ponziMath.getAllGames() } catch (_) { [] };
        let sorted = Array.sort<PonziMathGameRecord>(games, func(a, b) = Nat.compare(a.id, b.id));
        for (game in sorted.vals()) {
            if (game.id >= gameIdCursor) {
                let ppPerIcp = switch (game.plan) {
                    case (#simple21Day) { mintConfig.simple21DayPpPerIcp };
                    case (#compounding15Day) { mintConfig.compounding15DayPpPerIcp };
                    case (#compounding30Day) { mintConfig.compounding30DayPpPerIcp };
                };
                let units = icpFloatToPpUnits(game.amount, ppPerIcp);
                let eventId = "game-" # Nat.toText(game.id);
                let res = await mintInternal(game.player, units, eventId);
                switch (res) {
                    case (#Ok(_)) {
                        await cascadeReferralMint(game.player, units, eventId);
                        gameIdCursor := game.id + 1;
                    };
                    case (#Err(msg)) {
                        Debug.print("Mint failed for " # eventId # ": " # msg);
                        return;
                    };
                };
            };
        };
    };
```

- [ ] **Step 2: Replace it with the deductive-split version**

```motoko
    func processNewGames() : async () {
        let ponziMath = getPonziMath();
        let games = try { await ponziMath.getAllGames() } catch (_) { [] };
        let sorted = Array.sort<PonziMathGameRecord>(games, func(a, b) = Nat.compare(a.id, b.id));
        for (game in sorted.vals()) {
            if (game.id >= gameIdCursor) {
                let ppPerIcp = switch (game.plan) {
                    case (#simple21Day) { mintConfig.simple21DayPpPerIcp };
                    case (#compounding15Day) { mintConfig.compounding15DayPpPerIcp };
                    case (#compounding30Day) { mintConfig.compounding30DayPpPerIcp };
                };
                let baseUnits = icpFloatToPpUnits(game.amount, ppPerIcp);
                let cascadeUnits = baseUnits * mintConfig.cascadeInitialBps / 10_000;
                let playerNet : Nat = if (baseUnits > cascadeUnits) { baseUnits - cascadeUnits } else { 0 };
                let eventId = "game-" # Nat.toText(game.id);

                let res = await mintInternal(game.player, playerNet, eventId);
                switch (res) {
                    case (#Ok(_)) {
                        await distributeDeductiveCascade(game.player, cascadeUnits, eventId);
                        // Track qualifying deposit for isActive() — observer is the
                        // single source of truth for activity timestamps.
                        if (game.amount >= 0.1) {
                            lastQualifyingDeposit := principalMap.put(lastQualifyingDeposit, game.player, Time.now());
                        };
                        gameIdCursor := game.id + 1;
                    };
                    case (#Err(msg)) {
                        Debug.print("Mint failed for " # eventId # ": " # msg);
                        return;
                    };
                };
            };
        };
    };
```

Key differences from the original:
- `baseUnits` replaces `units` (more descriptive).
- Computes `cascadeUnits` from `cascadeInitialBps`, derives `playerNet` as `baseUnits - cascadeUnits` (saturates at 0).
- `mintInternal` mints `playerNet` (not `baseUnits`).
- Calls `distributeDeductiveCascade(...cascadeUnits...)` instead of `cascadeReferralMint(...units...)`.
- Updates `lastQualifyingDeposit` on every ≥0.1 ICP deposit so `isActive()` works in real time.

- [ ] **Step 3: Build to typecheck**

```bash
dfx build shenanigans
```
Expected: build succeeds.

- [ ] **Step 4: Commit**

```bash
git add shenanigans/main.mo
git commit -m "$(cat <<'EOF'
feat(shenanigans): processNewGames now uses deductive cascade

Game mint path splits baseUnits into playerNet (mint to player) and
cascadeUnits (distribute up the chain). Updates lastQualifyingDeposit
on every ≥0.1 ICP deposit so isActive() works without an inter-canister
call from the cascade walker.

Backer-delta and signup-gift call sites still use cascadeReferralMint;
follow-up tasks switch them over.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 9: Switch `processBackerDeltas` to deductive split

**Files:**
- Modify: `shenanigans/main.mo` — `processBackerDeltas` function (around lines 515-539)

Same shape as Task 8, applied to the backer-delta hot path.

- [ ] **Step 1: Locate the function**

```motoko
    func processBackerDeltas() : async () {
        let ponziMath = getPonziMath();
        let backers = try { await ponziMath.getBackerPositions() } catch (_) { [] };
        for (backer in backers.vals()) {
            let seen : Float = switch (principalMap.get(backerSeen, backer.owner)) {
                case (null) { 0.0 };
                case (?v) { v };
            };
            if (backer.amount > seen) {
                let delta : Float = backer.amount - seen;
                let units = icpFloatToPpUnits(delta, mintConfig.backerPpPerIcp);
                let eventId = "backer-" # Principal.toText(backer.owner) # "-"
                    # Float.toText(backer.amount);
                let res = await mintInternal(backer.owner, units, eventId);
                switch (res) {
                    case (#Ok(_)) {
                        await cascadeReferralMint(backer.owner, units, eventId);
                        backerSeen := principalMap.put(backerSeen, backer.owner, backer.amount);
                    };
                    case (#Err(msg)) {
                        Debug.print("Backer mint failed: " # msg);
                    };
                };
            };
        };
    };
```

- [ ] **Step 2: Replace with the deductive version**

```motoko
    func processBackerDeltas() : async () {
        let ponziMath = getPonziMath();
        let backers = try { await ponziMath.getBackerPositions() } catch (_) { [] };
        for (backer in backers.vals()) {
            let seen : Float = switch (principalMap.get(backerSeen, backer.owner)) {
                case (null) { 0.0 };
                case (?v) { v };
            };
            if (backer.amount > seen) {
                let delta : Float = backer.amount - seen;
                let baseUnits = icpFloatToPpUnits(delta, mintConfig.backerPpPerIcp);
                let cascadeUnits = baseUnits * mintConfig.cascadeInitialBps / 10_000;
                let playerNet : Nat = if (baseUnits > cascadeUnits) { baseUnits - cascadeUnits } else { 0 };
                let eventId = "backer-" # Principal.toText(backer.owner) # "-" # Float.toText(backer.amount);

                let res = await mintInternal(backer.owner, playerNet, eventId);
                switch (res) {
                    case (#Ok(_)) {
                        await distributeDeductiveCascade(backer.owner, cascadeUnits, eventId);
                        if (delta >= 0.1) {
                            lastQualifyingDeposit := principalMap.put(lastQualifyingDeposit, backer.owner, Time.now());
                        };
                        backerSeen := principalMap.put(backerSeen, backer.owner, backer.amount);
                    };
                    case (#Err(msg)) {
                        Debug.print("Backer mint failed: " # msg);
                    };
                };
            };
        };
    };
```

- [ ] **Step 2: Build to typecheck**

```bash
dfx build shenanigans
```
Expected: build succeeds.

- [ ] **Step 3: Commit**

```bash
git add shenanigans/main.mo
git commit -m "$(cat <<'EOF'
feat(shenanigans): processBackerDeltas now uses deductive cascade

Mirrors processNewGames: split delta mint into playerNet + cascadeUnits,
mint net to backer, distribute cascade. Updates lastQualifyingDeposit
on every ≥0.1 ICP delta.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 10: Mint the signup gift in `processNewGames`

**Files:**
- Modify: `shenanigans/main.mo` — `processNewGames` function (from Task 8)

Insert the signup-gift logic immediately before the plan-rate mint. Idempotent via `signupGiftClaimed`. The gift itself goes through the deductive split (gift = mint event).

- [ ] **Step 1: Locate the post-Task-8 `processNewGames`**

This is the function you produced in Task 8. We're going to add a block just before the line `let res = await mintInternal(game.player, playerNet, eventId);`.

- [ ] **Step 2: Insert signup-gift logic**

In the `if (game.id >= gameIdCursor)` body, insert this block immediately above `let res = await mintInternal(game.player, playerNet, eventId);`:

```motoko
                // Signup gift — gated on first qualifying game record.
                // Gift itself goes through the deductive cascade (mint event).
                if (mintConfig.signupGiftPp > 0) {
                    switch (principalMap.get(signupGiftClaimed, game.player)) {
                        case (?_) {}; // already claimed
                        case (null) {
                            let giftBase = ppToUnits(mintConfig.signupGiftPp);
                            let giftCascade = giftBase * mintConfig.cascadeInitialBps / 10_000;
                            let giftNet : Nat = if (giftBase > giftCascade) { giftBase - giftCascade } else { 0 };
                            let giftEventId = "signup-" # Principal.toText(game.player);
                            switch (await mintInternal(game.player, giftNet, giftEventId)) {
                                case (#Ok(_)) {
                                    signupGiftClaimed := principalMap.put(signupGiftClaimed, game.player, Time.now());
                                    await distributeDeductiveCascade(game.player, giftCascade, giftEventId);
                                };
                                case (#Err(msg)) {
                                    Debug.print("Signup-gift mint failed for " # giftEventId # ": " # msg);
                                };
                            };
                        };
                    };
                };

```

(The trailing blank line is intentional — it visually separates from the plan mint that follows.)

- [ ] **Step 3: Build to typecheck**

```bash
dfx build shenanigans
```
Expected: build succeeds.

- [ ] **Step 4: Commit**

```bash
git add shenanigans/main.mo
git commit -m "$(cat <<'EOF'
feat(shenanigans): mint 500 PP signup gift on first qualifying game

Gated on signupGiftClaimed (idempotent across observer restarts).
Gift itself runs through the deductive cascade — new player keeps
90%, 10% cascades up. Recorded claim time doubles as join-time for
the upcoming toast/badge feature.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 11: Remove the now-unused `cascadeReferralMint`

**Files:**
- Modify: `shenanigans/main.mo` — delete the old function (around lines 853-888)

After Tasks 8-9, no call sites reference `cascadeReferralMint`. Remove it. Keep `bumpReferralEarnings` (still used by the new cascade).

- [ ] **Step 1: Verify no remaining call sites**

```bash
grep -n "cascadeReferralMint" shenanigans/main.mo
```
Expected: only the function *definition* line(s) — no call sites. If you see a call site, do NOT proceed — go back and ensure Tasks 8-9 actually switched it.

- [ ] **Step 2: Delete the function**

Delete the entire `func cascadeReferralMint(...) : async () { ... };` block including its doc comment (the lines starting with `/// For each of L1/L2/L3, mint referral PP-units...`).

- [ ] **Step 3: Build to typecheck**

```bash
dfx build shenanigans
```
Expected: build succeeds.

- [ ] **Step 4: Commit**

```bash
git add shenanigans/main.mo
git commit -m "$(cat <<'EOF'
refactor(shenanigans): remove cascadeReferralMint (replaced by deductive walker)

distributeDeductiveCascade has replaced all call sites
(processNewGames, processBackerDeltas, signup-gift). The additive
cascade is gone.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 12: Add the 5 admin setters

**Files:**
- Modify: `shenanigans/main.mo` — admin-section (look for other `setX` admin functions, around lines 1100-1160)

- [ ] **Step 1: Locate the admin-setter band**

Look for existing patterns like `setSimple21DayPpPerIcp`, `setBackerPpPerIcp`, etc. Add the new setters at the bottom of that band, just before any non-setter admin functions.

- [ ] **Step 2: Add the 5 setters**

```motoko
    public shared ({ caller }) func setHousePrincipal(p : Principal) : async () {
        requireAdmin(caller);
        housePrincipal := ?p;
    };

    public shared ({ caller }) func setCascadeBps(initial : Nat, passthrough : Nat) : async () {
        requireAdmin(caller);
        if (initial > 10_000 or passthrough > 10_000) {
            Debug.trap("BPS values must be ≤ 10_000");
        };
        mintConfig := {
            mintConfig with
            cascadeInitialBps = initial;
            cascadePassthroughBps = passthrough;
        };
    };

    public shared ({ caller }) func setSignupGiftPp(v : Nat) : async () {
        requireAdmin(caller);
        mintConfig := { mintConfig with signupGiftPp = v };
    };

    public shared ({ caller }) func setActivityRequiresDeposit(b : Bool) : async () {
        requireAdmin(caller);
        mintConfig := { mintConfig with activityRequiresDeposit = b };
    };

    public shared ({ caller }) func setActivityWindowDays(d : ?Nat) : async () {
        requireAdmin(caller);
        switch (d) {
            case (null) {};
            case (?n) {
                if (n == 0 or n > 3650) {
                    Debug.trap("activityWindowDays must be in [1, 3650] or null");
                };
            };
        };
        mintConfig := { mintConfig with activityWindowDays = d };
    };
```

- [ ] **Step 3: Build to typecheck**

```bash
dfx build shenanigans
```
Expected: build succeeds.

- [ ] **Step 4: Commit**

```bash
git add shenanigans/main.mo
git commit -m "$(cat <<'EOF'
feat(shenanigans): admin setters for cascade + signup + activity tunables

setHousePrincipal, setCascadeBps (validates BPS ≤ 10_000),
setSignupGiftPp, setActivityRequiresDeposit, setActivityWindowDays
(validates [1, 3650] when non-null).

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 13: Extend `ReferralStats` with `recentSignups`

**Files:**
- Modify: `shenanigans/main.mo` — the `ReferralStats` type declaration (around line 127) and the `getReferralStats` query (around line 315)

- [ ] **Step 1: Add the `SignupEntry` type**

Find the `public type ReferralStats = { ... }` declaration. Just *before* it, add:

```motoko
    public type SignupEntry = {
        principal : Principal;
        joinedAt : Int;  // ns since epoch
        level : Nat;     // 1, 2, or 3 — chain level relative to caller
    };
```

- [ ] **Step 2: Extend `ReferralStats`**

Replace the existing `public type ReferralStats = { ... }` declaration with:

```motoko
    public type ReferralStats = {
        l1Count : Nat;
        l2Count : Nat;
        l3Count : Nat;
        l1Units : Nat;
        l2Units : Nat;
        l3Units : Nat;
        recentSignups : [SignupEntry];  // L1/L2/L3 only; sorted joinedAt desc; capped 20
    };
```

- [ ] **Step 3: Extend the `getReferralStats` query body**

Find the existing query method. The current body counts L1/L2/L3 by iterating `referralChain`. We add a second pass that builds the recent-signups list. Replace the entire query with:

```motoko
    public query func getReferralStats(user : Principal) : async ReferralStats {
        var l1 : Nat = 0;
        var l2 : Nat = 0;
        var l3 : Nat = 0;
        var signupEntries : [var SignupEntry] = [var];
        // Lazy resize via a buffer-style array — total entries bounded by L1+L2+L3 count.

        let buf = List.nil<SignupEntry>();
        var bufRef : List.List<SignupEntry> = buf;

        for ((downliner, l1Ref) in principalMap.entries(referralChain)) {
            if (Principal.equal(l1Ref, user)) {
                l1 += 1;
                switch (principalMap.get(signupGiftClaimed, downliner)) {
                    case (?ts) { bufRef := List.push({ principal = downliner; joinedAt = ts; level = 1 }, bufRef) };
                    case (null) {};
                };
            } else {
                switch (principalMap.get(referralChain, l1Ref)) {
                    case (?l2Ref) {
                        if (Principal.equal(l2Ref, user)) {
                            l2 += 1;
                            switch (principalMap.get(signupGiftClaimed, downliner)) {
                                case (?ts) { bufRef := List.push({ principal = downliner; joinedAt = ts; level = 2 }, bufRef) };
                                case (null) {};
                            };
                        } else {
                            switch (principalMap.get(referralChain, l2Ref)) {
                                case (?l3Ref) {
                                    if (Principal.equal(l3Ref, user)) {
                                        l3 += 1;
                                        switch (principalMap.get(signupGiftClaimed, downliner)) {
                                            case (?ts) { bufRef := List.push({ principal = downliner; joinedAt = ts; level = 3 }, bufRef) };
                                            case (null) {};
                                        };
                                    };
                                };
                                case null {};
                            };
                        };
                    };
                    case null {};
                };
            };
        };

        let allSignups = List.toArray(bufRef);
        let sorted = Array.sort<SignupEntry>(allSignups, func(a, b) = Int.compare(b.joinedAt, a.joinedAt));
        let capped = if (sorted.size() <= 20) { sorted } else { Array.subArray(sorted, 0, 20) };

        let earnings = switch (principalMap.get(referralEarnings, user)) {
            case (?e) { e };
            case null { { l1Units = 0; l2Units = 0; l3Units = 0 } };
        };
        {
            l1Count = l1;
            l2Count = l2;
            l3Count = l3;
            l1Units = earnings.l1Units;
            l2Units = earnings.l2Units;
            l3Units = earnings.l3Units;
            recentSignups = capped;
        };
    };
```

- [ ] **Step 4: Build to typecheck**

```bash
dfx build shenanigans
```
Expected: build succeeds. If `Array.subArray` errors, the equivalent is to allocate a fresh `Array.tabulate` over the first 20 indices.

- [ ] **Step 5: Commit**

```bash
git add shenanigans/main.mo
git commit -m "$(cat <<'EOF'
feat(shenanigans): getReferralStats returns recentSignups for L1/L2/L3

New SignupEntry type carries principal + joinedAt (ns) + level.
recentSignups is sorted joinedAt desc, capped at 20. joinedAt sourced
from signupGiftClaimed — accurate for both new (post-V3) players and
grandfathered ones (seeded by seedMigrationV2 in next task).

Frontend will diff against localStorage.lastSeen to show toast + badge;
backend stays stateless about read-status.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 14: Add `seedMigrationV2` admin one-shot

**Files:**
- Modify: `shenanigans/main.mo` — admin-section

This is the data-seeding step: grandfather `signupGiftClaimed` so existing players don't claim retroactive gifts, seed `lastQualifyingDeposit` so `isActive()` returns true for current depositors immediately, backfill `referrerToDownline`, and initialize `housePrincipal` to the calling admin if null. Idempotent — safe to re-run.

- [ ] **Step 1: Add the seeding function**

Place near other admin one-shots (or at the bottom of the admin band):

```motoko
    /// One-shot post-upgrade seeding for the deductive-cascade rollout.
    ///
    /// 1. housePrincipal := ?caller if null
    /// 2. For every player with an existing game record: signupGiftClaimed
    ///    [player] := earliest game timestamp (prevents retroactive gifts).
    /// 3. For every player with ≥0.1 ICP cumulative deposit (game or backer):
    ///    lastQualifyingDeposit[player] := Time.now() (conservative: all
    ///    existing depositors are treated as just-qualified).
    /// 4. Backfill referrerToDownline from referralChain.
    ///
    /// Idempotent: re-running produces the same end state. Admin-only.
    public shared ({ caller }) func seedMigrationV2() : async () {
        requireAdmin(caller);

        // 1. Initialize housePrincipal if needed.
        switch (housePrincipal) {
            case (?_) {};
            case (null) { housePrincipal := ?caller };
        };

        let ponziMath = getPonziMath();
        let now = Time.now();

        // 2 & 3. Grandfather signupGiftClaimed + seed lastQualifyingDeposit from games.
        let games = try { await ponziMath.getAllGames() } catch (_) { [] };
        for (game in games.vals()) {
            // signupGiftClaimed: earliest-wins, so only set if not present.
            switch (principalMap.get(signupGiftClaimed, game.player)) {
                case (?_) {};
                case (null) {
                    signupGiftClaimed := principalMap.put(signupGiftClaimed, game.player, now);
                };
            };
            // lastQualifyingDeposit: any ≥0.1 ICP game qualifies; set to now (conservative).
            if (game.amount >= 0.1) {
                lastQualifyingDeposit := principalMap.put(lastQualifyingDeposit, game.player, now);
            };
        };

        // 3 (cont). Same for backer positions.
        let backers = try { await ponziMath.getBackerPositions() } catch (_) { [] };
        for (backer in backers.vals()) {
            if (backer.amount >= 0.1) {
                lastQualifyingDeposit := principalMap.put(lastQualifyingDeposit, backer.owner, now);
                // Also grandfather signupGiftClaimed for backers without game records.
                switch (principalMap.get(signupGiftClaimed, backer.owner)) {
                    case (?_) {};
                    case (null) {
                        signupGiftClaimed := principalMap.put(signupGiftClaimed, backer.owner, now);
                    };
                };
            };
        };

        // 4. Backfill referrerToDownline from referralChain.
        // Reset to empty first to ensure idempotency.
        referrerToDownline := principalMap.empty<List.List<Principal>>();
        for ((downliner, referrer) in principalMap.entries(referralChain)) {
            let existing = switch (principalMap.get(referrerToDownline, referrer)) {
                case (?list) { list };
                case (null) { List.nil<Principal>() };
            };
            referrerToDownline := principalMap.put(referrerToDownline, referrer, List.push(downliner, existing));
        };
    };
```

- [ ] **Step 2: Build to typecheck**

```bash
dfx build shenanigans
```
Expected: build succeeds.

- [ ] **Step 3: Commit**

```bash
git add shenanigans/main.mo
git commit -m "$(cat <<'EOF'
feat(shenanigans): seedMigrationV2 one-shot for deductive-cascade rollout

Admin-only. Idempotent. Initializes housePrincipal to caller (if null),
grandfathers signupGiftClaimed so existing players don't get retroactive
gifts on next deposit, seeds lastQualifyingDeposit from games + backer
positions, and backfills referrerToDownline from referralChain.

Must be called once post-upgrade before the next observer tick to keep
isActive() accurate for existing depositors.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 15: Local deploy + smoke test

**Files:**
- No code changes.

End-to-end verification on a local replica. Confirms the canister deploys, the migration runs cleanly, seeding works, and a simulated mint produces a deductive cascade.

- [ ] **Step 1: Start a clean local replica**

```bash
dfx start --background --clean
```
Wait for "Replica is running on..." message.

- [ ] **Step 2: Deploy locally**

```bash
dfx deploy --network local
```
Expected: every canister deploys. Note any errors and abort if `shenanigans` fails — go back and fix.

- [ ] **Step 3: Verify the new `MintConfig` fields are present**

```bash
dfx canister --network local call shenanigans getMintConfig
```
Expected: the candid record includes `cascadeInitialBps = 1_000`, `cascadePassthroughBps = 5_000`, `signupGiftPp = 500`, `activityRequiresDeposit = true`, `activityWindowDays = null`. If any are missing, the migration didn't apply — investigate before proceeding.

- [ ] **Step 4: Run the seeding one-shot**

```bash
dfx canister --network local call shenanigans seedMigrationV2
```
Expected: returns `()`. No errors. (On a fresh local replica with no game records, this is a no-op for the seeding loops but still initializes `housePrincipal`.)

- [ ] **Step 5: Verify `housePrincipal` was set**

There is no direct getter for `housePrincipal`. Verify indirectly via a setter round-trip: call `setHousePrincipal(<your test principal>)` as admin, then call it again to revert. Expected: both calls succeed with `()`.

```bash
ADMIN_PRINCIPAL=$(dfx identity get-principal)
dfx canister --network local call shenanigans setHousePrincipal "(principal \"$ADMIN_PRINCIPAL\")"
```
Expected: `()`.

- [ ] **Step 6: Verify `getReferralStats` shape includes `recentSignups`**

```bash
dfx canister --network local call shenanigans getReferralStats "(principal \"$ADMIN_PRINCIPAL\")"
```
Expected: returns a record with `recentSignups = vec {}` (empty on fresh replica) alongside the existing L1/L2/L3 fields.

- [ ] **Step 7: Verify the cascade tunables are admin-mutable**

```bash
dfx canister --network local call shenanigans setCascadeBps "(2_000 : nat, 4_000 : nat)"
dfx canister --network local call shenanigans getMintConfig
```
Expected: `cascadeInitialBps = 2_000`, `cascadePassthroughBps = 4_000`. Then revert:
```bash
dfx canister --network local call shenanigans setCascadeBps "(1_000 : nat, 5_000 : nat)"
```

- [ ] **Step 8: Verify BPS-validation traps work**

```bash
dfx canister --network local call shenanigans setCascadeBps "(10_001 : nat, 5_000 : nat)"
```
Expected: trap with "BPS values must be ≤ 10_000". (The error message format may vary, but the call MUST fail.)

- [ ] **Step 9: Verify `setActivityWindowDays` accepts null and a positive nat, rejects 0/over-cap**

```bash
dfx canister --network local call shenanigans setActivityWindowDays "(null)"
dfx canister --network local call shenanigans setActivityWindowDays "(opt 30 : opt nat)"
dfx canister --network local call shenanigans setActivityWindowDays "(opt 0 : opt nat)"
```
Expected: first two succeed; third traps. Revert:
```bash
dfx canister --network local call shenanigans setActivityWindowDays "(null)"
```

- [ ] **Step 10: Stop local replica + commit verification log**

```bash
dfx stop
```

Then create a brief verification note. Write to `docs/superpowers/plans/2026-05-16-mlm-deductive-cascade-verification.md` with output snippets from steps 3, 4, 6, 7. (One paragraph each — capture the candid responses so the next reviewer doesn't have to re-run locally.)

Then:
```bash
git add docs/superpowers/plans/2026-05-16-mlm-deductive-cascade-verification.md
git commit -m "$(cat <<'EOF'
test(shenanigans): local-replica smoke test for deductive cascade rollout

dfx deploy succeeds, MintConfig contains all 5 new fields with intended
defaults, seedMigrationV2 idempotent, admin setters and BPS validation
all behave per spec. Logged in verification doc.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 11: DO NOT deploy to mainnet from this plan**

Mainnet deploy requires explicit user permission per the project's saved memory (`feedback_deploy_safety`). When you finish this plan, hand back to the user with: "Local smoke test green. Ready for mainnet deploy when you give the word — `dfx deploy --network ic shenanigans` followed by `dfx canister --network ic call shenanigans seedMigrationV2`. The seeding call MUST happen before the next observer tick (default 10s) so existing players are grandfathered."

---

## Out of scope for this plan

Deferred per the design spec:

- Frontend toast + tab badge wiring (consumes `recentSignups` from `getReferralStats`).
- Updates to `frontend/src/components/DocsPage.tsx` to describe the deductive cascade and the signup gift (will need a new doc-rewrite session once this lands and the UI is wired).
- Shenanigan effect implementations, rubber-banding modifier (separate phase).
- v2 NFT contracts + Harberger tax (explicitly v2; `getPayoutTarget` is the swap-point).

When this plan is fully landed, the natural next steps are: (a) a frontend session for the toast/badge UI, (b) a docs-update session to align `DocsPage.tsx` with the new mechanic.
