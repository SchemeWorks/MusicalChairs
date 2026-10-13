# Processing and randomness recovery recommendation

> Update 2026-10-12: Rob accepted the defaults and authorized only the Rust economic model plus local provider/resource probes. [Acceptance and scope](../solana/launch-decisions.md). Earlier discovery status/evidence paragraphs are historical; no deployment or live financial action is authorized.
Date: 2026-10-12. Status: recommended design, not approved implementation or measured performance.
Design-pass baseline: documentation commit `9327a53ee6cff11a0f2ec5fad0dcb8e49a5c5203`; financial source still `f96b33ff95020458be754bfa0d7471b72a7403af`.

Rob explicitly rejected indefinite stuck rounds. This supersedes the prior recommendation to await the same oracle request indefinitely. Native Luna workers SOL-09/SOL-10 examined pauses/outages; SOL-11 assessed skills. Coordinator integration and independent review are recorded on the [board](kanban.md). During that availability pass no product code, skill installation, deployment, signing or funds were changed.

Subsequent authorized [defaults pass](native-solana-defaults.md): Foundation skill refreshed; first-release on-chain keeper reward is zero. Sponsor pays shared storage/provider/automatic-B reserve, and sponsor-run keepers or any caller pay their own network fees. Finite captured work and protected on-chain reserve still apply; no product implementation started.

## 1. Recommended behavior

- Preserve accepted SOL payouts and every funded/pending Backer allocation.
- Use an inline complete allocation when measured account/compute limits permit it; otherwise resume bounded permissionless chunks. No exclusive keeper lease.
- Allow new game deposits during ordinary nonclosing allocation. Pause only Backer membership/funding, subsequent game payouts and affected financial configuration. Closing rounds reject new deposits.
- Keep funded claims, progress and segregated management accessible during either processing phase and during an admission pause.
- Give each closing round one fixed **600-second promotion deadline** from accepted close. After it expires without a timely captured result, anyone can skip promotion and finish reset/seed carry once cash allocation has committed.
- No rerolls, caller-selected winner, alternate-seed collision fallback, or late promotion.

The deadline removes a mandatory oracle wait, not the need to submit transactions or finish already accepted cash accounting. No program can guarantee a ten-minute wall-clock completion during a chain halt, lack of transaction submission, or arbitrary unbounded processing population. Recovery must be executable without admin approval or the original keeper.

## 2. Narrow processing permissions

| Operation | Ordinary allocation | Closing round | Admission pause |
| --- | --- | --- | --- |
| Open game | Allow; gross/net entry accounting and UserRound append only | Reject direct open; finalize-then-open may be composed only when eligible | Reject |
| Fund/top-up A; discretionary registry mutation | Reject | Reject | Reject |
| Another game withdrawal/settlement | Wait for previous ticket | Reject | Allow when round Active and no prior ticket; admission pause alone does not block exits |
| Claim finalized A/B balances | Allow | Allow | Allow |
| Advance allocation; prune; commit | Allow any payer | Allow any payer | Allow |
| Build/capture/expire promotion; terminal round finish | Not applicable | Allow per deadline/state rules | Allow |
| Segregated management withdrawal | Existing narrow authority; disjoint balance only | Same | Independent of admission pause |
| Change terms affecting active ticket/close | Reject | Reject | No recovery override |

A normal deposit cannot create a Backer or touch frozen ticket weights/registry. Its pot/management/P&L updates linearize after the accepted payout, so old toll allocation is unchanged. Closing admission is sealed before candidate construction. Claims lower previously finalized liabilities only, not current pending credit or entitlement weights.

New control names should reflect an admission pause, not a global mutex that disables recovery. Management cannot sweep pending toll, funded claims, reserved work escrow or seed. Before claiming completion, test each permission against actual touched accounts, not just UI disabled states.

## 3. Finite work and executable processing recovery

Capture `N=active Backers`, registry version, phase/cursors and the resource schedule before accepting payout. For the recommended Discover/Allocate/Prune passes, reserve worst-case **three row visits per member plus constant phase work**; extra snapshot/page creation and swap-delete accounts are accounted separately. Each phase only advances its exact cursor. New normal game deposits do not alter captured N. Prune may remove up to N members; it cannot append fresh ones.

Capture work units rather than budgeting for `ceil(N/8)` calls and allowing one-row calls to exhaust that estimate. First-release reward price is zero: keepers/callers pay their own network fees, with no on-chain progress reward. Reserve shared storage/provider costs separately, independent of chunk grouping; preserve finite row/constant/page bounds. If paid rewards are later accepted, price only verified new units once, never replay/no-progress. Return unused storage/provider reserve to its recorded sponsor/contributor after terminal completion, not pot revenue.

Inline allocation is a measured optimization, not a population cap: test N=0/1/16/17 as candidate boundaries, then select the actual threshold separately for supported transaction formats. Eight rows per chunk is only an initial candidate. Expose `advance_ticket(ticket, rows)` with a proven one-row fallback; callers can reduce chunk size when simulation/resource limits require it. Never publish unmeasured CU/latency guarantees.

Funding recommendation: reserve shared storage/provider/automatic-B costs from a separate sponsor operations vault before accepted payout/close. If unavailable, owner may **explicitly opt in** to a bounded quoted contribution with `max_work_contribution_lamports` default zero; otherwise reject before payout/position mutation. This is a new disclosed cost choice, not approved or a silent haircut. Sponsor reserve is consumed first and opted-in caller covers only shortfall; unused partitions return to their own funder. Candidate/provider and possible automatic B-account/page rent are closing costs; winner need not sign. First-release reserve contains no prepaid keeper network-fee reward.

Reserved funds cannot be withdrawn by another ticket/operator. Fee spikes may stop sponsor-run keepers; any payer can still submit self-funded progress or top up operations funding without receiving economic authority. Top-ups cannot change seed, deadline, recipient weights or custody partitions. A paused or failed sponsor cannot disable the instruction itself. No inactivity lease, private completion credential or admin-only resume path exists.

Operational objectives: expose last successful cursor progress and estimate remaining work. Alert after 60 seconds without progress; escalate after 300 seconds to replacement keeper, smaller chunks, self-funded progress or operations top-up. These are monitoring defaults, not proven protocol completion times. A signed existing-GUI “Resume processing” action and unsigned transaction builder/CLI should prepare the next valid chunk for any wallet; no secret/operator account is required. A future worker signs its own progress only under separately authorized operational setup.

Do not unlock an incomplete cash ticket, reroute its toll or carry only part of its seed to meet a clock target. Each accepted ticket has a finite captured work path; a deterministic validation/arithmetic failure is a defect to prevent with model/runtime tests and a separately reviewed upgrade recovery, not evidence that timeout can erase liabilities. Such a defect is outside the operational-interruption recovery guarantee: preserve the ticket/claims, expose phase/cursor and concrete transaction/simulation diagnostics, and do not label it repaired by promotion timeout. Test tiny-claim work-cost griefing, keeper fee shortage, arbitrary phase interruption, one-row progress and replacement keepers. No new minimum/cooldown/population cap is assumed.

## 4. Fixed promotion deadline

At accepted insolvency/eligible zero-pot close, atomically store `close_id`, `close_started_at`, `promotion_deadline=close_started_at+600`, round/registry/P&L snapshot identity and policy version. Use Solana Clock seconds at instruction execution; the deadline never resets on retries, fee top-up, candidate processing, admin changes or keeper replacement.

Coverage includes candidate construction, provider request creation and result capture. Do not start a fresh 600 seconds only after a slow allocation/candidate pass. Before the deadline, complete candidate lists and request the same bound seed/PDA using the authenticated client interface. No-candidate close skips normally without an oracle request.

Exact boundary rules:

Canonical terminal representation: `PromotionApplied` or `PromotionSkipped(reason)`, where reason is exactly `Deadline`, `NoCandidates`, or `EntropyExhausted`. Account state, events and `finalize_round` checks use these same variants. A Deadline skip may include a separate diagnostic stage (candidate build, request initialization, collision, provider or callback); that detail is not another terminal state. Timely capture remains nonterminal until application or deterministic entropy exhaustion.

- Accept verified capture when `Clock <= deadline`, including equality.
- Permit permissionless expiration when `Clock > deadline` and no timely capture exists.
- Submitted before deadline but executed after is late. Executed before deadline and finalized later can be valid; off-chain timestamps are not proof.
- Timely capture becomes immutable. Promotion/application may complete afterward; expiration cannot overwrite it. Ordinal mapping has a fixed total bound of 64 deterministic samples from that stored output; if all fail rejection sampling, terminalize `PromotionSkipped(EntropyExhausted)` rather than loop indefinitely or request fresh randomness. A keeper cannot skip an accepted sample. Include this rare omission and cryptographic-expansion assumptions in fairness disclosure.
- No capture by the deadline: transition once to `PromotionSkipped(Deadline)` with an observable diagnostic stage, without needing an operator outage certificate.

Expiration may apply during candidate construction, request-init failure/collision, awaiting fulfillment or rejected callbacks. Record partial candidate work as abandoned and clean those pages later; it contains no awarded entitlement. Do not delete/alter allocation tickets, pending Backer credits, funded claims or seed. ORAO/provider fees already spent are separate operations costs, not game refunds.

With financial allocation committed and promotion terminal (`PromotionApplied` or `PromotionSkipped(Deadline | NoCandidates | EntropyExhausted)`), `finalize_round(close_id)` carries the **full** seed exactly once, advances round ID and reopens admission. This instruction is permissionless. Direct `open_position` against a Closing round remains rejected. The client may atomically compose expiration (if eligible), finalization and **then** `open_position` against the expected newly Active round when all cash work is complete; it cannot deposit before those checks/updates. Verify expected round/config to prevent stale recovery from opening an unintended later round. If cash work is incomplete or a timely capture awaits application, the composer does not bypass it. Promotion timeout is recorded without pretending money was allocated.

Late authentic callback/capture is a harmless deterministic no-op with an ignored-late marker; it cannot create B credit, reroute funds, change a new round or roll back an otherwise valid provider fulfillment. Reject unauthenticated accounts normally. Duplicate expiration, promotion and seed carry are idempotent. Keep terminal close/request binding records so old callbacks cannot attach to a later round.

## 5. Randomness fairness and provider integration

Keep exactly one protocol-derived seed/request per close and the prior registered-client authority, atomic request marker and collision checks. Never request fresh entropy after nonfulfillment or timeout, and never select a fallback winner/provider. A future provider change applies to future rounds under reviewed configuration; it cannot replace the active draw after its outcome might be known.

The 600-second policy prioritizes availability and resembles the existing source's skip-on-`raw_rand`-failure reset (`ponzi_math_sol/main.mo:944–951,1001–1006`). **Selective withholding can deny a promotion.** No-reroll prevents choosing a different winner for that round, but does not make omission unbiased. Permissionless direct capture from the canonical fulfilled account reduces dependence on the designated callback/keeper; provider quorum, censorship and late execution remain trust/availability risks. This tradeoff is recommended under Rob's explicit no-indefinite-wait requirement, not hidden as perfect fairness.

The [ORAO registered-client guide](https://github.com/orao-network/solana-vrf/tree/master/callback) documents signer-bound clients/seeds and a separate callback deadline in slots: after that deadline, fulfillment can ignore a failed callback. Application Clock deadline and provider callback slot deadline are distinct. Verify the pinned SDK/program lifecycle, direct capture after callback failure, request retention and ignored-late callback behavior. Never convert 600 seconds to a fixed guaranteed slot count or claim provider retries alone guarantee delivery.

## 6. Acceptance scenarios

- Normal deposit during ordinary allocation preserves frozen recipients, weights, old toll and conservation; closing-round deposit fails atomically.
- Inline/chunk paths yield the same accepted integer oracle; 1/4/8-row grouping gives the same allocation and total verified work units. All first-release progress rewards are zero; replay cannot change credit or units.
- Work reserve short by one lamport rejects before money/state mutation; optional caller contribution cannot exceed approved maximum. Sponsor withdrawal cannot steal active reserve.
- Every phase resumes with a replacement/self-funded payer and one-row transaction; admission pause leaves claims/progress/expiration/finalization accessible.
- Clock at deadline−1/equality/+1 enforces disjoint capture/expire predicates. Timely capture cannot be deadline-skipped; deterministic 64-sample exhaustion can omit promotion only after all samples fail. Valid samples cannot be bypassed; no fresh requests. Late capture cannot mint.
- Candidate partial/request creation fails/provider never responds/config or callback fails: expiry remains callable, candidate-only cleanup does not block reopening, no seed/PDA retry changes the deadline.
- Financial allocation pending at expiry: promotion terminalizes, pending liabilities remain intact, full seed carries once after commit.
- No keepers: a user prepares/submits expiration/finalization, or normal opening composes completed-cash recovery; no operator privilege required.
- Source/game P&L, closed-entry preference and all economic vectors stay correct. Measure pauses/costs at the agreed population envelope and test fee spikes/tiny-claim load; no active count cap or latency guarantee has yet been approved.

Proposed durable lessons (not installed in memory/catalog): timestamp a promotion deadline at close acceptance rather than request creation; terminal oracle skip cannot delete unresolved financial liabilities; normal game admission and Backer-weight mutations need different processing permissions; chunk-adaptive recovery must reserve work units rather than guessed transaction counts.

Independent review accepted documentation coherence after cross-document permissions, explicit finalize-before-open ordering, bug recovery limits and canonical terminal reason/event naming were clarified. Follow-up confirmed no remaining documentation issue; program/runtime readiness is not claimed.
