# Audit Scope

**Date:** 2026-06-03
**Commit:** 19ad41df7d91b4f7a8cf9492b84f64146d2275ef
**Branch:** feat/sol-self-serve-backer
**Auditor:** audit-icp-cdp skill (Motoko-adapted), 5 parallel specialist passes + reducer

## In scope

- `ponzi_math/main.mo` (1963 lines) — the PonziMath persistent actor: deposits (createGame),
  backer funding (addBackerMoney), simple-plan withdrawals (withdrawEarnings), compounding-plan
  settlement (settleCompoundingGame), backer repayment claims (claimBackerRepayment), cover-charge
  sweep (sweepCoverCharges), exit-toll distribution, round reset, Series B promotion, admin/test
  hatches, and god-view queries.
- `ponzi_math/ledger.mo` (193 lines) — ICP ICRC-1/2 ledger interface + helpers.
- `ponzi_math/icrc21.mo` (113 lines) — ICRC-21/28/10 consent + standards metadata.

## Out of scope

- The ICP ledger canister (ryjl3-tyaaa-aaaaa-aaaba-cai) internals — integration surface audited,
  internals assumed correct.
- The management canister raw_rand — assumed correct.
- Frontend code, backend/ canister, ponzi_math_sol, shenanigans (separate canisters).
- Real mainnet state — this is a source-code audit at the commit above.
- Economic assumptions about whether a ponzi "should" exist — the protocol is an intentional
  satirical ponzi; findings distinguish "working as designed (sleazy but intended)" from "bug".

## Platform notes

- This is Motoko, not Rust. The IC-platform rule pack was translated: "reentrancy" = message
  interleaving across awaits; trap = full single-message rollback; `transient` vars reset on upgrade.
- All internal money is stored as `Float` (ICP units), converted to e8s (Nat) only at ledger
  transfer boundaries via `Int.abs(Float.toInt(x * 1e8))` (truncation toward zero).
- The CDP-domain rule pack (oracle, liquidation, peg) largely does not apply; the financial-accounting
  and stability-pool-accounting analogues (toll distribution, backer repayment) were used instead.

## Solvency invariant (from scripts/verify-fee-invariant.sh)

```
actual_icp_e8s  ≈  floor(potBalance·1e8) + floor(roundSeedReserve·1e8)
                 + Σ floor(backerRepayments·1e8) + coverChargeBalance      (±10 e8s tolerance)
```

## Prior review

Commit a1607ab ("ponzi_math: security fixes (audit + 6 hardenings) #53", 2026-05-18) was a prior
code-level audit (admin-gating, global-lock in admin mutators, pagination, force-reset, fee-leak).
No `audit-reports/` artifacts existed before this run, so this is the first run of this harness
(not differential mode).

## Dirty files at audit time

`ponzi_math/main.mo` carries one uncommitted change made during this audit: the F-004 fix
(distributeExitToll orphaned-slice redirect). No other ponzi_math file is modified.
