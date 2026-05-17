# Shenanigans v1 — Spell Effects, Rubber-Banding, Docs

**Date:** 2026-05-16
**Scope:** Phases E, F, H of the (deleted) PONZI_POINTS_REDESIGN. Phase G (payoff caps) is intentionally deferred — values unknown.
**Canister touched:** `shenanigans` only. No `ponzi_math` changes.
**Deploy:** code only. User has explicit policy: never deploy backend without permission.

## Problem

The shenanigans canister has 11 spell types declared, with admin-tunable configs (cost, odds, `effectValues`). When a player casts, the cost is burned and the outcome is rolled, but only 3 spells have **backfire** handlers and **zero** have success handlers. Successful casts are no-ops. Rubber-banding (success-rate modifier based on caster vs target PP) is unimplemented.

## v1 Out of scope

- Phase G payoff cap updates — original values lost.
- Frontend UI for new state (shield indicators, active-buff display, golden-name styling on leaderboard). Backend exposes everything via queries; UI passes are separate.
- Caster-chosen rename text. Rename Spell picks from a hard-coded pool of 8 satirical names — `chooseRandomName(seed)`. Cheaper to ship; can wire a text input later.
- Frontend cooldown / cast-limit enforcement. Backend already trusts the client; v1 leaves it that way.

## Phase E — Spell effects

Add a `#success` branch to `castShenanigan`. Per-spell behavior:

| Id | Spell | Success effect | Backfire effect |
|----|-------|----------------|-----------------|
| 0 | Money Trickster | `chipTransfer(target → caller, rollPct(2,8) of targetBal, cap 250)` | existing (no change) |
| 1 | AOE Skim | for each player in `allChipHolders()` except caller and protection-floor: `chipTransfer(player → caller, rollPct(1,3) of theirBal, cap 60)` | existing |
| 2 | Rename Spell | `customDisplayNames[target] = (pickName(seed), now + 7d)` | `customDisplayNames[caster] = (pickName(seed), now + 7d)` |
| 3 | Mint Tax Siphon | `mintSiphons.put(target, {caster, untilTs = now+7d, pct = 5, capUnits = ppToUnits(1000), siphonedSoFar = 0})` | symmetric, target/caster swapped, half duration |
| 4 | Downline Heist | move one downline edge: pick `victim` such that `referralChain[victim] == target` (favor deepest), set `referralChain[victim] = caller` | swap roles — caster's deepest downline moves to target |
| 5 | Magic Mirror | `shieldsActive[caller] = {chargesRemaining = 1, expiresAt = now + 24h}` | n/a (success-only spell) |
| 6 | PP Booster Aura | `mintMultipliers[caller] = (rollPct(105,115), roundEndTs)` | n/a |
| 7 | Purse Cutter | `burnFrom(target, rollPct(25,50) of targetBal, cap 800)` | symmetric — burn caster |
| 8 | Whale Rebalance | top-3 by chip balance (excluding caster). For each: `chipTransfer(whale → caller, 20% of whaleBal, cap 300)` | symmetric — caster pays each top-3 |
| 9 | Downline Boost | `cascadeBoosts[caller] = (1.3, roundEndTs)` | n/a |
| 10 | Golden Name | `goldenUntil[caller] = now + 24h` | n/a |

### `rollPct(min, max)` helper

```motoko
func rollPct(min : Nat, max : Nat) : Nat {
  if (max <= min) return min;
  min + (Int.abs(Time.now()) % (max - min + 1));
};
```

Same randomness pattern as existing `determineOutcome`. Not cryptographic — acknowledged.

### Protection floor

`SHENANIGAN_PROTECTION_FLOOR = 200 PP`. Apply ONLY when a spell would **reduce** a target's balance (Money Trickster, AOE Skim, Mint Tax Siphon, Purse Cutter, Whale Rebalance, and shield-bypass cases). If target balance < 200 PP, success becomes a no-op for that target — caster's cost is still burned, outcome still recorded as `#success`.

### Shield interaction

Hostile spells (Money Trickster, AOE Skim if shielded player is in pool, Mint Tax Siphon, Purse Cutter, Whale Rebalance) check `shieldsActive[target]` before applying. If shield present and `chargesRemaining > 0` and unexpired: decrement charges, skip effect for that target, log `ShenaniganRecord` outcome as `#fail` (the spell didn't take). For AOE-style spells with multiple targets, shield only protects the shielded ones; the rest still take damage.

Downline Heist does NOT respect shields — it's a structural move, not a balance change.
Rename Spell does NOT respect shields — it's cosmetic.

### Mint Tax Siphon — runtime application

When the observer mints PP to a player, it checks `mintSiphons[player]`:
- if entry exists and not expired and `siphonedSoFar < capUnits`:
  - `siphonAmount = min(mintAmount * pct / 100, capUnits - siphonedSoFar)`
  - mint `mintAmount - siphonAmount` to the player
  - mint `siphonAmount` to the siphoner
  - update `siphonedSoFar`
- if expired or capped, remove the entry.

This means siphon hooks into the observer's `mintInternal` call sites — a small wrapper `mintWithSiphon(player, units, memo)` that wraps `mintInternal`.

### PP Booster Aura — runtime application

When the observer mints PP to a player, multiply by `mintMultipliers[player]` if present and `now < expiresAt`. Apply BEFORE the Mint Tax Siphon (so siphon takes a percentage of the boosted amount).

### Downline Boost — runtime application

When `cascadeReferralMint` runs and the upline at any level has an active `cascadeBoosts[upline]`, multiply that level's referral mint by 1.3. Per-level independent — each level's upline is checked separately.

### Golden Name — runtime application

Backend just records `goldenUntil[player]`. Adds query `getGoldenPlayers() : async [Principal]` returning currently-golden players. Frontend renders gold styling — out of scope here; backend just exposes the data.

## Phase F — Rubber-banding

Applied to **only** these spells:
- Money Trickster (0)
- AOE Skim (1)
- Mint Tax Siphon (3)
- Downline Heist (4)
- Purse Cutter (7)
- Whale Rebalance (8)

Pure-buff and 100%-success spells are exempt.

### Formula

```motoko
func rubberBandModifier(caster : Principal, target : ?Principal) : Int {
  let casterPp = chipBalanceSync(caster);  // assumes pre-cached at cast start
  let targetPp = switch (target) {
    case (?t) chipBalanceSync(t);
    case null avgChipBalance();  // for AOE / Whale, average of affected pool
  };
  if (targetPp == 0) return 0;
  // ratio scaled to thousandths: 1000 == parity
  let ratio = casterPp * 1000 / targetPp;
  // ratio < 1000 → underdog → positive modifier
  // ratio > 1000 → top dog → negative modifier
  // map (0..1000) → (+25..0), (1000..∞) → (0..-25), clamped
  if (ratio < 1000) {
    let bonus = (1000 - ratio) * 25 / 1000;
    if (bonus > 25) 25 else Int.fromNat(bonus)
  } else {
    let penalty = (ratio - 1000) * 25 / 1000;
    if (penalty > 25) -25 else -(Int.fromNat(penalty))
  }
};
```

### Application

`determineOutcome` becomes `determineOutcomeWithMod(type, modifierPct)`. The `successOdds` from config gets `+ modifierPct`, clamped to `[5, 95]`. The `backfireOdds` is left alone; `failureOdds` absorbs the delta.

Worked examples:
- caster 100 PP, target 1000 PP, Money Trickster (60% base): modifier = +22.5 → success roll < 82
- caster 1000 PP, target 100 PP, Money Trickster: modifier = -25 → success roll < 35 (clamped)
- caster 500 PP, target 500 PP: modifier = 0 → unchanged

## New stable state

```motoko
type DisplayNameOverride = { name : Text; expiresAt : Int };
type MintSiphon = { siphoner : Principal; expiresAt : Int; pctTimes100 : Nat; capUnits : Nat; siphonedSoFar : Nat };
type ShieldState = { chargesRemaining : Nat; expiresAt : Int };
type MintMultiplier = { multiplierBps : Nat; expiresAt : Int };  // 11500 = 1.15x
type CascadeBoost = { multiplierBps : Nat; expiresAt : Int };

var customDisplayNames = principalMap.empty<DisplayNameOverride>();
var mintSiphons = principalMap.empty<MintSiphon>();
var shieldsActive = principalMap.empty<ShieldState>();
var mintMultipliers = principalMap.empty<MintMultiplier>();
var cascadeBoosts = principalMap.empty<CascadeBoost>();
var goldenUntil = principalMap.empty<Int>();
```

All defaults are `empty`. Orthogonal persistence picks them up; no migration module change needed.

## Round-end cleanup

Spells with `roundEndTs` durations (Booster Aura, Downline Boost) need a hook on round reset. The shenanigans canister doesn't currently know about rounds — it observes ponzi_math games but not round boundaries. Simplest fix: store `expiresAt = now + 24h` for these instead of "until round end". Document the cap, move on. Acceptable for v1.

## New queries

```motoko
public query func getActiveSpellEffects(user : Principal) : async {
  shield : ?ShieldState;
  mintMultiplier : ?MintMultiplier;
  cascadeBoost : ?CascadeBoost;
  displayName : ?DisplayNameOverride;
  mintSiphon : ?MintSiphon;
  golden : Bool;
};

public query func getGoldenPlayers() : async [Principal];

public query func getCustomDisplayName(user : Principal) : async ?Text;  // null if expired
```

## Phase H — Docs delta

`frontend/src/components/DocsPage.tsx` already covers the 11 spells. Additions only:

1. New short subsection under "Shenanigans" header: **"How Spells Resolve"** — paragraph explaining rubber-banding (use the satirical voice — "level playing field for the little guy, mandatory haircut for whales"), the protection floor (already mentioned at line 280, expand it), shield mechanic.
2. Update the existing spell table rows for the 6 rubber-banded spells to add a footnote marker; add the footnote at the end of the section.
3. New row in glossary: **Shield**, **Mint Tax Siphon (active)**, **Boost (active)**.

No restructure. Less than 80 lines added.

## Testing plan

The repo has no automated test harness for shenanigans. Manual verification matrix:
- Cast each of 11 spells with a controlled-balance target. Verify outcomes match `effectValues`.
- Cast hostile spell with target below protection floor → no-op (target balance unchanged), caster cost burned, outcome `#success`.
- Cast hostile spell with target who cast Magic Mirror in the last 24h → outcome flipped to `#fail`, no balance change, shield charges decremented.
- Cast Mint Tax Siphon, then trigger observer mint to target → siphoner receives 5%, target receives 95%.
- Cast PP Booster Aura, then trigger observer mint → caller mints at 1.05-1.15x.
- Cast Downline Boost, then trigger cascade → upline cascade mints at 1.3x.
- Rubber-band: cast Money Trickster with caster_pp=100, target_pp=1000 → success rate ~82%. Inverse → ~35%.

Run on a `dfx start --background --clean` local replica with a seeded canister. No mainnet deploy.

## Implementation order

1. `rollPct`, `chipBalanceSync` (read-only cache populated at cast entry), new stable state declarations.
2. Spell success branch — Money Trickster, AOE Skim, Purse Cutter (chip-transfer based, simplest).
3. Whale Rebalance (needs top-3 leaderboard query — already exists as `getTopPpBurners` but for chip balance, need a new helper that sorts chip balances).
4. Mint Tax Siphon (observer hook).
5. Magic Mirror + shield interactions in 2–4 above.
6. PP Booster Aura + cascade boost (observer hook).
7. Rename Spell + Golden Name (simple state writes).
8. Downline Heist (referral chain mutation).
9. Rubber-band modifier — applied last so all 6 affected spells already exist.
10. New queries.
11. Docs delta.
12. Manual test pass.

## Open questions deferred

- **Phase G payoff caps**: TBD. Punted per user.
- **Rename text source**: hard-coded pool. Caster-chosen text is a follow-up.
- **Frontend buff/debuff UI**: separate session.
- **Cryptographic randomness for spell rolls**: uses `Time.now() % range`. Casino-grade enough for v1; if it becomes exploitable, swap for `Random.blob()`.
