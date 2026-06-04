# Audit Report: ponzi_math

**Date:** 2026-06-03
**Commit:** 19ad41df7d91b4f7a8cf9492b84f64146d2275ef (branch: feat/sol-self-serve-backer)
**Auditor:** audit-icp-cdp skill (Motoko-adapted), 5 parallel specialist passes + reducer
**Prior audit:** first run of this harness; prior code-level hardening in commit a1607ab (#53)

## Summary

- Total findings: 18 (3 High/Medium-High, 5 Medium, 8 Low, 2 Informational)
- Critical: 0, High: 2, Medium-High: 1, Medium: 4, Low: 8, Informational: 3
- One Medium finding (F-004) is FIXED in the working tree during this audit.
- The concurrency model, rollback discipline, insolvency scaling, rounding direction, rate-limit,
  round-reset carryover, and stable-memory model were all examined and confirmed SAFE.

### Remediation applied (working tree, post-audit, NOT deployed)

All changes below compile clean (`dfx build ponzi_math --check`, 570 KB wasm, zero errors; only
pre-existing M0155/M0194 warnings remain).

- **F-004 (Medium)** FIXED. distributeExitToll routes the orphaned senior slice to roundSeedReserve.
- **F-002 (Medium-High)** IMPLEMENTED. Added a stable `activeGameIds` index + `activeGameIndexBackfilled`
  flag; triggerGameReset and selectPromotionCandidate now iterate the index via a drift-guarded
  `activeGamesSnapshot()` (falls back to a full scan if the index size disagrees with the activeGames
  counter, so correctness never depends on the index). Maintained in createGame/createBackdatedGame
  (add), withdraw/settle (delete on close, restored on rollback). Lazy one-time backfill from gameRecords.
- **F-009 (Low)** IMPLEMENTED. Added `liveIcpFee()` (queries icrc1_fee, falls back to 10_000); withdraw,
  settle, claim, and sweepCoverCharges now use the live fee and pass `fee = ?icpFee`. (adminSweepUntracked
  left unchanged — it is a pre-blackhole test hatch slated for deletion.)
- **F-011 (Low)** IMPLEMENTED. Added `getAllGamesPage(offset, limit)` mirroring getGeneralLedgerPage.
  Requires a frontend declarations sync (the candid interface gained a method).
- **F-013 (Low)** IMPLEMENTED. BACKEND_PRINCIPAL/TEST_ADMIN now resolve from stable `storedBackend`/
  `storedTestAdmin`, pinned on first init; later upgrades ignore the constructor args, so a stale
  `--argument` cannot silently rotate privileged principals.
- **F-010 (Low)** NOT IMPLEMENTED, deliberately. Adding `created_at_time` to transfers would introduce
  CreatedInFuture/TooOld rejection risk on the withdraw path from cross-subnet clock skew, for near-zero
  benefit: the caller+global lock already blocks rapid double-submits, and a lost-reply manual retry would
  carry a different timestamp (so it would not dedup anyway). Kept as a documented operational note
  (do not blind-retry; re-read state).

RUNTIME-VALIDATED (local dfx replica + local ICP ledger at ryjl3-tyaaa-aaaaa-aaaba-cai, 2026-06-03):
- Old (HEAD) → new upgrade: no trap, no brick; roundId + platformStats survived (additive stable-var
  migration is sound).
- F-013: upgrading with a deliberately wrong testAdmin arg was IGNORED (original principal stayed admin).
- F-011: getAllGamesPage returns correctly.
- F-002 lifecycle: 4 games created (incremental index add), one matured game withdrawn (closed + removed
  from index, no spurious reset), then adminForceReset closed the remaining games via the drift-guarded
  index; activeGames went 4 -> 3 -> 0, full history preserved (total stayed 4).
- F-009: withdrawal queried the live icrc1_fee (10_000) and paid the exact net (224_060_000 e8s).
- F-004 (the original bug scenario): set up a Series B backer with ZERO Series A (via an insolvency
  promotion), then drove a 0.0693 ICP toll through distributeExitToll. Result: seed 0.05544 + backer
  0.01386 = 0.0693, dropped value = 0.00000000. Before the fix the orphaned 0.02079 would have vanished.

The migration paths are now both compile- and runtime-verified. A mainnet upgrade should still re-run the
ponzi_math/scripts/verify-*.sh invariant checks as standard practice.

The protocol is an intentional satirical ponzi, so "a later player's deposit funds an earlier player's
withdrawal" is the design, not a bug. Findings are framed against two things that are NOT supposed to be
true: (1) the canister must stay solvent against its own internal accounting (the verify-fee-invariant
relationship), and (2) it is meant to become trustless after blackhole. The real issues cluster around
the pre-blackhole admin hatch surface and one accounting leak.

## Scope

**In scope:** ponzi_math/main.mo (1963 LoC), ledger.mo (193), icrc21.mo (113). Entry points: createGame,
addBackerMoney, withdrawEarnings, settleCompoundingGame, claimBackerRepayment, sweepCoverCharges, the
TEST_ADMIN hatch block, and the admin god-view queries.

**Out of scope:** ICP ledger internals, raw_rand, frontend, other canisters (backend/ponzi_math_sol/
shenanigans), real mainnet state, the philosophical legitimacy of a ponzi.

## High findings

### F-001: createBackdatedGame lets TEST_ADMIN mint a matured compounding position and drain the pot

`createBackdatedGame` ([main.mo:1706](ponzi_math/main.mo#L1706)) is gated only on `caller == TEST_ADMIN`
and accepts an arbitrary past `startTimeNanos`, setting both `startTime` and `lastUpdateTime` to it, so
the position is born already matured. TEST_ADMIN funds it with their own `transfer_from`, then calls
`settleCompoundingGame` to extract compounded earnings paid from the SHARED pot. A 30-day compounding
position pays ~12.27x gross; net extraction is ~+9.67 ICP per 1 ICP of admin capital, bounded only by
the pot. On the insolvency path the payout scales to the entire pot.

This is an intended PRE-BLACKHOLE TEST HATCH (the block is explicitly labeled "DELETE THIS ENTIRE BLOCK
BEFORE BLACKHOLING"). It is HIGH against the stated trustless-after-blackhole goal: if the block survives
blackhole, TEST_ADMIN keeps an unlimited pot-drain forever. Pre-blackhole it is an operator-custody risk.

Recommendation: delete the hatch block (lines 1699-1946) and redeploy before removing controllers; verify
the deployed module hash contains none of these methods; treat TEST_ADMIN as a discardable key meanwhile;
consider a compile-time flag so a forgotten deletion cannot ship.

### F-003: adminClearAllBackerPositions + adminSweepUntracked converts backer-owed ICP into an admin withdrawal

`adminSweepUntracked` ([main.mo:1882](ponzi_math/main.mo#L1882)) pays `actual - internal - fee` to
TEST_ADMIN, where `internal` includes `sum(backerRepayments)`. `adminClearAllBackerPositions`
([main.mo:1863](ponzi_math/main.mo#L1863)) zeroes `backerRepayments` without paying those users and
without reducing `potBalance`. After the clear, the formerly-owed balances drop out of `internal`, so the
sweep pays them to TEST_ADMIN. The sweep records no ledger event, so the movement is unauditable.

Recommendation: delete with the hatch block before blackhole. Independently, adminSweepUntracked should
refuse to run while any backerRepayments are non-zero and must recordLedger; a genuine reconciliation
hatch should route proceeds back to the pot, not to the admin.

## Medium-High findings

### F-002: Round-reset / promotion paths iterate ALL historical games and can eventually trap core flows

`gameRecords` is never pruned (nextGameId is monotonic; triggerGameReset only flips isActive=false).
`triggerGameReset` ([main.mo:631](ponzi_math/main.mo#L631)) and `selectPromotionCandidate`
([main.mo:520](ponzi_math/main.mo#L520)) iterate every game (active + all historical) and run on the
insolvency/pot-empty path inside withdrawEarnings/settleCompoundingGame. As N_games grows toward the ~5B
instruction limit, the reset traps, so the triggering payout traps, the round never closes, and payouts
freeze. `adminForceReset` hits the same loop, so the manual recovery hatch fails too. Because the canister
is meant to be blackholed (no future upgrade), this is UNFIXABLE post-blackhole.

Reaching the threshold needs ~10^5-10^6 games (long operation or expensive sybil), hence Medium-High not
High, but it must be fixed before blackhole.

Recommendation: maintain a stable index of ACTIVE game ids (e.g. OrderedSet<Nat> updated on create/close)
and iterate only that in the reset/promotion paths; keep getAllGames reading full history. Backfill the
index once on the introducing upgrade.

## Medium findings

### F-004: distributeExitToll dropped the oldest-Series-A slice when no Series A backer exists  [FIXED in working tree]

When the backer set has >=1 backer but no Series A (e.g. only Series B promotion positions), `oldestBacker`
stays null, the `case (null) {}` arm credited nobody, yet `toOldest` (60% of the backer half = 30% of the
toll) was already carved out. That value was deducted from the pot but credited to no internal field,
becoming untracked balance and breaking the solvency invariant by ~30% of the toll (far above the 10 e8s
tolerance). Two independent passes plus the lead reviewer converged on this.

Fixed at [main.mo:483](ponzi_math/main.mo#L483): the null arm now routes the orphaned slice to
`roundSeedReserve` (the same tracked sink the zero-backers branch uses), and the returned
TollDistributionDetails reflects the actual credit. Conservation now holds in every backer-set composition
(verified by trace; compiles clean via `dfx build ponzi_math --check`). See findings.json for the table.

### F-005: Compounding plans are intentionally uncapped (by design, reclassified to Informational)

The `Float.max(potBalance*0.2, 5.0)` cap runs only inside `if (not isCompounding)`
([main.mo:759](ponzi_math/main.mo#L759)); compounding deposits are uncapped. Initially flagged as a
possible oversight, then confirmed BY DESIGN by the protocol owner (2026-06-03). The asymmetry is
principled: simple plans withdraw continuously and can drain the pot fast, so they are capped; compounding
plans are maturity-locked (no early exit), so a compounding deposit's net enters the pot immediately as
exit liquidity for everyone else while the depositor waits until maturity. A late, large compounding
position is a likely-losing bet that primarily funds earlier players, so capping it would reduce the exit
liquidity that benefits earlier entrants. No change. (Residual: a whale who can market-time inflows and
front-run maturities has a speculative edge, but that is gambling against the house, not a defect.)

### F-006 / F-007: Backer entitlement is cosmetic, and backer positions are a perpetual toll claim

`entitlement` is never read by any payout path; backers are paid the uncapped toll split via
`backerRepayments`, while `getTotalBackerDebt` presents the static entitlement to users as real debt
(F-006). Positions persist across rounds (triggerGameReset does not clear them), so the first-ever backer
seizes the "oldest Series A" rank for 0.1 ICP and collects 35-60% of every future toll's backer half
indefinitely (F-007). Both may be "sleazy but intended"; they need a product decision, not necessarily a
code change. At minimum, do not present entitlement as outstanding debt.

### F-008: adminSweepUntracked pays rounding dust / drift to the admin instead of the pot

Float-vs-integer reconciliation leaves small positive discrepancies that morally belong to users; the
sweep monetizes them to TEST_ADMIN. Remove with the hatch block; if kept, route to the pot and add a
ledger event.

## Low findings

- **F-009** Hardcoded ICP_TRANSFER_FEE=10_000 with fee=null desyncs if the ledger fee changes. Query
  icrc1_fee() (already in the interface) instead. Matters more because the canister will be blackholed and
  cannot be patched later.
- **F-010** created_at_time=null on all transfers: a lost reply after a committed payout enables a
  caller-driven double-pay (theoretical, bounded). Add deterministic created_at_time + memo; do not
  roll back on SysTransient.
- **F-011** Unbounded full-collection queries (getGeneralLedger, getAllGames, ...) can exceed the query
  reply limit as history grows. Paginate (getGeneralLedgerPage already exists for the ledger).
- **F-012** GeneralLedgerEvent variant arms are append-only across upgrades. When deleting the hatch block,
  KEEP the #backdatedGameCreated arm or the upgrade traps on decode of historical entries.
- **F-013** TEST_ADMIN/BACKEND_PRINCIPAL are transient init args re-supplied each upgrade; a wrong arg
  silently rotates privileged principals. Persist on first init or assert-on-change.
- **F-014** Series B promotion farming (small, bounded). Optionally exclude existing backers from the
  phase-2 fallback.
- **F-015** Exit-toll tier is a single age snapshot on the whole gross, so patient players pay the 3%
  floor. Confirm intent; plausibly intended.
- **F-016** adminForceReset / adminClearAllBackerPositions can destroy user entitlements without moving
  funds (griefing; sweepable via F-003). Remove with the hatch block.

## Informational

- **F-017** A null-firstDepositDate Series A would have dropped its slice too; unreachable today and now
  neutralized by the F-004 fix.
- **F-018** Pre-existing M0155 "operator may trap" warnings on guarded Nat subtractions; unused Nat64
  import. No behavior change; optional hygiene.
- ADMIN_PRINCIPALS confers read-only god-view only; no allowlist member can move funds (only TEST_ADMIN
  and BACKEND_PRINCIPAL can). Confirmed correct separation.
- Pre-blackhole, any controller can reinstall and bypass every guard; trustlessness depends on removing
  controllers AND deleting the hatch block.

## What was examined and confirmed SAFE

- **Concurrency:** all 11 state-mutating methods acquire the global lock before their first await; no
  field is mutated without it. Lock-ordering trap pre-await does not leak the caller lock (trap = full
  rollback). Transient locks are the correct choice (a stable lock would wedge the canister post-upgrade).
- **Rollback:** withdraw/settle/claim/sweep snapshot and restore exactly the mutated fields on both the
  catch and the #Err arms; distributeExitToll's roundSeedReserve/backerRepayments are restored; no ledger
  entry is written before a transfer, so no phantom/double records; no reachable trap after a successful
  transfer.
- **Insolvency scaling:** no divide-by-zero (guarded by the pot<=0 early return; isInsolvent implies
  earnings>pot>0); pot cannot go negative; payout + toll never exceeds pot.
- **Rounding direction:** every payout truncates toward zero, so the canister never sends more e8s than the
  Float liability it decrements; the network fee is always absorbed by the payee.
- **Cover charge:** per-deposit reconciliation gap is +0..2 e8s (untracked surplus, never a deficit);
  addBackerMoney correctly does not skim; coverChargeBalance is decremented only in sweepCoverCharges.
- **Rate limit:** no same-principal bypass (timestamp recorded only on success; caller lock serializes).
- **Round reset / closed-game reuse:** nextGameId never resets; closed games are rejected by the isActive
  guards; seed is moved into the pot exactly once.
- **Auth:** requireAuthenticated rejects the anonymous principal as the first statement of every user
  update; no fund-moving or state-mutating method is anonymous-callable.
- **Stable memory:** every field that must survive upgrade is non-transient; the OrderedMap idiom (transient
  comparator, stable data) is correct; no heap-only field holds user value.

## Methodology

- Passes executed (parallel sub-agents): (1) async state races / trap-rollback / concurrency,
  (2) Float precision / accounting / toll distribution, (3) access control / admin surface /
  blackhole-readiness, (4) economic / game-logic invariants, (5) upgrade-safety / ICRC hygiene / DoS.
- Rule-pack sections loaded: IC-platform 1-10 (async races, inter-canister failure, stable memory,
  cycle DoS, caller auth, controller-vs-admin, ICRC hygiene, query/update, timers, trap/rollback).
  CDP-domain: financial-accounting and stability-pool analogues only (oracle/liquidation/peg N/A).
- Reducer (lead): independent full read of all three files; deduped 5 reports; recalibrated severities;
  verified F-004 root cause against source and applied + compiled the fix.
- Sub-agents dispatched: 5. Build verification: `dfx build ponzi_math --check` (passes; only pre-existing
  warnings).

## Limitations

- This is a source audit at the commit above, not of live mainnet state.
- No PocketIC/Motoko test harness exists in the repo; PoCs are reasoning traces with numeric examples,
  not executed tests. The verify-fee-invariant.sh script would empirically catch F-004 on a live canister.
- The ICP ledger, raw_rand, and IC platform crypto are assumed correct.
- Economic findings (F-005..F-007, F-015) depend on product intent for the satirical ponzi; they are
  flagged for a design decision, not asserted as defects.
- Controller list / key custody were not inspected (out of source scope); F-001/F-003/F-013 assume the
  documented blackhole plan.

## New findings

F-001 through F-018 are all NEW (first run of this harness).
