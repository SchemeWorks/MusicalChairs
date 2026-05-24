# Push-Observer Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace shenanigans' 60-second polling observer with an event-driven push from ponzi_math, retaining a low-frequency (5-min) polling tick as a safety backstop. Cuts observer-driven cycle burn by ~98% and reduces deposit-to-PP-mint latency from up-to-60s down to ~1s in the happy path.

**Architecture:** ponzi_math gains an admin-settable `shenanigansPrincipal` stable var. On every new game and every backer top-up, ponzi_math fires a single fire-and-forget call to `shenanigans.pokeObserver()`. The call is unawaited and wrapped in `ignore` so a stopped/upgrading/broken shenanigans cannot trap ponzi_math's user-facing endpoints. shenanigans gains a new public `pokeObserver()` endpoint (anyone can call) with a 1-second per-canister rate limit; it runs the existing `observerTick` which is already cursor-idempotent. shenanigans' polling interval is bumped from 60s → 300s as the safety backstop. The push payload carries no data — shenanigans always re-verifies against ponzi_math, which remains the authoritative source.

**Tech Stack:** Motoko (persistent actor, moc 0.16.2), dfx 0.30+, IC mainnet. No frontend changes. No data migration required (all new state is `stable var` with default values).

**Background (for context — not part of execution):**

Diagnosed in conversation on 2026-05-23:

1. CycleOps dashboard showed shenanigans burning 0.62 TC/day, with a step-change jump around 2026-05-10. Memory at 256MB only contributes ~$0.01/day idle — not the culprit.
2. Root cause of the burn: shenanigans' `observerTick` fires every 10s (now bumped to 60s as immediate mitigation, via `setObserverIntervalSeconds(60)` called 2026-05-23). Each tick makes 2 inter-canister calls into ponzi_math (`getAllGames` + `getBackerPositions`), even when nothing has happened. At 60s that's still 2,880 calls/day of pure polling overhead.
3. With only 3 active players, the spell-cast hot path is negligible — the dominant burn is the constant polling.
4. Design discussion concluded on a push pattern: ponzi_math fires-and-forgets a notification on every event; shenanigans' new public endpoint triggers the existing observer; polling backstop handles missed pushes.
5. ponzi_math is planned to be blackholed eventually. Hardcoding shenanigans' canister ID `j56tm-oaaaa-aaaac-qf34q-cai` into ponzi_math is safe because canister IDs are immutable for life. The single-target (vs subscriber-list) design was chosen — switching to a list later is trivial pre-blackhole.

---

## File Structure

**Backend (Motoko):**

- Modify: `shenanigans/main.mo` — add `pokeObserver()` public endpoint near the existing observer functions (~line 957, immediately after `observerTick`). Add rate-limit state vars near line 506 (with the other observer-state vars).
- Modify: `ponzi_math/main.mo` — add `shenanigansPrincipal` stable var (~line 220, near other state), `setShenanigansPrincipal` + `getShenanigansPrincipal` endpoints (near other admin setters, ~line 1660), `notifyShenanigans` helper, and call sites in `startGame` (~line 827, just after `gameRecords := natMap.put`) and `addBackerMoney` (~line 910, just after the backer position write).

**No frontend changes. No tests.** Motoko canisters in this project don't have a test harness; we verify on local replica and via dfx probes against mainnet.

---

## Safety Notes — READ BEFORE EXECUTING

1. **ponzi_math is the money-math canister.** Per project memory: never deploy backend canisters without explicit user permission. The plan's deploy steps say "user deploys" — agent only proposes the dfx command.
2. **shenanigans deploys require a stop→deploy→start dance** due to in-flight observer callbacks. Per `shenanigans_deploy_lineage` memory.
3. **Order matters:** shenanigans must be deployed BEFORE ponzi_math gets the notification calls wired up, otherwise the early notifications will fail (harmlessly — they're fire-and-forget — but the backstop will be the only working path until shenanigans catches up).
4. The user already bumped the observer interval to 60s. **Do NOT lower it back to 10s during local testing** — there's no point and it could confuse mainnet-vs-local comparisons.
5. Local replica testing first; mainnet last. Use small ICP amounts on mainnet (0.1 ICP is the minimum deposit).

---

## Task 1: Add `pokeObserver()` endpoint to shenanigans

**Files:**
- Modify: `shenanigans/main.mo:506` (add rate-limit state)
- Modify: `shenanigans/main.mo:957` (add public endpoint after `observerTick`)

- [ ] **Step 1: Add rate-limit state next to existing observer state**

Open `shenanigans/main.mo`. Find the existing observer-state block at line 504-506:

```motoko
    // Observer lock to prevent concurrent ticks
    transient var observerRunning : Bool = false;
    var observerTimerId : ?Timer.TimerId = null;
```

Replace with:

```motoko
    // Observer lock to prevent concurrent ticks
    transient var observerRunning : Bool = false;
    var observerTimerId : ?Timer.TimerId = null;

    // Rate-limit state for the public pokeObserver() endpoint. Anyone can
    // call pokeObserver; this enforces a 1-second minimum gap between
    // accepted pokes. Transient because absolute monotonicity isn't needed —
    // on upgrade we tolerate a single extra tick. See pokeObserver below.
    transient var lastPokeAt : Int = 0;
    transient let MIN_POKE_GAP_NS : Int = 1_000_000_000; // 1 second
```

- [ ] **Step 2: Add the public `pokeObserver()` endpoint**

Find the end of `observerTick` at line 957 (closes with `observerRunning := false; };`). Immediately after that closing `};`, insert:

```motoko

    /// Public observer nudge. Intended to be called by ponzi_math
    /// immediately after a deposit or backer top-up, so PP mints in ~1s
    /// instead of waiting up to one polling interval. Open to all callers
    /// (not admin-gated) because:
    ///   - The observer is cursor-idempotent (gameIdCursor + backerSeen);
    ///     duplicate pokes do no extra work.
    ///   - The actual minting authority is shenanigans' verify-against-
    ///     ponzi_math step inside observerTick, NOT the poke payload.
    ///   - A 1-second rate limit bounds worst-case abuse to ~86k ticks/day
    ///     (≈ the current polling cost before this migration).
    ///
    /// Silently no-ops if:
    ///   - bootstrap migration hasn't completed (matches observerTick)
    ///   - another tick is already in flight (matches observerTick)
    ///   - last poke was less than MIN_POKE_GAP_NS ago (rate limit)
    public shared func pokeObserver() : async () {
        if (not bootstrapped) return;
        if (observerRunning) return;
        let now = Time.now();
        if (now - lastPokeAt < MIN_POKE_GAP_NS) return;
        lastPokeAt := now;
        await observerTick();
    };
```

- [ ] **Step 3: Build locally to verify the change compiles**

```bash
dfx build shenanigans --check
```

Expected: no errors, no new warnings. If the build fails, fix the syntax and re-run.

- [ ] **Step 4: Commit**

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): add pokeObserver() public endpoint"
```

---

## Task 2: Bump shenanigans default polling interval to 300s

The runtime interval was already bumped to 60s via admin call on 2026-05-23. This step bumps the **default** so freshly initialized canisters start at the new value, and so the source-of-truth in code matches the running config.

**Files:**
- Modify: `shenanigans/main.mo:492`

- [ ] **Step 1: Change the default interval**

Find line 492:

```motoko
        observerIntervalSeconds = 10;
```

Replace with:

```motoko
        observerIntervalSeconds = 300; // 5-min backstop; push notifications from ponzi_math drive real-time mints
```

- [ ] **Step 2: Build locally**

```bash
dfx build shenanigans --check
```

- [ ] **Step 3: Commit**

```bash
git add shenanigans/main.mo
git commit -m "chore(shenanigans): default observer interval to 300s backstop"
```

Note: This change only affects fresh deploys / re-init. The currently-deployed canister will keep using whatever value is in its persistent `mintConfig` (currently 60s from the 2026-05-23 admin call). After deploying this change, run the admin call in Task 6 to bring the runtime value in line.

---

## Task 3: Deploy shenanigans to mainnet

**This step requires explicit user approval before executing.** The agent proposes the commands; the user runs them or confirms.

- [ ] **Step 1: Stop the shenanigans canister to drain in-flight callbacks**

```bash
dfx canister stop shenanigans --network ic
```

Expected output: `Stopped canister shenanigans.`

- [ ] **Step 2: Deploy**

```bash
dfx deploy shenanigans --network ic
```

Expected: clean upgrade. Look for `Upgraded code for canister shenanigans` in the output.

- [ ] **Step 3: Start the canister**

```bash
dfx canister start shenanigans --network ic
```

Expected output: `Started canister shenanigans.`

- [ ] **Step 4: Verify the new endpoint exists**

```bash
dfx canister call shenanigans pokeObserver '()' --network ic
```

Expected output: `()`. (Even if the rate limit or bootstrap guard kicks in, the call succeeds — it just no-ops.)

- [ ] **Step 5: Verify the observer is still healthy**

```bash
dfx canister call shenanigans getObserverStatus --network ic
```

Expected: `running = true`, `intervalSeconds = 60` (still 60 from the earlier admin call; we bump to 300 in Task 6).

---

## Task 4: Add `shenanigansPrincipal` state + admin setter to ponzi_math

**Files:**
- Modify: `ponzi_math/main.mo:220` (state declaration)
- Modify: `ponzi_math/main.mo` near other admin setters (~line 1660-1680) (endpoint)

- [ ] **Step 1: Add the stable var**

Find line 220-223 in `ponzi_math/main.mo`:

```motoko
    var coverChargeBalance : Nat = 0;
    var generalLedger = natMap.empty<GeneralLedgerEntry>();
    var nextGeneralLedgerId : Nat = 0;
    var currentRoundId : Nat = 1;
```

After line 223, insert:

```motoko

    // Optional principal to notify on PP-earning events (new games, backer
    // top-ups). Settable via setShenanigansPrincipal; null disables push
    // notifications (shenanigans falls back to polling). See
    // 2026-05-23-push-observer-migration.md for design.
    var shenanigansPrincipal : ?Principal = null;
```

- [ ] **Step 2: Add the setter and getter near other admin endpoints**

Find a clean spot near other admin setters (line 1660-1680 is the admin block). Insert this group:

```motoko

    /// Set the shenanigans canister principal to notify on PP-earning events.
    /// Pass `null` to disable push notifications; shenanigans will continue
    /// to discover events via its polling backstop. See plan
    /// 2026-05-23-push-observer-migration.md.
    public shared ({ caller }) func setShenanigansPrincipal(p : ?Principal) : async () {
        requireAdmin(caller);
        shenanigansPrincipal := p;
    };

    /// Inspect the current target. Used by deploy verification.
    public query func getShenanigansPrincipal() : async ?Principal {
        shenanigansPrincipal
    };
```

- [ ] **Step 3: Add the notification helper**

Place this helper inside the actor body, before the `startGame` function. A natural spot is right after the `requireAdmin` function (~line 295):

```motoko

    /// Fire-and-forget notification to shenanigans that a PP-earning event
    /// has occurred. No await, no error handling — shenanigans verifies
    /// against ponzi_math's authoritative state and falls back to polling
    /// if this notification is lost. Safe to call even if shenanigans is
    /// stopped, upgrading, or doesn't exist: the future is discarded.
    /// Cycle cost: ~5M cycles per call, dominated by the cross-canister
    /// message base fee.
    func notifyShenanigans() {
        switch (shenanigansPrincipal) {
            case (?p) {
                let s = actor (Principal.toText(p)) : actor {
                    pokeObserver : shared () -> async ();
                };
                ignore s.pokeObserver();
            };
            case null {};
        };
    };
```

**Note on fire-and-forget:** the call is `ignore s.pokeObserver()` — no `await`. Motoko returns a future (`async ()`) which we immediately discard. The message IS sent (the IC schedules it), but ponzi_math never reads the response. If shenanigans traps, returns an error, or doesn't exist, ponzi_math sees nothing. This is the standard fire-and-forget pattern in Motoko. The signature matches shenanigans' actual `pokeObserver` exactly.

- [ ] **Step 4: Build to verify**

```bash
dfx build ponzi_math --check
```

Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add ponzi_math/main.mo
git commit -m "feat(ponzi_math): add shenanigansPrincipal state + notifyShenanigans helper"
```

---

## Task 5: Wire `notifyShenanigans()` into the two event sites in ponzi_math

**Files:**
- Modify: `ponzi_math/main.mo:826` (after `startGame` writes a new game)
- Modify: `ponzi_math/main.mo` (after `addBackerMoney` writes a backer position; ~line 901 or 909)

- [ ] **Step 1: Add notification after new game creation**

Find line 826:

```motoko
            gameRecords := natMap.put(gameRecords, gameId, newGame);
```

Immediately after this line (still inside the same scope), add:

```motoko
            notifyShenanigans();
```

- [ ] **Step 2: Add notification after backer position write in addBackerMoney**

In `addBackerMoney` (starts at line 855), there are two writes to backerPositions:

- Line 901: `backerPositions := backerKeyMap.put(backerPositions, (caller, #seriesA), newBacker);` (new position)
- Line 909: `backerPositions := backerKeyMap.put(backerPositions, (caller, #seriesA), updated);` (top-up to existing position)

After EACH of these two lines, add:

```motoko
                    notifyShenanigans();
```

(Match the indentation level of the line above.)

- [ ] **Step 3: Build to verify**

```bash
dfx build ponzi_math --check
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add ponzi_math/main.mo
git commit -m "feat(ponzi_math): fire notifyShenanigans on new games and backer top-ups"
```

---

## Task 6: Local replica end-to-end test

Verify the push path works on the local replica before touching mainnet.

- [ ] **Step 1: Start the local replica**

```bash
dfx start --clean --background
```

Wait for the "replica is running" line.

- [ ] **Step 2: Deploy both canisters locally**

```bash
dfx deploy shenanigans
dfx deploy ponzi_math
```

Note: local deploys don't need the stop/start dance — only mainnet does.

- [ ] **Step 3: Wire the principal**

```bash
SHENANIGANS_ID=$(dfx canister id shenanigans)
dfx canister call ponzi_math setShenanigansPrincipal "(opt principal \"$SHENANIGANS_ID\")"
dfx canister call ponzi_math getShenanigansPrincipal
```

Expected: `getShenanigansPrincipal` returns `(opt principal "...")` matching `$SHENANIGANS_ID`.

- [ ] **Step 4: Confirm shenanigans accepts pokes**

```bash
dfx canister call shenanigans pokeObserver '()'
```

Expected: `()`.

- [ ] **Step 5: Manually trigger a notify via the seam**

This step depends on what local game-creation calls are available. Easiest is to call ponzi_math's `joinGame` (or whatever the public deposit endpoint is named) with a small amount; harder is just to confirm via the ponzi_math code that the new `notifyShenanigans()` line is reached.

A simpler validation: call `pokeObserver` directly from a test script, then call `getObserverStatus` and confirm the timestamp on `lastPokeAt` advanced (you may need to add a debug query for this if not already exposed; skip if not worth the round-trip).

Pragmatically: this step can be deferred to mainnet verification (Task 8). Local replica game flows are nontrivial to drive without the frontend.

- [ ] **Step 6: Stop the replica**

```bash
dfx stop
```

---

## Task 7: Deploy ponzi_math to mainnet

**This step requires explicit user approval before executing.** ponzi_math holds money math; do not deploy without explicit go-ahead from Charles.

Per project memory: any older branch needs rebase before deploy or M0169/M0216 blocks it. Verify branch is current.

- [ ] **Step 1: Confirm branch is current**

```bash
git fetch origin
git status
git log --oneline -5
```

The branch should be on or rebased onto current main.

- [ ] **Step 2: Local build sanity check**

```bash
dfx build ponzi_math --check
```

- [ ] **Step 3: Deploy**

```bash
dfx deploy ponzi_math --network ic
```

ponzi_math does NOT require stop/start (no observer timer). If the upgrade traps, abort and investigate before retrying.

- [ ] **Step 4: Verify the new endpoint exists**

```bash
dfx canister call ponzi_math getShenanigansPrincipal --network ic
```

Expected: `(null)` — we haven't wired the principal yet.

---

## Task 8: Wire the principal and verify end-to-end on mainnet

- [ ] **Step 1: Set the shenanigans principal in ponzi_math**

```bash
dfx canister call ponzi_math setShenanigansPrincipal '(opt principal "j56tm-oaaaa-aaaac-qf34q-cai")' --network ic
```

Expected: `()`.

- [ ] **Step 2: Verify it stuck**

```bash
dfx canister call ponzi_math getShenanigansPrincipal --network ic
```

Expected: `(opt principal "j56tm-oaaaa-aaaac-qf34q-cai")`.

- [ ] **Step 3: Bump the runtime polling interval to 300s on shenanigans**

```bash
dfx canister call shenanigans setObserverIntervalSeconds '(300)' --network ic
dfx canister call shenanigans getObserverStatus --network ic
```

Expected: `intervalSeconds = 300`.

- [ ] **Step 4: End-to-end smoke test**

User deposits 0.1 ICP into a Simple21Day plan via the frontend. Watch for:

- PP mint completing within ~5 seconds (target: ~1–2 seconds; if it takes 5+ minutes, the push didn't fire and the backstop is doing all the work).
- The `#signup` (or `#deposit`) chat item appearing within the same window.
- `getObserverStatus` showing `gameIdCursor` advanced.

Compare against the previous 60s interval baseline.

- [ ] **Step 5: Watch CycleOps for the next 24-48 hours**

The shenanigans Cycles Burn bars should drop dramatically — target ≤0.05 TC/day (down from 0.62 TC/day pre-mitigation, 0.10 TC/day with the 60s interval).

- [ ] **Step 6: Tag and document the deploy in tuning notes**

Update `docs/superpowers/specs/2026-05-21-shenanigans-future-spells.md` (or the appropriate tuning-notes doc) with:

- Date of deploy
- The before/after burn numbers from CycleOps
- The shenanigans + ponzi_math commit SHAs

Update memory entry `shenanigans_deploy_lineage` and add a new entry for `ponzi_math_deploy_lineage` covering this migration.

---

## Rollback

If anything goes wrong post-deploy:

1. **Push notifications causing issues?** Set `shenanigansPrincipal` to `null` on ponzi_math:
   ```bash
   dfx canister call ponzi_math setShenanigansPrincipal '(null)' --network ic
   ```
   This immediately disables push; shenanigans falls back to pure polling.

2. **Polling interval too slow at 300s?** Lower it:
   ```bash
   dfx canister call shenanigans setObserverIntervalSeconds '(60)' --network ic
   ```
   Returns to the pre-migration mitigation state.

3. **shenanigans deploy broken?** Re-deploy the prior commit's WASM via the stop→deploy→start dance with the last known good commit.

4. **ponzi_math deploy broken?** This is the dangerous one — money math. If broken, revert the commit and re-deploy. Coordinate carefully with Charles.

No data migration was performed; rollback is purely a code revert + redeploy.

---

## Out of scope (intentionally deferred)

- **Round-result notifications.** ponzi_math closes/settles games inside `settleCompoundingGame` etc. — these aren't notified to shenanigans. The polling backstop catches them within 5 min, and round results aren't the latency-sensitive path (the player isn't watching for them in real time).
- **Admin-backdated game creation** (`ponzi_math/main.mo:1768`). Not wired with `notifyShenanigans()` because admin can manually `runObserverOnce` if needed.
- **Subscriber-list pattern.** Single-target principal only. Switching to a `[Principal]` list is a 10-line migration if a second consumer ever shows up before blackhole.
- **Removing the polling loop entirely.** The 5-min backstop is intentional. Don't remove it even after push proves stable.
- **Frontend pinging shenanigans.** Architecture A from the design discussion. Not implemented because the ponzi_math push (Architecture B) is strictly better — survives browser closure, doesn't depend on frontend availability.
