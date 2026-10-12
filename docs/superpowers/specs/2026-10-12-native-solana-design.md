# Musical Chairs native Solana — discovery specification

Date: 2026-10-12. Status: **draft for discussion, not an approved execution specification**.
Source inspected initially on `main` at `f96b33ff95020458be754bfa0d7471b72a7403af`; documentation branch: `codex/native-solana-discovery-20261012`. Source files are unchanged on that branch.
Companion: [implementation roadmap](../plans/2026-10-12-native-solana.md), [discovery evidence](../../coordination/native-solana-discovery.md), [team board](../../coordination/kanban.md).

## 1. Product brief

Rob wants Musical Chairs' financial logic on Solana, with the frontend continuing to be hosted on ICP. The existing look, feel, navigation, terminology, and major screens should remain recognizable. Minor changes needed for wallet transactions and accurate chain status are in scope; a visual overhaul is not.

Success for the SOL game means a Solana wallet signs program instructions, the Solana program controls the pooled SOL and financial state, and a user can verify a deposit, payout, or Backer claim from Solana accounts and transaction receipts. ICP must not be the authority that decides or signs ordinary native SOL payouts.

This document recommends a target architecture and records the decisions needed to freeze it. Research and draft planning are authorized. Product code, dependencies, deployments, operational changes, and fund movements have not started.

**Confirmed scope, 2026-10-12:** Rob selected a fresh launch with new SOL deposits and separate legacy withdrawals, and SOL financial game first with PP and Shenanigans staying on ICP. No existing position, balance, Backer entitlement or reserve is imported into the native pool. ICP hosting and the current GUI remain in scope to preserve.

## 2. What exists today

The current SOL game is **Chain Fusion**, not a native Solana program. `ponzi_math_sol` is Motoko on ICP (`dfx.json:11–13`). It holds the financial records, derives threshold-controlled deposit/pool addresses, observes inbound SOL transfers, and signs outgoing payouts (`ponzi_math_sol/main.mo:788–825, 1114–1188, 2973–3045`). No native Rust/Anchor program was found in the inspected checkout.

The frontend already has Solana wallet integration, panels, SOL units, and ICP asset hosting (`package.json`, `dfx.json:29–33`). SOL hooks still read/write an ICP actor (`frontend/src/hooks/useQueries.ts:2447–2695`). Wallet transaction code currently submits a plain transfer and intentionally leaves credit recognition to the ICP observer (`frontend/src/solana/sendSolDeposit.ts:92–104`).

The May 23 Anchor proposal was a thought experiment. The May 25 Chain Fusion design explicitly superseded it. Its feature omissions, randomness design, and old economics are not accepted requirements for this migration. The old premortem describes hypothetical failures, not observed production incidents.

## 3. Approaches considered

| Approach | Result | Main tradeoff | Recommendation |
| --- | --- | --- | --- |
| Native SOL program; ICP hosting and social services | SOL custody/accounting native; PP remains an ICP financial rail | Preserves the shared social game, but PP sales and rewards stay cross-chain | Selected first-release scope |
| Native SOL program plus native PP financial rails | SOL and PP ownership/transfers/financial rules move to Solana; ICP can host and serve nonfinancial social content | SPL token migration, reward rules, chip custody, purchases, burns and spell effects substantially expand work | Plan as a separate dependent workstream if requested |
| Continue ICP Chain Fusion and improve transaction UX | Smallest technical change | Financial authority remains ICP | Does not satisfy the requested native SOL program goal |

Anchor/Rust is the recommended program stack for initial development because it provides account validation and an IDL. Pinocchio is optional optimization if measurements later justify it. An ICP history mirror is useful for preserving existing screens, but is not required for users to withdraw SOL.

## 4. Authority and component boundaries

```mermaid
flowchart LR
    H[ICP asset hosting] --> UI[Existing React interface]
    UI --> W[Solana wallet]
    W --> P[Native Solana program]
    P --> A[Pool, Positions, Backers, Rounds]
    UI --> R[Solana RPC: canonical financial reads]
    A --> R
    R --> M[Optional ICP finalized-event mirror]
    UI --> M
    UI --> S[ICP profiles, chat and social services]
```

- **Solana:** financial configuration, deposited SOL, position state, payout eligibility, fee/reserve allocation, Backer claims, round transitions and promotion entitlements.
- **Browser:** typed instruction building, simulation, wallet request, submission, chain confirmation/finality and canonical account reads. Display projections cannot authorize payment.
- **ICP assets:** retain existing certified static frontend hosting and domain. No hosting-platform migration is proposed.
- **ICP social services:** PP, Shenanigans, profiles, chat and referrals remain on ICP, accessed through existing SIWS identities. A SIWS delegation is not authority to spend native program funds.
- **Optional ICP mirror:** finalized history, feeds, analytics and social reward inputs. No private pool key, balance repair, payout authorization, or off-chain chosen promotion winner. Its ingestion is a separately disclosed trust boundary, especially if it triggers PP issuance.

Solana transactions can combine account mutation and SOL movement atomically; transaction fees may still be charged on failure. PDA-based custody removes the need for an ordinary private pool key. See [Solana transactions](https://solana.com/docs/core/transactions), [PDAs](https://solana.com/docs/core/pda), and [Anchor account constraints](https://www.anchor-lang.com/docs/references/account-constraints). Keeping the browser app on ICP is compatible with this design; see [ICP application architecture](https://docs.internetcomputer.org/getting-started/app-architecture/) and the [official documentation index](https://docs.internetcomputer.org/llms.txt).

## 5. Economic behavior to freeze

These are current **source rules**, not a verification of deployed state. The proposed default is to preserve the SOL game's intended rules, with explicit approval of material differences.

| Feature | Current source behavior | Native target / decision |
| --- | --- | --- |
| Assets | Separate ICP and SOL pools | Recommend native SOL first. No stablecoin, bridge or asset conversion is assumed |
| Plans | Simple: 11% daily, 21 days; compound: 12% daily, 15 days; compound: 9% daily, 30 days | Preserve rates, durations and payout meaning |
| Deposit amount | Position records gross deposit; pot receives amount after entry fee | Preserve; do not accrue on the net amount accidentally |
| Front-End Load | 4% gross; tracked management balance | Preserve 400 bps, segregated from spendable pot |
| Simple Carried Interest | 12% before day 7; 7.5% from day 7 to before 14; 3% from day 14 | Preserve exact boundary comparisons |
| Jackpot Fee | 9% of 15-day earnings; 13% of 30-day earnings | Preserve |
| Principal | Earnings formulas pay `amount × rate × time` or `amount × ((1+rate)^days−1)`; no separate principal refund in settlement | Preserve unless expressly redesigned; UI must not imply principal is guaranteed/redeemable |
| Toll allocation | 50% seed reserve; other 50%: 35% senior A, 45% proportional remaining A entitlement, 20% equal per open B | Recommend source behavior; frontend constants still say 35/25/40 |
| Missing/capped tiers | Fold absent tier pool to other tier; senior without A and capped overshoot go to seed | Preserve total conservation and ordering |
| Series A | Gross funding enters pot, no Front-End Load; entitlement `amount × 1.24` | Preserve; 0.05 SOL minimum in current SOL path |
| Series B | Current-round aggregate loser promoted; entitlement `loss × 1.16` | Preserve intended eligibility and bonus; randomness and bounded selection need design |
| Position minimum | 0.01 SOL | Recommend preserve 10,000,000 lamports |
| Rate limit | Three position openings per hour, shared with self-serve backing | Preserve only with exact source applicability fixed in parity matrix |
| Insolvency | If gross earnings exceed pot, scale payout/toll to pot, close and reset; zero pot resets without payout | Preserve intended partial-payout behavior; no global pro-rata guarantee is implied |
| Round reset | Carry seed into new pot, invalidate old active positions, retain histories and Backer claims | Use round-ID validity instead of touching all historical positions |
| Rounding | Float SOL values truncate to eight decimal places, then convert to lamports | Integer lamports and checked wide intermediates; choose compatibility at 10-lamport precision vs intentional 1-lamport upgrade |
| Payout fees/dust | Canister deducts 5,000 lamports; sometimes skips tiny transfer while updating accounting | Recommend wallet fee payer, fees outside game payout, and no silently lost claim. These are explicit behavior changes |
| Alternate payout destination | Caller can specify a SOL address for simple/compound payout and Backer claim | Preserve owner-selected destination, default to owner wallet; require wallet signature and explicit transaction review |
| Manual deposit credit and observer recovery | Canister operator can recover/credit unmatched legacy transfers | Legacy-only. Native program deposits are atomic; unsolicited transfers need an explicit refund/donation policy and cannot create discretionary positions |
| Desk proceeds and refunds | Canister has separate proceeds withdrawal and refund routes | Preserve the existing ICP/Chain Fusion PP desk as a separate rail. Native desk replacement is deferred; never mix desk proceeds with native game funds |

Economic references: `ponzi_math_sol/main.mo:532–589, 595–785, 903–1108, 1226–1453, 2443–2454, 2782–2834, 2973–3080`; eligibility helper `ponzi_math_sol/Promotion.mo:25–57`.

Numeric example for proposed exact-lamport accounting: a 1 SOL position deposits 1,000,000,000 lamports, allocates 40,000,000 to management and 960,000,000 to pot, but accrues on 1 SOL gross. One simple-plan day earns 110,000,000 lamports; early Carried Interest is 13,200,000; user payout is 96,800,000. Of the toll, 6,600,000 seeds the next round; the Backer half initially divides 2,310,000 senior / 2,970,000 A pool / 1,320,000 B pool before caps and absent-tier folding. Wallet network fees and rent are separate. This example assumes a solvent pot and both Backer tiers available.

The promised rate formulas are game obligations funded from the pool, not external investment yield. Future scheduled earnings and maximum Backer entitlements are not present liquid reserves. A chain migration does not change the game's insolvency risk or transaction-order sensitivity.

## 6. Program design sketch

This is a candidate account/interface map; seeds and schemas become frozen contracts after the bounded-allocation design is proved.

| Account | Purpose | Candidate seeds / invariant |
| --- | --- | --- |
| Config | Program schema, authority roles, plan schedule, allowed changes | `config`; one canonical instance, authenticated initialization |
| Pool and custody vault | Spendable pot, accounted partitions, physical lamports | `pool`, `vault`; rent reserve excluded from spendable funds |
| Position | Owner, nonce/ID, plan, gross amount, times, withdrawn, round | `position, owner, client_nonce`; signer owns action |
| UserRound | Aggregate gross deposits and actual game payouts | `user_round, round, owner`; includes closed positions, excludes Backer claims from game P/L |
| Backer | Series A/B amount, entitlement, credited-to-date and claimable | `backer, owner, series`; no credit beyond cap, claim does not erase lifetime credited |
| Backer registry/queue | Complete active set and senior-A order | Bounded, verifiable membership; no omitted-account bias |
| Round / candidate pages | Round state and complete promotion candidate set | `round, round_id`; each participant processed once with completeness proof |
| Management balance/vault | Withdrawable Front-End Load | Segregated accounting; cannot sweep pot, reserves or user claims |

Candidate instructions: `initialize`, `open_position(plan, gross_lamports, client_nonce)`, `withdraw_simple(position, minimum_receive_lamports, destination)`, `settle_compounding(position, minimum_receive_lamports, destination)`, `fund_series_a(gross_lamports)`, `claim_backer(series, destination)`, bounded round-close/selection/finalization steps, and narrow pause/config/management instructions. Destination is a transaction account selected by the owner. New deposits should use a program instruction that transfers SOL and creates state in the same transaction. A plain transfer to an address must never be credited as a native position merely because an observer sees it.

Prefer owner-wallet destinations for ordinary payouts. Supporting a different destination requires owner signature and explicit UI review; recipients must never be selected by an indexer. A client nonce makes an opening uniquely addressable and permits recovery from unknown submission results. Program accounts enforce ownership, seeds, account types, signer privileges, and allowed writable aliases.

## 7. The two design problems to solve before freezing core instructions

### Backer distribution

The current source scans all open Backers on every toll and applies the senior bonus **before** recomputing the A pool's remaining-entitlement weights. A generic deposit-weighted reward index is not automatically equivalent. Cap crossing, membership changes, senior rotation, and absent-tier folding make this a semantic problem, not just a performance optimization.

Investigate (1) a proven constant/bounded-cost accumulator plus senior queue and (2) complete, bounded allocation batches with financial funds reserved until allocations finalize. Compare exact results, liveness and user friction. A small capped registry is an alternative only if Rob accepts an explicit new capacity limit. Do not silently flatten the fee split, remove seniority, defer Backers, or trust caller-selected recipients. Full core parity remains the target; a deposit-only prototype is not the complete migration.

The proof deliverable must define denominators after senior credit, credited-but-unclaimed treatment, close/cap handling, new funding merge behavior, rounding sink, queue maintenance and worst-case costs. Until then, backer reward indexes are an investigation, not an accepted implementation.

### Round closure and Series B

Use an on-chain round transition to invalidate old positions without a whole-history scan. Maintain per-user round P/L incrementally. Freeze the candidate set before requesting randomness; include net losers from all current-round positions and the existing no-Backer preference/fallback. A bounded registry traversal must prove no eligible player was omitted or processed twice.

Recommend a verifiable randomness integration rather than slot hashes, timestamps, operator choice, or caller-selected slots. Provider, costs, request/fulfillment ordering and timeout recovery remain decisions. Current source catches `raw_rand` failure and resets without a Series B grant (`main.mo:944–951, 1001–1006`). D7 must choose retry with frozen candidates versus a disclosed close-without-promotion policy, including timeout and user-visible recovery. Do not promise one promotion in every round; permit no-candidate rounds and the accepted outage result. No fallback may let a keeper choose the winner. Resolve whether new rounds may start while prior promotion is pending, and ensure pending draws cannot freeze withdrawals of already reserved claims. This is an intentional async replacement for ICP `raw_rand`, not identical infrastructure.

Keepers may advance verifiable state machines and pay transaction fees; they must not invent balances, candidate lists or payment authorizations. Availability, retries, chunk limits and incentives need measurement. An ICP keeper is optional operations infrastructure, with no custody key.

## 8. Financial conservation and governance

Required proposed invariant: the physical custody lamports, less the protected rent floor, equal the sum of tracked spendable pot, seed reserve, funded unclaimed Backer allocations, management balance, and any explicit pending allocation escrows, plus a separately classified unallocated donation/dust balance. These partitions are disjoint; moving value between them is not new revenue. Future ROI promises and uncredited Backer entitlement do not enter that equality. Legacy liabilities and ICP desk proceeds remain outside the native pool.

Use `u64` for transferable lamport amounts, checked `u128` for intermediate products and appropriate aggregates, and explicit overflow failures. No floats in program accounting or native transaction amounts. Fractional-day compounding displays need a reviewed approximation; maturity payouts can use reproducible fixed-point maturity factors. Pin bit-for-bit oracle outputs and any accepted divergence from legacy Float behavior.

Unsolicited SOL must have an explicit treatment; it must not silently increase pot or management revenue. Every payout/claim updates state and transfers funds atomically. Small legitimate claims must remain tracked until paid or be handled under an expressly approved dust rule. Partial payout and minimum-receive checks need an explicit zero-pot/round-reset recovery path rather than a transaction that only reverts and leaves the round stuck.

Proposed rent contract to freeze with account schemas: users pay rent for their Position/Backer/UserRound accounts separately from gross game deposits; an operator-funded operations budget pays Config, vault and registry/candidate-page rent and keeper costs. All account-specific rent-exempt minima are protected, determined from actual allocated size at the selected cluster, and excluded from pot/reserve/management partitions. Record original rent payer/refund destination in state. Closure requires completed liabilities and finalized round work; refund rent only to the recorded payer, without counting it as game yield or management proceeds. Candidate-page creation must have a funded budget so an otherwise valid round cannot stall for missing storage rent. If different sponsorship or shared-account ownership is selected, explicitly update this contract and tests before freezing the design. No payer, cost or subsidy is yet authorized.

Proposed admin model: separate program upgrade authority, config authority, pause authority and management recipient controls. Recommend multisig upgrades, narrow deposit pause preserving already funded claims, versioned schedules for new positions, and no production test-credit, backdate, arbitrary-balance, or god-mode reset instructions. Names, thresholds and upgrade/timelock policy require Rob's decision. Renouncing upgrade authority is optional and separately irreversible.

## 9. GUI and client preservation

Retain `App`, `Dashboard`, `GamePlans`, `GameTracking`, `HouseDashboard`, `BankPage`, Hall of Fame, Live Feed, Shenanigans and Trollbox presentation and existing styling. Adapt financial hooks behind a typed native service; do not mix ICP principal IDs with Solana public keys or imply balances are merged.

New native flow inside existing panels: estimate → simulate → wallet approval → submitted with signature → confirmed → finalized → optional history mirrored. Reads of positions and withdrawability come directly from Solana; mirror lag cannot block a valid native withdrawal. SIWS remains optional for ICP social access, separate from wallet financial authorization.

Minor UI changes: cluster indicator, real wallet fee/rent estimates, transaction/explorer link, expiry/retry/rejection states, explicit minimum receive, stale-history indicator and correct native balances. Remove observer-credit, derived-deposit-address and manual-transfer fallbacks for native positions. Preserve legacy recovery screens only on the legacy rail.

Keep existing wallet adapters where they work; isolate their legacy SDK types. Prefer current Solana Kit tooling for new client code where compatible, but a framework-wide wallet/SDK rewrite is optional groundwork. Pin compatible tool versions during implementation, after testing the existing React/Anchor/client combinations. Preserve the prohibition on direct `@rollup/rollup-darwin-*` dependencies.

The current asset CSP permits selected HTTPS RPCs but the sender explicitly avoids WebSocket confirmation. HTTPS status polling is a viable first design; any new HTTP/WSS origins need an explicit allowlist and browser proof (`frontend/public/.ic-assets.json:19`, `sendSolDeposit.ts:96–103`).

## 10. PP, Shenanigans and the desk — explicit scope boundary

PP is an ICRC token owned by ICP principals, including SIWS-derived principals. Shenanigans currently issues deposit/Backer rewards by observing the two financial canisters (`shenanigans/main.mo:1400–1457`). PP wallet/chip transfers and spells are ICP-authoritative; the Founder's Allocation desk sells ICP PP for SOL (`ponzi_math_sol/main.mo:322–407, 3283–3547`).

**Selected: PP stays on ICP.** Add a deduplicated finalized-event consumer for the native SOL game alongside the existing legacy reward observer. Use the canonical SIWS-derived principal for the event's actual signing/position-owner pubkey; validate signed domain, challenge nonce, expiry and chain context. Operators cannot choose reward recipients. Event entitlement IDs include cluster/program ID, signature, instruction/event index and schema version. Rebinding cannot move historical rewards or remint an event; any recovery/linking mechanism requires proof of both identities, a recorded versioned policy and separate review. Keep SIWS domain/session protections. Native SOL ownership must not depend on SIWS availability. Social effects may modify ICP PP rewards, but must not modify native SOL entitlements. Describe this honestly as native SOL game plus ICP PP/social services.

The SOL↔ICP PP desk cannot become atomic simply by porting its SOL half. Preserve its existing ICP/Chain Fusion purchase, proceeds and refund flow as a clearly identified separate rail, accessible through the existing GUI and SIWS session. It does not deposit into or withdraw from the native game pool, and desk transactions must not create native Positions or share native game accounting. A native desk or new cross-chain escrow protocol is optional future work, deferred from this first release. Existing desk availability and trust assumptions must be accurately represented; hiding or silently removing the desk is not feature parity.

**Deferred: native PP.** A future native PP project would need a separate specification for mint identity/supply, existing balances and chip custody, transfer/burn/mint authority, deposit rewards, three-level referrals, desk inventory/proceeds, spell costs/effects and payout claims. It is outside this release and is not a prerequisite for the native SOL game. Avoid two independently mintable tokens presented as one balance.

II/SIWS account linking or merging is optional. Neither frontend wallet switching nor a matching display name establishes ownership of both identities.

## 11. Launch and legacy state

**Selected: fresh launch.** Launch a new SOL pool with new deposits, without importing legacy liabilities. Preserve separate legacy position displays, withdrawals, settlements, Backer claims and recovery paths; clearly label their rail and source of funds. Existing legacy positions do not become native Positions when the same wallet connects. This selection is not permission to pause or retire any existing canister.

Balance imports, snapshot claims, liability conversion, bridges and legacy-to-native reserve transfers are outside this release. Check actual deployed legacy withdrawal/settlement/recovery capabilities before promising a withdraw-only mode. Any later deposit pause or retirement requires separately authorized operational changes and must preserve recovery. Native launch capital is a separate funding decision; it must not be assumed to come from legacy pots, PP balances or desk proceeds.

No live state, active-user count, legacy solvency, deployed module bytes or current balances were checked in this discovery. Separate legacy withdrawals describe preserved access, not guaranteed liquidity or verified payout capability. Native and legacy launch readiness cannot yet be established from production evidence.

## 12. Decisions and proposed readiness criteria

| Decision | Recommendation / unresolved item | Affected work |
| --- | --- | --- |
| D1 Launch | **Confirmed by Rob:** fresh pool; legacy positions withdrawable separately; no imports | Separate legacy access and native launch funding |
| D2 PP | **Confirmed by Rob:** SOL financial game first; PP and Shenanigans stay on ICP | Native finalized-event rewards; existing desk remains a separate rail |
| D3 Assets | Native SOL first; historical no-stablecoin choice must not be presumed newly accepted | Units, vaults and interface |
| D4 Economics | Source 35/45/20, all three plans, full Backer/EEC feature parity | Oracle and screens |
| D5 Rounding/fees | Lamports; wallet pays network fees; legitimate dust retained | Intentional parity differences |
| D6 Bounded distribution | Exact proven accumulator or complete bounded batches; no silent simplification | Account schema and payout instructions |
| D7 Promotion | Complete frozen candidate set and verifiable randomness; provider/failure rules open | Round state machine |
| D8 Control | Multisig upgrade authority; narrow config/pause; no arbitrary balance repair | Governance/deployment |
| D9 Operations | Direct canonical reads; optional lag-aware mirror; permissionless verifiable keepers | UI liveness and history |

D1/D2 are settled product scope. Before the production implementation plan is declared frozen, turn D3–D9 into reviewed contracts and measured feasibility evidence. These are proposed acceptance criteria for this requested migration, not previously accepted review findings. Unrelated refactors, account linking, hosting migration, native PP/desk replacement, marketing and SDK-wide upgrades are optional backlog.

Proposed later release evidence: economic oracle and randomized conservation tests; signer/account-substitution tests; complete bounded allocation/round-close proofs under worst-case populations; failed transaction and replay tests; rent/compute measurements; stale-RPC/indexer and randomness outage recovery; matching generated IDL/client; devnet wallet receipts and balances; unchanged visual baseline across major screens; independent fund-flow review; exact reviewed artifact and program authority verification. Mainnet deployment and asset funding require a separate explicit go decision.

## 13. Recommended immediate next step

Finish the decision sheet and freeze a **core economic oracle plus bounded Backer/round-close design** before writing the production program. The roadmap sequences that work first, followed by program, client/GUI adaptation, PP/social integration, then verification and separately authorized launch. This keeps the financial rewrite focused and the familiar interface intact.
