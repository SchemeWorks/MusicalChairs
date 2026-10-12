# Native SOL economic and lifecycle recommendation

Date: 2026-10-12. Status: **recommended design, not approved implementation or measured SVM proof**.
Design-pass checkout: `codex/native-solana-discovery-20261012`, starting commit `f26022ab610d4df611b994698bf60ec1426502c1`. Financial source is unchanged from `f96b33ff95020458be754bfa0d7471b72a7403af`.

Current processing/outage policy is detailed in [availability and recovery](native-solana-availability.md). Rob rejected indefinite stuck rounds; a fixed 600-second promotion deadline now supersedes the earlier indefinite-oracle-wait recommendation. These revisions remain design recommendations.

Subsequent [recommended defaults](native-solana-defaults.md) preserves source operator eligibility, proposes public A on as an explicit fresh-launch flag difference, and selects zero on-chain keeper rewards with mandatory separate storage/provider/automatic-B reserve. Foundation refresh is verified; no product implementation started.

Three native `gpt-5.6-luna` workers covered equations (SOL-05), Backers (SOL-06), and rounds (SOL-07). The coordinator checked their source claims, recomputed numeric examples and integrated the recommendations below. Native independent review is recorded on the [board](kanban.md). No external-provider repository disclosure, product implementation, dependency install, build, deployment, signing or fund movement occurred.

Confirmed scope remains a fresh native SOL game, separate legacy withdrawals, ICP-hosted current GUI, and ICP PP/Shenanigans. No legacy balance imports, native PP project, or desk rewrite is added.

## Recommendation in plain language

Keep the game's financial rules. Use integer lamports and atomic program deposits/payouts. Process Backer fees in complete, bounded batches, preserving senior-first allocation rather than replacing it with a deposit-weight reward index. Close rounds using current-round participant records and one bound randomness request rather than historical position scans or caller-chosen winners.

During an ordinary processing phase, new game deposits and funded Backer claims stay available; Backer funding and subsequent game payouts wait. During round closing, new deposits wait too. The player receives the accepted payout in the first transaction, without signing every batch. Allocation work is finite for captured N but latency is not yet measured. Randomness receives one fixed promotion deadline, after which public skip/finalization removes oracle dependence without discarding financial allocations. These are disclosed operational differences, not cosmetic details.

## 1. Economic oracle

Treat executable `ponzi_math_sol` source as the preservation reference. It implements 35/45/20 on the Backer half, despite stale comments and frontend text (`main.mo:656–785`; `frontend/src/lib/gameConstants.ts:39–44`; `HouseDashboard.tsx:77–80`). It accrues on gross deposit, while the SOL investment preview currently uses net (`main.mo:2804–2824, 1062–1100`; `SolInvestPanel.tsx:70–76`). Correct those displays in implementation without redesigning the interface. Local source is not proof of deployed state.

Let `P` be gross principal in lamports; all division below floors unless specified. Proposed native formulas:

```text
entry_fee = floor(P * 400 / 10_000)
pot_credit = P - entry_fee
simple_numerator = P * 11 * elapsed_seconds + prior_remainder
simple_gross = floor(simple_numerator / 8_640_000)
next_remainder = simple_numerator % 8_640_000
compound_gross = floor(P * ROI_Q18 / 10^18)
gross_paid = min(gross_owed, spendable_pot)
paid_toll = floor(gross_paid * applicable_toll_bps / 10_000)
paid_net = gross_paid - paid_toll
```

For Simple, elapsed seconds are clamped to remaining time before the 21-day expiry. Carry sub-lamport numerator remainder between claims; at expiry any remaining fraction is below one lamport and creates no funded balance. Reject backwards clock observations rather than underflowing or moving `last_accrued_at` backwards. For compounding, permit settlement only at/after maturity; no on-chain fractional exponent is needed. UI can show an explicitly estimated intermediate curve on the gross base.

Rates and durations remain 11%/21 days, 12%/15 days and 9%/30 days. Simple toll is 1,200 bps before day 7, 750 bps from day 7 to before 14, then 300 bps. Jackpot Fees are 900/1,300 bps. Terms are bound to a version when a Position opens. No separate principal refund is added.

Floor Q18 maturity factors, independently recomputed with Python Decimal at precision 100 from exact decimal rates:

```text
ROI15_Q18 = 4_473_565_759_257_037_859
ROI30_Q18 = 12_267_678_469_131_246_220
```

The worker's nearest-rounded values ended in 860/221. The recommendation explicitly selects **floor**, so these constants end in 859/220. Factor error is below `10^-18` of principal before final floor; for any `u64` principal its downward difference from exact rational maturity earnings is at most 19 lamports. IEEE Float legacy results are a different comparison and need recorded acceptance vectors; do not claim bit-for-bit equivalence.

Explicit width contract:

| Value | Width / bound |
| --- | --- |
| Deposited/transferred SOL, pot, seed, management, pending toll, individual funded/pending credit | `u64`; checked additions and physical backing |
| Owed earnings, cumulative deposit/payout counters, Backer amount/entitlement/lifetime credited, UserRound gross/paid, global funded-claim liability | checked `u128`; funded global liability must still be physically backed and no greater than vault spendable lamports |
| Sum of A headrooms and `slice * individual_headroom` | fixed-width checked `U256` (four `u64` limbs); no arbitrary precision/unbounded arithmetic |

Computed owed earnings may exceed `u64`; compare to pot before narrowing actual payout. Q18 factor multiplication by a `u64` principal fits `u128`. A Backer entitlement can exceed `u64`, so its headroom remains `u128`; pro-rata multiplication uses U256, with the result bounded by the `u64` slice. With a `u64` registry length and `u128` row headroom, its sum fits below 192 bits. This avoids inventing a small monetary cap merely to fit an intermediate product. Pin U256 serialization/checked division and test its cost.

All cumulative totals and entitlement increases are checked. Compute loss only as `deposits > payouts ? deposits - payouts : 0`, avoiding unsigned underflow. Before accepting each game deposit, verify the prospective current-round loss's `floor(loss*116/100)` plus that owner's existing B entitlement fits `u128`, using U256 for the calculation; B entitlement cannot increase elsewhere before the current round closes. Payouts can only reduce that prospective loss. This prevents a later promotion from becoming impossible due to numeric overflow after deposits were accepted. Check Series A entitlement increase before funding transfer. Claims and allocation credits obey physical `u64` limits; lifetime credit must never exceed entitlement. Numerical limits are explicit representation limits, not a Backer population cap.

For an insolvent payout, charging bps on actual gross paid makes the partition exactly conserve the pot without multiplying two arbitrarily wide owed values. It is a declared integer-rounding choice; legacy first rounds net earnings and scales toll/net separately. Preserve strict insolvency `gross_owed > pot`; equality is solvent even if it leaves zero pot. A later accepted claim can trigger closure, or a deposit can refill the same round before that occurs (`main.mo:1257–1331, 1397–1468`).

Game minimum is 10,000,000 lamports; Series A minimum is 50,000,000. Series A adds the full gross to pot and `floor(P*124/100)` entitlement; Series B adds `floor(loss*116/100)` entitlement without adding cash to pot. Top-ups preserve original seniority and prior credited/claimable amounts. Preserve one A and one B aggregate per native wallet.

Recommend an atomic rolling quota of three successful `open_position`/`fund_series_a` calls per owner per hour, storing the three most recent successes. A timestamp exactly one hour old expires. The legacy check-at-intent/record-at-credit split admits overlapping pending intents; tightening it is an explicit native admission difference. Claims, payouts, round progress and the separate ICP PP desk do not consume the quota. SOL source exposes `getMaxDepositLimit` but does not enforce its dynamic 20%-of-pot/5 SOL quote on admission; do not introduce that cap silently (`main.mo:1675–1677, 2973–3118`).

GUI defaults transaction fee payer to owner, and first-release personal/A rent payer is owner; a third-party network fee payer gains no financial authority. Costs remain outside gross/game payout. Sponsor pays shared/automatic-B rent by default, with explicit owner shortfall contribution/refund partitions in the defaults register. Remove fixed 5,000-lamport deductions and silent tiny-payout success. A positive funded lamport remains claimable; failed transfers do not consume it. Keep Backer-open cutoff `remaining > 10` without discarding funded dust (`main.mo:595–639,1483–1529`).

### Arithmetic vectors (lamports)

| Case | Gross/owed | Toll | Net |
| --- | ---: | ---: | ---: |
| 1 SOL Simple, one day | 110,000,000 | 13,200,000 | 96,800,000 |
| Simple total, one second before day 7 | 769,998,726 | 92,399,847 | 677,598,879 |
| Simple total, exact day 7 | 770,000,000 | 57,750,000 | 712,250,000 |
| Simple total, one second before day 14 | 1,539,998,726 | 115,499,904 | 1,424,498,822 |
| Simple total, exact day 14 | 1,540,000,000 | 46,200,000 | 1,493,800,000 |
| Simple total, maturity day 21 | 2,310,000,000 | 69,300,000 | 2,240,700,000 |
| Compound 15, maturity, 1 SOL | 4,473,565,759 | 402,620,918 | 4,070,944,841 |
| Compound 30, maturity, 1 SOL | 12,267,678,469 | 1,594,798,200 | 10,672,980,269 |
| Owed 100,000,003; pot 50,000,001; toll 1,200 bps | Actual gross 50,000,001 | 6,000,000 | 44,000,001 |

Simple total vectors assume no earlier claims. The 1 SOL entry partitions 40,000,000 management / 960,000,000 pot, with 1,000,000,000 accrual base. A 0.05 SOL A funding grants 62,000,000 entitlement. Sub-minimum 11-lamport helper input earns 1 lamport after a day under native math versus zero after legacy eight-decimal truncation; it is a precision vector, not an admissible deposit.

Native custody conservation at every phase (sum evaluated in U256 or checked u128): `vault_lamports - protected_rent = pot + seed + management + global_funded_claims + pending_toll + classified_donations`. Operations/provider funds reside in separate accounts. During partial allocation, pending credits and pending seed are subdivisions of pending toll, not additional liabilities in that equality. Commit subtracts toll from pending, adds credited total once to global funded claims, and adds seed destination once. Global funded claims equal raw local funded balances plus **finalized but unmaterialized** local pending credits. Lazy materialization changes only that local representation; a claim lowers global liability and physical lamports by the same amount. Reject a mismatch rather than reporting theoretical entitlement as cash.

## 2. Backer allocation — complete serialized batches

Recommended persistent state: one `Backer(owner, series)` PDA with entitlement, lifetime credited, funded claimable, pending ticket credit/ID, original first-funding sequence and active-registry slot. Permanent `User.has_ever_A/has_ever_B` bits preserve the source's existence-based EEC preference even after allocation membership is compacted. Do not erase history/existence when a capped Backer leaves the active registry.

Use a **dense active registry** in program-owned PDA pages. Between tickets, append on new/reopened Backer and swap-delete capped entries; update the moved Backer's slot atomically. Top-up of an existing open row does not duplicate membership. Retain the original first-funding sequence when reopening. Closed history remains in persistent records/mirror, but every toll need not scan all lifetime Backers. First-funding sequence provides deterministic native seniority/tie order; it replaces subsecond ICP timestamps explicitly. No arbitrary cross-wallet administrative merge is proposed for production; preserve ordinary same-owner funding merges.

Capture registry version and active length at ticket creation. Canonical page addresses, indices, per-row Backer PDA/slot cross-checks, exact next cursor and program ownership prove a complete traversal; callers provide accounts as transport. No off-chain chosen root is authoritative. Snapshot records are created only by program-verified traversal. A rolling digest can aid audit, but does not replace completeness checks.

Payout request preconditions include funded bounded-work/rent budget and no active ticket. Atomically debit actual gross from pot, pay net to the owner-authorized destination, update Position/UserRound, put the **whole toll** in pending allocation accounting and create one ticket. If toll is zero, skip allocation work. User payout is final separately from later Backer allocation. Do not allow cancellation or timeout to reclaim an already accepted toll into pot/management.

Ticket phases:

| Phase | Bounded action and invariant |
| --- | --- |
| Discover | Visit every captured active slot exactly once; snapshot initial A/B membership, open counts and remainders; find oldest open A |
| Senior | Credit the senior slice against cap, recording ticket-pending credit and advancing lifetime credited |
| SealWeights | Seal post-senior A denominator as discovered A total minus actual senior credit; derive each immutable row's weight from discovered remainder minus senior credit only for the senior row |
| Allocate | Process immutable rows; A pro-rata against sealed post-senior denominator, B equal per initial open B; credit caps and update pending totals |
| Prune | After allocations finish, remove newly capped registry members in bounded descending original-slot order; update swap-moved slots; preserve permanent existence bits |
| Commit | Check all cursors/counts and `pending_credits + seed_destination == toll`; atomically move accounting partitions and advance `last_finalized_ticket` |

Discover writes each row's immutable initial remainder. Only senior credit changes A headroom before proportional allocation, so SealWeights needs no third full scan or rewriting of all snapshots. Include a newly capped senior with zero weight, and a senior with 1–10 lamports remaining with that positive weight: do not re-filter original membership after senior credit. Discover/Allocate/Prune use cursor-bounded chunks; Senior/SealWeights/Commit are constant-size. Eight rows per chunk is an initial measurement candidate, not a guaranteed compute limit. Reject skipped, repeated, reordered or substituted rows. Replaying a completed chunk cannot re-credit it. Registry version stays frozen through allocation; Prune may advance it only through ticket-validated removals while the economics barrier remains active.

Split integer toll as follows: `seed0=floor(T/2)`; `H=T-seed0`; senior/A/B slices are individually `floor(H*35/100)`, `floor(H*45/100)`, `floor(H*20/100)`. Percentage remainder goes to seed. Fold absent B into A and absent A into B based on membership **before** senior credit. Without any open Backers, all toll goes to seed. Apply senior first; if that closes the only A, its still-present A pool has denominator zero and goes to seed, rather than being re-folded into B. Compute all A shares from the same sealed post-senior remainders. Per-row cap overshoot and all division residue go to seed, never redistributed opportunistically.

**Constant-size commit and lazy materialization:** The worker's draft finalizer merged every pending Backer balance; that would be unbounded. Instead, allocation chunks accumulate per-Backer pending credit and advance lifetime credited. Commit once moves total credited lamports from pending toll to global funded-claim accounting and advances a scalar finalized-ticket watermark. On later claim/top-up/next allocation access, a Backer lazily moves its previously finalized pending credit into its local funded balance. This local materialization does not increment global funded totals again. During an active ticket, only previously finalized credits are withdrawable. Each Backer carries at most one pending ticket; settle its previous finalized local credit before adding another ticket's credit.

Native `claim_backer(destination)` should retain the existing single-action combined A+B claim. Verify both expected owner/series accounts from persistent existence flags; users cannot substitute recipients or forget a pending liability. Claims lower funded local/global claim balances only; they never lower lifetime credited or alter allocation weights.

### Economics barrier and recovery

While allocation is active, reject A funding/top-up, other game payouts, affected term changes and registry membership mutations. Allow ordinary game deposits when the ticket is nonclosing; these change pot/management/Position/UserRound only, not frozen Backer weights. Reject deposits during closing. Funded claims, permissionless progress and segregated management stay available, including during an admission pause. No private keeper lease or original-keeper signature is needed to resume.

Capture worst-case Discover/Allocate/Prune row visits and constant/page phases, not guessed chunk counts. First release pays zero on-chain rewards: sponsor keepers/other payers pay network fees externally. Reserve shared storage/provider/automatic-B rent from a separate operations vault before payout/close; optional caller contribution has explicit maximum default zero. Insufficient reserve rejects before payout; later fee/budget issues cannot erase accepted toll liabilities. Winner need not sign for automatic B creation. No unreviewed cash abort or partial finalization; positive work-unit incentives are optional future measured policy.

Toll/operations rent pages must be prefunded or reusable before payout acceptance, with a size/phase-cost contract. Reusable snapshot pages can be overwritten only after their ticket is terminal and persistent credits are safe. No cleanup step may close Backer/User records needed for lifetime caps or EEC history. Operations funds, account rent and keeper rewards never come from pot/reserve/user claims implicitly.

The barrier preserves economic ordering but changes availability. Work is `O(active Backers)` total per toll, spread over bounded transactions; it is not constant total cost. No active population cap is assumed. Benchmark 64/256/1,024 active records and growth, keeper fees, interruption recovery and UI processing delays. If costs/latency fail the agreed launch envelope, stop and offer a measured capacity limit or different algorithm for approval; do not ship a silently capped or altered game.

Include frequent tiny withdrawals in that cost/availability assessment: subsidized batch work can cost more than the toll, letting a valid caller consume operations funds and keep the barrier busy. A zero toll requires no ticket; if all three rounded Backer slices are zero, direct the entire toll to seed without traversal under the same oracle. A minimum withdrawal/cooldown or population cap would be a new game rule requiring explicit acceptance, not an assumed fix or hidden gate. The batching recommendation does not claim resistance to all transaction-order/availability griefing.

### Backer vectors

All rows below use initially open remainders, oldest A1, and sufficiently large B remainders unless specified. They test the proposed integer oracle, not executed Motoko/SVM behavior.

| Toll and initial state | Seed | Credits |
| --- | ---: | --- |
| 10,000; A1=4,000, A2=6,000; two B | 5,001 | A1=2,363 (1,750 senior + 613); A2=1,636; each B=500 |
| 10,000; only A1=1,000, A2=10,000 | 5,750 | A1=1,000; A2=3,250 |
| 10,000; only three B | 6,751 | Each B=1,083 |
| 20,000; A1=2,000, A2=4,000, B1=500 | 13,500 | A1=2,000; A2=4,000; B1=500 |
| 10,001; no open Backers | 10,001 | None |
| 10,001; A1=4,000, A2=6,000; two B | 5,002 | A1=2,363; A2=1,636; each B=500 |
| 10,000; only A1=1,000 and B1 | 8,000 | A1=1,000; B1=1,000; zero post-senior A denominator goes to seed |

Each row conserves toll exactly. Claim-during-ticket test: a Backer with 100 already funded and 20 pending may claim 100; commit adds 20 to global funded accounting once; lazy materialization and next claim transfer only 20, without a second lifetime credit/global addition.

Rejected default options: generic deposit-weight index changes the senior/cap/denominator game; fixed-cap array changes admission capacity; caller-selected recipient lists cannot prove complete allocation. Custom cap-aware accumulators remain optional later optimization after an equivalence proof.

## 3. Round closing and Series B

Maintain one unique `UserRound(round, owner)` on-chain, registered atomically on that user's first game position in that round. Track gross game deposits and actual net game payouts across **all** positions, including closed/matured ones. Backer funding/claims are not game P/L. Loss is `max(0, deposits-payouts)`. Keep a canonical current-round paginated participant registry and persistent Backer existence bits; no historical game scan or frontend candidate list (`Promotion.mo:25–57`; `main.mo:903–943`).

An accepted insolvency-triggering owner payout records one `close_started_at` and deadline `+600 seconds`, then follows `Active → AllocatingClosingToll → CountingCandidates → AwaitingRandomness → PromotionApplied or PromotionSkipped → ResetReady → Active(next round)`. The deadline covers candidate processing and request creation as well as oracle delivery. Round ID/phase validity closes old positions without updating all old accounts; retain close timestamp/history. Next-round funding waits until closing cash allocation commits and promotion is terminal, so source membership/order cannot change mid-close. Ordinary allocation tickets do not block new game deposits.

Zero-pot closure must be backed by an owner-authorized valid positive-earnings Simple claim or matured compound claim, with `minimum_receive=0`; no permissionless timer closes an initial empty pot or overrides the source equality boundary. Minimum-receive failure atomically reverts, and the owner can explicitly retry the zero-payout close action. A zero-toll close needs no allocation ticket. A zero-earnings ordinary claim can close a matured Simple position without division by zero or inventing a round reset.

At close, freeze round membership and P/L. Bounded traversal creates program-owned dense preferred/fallback candidate lists. Preferred means loss >0 and **neither A nor B entry has ever existed in this native game**, including closed Backers; fallback is all net losers. Seal exact lengths/digests after every participant is processed once. Select preferred if nonempty, otherwise fallback. Empty lists require no randomness and move directly toward reset. Users with legacy Backer records are not imported into native membership.

Bind a single request to the sealed round/list, program/cluster domain and unique close ID. Protocol seed is `SHA256(ASCII("SchemeWorks/MC/EEC/v1") || cluster_domain_32 || native_program_id_32 || round_id_u64_le || close_id_u64_le || selected_set_tag_u8 || selected_count_u64_le || sealed_candidate_digest_32)`, with fixed-width encoding and an immutable genesis cluster-domain hash. Keepers cannot supply another seed.

Recommend the authenticated client-scoped provider interface below. Round states are `DrawNotRequested → DrawRequested → DrawCaptured`, each monotonic. `request_round_draw` derives the exact seed/request PDA, checks the registered provider client/config, CPI-signs only with the native program's RNG-authority PDA, and atomically stores seed/address/request marker with successful provider creation. A failed CPI leaves both uncreated; a delivery retry after success looks up that same marked request rather than calling create again. If an account unexpectedly exists before the marker, fail closed on that seed and expose the collision; no alternate seed or arbitrary prefulfilled account is accepted. A protocol instruction accepting arbitrary proxy seeds must not exist.

Capture requires exact provider ownership, client-scoped PDA, stored seed/client/config and actual fulfilled state, with execution-time Clock at or before the immutable deadline. Copy output once into the Round; duplicates are idempotent. Timely capture may apply later and cannot be deadline-skipped. Without timely capture, public expiration is permitted only when Clock is strictly greater. Late authentic callbacks are no-ops. Use `r = u64_le(SHA256(ASCII("MC/EEC/INDEX/v1") || output || round_id_u64_le || counter_u64_le)[0..8])` and `limit=floor(2^64/N)*N` in u128. Consume counters 0–63, choose first `r < limit`, then ordinal `r % N`; continuation keeps the same output/counter and cannot bypass success. All 64 failing maps to terminal `PromotionSkipped(EntropyExhausted)` with no grant/new draw. This bounded cryptographic expansion removes modulo bias among accepted results, with disclosed rare omission; selected candidate uses direct sealed-list lookup.

Applying promotion creates/merges the selected owner's B entitlement `floor(loss*116/100)`, preserving existing chronology and lifetime credit; adds no cash and marks promotion terminal. After cash allocation commits and promotion is terminal, permissionless finalization carries seed exactly once into new pot, resets round-local totals/active counts, advances round ID and opens the next round. Backer claims, caps, management balance and position IDs/history survive. Guard promotion and seed carry separately against replay.

### Randomness recommendation and outage tradeoff

Prefer a **durable, client-authenticated fulfilled result**. ORAO's generic [integration guide](https://github.com/orao-network/solana-vrf/blob/master/README.md) shows seed-derived requests, and [provider documentation](https://orao.network/solana-vrf) describes quorum fulfillment. Those alone do not solve third-party seed precreation.

Recommend evaluating **ORAO's registered-client interface (Callback VRF)** first. Its [official client guide](https://github.com/orao-network/solana-vrf/tree/master/callback) documents State-PDA-signed request CPI, a seed namespace per registered client, and request PDA derivation including client address and seed. Client registration requires program upgrade authority, adds a separately funded fee/rent account, and client ownership can change callbacks/withdraw funds; disclose those governance powers. Use a fixed canonical RNG-authority State PDA and registered Client, no arbitrary seed proxy, and protect the pending request/account from closure. A callback should only capture verified output; alternatively permissionless polling may consume the same retained fulfilled record if the pinned SDK proves that lifecycle. Never let the oracle choose a candidate or perform an unchecked financial callback. Verify source/version, quorum/upgrade powers, retention/recovery and live identity before approving this adapter. No deployment or provider integration has been proved.

Switchboard On-Demand is an alternative with an explicit TEE/oracle trust model ([overview](https://docs.switchboard.xyz/docs-by-chain/solana-svm/randomness.md)). Its [tutorial](https://docs.switchboard.xyz/docs-by-chain/solana-svm/randomness/randomness-tutorial) and [SDK source](https://docs.rs/switchboard-on-demand/latest/src/switchboard_on_demand/on_demand/accounts/randomness.rs.html) show request binding and a current-reveal-slot `get_value` requirement. A compatible adapter would need to capture verified output into Round state in the reveal transaction; a late multi-transaction candidate traversal cannot repeatedly rely on that freshness window. Do not copy a tutorial's unchecked account parsing as authority validation.

Fresh-attempt/two-slot rerolls remain rejected. Under Rob's subsequent availability requirement, recommend **one immutable 600-second promotion deadline**, no new seeds, and permissionless `PromotionSkipped(Deadline)` if no valid output was captured in time. Retries never extend it. Candidate-only partial work can be abandoned after expiry; cash allocation cannot. After cash commit and terminal promotion, carry the full seed once and open the next round. Public expiration/finalization or a composed next deposit can reopen without the original keeper. This bounds oracle eligibility, not chain/transaction availability or unbounded financial work latency.

This policy resembles current source skip-on-`raw_rand`-failure (`main.mo:944–951,1001–1006`) and supersedes the earlier indefinite wait. Selective withholding/censorship can force no promotion: availability is prioritized, and no-reroll does not make omission unbiased. No fallback winner is chosen. No-candidate rounds still close without a request normally. See the companion for exact boundary, late callback, budget and permission rules.

## 4. What the implementation plan now needs to prove

- Exact oracle vectors, repeated-claim remainder carry, tier/maturity boundaries, integer insolvency, numeric bounds and the source/display divergences.
- Complete bounded traversal and snapshot sealing; senior-first weights; no rerouting of capped A pool to B after senior closes A.
- Constant-size global commit and lazy funded balances; combined A+B claim; no double credit/global funding.
- Active registry compaction/reopening with stable seniority; permanent existence flags; no closed historical scan on every payout.
- Progress interruption/replay, replacement/self-funded one-row keepers, work-unit/rent prebudgeting, optional caller contribution maximum, narrowed admission permissions and measured population costs.
- Strict insolvency/equality and owner zero-pot trigger; complete current-round P/L, candidates, exactly-once promotion/seed carry, no early next-round funding.
- Provider-specific proof of registered-client authority/namespace, exact seed/atomic marker, capture Clock deadline, persistent result, collision recovery, no reroll and disclosed selective-withholding risk; public expiration and no late promotion.
- Existing GUI with payout-finalized/allocation-processing/round-closing statuses, correct gross-base projections and source split; direct funded claims accessible throughout.

These are proposed tests/contracts for this design, not tests already passed. Checked so far: source references, high-precision Decimal constants and standalone integer allocation/conservation examples. No Motoko oracle harness, property test, program code, SVM, devnet or live-state checks have run.

Independent native review accepted this direction and the recomputed vectors, with two material clarifications. Coordinator addressed width/conservation using explicit u64/u128/U256 bounds and pre-admission future-promotion checks; addressed randomness initiation with exact seed/marker/collision rules and the authenticated client-scoped interface. The same reviewer then confirmed both findings resolved at the design-document level with no concrete residual flaw in those areas. Runtime cost, operations-budget griefing and actual provider lifecycle remain unproved.

Proposed durable persona lessons (not applied to catalog/memory): source gross-base accrual can disagree with ROI previews; senior credit changes subsequent proportional weights; bounded chunking still needs a bounded final commit; closed Backer history and active allocation membership have different roles; retrying randomness with fresh seeds can bias a promotion. These are design/source lessons, not production readiness evidence.
