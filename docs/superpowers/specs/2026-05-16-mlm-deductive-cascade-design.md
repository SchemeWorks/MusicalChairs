# MLM Deductive Cascade — Design Spec

**Status:** Approved design, not yet implemented.
**Date:** 2026-05-16.
**Scope:** v1 of the MLM redesign. NFT/Harberger v2 (per [docs/PONZI_POINTS_REDESIGN.md](../../../../keen-franklin-a424d7/docs/PONZI_POINTS_REDESIGN.md)) deferred.
**Touches:** `shenanigans/main.mo` only. No changes to `ponzi_math` or `backend`.

## Motivation

The MLM pyramid currently runs as an **additive** cascade in [shenanigans/main.mo:853](../../../shenanigans/main.mo:853): each PP mint event credits L1/L2/L3 uplines with freshly minted PP at 8%/5%/2% of the mint base. The depositor keeps 100% of their mint; uplines get bonus PP that costs the downline nothing. This was implemented at commit `7c6e80f` (2026-04-20) and unchanged since (PR #27 added stats instrumentation but not math changes).

The intended design — captured in `PONZI_POINTS_REDESIGN.md` and verbally restated by the user on 2026-05-16 — is a **deductive** cascade: 10% of the mint comes out of the depositor's PP and is distributed up the chain with 50% passthrough at each active level. Total PP minted per event is conserved (no extra mints).

This spec defines v1: replace the additive cascade with the deductive one, add a signup gift, expose toast/badge data, and provide the abstraction `getPayoutTarget` that v2 will swap to NFT-ownership lookup.

## Settled decisions

| # | Decision | Value |
|---|---|---|
| 1 | Cascade scope | Every in-game PP mint event (game deposits, backer deltas, signup gift). Wallet↔chip transfers and player-to-player transfers excluded. |
| 2 | No-referrer rule | Charles (`housePrincipal`) is the catch-all upline. Every cascade fires. |
| 3 | Activity gating | Admin-tunable. Default: lifetime ≥0.1 ICP deposit. Tunable via `activityRequiresDeposit : Bool` and `activityWindowDays : ?Nat`. |
| 4 | Skipped-upline routing | Flow-around. Inactive uplines treated as nonexistent; chain compacts. |
| 5 | Charles principal | Dedicated `housePrincipal : ?Principal` field, admin-settable, defaults to deploying admin at first init. |
| 6 | Late `registerReferral` | Document-only. Past cascades to Charles stay there. Frontend gates registration before deposit. |
| 7 | Depth cap | Hardcoded `CASCADE_DEPTH_CAP = 10`. Residual to Charles. Cycle detection via visited-set, stop on revisit. |

## Mechanic

### Cascade math

For every PP mint event of size `baseUnits`:

```
playerNet      = baseUnits * (10_000 - cascadeInitialBps) / 10_000
cascadeUnits   = baseUnits - playerNet
```

With default config (`cascadeInitialBps = 1000`): `playerNet = 90% of base`, `cascadeUnits = 10% of base`.

The cascade walks the chain via `getPayoutTarget` (Charles = catch-all), skipping inactive uplines (flow-around), paying each active upline `cascadePassthroughBps / 10_000` (default 50%) of `remaining`, capped at `CASCADE_DEPTH_CAP = 10`. Residual to Charles.

### Worked example

Player earns 4000 PP-units. Chain: Player → Alice (active) → Bob (inactive) → Carol (active) → ... empty upward. Charles is the house.

| Step | Recipient | Receives | Keeps | Passes up | Note |
|---|---|---|---|---|---|
| 0 | Player | 4000 (mint) | 3600 | 400 | Initial deduction (10%). |
| 1 | Alice (L1, active) | 400 | 200 | 200 | Effective L1. 50% passthrough. |
| 2 | Bob (chain L2, inactive) | — | — | — | Skipped; chain compacts. |
| 3 | Carol (chain L3, active L2) | 200 | 100 | 100 | Effective L2. |
| 4 | (no further uplines) | — | — | — | `referralChain.get(Carol)` returns null. |
| 5 | Charles (catch-all/residual) | 100 | 100 | — | Receives residual via `getPayoutTarget(Carol) → house`, then cycle-detected. |

Conservation check: 3600 (player) + 200 (Alice) + 100 (Carol) + 100 (Charles) = 4000. ✓

All arithmetic is integer division (`remaining * cascadePassthroughBps / 10_000`). At realistic mint sizes (e.g. a 1 ICP simple deposit = 200 × 10⁸ PP-units) the rounding error per hop is at most a single unit, and any unminted remainder lands on the house via the residual sweep — conservation holds exactly.

### What the cascade does NOT apply to

- External wallet ↔ chip-subaccount PP transfers
- Player-to-player chip transfers (`chipTransfer`)
- Shenanigan-driven PP movement (Money Trickster, AOE Skim, Purse Cutter, Whale Rebalance, etc. — all currently use `chipTransfer` or `burnFrom`, not `mintInternal`)
- ICP transfers, position payouts, ICP withdrawals (those don't mint PP)

The trigger surface is exactly the set of `mintInternal` call sites inside the observer loop today: [shenanigans/main.mo:500](../../../shenanigans/main.mo:500) (game mint) and [shenanigans/main.mo:528](../../../shenanigans/main.mo:528) (backer mint), plus the new signup gift mint.

## State changes

### New stable variables in `shenanigans/main.mo`

```motoko
// House identity (catch-all upline + residual destination).
// Initialized to the deploying admin on first init; settable by admin.
var housePrincipal : ?Principal = null;

// Tracks signup-gift claim time per player. Doubles as the "join time"
// for the toast/badge feature. The existing `var signupGiftPp : Nat = 0`
// at shenanigans/main.mo:271 is left in place (deprecated, unused) to
// avoid stable-state migration risk; the new amount lives in mintConfig.
var signupGiftClaimed = principalMap.empty<Int>();  // Principal → claim timestamp (ns)

// Per-principal last-qualifying-deposit time, populated by the observer.
// Drives isActive() without per-cascade inter-canister calls.
var lastQualifyingDeposit = principalMap.empty<Int>();  // Principal → ns
```

### Extended `MintConfig`

Five new admin-tunable fields:

```motoko
type MintConfig = {
    // ... existing fields preserved ...
    cascadeInitialBps       : Nat;     // default 1000  (10% off the top)
    cascadePassthroughBps   : Nat;     // default 5000  (50% kept by each active upline)
    signupGiftPp            : Nat;     // default 500   (PP minted on first deposit)
    activityRequiresDeposit : Bool;    // default true
    activityWindowDays      : ?Nat;    // default null  (null = lifetime)
    // ... referralL1Bps/L2Bps/L3Bps remain (deprecated, unused by new cascade) ...
};
```

The three existing `referralL[1-3]Bps` fields are kept in the record to maintain candid-interface stability with prior `MintConfig` consumers. Marked deprecated in code comments. The new cascade ignores them.

### Reused state (unchanged)

- `referralChain : Map<Principal, Principal>` — populated by `registerReferral`, unchanged.
- `referralEarnings : Map<Principal, ReferralEarnings>` — kept. L1/L2/L3 buckets are repurposed to mean **active-upline position** (closest, second-closest, third-closest active upline). Active-rank ≥4 still gets paid by the cascade but is not tracked in display buckets.
- `referrerToDownline : Map<Principal, List<Principal>>` — already declared at [shenanigans/main.mo:270](../../../shenanigans/main.mo:270) but unpopulated. Backfilled at upgrade from `referralChain`; populated incrementally on `registerReferral`.

## Algorithm

### `distributeDeductiveCascade`

Replaces `cascadeReferralMint`. Renamed for clarity.

```motoko
func distributeDeductiveCascade(originUser : Principal, cascadeUnits : Nat, eventId : Text) : async () {
    if (cascadeUnits == 0) return;
    var remaining : Nat = cascadeUnits;
    var visited = Set.empty<Principal>(); visited.add(originUser);
    var depth : Nat = 0;
    var activeRank : Nat = 0;
    var current : Principal = originUser;

    label walk while (remaining > 0 and depth < CASCADE_DEPTH_CAP) {
        let next = getPayoutTarget(current);  // referralChain.get(current) ?? house()
        if (visited.contains(next)) break walk;  // cycle stop
        visited.add(next);
        depth += 1;
        if (not isActive(next)) { current := next; continue walk };

        activeRank += 1;
        let payout = remaining * mintConfig.cascadePassthroughBps / 10_000;
        switch (await mintInternal(next, payout, "cascade-A" # Nat.toText(activeRank) # "-" # eventId)) {
            case (#Ok(_)) {
                // Display buckets are L1/L2/L3 only. activeRank >= 4 still
                // receives the payout via the mint above; we just don't
                // inflate the L3 bucket with their share.
                if (activeRank <= 3) { bumpReferralEarnings(next, activeRank, payout) };
            };
            case (#Err(_)) {};
        };
        remaining -= payout;
        current := next;
    };

    // Residual lands on Charles (covers cycle break, depth cap, chain-exhausted).
    if (remaining > 0) {
        let _ = await mintInternal(house(), remaining, "cascade-residual-" # eventId);
    };
};
```

Memo prefix `cascade-A{N}-` (where N is active-rank, not chain depth) makes ledger entries self-describing and lets `mintInternal`'s memo-based dedup work per-event-per-rank.

**Depth semantics.** `depth` increments on **every chain hop**, including hops over inactive uplines. This is a DoS guard: an attacker who creates a million-deep chain of dormant accounts cannot force the canister to walk it all — the loop bails at `CASCADE_DEPTH_CAP` (10) regardless of how many active payouts occurred. `activeRank` is a separate counter used only for bucket attribution and memo tagging.

### Helpers

```motoko
// The v2-swap point. v1 returns chain.get(current) ?? Charles.
// v2 will swap implementation to NFT-ownership lookup.
func getPayoutTarget(current : Principal) : Principal {
    switch (principalMap.get(referralChain, current)) {
        case (?p) { p };
        case (null) { house() };
    };
};

// Resolves housePrincipal with a fallback to the canister admin
// (defensive — should always be ?Some after init).
func house() : Principal {
    switch (housePrincipal) {
        case (?p) { p };
        case (null) {
            switch (adminPrincipal) {
                case (?p) { p };
                case (null) { Debug.trap("housePrincipal not initialized") };
            };
        };
    };
};

// Active = meets the configured deposit criterion.
func isActive(p : Principal) : Bool {
    if (not mintConfig.activityRequiresDeposit) { return true };
    switch (principalMap.get(lastQualifyingDeposit, p)) {
        case (null) { false };
        case (?ts) {
            switch (mintConfig.activityWindowDays) {
                case (null) { true };  // lifetime
                case (?days) {
                    let windowNs = Int.abs(Time.now() - ts);
                    windowNs <= days * 86_400 * 1_000_000_000;
                };
            };
        };
    };
};
```

### Observer integration

The observer is already polling `ponzi_math.getAllGames()` and `getBackerPositions()` per tick. Existing call sites at [shenanigans/main.mo:500](../../../shenanigans/main.mo:500) and [shenanigans/main.mo:528](../../../shenanigans/main.mo:528) change as follows:

```motoko
// processNewGames (replaces existing logic):
let ppPerIcp = ... // existing plan lookup
let baseUnits = icpFloatToPpUnits(game.amount, ppPerIcp);
let playerNet = baseUnits * (10_000 - mintConfig.cascadeInitialBps) / 10_000;
let cascadeUnits = baseUnits - playerNet;
let eventId = "game-" # Nat.toText(game.id);

// First-time depositor? mint signup gift FIRST (also goes through deductive split).
if (mintConfig.signupGiftPp > 0 and principalMap.get(signupGiftClaimed, game.player) == null) {
    let giftBase = mintConfig.signupGiftPp * PP_DECIMALS;  // (or appropriate unit conversion)
    let giftNet = giftBase * (10_000 - mintConfig.cascadeInitialBps) / 10_000;
    let giftCascade = giftBase - giftNet;
    let giftEventId = "signup-" # Principal.toText(game.player);
    switch (await mintInternal(game.player, giftNet, giftEventId)) {
        case (#Ok(_)) {
            signupGiftClaimed := principalMap.put(signupGiftClaimed, game.player, Time.now());
            await distributeDeductiveCascade(game.player, giftCascade, giftEventId);
        };
        case (#Err(_)) {};
    };
};

// Update lastQualifyingDeposit (used by isActive).
if (game.amount >= 0.1) {
    lastQualifyingDeposit := principalMap.put(lastQualifyingDeposit, game.player, Time.now());
};

// Plan PP mint (player net + cascade).
switch (await mintInternal(game.player, playerNet, eventId)) {
    case (#Ok(_)) {
        await distributeDeductiveCascade(game.player, cascadeUnits, eventId);
        gameIdCursor := game.id + 1;
    };
    case (#Err(msg)) { Debug.print("Mint failed: " # msg); return };
};
```

`processBackerDeltas` follows the same shape: split the delta mint into `playerNet + cascadeUnits`, mint net to backer, call `distributeDeductiveCascade`. Update `lastQualifyingDeposit` on every qualifying backer delta.

## Signup gift

- Triggered on the player's first appearance in `processNewGames` (gated by `signupGiftClaimed`).
- Amount: `mintConfig.signupGiftPp` (default 500 PP). Set to 0 to disable.
- Cascade applies: 90% net to player, 10% up the chain via `distributeDeductiveCascade`.
- Idempotent: the `signupGiftClaimed` map prevents double-claims; observer dedup on `game.id` provides additional protection.
- Claim timestamp doubles as the player's "join time" for the toast/badge feature.

## Toast & tab badge

### Backend surface

`getReferralStats` return shape gains one field:

```motoko
public type SignupEntry = {
    principal : Principal;
    joinedAt : Int;      // ns
    level : Nat;         // 1, 2, or 3 (only L1/L2/L3 downlines reported)
};

public type ReferralStats = {
    // ... existing l1Count/l2Count/l3Count/l1Units/l2Units/l3Units ...
    recentSignups : [SignupEntry];   // L1/L2/L3 downlines only, sorted joinedAt desc, capped at 20
};
```

`recentSignups` is computed at query time by walking `referrerToDownline` for the caller (L1 directly, L2 via `chain.get` on each L1 downline, L3 similarly), looking up `signupGiftClaimed[principal]` for `joinedAt`, sorting desc by `joinedAt`, taking the first 20.

### Frontend contract

The backend is stateless about "read" status; the frontend handles it.

- On MLM tab mount + periodic refetch, call `useGetReferralStats`.
- Read `localStorage.mc_lastSignupSeenAt_<userPrincipal>` (default `0`).
- For each `recentSignups[i].joinedAt > lastSeen`: queue a toast.
- Badge count = number of entries with `joinedAt > lastSeen`.
- When user opens the MLM tab, set `lastSeen = now` to clear badge.

Toast wording (from `PONZI_POINTS_REDESIGN.md` line 20): "You've grown your downline!" with subtext "You've added a new member to your organization."

## Admin endpoints

```motoko
public shared ({ caller }) func setHousePrincipal(p : Principal) : async () { requireAdmin(caller); housePrincipal := ?p };
public shared ({ caller }) func setCascadeBps(initial : Nat, passthrough : Nat) : async () { requireAdmin(caller); /* validate 0..10_000; update mintConfig */ };
public shared ({ caller }) func setSignupGiftPp(v : Nat) : async () { requireAdmin(caller); mintConfig := { mintConfig with signupGiftPp = v } };
public shared ({ caller }) func setActivityRequiresDeposit(b : Bool) : async () { requireAdmin(caller); mintConfig := { mintConfig with activityRequiresDeposit = b } };
public shared ({ caller }) func setActivityWindowDays(d : ?Nat) : async () { requireAdmin(caller); mintConfig := { mintConfig with activityWindowDays = d } };
```

All five validate input where applicable (BPS values in [0, 10_000]; days within sane bounds).

## Migration / upgrade behavior

The upgrade is non-trivial — it changes both data shape and mint math. Order matters.

### State migrations

1. **`MintConfig` extension**: add the 5 new fields with defaults (`cascadeInitialBps = 1000`, `cascadePassthroughBps = 5000`, `signupGiftPp = 500`, `activityRequiresDeposit = true`, `activityWindowDays = null`). Old `referralL[1-3]Bps` values are preserved untouched.
2. **`housePrincipal` initialization**: at upgrade time, if `null`, set to the canister's admin principal. Existing admin tooling continues to work.
3. **`signupGiftClaimed` grandfathering**: walk `ponzi_math.getAllGames()`; for every distinct player, record `signupGiftClaimed[player] = earliestGameRecord.timestamp`. This prevents existing players from getting a retroactive signup gift on their next deposit AND gives accurate join times for the toast/badge feature.
4. **`lastQualifyingDeposit` seeding**: walk `ponzi_math.getAllGames()` and `getBackerPositions()`; for each player with ≥0.1 ICP cumulative deposit, set `lastQualifyingDeposit[player] = latestQualifyingEventTime`. Ensures `isActive()` returns true for existing depositors immediately.
5. **`referrerToDownline` backfill**: iterate `referralChain` once at upgrade; for each `(downline, referrer)` entry, prepend `downline` to `referrerToDownline[referrer]`. Already-declared map at [shenanigans/main.mo:270](../../../shenanigans/main.mo:270) gets populated.

These migrations run in a one-shot upgrade-time function (e.g., `Migration.runV2`) invoked from `postupgrade`. Each step is idempotent (re-running is safe) in case of partial upgrade.

### Historic earnings handling

`referralEarnings` accumulator is **not wiped**. Pre-upgrade additive-cascade payouts stay in the L1/L2/L3 buckets. Post-upgrade deductive-cascade payouts add on top. Players see cumulative totals that span both regimes. Acceptable because the user-facing concept ("total PP I've earned from my downline") hasn't changed.

### Trigger semantics during upgrade

The observer state vars (`gameIdCursor`, `backerSeen`) are preserved. Mid-tick interruption is safe: observer resumes from `gameIdCursor` after upgrade, re-processing any games not yet credited. The new deductive math applies from the first post-upgrade tick onward. Already-credited games (`game.id < gameIdCursor` at upgrade time) are NOT re-cascaded — they already paid under the additive model.

### Rollback considerations

If the cascade math behaves unexpectedly, the admin can:
- Set `cascadeInitialBps = 0` → no deduction; the deductive cascade becomes a no-op
- Set `signupGiftPp = 0` → no signup gift minted
- These are live config changes; no upgrade required

A code-level rollback (revert canister upgrade) would restore the additive cascade. The new state fields would be ignored by the old code (Motoko's enhanced-orthogonal-persistence trims unknown fields). Acceptable risk for the planned upgrade.

## Open items deferred to implementation

These are intentionally left to the implementation plan (writing-plans skill) rather than re-litigated here:

- Exact unit conversion at the signup-gift mint (`signupGiftPp` is "whole PP"; need to multiply by `PP_DECIMALS` like other mint sites).
- Implementation of `Migration.runV2` — whether the existing migration framework lives in `shenanigans/migration.mo` or warrants new infrastructure.
- Memo length validation for cascade memos (`"cascade-A{N}-{eventId}"` — ensure under the ledger's memo size limit).
- Test scenarios for the smoke test phase: cycle detection, depth cap, all-inactive chain, single-active-at-depth-5, etc.
- Frontend wiring (toast component, badge rendering, localStorage key namespacing) — separate UI session per the redesign doc.

## Out of scope

Per `PONZI_POINTS_REDESIGN.md`, deferred from v1:

- Shenanigan effect implementations (Money Trickster, AOE Skim, Purse Cutter, Whale Rebalance, Magic Mirror math). Separate phase.
- Rubber-banding modifier on hostile shenanigans. Separate phase.
- Public shenanigan feed / leaderboard / rank titles / bounties / insurance. Confirmed pile, not scheduled.
- v2 NFT contracts + Harberger tax + marketplace. v2 starts after v1 ships and player behavior is observed. `getPayoutTarget` is the swap-point.

## Acceptance criteria

This design is implemented correctly when:

1. Every `mintInternal` call on the observer hot path (game, backer, signup) is preceded by a `distributeDeductiveCascade` of `cascadeUnits = baseUnits * cascadeInitialBps / 10_000` and the player receives `playerNet = baseUnits - cascadeUnits`.
2. Total PP minted per observer event = `baseUnits` (conserved). Verified by summing `mintInternal` units across player + all cascade levels + residual.
3. A player with no `referralChain` entry triggers a full cascade to `housePrincipal`.
4. A cascade hitting an inactive upline skips them; the next active upline receives the cascade at the rate they would have received had the inactive position not existed.
5. `signupGiftClaimed` prevents duplicate signup-gift mints across observer restarts and upgrade cycles.
6. `getReferralStats(p)` returns `recentSignups` with accurate `joinedAt` times for L1/L2/L3 downlines.
7. Admin can disable the cascade live by setting `cascadeInitialBps = 0` without a canister upgrade.
8. Existing players post-upgrade do not retroactively receive a signup gift on their next deposit.
9. Cycle detection prevents infinite walks (verifiable by registering a chain that loops through Charles).
10. Depth cap of 10 is enforced; residual lands on `housePrincipal`.
