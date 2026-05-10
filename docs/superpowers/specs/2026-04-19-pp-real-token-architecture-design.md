# Ponzi Points as a Real ICRC-1 Token

**Date**: 2026-04-19
**Status**: Design approved; implementation plan pending

## Problem

Ponzi Points today are not a real token. They live as a `Map<Principal, Float>` inside the backend canister ([backend/main.mo:203](../../../backend/main.mo#L203)), mutated directly by backend and by the shenanigans canister (which calls backend's `deductPonziPoints` / `transferPonziPoints` / `burnPonziPoints`). The `pp_ledger` ICRC-1 canister referenced in [dfx.json](../../../dfx.json) has been deployed on mainnet with a genesis mint of 1,000,000 PP to the project owner's personal wallet, but nothing in the code reads or writes to it — it's vestigial.

This blocks the trust story the project is ultimately aiming for:

1. **The backend canister should be blackhole-able** — once feature-complete, its controllers should be removed so nobody (including the project owner) can rug players. For this to work, *everything* in the backend must either be immutable by design (game math, fee formulas, fund custody) or deliberately frozen at blackhole time.
2. **PP tokenomics must remain tunable** — mint rates, multipliers, referral cuts, and gameplay balance will need iteration based on observed player behavior. If PP minting lives inside the backend, blackholing freezes tokenomics too.
3. **PP should be tangible** — a real on-chain token players can see in their wallet, screenshot, transfer, and optionally trade. Floating-point balances in an internal map are prototype-grade and provide no externally verifiable record.

## Goal

Restructure PP as a first-class ICRC-1 token on `pp_ledger`, with all PP-related logic (minting, custody, gameplay, burns, leaderboard) living in a single mutable canister (`shenanigans`) that observes the immutable backend via polling. Backend has no awareness of PP post-migration.

## Non-goals

- No changes to ICP custody, ponzi math, fee structure, dealer logic, or any money-handling code in backend. This spec does not touch real-money flows.
- No migration of existing PP balances. All current PP holders are test accounts owned by the project owner, who has accepted a wipe on cutover.
- No change to the 1,000,000 PP genesis balance sitting in the owner's personal wallet. Left in place.
- No redesign of mint amounts or spell configs. Current rules are preserved 1:1 as starting values; tuning happens later via admin setters.
- No splitting of the observer and gameplay responsibilities into separate canisters. One mutable canister handles both (YAGNI on the split until there is a concrete reason).

## Architecture

Three canisters, clean responsibility split:

```
┌─────────────────────┐         ┌──────────────────────────┐
│  backend (mainnet)  │         │  pp_ledger (ICRC-1)      │
│                     │         │                          │
│  Ponzi + ICP logic  │         │  Minting account:        │
│  Blackhole-able.    │         │    shenanigans           │
│  Knows nothing      │         │  Controller: owner       │
│  about PP.          │         │  Decimals: 8             │
└──────────▲──────────┘         └────────────▲─────────────┘
           │                                 │
           │ polling                         │ icrc1_transfer
           │ (query-only)                    │ (mint/burn/move)
           │                                 │
           │        ┌────────────────────────┴────────┐
           └────────┤   shenanigans (mutable)          │
                    │                                  │
                    │   • Observer (polls backend)     │
                    │   • Mint engine                  │
                    │   • Chip custody (subaccounts)   │
                    │   • Spell logic                  │
                    │   • Leaderboard                  │
                    │   • Admin tunables + dashboard   │
                    └──────────────────────────────────┘
```

### backend

Blackhole-able eventually. Pure ICP / ponzi / money code. All PP-related methods (`getPonziPoints*`, `deductPonziPoints`, `transferPonziPoints`, `burnPonziPoints`, `getPonziPointsBalanceFor`, `distributeDealerCutFromShenanigans`, referral credit helpers, etc.) are removed. The `ponziPoints` and `ponziPointsBurned` maps are deleted. The `shenanigansPrincipal` setter is deleted (backend no longer needs to trust anyone to call PP methods, because there are no PP methods).

### pp_ledger

Re-initialized with:

- **Decimals: 8** (matches ICP/ckBTC convention; the current 0-decimal deployment is replaced)
- **Minting account: the shenanigans canister principal** (currently `j56tm-oaaaa-aaaac-qf34q-cai`)
- **Transfer fee: 0** (same as today)
- **Controller: project owner / future DAO** (keep upgradeable to pull in DFINITY's ICRC-1 security patches; not blackholed)

The existing 1M PP in the owner's wallet from the original deploy stays. Any other balances in the ledger at cutover time are wiped as part of the re-init.

### shenanigans

Single mutable canister. Responsibilities:

1. **Observer** — a timer fires every N seconds, calls backend's existing query methods (`getAllActiveGames`, `getUserGames`, `getDealerPositions`, etc.), diffs against the last-processed state, and mints PP for new events.
2. **Mint engine** — holds the current PP mint rules as mutable config (deposit multiplier, dealer multiplier, referral L1/L2/L3 cuts, etc.). Uses `icrc1_transfer` on `pp_ledger` to mint PP from the minting account (self) into players' chip subaccounts.
3. **Chip custody** — holds all in-game PP on `pp_ledger` under subaccounts derived deterministically from each player's principal. `(shenanigans, user_subaccount(P))` is P's chip account.
4. **Spell logic** — existing spell config preserved 1:1. Casting, outcomes, targeting all unchanged semantically; the only change is that PP movements are real ledger transfers between shenanigans' subaccounts rather than edits to a backend map.
5. **Leaderboard** — tracks cumulative PP burned / spells cast per player (not holdings). Served from shenanigans state, not from the ledger.
6. **Admin dashboard API** — getter/setter pairs for every tunable, gated to an admin principal.

## Key mechanisms

### Minting (observer pattern)

The shenanigans canister runs a heartbeat timer (suggested interval: 5–10 seconds). On each tick:

1. Read the last-processed cursor from stable storage (e.g., highest game ID seen, last referral event timestamp).
2. Call backend query methods to fetch new events since that cursor.
3. For each new event, compute the PP award using current mint rules.
4. `icrc1_transfer` PP from the minting account (self, default subaccount) to the recipient's chip subaccount.
5. Advance the cursor.

**Idempotence**: the cursor in stable storage guarantees an event is never re-processed. If the canister crashes mid-batch, the cursor only advances after successful mints.

**Catch-up**: if shenanigans is down or buggy, backend state remains the system of record. Shenanigans can always replay missed events from backend queries.

**Event → mint mapping** (preserved 1:1 from current backend behavior):

| Backend event                 | Recipient           | PP minted                                |
|-------------------------------|---------------------|------------------------------------------|
| `createGame` (ICP deposit)    | depositor           | Per current rule in `backend/main.mo:684` |
| `addDealerMoney`              | dealer              | `icpAmount × 4000` (current rule)        |
| Referral L1 credit            | L1 referrer         | Current % cut                            |
| Referral L2 credit            | L2 referrer         | Current % cut                            |
| Referral L3 credit            | L3 referrer         | Current % cut                            |

All rates are stored as mutable config with admin setters; the "current rule" column is the initial value, not the permanent value.

### Chip custody and subaccount layout

Every player has a unique "chip subaccount" derived deterministically from their principal — for example, a 32-byte subaccount equal to `sha256(principal_bytes)` truncated/padded to 32 bytes. Deterministic derivation means shenanigans doesn't need to store a mapping of principal → subaccount; it recomputes on demand.

All in-game PP lives at `(shenanigans_principal, user_subaccount(P))` on `pp_ledger`. When shenanigans needs to move PP between players (spell effects), it issues `icrc1_transfer` with itself as the caller, source subaccount = victim's derived subaccount, destination subaccount = beneficiary's derived subaccount. No ICRC-2 allowance is ever needed, because shenanigans is the account owner for all chip subaccounts.

### Deposit flow (external wallet → chips)

Used when a player wants to bring PP they already hold outside the game (e.g., the 1M owner balance, or PP they previously cashed out) back onto the table.

1. Player signs one `icrc2_approve` on `pp_ledger` giving shenanigans permission to spend from their main account, up to some allowance (can be large — this is a one-time signature).
2. Player calls `shenanigans.depositChips(amount)`.
3. Shenanigans validates `amount >= MIN_DEPOSIT` (initial value: **5,000 PP**; admin-tunable). On failure, reject.
4. Shenanigans calls `icrc2_transfer_from` pulling `amount` from the player's main account `(P, null)` to the player's chip subaccount `(shenanigans, user_subaccount(P))`.
5. On success, chips are immediately available for spells.

### Cash-out flow (chips → external wallet)

Used when a player wants to move PP out of the game to their external wallet. Designed with 7-day friction to prevent "shuttle in, cast, shuttle out" exploitation.

1. Player calls `shenanigans.requestCashOut(amount)`.
2. Shenanigans validates the player has at least `amount` in their chip subaccount (not already queued). On success, records a pending cash-out entry: `{player, amount, claimable_after: now + 7 days}`.
3. The `amount` stays in the chip subaccount during the queue window — still exposed to spells. If a spell drains the chip balance below the queued amount before claim, the cash-out partially or fully fails at claim time (claims can only take what's still there).
4. After 7 days, player calls `shenanigans.claimCashOut(queue_id)`. Shenanigans `icrc1_transfer`s from the chip subaccount to the player's main account `(P, null)`, up to the lesser of the queued amount and the current chip balance.
5. Multiple cash-out requests can be queued concurrently (each with its own claimable timestamp).

The 7-day delay is a tunable parameter with an admin setter; 7 days is the initial value.

### Spell casting

Conceptually unchanged from current shenanigans. What changes:

- Where current code calls `backend.deductPonziPoints(user, amount)`, new code calls `pp_ledger.icrc1_transfer` from `user_subaccount(user)` to the minting account (the burn sink).
- Where current code calls `backend.transferPonziPoints(from, to, amount)`, new code calls `pp_ledger.icrc1_transfer` from `user_subaccount(from)` to `user_subaccount(to)`.
- Where current code calls `backend.burnPonziPoints`, same as the deduct case — transfer to minting account = burn in ICRC convention.
- Where current code calls `backend.distributeDealerCutFromShenanigans`: this one is backend-side state (dealer pool balances). It remains a call to backend if and only if backend still owns dealer repayment balances; however since we're removing all PP-knowledge from backend, this path must be redesigned. **See Open Question 1 below.**
- Balance checks (`getPonziPointsBalanceFor`) become `pp_ledger.icrc1_balance_of` queries on the chip subaccount.

Spell configs (names, costs, odds, outcomes, colors, durations) are preserved verbatim. The existing admin methods for config tuning (`updateShenaniganConfig`, `resetShenaniganConfig`, `saveAllShenaniganConfigs`) stay.

### Leaderboard

Lives entirely in shenanigans state. Two maps:

- `pp_burned_per_player : Map<Principal, Nat>` — cumulative PP transferred to the burn sink by this player across all casts
- `spells_cast_per_player : Map<Principal, Nat>` — cumulative count of successful casts

Query methods serve top-N lists for both. No "top holders" leaderboard (explicitly rejected: rewards hoarding instead of playing).

### Admin dashboard

Every tunable parameter gets a getter/setter pair on shenanigans, admin-gated via caller check. The existing [ShenanigansAdminPanel.tsx](../../../frontend/src/components/ShenanigansAdminPanel.tsx) is extended with rows for:

- Deposit → PP multiplier
- Dealer money PP multiplier (currently 4000×)
- Referral L1 / L2 / L3 cuts
- `MIN_DEPOSIT` (initial: 5,000 PP)
- `CASH_OUT_DELAY_SECONDS` (initial: 604,800 = 7 days)
- Observer polling interval
- Plus the existing spell config controls

Authorization: all setters check `caller == admin_principal` and reject otherwise. The admin principal is set at init, changeable by the current admin (standard rotation pattern).

## Tunable parameters (initial values)

| Parameter                        | Initial value     | Unit                 |
|----------------------------------|-------------------|----------------------|
| `MIN_DEPOSIT`                    | 5,000             | PP (in whole tokens) |
| `CASH_OUT_DELAY_SECONDS`         | 604,800           | seconds (7 days)     |
| Deposit → PP multiplier          | (match backend)   | PP per e8 ICP        |
| Dealer money PP multiplier       | 4,000             | PP per e8 ICP        |
| Referral L1 / L2 / L3 cuts       | (match backend)   | %                    |
| Observer polling interval        | 10                | seconds              |

All adjustable via admin dashboard after launch.

## Trust properties

- **PP supply** — only shenanigans can mint PP (it's the minting account). Shenanigans is mutable, so in principle a malicious upgrade could mint unlimited PP. This is accepted: PP is a gameplay token, not a store of value. The critical trust property (no one can rug ICP) depends on backend, not shenanigans.
- **ICP custody** — unchanged. Backend holds all ICP. No code path introduced by this spec touches ICP. Backend can be blackholed independently.
- **Chip isolation** — chips can only be moved by shenanigans (which owns the subaccounts). A player's external wallet balance (at `(P, null)`) is untouchable by shenanigans except via `icrc2_transfer_from` with an explicit player-signed allowance on a `depositChips` call.
- **Blackhole-readiness of backend** — after this migration, backend has no PP-specific methods, no `shenanigansPrincipal` reference, no admin setters related to PP. The blackhole work from the earlier audit (removing `setTestMode`, `setCanisterPrincipal`, `setShenanigansPrincipal`, seed methods, `assignCallerUserRole`) remains to be done separately; this spec only removes the PP-related obstacles.

## Migration

Because existing PP balances are test-only and wipeable, migration is a cutover, not a state transfer.

1. Deploy new `shenanigans` code as an **upgrade of the existing shenanigans canister** (canister ID unchanged: `j56tm-oaaaa-aaaac-qf34q-cai`). The only state worth preserving is spell configs; cast history / shenanigan stats can be wiped along with PP. Migration hooks preserve the spell configs and reset the rest.
2. Reinstall `pp_ledger` with new init args: minting account = shenanigans principal, decimals = 8. This wipes all PP balances including the 1M owner balance. (Owner balance can be re-minted manually post-reinstall if desired.)
3. Deploy new `backend` code with all PP methods and PP state stripped out.
4. Shenanigans observer starts from the current backend state as "fully caught up" — no backfill of historical PP is attempted. Everyone's PP balance starts at 0. New gameplay events from cutover forward mint normally.
5. Admin dashboard deploys with initial tunable values as listed above.

Frontend changes:
- All UI that reads PP balances switches from `backend.getPonziPoints*` to `pp_ledger.icrc1_balance_of` on the chip subaccount.
- New UI for deposit-chips, request-cashout, claim-cashout flows.
- Cashout queue visibility (pending cashouts list with countdown).
- Wallet balance display showing `(P, null)` balance (main wallet PP vs chip PP).

## Open questions

1. **Dealer cut from shenanigans** — today, shenanigans calls `distributeDealerCutFromShenanigans` on backend, which distributes 10% of spell cost to the dealer repayment pool. This crosses the PP/ICP boundary (PP burned, dealers compensated in ICP? or in PP?). The cleanest interpretation of "backend knows nothing about PP" is that shenanigans handles dealer cuts internally in PP (shenanigans could maintain its own dealer-pool subaccount), and backend's dealer repayment pool stays purely ICP-denominated from ponzi activity. Needs a specific decision during implementation planning.

2. **Referral chain ownership** — backend currently stores `referralChain` (one-time referrer link per player). This is used for both PP and ICP flows (dealer referral cuts). It stays in backend since it gates ICP-denominated dealer behavior. Shenanigans reads it via query when attributing PP referral credits. Confirmed: no change needed here, but naming it explicitly so it doesn't get accidentally removed during backend cleanup.

3. **Observer lag UX** — a 10-second polling interval means PP from a deposit shows up ~10 seconds after the deposit. The frontend should be designed to either show a "PP pending" state during the gap, or have the UI optimistically reflect the expected mint and reconcile. Cosmetic, not a correctness issue. Worth flagging for the frontend plan.

4. **Re-init of pp_ledger** — ICRC-1 ledger canisters support upgrades but re-initializing with new args (specifically, changing the minting account) may require a reinstall. If reinstall is required, the canister ID stays the same but all state is wiped — acceptable per the non-goals. Implementation planning will verify the exact procedure.

## Out of scope

- Blackhole-readiness cleanup of other backend concerns (`setTestMode` removal, admin-principal hardcoding, etc.) — tracked separately.
- PP supply schedule / inflation modeling — deliberately deferred; tunable parameters exist to adjust once real data is available.
- Any trading venue / liquidity pool for PP — PP is transferable post-migration but no market is proposed.
- DAO controls for shenanigans upgrades — long-term consideration, not part of this migration.
