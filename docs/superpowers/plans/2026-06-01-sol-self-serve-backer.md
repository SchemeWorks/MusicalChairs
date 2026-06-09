# Self-Serve SOL Series A Backing — scope + plan

**Goal:** Let SIWS/SOL users fund a Series A backer position by depositing real SOL through the app, mirroring ICP's public `addBackerMoney`. Today the only SOL path is admin-only `adminRegisterSeriesABacker`; the Seed Round "Fund the Project" button only works for ICP.

**Why it was missing:** SOL deposits are async (derive address → send → timer detects), unlike ICP's synchronous ICRC-2 pull. The backer path was never ported over the detection flow.

## Scope (verified against live code 2026-06-01)

Backend `ponzi_math_sol/main.mo` + frontend only. **No shenanigans change** — the observer already auto-mints backer PP from `getBackerPositions()` (shenanigans `main.mo:1468/1505`). **No migration** — additive: one new stable var (`pendingBackerIntents`) + one new type.

### Backend edits (`ponzi_math_sol/main.mo`)
1. New type `BackerIntent` + stable var `pendingBackerIntents` (parallel to `pendingBuyIntents` — the upgrade-safe pattern).
2. New const `MIN_BACKER_LAMPORTS = 50_000_000` (0.05 SOL — matches `adminRegisterSeriesABacker` min).
3. `prepareBackerDeposit({ expectedAmountLamports })` — public, mirrors `prepareSolDeposit` (auth, bootstrapped, 3/hr rate-limit, ensure deposit address, create BackerIntent).
4. `creditDeposit` — in the unmatched (`case null`) branch, match an open BackerIntent **before** buy intents. On match: create/merge `#seriesA` BackerPosition (entitlement = gross × 1.24), `potBalance += gross` (**no Front-End Load**, matching ICP `addBackerMoney` + the admin path), `recordLedger(#backerDeposit)`, rate-limit stamp, mark fulfilled, sweep, advance cursor.
5. `getMyPendingBackerIntents()` query (mirrors `getMyPendingIntents`).
6. `pokeMyDeposit` open-intent gate — add `pendingBackerIntents`.
7. `runDetectionForOpenIntents` — add `pendingBackerIntents` to the scan set (else timer won't auto-detect backer-only deposits).

### Frontend
- Seed Round page: when wallet is SIWS/SOL, route "Fund the Project" through the SOL deposit flow (`prepareBackerDeposit` → show address/amount → poke + poll `getMyPendingBackerIntents`/`getBackerPositions`) instead of `addBackerMoney`. Reuse the existing SOL deposit panel pattern.
- Declarations sync (`.did`/`.did.d.ts`/`.did.js`) for `ponzi_math_sol`.

### Deploy (authorized)
Upgrade `ponzi_math_sol` (additive, no migration) on the freshly-reinstalled EMPTY mainnet canister — safest possible moment for a stable-shape change. Then frontend. Smoke: self-serve a Series A backer deposit end-to-end (can double as the M3 seed).

## Accounting invariant
ICP `addBackerMoney`, SOL `adminRegisterSeriesABacker`, and new SOL self-serve all agree: entitlement = gross × 1.24, pot += gross, **no cover charge** on backer deposits.
