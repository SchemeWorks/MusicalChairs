# Shenanigan Per-Outcome Costs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the single `cost` field on each shenanigan with three admin-tunable costs (`costSuccess`, `costFailure`, `costBackfire`), so the caster's PP charge depends on what actually happened. If the caster can't afford the charged outcome, burn whatever they have and zero them out (no trap).

**Architecture:**
- Backend (`shenanigans/main.mo`): Split `ShenaniganConfig.cost` into three fields. Reorder `castShenanigan` to roll outcome FIRST, then burn the matching per-outcome cost, clamped to actual balance. Pre-cast gate uses `costSuccess` (the minimum the player commits to paying).
- Migration (`shenanigans/migration.mo` + `(with migration = ...)` on actor): map old single `cost` → all three new fields equal to the old value (no economic change on upgrade; admin retunes via panel).
- Frontend admin panel: three numeric inputs replacing one; list display shows all three; persistence flows through existing `updateShenaniganConfig` / `saveAllShenaniganConfigs` endpoints.
- Frontend player UI: surface all three costs on the spell card so the player understands the wager.
- Generated did/d.ts: regenerate after backend signature changes.

**Tech Stack:** Motoko (canister), TypeScript + React (frontend), dfx for declarations.

---

## File Structure

**Modify:**
- `shenanigans/main.mo` — `ShenaniganConfig` type, default seed, `castShenanigan` flow, admin validators, stats accounting
- `shenanigans/migration.mo` — append a new `runV4` for cost split, wire into actor declaration
- `frontend/src/components/ShenanigansAdminPanel.tsx` — three inputs, type interface, list display, save/saveAll payloads
- `frontend/src/components/Shenanigans.tsx` — surface outcome-conditional costs on the spell cards
- `frontend/src/declarations/shenanigans/*` — regenerated did/d.ts

**Test plan:** No automated test harness exists. Verification is manual via local replica (`dfx deploy shenanigans`) and the admin panel + game UI.

---

## Task 1: Backend type + default seed

**Files:**
- Modify: `shenanigans/main.mo` (ShenaniganConfig type ~line 90, initializeDefaultShenanigans ~line 1237)

- [ ] **Step 1: Update `ShenaniganConfig` type to use three cost fields**

Replace `cost : Float` with three separate fields. Keep field order stable so generated candid is readable.

```motoko
public type ShenaniganConfig = {
    id : Nat;
    name : Text;
    description : Text;
    costSuccess : Float;
    costFailure : Float;
    costBackfire : Float;
    successOdds : Nat;
    failureOdds : Nat;
    backfireOdds : Nat;
    duration : Nat;
    cooldown : Nat;
    effectValues : [Float];
    castLimit : Nat;
    backgroundColor : Text;
};
```

- [ ] **Step 2: Rewrite `initializeDefaultShenanigans` so all 11 spells get all three costs**

Default policy: every cost equals the old single cost. This preserves behavior on a fresh seed; admin retunes from the panel. Keep names/descriptions/odds/etc. exactly as-is.

```motoko
func initializeDefaultShenanigans() {
    let defaultConfigs : [ShenaniganConfig] = [
        { id = 0; name = "MEV Attack"; description = "Sandwich-attacks the target for 2\u{2013}8% of their Ponzi Points (max 250 PP)."; costSuccess = 120.0; costFailure = 120.0; costBackfire = 120.0; successOdds = 60; failureOdds = 25; backfireOdds = 15; duration = 0; cooldown = 2; effectValues = [2.0, 8.0, 250.0]; castLimit = 0; backgroundColor = "#fff9e6" },
        { id = 1; name = "Contagion"; description = "Losses get socialized \u{2014} every player surrenders 1\u{2013}3% (max 60 PP each)."; costSuccess = 600.0; costFailure = 600.0; costBackfire = 600.0; successOdds = 40; failureOdds = 40; backfireOdds = 20; duration = 0; cooldown = 0; effectValues = [1.0, 3.0, 60.0]; castLimit = 1; backgroundColor = "#e6f7ff" },
        { id = 2; name = "Cease & Desist"; description = "Target is forced to change their display name for 7 days."; costSuccess = 200.0; costFailure = 200.0; costBackfire = 200.0; successOdds = 90; failureOdds = 5; backfireOdds = 5; duration = 168; cooldown = 0; effectValues = [7.0]; castLimit = 0; backgroundColor = "#ffe6f7" },
        { id = 3; name = "Trailing Commission"; description = "Skims 5% of target's new PP for 7 days (max 1000 PP)."; costSuccess = 1200.0; costFailure = 1200.0; costBackfire = 1200.0; successOdds = 70; failureOdds = 20; backfireOdds = 10; duration = 168; cooldown = 0; effectValues = [5.0, 1000.0]; castLimit = 0; backgroundColor = "#f3e6ff" },
        { id = 4; name = "Crossline Poach"; description = "Poach one member from target's downline (favors L3)."; costSuccess = 500.0; costFailure = 500.0; costBackfire = 500.0; successOdds = 30; failureOdds = 60; backfireOdds = 10; duration = 0; cooldown = 0; effectValues = []; castLimit = 1; backgroundColor = "#e6fff2" },
        { id = 5; name = "Poison Pill"; description = "Defensive measure \u{2014} blocks one hostile shenanigan."; costSuccess = 200.0; costFailure = 200.0; costBackfire = 200.0; successOdds = 100; failureOdds = 0; backfireOdds = 0; duration = 0; cooldown = 0; effectValues = []; castLimit = 2; backgroundColor = "#fff4e6" },
        { id = 6; name = "Yield Boost"; description = "Earn +5\u{2013}15% additional PP for the rest of the round."; costSuccess = 300.0; costFailure = 300.0; costBackfire = 300.0; successOdds = 100; failureOdds = 0; backfireOdds = 0; duration = 0; cooldown = 0; effectValues = [5.0, 15.0]; castLimit = 1; backgroundColor = "#e6f2ff" },
        { id = 7; name = "Bridge Exploit"; description = "Target loses 25\u{2013}50% of their PP (max 800 PP)."; costSuccess = 900.0; costFailure = 900.0; costBackfire = 900.0; successOdds = 20; failureOdds = 50; backfireOdds = 30; duration = 0; cooldown = 0; effectValues = [25.0, 50.0, 800.0]; castLimit = 0; backgroundColor = "#ffe6e6" },
        { id = 8; name = "Wealth Tax"; description = "A socialist mayor takes office \u{2014} 20% from the top 3 PP holders (max 300 PP/whale)."; costSuccess = 800.0; costFailure = 800.0; costBackfire = 800.0; successOdds = 50; failureOdds = 30; backfireOdds = 20; duration = 0; cooldown = 0; effectValues = [20.0, 300.0]; castLimit = 0; backgroundColor = "#f0e6ff" },
        { id = 9; name = "Override Bonus"; description = "Your downline kicks up 1.3x PP for the rest of the round."; costSuccess = 400.0; costFailure = 400.0; costBackfire = 400.0; successOdds = 100; failureOdds = 0; backfireOdds = 0; duration = 0; cooldown = 0; effectValues = [1.3]; castLimit = 1; backgroundColor = "#e6fffa" },
        { id = 10; name = "Whitelisted"; description = "Gold name on the leaderboard (24h or 7d) \u{2014} the only clout that matters."; costSuccess = 100.0; costFailure = 100.0; costBackfire = 100.0; successOdds = 100; failureOdds = 0; backfireOdds = 0; duration = 24; cooldown = 0; effectValues = [24.0, 168.0]; castLimit = 1; backgroundColor = "#fff0e6" },
    ];
    for (config in defaultConfigs.vals()) {
        shenaniganConfigs := natMap.put(shenaniganConfigs, config.id, config);
    };
};
```

- [ ] **Step 3: Build to confirm the type change compiles in isolation (will fail because callers still reference `config.cost`)**

Run: `mops build` (or `dfx build shenanigans --check`)
Expected: errors at the `config.cost` callsites — we'll fix them in Task 2.

---

## Task 2: Reorder cast flow + clamp-to-balance burn

**Files:**
- Modify: `shenanigans/main.mo` `castShenanigan` (~line 1721) and `updateShenaniganStats` (~line 2299)

- [ ] **Step 1: Reorder `castShenanigan` to roll outcome before burning, charge the per-outcome cost, clamp to balance**

Replace the section from the config lookup through the `updateShenaniganStats` call. Key changes:
1. Pre-cast gate is `casterBalPre < costSuccessUnits` (was `costUnits`).
2. Roll outcome (and rubber-band modifier) BEFORE burning.
3. Burn `min(costForOutcome, casterBalPre)` — clamp so a backfire that exceeds balance just zeros them.
4. `actualBurnedUnits` (the clamped amount) flows into stats + record + ppBurnedPerPlayer.
5. `casterBal` (post-burn) feeds the effect handlers — recompute as `casterBalPre - actualBurnedUnits`.

```motoko
        let config = switch (getConfigForType(shenaniganType)) {
            case (null) { Debug.trap("Unknown shenanigan type") };
            case (?c) { c };
        };
        let costSuccessUnits = ppToUnits(Int.abs(Float.toInt(config.costSuccess)));
        let costFailureUnits = ppToUnits(Int.abs(Float.toInt(config.costFailure)));
        let costBackfireUnits = ppToUnits(Int.abs(Float.toInt(config.costBackfire)));

        let casterBalPre = await getChipBalance(caller);
        if (casterBalPre < costSuccessUnits) {
            Debug.trap("Insufficient chips to cast this shenanigan");
        };

        let castId = nextShenaniganId;

        // Roll the outcome BEFORE burning. The cost charged depends on the
        // outcome rolled, so we have to know the outcome first. Rubber-band
        // modifier still applies to aggressive spells; defensive/cosmetic
        // spells get modifier 0.
        let targetBalForRoll : Nat = switch (target) {
            case (?t) { await getChipBalance(t) };
            case null { 0 };
        };
        let isAggressive = switch (shenaniganType) {
            case (#moneyTrickster) { true };
            case (#aoeSkim) { true };
            case (#mintTaxSiphon) { true };
            case (#downlineHeist) { true };
            case (#purseCutter) { true };
            case (#whaleRebalance) { true };
            case (_) { false };
        };
        let modPct : Int = if (isAggressive) { rubberBandMod(casterBalPre, targetBalForRoll) } else { 0 };
        let outcome = determineOutcomeWithMod(shenaniganType, modPct);

        // Determine the cost this outcome charges, then clamp to balance so
        // an unaffordable backfire/failure just zeros the caster instead of
        // trapping mid-cast. (When debt is added in a follow-up phase, the
        // shortfall will be written here instead of vanishing.)
        let costForOutcomeUnits = switch (outcome) {
            case (#success) { costSuccessUnits };
            case (#fail) { costFailureUnits };
            case (#backfire) { costBackfireUnits };
        };
        let actualBurnedUnits : Nat = if (costForOutcomeUnits <= casterBalPre) {
            costForOutcomeUnits
        } else {
            casterBalPre
        };

        let burnMemo = "cast-" # Nat.toText(castId);
        if (actualBurnedUnits > 0) {
            switch (await burnFrom(caller, actualBurnedUnits, burnMemo)) {
                case (#Err(msg)) { Debug.trap("Burn failed: " # msg) };
                case (#Ok(_)) {};
            };
        };

        let priorBurn = switch (principalMap.get(ppBurnedPerPlayer, caller)) {
            case (null) { 0 };
            case (?n) { n };
        };
        ppBurnedPerPlayer := principalMap.put(ppBurnedPerPlayer, caller, priorBurn + actualBurnedUnits);

        // Caster balance after burn — what they have left when effects fire.
        let casterBal : Nat = casterBalPre - actualBurnedUnits;
        let targetBal : Nat = targetBalForRoll;

        let detail : { ppDeltaCaster : Int; affectedTarget : ?Principal; affectedCount : Nat } = switch (outcome) {
            case (#success) {
                await applySuccessEffect(shenaniganType, caller, target, casterBal, targetBal, castId);
            };
            case (#backfire) {
                await applyBackfireEffect(shenaniganType, caller, target, casterBal, targetBal, castId);
            };
            case (#fail) {
                { ppDeltaCaster = 0; affectedTarget = null; affectedCount = 0 };
            };
        };

        nextShenaniganId += 1;
        // Convert burned units back to whole PP for the record/stats. We
        // record the actual amount paid, not the nominal config cost.
        let actualCostFloat = Float.fromInt(actualBurnedUnits) / Float.fromInt(PpLedger.PP_UNIT_SCALE);
        let newShenanigan : ShenaniganRecord = {
            id = castId;
            user = caller;
            shenaniganType;
            target;
            outcome;
            timestamp = Time.now();
            cost = actualCostFloat;
        };
        shenanigans := natMap.put(shenanigans, castId, newShenanigan);

        let _ = appendChatItem(Principal.fromActor(Self), #spellCast({ castId }));

        if (outcome == #backfire) {
            let coin = Int.abs(Time.now()) % 4;
            if (coin == 0) {
                switch (reginaldPickFor("spellBackfire")) {
                    case (?line) {
                        let _ = appendChatItem(Principal.fromActor(Self), #reginald({ line; triggerKind = "spellBackfire" }));
                    };
                    case (null) {};
                };
            };
        };

        updateShenaniganStats(caller, actualCostFloat, outcome);
        if (outcome == #success or outcome == #backfire) {
            let prior = switch (principalMap.get(spellsCastPerPlayer, caller)) {
                case (null) { 0 };
                case (?n) { n };
            };
            spellsCastPerPlayer := principalMap.put(spellsCastPerPlayer, caller, prior + 1);
        };

        {
            outcome;
            ppDeltaCaster = detail.ppDeltaCaster;
            affectedTarget = detail.affectedTarget;
            affectedCount = detail.affectedCount;
        };
    };
```

- [ ] **Step 2: Build and confirm compilation succeeds**

Run: `dfx build shenanigans --check`
Expected: clean build (admin endpoints still reference `config.cost` — fix in Task 3).

---

## Task 3: Admin validators

**Files:**
- Modify: `shenanigans/main.mo` `updateShenaniganConfig` (~line 3329) and `saveAllShenaniganConfigs` (~line 3345)

- [ ] **Step 1: Update both admin validators to check all three cost fields are non-negative**

```motoko
    public shared ({ caller }) func updateShenaniganConfig(config : ShenaniganConfig) : async () {
        requireAdmin(caller);
        if (config.successOdds + config.failureOdds + config.backfireOdds != 100) {
            Debug.trap("Success, failure, and backfire odds must sum to 100");
        };
        if (config.costSuccess < 0.0 or config.costFailure < 0.0 or config.costBackfire < 0.0
            or config.duration < 0 or config.cooldown < 0 or config.castLimit < 0) {
            Debug.trap("Costs, duration, cooldown, and cast limit must be non-negative");
        };
        shenaniganConfigs := natMap.put(shenaniganConfigs, config.id, config);
    };
```

```motoko
    public shared ({ caller }) func saveAllShenaniganConfigs(configs : [ShenaniganConfig]) : async () {
        requireAdmin(caller);
        for (config in configs.vals()) {
            if (config.successOdds + config.failureOdds + config.backfireOdds != 100) {
                Debug.trap("Success, failure, and backfire odds must sum to 100");
            };
            if (config.costSuccess < 0.0 or config.costFailure < 0.0 or config.costBackfire < 0.0
                or config.duration < 0 or config.cooldown < 0 or config.castLimit < 0) {
                Debug.trap("Costs, duration, cooldown, and cast limit must be non-negative");
            };
            shenaniganConfigs := natMap.put(shenaniganConfigs, config.id, config);
        };
    };
```

- [ ] **Step 2: Build clean**

Run: `dfx build shenanigans --check`
Expected: no errors. Stable signature now incompatible with deployed canister — migration is Task 4.

---

## Task 4: Stable-state migration

**Files:**
- Modify: `shenanigans/migration.mo` (append V4 types + `runV4`)
- Modify: `shenanigans/main.mo` line 31 — add `(with migration = Migration.runV4)` before `persistent actor Self`

- [ ] **Step 1: Append V4 migration to `shenanigans/migration.mo`**

Old config (with single `cost`) → new config (with three costs). Map `old.cost` to all three new fields. The migration domain only declares the fields it transforms — other stable fields flow through implicitly because their types don't change.

```motoko
    // ================================================================
    // V4 — Shenanigan per-outcome costs
    //
    // Splits the single `cost` field on each ShenaniganConfig into three
    // outcome-specific fields. Default policy on migration: map the old
    // single cost to all three new fields, so no spell's economics change
    // implicitly — admin retunes from the admin panel.
    // ================================================================

    import OrderedMap2 "mo:base/OrderedMap";

    type V4OldShenaniganConfig = {
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

    type V4NewShenaniganConfig = {
        id : Nat;
        name : Text;
        description : Text;
        costSuccess : Float;
        costFailure : Float;
        costBackfire : Float;
        successOdds : Nat;
        failureOdds : Nat;
        backfireOdds : Nat;
        duration : Nat;
        cooldown : Nat;
        effectValues : [Float];
        castLimit : Nat;
        backgroundColor : Text;
    };

    type V4OldConfigMap = OrderedMap.Map<Nat, V4OldShenaniganConfig>;
    type V4NewConfigMap = OrderedMap.Map<Nat, V4NewShenaniganConfig>;

    public func runV4(
        old : { var shenaniganConfigs : V4OldConfigMap }
    ) : { var shenaniganConfigs : V4NewConfigMap } {
        let natMap = OrderedMap.Make<Nat>(Nat.compare);
        let migrated = natMap.map<V4OldShenaniganConfig, V4NewShenaniganConfig>(
            old.shenaniganConfigs,
            func(_id : Nat, c : V4OldShenaniganConfig) : V4NewShenaniganConfig {
                {
                    id = c.id;
                    name = c.name;
                    description = c.description;
                    costSuccess = c.cost;
                    costFailure = c.cost;
                    costBackfire = c.cost;
                    successOdds = c.successOdds;
                    failureOdds = c.failureOdds;
                    backfireOdds = c.backfireOdds;
                    duration = c.duration;
                    cooldown = c.cooldown;
                    effectValues = c.effectValues;
                    castLimit = c.castLimit;
                    backgroundColor = c.backgroundColor;
                };
            },
        );
        { var shenaniganConfigs = migrated };
    };
```

Also add `import Nat "mo:base/Nat";` at the top of migration.mo if it's not already there.

- [ ] **Step 2: Attach the migration to the actor**

Edit `shenanigans/main.mo` line 31. Replace `persistent actor Self {` with:

```motoko
import Migration "migration";

// ...other imports...

(with migration = Migration.runV4)
persistent actor Self {
```

(Place the `import Migration "migration";` near the other module imports — line ~22 area, after `import Subaccount`.)

- [ ] **Step 3: Build clean**

Run: `dfx build shenanigans --check`
Expected: no errors.

- [ ] **Step 4: Local upgrade test — seed old-shape data, upgrade, verify**

1. Start with a clean local replica that has the **old** main.mo deployed and a few configs tuned via the admin panel.
2. Switch to the new main.mo + migration.
3. `dfx deploy shenanigans` (upgrade mode).
4. `dfx canister call shenanigans getShenaniganConfigs` and verify each config has `costSuccess = costFailure = costBackfire = (old cost)`.

If you can't easily reproduce the deployed shape locally, at minimum:
- Build deploys cleanly (`dfx build shenanigans`)
- Fresh-install test exercises `initializeDefaultShenanigans` (which seeds the new shape directly)

- [ ] **Step 5: Commit backend + migration together**

```bash
git add shenanigans/main.mo shenanigans/migration.mo
git commit -m "feat(shenanigans): per-outcome costs (success/fail/backfire)"
```

---

## Task 5: Regenerate frontend declarations

**Files:**
- Modify (generated): `frontend/src/declarations/shenanigans/shenanigans.did`, `shenanigans.did.d.ts`, `shenanigans.did.js`, `index.d.ts`, `index.js`

- [ ] **Step 1: Regenerate did/declarations**

```bash
dfx generate shenanigans
```

- [ ] **Step 2: Verify `costSuccess`, `costFailure`, `costBackfire` appear in the generated d.ts**

```bash
grep -n "costSuccess\|costFailure\|costBackfire" frontend/src/declarations/shenanigans/shenanigans.did.d.ts
```
Expected: matches for all three.

- [ ] **Step 3: Commit generated declarations**

```bash
git add frontend/src/declarations/shenanigans
git commit -m "chore(shenanigans): regenerate declarations after cost-split"
```

---

## Task 6: Admin panel — three inputs

**Files:**
- Modify: `frontend/src/components/ShenanigansAdminPanel.tsx`

- [ ] **Step 1: Update `ShenaniganConfig` interface (top of file, ~line 40)**

```tsx
interface ShenaniganConfig {
  id: number;
  name: string;
  description: string;
  costSuccess: number;
  costFailure: number;
  costBackfire: number;
  successOdds: number;
  failureOdds: number;
  backfireOdds: number;
  duration: number;
  cooldown: number;
  effectValues: number[];
  castLimit: number;
  backgroundColor: string;
}
```

- [ ] **Step 2: Update `mappedConfigs` in the useEffect (~line 124) to read all three costs**

```tsx
      const mappedConfigs = backendConfigs.map(config => ({
        id: Number(config.id),
        name: config.name,
        description: config.description,
        costSuccess: config.costSuccess,
        costFailure: config.costFailure,
        costBackfire: config.costBackfire,
        successOdds: Number(config.successOdds),
        failureOdds: Number(config.failureOdds),
        backfireOdds: Number(config.backfireOdds),
        duration: Number(config.duration),
        cooldown: Number(config.cooldown),
        effectValues: config.effectValues,
        castLimit: Number(config.castLimit),
        backgroundColor: config.backgroundColor,
      }));
```

- [ ] **Step 3: Update `handleSaveShenanigan` validation + payload (~line 152)**

```tsx
  const handleSaveShenanigan = async () => {
    if (!selectedShenanigan) return;
    const oddsSum = selectedShenanigan.successOdds + selectedShenanigan.failureOdds + selectedShenanigan.backfireOdds;
    if (oddsSum !== 100) { toast.error('Odds must sum to 100%'); return; }
    if (selectedShenanigan.costSuccess < 0 || selectedShenanigan.costFailure < 0 || selectedShenanigan.costBackfire < 0
        || selectedShenanigan.cooldown < 0 || selectedShenanigan.duration < 0 || selectedShenanigan.castLimit < 0) {
      toast.error('Numeric values cannot be negative'); return;
    }
    try {
      await updateConfig.mutateAsync({
        id: BigInt(selectedShenanigan.id), name: selectedShenanigan.name,
        description: selectedShenanigan.description,
        costSuccess: selectedShenanigan.costSuccess,
        costFailure: selectedShenanigan.costFailure,
        costBackfire: selectedShenanigan.costBackfire,
        successOdds: BigInt(selectedShenanigan.successOdds), failureOdds: BigInt(selectedShenanigan.failureOdds),
        backfireOdds: BigInt(selectedShenanigan.backfireOdds), duration: BigInt(selectedShenanigan.duration),
        cooldown: BigInt(selectedShenanigan.cooldown), effectValues: selectedShenanigan.effectValues,
        castLimit: BigInt(selectedShenanigan.castLimit), backgroundColor: selectedShenanigan.backgroundColor,
      });
      setShenanigans(prev => prev.map(s => s.id === selectedShenanigan.id ? selectedShenanigan : s));
      window.dispatchEvent(new CustomEvent('shenaniganUpdated', {
        detail: {
          id: selectedShenanigan.id, name: selectedShenanigan.name, icon: shenaniganIcons[selectedShenanigan.id],
          description: selectedShenanigan.description,
          costSuccess: selectedShenanigan.costSuccess,
          costFailure: selectedShenanigan.costFailure,
          costBackfire: selectedShenanigan.costBackfire,
          successOdds: selectedShenanigan.successOdds, failOdds: selectedShenanigan.failureOdds,
          backfireOdds: selectedShenanigan.backfireOdds, effectValues: selectedShenanigan.effectValues.join(', '),
        }
      }));
      toast.success(`${selectedShenanigan.name} saved`);
    } catch (error: any) {
      toast.error(`Save failed: ${error.message || 'Unknown error'}`);
    }
  };
```

- [ ] **Step 4: Update `handleSaveAllChanges` (~line 195) the same way**

```tsx
  const handleSaveAllChanges = async () => {
    for (const shen of shenanigans) {
      const oddsSum = shen.successOdds + shen.failureOdds + shen.backfireOdds;
      if (oddsSum !== 100) { toast.error(`${shen.name}: Odds must sum to 100%`); return; }
      if (shen.costSuccess < 0 || shen.costFailure < 0 || shen.costBackfire < 0
          || shen.cooldown < 0 || shen.duration < 0 || shen.castLimit < 0) {
        toast.error(`${shen.name}: Numeric values cannot be negative`); return;
      }
    }
    try {
      await saveAllConfigs.mutateAsync(shenanigans.map(shen => ({
        id: BigInt(shen.id), name: shen.name, description: shen.description,
        costSuccess: shen.costSuccess, costFailure: shen.costFailure, costBackfire: shen.costBackfire,
        successOdds: BigInt(shen.successOdds), failureOdds: BigInt(shen.failureOdds),
        backfireOdds: BigInt(shen.backfireOdds), duration: BigInt(shen.duration),
        cooldown: BigInt(shen.cooldown), effectValues: shen.effectValues,
        castLimit: BigInt(shen.castLimit), backgroundColor: shen.backgroundColor,
      })));
      shenanigans.forEach(shen => {
        window.dispatchEvent(new CustomEvent('shenaniganUpdated', {
          detail: {
            id: shen.id, name: shen.name, icon: shenaniganIcons[shen.id],
            description: shen.description,
            costSuccess: shen.costSuccess, costFailure: shen.costFailure, costBackfire: shen.costBackfire,
            successOdds: shen.successOdds, failOdds: shen.failureOdds,
            backfireOdds: shen.backfireOdds, effectValues: shen.effectValues.join(', '),
          }
        }));
      });
      // (rest of function unchanged)
```

- [ ] **Step 5: Replace the single "Cost (PP)" input in the editor with three inputs**

Replace the block currently rendering `<AdminInput label="Cost (PP)" ...>` (around line 356).

Replace the parent grid section as follows so all three costs fit in a compact row:

```tsx
                <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                  <AdminInput label="Name" value={selectedShenanigan.name}
                    onChange={v => updateField('name', v)} />
                  <div />
                </div>

                <div>
                  <div className="text-xs font-bold uppercase tracking-wider text-white/50 mb-2">
                    Caster Cost (PP) by Outcome
                  </div>
                  <div className="grid grid-cols-3 gap-3">
                    <AdminInput label="On Success" type="number" value={selectedShenanigan.costSuccess}
                      onChange={v => updateField('costSuccess', Math.max(0, parseFloat(v) || 0))} min="0"
                      hint="0 = success is free for aggressive spells" />
                    <AdminInput label="On Failure" type="number" value={selectedShenanigan.costFailure}
                      onChange={v => updateField('costFailure', Math.max(0, parseFloat(v) || 0))} min="0"
                      hint="What they pay when nothing happens" />
                    <AdminInput label="On Backfire" type="number" value={selectedShenanigan.costBackfire}
                      onChange={v => updateField('costBackfire', Math.max(0, parseFloat(v) || 0))} min="0"
                      hint="What they pay when it blows up. If broke, they zero out." />
                  </div>
                </div>
```

- [ ] **Step 6: Update the selector list display (~line 316) to show all three**

```tsx
                          <div className="text-[11px] mc-text-muted">
                            {shen.costSuccess}/{shen.costFailure}/{shen.costBackfire} PP · {shen.successOdds}/{shen.failureOdds}/{shen.backfireOdds}
                          </div>
```

- [ ] **Step 7: Build the frontend and verify clean**

Run: `cd frontend && npx tsc --noEmit`
Expected: no type errors.

- [ ] **Step 8: Commit**

```bash
git add frontend/src/components/ShenanigansAdminPanel.tsx
git commit -m "feat(shenanigans-admin): edit per-outcome costs"
```

---

## Task 7: Player-facing UI — show per-outcome costs

**Files:**
- Modify: `frontend/src/components/Shenanigans.tsx`

- [ ] **Step 1: Find every reference to `config.cost` / `shen.cost` in Shenanigans.tsx**

```bash
grep -n "\.cost\b\|cost:" frontend/src/components/Shenanigans.tsx
```
Map each usage to either `costSuccess` (the upfront commitment), or the appropriate cost field.

- [ ] **Step 2: Replace single-cost displays with all three**

For each spell-card cost display, replace `{shen.cost} PP` with a compact line that shows all three (e.g. `{shen.costSuccess}/{shen.costFailure}/{shen.costBackfire} PP`).
For the "Insufficient PP" / cast-cost gating check, use `costSuccess` (the upfront commitment).

(Exact diffs depend on the current Shenanigans.tsx layout — apply principle: show all three on the card; gate cast on `costSuccess`.)

- [ ] **Step 3: TypeScript check**

```bash
cd frontend && npx tsc --noEmit
```
Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/Shenanigans.tsx
git commit -m "feat(shenanigans-ui): surface per-outcome costs on spell cards"
```

---

## Task 8: Manual verification

**No automated tests exist** — verify on a local replica.

- [ ] **Step 1: Deploy locally**

```bash
dfx start --background --clean
dfx deploy
```

- [ ] **Step 2: Verify default seed**

Open the admin panel, confirm every spell shows three distinct cost inputs all initialized to the legacy single value.

- [ ] **Step 3: Tune Bridge Exploit (id=7) to confirm spec**

In admin panel set Bridge Exploit: success=0 / fail=500 / backfire=1500. Save.

- [ ] **Step 4: Cast a guaranteed-success spell (Magic Mirror, id=5)**

From a player account with known balance, cast Poison Pill (`magicMirror`). Confirm: chip balance drops by `costSuccess` only.

- [ ] **Step 5: Cast Bridge Exploit repeatedly to hit all three outcomes**

Cast 10–20 times. After each cast confirm:
- Success → balance dropped by 0
- Fail → balance dropped by 500
- Backfire → balance dropped by 1500 (or zeroed if you had < 1500)

- [ ] **Step 6: Zero-out test — set up insufficient balance**

1. Drain a test account down to 200 PP.
2. Try to cast Bridge Exploit. Pre-cast gate is `costSuccess=0`, so cast goes through.
3. Cast repeatedly until a backfire fires. Expected: balance → 0 (not negative, not trap).

- [ ] **Step 7: Stats/history sanity check**

Verify `getShenaniganHistory` records show the **actual** cost paid (e.g., 200 for the zero-out, not 1500) and `ShenaniganStats.totalSpent` aggregates correctly.

---

## Task 9: Open PR

- [ ] **Step 1: Push branch + open PR with summary**

```bash
git push -u origin feat/shenanigan-per-outcome-costs
gh pr create --title "feat(shenanigans): per-outcome costs (success/fail/backfire)" --body "..."
```

PR body should call out:
- Type/API change on `ShenaniganConfig` (one field becomes three)
- Stable-state migration runV4 (load-bearing — note the upgrade test was/wasn't performed against deployed-shape data)
- Admin panel + player UI updates
- Backfire-overshoot policy: clamp to balance (zero them out); debt is the follow-up feature, not in this PR
