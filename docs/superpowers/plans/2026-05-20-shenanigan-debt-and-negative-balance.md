# Shenanigan Debt / Negative-Balance Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a caster's effective PP go negative — framed as **debt**. When a backfire (or any cast outcome) charges more than the caster has, burn what they have and write the shortfall to a per-player debt ledger maintained by the shenanigans canister. Any future PP they receive (mints, referral cascade, spell winnings, chip transfers in) services that debt first; only the remainder lands in their actual chip subaccount.

**Architecture:**
- The ICRC-1 PP ledger holds balances as `Nat` — a chip subaccount cannot actually go negative. The "negative balance" is a fiction maintained inside the shenanigans canister: a `ppDebt : Map<Principal, Nat>` stable variable plus two wrapper helpers (`creditPlayer`, `transferToPlayer`) that every player-bound credit path routes through. Wrappers do `pay = min(amount, debt); burn-from-minting-account pay; mint/transfer the remainder; decrement debt by pay`.
- Castle the burn clamp from phase 1: when a cast outcome charges more PP than the caster has, the shortfall now goes into the debt map instead of vanishing.
- Display layer reports `effectiveBalance = chipBalance - debt` as `Int`. Frontend renders red negative when debt > 0 and surfaces a "next earnings pay this off" hint.
- Admin gets `getDebt` + `adminClearDebt` for visibility/compensation. `adminMint` gains an optional `bypassDebt` flag so the admin can hand-grant PP without it disappearing into a debt hole.

**Tech Stack:** Motoko (canister), TypeScript + React (frontend), dfx for declarations.

**Pre-work:** This plan assumes the per-outcome cost phase ([PR #77](https://github.com/SchemeWorks/MusicalChairs/pull/77)) has shipped to mainnet (it has). The V4 migration attachment on `persistent actor Self` will be cleaned up in Task 0 of this plan since adding the `ppDebt` stable field is an implicit (compatible) change that needs no explicit migration.

---

## File Structure

**Modify:**
- `shenanigans/main.mo` — remove V4 migration attachment (cleanup), add `ppDebt` stable state + helpers, rewire all credit paths, expand `ShenaniganOutcomeDetail`, add admin endpoints
- `shenanigans/migration.mo` — leave V4 in place as legacy reference (matches V2/V3 pattern)
- `frontend/src/declarations/shenanigans/*` — regenerated
- `frontend/src/components/Shenanigans.tsx` — outcome toast surfaces debt service, gate uses effective balance
- `frontend/src/components/Wallet*.tsx` / wherever PP balance is displayed — show debt
- `frontend/src/components/ShenanigansAdminPanel.tsx` — optional debt management section (admin view of indebted players, clear-debt button)

**Test plan:** No automated test harness exists. Verification is manual against a local replica plus mainnet after deploy.

---

## Task 0: Clean up V4 migration attachment

**Context:** The V4 migration ran on mainnet during PR #77's deploy. Leaving it attached means the next upgrade tries to apply V4 again — V4's input type expects the OLD single-`cost` shape, but the canister now stores the NEW three-cost shape, so the migration would trap. We remove the attachment so subsequent upgrades use implicit field-add migration (which works for the new `ppDebt` field).

**Files:**
- Modify: `shenanigans/main.mo:32-37` (the `(with migration = Migration.runV4)` block)

- [ ] **Step 1: Remove the migration attachment + the now-stale comment**

```motoko
import PpLedger "PpLedger";
import Reginald "Reginald";
import Subaccount "Subaccount";
import Icrc21 "icrc21";

// TODO(2026-05-11): Rename "chips" terminology in this canister — depositChips,
// claimCashOut, chip subaccount, CashOutEntry, etc. — to non-casino verbiage
// (e.g. credits, PP units, tokens). Deferred from the ponzi_math extraction
// migration to keep that scope tight. See
// docs/superpowers/specs/2026-05-11-ponzi-math-extraction-design.md.

persistent actor Self {
```

(Drop the `import Migration "migration";` line too since nothing else in main.mo references it. `migration.mo` itself stays — it carries V2/V3/V4 as legacy reference, matching the existing pattern.)

- [ ] **Step 2: Build clean**

Run: `dfx build shenanigans --check`
Expected: clean compile, no warnings about migration.

- [ ] **Step 3: Commit**

```bash
git add shenanigans/main.mo
git commit -m "chore(shenanigans): drop V4 migration attachment now that it has run"
```

---

## Task 1: Add `ppDebt` stable state + read accessors

**Files:**
- Modify: `shenanigans/main.mo` near the other map declarations (~line 302)

- [ ] **Step 1: Declare the stable map**

Add immediately below the `shenaniganConfigs` declaration:

```motoko
    // Spell configs — PRESERVED across migration (admin-tunable spell definitions)
    var shenaniganConfigs = natMap.empty<ShenaniganConfig>();

    // Outstanding PP debt per player. Populated when a cast outcome charges
    // more chips than the caster has — the shortfall lands here. Every
    // credit-to-player path (mint, chip-transfer-in) services debt before
    // crediting the player's actual chip subaccount. Zero entries are
    // pruned on debt-clear; debt-free players don't appear in the map.
    var ppDebt = principalMap.empty<Nat>();
```

This is a new persistent field with an initializer — no explicit migration needed (the runtime allows adding fields). On upgrade, every existing player gets an implicit `0` (absent from the map).

- [ ] **Step 2: Add private helpers for reading/mutating debt**

Place these near `getChipBalance` (~line 1376):

```motoko
    /// Read outstanding debt for a player. Absent ⇒ 0.
    func getDebt(player : Principal) : Nat {
        switch (principalMap.get(ppDebt, player)) {
            case (null) { 0 };
            case (?n) { n };
        };
    };

    /// Set or remove a player's debt. Zero removes the entry to keep the
    /// map clean (so `principalMap.size(ppDebt)` is "indebted-player count").
    func setDebt(player : Principal, amount : Nat) {
        if (amount == 0) {
            ppDebt := principalMap.delete(ppDebt, player);
        } else {
            ppDebt := principalMap.put(ppDebt, player, amount);
        };
    };

    /// Add `delta` to a player's debt.
    func incrementDebt(player : Principal, delta : Nat) {
        if (delta == 0) return;
        setDebt(player, getDebt(player) + delta);
    };

    /// Subtract up to `delta` from a player's debt. Returns the amount
    /// actually subtracted (clamped to current debt). Useful for crediting
    /// debt-pay-off back to the caller as "amount that went to debt".
    func decrementDebt(player : Principal, delta : Nat) : Nat {
        let current = getDebt(player);
        let take = if (delta < current) { delta } else { current };
        setDebt(player, current - take);
        take;
    };
```

- [ ] **Step 3: Add public query for the frontend**

Place near other player query endpoints:

```motoko
    /// Per-player debt query. Returns 0 for unknown / debt-free players.
    public query func getPpDebt(player : Principal) : async Nat {
        getDebt(player);
    };

    /// Effective balance = chipBalance - debt. Signed Int because debt can
    /// exceed chip balance (the whole point of this feature).
    public func getEffectiveBalance(player : Principal) : async Int {
        let bal = await getChipBalance(player);
        bal - getDebt(player);
    };
```

- [ ] **Step 4: Build clean**

Run: `dfx build shenanigans --check`
Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): add ppDebt stable state + read accessors"
```

---

## Task 2: Add `creditPlayer` helper (mint-style)

**Files:**
- Modify: `shenanigans/main.mo` near `mintInternal` (~line 1290)

- [ ] **Step 1: Add the wrapper**

Place immediately after `mintInternal`:

```motoko
    /// Mint-style credit to a player that services debt first. Returns
    /// `{ creditedToChips, paidToDebt }` in PP-units so callers can report
    /// the split in outcome detail.
    ///
    /// Atomicity caveat: the debt-burn is a separate ledger transfer from
    /// the mint. If the debt-burn fails after the mint succeeds, the player
    /// keeps the chips and the debt entry stays — bounded, cosmetic edge.
    func creditPlayer(
        player : Principal,
        amount : Nat,
        memoText : Text,
    ) : async { creditedToChips : Nat; paidToDebt : Nat; result : { #Ok : Nat; #Err : Text } } {
        if (amount == 0) {
            return { creditedToChips = 0; paidToDebt = 0; result = #Ok(0) };
        };
        let outstanding = getDebt(player);
        // If the player has no debt, just mint normally.
        if (outstanding == 0) {
            let r = await mintInternal(player, amount, memoText);
            return { creditedToChips = amount; paidToDebt = 0; result = r };
        };
        // Player has debt — service it first.
        let toDebt = if (amount < outstanding) { amount } else { outstanding };
        let toChips : Nat = amount - toDebt;
        // Decrement debt synchronously up front. We do NOT mint the toDebt
        // portion at all — the supply just doesn't move for that slice.
        let _ = decrementDebt(player, toDebt);
        if (toChips == 0) {
            return { creditedToChips = 0; paidToDebt = toDebt; result = #Ok(0) };
        };
        let r = await mintInternal(player, toChips, memoText # "-net");
        { creditedToChips = toChips; paidToDebt = toDebt; result = r };
    };
```

- [ ] **Step 2: Build clean**

Run: `dfx build shenanigans --check`
Expected: clean.

- [ ] **Step 3: Commit**

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): add creditPlayer helper that services debt before minting"
```

---

## Task 3: Add `transferToPlayer` helper (chip-to-chip credit)

**Files:**
- Modify: `shenanigans/main.mo` near `chipTransfer` (~line 1353)

- [ ] **Step 1: Add the wrapper**

Place immediately after `chipTransfer`:

```motoko
    /// Chip-to-chip transfer where the receiver is a player whose debt
    /// should be serviced. Does the full transfer first, then if the
    /// receiver has debt, burns up to `min(transferred, debt)` from the
    /// receiver back to the minting account and decrements debt.
    ///
    /// Returns `{ creditedToChips, paidToDebt, result }` in PP-units.
    /// Falls back to plain chipTransfer behavior when the receiver has no
    /// debt (single ledger call, no debt-burn).
    func transferToPlayer(
        from : Principal,
        to : Principal,
        units : Nat,
        memoText : Text,
    ) : async { creditedToChips : Nat; paidToDebt : Nat; result : { #Ok : Nat; #Err : Text } } {
        if (units == 0) {
            return { creditedToChips = 0; paidToDebt = 0; result = #Ok(0) };
        };
        let transferRes = await chipTransfer(from, to, units, memoText);
        switch (transferRes) {
            case (#Err(msg)) {
                return { creditedToChips = 0; paidToDebt = 0; result = #Err(msg) };
            };
            case (#Ok(_)) {};
        };
        let outstanding = getDebt(to);
        if (outstanding == 0) {
            return { creditedToChips = units; paidToDebt = 0; result = transferRes };
        };
        // Burn-back from receiver's chip subaccount to the minting account.
        // Clamp burn to receiver's CURRENT balance — between the transfer
        // landing and this burn, they may have spent some. A partial debt
        // service is fine; only the actual burned amount decrements debt.
        let toDebtTarget = if (units < outstanding) { units } else { outstanding };
        let receiverBal = await getChipBalance(to);
        let toDebt = if (toDebtTarget < receiverBal) { toDebtTarget } else { receiverBal };
        if (toDebt == 0) {
            return { creditedToChips = units; paidToDebt = 0; result = transferRes };
        };
        let burnRes = await burnFrom(to, toDebt, memoText # "-debt-svc");
        switch (burnRes) {
            case (#Ok(_)) {
                let _ = decrementDebt(to, toDebt);
                { creditedToChips = (units - toDebt : Nat); paidToDebt = toDebt; result = transferRes };
            };
            case (#Err(_)) {
                // Debt-burn failed (race or InsufficientFunds). Player keeps
                // the full transfer, debt stays. Bounded cosmetic edge.
                { creditedToChips = units; paidToDebt = 0; result = transferRes };
            };
        };
    };
```

- [ ] **Step 2: Build clean**

Run: `dfx build shenanigans --check`
Expected: clean.

- [ ] **Step 3: Commit**

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): add transferToPlayer wrapper that services receiver debt"
```

---

## Task 4: Rewire `mintInternal` callsites that target players

**Context:** There are three sites where `mintInternal` credits a player and should now service debt: organic yield ([main.mo:1720](shenanigans/main.mo:1720)), siphoner payouts ([main.mo:1723](shenanigans/main.mo:1723)), admin mint ([main.mo:3176](shenanigans/main.mo:3176)). Each gets replaced with a `creditPlayer` call.

Admin mint gets a `bypassDebt : Bool` flag so the admin can manually issue PP without it disappearing into a debt hole (useful for compensation).

**Files:**
- Modify: `shenanigans/main.mo` lines 1720, 1723, 3174-3177

- [ ] **Step 1: Update organic yield mint at line 1720**

Read the surrounding context first — the function is `processMint` (handles siphons + primary mint per game event). Replace the two `mintInternal` calls:

```motoko
        let primary = await creditPlayer(player, toPlayer, eventId);
        switch (siphonTuple) {
            case (?(siphoner, take)) {
                let _ = await creditPlayer(siphoner, take, "siphon-" # eventId);
            };
            case null {};
        };
        primary.result;
```

(The function returns `{ #Ok : Nat; #Err : Text }` to its caller — we surface `primary.result`. The `paidToDebt` and `creditedToChips` info is discarded here because the upstream mint flow doesn't carry it; if we want to surface debt-service in admin observability we add a Debug.print.)

- [ ] **Step 2: Update `adminMint` at line 3174**

Replace the body:

```motoko
    public shared ({ caller }) func adminMint(to : Principal, wholePp : Nat, bypassDebt : Bool) : async { #Ok : Nat; #Err : Text } {
        requireAdmin(caller);
        let units = ppToUnits(wholePp);
        if (bypassDebt) {
            // Direct mint — does NOT service debt. Use when compensating
            // a player who hit debt due to a bug we're fixing.
            await mintInternal(to, units, "admin-mint-bypass-" # Principal.toText(to));
        } else {
            let r = await creditPlayer(to, units, "admin-mint-" # Principal.toText(to));
            r.result;
        };
    };
```

**Frontend impact:** the candid signature of `adminMint` changes from `(Principal, Nat)` to `(Principal, Nat, Bool)`. The admin panel hook (`useAdminMintPp` or similar — search to confirm) needs the extra `bypassDebt` argument. Default in the UI to `false` (services debt).

- [ ] **Step 3: Build clean**

Run: `dfx build shenanigans --check`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): route mintInternal callsites through creditPlayer"
```

---

## Task 5: Rewire spell-effect `chipTransfer` callsites that credit players

**Context:** When a spell effect transfers chips to a player (caster wins from victim, or backfire-pays target), the recipient should service their debt. Sites:

- **moneyTrickster success** ([main.mo:1921](shenanigans/main.mo:1921)) — caster receives from target
- **aoeSkim success** ([main.mo:1942](shenanigans/main.mo:1942)) — caster receives from each victim
- **whaleRebalance success** ([main.mo:2119](shenanigans/main.mo:2119)) — caster receives from each whale
- **moneyTrickster backfire** ([main.mo:2171](shenanigans/main.mo:2171)) — target receives from caster
- **whaleRebalance backfire** ([main.mo:2301](shenanigans/main.mo:2301)) — whale receives from caster
- **payManagement recipient** ([main.mo:2795](shenanigans/main.mo:2795)) — recipient receives from caster

The house transfer at line 2802 stays plain `chipTransfer` — the house is not a player and has no debt.

**Files:**
- Modify: `shenanigans/main.mo` lines listed above

- [ ] **Step 1: Update moneyTrickster success at line 1921**

```motoko
                        switch (await transferToPlayer(t, caster, amount, memo)) {
                            case ({ result = #Ok(_); creditedToChips = chips; paidToDebt = debt }) {
                                return { ppDeltaCaster = chips; ppToDebt = debt; affectedTarget = ?t; affectedCount = 1 };
                            };
                            case ({ result = #Err(_); _ }) {
                                return { ppDeltaCaster = 0; ppToDebt = 0; affectedTarget = ?t; affectedCount = 0 };
                            };
                        };
```

(`ppToDebt` is the new field added to `ShenaniganOutcomeDetail` in Task 6 — keep this code as-is and Task 6 will add the type field.)

- [ ] **Step 2: Update aoeSkim success at line 1942**

```motoko
                        if (amount > 0) {
                            switch (await transferToPlayer(victim, caster, amount, memo)) {
                                case ({ result = #Ok(_); creditedToChips = chips; paidToDebt = debt }) {
                                    total += chips;
                                    totalToDebt += debt;
                                    victims += 1;
                                };
                                case ({ result = #Err(_); _ }) {};
                            };
                        };
```

Add `var totalToDebt : Nat = 0` next to the existing `var total : Nat = 0; var victims : Nat = 0` declarations earlier in the block. Update the return:

```motoko
                return { ppDeltaCaster = total; ppToDebt = totalToDebt; affectedTarget = null; affectedCount = victims };
```

- [ ] **Step 3: Update whaleRebalance success at line 2119**

Same pattern as moneyTrickster success — switch the `chipTransfer` to `transferToPlayer` and pass `chips`/`debt` through to the return tuple, accumulating into `total` and `totalToDebt` if the spell aggregates multiple recipients.

```motoko
                            switch (await transferToPlayer(whale, caster, amount, memo)) {
                                case ({ result = #Ok(_); creditedToChips = chips; paidToDebt = debt }) {
                                    total += chips;
                                    totalToDebt += debt;
                                    whales += 1;
                                };
                                case ({ result = #Err(_); _ }) {};
                            };
```

(Add `var totalToDebt : Nat = 0` alongside any existing `total`/`whales` declarations and surface it in the return.)

- [ ] **Step 4: Update moneyTrickster backfire at line 2171**

Backfire damage flows caster → target. The target services their debt on the received amount.

```motoko
                        switch (await transferToPlayer(caster, t, amount, memo)) {
                            case ({ result = #Ok(_); _ }) {
                                // ppDeltaCaster is negative (caster lost amount).
                                // Debt service happens on the TARGET, not the
                                // caster — we don't surface it here.
                                return { ppDeltaCaster = -amount; ppToDebt = 0; affectedTarget = ?t; affectedCount = 1 };
                            };
                            case ({ result = #Err(_); _ }) {
                                return { ppDeltaCaster = 0; ppToDebt = 0; affectedTarget = ?t; affectedCount = 0 };
                            };
                        };
```

- [ ] **Step 5: Update whaleRebalance backfire at line 2301**

Same shape as moneyTrickster backfire — caster pays whales. Whales service debt.

```motoko
                        switch (await transferToPlayer(caster, whale, amount, memo)) {
                            case ({ result = #Ok(_); _ }) {
                                lost += amount;
                            };
                            case ({ result = #Err(_); _ }) {};
                        };
```

- [ ] **Step 6: Update payManagement recipient at line 2795**

Payments to managers/recipients should also service debt. The house transfer at line 2802 stays plain.

```motoko
        switch (await transferToPlayer(caller, recipient, recipientUnits, payMemo)) {
            case ({ result = #Ok(_); _ }) {};
            case ({ result = #Err(msg); _ }) { return #Err(msg) };
        };
```

(Leave the next `chipTransfer(caller, house(), mgmtUnits, mgmtMemo)` call unchanged — the house has no debt.)

- [ ] **Step 7: Build clean**

Run: `dfx build shenanigans --check`
Expected: clean (will fail until Task 6 adds `ppToDebt` to `ShenaniganOutcomeDetail` — do Task 6 next).

- [ ] **Step 8: Defer commit to after Task 6 (single commit covers both)**

---

## Task 6: Expand `ShenaniganOutcomeDetail` with `ppToDebt`

**Files:**
- Modify: `shenanigans/main.mo` (~line 64)

- [ ] **Step 1: Add `ppToDebt` field to the type**

```motoko
    public type ShenaniganOutcomeDetail = {
        outcome : ShenaniganOutcome;
        ppDeltaCaster : Int;
        ppToDebt : Nat;
        affectedTarget : ?Principal;
        affectedCount : Nat;
    };
```

- [ ] **Step 2: Update every `applySuccessEffect` / `applyBackfireEffect` return site to include `ppToDebt`**

For sites that don't credit a player (no chip-transfer-in), set `ppToDebt = 0`. Specifically, the buff/cosmetic spells return like this:

```motoko
            case (#magicMirror) {
                return { ppDeltaCaster = 0; ppToDebt = 0; affectedTarget = null; affectedCount = 0 };
            };
            case (#ppBoosterAura) {
                return { ppDeltaCaster = 0; ppToDebt = 0; affectedTarget = null; affectedCount = 0 };
            };
            case (#downlineBoost) {
                return { ppDeltaCaster = 0; ppToDebt = 0; affectedTarget = null; affectedCount = 0 };
            };
            case (#goldenName) {
                return { ppDeltaCaster = 0; ppToDebt = 0; affectedTarget = null; affectedCount = 0 };
            };
            case (#renameSpell) {
                // ...existing logic...
                return { ppDeltaCaster = 0; ppToDebt = 0; affectedTarget = ?t; affectedCount = 1 };
            };
            // ...repeat for every other case that returns a record literal
```

Use `grep -n "ppDeltaCaster\s*=" shenanigans/main.mo` to find all return sites and add `ppToDebt = 0` (or the accumulated value, per Task 5) to each.

- [ ] **Step 3: Update castShenanigan's final return to include ppToDebt**

Around line 1845, the outer cast handler returns:

```motoko
        {
            outcome;
            ppDeltaCaster = detail.ppDeltaCaster;
            ppToDebt = detail.ppToDebt;
            affectedTarget = detail.affectedTarget;
            affectedCount = detail.affectedCount;
        };
```

Also update the early-return `{ ppDeltaCaster = 0; affectedTarget = null; affectedCount = 0 }` literal on the `#fail` branch (~line 1796) to include `ppToDebt = 0`.

- [ ] **Step 4: Build clean**

Run: `dfx build shenanigans --check`
Expected: clean.

- [ ] **Step 5: Commit Tasks 5 + 6 together**

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): route player-bound chip transfers through transferToPlayer

Adds ppToDebt to ShenaniganOutcomeDetail so the frontend can surface
how much of a credit went to debt service. Every chipTransfer call
that credits a player principal now goes through transferToPlayer
which burns up to min(transferred, debt) from the receiver back to
the minting account. House transfers stay plain (house is not a
player and has no debt)."
```

---

## Task 7: Add debt-write to the cast clamp

**Context:** Phase 1 clamped the burn at `casterBalPre` and lost the shortfall. Now the shortfall lands in `ppDebt`.

**Files:**
- Modify: `shenanigans/main.mo` `castShenanigan` (~line 1800)

- [ ] **Step 1: Replace the clamp block**

Locate the existing block:

```motoko
        let actualBurnedUnits : Nat = if (costForOutcomeUnits <= casterBalPre) {
            costForOutcomeUnits
        } else {
            casterBalPre
        };
```

Replace with:

```motoko
        let actualBurnedUnits : Nat = if (costForOutcomeUnits <= casterBalPre) {
            costForOutcomeUnits
        } else {
            casterBalPre
        };
        // Shortfall — the portion of the outcome cost the caster couldn't
        // pay from their chip balance. Written to ppDebt so future credits
        // service it. casterBalPre is in PP-units, as is the cost.
        let shortfallUnits : Nat = if (costForOutcomeUnits > casterBalPre) {
            costForOutcomeUnits - casterBalPre
        } else { 0 };
        if (shortfallUnits > 0) {
            incrementDebt(caller, shortfallUnits);
        };
```

- [ ] **Step 2: Reconsider the pre-cast gate**

Currently: `if (casterBalPre < costSuccessUnits) { Debug.trap("Insufficient chips...") }`. With debt enabled, a player with 0 chips can still cast spells where `costSuccess = 0` — and indeed that's the design (they keep digging). The gate stays as-is.

For completeness, consider this comment update on the existing gate:

```motoko
        // Pre-cast gate is the *minimum* the caster commits to paying — i.e.
        // costSuccess. They might roll a worse outcome and owe more than
        // they have; the shortfall lands in ppDebt and is paid off by future
        // credits. If costSuccess itself exceeds the chip balance, we trap
        // (the caster has to actually pay the upfront commitment in chips —
        // debt cannot fund another cast's commitment).
        if (casterBalPre < costSuccessUnits) {
            Debug.trap("Insufficient chips to cast this shenanigan");
        };
```

- [ ] **Step 3: Build clean**

Run: `dfx build shenanigans --check`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): unaffordable cast cost writes shortfall to ppDebt"
```

---

## Task 8: Admin endpoints — clear debt, list indebted players

**Files:**
- Modify: `shenanigans/main.mo` near other admin endpoints (~line 3329)

- [ ] **Step 1: Add `adminClearDebt`**

```motoko
    /// Admin-only: zero out a player's debt. Useful for compensating
    /// players who hit debt due to a bug. Returns the amount cleared
    /// (so the admin tooling can show "cleared 350 PP of debt").
    public shared ({ caller }) func adminClearDebt(player : Principal) : async Nat {
        requireAdmin(caller);
        let prior = getDebt(player);
        setDebt(player, 0);
        prior;
    };
```

- [ ] **Step 2: Add `listIndebtedPlayers`**

```motoko
    /// Admin-only: snapshot of every player with nonzero debt.
    /// (Bounded by indebted-player count; debt-free players don't appear.)
    public shared query ({ caller }) func listIndebtedPlayers() : async [(Principal, Nat)] {
        requireAdmin(caller);
        Iter.toArray(principalMap.entries(ppDebt));
    };
```

- [ ] **Step 3: Build clean**

Run: `dfx build shenanigans --check`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): admin endpoints for debt visibility + clearing"
```

---

## Task 9: Regenerate frontend declarations

**Files:**
- Modify (generated): `frontend/src/declarations/shenanigans/*`

- [ ] **Step 1: Regenerate**

```bash
dfx generate shenanigans
cp src/declarations/shenanigans/shenanigans.did frontend/src/declarations/shenanigans/shenanigans.did
cp src/declarations/shenanigans/shenanigans.did.js frontend/src/declarations/shenanigans/shenanigans.did.js
cp src/declarations/shenanigans/shenanigans.did.d.ts frontend/src/declarations/shenanigans/shenanigans.did.d.ts
```

- [ ] **Step 2: Confirm `ppToDebt`, `getPpDebt`, `getEffectiveBalance`, `adminClearDebt`, `listIndebtedPlayers` are present**

```bash
grep -n "ppToDebt\|getPpDebt\|getEffectiveBalance\|adminClearDebt\|listIndebtedPlayers" frontend/src/declarations/shenanigans/shenanigans.did.d.ts
```

Expected: matches for all five identifiers.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/declarations/shenanigans/
git commit -m "chore(shenanigans): regenerate declarations for debt feature"
```

---

## Task 10: Frontend — display debt on player UI

**Files:**
- Modify: `frontend/src/components/Shenanigans.tsx` (outcome toast + cast gate)
- Modify: wallet / balance display component (search: `grep -rn "totalPoints\|chipBalance" frontend/src/components` to locate)

- [ ] **Step 1: Add a `useGetPpDebt(principal)` hook**

In `frontend/src/hooks/useQueries.ts` (or wherever shenanigan hooks live), add:

```tsx
export function useGetPpDebt(principal: Principal | undefined) {
  const actor = useReadShenaniganActor();
  return useQuery({
    queryKey: ['ppDebt', principal?.toText()],
    queryFn: async () => {
      if (!actor || !principal) return 0n;
      return await actor.getPpDebt(principal);
    },
    enabled: !!actor && !!principal,
    refetchInterval: 15_000, // refresh after casts
  });
}
```

- [ ] **Step 2: Surface debt in the wallet / balance display**

In the component that shows total PP (search for `totalPoints` in `frontend/src/components/`), add a debt row when `debt > 0`:

```tsx
{debtPp > 0 && (
  <div className="flex justify-between items-center text-xs">
    <span className="mc-text-danger">Debt</span>
    <span className="mc-text-danger font-bold">−{Number(debtPp).toLocaleString()} PP</span>
  </div>
)}
{debtPp > 0 && (
  <p className="text-[10px] mc-text-muted italic mt-1">
    Your next PP earnings pay this down before they hit your wallet.
  </p>
)}
```

The "effective balance" view should show `chipBalance - debt`, rendered red if negative.

- [ ] **Step 3: Update the cast outcome toast to surface debt service**

In `frontend/src/components/Shenanigans.tsx`, the outcome-toast block (around the existing `outcomeToast.cost > 0` line):

```tsx
{outcomeToast.cost > 0 && (
  <p className="text-xs mc-text-muted mb-3">{outcomeToast.cost} PP spent</p>
)}
{outcomeToast.ppToDebt && outcomeToast.ppToDebt > 0 && (
  <p className="text-xs mc-text-danger mb-3">
    {outcomeToast.ppToDebt} PP went to debt
  </p>
)}
```

Add `ppToDebt?: number;` to the `outcomeToast` state interface and populate from `Number(detail.ppToDebt) / 100_000_000` in `handleConfirmCast`.

- [ ] **Step 4: TypeScript check**

```bash
cd frontend && npx tsc --noEmit
```

Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/Shenanigans.tsx frontend/src/hooks/useQueries.ts
# plus the wallet-display file you touched
git commit -m "feat(shenanigans-ui): surface debt + per-cast debt service in the player UI"
```

---

## Task 11: Frontend — admin panel debt section

**Files:**
- Modify: `frontend/src/components/ShenanigansAdminPanel.tsx`

- [ ] **Step 1: Add hooks for `listIndebtedPlayers` and `adminClearDebt`**

In `frontend/src/hooks/useQueries.ts`:

```tsx
export function useListIndebtedPlayers() {
  const actor = useReadShenaniganActor();
  return useQuery({
    queryKey: ['indebtedPlayers'],
    queryFn: async () => {
      if (!actor) return [];
      return await actor.listIndebtedPlayers();
    },
    enabled: !!actor,
    refetchInterval: 30_000,
  });
}

export function useAdminClearDebt() {
  const actor = useWriteShenaniganActor();
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (player: Principal) => {
      if (!actor) throw new Error('no actor');
      return await actor.adminClearDebt(player);
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['indebtedPlayers'] });
      qc.invalidateQueries({ queryKey: ['ppDebt'] });
    },
  });
}
```

- [ ] **Step 2: Add a "Indebted Players" section to ShenanigansAdminPanel.tsx**

Place between existing admin sections (mint rules / observer). Shape:

```tsx
function IndebtedPlayersSection() {
  const { data: indebted, isLoading } = useListIndebtedPlayers();
  const clearDebt = useAdminClearDebt();
  if (isLoading) return null;
  if (!indebted || indebted.length === 0) {
    return (
      <div className="mc-card p-4">
        <h3 className="font-display text-sm mc-text-primary mb-1">Indebted Players</h3>
        <p className="text-xs mc-text-muted">Nobody currently in debt. Quiet day.</p>
      </div>
    );
  }
  return (
    <div className="mc-card p-4">
      <h3 className="font-display text-sm mc-text-primary mb-3">Indebted Players ({indebted.length})</h3>
      <div className="space-y-2">
        {indebted.map(([principal, amount]) => (
          <div key={principal.toText()} className="flex items-center justify-between text-xs">
            <code className="mc-text-dim truncate max-w-[60%]">{principal.toText()}</code>
            <span className="mc-text-danger font-bold">−{Number(amount).toLocaleString()} PP</span>
            <button
              onClick={async () => {
                if (confirm(`Clear ${Number(amount).toLocaleString()} PP of debt for this player?`)) {
                  await clearDebt.mutateAsync(principal);
                  toast.success('Debt cleared');
                }
              }}
              className="mc-btn-secondary px-2 py-1 rounded text-[10px]"
            >
              Clear
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}
```

Render `<IndebtedPlayersSection />` near the top of the admin panel.

- [ ] **Step 3: Wire the new `bypassDebt` flag into the existing admin-mint UI**

If the admin panel has a "Manually mint PP" form, add a checkbox:

```tsx
<label className="flex items-center gap-2 text-xs mc-text-dim">
  <input type="checkbox" checked={bypassDebt} onChange={e => setBypassDebt(e.target.checked)} />
  Bypass debt (direct credit, doesn't service outstanding debt)
</label>
```

Pass `bypassDebt` as the third argument to `adminMint`. Default unchecked.

- [ ] **Step 4: TypeScript check**

```bash
cd frontend && npx tsc --noEmit
```

Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/ShenanigansAdminPanel.tsx frontend/src/hooks/useQueries.ts
git commit -m "feat(shenanigans-admin): indebted-players section + adminMint bypass-debt toggle"
```

---

## Task 12: Manual verification (local replica)

**No automated tests** — run through the scenarios on a local replica.

- [ ] **Step 1: Local deploy**

```bash
dfx start --background --clean
dfx deploy
```

- [ ] **Step 2: Set up an aggressive spell with a backfire-only cost**

Open the admin panel and set Bridge Exploit to:
- costSuccess: 0
- costFailure: 0
- costBackfire: 1500
- odds: 20 / 50 / 30

Save.

- [ ] **Step 3: Cast on an underfunded account**

From a player account with **200 PP** total:
- Cast Bridge Exploit until a backfire fires.
- Expected: chip balance → 0, debt → 1300 (= 1500 - 200).
- Confirm via `dfx canister call shenanigans getPpDebt '(principal "<player>")'`.

- [ ] **Step 4: Earn PP and verify debt service**

Deposit (or have admin mint) 500 PP to that player.
- Expected: chip balance still 0 (or close to it), debt drops to 800.
- Confirm: `getPpDebt` returns 800.
- Confirm: `getEffectiveBalance` returns `-800` (or a small positive if there's leftover from rounding).

- [ ] **Step 5: Pay off debt completely**

Mint 1500 PP more. Expected:
- 800 services debt → debt = 0, ppDebt entry removed
- 700 lands in chip subaccount
- `getEffectiveBalance` = 700
- `listIndebtedPlayers` no longer includes this player

- [ ] **Step 6: Chip-transfer-to-player debt service**

Have another player win PP from this one (now with 700 chips). Verify that mid-game transfers don't break.

Then put the player back in debt (cast Bridge Exploit again, hit backfire). Have a third player's cast send chips to this player (e.g., backfire on someone else). Verify the chips arrive, then debt-service burns part of them back.

- [ ] **Step 7: Referral cascade**

Have an indebted upline. Trigger a downline deposit. Verify the upline's cascade slice services their debt while downliner / sibling uplines get credited normally.

- [ ] **Step 8: Admin clear-debt**

Hit "Clear" on an indebted player in the admin panel. Confirm debt drops to 0 and they disappear from the list.

- [ ] **Step 9: adminMint bypass**

Indebt a player. Use admin panel manual-mint with `bypassDebt = true` to credit them 500 PP. Confirm debt is UNCHANGED and chip balance went up by 500.

Then mint another 500 with `bypassDebt = false` — confirm debt drops by 500 (no chip credit).

---

## Task 13: Open PR

- [ ] **Step 1: Push branch + create PR**

```bash
git push -u origin feat/shenanigan-debt
gh pr create --title "feat(shenanigans): debt / negative effective balance" --body "$(cat <<'EOF'
## Summary

Adds debt as an internal fiction on top of the ICRC-1 ledger: when a cast outcome charges more chips than the caster has, the shortfall lands in a per-player `ppDebt` map. Future credits (mints, referral cascade, chip transfers in) service that debt before crediting the player's chip subaccount.

- New stable map: `ppDebt : Map<Principal, Nat>`.
- New helpers: `creditPlayer` (mint-with-debt-service) and `transferToPlayer` (chip-to-chip-with-debt-service).
- Every player-bound credit path routed through one of those helpers.
- `ShenaniganOutcomeDetail.ppToDebt` reports how much of a win paid down debt instead of landing in chips.
- Admin: `listIndebtedPlayers`, `adminClearDebt`, and a `bypassDebt` flag on `adminMint`.
- Frontend surfaces debt in the wallet display and the cast outcome toast.

## What this isn't

- The PP ledger balance still cannot go negative — debt is a shenanigans-canister fiction.
- Cash-out: an indebted player naturally has chipBalance = 0, so cash-out is gated automatically.
- Casting while indebted: allowed if costSuccess ≤ chip balance. Spells with costSuccess = 0 are castable at 0 chip balance — caster can keep digging.

## Migration note

No explicit migration needed — adding `ppDebt` as a stable field is an implicit compatible change. Task 0 of the plan removes the residual V4 attachment (which already ran on mainnet) so the next upgrade doesn't re-run V4 against the new actor shape.

## Test plan

See plan doc — [`docs/superpowers/plans/2026-05-20-shenanigan-debt-and-negative-balance.md`](docs/superpowers/plans/2026-05-20-shenanigan-debt-and-negative-balance.md) for the full manual verification suite.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

---

## Self-review notes (pre-execution)

- **Atomicity:** `creditPlayer` decrements debt synchronously *before* awaiting the mint. If the mint fails, the debt is already gone but the chips never arrived — meaning the player got "free" debt forgiveness. Bounded (one mint at a time) and arguably fine — but if it bothers, refactor so debt is decremented only on Ok. The current draft picks "synchronous decrement" for simplicity; flag at review.
- **`transferToPlayer`:** the debt-burn-back can fail (race with the receiver spending their just-received chips). The plan accepts this as cosmetic. If it turns out to matter, the alternative is "lock receiver's just-credited amount via a separate stable lookup until the burn-back lands" — overkill for the current feature gravity.
- **Referral cascade:** the cascade entry point is `processMint` (line 1720) which calls `mintInternal` for the recipient AND for the siphoner. Both go through `creditPlayer` now. The actual upline cascade may live further up the call stack (search for `cascadeBps` / `referrerToDownline`) — verify during Task 4 that every upline-payment site also uses `creditPlayer`.
- **`getEffectiveBalance` returns `Int`:** confirm the candid generator emits `int` (not `nat`) for this — if not, change to two separate queries (`getChipBalance`, `getPpDebt`) and let the frontend subtract. (`Int` should generate as `int` per the Motoko book — but verify.)
