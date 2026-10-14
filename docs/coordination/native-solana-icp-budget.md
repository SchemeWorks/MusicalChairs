# ICP keeper placement and pilot operating budget

Date: 2026-10-12. Status: researched recommendation; independent native Luna document/arithmetic review accepted after seven clarifications. Not a deployment quote or funding instruction. Baseline `fb8b5e5419381929f5a1f553c02a2f12de027b3c`; no native program exists yet.

Rob selected a **small pilot: approximately 100 active Backers, 1,000 payouts and 10 round closes per month**. For calculations only, assume 100 distinct participants per closing round, all payouts have a nonzero allocated toll, eight rows per progress transaction and one keeper signature. Participants and new unique Backers are separate from active Backer count; these are price scenarios, not measured capacity or a protocol cap.

Here N counts **active tier records** traversed by allocation. If 100 owners each have both A and B, N could be 200 and costs increase; the launch manifest must translate the user-facing count to actual registry rows. M=100 is separate assumed current-round participation, not an observed user count.

## Recommendation

ICP can host keepers using timers, threshold Ed25519 signing and SOL RPC submission. It can also host the existing frontend, PP/Shenanigans/social services, finalized-history ingestion, reward outboxes, monitoring and recovery preparation. Native custody, payout/Backer accounting and round rules remain enforced by the Solana program.

At the eight-row scenario, **ICP threshold signing dominates ongoing cost**. Recommend ICP for hosting/social/history and a watchdog/emergency keeper, with an external fee-only primary worker for frequent processing. Fully ICP keepers remain a valid option if Rob prefers that architecture or measured batching makes its cost acceptable. This is a recommended operating-host revision, not authorization to create servers, canisters, signers or transactions.

For this pilot, provision a **12 SOL project/sponsor planning envelope** for deployment/shared storage plus the first operating month, conditional on a compiled binary at most 512 KiB, prompt storage reuse/cleanup and the count/fee assumptions below. Consider about **20 SOL** if the binary reaches 1 MiB and double code-capacity headroom is chosen. Budget **0.5–1.5 SOL/month** for ongoing sponsor fees and retained-account growth under modeled priorities/retries. **Player-funded account rent/fees, ICP cycles, subscriptions, development/audit and game liquidity are excluded.** Amounts are replenishable, not a lifetime prepayment or guaranteed maximum.

## What runs where

| Component | Recommended placement | Authority / cost boundary |
| --- | --- | --- |
| Current GUI, domain/assets | Existing ICP asset hosting | Preserve look/feel and wallet connection |
| PP, Shenanigans, profiles/chat/referrals, existing PP desk | Existing ICP canisters | Already selected separate financial/social rail; no native pool access |
| Native history/PP outbox and monitoring | ICP | RPC observation trust remains explicit; no SOL payout authorization |
| Routine allocation/close worker | External small service initially | Dedicated fee-only key, no user/pool/G/management/provider-client role; public cursor-checked instructions only |
| Watchdog/emergency progress | ICP canister | Dedicated threshold-derived fee-only address; signs rarely after stale progress; any wallet can still recover |
| Fully ICP alternative | Two keeper canisters, active/standby | Prefer separate application subnets; same subnet is process redundancy, not independent subnet failure protection |
| Financial state and custody | Native Solana program/PDAs | ICP does not carry a pool spending key or authorize withdrawals |
| Promotion randomness | ORAO registered-client request on Solana | ICP `raw_rand` attestation would be a different oracle trust model; optional redesign, not silently substituted |

[ICP Solana integration](https://docs.internetcomputer.org/guides/chain-fusion/solana/) documents RPC and threshold signing; [timers](https://docs.internetcomputer.org/guides/backends/timers/) support background execution. A keeper needs a bounded transaction journal, stale cursor/version checks, unknown-outcome reconciliation, timer rearming after upgrades and adaptive idle backoff. No arbitrary-byte signing endpoint, original-keeper credential, player signature proxy or governance key. Any malicious keeper/controller can spend its own hot fee balance or waste fees; neither may change economic state outside public program rules. Cap/refill that balance separately from operations escrow.

Use idle backoff (for example 1/5/15 minutes) and active monitoring around 30–60 seconds, with batched signature status reads. Standby begins after stalled progress rather than racing every call. No ICP timer/chain/transaction latency guarantee is claimed; close+600-second eligibility stays fixed. Test ICP processing latency against that policy as well as cost.

Reusable source: `ponzi_math_sol/SolSigner.mo:41–97`, `SolTx.mo:406`, `SolRpc.mo:472–573`, `main.mo:1134–1175,3655,3792–3821`. Reuse parsing/signing primitives after review, not legacy pool authority. Current official SOL RPC identity differs from the old source constant; verify a new binding/Candid and preserve the legacy deployment. Current official testing-key guidance also differs from the local-key comment; do not assume local threshold signing is available.

## Public price evidence

Read-only public mainnet RPC checks on 2026-10-12:

- `getMinimumBalanceForRentExemption`: 0 bytes = 650,240 lamports; 256 = 1,950,720; 4,096 = 21,457,920; 65,536 = 333,573,120. Independent public RPC confirmed 256/4,096. These quotes fit **R(bytes)=5,080 × (bytes+128)** lamports, superseding the older 6,960 coefficient used in generic examples. The 512-KiB ProgramData quote was also checked. Re-query actual schemas before deployment.
- ORAO Callback program owner/discriminator/PDA/bump/schema validated at finalized slot **456271210**: configured request fee **300,000 lamports = 0.0003 SOL**. Three fulfillment authorities currently imply three required responses under its `floor(2N/3)+1` rule; this is a provider snapshot, not a permanent liveness promise.
- Squads v4 ProgramConfig owner/discriminator validated at the same slot: creation fee **zero**. Account rent/proposal/transaction fees still apply.

[Rent RPC definition](https://solana.com/docs/rpc/http/getminimumbalanceforrentexemption), [Solana fees](https://solana.com/docs/core/fees), [pinned Callback source](https://github.com/orao-network/solana-vrf/tree/5ef94f9c7d83952573b0276fa91e14495e2531a3/callback) and [Squads source](https://github.com/Squads-Protocol/v4) explain these fields. Public RPC observations are unsigned price/state snapshots, not deployed Musical Chairs proof. [Raw validated receipts](evidence/2026-10-12-native-solana-costs.json) record endpoints/slots and UTC collection metadata; date above uses Rob's local timezone.

## Setup: code, shared storage and peak liquidity

Let B be binary bytes and H chosen executable capacity. Loader-v3 metadata adds 45 bytes to ProgramData, 37 to an upload buffer and 36 to the Program record. Conservative liquidity provision covers **R(H+45)+R(B+37)+R(36)**; retained code rent is **R(H+45)+R(36)**. Actual initial deployment can move funded buffer lamports into ProgramData, so this is an upper planning allowance, not two permanently spent copies. Upgrades can require separate buffer liquidity while old code remains live.

| Illustrative binary | Retained code, H=B | Conservative code+buffer, H=B | Conservative code+buffer, H=2B |
| --- | ---: | ---: | ---: |
| 256 KiB | 1.3334 SOL | 2.6659 SOL | 3.9976 SOL |
| 512 KiB | 2.6651 SOL | 5.3293 SOL | 7.9927 SOL |
| 1 MiB | 5.3285 SOL | 10.6561 SOL | 15.9828 SOL |

[Program deployment](https://solana.com/docs/core/programs/program-deployment) describes upload/upgrade buffers. The inspected [Agave 4.3.0 CLI](https://github.com/anza-xyz/agave/blob/v4.3.0/cli/src/program.rs#L1386) defaults max length to binary length; **2B headroom is an explicit scenario**, not a mandatory default. Pin the actual CLI and measure `.so` before selecting capacity. Upload signatures/priority fees, IDL metadata and governance transaction accounts are additional; allow a provisional 0.05–0.10 SOL setup contingency, then replace it with an exact manifest.

Provisional shared sizes, including assumed discriminators/padding: Config 512 bytes; Pool/Ops state 256 each; Management 128; Round/close state 768 each; ticket 1,024; registry page 512/8 rows; toll snapshot 1,024/8; participant and each candidate page 768/8; retained close receipt 512; automatic B 256. These are **estimates**, not existing account layouts. Conservatively allow both preferred and fallback candidate lists to contain M rows each; no storage optimization is assumed.

For N=M=100: global/registry rent about **0.0553 SOL**, plus reusable ticket/snapshot/participant/two-list/closing storage about **0.2659 SOL**; shared peak **0.3212 SOL**. Add provider/client, retained receipts/new B, cleanup overlap and schema slack; **0.5 SOL** is a provisional shared working reserve. Cleanup lag or larger M raises it. Owner-funded User/Position/UserRound/A rent is excluded. Persistent User/Backer flags remain; their rent is not assumed reclaimable promptly.

Exact composition of that estimate:

| Sponsor component | Count × assumed bytes | Rent subtotal SOL | Treatment |
| --- | --- | ---: | --- |
| Config | 1 × 512 | 0.00325120 | Retained |
| Pool and operations state | 2 × 256 | 0.00390144 | Retained |
| Management state | 1 × 128 | 0.00130048 | Retained |
| Current Round | 1 × 768 | 0.00455168 | One active account; old record budgeted separately |
| Active registry pages | 13 × 512 | 0.04226560 | Retained/reusable active membership |
| **Global/registry subtotal** | — | **0.05527040** | Excludes past receipts and new B |
| Allocation ticket | 1 × 1,024 | 0.00585216 | Reusable after obligations complete |
| Toll snapshots | 13 × 1,024 | 0.07607808 | Reusable after credit safety |
| Participants | 13 × 768 | 0.05917184 | Shared pages, not owner UserRound |
| Preferred + fallback candidates | 26 × 768 | 0.11834368 | Two conservative full lists |
| Closing state | 1 × 768 | 0.00455168 | Separate conservative close-work allowance |
| B creation/work allowance | 1 × 256 | 0.00195072 | Converts to persistent B if a new account is created |
| **One-workspace subtotal** | — | **0.26594816** | Peak temporary/convertible collateral |
| **Shared peak** | — | **0.32121856** | Current globals + one workset |

The B allowance is present peak liquidity, then remains retained when consumed; monthly B growth replenishes it for later winners rather than counting a second same-account purchase. Terminal receipts/provider records are excluded from the above reusable subtotal and counted in growth below. If the old Round stays 768 bytes instead of becoming a 512-byte receipt, use 0.00455168 SOL per retained round instead of 0.00325120, not both.

Cleanup high-water examples (excluding separately counted accumulated B/receipt/provider records):

| Overlap assumption | Shared storage collateral |
| --- | ---: |
| Current workspace, no prior cleanup backlog | 0.3212 SOL |
| One complete prior workset still held + current | 0.5872 SOL |
| Ten candidate-list sets abandoned + current | 1.5047 SOL |
| Ten complete prior worksets still held + current | 2.9807 SOL |

These bounds use the same assumed sizes and are not an automatically enforced backlog limit. Extra request markers/receipts and allocation schemas must be sized explicitly. The 600-second eligibility deadline never promises storage release or cash completion. Prompt reusable-page cleanup is an operational assumption of the 0.5-SOL reserve; otherwise replenish, clean safely or reject unfunded new work atomically. No accepted claim/toll can be discarded to reclaim rent.

Player-side unit rents under these hypothetical sizes: User 160 bytes = 0.00146304 SOL; Position 320 = 0.00227584; UserRound 224 = 0.00178816; A record 256 = 0.00195072. A wallet creating one of each locks about **0.00747776 SOL**, outside deposits/network fees. A thousand payouts does not imply a thousand new Positions or wallets, so no fictional total player cohort is charged to the sponsor envelope. Some personal records can safely close/refund; permanent flags/Backers persist.

ORAO no-callback sizes from the exact SDK: Client 123 bytes (**0.00127508 SOL**); pending request 759 (**0.00450596**); frozen fulfilled request 147 (**0.00139700**). Fulfillment returns **0.00310896 SOL** excess request rent to Client; fee is consumed. No public close-request/client ABI is assumed. A failed/unfulfilled request can retain the full pending allocation; permissionless game expiry does not refund provider storage. First request plus Client/fee requires approximately **0.00608104 SOL**, excluding network fees.

Two three-member Squads records require **0.00364744 SOL** combined rent at quoted prices; base creation signatures approximately **0.00002000 SOL** in a two-signature-per-create scenario. A three-member proposal is **0.00246888 SOL** rent, with dynamic transaction storage additional and some terminal proposal/transaction rent reclaimable. No governance creation protocol fee in the observed snapshot.

## Monthly SOL calculation

For payout count P, closing count C, active Backers N, participants M and measured rows per transaction b, use the planning count:

```text
K = P × (3 × ceil(N/b) + 3) + C × (ceil(M/b) + 4)
```

Three allocation constants represent Senior/SealWeights/Commit; four close constants represent request/capture/promotion/finalize. The triggering user payout is already in P and its network fee is user-paid, not another keeper transaction. Other cleanup/pages/multi-step ordinal exhaustion, fee retries and standby races need separate measured allowance. Zero-toll/inline/no-candidate paths can reduce K. The formula assumes phases fit their proposed transactions, not a proof they do.

For one N=M=100 closing payout, the model implies **42 allocation + 17 close transactions** before additional cleanup/retries. Monthly K is not a throughput proof. ICP sequential sign/RPC delay, queueing and peak burst behavior must be measured against the close deadline; a result may be deadline-skipped while protected cash work continues afterward.

For the pilot b=8: **K=42,170/month**, base keeper fee **0.21085 SOL** using the documented 5,000-lamport per-signature fee assumption, not a Musical Chairs transaction quote. Average priority fee is also a planning input; actual legacy/v0 fee is computed from CU limit/price. One request per nonempty close at current fee adds **0.003 SOL/month**.

If ten closes create ten new B records and retain ten native receipts plus ten fulfilled provider records, additional locked rent is **0.0659892 SOL/month**. Reused B accounts reduce growth. If all requests remain unfulfilled, the analogous storage total is **0.0970788 SOL** instead. This is growing collateral, not a recurring rent invoice.

| Average priority per keeper transaction (assumption) | Fees + oracle + fulfilled-record/B/receipt growth, no retries | Same with 25% extra fee-paying attempts |
| --- | ---: | ---: |
| 0 lamports | 0.2798 SOL/month | 0.3326 SOL/month |
| 5,000 lamports | 0.4907 | 0.5961 |
| 20,000 lamports | 1.1232 | 1.3868 |
| 50,000 lamports | 2.3883 | 2.9682 |

Thus **0.5–1.5 SOL/month is a replenishment budget for ordinary modeled priorities**, not protection against unlimited congestion, cleanup cost, growth or griefing. Keep shared rent working capital and fee-address balances separate. Idle polling itself spends cycles/API quota, not SOL transaction fees. Owner wallet fees/rent, new deposits, A funding, management proceeds and game payouts are separate from this sponsor budget.

Stress example, not launch target: N=M=1,000, P=10,000 and C=100 at b=8 requires about **3,792,900 keeper transactions/month** and **18.9645 SOL** base network fees before priority/rent/provider costs. One-row fallback at the pilot N=100 also materially exceeds the eight-row budget if routinely needed. Growth requires repricing, not treating the pilot allowance as unlimited.

## ICP cycles and hosting choice

Current official production `key_1` signature price: **26,153,846,153 cycles per signature**. Pilot K implies **1,102.9 trillion cycles just for signing**. At the docs' explicitly dated XDR/USD conversion (1T cycles=1 XDR; $1.366430/XDR), that is approximately **$1,507/month**. This conversion is illustrative, not a current certified FX quote. Budget cycles precisely; reprice USD at funding time.

Assuming 2–3 small SOL RPC operations per signed progress transaction and 1–5B cycles per operation, add roughly **84–633T cycles/month**: total **1,187–1,735T** or about **$1,620–$2,370** at that reference conversion, before idle/standby polling, execution/storage, bridge work or retries. A practical all-ICP allowance under these assumptions is roughly **$1,700–$3,000/month equivalent in cycles**, plus the SOL budget. Reusing/batching reads reduces the RPC portion; one threshold signature per distinct transaction remains. An emergency-only ICP backup spends proportionally less.

Batching matters: b=32 instead of 8 would reduce pilot K to **15,080**, cutting signature-only cost to about **$539** at that conversion. A truly feasible inline allocation inside each user payout would remove most ordinary keeper signatures; it has not been proved at N=100. Measure packet/accounts/CU and ICP end-to-end latency, then choose the host/chunk plan. A keeper may use a proved newer transaction format or ALT independently of the GUI; that does not require changing the program's economic rules or a whole-frontend migration. Preserve economics/cursors; no capacity cap is invented to force a lower budget.

For a hybrid, a small external worker VM is an operating estimate around **$6–$12/month for compute alone** using a current [published VPS example](https://www.digitalocean.com/pricing/droplets). RPC/archive/monitoring/backup/security operations cost extra; no vendor capacity, free production RPC or server order is claimed. ICP can retain the durable history/PP services and monitor/backup, reducing signing spend sharply. Neither host is trusted for native payment eligibility.

## Measured (2026-10-14, local LiteSVM; supersedes the scenario figures above where they differ)

Source: `docs/solana/measurements-2026-10.md` and receipt `docs/solana/evidence/2026-10-14-m9-measurements.json`. Rent is the local default fixture (3,480 lamports per byte-year × 2-year exemption = 6,960 lamports per byte; the active SIMD-0437 step at measurement time) with the 696-lamport terminal step as a second column (DR-27). The 12-SOL and 0.5–1.5-SOL figures above remain superseded scenarios, not measurements.

| Quantity | Measured (local default rent) | At the 696-lamport terminal step |
| --- | ---: | ---: |
| Keeper transactions per month, core (P = 1,000, C = 10, N = M = 100, b = 8) | 42,200 (scenario 42,170) | same |
| … plus LockedClose provisioning (+9 per close) | 42,290 | same |
| … plus observe and cleanup (+23 per close) | 42,520 | same |
| Base network fee at 5,000 lamports per signature (assumption, not a quote) | 0.211 SOL/month core; 0.2126 with everything | same |
| ICP signing cycles (26,153,846,153 per signature), core | 1,103.7 T | same |
| Required close reserve, M = 100, N = 0 (locked at the closing payout, mostly refundable) | 94,633,200 lamports | 9,463,320 lamports |
| Candidate page rent (331 B) | 3,194,640 | 319,464 |
| Participant page rent (330 B) | 3,187,680 | 318,768 |
| Backer / B account rent (147 B) | 1,914,000 | 191,400 |
| Pending ORAO request rent (759 B) / fulfilled (147 B) | 6,173,520 / 1,914,000 | 617,352 / 191,400 |
| Rent held by one M = 100 close before cleanup / returned by cleanup | 72,105,600 / 70,191,600 | ÷ 10 |
| Permanent `CloseRecord` (657 B) retained per accepted close (DR-02; funded at `provision_close_record`, not in the reserve, not in the held-before-cleanup total) | 5,463,600 | 546,360 |
| Retained per close incl. the CloseRecord: record + fulfilled request record + consumed provider fee (+ B account when a winner exists) | 7,677,600 (+ 1,914,000) | 1,037,760 (+ 191,400) |
| Ten sponsor-funded closes, CloseRecords only | 0.054636 SOL | 0.0054636 SOL (terminal scenario, not a quote) |

Reading: the eight-row chunk assumption of the scenario holds with production accounts (every transaction of the N = 100 and N = 1,024 tickets fits), so the planning count was right within +30 core transactions for the pilot; provisioning and cleanup add about 320 keeper transactions per month. Rent is a refundable deposit: the close reserve is locked at the payout and returned at settlement and cleanup; only the fulfilled request record, the consumed oracle fee and a new B account are retained. An inline allocation inside the payout is implemented (IMPL-39b) and measured for real up to eight Backers (1,058 B / 26 keys / 147,005 CU at N = 8, under the default compute budget; packet-bound above that), so it removes keeper signatures only for registries of at most eight Backers; the pilot's N = 100 stays on chunks. The deployment buffer, RPC/host subscriptions and the hybrid VM estimate above are unchanged inputs, not measurements.

## What this resolves before implementation

The selected workload now has a budget and both host options, with recommendation to use hybrid until measured batching justifies fully ICP. No game liquidity or legacy reserves are included. Early implementation replaces fictional byte/chunk/binary sizes with measured values and verifies ICP SDK/Candid/current subnet/key behavior. Pre-deployment replans exact code/storage/provider/governance/fee reserves from current prices; no funding or deployment is authorized here. Old Round/request records and sponsor cleanup overlap must be classified, not assumed free.

Provenance: unchanged **financial source baseline** `f96b33ff95020458be754bfa0d7471b72a7403af`; **documentation pass baseline** `fb8b5e5419381929f5a1f553c02a2f12de027b3c`; this register and linked spec/plan are coordinator edits on that documentation branch. Older baseline references in the evidence handoff describe previous passes, not deployed bytes. Public snapshots and independent arithmetic review do not establish runtime or launch readiness. SOL-22 re-review accepted all seven documentation clarifications: component ledger, payer exclusions, cleanup high-water, close timing limitations, reproducible snapshot metadata, separated fee assumptions and baseline provenance. Runtime sizing, transaction packing, latency and live configuration remain unproved.
