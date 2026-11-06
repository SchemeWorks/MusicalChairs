<!-- Verbatim output of the `musicalchairs-ponzi-economy-auditor` review persona (Sonnet worker, read-only, no build or test executed), commissioned by the coordinator for kanban IMPL-40 (plan Milestone 10 step 1) against commit 3c3003c of codex/native-solana-production-core (program sources identical through 37185ea). Published unedited as repo evidence at Codex's request so the returned decisions can be adjudicated from readable artifacts; the coordinator's triage, the applied fixes and the returned decisions are in docs/coordination/native-solana-m10-review-triage-20261014.md. Severity labels and numeric examples are the persona's own; every runtime claim marked "cluster" is a reading of the rent rule, not a measurement. -->

# M10 economy audit (persona output, verbatim)

# IMPL-40 economy audit (musicalchairs-ponzi-economy-auditor)

Scope: native Solana program `solana/programs/musical_chairs` and its host oracle `solana/crates/economics`, read-only review. No file was edited, no git write, no cargo/anchor/build/test was run. Numbers below are recomputed with `python3` integer arithmetic only (not program execution); anything labelled "cluster" is a reading of the runtime rule, not a measurement.

## 0. Header

- Commit: reviewed at HEAD `3c3003c` as instructed. During the review the coordinator committed `37185ea` (tests/inline_alloc.rs + docs only). `git diff --stat 3c3003c HEAD -- solana` lists only `tests/financial-runtime/tests/inline_alloc.rs`; `solana/programs` and `solana/crates` are byte-identical between the two commits, so every finding applies to both.
- Program source read (non-test code in full unless noted):
  - `lib.rs` 1-295; `error.rs` 1-132; `claims.rs` 1-135.
  - `state.rs` 1-900 (all non-test code incl. `required_close_reserve`, SPACE table); tests outline only.
  - `positions.rs` 1-1510 (all non-test) + unit tests 1760-1800 and 2200-2270.
  - `allocation.rs` 1-1445 (all non-test) + test outline 1446-3356.
  - `close.rs` 1-1560 (all non-test) + row outline of the harness.
  - `controls.rs` 40-345, 340-1335 (initialize, ops, management, config, provisioning, closure, reconcile), 1510-1735 (account structs), tests 2090-2260.
  - `provider.rs` 1-230, `sampling.rs` 1-100, `events.rs` field outline.
  - Host crate: `fees.rs` 1-151, `earnings.rs` 1-211, `backers.rs` 84-135 and 604-634.
- Harness read: `payout_props.rs` 1-150; `inline_alloc.rs` 1-200; `allocation_diff.rs` header 1-120 + test list; `fuzz.rs` 1-140 + `assert_books` 550-640; `world.rs` 1570-1650; `runtime.rs::conservation_check` 480-515; `close.rs` (harness) 500-560 + full row list; `positions_suite.rs` 170-260 + row list; `controls.rs` (harness) 1940-2025.
- Docs read: `economics.md`, `backer-allocation.md`, `round-state-machine.md`, `launch-decisions.md`, `measurements-2026-10.md`, M7 supplement, spec section 3, 4, 5, 7, 8 and headings, decision-register headings, `native-solana-economics.md` (first 80 lines), kanban IMPL-40 card, M8 handoff excerpts.

## 1. Findings (economy)

Summary

| ID | Severity | Confidence | One line |
| --- | --- | --- | --- |
| ECO-1 | Medium | High (code) / Medium (exploit economics) | Every toll-bearing claim starts a 6-387 transaction serial ticket for the price of one transaction; tolls of 1-4 lamports start tickets that can only credit zero, unlike the host oracle |
| ECO-2 | Medium | High | `ConfigTerms::validate` does not pin `simple_toll_bps` / `simple_toll_day_bounds`, but `simple_toll()` hard-refuses divergent tables at payout time: a staged-and-activated version strands its Simple positions |
| ECO-3 | Low | High (mechanics) / Low (likelihood) | `sync_game` / `sync_ops` ratchet: a rent-floor decrease becomes permanent ownerless `excess`, and a later rent increase makes every `sync_*` caller fail `InvalidState` |
| ECO-4 (= M9-F1) | Low / Info | High | `claim_backer` and every other direct-lamport credit lack a deliverability preflight; assessed: atomic, no fund loss, dust only, adapter-side preflight is the fix |
| ECO-5 | Info | High | Candidacy needs only `loss > 0`: a dust-loss (12-lamport single claim, single digits with a tuning claim) Sybil can be promoted, burning a 1,914,000-lamport B-account rent for a worthless entitlement |
| ECO-6 | Info | High | Inventory of lamports with no owner and no claim (excess sources, permanent-page `Caller` rent, request-account donations) |

### ECO-1  Ticket amplification and dust-toll tickets (Medium)

Where: `positions.rs:984-992` (`toll_route`), `positions.rs:1245-1257` (`payout_preflight`), `allocation.rs:501-515` (`finish_discover`), `positions.rs:842-848` (`payout_gate`: `active_ticket == 0`). Oracle: `crates/economics/src/backers.rs:604-625` (`Book::allocate` sends a toll to seed with no ticket when `floor(H*35%) == floor(H*45%) == floor(H*20%) == 0`, `H = toll - toll/2`, i.e. toll <= 4). The test header in `tests/allocation_diff.rs` records the divergence ("the program's `toll_route` always runs a ticket when the registry is non-empty") but not its cost.

Claim 1 (parity; the routing is an explicit pin, so this is a decision for the coordinator, see the fix): the program starts a full allocation ticket for toll 1..4 although every slice floors to zero. The pin is `toll_routes_to_nothing_the_seed_or_a_ticket` (`positions.rs:2073-2079`: `toll_route(1, 1) == Ticket`); the fixture-driven dust rows (`only_b_dust_tolls`, `start_ticket_fixture`) bypass `toll_route` and are unaffected.
- toll = 3, registry = {A (rem 1e9), B (rem 1e9)}: `fold_slices_with_bps(3)`: seed0 = 1, H = 2, senior = floor(0.70) = 0, a = floor(0.90) = 0, b = floor(0.40) = 0, rounding 2 to seed, so seed = 3, credited = 0.
- Program: `toll_route(3, 2) = Ticket`, `pool.next_ticket += 1`, `pool.active_ticket = id`, `pending_toll += 3`, phase DISCOVER; the economic result after six phases is exactly "seed += 3". Host `Book::allocate(3)`: "seed += 3", no ticket, `ticket() == None`.
- Toll <= 4 corresponds to `paid_gross <= 41` at the 12% tier (`41 * 1200 / 10000 = 4`), `<= 66` at 7.5%, `<= 166` at 3% (toll < 5 iff `paid x bps < 50,000`).

Claim 2 (amplification, applies to any positive toll, not only dust): a claimant can force every other payout and every Series A funding to wait for a serial ticket whose completion costs the community many transactions.
- Attacker: a minimum 10,000,000-lamport Simple position accrues `10,000,000 * 11 / 8,640,000 = 12.73` lamports/s, so a claim one second after the previous one pays 12-13 gross, toll `floor(13 * 12%) = 1`. One transaction (5,000 lamports base fee) starts a ticket.
- Defender: measured ticket sizes at 8-row chunks (`measurements-2026-10.md` section 1): N <= 8 -> 6 tx (derived: one 8-row batch per row phase plus senior, seal, commit), N = 64 -> 27, N = 100 -> 42, N = 256 -> 99, N = 1,024 -> 387 (one-row fallback 3,075 at N = 1,024). At 5,000 lamports each: 30,000 / 135,000 / 210,000 / 495,000 / 1,935,000 lamports of fees per forced ticket, i.e. amplification 6x .. 387x in transactions. At the documented 26,153,846,153 ICP cycles per threshold signature, N = 1,024 costs 387 * 26,153,846,153 = 1.012e13 cycles per attacker transaction.
- Barrier: while the ticket is active `open_position` is allowed, but `withdraw_simple`, `settle_compounding` (AllocationPending 6010) and `fund_series_a` (6010) wait. Anyone can progress, so this is a tax and an ordering advantage (whoever bundles the final `commit_ticket` with their own payout in one transaction gets the next slot), not a permanent lock. It is an accepted design trade (availability doc "narrow barrier"), but the budget (`native-solana-icp-budget.md`, K = P x ticket_tx for P = 1,000 payouts/month) does not model adversarial P, and the dust subset has no justification at all.
- Not covered by the inline path: inline is opt-in by the payer and only for registry <= 8; an attacker simply does not opt in.

Affected state: `Pool.next_ticket`, `Pool.active_ticket`, `Pool.pending_toll`, `Ticket` (singleton). Methods: `withdraw_simple`, `settle_compounding`, `begin_ticket`, all six phase instructions.

Impact: liveness/cost amplification, no fund loss (conservation holds: `pending_toll` is released at commit; see ledger F5).

Minimal fix preserving the frozen contract (no ABI, error-code, event or SPACE change):
1. In `toll_route` (flipping the `positions.rs:2078` pin and re-running the suites), route to `TollRoute::Seed` when the pinned-bps fold yields senior == a == b == 0 (toll <= 4 at 35/45/20), exactly the host `Book::allocate` predicate. `PayoutPaid.ticket_id` stays 0 as for any zero-toll payout. Side effects dropped: the dust ticket's `materialize` writes and its prune of captured-closed rows (registry hygiene is deferred to the next real ticket, as the module docs already allow).
2. Keeper (M11) contract: ticket completion is the top-priority queue; alarm on tickets/hour; budget scenario with an adversarial P.
3. Any threshold above 4 (a minimum ticket-bearing toll scaled by N) is a contract change: RETURN to Rob, do not apply silently.

Test that catches it: `dust_toll_routes_to_seed_without_a_ticket` (section 4, T1). Expected: toll 3 with registry {A,B}: `seed += 3`, `pending_toll == 0`, `active_ticket == 0`, `next_ticket` unchanged, `PayoutPaid.ticket_id == 0`, no `TicketCommitted`; toll 5 still starts a ticket (H = 3: senior 1, a 1, b 0). RED on the current ELF (ticket phase DISCOVER after the payout).

### ECO-2  Staged toll table not pinned (Medium)

Where: `controls.rs:113-188` (`ConfigTerms::validate`) versus `positions.rs:880-892` (`simple_toll`: `require!(tier == host, InvalidState)` where `host = fees::simple_toll_bps(elapsed)` is hard-coded 1,200/750/300 with 7/14-day bounds, `fees.rs:39-46`). `validate` pins seven fixed-helper fields (Simple rate 1,100, A 12,400, B 11,600, seed_bps 5,000, open cutoff 10, deadline 600, quota) and only checks `simple_toll_day_bounds[0] < [1]` and each toll bps <= 10,000. The unit test `pinned_config_must_agree_with_the_hard_coded_simple_tables` (`positions.rs:1774-1789`) documents that a divergent table is refused at payout "rather than mispriced"; no harness row stages one (`grep simple_toll_bps` over the harness is empty; the staged-config rows only test canonical-restating pins).

Numeric example: G stages v2 = launch terms with `simple_toll_bps = [1_200, 750, 400]` (a plausible "raise the late tier to 4%" proposal). `validate` accepts it, `stage_config_version` creates it, `activate_config_version` is permissionless once `round_id >= effective_round_id`. A 1 SOL Simple position opened under v2 pins v2. Claim at day 6: tier[0] 1,200 = host 1,200, pays. Claim at elapsed = 14 days exactly (1,209,600 s): config tier = bps[2] = 400, host = 300, `InvalidState` 6001, fee-only. Every claim from day 14 on fails, forever; `close_simple_position` needs `owed == 0` so it cannot terminate the position; up to 7 days x 11% = 0.77 SOL per SOL of gross accrued after the last pre-day-14 claim is unreachable, and the position dies with its round (stale-round positions are terminal). Divergent day bounds fail the same way inside the divergent window (bounds [5,14]: elapsed 5.0-6.99 days gives tier 750 vs host 1,200).

Impact: irreversible user-fund immobilisation for every position opened under the bad version; the only repair (stage v3) is future-only. Requires a G action (2-of-3, 172,800 s delay) so likelihood is low, but the failure is silent at staging time and surfaces 14 days later.

Related, not findings (governance-power surface, listed for the threat model): `front_end_load_bps` in [0, 10,000] (a staged 10,000 sends 100% of new deposits to `management`), `jackpot_bps` in [0, 10,000], `compound_factors_q18` any `u64`, `min_game`/`min_a` any value > 0, `plan_durations_s` any > 0. Positions pin the version at open, so changes are prospective, and G already holds upgrade authority (spec section 5), so these add no new trust, but only ECO-2 is a self-inflicted lock-up by a well-meaning change.

Minimal fix: add to `validate`: `simple_toll_bps == [1_200, 750, 300] && simple_toll_day_bounds == [7, 14]`, with two new rows in `config_terms_reject_each_bound_once` and two `noncanonical_*` rows in the `stage_config_requires_g_and_future_round` harness row (InvalidState, fee-only). No layout, ABI, code or event change.

### ECO-3  Rent-floor ratchet in `sync_game` / `sync_ops` (Low)

Where: `lib.rs:50-63` and `lib.rs:86-96`.
```
rent_floor = Rent::get().minimum_balance(len)
spendable  = lamports - rent_floor
unaccounted = spendable - recorded   // recorded includes pool.excess; Err(InvalidState) if negative
pool.excess += unaccounted
```
Numeric example (local default 6,960 lamports per byte incl. the 128 B overhead; DR-27 "696 terminal step" is a scenario, not a quote): GameVault is 9 B -> floor 953,520; OpsCash is 0 B -> floor 890,880. If the floor falls to 696 per byte: 95,352 and 89,088. The first `sync_game` after the drop books `excess += 858,168`; the first `sync_ops` books `Ops.unallocated_excess += 801,792`. Total 1,659,960 lamports (0.00166 SOL) of sponsor-funded rent become ownerless forever (no instruction spends excess; correct by contract). Worse, `recorded` now contains the phantom 858,168: if the floor ever rises above the lowest value it was synced at (even by one lamport per byte), `spendable < recorded` and every `sync_game` caller fails `InvalidState`: `open_position`, `fund_series_a`, both payouts, `claim_backer`, `withdraw_management`; every `sync_ops` caller fails likewise, which includes every payout (`payout_apply` calls `sync_ops` unconditionally, `positions.rs:1304`), `fund_operations`, provisioning and the whole close path. Recovery would need a program upgrade or an anonymous top-up equal to the increase.

Likelihood: the SIMD-0437 schedule in DR-27 is monotone decreasing, so low; but nothing in the harness exercises a floor change against `sync_*` (only `changed_rent_sysvar_after_fulfillment_does_not_overstate_eligible`, which tests the observation path). `send_with_rent_rate` already exists.

Minimal fix: let `excess` float. Compute `non_excess = recorded - pool.excess`; `pool.excess = spendable.checked_sub(non_excess)` (InvalidState only if even the non-excess partitions are uncovered, i.e. a real shortfall). Same in `sync_ops`. A rent decrease still lands in `excess`, a later increase now shrinks it instead of failing. No ABI/layout/event change.

Test: T4 (section 4).

### ECO-4  M9-F1 assessment: `claim_backer` has no deliverability preflight (Low / Info)

Where: `claims.rs:74-134`, `lib.rs:72-83` (`send_game`: direct lamport mutation then the runtime post-transaction rent-state check). Same class: net payout in `payout_apply` (`positions.rs:1310`), `withdraw_management` (`controls.rs:577`), `withdraw_operations` (system CPI, same rule), `close_shared_account` owner component (M8-F1). The deliverability predicate `credit_is_deliverable(c, a, min) = c + a >= min` exists only for owner refunds (`close.rs:270-272`).

Assessment:
- Atomic. A failing credit reverts the whole transaction including `funded_claims -= amount` and `Backer.funded = 0`; verified by reading order in `claim_backer` (state writes at 108-121 precede the send at 122, both inside one instruction). The funded balance stays claimable.
- Dust only. A destination that already holds at least `min` (890,880 lamports for a 0-byte account at the local rent fixture; read `Rent::minimum_balance(0)` live) can always receive. Numeric: funded 500,000 to a fresh destination (c = 0): post-balance 500,000 < 890,880, cluster rejects (`InsufficientFundsForRent`), LiteSVM accepts (measured: yes; `tests/transfer_constraints.rs` documents the data-less gap). Claiming to any account with c >= 390,880 succeeds. A Backer's own signing wallet virtually always qualifies.
- Liveness consequence for the closing payout: the closing claimant's net credit must be deliverable, otherwise the claimant must pick another destination; with net = 0 (empty pot) no credit occurs, so a fresh destination is accepted (and its owner refund is then deferred by the IMPL-43 mechanism).
- Not a lamport-stranding path; no new mechanism recommended (matches the coordinator's "accepted/documented"). The frozen error list has no apt code (6051 `RefundNotDeliverable` would be a semantic stretch).

Recommended: the GUI/keeper adapter preflights `destination_lamports + amount >= Rent::minimum_balance(0)` for claims, payouts, management and operations withdrawals and shows "claim dust to an account that already holds SOL". Add the rule to the M11/M12 adapter contract. Label stays M9-F1.

### ECO-5  Dust-loss Sybil candidates (Info)

Where: `close.rs:98-102` (`classify`: `fallback = loss > 0`, `preferred = fallback && !ever_a && !ever_b`), `close.rs:1228-1276` (`apply_winner`).

Numeric example: a 10,000,000-lamport Simple position (FEL 400,000; pot credit 9,600,000) claimed once at elapsed 849,139 s (9.83 days, tier 750 bps): owed = floor(10,000,000 x 11 x 849,139 / 8,640,000) = 10,810,797, toll = 810,809, net = 9,999,988, so `UserRound.deposits - net_payout = 12` (single-claim granularity is ~12 lamports per second; a second small claim can tune the loss to single-digit values). That wallet is a preferred candidate. If drawn with loss 12: entitlement = floor(12 x 11,600 / 10,000) = 13, `remaining = 13 > 10`, so it IS appended to the registry as a 13-lamport row (the next real ticket credits at most 13 and prunes it). With loss <= 9 (entitlement <= 10) it is not appended. Either way the Series B Backer account has already been created from the locked reserve (1,914,000 lamports of sponsor/owner rent at the local fixture) and `ever_b` is permanently set, removing the wallet from future preferred sets. Loss 9 -> entitlement 10; loss 10 -> 11 (the append threshold).

Attacker cost per Sybil: permanent rent User 1,475,520 + UserRound 1,510,320 + Position 1,997,520 = 4,983,360 lamports plus fees (the deposit itself is recycled through the pot). Effect: dilution of real losers' draw probability (N real, k Sybils: P = N / (N + k)) and waste of one promotion plus 1,914,000 lamports of reserve per win. No cash is created or owed. Accepted game-design surface; if Rob wants a loss floor it is a contract change (RETURN, not applied silently). Suggested minimum: loss >= `min_game` x 10% would keep real losers and exclude dust.

### ECO-6  Inventory of ownerless lamports (Info, supports invariant 8)

No instruction can spend any of these (contract: no credit/refund/sweep ABI). All are bounded and either donations or sponsor dust:
1. Anonymous credits to GameVault / OpsCash (`Pool.excess`, `Ops.unallocated_excess`): donors' loss.
2. Rent-floor decreases (ECO-3): sponsor-funded rent, 858,168 + 801,792 lamports in the 696 scenario.
3. Prefunded page remainder at `close_shared_account` (`controls.rs:1177-1181`): `page_lamports - (ops + owner)` goes to `Ops.unallocated_excess`; belongs to the prefunding donor only.
4. `initialize` books any OpsCash prefund above rent as excess (`controls.rs:404`).
5. `Funding::Caller` rent paid for permanent pages (Registry 3,125,040 / Snapshot 4,572,720 at the local fixture): recorded in `RentComponents.owner` but `close_shared_account` returns `NotClosable` for those kinds, so the payer donates it. UI/keeper docs must say "never use Caller funding for Registry/Snapshot".
6. Donations to an ORAO request account before `observe_request_fulfilled` lower `eligible` 1:1 (`close.rs:335-339`); the attacker burns exactly what the sponsor/owner loses, no profit.
7. Fulfilled-request rent retained by ORAO (1,914,000 per close) and the consumed provider fee (300,000): documented consumption.
8. Backer entitlement remainder <= 10 lamports per Backer (promise, not cash) and Simple accrual remainder < 1 lamport at expiry.

Nothing that belongs to a user or the sponsor lands in `excess` except item 2 (sponsor rent) and nothing owed by a close does (owner refunds are deferred in `CloseRecord.locked_owner_remaining`, never converted: `close.rs:284-299`).

## 2. Compact invariant ledger

Notation: G gross, L = `ops.locked_remaining_total`, V_game = GameVault lamports - rent(9), V_ops = OpsCash lamports - rent(0).
Identities (asserted after every harness transaction, `runtime.rs:480-515`):
- I1: V_game = pot + seed + management + pending_toll + funded_claims + excess.
- I2: V_ops = free_balance + L + unallocated_excess.
- I3: L = sum over closes of (locked_sponsor_remaining + locked_owner_remaining).

| Flow | Inputs | Fees / splits | State deltas | External transfers | Residual |
| --- | --- | --- | --- | --- | --- |
| F1 `open_position(G)` (`positions.rs:632-741`) | G >= 10,000,000 from owner; owner also pays User/UserRound/Position rent | FEL = floor(G x 400 / 10,000) to management; pot credit = G - FEL | management += FEL; pot += G - FEL; total_gross += G; UserRound.deposits += G; participant_count += 1 if first in round; quota slot; Position pins config, principal = G (gross base) | owner -> GameVault: G | I1 holds: V_game += G = (G - FEL) + FEL. Rounding: FEL floored, remainder (<= 0.96 lamport) stays in pot. G = 10,000,001 -> FEL 400,000, pot 9,600,001 |
| F2 `fund_series_a(G)` (`:750-825`) | G >= 50,000,000 | no FEL | pot += G; Backer.amount += G; entitlement += floor(G x 124 / 100); registry append iff inactive and remaining > 10; quota slot | owner -> GameVault: G | I1 holds. Blocked while ticket active (6010), Closing (6011), paused. 50,000,000 -> entitlement 62,000,000 |
| F3 solvent payout (`:1292-1362`) | position, `minimum_receive`, destination | paid = min(owed, pot); toll = floor(paid x bps / 10,000); net = paid - toll; bps 1,200/750/300 by elapsed < 7 d / < 14 d / else (Simple), 900/1,300 (Compound15/30) | pot -= paid; toll -> pending_toll (ticket) or seed (registry empty) or 0; UserRound.net_payout += net; Position.paid_gross += paid | GameVault -> destination: net | I1: V_game -= net = -(paid) + toll. 1 SOL, 1 day: owed 110,000,000, toll 13,200,000, net 96,800,000. Day 7: 770,000,000 / 57,750,000 / 712,250,000 |
| F4 insolvent closing payout | owed > pot (strict); reserve R from schedule; `max_work_contribution` | paid = pot; toll on paid; reserve split s = min(R, free), o = min(R - s, max_contribution), s + o = R else ReserveUnavailable | pot = 0; close counters advanced; CloseRecord bound (deadline = now + 600, owner_refund_destination = destination); free -= s; L += R | owner -> OpsCash: o (before the vault payout); GameVault -> destination: net | I2: V_ops += o, free -= s, L += R = s + o. Example pot 500,000,000, owed 700,000,000, 12% tier: paid 500,000,000, toll 60,000,000, net 440,000,000; R = 14,836,800 (participants 1-8, registry_len 1, no pre-provisioned pages) |
| F5 ticket (`allocation.rs`) | toll T from pending_toll | seed0 = floor(T/2); H = T - seed0; senior floor(H x 35%), a floor(H x 45%), b floor(H x 20%), rounding -> seed; B absent: b -> a; A absent: senior -> seed, a -> b; none: all -> seed; senior to oldest open A (cap, overshoot -> seed); A shares floor(a x w / D), D = sum(open A rem) - senior; B equal floor; caps and residues -> seed | at commit: pending_toll -= T; seed += seed_credit; funded_claims += credited; credited + seed_credit = T checked; Backer.pending/lifetime per row | none | I1 invariant: pending_toll -> seed + funded_claims (sum unchanged). Example T = 13,200,000, registry {A rem 5e9, B rem 1e9}: seed0 6,600,000, H 6,600,000, senior 2,310,000, a 2,970,000, b 1,320,000, A1 5,280,000, B1 1,320,000, seed_credit 6,600,000, credited 6,600,000 |
| F6 `claim_backer` (`claims.rs`) | A and/or B rows | none | materialize pending if ticket <= finalized_ticket; amount = sum(funded); funded = 0; funded_claims -= amount; lifetime unchanged (cap headroom never restored) | GameVault -> destination: amount | I1 holds. Pays only finalized credit; a second call -> NothingToClaim |
| F7 close path (`close.rs`) | R locked at F4 | candidate pages, request, B account from the locked reserve, sponsor first | every spend: OpsCash lamports out = s/o partitions down = L down; `provider_fee_consumed_total`, `request_rent_funded_s/o`; settlement: s -> free, o -> destination if deliverable else stays in `locked_owner_remaining`; finalize: pot += seed, seed = 0, round_id += 1 | OpsCash -> page rent; OpsCash -> Client (fee + rent, exactly); Client -> Treasury/Request (ORAO CPI); settlement owner payout | I2/I3 hold at every row (harness asserts). Owner-cash trace below |
| F8 `withdraw_management(x)` | recipient signs | none | management -= x (x <= management) | GameVault -> recipient | I1 holds |
| F9 `withdraw_operations(x)` | sponsor signs | none | free -= x (x <= free); OpsCash lamports >= rent + L + x | OpsCash -> sponsor | I2 holds; locked and excess untouchable |

Owner-cash trace to the lamport (defaults: fee 300,000, headroom 2,000 bps, 1 candidate in each set, sponsor free = 0, owner max contribution >= 14,836,800):
- out: 14,836,800 (R = 2 x 3,194,640 candidate pages + 0 registry extension + 1,914,000 B + 6,173,520 pending request rent + 360,000 provider cap).
- spent: pages 6,389,280, request 300,000 + 6,173,520 = 6,473,520, B 1,914,000 = 14,776,800.
- back: settlement 60,000 (unspent headroom) + provider return 4,259,520 (eligible = 6,173,520 - 1,914,000 retained; owner-first attribution) + page cleanup 6,389,280 = 10,708,800.
- net owner cost 4,128,000 = fee 300,000 + retained fulfilled request 1,914,000 + B account 1,914,000. Sponsor-funded equivalent adds the permanent CloseRecord 5,463,600 = 9,591,600 (matches `measurements-2026-10.md` section 4a).
- Zero-candidate close: all 14,836,800 returns at settlement.

Dust rules (explicit): FEL floored (pot keeps the remainder); toll floored (claimant keeps the remainder; paid <= 8 at 12%, <= 13 at 7.5%, <= 33 at 3% pays zero toll but each claim costs 5,000 lamports so splitting is uneconomic and same-second claims accrue 0); Simple accrual carries the numerator remainder (< 8,640,000) so repeated claims equal one claim; compound floors; tier rounding, A residue (< open A count), B residue, cap overshoot and the 10-lamport cutoff all route to seed or stay as unpaid entitlement; funded dust stays claimable.

## 3. Proof-scope assessment and hostile timing

Inline vs chunked (item 4):
- Host level (`allocation.rs` tests 2925-3305): account-byte digest equality for 1-8 rows over goldens, dead A pool, only-closed registry, full registry of capped rows, mixed sequences and random sequences vs the host `Book`. Strong.
- SBF level (`tests/inline_alloc.rs`): N = 1..8 on ONE registry shape (alternating A/B, remaining 1e9 / 20 / 5e6 / 1e9, fixed 13,200,000 toll) plus Compound15/30 matured cases at 37185ea; refusal rows (N = 9, pages-first, partial, duplicate). Gaps: no SBF row for an all-8-rows-capped inline run (every row pruned: worst `prune_inline` clone/heap/CU shape; measured 147,005 CU at N = 8 only covers two prunes), a dead-A-pool, an all-closed registry, toll <= 4, or "closing payout / zero toll with accounts appended is ignored". Not financial (a CU overrun fails atomically and the keeper falls back to chunks) but the equivalence claim is only SBF-proven for one shape.
- Cross-ledger invariant: `allocation_diff.rs:585-588` checks per-Backer funded+pending against `Pool.funded_claims` over 50 tickets; the 300-step fuzz and the close suite check only the global I1-I3 and book consistency, not `sum(Backer.funded + committed pending) == funded_claims`.
- Payout differential (`payout_props.rs`) uses fixture positions and a pre-funded destination; the open-then-pay end-to-end conservation with odd gross values is covered by unit tests of `entry_split` (10,000,001 -> 400,000 / 9,600,001) and by the 1 SOL rows, not by a randomized open-then-pay LiteSVM walk.

Hostile timing (item 7), expected vs observed:

| Scenario | Code | Result | Test row |
| --- | --- | --- | --- |
| payout during active ticket | `positions.rs:846` | AllocationPending 6010, fee-only | allocation.rs 1394/1402 |
| payout during Closing | `:845` | Closing 6011 | positions_suite |
| deposit during Closing | `:460` | Closing 6011 | close.rs 1764 |
| deposit during non-closing ticket | `:452` | allowed; pot only; ticket untouched | matrix, allocation rows |
| claim during ticket | `claims.rs:60-63` | pays finalized only; commit adds credit once | `claim_during_ticket_pays_only_finalized_and_commit_adds_once` |
| fund_series_a during ticket / Closing | `:477-478` | 6010 / 6011 | positions_suite 890, close.rs 1766 |
| withdraw_operations racing a lock | `controls.rs:509-521`, `positions.rs:1357-1359` | serialized per transaction; locked and free cannot both be spent | reserve suite |
| two closes | `:845`, `:1338-1339` | second payout Closing; `next_close_id` advanced at the first | `two_closes_two_rounds_sequence` |
| expire vs capture | `close.rs:488`, `:1411-1412` | capture timely iff now <= deadline; expire iff now > deadline and not captured. A fulfilled-but-uncaptured request is expirable at deadline + 1 (keeper requirement, not a bug) | close rows 3750-3917 |
| duplicate claim / replay | `claims.rs:70`, `close.rs:372-376` | NothingToClaim / NothingEligible | various |
| population inflation | `required_close_reserve` | reserve scales ~0.8M lamports per participant (2 sets / 8 rows x 3,194,640); a Sybil pays ~5M permanent rent each, so holding a close hostage costs the attacker far more than the closer's `max_work_contribution` fallback | measurements 4 |

## 4. Recommended tests

T1 `dust_toll_routes_to_seed_without_a_ticket` (ECO-1). Setup: World with registry {A rem 1e9, B rem 1e9}, Simple position whose accrued gross at claim is 33 (toll 3 at 12%) via fixture. Expect after payout: `pool.seed += 3`, `pool.pending_toll == 0`, `pool.active_ticket == 0`, `pool.next_ticket` unchanged, `PayoutPaid { toll: 3, ticket_id: 0 }`, no `TicketCommitted`, state digest equals host `Book::allocate(3, 8)`. Variants: toll 4 same; toll 5 (paid 42) starts a ticket with phase DISCOVER and ends credited <= 2.
T2 `ticket_cost_table_is_documented` (ECO-1). Measure transactions per forced ticket for N in {8, 100, 1024}: expect 6 / 42 / 387 at 8 rows; assert attacker cost = 1 tx. Pure documentation of the amplification for the keeper budget.
T3 `staged_divergent_toll_table_is_rejected` (ECO-2). Unit: add `("simple toll tier 3 = 400", |t| t.simple_toll_bps[2] = 400)`, `("simple toll tier 1 = 1_300", ...)`, `("toll day bounds [5,14]", |t| t.simple_toll_day_bounds = [5, 14])`, `("toll day bounds [7,30]", ...)` to `config_terms_reject_each_bound_once`. LiteSVM: stage each via G; expect InvalidState 6001, fee-only, `Ops.free_balance` unchanged. End-to-end RED on current ELF: stage and activate v2 with bps `[1200,750,400]`, open 1 SOL, warp 14 days, withdraw: expect InvalidState (current), "cannot be staged" (after fix).
T4 `rent_floor_down_then_up_keeps_game_live` (ECO-3). With `send_with_rent_rate`: lower to 10% of default for one deposit of 10,000,000: expect `pool.excess += 858,168` (953,520 - 95,352) and, on the next `sync_ops` caller, `Ops.unallocated_excess += 801,792`; restore the default rate and deposit again: current ELF -> InvalidState 6001 (RED); after the float fix -> Ok, `excess` back to its prior value, I1/I2 hold.
T5 `backer_ledger_sums_equal_global_claims` (proof gap). In `fuzz.rs::assert_books`: sum over every Backer account of `funded + (pending if pending_ticket <= finalized_ticket)` must equal `pool.funded_claims`, and `sum(pending where pending_ticket == active_ticket) == ticket.credited` while a ticket is active; hold at all 300 steps.
T6 `inline_all_rows_capped_n8_equals_chunked` (proof scope). 8 rows each remaining 20, toll 13,200,000: every row capped and pruned; byte-identical to chunked; record CU and packet; add `inline_dead_a_pool`, `inline_all_closed_registry`, and `closing_payout_with_inline_accounts_is_the_chunked_closing_ticket`.
T7 `claim_dust_deliverability_rule` (ECO-4). Unit on the predicate: `credit_is_deliverable(0, 500_000, 890_880) == false`, `(390_880, 500_000, 890_880) == true`; adapter test mirrors it. LiteSVM cannot show the cluster rejection (documented).
T8 `candidate_dust_loss_is_characterized` (ECO-5). Fixture UserRound deposits 10,000,000, net_payout 9,999,991 (loss 9): expect candidate in both sets, promoted entitlement 10, `registry_len` unchanged (10 is not open), B account created (rent 1,914,000 booked under `b_rent_s/o`), `user.ever_b == true`; companion fixture net_payout 9,999,990 (loss 10): entitlement 11, row appended.
T9 `owner_cash_trace_to_the_lamport` (ledger). Closing payout with sponsor free = 0 and owner contribution 14,836,800: assert owner wallet delta at each step equals the trace in section 2 and the final net 4,128,000.
T10 `exclusion_of_permanent_page_caller_rent` (ECO-6). `provision_page(Snapshot, Caller)` then `close_shared_account(Snapshot)` -> NotClosable; document donation.

## 5. Terminology (future user-facing copy)

Keep internal names (do not rename): `front_end_load_bps`, `management`, `PositionOpened.load` = Front-End Load; `simple_toll_bps`, `simple_toll()` = Carried Interest (Simple plan); `jackpot_bps` = Jackpot Fee (compounding plans only, the accepted user-facing label); `toll` / `pending_toll` / `Ticket` internal only.
User-facing mapping: `pot` -> AUM (never "pot"); `seed` -> seed reserve; `management` -> Front-End Load balance; Series A -> Backer funding; Series B / `apply_promotion` / `Promotion*` -> Emergency Equity Conversion; `winner` -> "converted Backer" / selected investor; `draw`, `candidates`, `loser`, `ticket` -> avoid in copy (casino-table or lottery flavor): use "randomized selection", "eligible holders", "net-loss holder", "allocation run". `excess` -> unallocated excess. Position, Backer, AUM, Front-End Load, Carried Interest stay as is. Error names surface unchanged in the IDL; the adapter must map them (Closing, AllocationPending, QuotaExceeded, MinimumReceive, ReserveUnavailable, RefundNotDeliverable) to plain copy. Reminder for the adapter: GUI projections must use gross accrual (R3), and must show Carried Interest as a tier of elapsed time since open.

## 6. Non-findings (checked, with lines)

- Deposit split and unaccounted lamports: `entry_split` floors the load and gives the pot the remainder (`positions.rs:499-507`); `sync_game` precedes the transfer so unsolicited credit cannot be misread as the deposit (`:684-685`, `lib.rs:50-63`). No lamport unaccounted; minimum 10,000,000 enforced before quota (`:463`).
- Simple accrual on gross, remainder carry, 21-day clamp: `positions.rs:693-709` (principal = gross), `earnings.rs:140-180`. `minimum_receive` is checked on net before any mutation (`:940-944`, preflight is read-only `:1218-1287`); toll floors on the gross actually paid (`fees.rs:82-96`); equality is solvent (`fees.rs:94`).
- Compound growth: `compound_owed` multiplies u64 x u64 in u128 (< 2^128 even at u64::MAX, `positions.rs:905-912`); floors only, no minting; no principal return.
- Toll split math: `allocation.rs:119-169` matches the three goldens (T = 10,000 -> seed 5,001 / A 2,363, 1,636 / B 500 each, recomputed); residue by difference bounded by open A count (`:670-678`); cap overshoot to seed; sealed denominator excludes only the senior credit; prune at <= 10 (`:253-255`, `:696-704`); commit adds once (`:754-810`); claims never lower `lifetime` (`claims.rs:60-63`, `allocation.rs:521-538`); lazy `materialize` before any credit in every credit path (`:568`, `:940`).
- Backer registry integrity: dense swap-delete with alias handling (`:710-748`, `:1031-1070`); re-funded capped A keeps `first_sequence` (`positions.rs:775-803`); no duplicate append (`series_a_outcome`, `close.rs:1275`).
- Close accounting: lock before cash moves, owner transfer before vault payout, sync before both (`positions.rs:1303-1310`, `:1357-1359`); partitions fall with every spend (`lib.rs:149-164`, `close.rs:232-259`, `:181-220`); `split_locked` sponsor-first then owner or `LockNotPermitted` (`lib.rs:138-143`); provider return owner-first, preflighted before the CPI, never re-locked (`close.rs:371-404`, `:1493-1511`); deferred owner refund counted in L and deliverable once (`:284-312`, `:1535-1559`); settlement sets `reserve_settled` regardless of destination so finalize is never blocked; finalize carries the seed once (`:416-431`); 116% base is `UserRound.deposits - net_payout` of the winner's round (`:1218-1220`); registry-extension and B rent are held back by the request planner (`:157-177`) and the registry cannot grow between lock and apply (A funding blocked while Closing), so the flagged-or-not extension page always exists or is funded.
- `unallocated_excess` write paths: only `sync_game`, `sync_ops`, `close_shared_account` and genesis; none spend it (`controls.rs:1177-1181`, `:1255-1256`).
- Management and operations: `withdraw_management` is capped by `pool.management` after `sync_game` (`controls.rs:569-582`); a bogus `ops_cash` argument cannot weaken protection because `validate_destination` rejects the real OpsCash PDA (`lib.rs:69`) and `rotate_recipient` rejects program PDAs (`controls.rs:718`); `withdraw_operations` keeps rent + L (`:511-521`) and excess is untouchable because amount <= free and `sync_ops` made I2 an equality.
- `reconcile_ops_cash*`: pure `sync_ops` plus informational Client numbers; an airdrop becomes excess, never free or a provider return (`close.rs:1518-1527`); G's return path books only the executed CPI delta (`:1499-1505`).
- Aliasing: payout destination excludes game vault, OpsCash, pool, admission, ops, ticket, position, user_round, config and the close record (`positions.rs:1229-1243`); non-system or non-empty destinations fail `validate_destination`; claim rejects Backer and pool aliases (`claims.rs:84-102`); allocate rejects duplicate Backers before identity (`allocation.rs:917`).
- Locks: serial ticket, ids dense, watermark materialization sound (`allocation.rs:352-354`).

## 7. Commands run (all read-only)

- `git log --oneline -3`, `git status --short`, `git rev-parse HEAD`, `git diff --stat -- solana`, `git diff --stat 3c3003c HEAD [-- solana]`, `git log --oneline -4`.
- `wc -l` over `solana/programs/musical_chairs/src/*.rs`, docs and harness; `ls` of the crate and harness trees.
- `cat -n` / `sed -n` / `cat` over the files listed in section 0; `grep -n` for symbol, test-name and coverage searches (`simple_toll_bps`, `funded_claims`, `Rent::`, `set_rent`, `AllocationPending`, `next_sequence`, vocabulary scan for casino terms).
- `python3 -` heredocs for integer arithmetic only: rent table `(n + 128) x 3480 x 2`, reserve examples (14,836,800; 17,961,840; 94,633,200), toll/slice thresholds, the worked ledger examples, Sybil loss example, rent-floor deltas (858,168; 801,792). No cargo, anchor, build, test or SBF tool was invoked.
