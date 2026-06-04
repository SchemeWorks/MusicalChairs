# Shenanigan winnings burn (15% carry) — design

**Date:** 2026-06-04
**Branch:** `feat/shenanigan-winnings-burn` (off `main`)
**Canister:** `shenanigans` (`shenanigans/main.mo`)
**Status:** approved design, ready to implement

## Goal

> Every time a player gains PP from a successfully cast Shenanigan, 15% of their
> winnings should be burned.

A deflationary sink on spell winnings: skim 15% off what a successful caster
nets and burn it (reduce PP supply).

## Decisions (locked with the user)

1. **Scope = caster's winnings only.** Burn 15% of what the *caster* nets — their
   steals and their own minted gain. Third-party gains stay untouched:
   `stimulusCheck`'s payouts to other holders and `slushFund`'s gift to its
   target are *not* burned. This falls out for free because those never touch
   `ppDeltaCaster`.
2. **Both steals and mints count.** Whether the caster's gain came from stealing
   existing PP (zero-sum PvP) or from a fresh mint, the 15% applies. Stolen wins
   therefore become slightly negative-sum (15% destroyed); minted wins net 85%.
3. **Silent net, backend-only.** Burn the 15% and report the *net* win. No new
   candid fields, no migration, no frontend work — every display
   (`Shenanigans.tsx` outcome toast, `SpellRow.tsx`, `LiveFeedPanel.tsx`) already
   reads `ppDeltaCaster`/`ppDelta`, so they show the post-burn number
   automatically.
4. **Not credited to the burn leaderboard.** The cut still reduces total PP
   supply, but is deliberately *not* added to `ppBurnedPerPlayer` /
   `ppBurnedPerPlayerPerRound` (those track cast-cost + karma burns only, and
   feed `getTopPpBurners` / `getPpBurnedFor` / `getRoundBurnedLeaderboard`).

## Where PP is gained on a successful cast

`applySuccessEffect` is called from exactly one place — line 3077 inside
`castShenanigan`. `echo`/`confettiCannon` only set buff deadlines; they don't
replay casts. So a single chokepoint in `castShenanigan` catches every
PP-gaining success path. The caster's net gain is reported as `ppDeltaCaster`:

| Spell | Caster gross (`ppDeltaCaster`) | Mechanism |
|---|---|---|
| `moneyTrickster` | amount stolen | `chipTransfer` from target |
| `aoeSkim` | total stolen | `chipTransfer` from many holders |
| `whaleRebalance` | total stolen | `chipTransfer` from top-3 |
| `tenderOffer` | amount (full target balance) | `chipTransfer` |
| `bearRaid` | `casterNet` (after its own excess-burn) | `chipTransfer` from holders |
| `stimulusCheck` | `casterMinted` (caster's mint only) | `mintInternal` |
| everything else | `0` | buffs / cosmetics / `slushFund` gift |

All other arms return `ppDeltaCaster = 0`, so they are no-ops under the burn.
`burnFrom` is a true burn — it transfers to the ledger's minting account, which
reduces supply (already used for the cast cost and `purseCutter`).

## Approach

Single post-effect chokepoint in `castShenanigan` (recommended over editing each
of the 6 gain arms — no accuracy gain, more churn, easy to miss a future spell;
and over burning inside `chipTransfer`/`mintInternal`, which also serve cascades,
deposits and signup gifts).

## Implementation

All in `shenanigans/main.mo`. No candid/migration/frontend changes.

**1. Constant.** Add near the other top-level caps (`CASCADE_DEPTH_CAP` /
`AOE_MAX_VICTIMS`, ~line 1005–1014):

```motoko
// Winnings burn: % of a successful caster's net winnings that is burned
// (carried interest). Tunable here; promoting to MintConfig would require a
// migration, so it stays a compile-time constant for now.
transient let WINNINGS_BURN_BPS : Nat = 1_500; // 15%
```

**2. Burn + net the delta.** In `castShenanigan`, immediately after the
`try { ... } catch { ... }` block that computes `detail` (after line 3093) and
before the `actualCostFloat` / record build (line 3095):

```motoko
// Winnings burn (carried interest): on a successful cast, skim 15% of the
// caster's net winnings and burn it (→ ledger minting account, reduces PP
// supply). Only positive caster gains qualify — fails, backfires, and
// zero-delta buffs are untouched. The reported delta is netted so the toast
// and live feed show what the caster actually kept. Deliberately NOT added to
// ppBurnedPerPlayer / the burn leaderboards (distinct from cast-cost & karma
// burns). On burn failure (caster drained by a concurrent inbound cast during
// the await, or ledger briefly unavailable) we log and keep the gross delta so
// the reported number always matches reality — no trap; the in-flight lock is
// still released below. burnFrom catches internally and never throws, so it
// cannot strand the lock.
var casterDelta : Int = detail.ppDeltaCaster;
if (outcome == #success and detail.ppDeltaCaster > 0) {
    let cutUnits : Nat = Int.abs(detail.ppDeltaCaster) * WINNINGS_BURN_BPS / 10_000;
    if (cutUnits > 0) {
        switch (await burnFrom(caller, cutUnits, "winnings-burn-" # Nat.toText(castId))) {
            case (#Ok(_)) { casterDelta := detail.ppDeltaCaster - cutUnits };
            case (#Err(msg)) {
                Debug.print("winnings-burn failed for cast " # Nat.toText(castId) # ": " # msg);
            };
        };
    };
};
```

**3. Use the netted delta.** Replace `detail.ppDeltaCaster` with `casterDelta` at
the three consumption sites:
- record `ppDelta = ?detail.ppDeltaCaster` (line 3106)
- chat `#spellCast` `ppDelta = ?detail.ppDeltaCaster` (line 3121)
- return `ppDeltaCaster = detail.ppDeltaCaster` (line 3161)

**4. Docs.** Update the `ppDeltaCaster` (lines 82–84) and `ppDelta` (lines
105–107) doc comments to note the 15% winnings burn, and add a one-line entry to
`shenanigans/TUNING_NOTES.md`.

## Edge cases / error handling

- Only `#success` with `ppDeltaCaster > 0` triggers a burn. Fails, backfires, and
  zero-delta buffs (rename, siphon, shields, multipliers, cosmetics, `slushFund`)
  are no-ops.
- Integer math truncates (`gross * 1500 / 10_000`); at the 10^8 unit scale the
  rounding is negligible and truncation favors the player. `Nat` is unbounded —
  no overflow.
- The caster always holds ≥ the cut at the moment of the burn (they just gained
  `ppDeltaCaster`), unless a concurrent *inbound* hostile cast drains them in the
  `await` window. If `burnFrom` returns `#Err`, we log and keep the gross delta —
  conservative (under-burn rather than misreport), no trap.
- `bearRaid` already burns its own excess above `casterGain`; the 15% then
  applies to `casterNet` (what the caster keeps). Consistent.

## Out of scope (explicit)

- Third-party gains: `stimulusCheck` payouts to other holders, `slushFund` target
  gift. Untouched.
- Passive/delayed buffs that pay out later through normal mint paths
  (`ppBoosterAura` / `foundersRound` / `insiderTip` mint multipliers,
  `mintTaxSiphon` redirects, `downlineBoost` cascade boosts). These are ongoing
  effects, not cast winnings — the 15% does not apply.
- `ppBurnedPerPlayer` / burn leaderboards. Unchanged.
- Frontend, candid, and migration. None.

## Verification

- `dfx build shenanigans --check` compiles clean (baseline on `main` already
  passes, modulo the pre-existing M0155 warning at line 3622 in `bearRaid`).
- Reason through two cases: a `moneyTrickster` (transfer) and a `stimulusCheck`
  (mint) — confirm caster nets 85% of gross, supply drops by the cut for the
  mint case, and the reported `ppDelta` equals the net.
- Optional live-replica smoke test (deferred, needs the local ppLedger+replica
  harness with a funded `caster`): cast a gain spell, read `ppDeltaCaster` off
  `getRecentShenanigans`, assert it equals 85% of the expected gross.

## Non-goals / not deploying

This branch stops at verified + committed code. No deploy (shenanigans deploys
from `main` via the migration recipe; deploys require explicit permission).
