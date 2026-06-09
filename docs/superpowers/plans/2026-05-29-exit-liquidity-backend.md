# Exit Liquidity — Backend Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the server-authoritative Exit Liquidity mini-game (a PP-sink, multi-stage judgment game with a per-Round clout leaderboard) to the `shenanigans` canister.

**Architecture:** Pure-additive state and functions on the existing `shenanigans` persistent actor — no migration module change (new top-level stable `var`s flow through orthogonal persistence; only *modifying* existing stored records needs a migration). The canister is authoritative: it burns the PP buy-in, holds the active run, rolls each rotation server-side, and computes the ranking. The client only submits discrete decisions and reads state — so latency is noise, not signal.

**Tech Stack:** Motoko (`mo:base`, `persistent actor`, `OrderedMap`), the existing `ppLedger` ICRC-1 burn path, `dfx` for local verification.

**Scope:** Backend engine + queries ONLY. The frontend (game UI, leaderboard rendering, hooks) is a separate follow-up plan — this backend is independently testable via `dfx canister call`.

**Testing reality:** There is **no `mops.toml`** — the canister has no Motoko unit-test harness. Verification is a `dfx canister call` matrix on a local replica (Task 6), matching the existing `2026-05-16-shenanigans-v1-design.md` approach. Deterministic logic (banking math, window average, qualifying gate) is asserted directly via calls; the RNG rotation is verified statistically. `vitest` exists but is for the frontend plan.

---

## Conventions to follow (verified against the live code)

- **State:** declare new `var`s alongside the existing effect maps (`shenanigans/main.mo:674`, near `goldenUntil`). Helpers `principalMap`/`natMap` already exist (`:403-404`). New standalone `var`s need **no migration**.
- **Auth:** mirror the cast path — `if (Principal.isAnonymous(caller)) { Debug.trap("Authentication required") };` (`:2788`) for update calls; `markActive(caller)` after.
- **Burn:** `await burnFrom(player, units, memoText)` returns `{ #Ok : Nat; #Err : Text }`; trap on `#Err` (pattern at `:2979-2982`). Then bump `ppBurnedPerPlayer` (`:2989`) and `ppBurnedPerPlayerPerRound` (`:2992-3002`) so buy-ins count toward the existing PP-burned boards.
- **Round id:** `await readCurrentRoundIdCached()` (`:1153`) returns the current ponzi_math round id.
- **Leaderboard:** mirror `getRoundBurnedLeaderboard` (`:4958`) and `getTopPpBurners` (`:4928`) — `Iter.toArray(principalMap.entries(...))`, `Array.sort` by `.1` desc, `Array.subArray(sorted, 0, cap)`.
- **RNG:** `Int.abs(Time.now()) % 100` is the house style (`rollPct`, TUNING_NOTES: "fine for fairness, not adversarial"). Use it for the rotation roll in v1; hardening is deferred (see end).
- **Units:** PP has 8 decimals (`dfx.json` `pp_ledger`), so `1 PP = 100_000_000` units.
- **All score math is integer bps** (`10000 = 1.0x`), matching the codebase's bps convention (e.g. `cascadeInitialBps`).

---

## File structure

- **Modify:** `shenanigans/main.mo` — all engine state, update calls, and queries (additive).
- **Regenerate:** `frontend/src/declarations/shenanigans/*` via `npm run generate` (Task 5) so the new candid is available to the future frontend plan.
- No new files. No `migration.mo` change.

---

## Starting config values (illustrative — tune in the balance pass)

These ship as the admin-tunable defaults in Task 1. They are **starting values**, not final; the balance pass retunes them live via the admin setter.

| Param | Field | Starting value | Meaning |
|------|-------|----------------|---------|
| Buy-in | `buyInUnits` | `1_000_000_000` (10 PP) | PP units burned per run start |
| Stages | `stageCount` | `5` | max rotations per run |
| Stage-1 multiplier | `baseMultiplierBps` | `10000` (1.0x) | riding value entering stage 1 |
| Stage step | `stageStepBps` | `16000` (×1.6) | riding growth per surviving advance |
| Stage-1 hazard | `baseHazardPct` | `15` | rotation chance on the 1→2 advance |
| Hazard step | `hazardStepPct` | `12` | hazard added per later stage |
| Bank fraction | `bankFractionPct` | `50` | portion of riding locked by "bank" |
| Window | `windowSize` | `25` | consecutive runs for ranking + qualifying gate |

**Acceptance for the balance pass (validate in playtest, not here):** under the tuned curve, a Disciplined player (bank ~half/stage, exit on a high-hazard stage) must beat both Turtle (bank-all-then-exit) and Moon (never bank) on best-window average.

---

## Task 1: Config + stable state

**Files:**
- Modify: `shenanigans/main.mo` (types near other type decls; `var`s near `:674`; admin setter near other admin setters)

- [ ] **Step 1: Add the type declarations** (place with the other effect/record types, e.g. just above the effect-map `var` block around `:656`)

```motoko
    // ===== Exit Liquidity — PP-sink judgment game (clout-only) =====
    // All score math is integer bps (10000 = 1.0x).
    type ExitLiquidityConfig = {
        buyInUnits : Nat;        // PP units burned to start a run
        stageCount : Nat;        // max rotations per run
        baseMultiplierBps : Nat; // riding value entering stage 1
        stageStepBps : Nat;      // riding *= stageStepBps/10000 per survived advance
        baseHazardPct : Nat;     // rotation chance on the 1->2 advance
        hazardStepPct : Nat;     // hazard added per later stage
        bankFractionPct : Nat;   // portion of riding locked by a "bank" decision
        windowSize : Nat;        // consecutive runs for ranking + qualifying gate
    };

    // One in-flight run per player. Deleted on completion.
    type ExitRun = {
        startedAt : Int;
        stage : Nat;             // current stage, 1-based
        bankedBps : Nat;         // locked, safe from rotation
        ridingBps : Nat;         // at risk; forfeited on rotation
    };

    type ExitDecision = { #bank; #ride; #exit };

    type ExitRunResult = {
        runScoreBps : Nat;       // = bankedBps at completion
        rotated : Bool;          // true if ended by rotation
        finalStage : Nat;
        qualified : Bool;        // run count this round >= windowSize
        bestWindowAvgBps : Nat;  // 0 until qualified
    };
```

- [ ] **Step 2: Add the stable `var`s** (immediately after `var goldenUntil = ...` at `:674`)

```motoko
    // Exit Liquidity config — admin-tunable; starting defaults below.
    var exitLiquidityConfig : ExitLiquidityConfig = {
        buyInUnits = 1_000_000_000;  // 10 PP
        stageCount = 5;
        baseMultiplierBps = 10000;   // 1.0x
        stageStepBps = 16000;        // x1.6 per advance
        baseHazardPct = 15;
        hazardStepPct = 12;
        bankFractionPct = 50;
        windowSize = 25;
    };
    // In-flight runs, one per player.
    var activeExitRuns = principalMap.empty<ExitRun>();
    // Per round -> player -> rolling buffer of the last `windowSize` run scores (bps).
    var exitRecentScores = natMap.empty<OrderedMap.Map<Principal, [Nat]>>();
    // Per round -> player -> best consecutive-window average (bps). Set only once qualified.
    var exitBestWindowAvg = natMap.empty<OrderedMap.Map<Principal, Nat>>();
    // Per round -> player -> total completed runs this round (qualifying gate).
    var exitRunCount = natMap.empty<OrderedMap.Map<Principal, Nat>>();
    // Vanity, lifetime: biggest single run score ever (bps). Off-rank.
    var exitBiggestRunBps = principalMap.empty<Nat>();
```

- [ ] **Step 3: Add the admin setter** (place near other admin config setters; reuse the existing admin guard — search for how `setShenaniganConfig`/admin setters gate `caller`, and copy that exact guard expression)

```motoko
    public shared ({ caller }) func setExitLiquidityConfig(cfg : ExitLiquidityConfig) : async () {
        assertAdmin(caller);  // REPLACE with the repo's actual admin guard used by other setters
        exitLiquidityConfig := cfg;
    };

    public query func getExitLiquidityConfig() : async ExitLiquidityConfig {
        exitLiquidityConfig;
    };
```

> Implementer note: confirm the admin-guard call (`assertAdmin` is a placeholder name) by reading an existing setter such as the one that writes `shenaniganConfigs`. Use the identical guard.

- [ ] **Step 4: Verify it compiles**

Run: `dfx build shenanigans`
Expected: builds with no errors (no migration prompt, since all additions are new top-level `var`s).

- [ ] **Step 5: Commit**

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): Exit Liquidity config + stable state"
```

---

## Task 2: `startExitRun` — burn buy-in, open a run

**Files:**
- Modify: `shenanigans/main.mo` (new update call near `castShenanigan`)

- [ ] **Step 1: Implement `startExitRun`**

```motoko
    // Starts a run: rejects if one is already in flight, burns the buy-in,
    // tallies it toward PP-burned boards, and opens the run at stage 1.
    public shared ({ caller }) func startExitRun() : async ExitRun {
        if (Principal.isAnonymous(caller)) { Debug.trap("Authentication required") };
        markActive(caller);

        switch (principalMap.get(activeExitRuns, caller)) {
            case (?_) { throw Error.reject("You already have a run in progress. Finish it first.") };
            case null {};
        };

        let cfg = exitLiquidityConfig;

        // Burn the buy-in (traps on failure, mirroring the cast path).
        switch (await burnFrom(caller, cfg.buyInUnits, "exit-liquidity-buyin")) {
            case (#Err(msg)) { Debug.trap("Buy-in burn failed: " # msg) };
            case (#Ok(_)) {};
        };

        // Tally toward existing PP-burned leaderboards (lifetime + per-round).
        let priorBurn = switch (principalMap.get(ppBurnedPerPlayer, caller)) { case null 0; case (?n) n };
        ppBurnedPerPlayer := principalMap.put(ppBurnedPerPlayer, caller, priorBurn + cfg.buyInUnits);
        let roundId = await readCurrentRoundIdCached();
        let rb : OrderedMap.Map<Principal, Nat> = switch (natMap.get(ppBurnedPerPlayerPerRound, roundId)) {
            case (?m) m; case null principalMap.empty<Nat>();
        };
        let rbPrior = switch (principalMap.get(rb, caller)) { case (?n) n; case null 0 };
        ppBurnedPerPlayerPerRound := natMap.put(ppBurnedPerPlayerPerRound, roundId, principalMap.put(rb, caller, rbPrior + cfg.buyInUnits));

        let run : ExitRun = {
            startedAt = Time.now();
            stage = 1;
            bankedBps = 0;
            ridingBps = cfg.baseMultiplierBps;
        };
        activeExitRuns := principalMap.put(activeExitRuns, caller, run);
        run;
    };
```

- [ ] **Step 2: Build**

Run: `dfx build shenanigans`
Expected: compiles clean.

- [ ] **Step 3: Commit**

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): startExitRun buy-in + run open"
```

---

## Task 3: `exitRunDecision` — bank / ride / exit + rotation roll + scoring

**Files:**
- Modify: `shenanigans/main.mo` (new update call + a private `finalizeExitRun` helper)

- [ ] **Step 1: Add the `finalizeExitRun` scoring helper** (private; place just above `startExitRun`)

```motoko
    // Records a completed run's score into the per-round rolling window,
    // updates the qualifying count, best consecutive-window average, and the
    // lifetime vanity max. Returns (qualified, bestWindowAvgBps).
    func finalizeExitRun(player : Principal, roundId : Nat, runScoreBps : Nat, cfg : ExitLiquidityConfig) : (Bool, Nat) {
        // Run count (per round).
        let countMap : OrderedMap.Map<Principal, Nat> = switch (natMap.get(exitRunCount, roundId)) {
            case (?m) m; case null principalMap.empty<Nat>();
        };
        let newCount = (switch (principalMap.get(countMap, player)) { case (?n) n; case null 0 }) + 1;
        exitRunCount := natMap.put(exitRunCount, roundId, principalMap.put(countMap, player, newCount));

        // Rolling buffer of the last windowSize scores (per round).
        let scoresMap : OrderedMap.Map<Principal, [Nat]> = switch (natMap.get(exitRecentScores, roundId)) {
            case (?m) m; case null principalMap.empty<[Nat]>();
        };
        let prior : [Nat] = switch (principalMap.get(scoresMap, player)) { case (?a) a; case null [] };
        // Append, then keep only the trailing windowSize entries.
        let appended = Array.append<Nat>(prior, [runScoreBps]);
        let buf : [Nat] = if (appended.size() <= cfg.windowSize) { appended }
            else { Array.subArray<Nat>(appended, appended.size() - cfg.windowSize, cfg.windowSize) };
        exitRecentScores := natMap.put(exitRecentScores, roundId, principalMap.put(scoresMap, player, buf));

        // Vanity max (lifetime).
        let priorMax = switch (principalMap.get(exitBiggestRunBps, player)) { case (?n) n; case null 0 };
        if (runScoreBps > priorMax) {
            exitBiggestRunBps := principalMap.put(exitBiggestRunBps, player, runScoreBps);
        };

        // Best consecutive-window average: only once the buffer is full (qualified).
        if (newCount >= cfg.windowSize and buf.size() == cfg.windowSize) {
            var sum : Nat = 0;
            for (s in buf.vals()) { sum += s };
            let windowAvg = sum / cfg.windowSize;
            let bwMap : OrderedMap.Map<Principal, Nat> = switch (natMap.get(exitBestWindowAvg, roundId)) {
                case (?m) m; case null principalMap.empty<Nat>();
            };
            let priorBest = switch (principalMap.get(bwMap, player)) { case (?n) n; case null 0 };
            let best = if (windowAvg > priorBest) { windowAvg } else { priorBest };
            exitBestWindowAvg := natMap.put(exitBestWindowAvg, roundId, principalMap.put(bwMap, player, best));
            (true, best);
        } else {
            (false, 0);
        };
    };
```

- [ ] **Step 2: Implement `exitRunDecision`**

```motoko
    // Advances the caller's run. #exit banks everything and ends (safe).
    // #bank locks bankFractionPct of riding, then rolls the rotation to advance.
    // #ride keeps the whole stack riding, then rolls the rotation to advance.
    // A rotation forfeits ridingBps; the run ends with score = bankedBps.
    public shared ({ caller }) func exitRunDecision(decision : ExitDecision) : async ExitRunResult {
        if (Principal.isAnonymous(caller)) { Debug.trap("Authentication required") };
        markActive(caller);

        let cfg = exitLiquidityConfig;
        let run = switch (principalMap.get(activeExitRuns, caller)) {
            case (?r) r;
            case null { throw Error.reject("No run in progress. Start one first.") };
        };
        let roundId = await readCurrentRoundIdCached();

        // #exit: bank all riding and finish (no rotation roll — exiting is safe).
        if (decision == #exit) {
            let score = run.bankedBps + run.ridingBps;
            activeExitRuns := principalMap.delete(activeExitRuns, caller);
            let (qualified, best) = finalizeExitRun(caller, roundId, score, cfg);
            return { runScoreBps = score; rotated = false; finalStage = run.stage; qualified; bestWindowAvgBps = best };
        };

        // #bank: lock a fraction of riding before the roll.
        var banked = run.bankedBps;
        var riding = run.ridingBps;
        if (decision == #bank) {
            let locked = (riding * cfg.bankFractionPct) / 100;
            banked += locked;
            riding -= locked;
        };

        // Rotation roll for the advance out of the current stage.
        // Hazard rises with stage: base + (stage-1)*step, capped at 95%.
        let rawHazard = cfg.baseHazardPct + (run.stage - 1) * cfg.hazardStepPct;
        let hazardPct = if (rawHazard > 95) { 95 } else { rawHazard };
        let roll = Int.abs(Time.now()) % 100;  // house RNG style; see hardening note

        if (roll < hazardPct) {
            // Rotated: forfeit riding, finish with banked.
            activeExitRuns := principalMap.delete(activeExitRuns, caller);
            let (qualified, best) = finalizeExitRun(caller, roundId, banked, cfg);
            return { runScoreBps = banked; rotated = true; finalStage = run.stage; qualified; bestWindowAvgBps = best };
        };

        // Survived: grow riding, advance a stage.
        let grownRiding = (riding * cfg.stageStepBps) / 10000;
        let nextStage = run.stage + 1;

        if (nextStage > cfg.stageCount) {
            // Cleared the final stage: auto-exit (bank everything).
            let score = banked + grownRiding;
            activeExitRuns := principalMap.delete(activeExitRuns, caller);
            let (qualified, best) = finalizeExitRun(caller, roundId, score, cfg);
            return { runScoreBps = score; rotated = false; finalStage = cfg.stageCount; qualified; bestWindowAvgBps = best };
        };

        let advanced : ExitRun = { startedAt = run.startedAt; stage = nextStage; bankedBps = banked; ridingBps = grownRiding };
        activeExitRuns := principalMap.put(activeExitRuns, caller, advanced);
        { runScoreBps = banked; rotated = false; finalStage = nextStage; qualified = false; bestWindowAvgBps = 0 };
    };
```

> Implementer note: confirm `Int` is imported (`import Int "mo:base/Int";`). `Array.append`/`Array.subArray` come from `mo:base/Array` (already imported per the leaderboard funcs). If `Int` is absent, add the import alongside the others at the top.

- [ ] **Step 3: Build**

Run: `dfx build shenanigans`
Expected: compiles clean.

- [ ] **Step 4: Commit**

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): exitRunDecision + rotation roll + scoring"
```

---

## Task 4: Query functions

**Files:**
- Modify: `shenanigans/main.mo` (new queries near `getRoundBurnedLeaderboard` at `:4958`)

- [ ] **Step 1: Add the queries**

```motoko
    // Active run for a player (null if none). For the UI to resume state.
    public query func getActiveExitRun(player : Principal) : async ?ExitRun {
        principalMap.get(activeExitRuns, player);
    };

    // Per-round clout board: ranked by best consecutive-window average (bps).
    // Only qualified players (>= windowSize runs that round) appear.
    // roundId = null -> current round.
    public query func getExitLiquidityLeaderboard(roundId : ?Nat, limit : Nat) : async [(Principal, Nat)] {
        let target : Nat = switch (roundId) { case (?r) r; case null cachedCurrentRoundId };
        switch (natMap.get(exitBestWindowAvg, target)) {
            case null { [] };
            case (?m) {
                let entries = Iter.toArray(principalMap.entries(m));
                let sorted = Array.sort<(Principal, Nat)>(entries, func(a, b) = Nat.compare(b.1, a.1));
                let cap = if (limit < sorted.size()) { limit } else { sorted.size() };
                Array.subArray(sorted, 0, cap);
            };
        };
    };

    // Lifetime vanity: biggest single run (bps) for a player. Off-rank.
    public query func getExitBiggestRun(player : Principal) : async Nat {
        switch (principalMap.get(exitBiggestRunBps, player)) { case (?n) n; case null 0 };
    };

    // A player's completed-run count for a round (qualifying progress).
    // roundId = null -> current round.
    public query func getExitRunCount(player : Principal, roundId : ?Nat) : async Nat {
        let target : Nat = switch (roundId) { case (?r) r; case null cachedCurrentRoundId };
        switch (natMap.get(exitRunCount, target)) {
            case null { 0 };
            case (?m) { switch (principalMap.get(m, player)) { case (?n) n; case null 0 } };
        };
    };
```

> Implementer note: `cachedCurrentRoundId` is the existing cache var (`:625`). Queries can't `await`, so they read the cache directly — same trade-off the existing per-round leaderboard queries accept.

- [ ] **Step 2: Build**

Run: `dfx build shenanigans`
Expected: compiles clean.

- [ ] **Step 3: Commit**

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): Exit Liquidity queries (leaderboard, run, counts)"
```

---

## Task 5: Regenerate declarations

**Files:**
- Regenerate: `frontend/src/declarations/shenanigans/*`

- [ ] **Step 1: Generate**

Run: `npm run generate`
Expected: `frontend/src/declarations/shenanigans/shenanigans.did` and `.did.d.ts` now include `startExitRun`, `exitRunDecision`, `getExitLiquidityLeaderboard`, `getActiveExitRun`, `getExitBiggestRun`, `getExitRunCount`, `getExitLiquidityConfig`, `setExitLiquidityConfig`, and the `ExitRun`/`ExitDecision`/`ExitRunResult`/`ExitLiquidityConfig` types.

- [ ] **Step 2: Commit**

```bash
git add frontend/src/declarations/shenanigans
git commit -m "chore: regenerate shenanigans declarations for Exit Liquidity"
```

---

## Task 6: Manual verification matrix (local replica)

No Motoko unit runner exists, so verify behavior with `dfx canister call` on a local replica.

**Precondition:** local replica running with `shenanigans` deployed and `initialize(<ponzi_math_id>)` called, the player principal holding PP in its chip subaccount (use the repo's existing local-bootstrap / seed flow; `shenanigans` depends on `pp_ledger` + `ponzi_math`). Use a non-anonymous identity: `dfx identity use <player>`.

- [ ] **Step 1: Config reads back**

Run: `dfx canister call shenanigans getExitLiquidityConfig`
Expected: the starting-values record (buyInUnits = 1_000_000_000, stageCount = 5, windowSize = 25, ...).

- [ ] **Step 2: Start a run burns the buy-in and opens at stage 1**

Run:
```bash
dfx canister call shenanigans getActiveExitRun "(principal \"$(dfx identity get-principal)\")"   # expect (null)
dfx canister call shenanigans startExitRun
dfx canister call shenanigans getActiveExitRun "(principal \"$(dfx identity get-principal)\")"
```
Expected: after start, `?record { stage = 1; bankedBps = 10000; ridingBps = 10000; ... }`. A second `startExitRun` before finishing rejects with "already have a run in progress". (Confirm PP balance dropped by `buyInUnits` via `icrc1_balance_of` on the chip subaccount.)

- [ ] **Step 3: Exit is safe and banks everything**

Run: `dfx canister call shenanigans exitRunDecision "(variant { exit })"`
Expected: `record { runScoreBps = 10000; rotated = false; finalStage = 1; qualified = false; ... }` and `getActiveExitRun` is back to `(null)`.

- [ ] **Step 4: Bank-then-advance protects banked on a later rotation**

Drive a fresh run: `startExitRun`, then `exitRunDecision "(variant { bank })"` repeatedly. Each call returns the new stage on survival or `rotated = true` on a rotation.
Expected: when `rotated = true`, `runScoreBps` equals the previously-banked total (NOT zero, because `#bank` locked portions), and `finalStage` is where it died. A run that `#ride`s every time and rotates returns `runScoreBps = 0`.

- [ ] **Step 5: Qualifying gate + leaderboard**

Complete fewer than `windowSize` runs, then check the board is empty for the player; complete `windowSize`+ runs and re-check.
Run: `dfx canister call shenanigans getExitLiquidityLeaderboard "(null, 10)"`
Expected: empty (or excludes the player) until the player has ≥ `windowSize` completed runs this round; then the player appears with a non-zero best-window average. `getExitRunCount "(principal \"...\", null)"` tracks progress.

- [ ] **Step 6: Vanity max is monotonic and off-rank**

Run: `dfx canister call shenanigans getExitBiggestRun "(principal \"$(dfx identity get-principal)\")"`
Expected: equals the largest single `runScoreBps` seen, never decreases, and is independent of leaderboard qualification.

- [ ] **Step 7: Admin retune takes effect**

Run: `dfx canister call shenanigans setExitLiquidityConfig '(record { buyInUnits = 500_000_000; stageCount = 3; baseMultiplierBps = 10000; stageStepBps = 20000; baseHazardPct = 20; hazardStepPct = 15; bankFractionPct = 50; windowSize = 5 })'` (as admin), then `getExitLiquidityConfig`.
Expected: values updated; a non-admin caller is rejected by the guard.

---

## Self-review (completed against the spec)

- **Spec coverage:** PP-sink buy-in (Task 2) ✓; multi-stage ride with bank/ride/exit (Task 3) ✓; banking locks gains, rotation forfeits only riding (Task 3) ✓; busts-count-as-0× via `runScore = banked` (Task 3) ✓; best-N-consecutive-window ranking + qualifying gate (Task 3/4) ✓; vanity biggest-run off-rank (Task 3/4) ✓; per-Round board reusing the round-id pattern (Task 1/4) ✓; canister-authoritative, turn-based, never exposes hazard (queries return state, not odds) ✓; additive state, no migration ✓.
- **Deferred per spec (not in this plan):** economy/curve tuning (balance pass via `setExitLiquidityConfig`); golden-name interaction + all UI (frontend plan); RNG hardening (below).
- **Type consistency:** `ExitRun`, `ExitDecision`, `ExitRunResult`, `ExitLiquidityConfig` are defined in Task 1 and used unchanged in Tasks 2–4. Field names (`bankedBps`, `ridingBps`, `stage`, `windowSize`, `bankFractionPct`) are consistent across tasks.
- **Placeholders:** the only intentional placeholder is `assertAdmin` (Task 1 Step 3) — the implementer must substitute the repo's real admin guard, called out explicitly.

---

## Known deferrals / follow-ups

- **RNG hardening:** the rotation uses `Int.abs(Time.now()) % 100` (house style, clout-only, bounded score limits harm). A determined player could try to time submissions toward a favorable tick. If the board needs to be bot/timing-resistant, move the roll to `Random.blob()` (async) or a per-run committed seed. Deferred.
- **Stuck active run:** `startExitRun` rejects while a run is in flight; the only finisher is `exitRunDecision`. There is no timeout/abandon. If players strand runs, add an `abandonExitRun` (forfeit riding, bank current) — out of scope for v1.
- **Frontend plan (next):** game UI (stage display, bank/ride/exit controls, the silent volatility tell driven off `finalStage`/hazard cues), leaderboard rendering on the cap table, `useShenaniganActor` hooks + react-query wiring, golden-name/champion styling decision. `vitest` covers pure client logic there.

---

## Execution handoff

Two execution options:

**1. Subagent-Driven (recommended)** — dispatch a fresh subagent per task, review between tasks, fast iteration. REQUIRED SUB-SKILL: superpowers:subagent-driven-development.

**2. Inline Execution** — execute tasks in this session with checkpoints. REQUIRED SUB-SKILL: superpowers:executing-plans.
