# Native Solana — measurements (October 2026, local LiteSVM)

Status: measured on 2026-10-14 against the compiled program ELF `4b134d05…` (source set `1d7359b7…`) in LiteSVM 0.7.1 with the harness measurement suite (`solana/tests/financial-runtime/src/measure.rs`, 6 families, 24 headline rows, 6,680 observations). **Every figure is a local measurement at the default rent fixture (3,480 lamports per byte-year, 2-year exemption threshold) and the legacy transaction format; none is a cluster quote.** Receipt: `docs/solana/evidence/2026-10-14-m9-measurements.json`; raw observations: `2026-10-14-m9-cli.json`.

## 1. Chunk populations (plan M9 step 1): CU, packet, keys per phase

Registries of N random Backers (A two thirds / B one third; 10% closed rows ≤ 10 lamports, 10% just-open 11–60, 80% ordinary 50,000–150,000; four fixed rows) with a toll from the host `Book` oracle sized so every open row is credited; one full ticket per (N, rows). Every transaction of every run is within the legacy packet (1,232 B), 64 account locks and the 1.4 M compute limit; `credited + seed_credit == toll` and every open row was credited in all 12 runs.

| N | rows | phase | tx | CU max | packet B max | keys max | writable max |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 64 | 1 | `discover` | 64 | 18,991 | 399 | 8 | 3 |
| 64 | 1 | `senior` | 1 | 7,988 | 291 | 5 | 3 |
| 64 | 1 | `seal` | 1 | 4,517 | 258 | 4 | 2 |
| 64 | 1 | `allocate` | 64 | 11,872 | 333 | 6 | 3 |
| 64 | 1 | `prune` | 64 | 25,344 | 465 | 10 | 7 |
| 64 | 1 | `commit` | 1 | 14,785 | 326 | 6 | 3 |
| 64 | 1 | **ticket total** | 195 | | | | |
| 64 | 4 | `discover` | 16 | 28,131 | 504 | 11 | 3 |
| 64 | 4 | `senior` | 1 | 7,988 | 291 | 5 | 3 |
| 64 | 4 | `seal` | 1 | 4,517 | 258 | 4 | 2 |
| 64 | 4 | `allocate` | 16 | 24,739 | 435 | 9 | 6 |
| 64 | 4 | `prune` | 16 | 51,170 | 704 | 17 | 14 |
| 64 | 4 | `commit` | 1 | 14,785 | 326 | 6 | 3 |
| 64 | 4 | **ticket total** | 51 | | | | |
| 64 | 8 | `discover` | 8 | 40,467 | 644 | 15 | 3 |
| 64 | 8 | `senior` | 1 | 7,988 | 291 | 5 | 3 |
| 64 | 8 | `seal` | 1 | 4,517 | 258 | 4 | 2 |
| 64 | 8 | `allocate` | 8 | 40,849 | 571 | 13 | 10 |
| 64 | 8 | `prune` | 8 | 53,696 | 724 | 17 | 14 |
| 64 | 8 | `commit` | 1 | 14,785 | 326 | 6 | 3 |
| 64 | 8 | **ticket total** | 27 | | | | |
| 100 | 1 | `discover` | 100 | 18,991 | 399 | 8 | 3 |
| 100 | 1 | `senior` | 1 | 7,988 | 291 | 5 | 3 |
| 100 | 1 | `seal` | 1 | 4,517 | 258 | 4 | 2 |
| 100 | 1 | `allocate` | 100 | 11,872 | 333 | 6 | 3 |
| 100 | 1 | `prune` | 100 | 25,344 | 465 | 10 | 7 |
| 100 | 1 | `commit` | 1 | 14,785 | 326 | 6 | 3 |
| 100 | 1 | **ticket total** | 303 | | | | |
| 100 | 4 | `discover` | 25 | 28,322 | 504 | 11 | 3 |
| 100 | 4 | `senior` | 1 | 7,988 | 291 | 5 | 3 |
| 100 | 4 | `seal` | 1 | 4,517 | 258 | 4 | 2 |
| 100 | 4 | `allocate` | 25 | 24,739 | 435 | 9 | 6 |
| 100 | 4 | `prune` | 25 | 47,808 | 672 | 16 | 13 |
| 100 | 4 | `commit` | 1 | 14,785 | 326 | 6 | 3 |
| 100 | 4 | **ticket total** | 78 | | | | |
| 100 | 8 | `discover` | 13 | 39,693 | 644 | 15 | 3 |
| 100 | 8 | `senior` | 1 | 7,988 | 291 | 5 | 3 |
| 100 | 8 | `seal` | 1 | 4,517 | 258 | 4 | 2 |
| 100 | 8 | `allocate` | 13 | 41,358 | 571 | 13 | 10 |
| 100 | 8 | `prune` | 13 | 49,720 | 692 | 16 | 13 |
| 100 | 8 | `commit` | 1 | 14,785 | 326 | 6 | 3 |
| 100 | 8 | **ticket total** | 42 | | | | |
| 256 | 1 | `discover` | 256 | 18,866 | 399 | 8 | 3 |
| 256 | 1 | `senior` | 1 | 7,988 | 291 | 5 | 3 |
| 256 | 1 | `seal` | 1 | 4,517 | 258 | 4 | 2 |
| 256 | 1 | `allocate` | 256 | 11,824 | 333 | 6 | 3 |
| 256 | 1 | `prune` | 256 | 25,344 | 465 | 10 | 7 |
| 256 | 1 | `commit` | 1 | 14,785 | 326 | 6 | 3 |
| 256 | 1 | **ticket total** | 771 | | | | |
| 256 | 4 | `discover` | 64 | 28,131 | 504 | 11 | 3 |
| 256 | 4 | `senior` | 1 | 7,988 | 291 | 5 | 3 |
| 256 | 4 | `seal` | 1 | 4,517 | 258 | 4 | 2 |
| 256 | 4 | `allocate` | 64 | 24,739 | 435 | 9 | 6 |
| 256 | 4 | `prune` | 64 | 47,808 | 672 | 16 | 13 |
| 256 | 4 | `commit` | 1 | 14,785 | 326 | 6 | 3 |
| 256 | 4 | **ticket total** | 195 | | | | |
| 256 | 8 | `discover` | 32 | 40,467 | 644 | 15 | 3 |
| 256 | 8 | `senior` | 1 | 7,988 | 291 | 5 | 3 |
| 256 | 8 | `seal` | 1 | 4,517 | 258 | 4 | 2 |
| 256 | 8 | `allocate` | 32 | 41,869 | 571 | 13 | 10 |
| 256 | 8 | `prune` | 32 | 57,410 | 756 | 18 | 15 |
| 256 | 8 | `commit` | 1 | 14,785 | 326 | 6 | 3 |
| 256 | 8 | **ticket total** | 99 | | | | |
| 1024 | 1 | `discover` | 1024 | 18,866 | 399 | 8 | 3 |
| 1024 | 1 | `senior` | 1 | 7,988 | 291 | 5 | 3 |
| 1024 | 1 | `seal` | 1 | 4,517 | 258 | 4 | 2 |
| 1024 | 1 | `allocate` | 1024 | 11,824 | 333 | 6 | 3 |
| 1024 | 1 | `prune` | 1024 | 25,344 | 465 | 10 | 7 |
| 1024 | 1 | `commit` | 1 | 14,785 | 326 | 6 | 3 |
| 1024 | 1 | **ticket total** | 3075 | | | | |
| 1024 | 4 | `discover` | 256 | 27,978 | 504 | 11 | 3 |
| 1024 | 4 | `senior` | 1 | 7,988 | 291 | 5 | 3 |
| 1024 | 4 | `seal` | 1 | 4,517 | 258 | 4 | 2 |
| 1024 | 4 | `allocate` | 256 | 24,739 | 435 | 9 | 6 |
| 1024 | 4 | `prune` | 256 | 47,808 | 672 | 16 | 13 |
| 1024 | 4 | `commit` | 1 | 14,785 | 326 | 6 | 3 |
| 1024 | 4 | **ticket total** | 771 | | | | |
| 1024 | 8 | `discover` | 128 | 40,564 | 644 | 15 | 3 |
| 1024 | 8 | `senior` | 1 | 7,988 | 291 | 5 | 3 |
| 1024 | 8 | `seal` | 1 | 4,517 | 258 | 4 | 2 |
| 1024 | 8 | `allocate` | 128 | 42,379 | 571 | 13 | 10 |
| 1024 | 8 | `prune` | 128 | 57,410 | 756 | 18 | 15 |
| 1024 | 8 | `commit` | 1 | 14,785 | 326 | 6 | 3 |
| 1024 | 8 | **ticket total** | 387 | | | | |

Worst packet in the matrix: 756 B (N=256 rows=8, `prune`); worst CU: 57,410 (N=256 rows=8, `prune`). **Eight-row chunks fit with production accounts at every population up to 1,024; the one-row fallback is executed at every N.**

## 2. Churn (20 reopen/prune cycles on a 64-row registry)

Three open Series A rows are capped and pruned by every ticket (the senior slice caps the oldest, the A slice the other two), two are re-funded with real `fund_series_a` calls and one new funder opens the third; registry length 64 → 61 → 64 each cycle; 603 transactions in total (30 max per cycle); prune max 30,814 CU / 500 B; re-funding max 42,269 CU / 628 B; first funding 49,300 CU; 41 distinct state digests out of 41; both conservation identities after every transaction.

## 3. Dust claims (owed 1–10 lamports)

Claims of 1..=10 lamports to a rent-exempt destination: 28,099 CU / 514 B each. To a fresh zero-lamport destination LiteSVM accepts every claim (yes); the cluster would refuse them under the rent-state rule and the credit would stay claimable (not asserted locally; see `tests/transfer_constraints.rs`). Finding M9-F1: `claim_backer` has no deliverability preflight (same class as M8-F1).

## 4. Full close path, M = 100 participants

100 participants (16 build batches of ≤ 8 rows aligned to page edges; preferred 21, fallback 41), closing payout, LockedClose provisioning of 9 pages, request, fulfil, observe, capture, apply, finalize, then `close_shared_account` for all 22 pages. Required close reserve at N = 0: **94,633,200 lamports** = 83,060,640 (26 candidate pages × 3,194,640) + 3,125,040 (registry extension page) + 1,914,000 (winner B account) + 6,173,520 (pending request rent) + 360,000 (provider fee cap with headroom). Rent held by close accounts before cleanup 72,105,600; returned by cleanup 70,191,600 to OpsCash. Per-kind maxima: build 65,799 CU / 1,116 B / 30 keys; apply 72,245 CU / 681 B; capture 26,684 CU. Keeper-signed core transactions per close: 20 (scenario assumed 17).

### 4a. Retained growth per accepted close (incl. the permanent CloseRecord)

The permanent `CloseRecord` (657 B, DR-02 / R11: one per accepted close, never closed) retains **5,463,600 lamports** at the local rent fixture (546,360 at the conditional 696-lamport terminal step). It is funded by `provision_close_record` before the closing payout, is **not** part of `required_close_reserve` and was **not** in the held-before-cleanup total above (candidate pages, participant pages and the request account only), so it is added here once, without double counting.

| Retained per close (local rent) | lamports | at the 696 terminal step (scenario, not a quote) |
| --- | ---: | ---: |
| Permanent `CloseRecord` (657 B) | 5,463,600 | 546,360 |
| Fulfilled ORAO request record (147 B) | 1,914,000 | 191,400 |
| Winner's Series B account (147 B), when a winner exists | 1,914,000 | 191,400 |
| Consumed provider fee (to the ORAO treasury) | 300,000 | 300,000 |
| **Total with a winner** | **9,591,600** | **1,229,160** |
| Total without a winner | 7,677,600 | 1,037,760 |

Ten sponsor-funded closes therefore retain **0.054636 SOL** in CloseRecords alone locally (0.0054636 SOL in the terminal scenario); held before cleanup including the record: 77,569,200 lamports for the M = 100 close.

## 5. Inline envelope (a real payout with N Backer and page accounts appended)

| N | packet B | keys | writable | payout CU | estimated inline CU (conservative) | fits packet | fits all |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | 728 | 16 | 10 | 48,563 | 48,563 | yes | yes |
| 1 | 827 | 19 | 13 | 49,432 | 132,929 | yes | yes |
| 2 | 860 | 20 | 14 | 49,697 | 189,401 | yes | yes |
| 4 | 926 | 22 | 16 | 50,227 | 302,345 | yes | yes |
| 8 | 1,058 | 26 | 20 | 51,287 | 528,233 | yes | yes |
| 16 | 1,388 | 36 | 30 | 53,937 | 980,539 | NO | NO |
| 17 | 1,487 | 39 | 33 | 54,732 | 1,037,541 | NO | NO |

Estimate formula: payout CU + N × (discover + allocate + prune CU of one row, maxima over the rows-1 run at N = 64) + senior + seal + commit; the marginal-basis estimate at N = 8 is 213,583 CU. **Feasible N under the fits-all rule: [0, 1, 2, 4, 8]; largest 8.** Above N = 8 the packet is the binding limit (1,388 B at N = 16). Any inline path above the 200k default compute needs a compute-budget instruction (N ≥ 4 on the conservative estimate).

### 5a. Real inline allocation (IMPL-39b, ELF `d77b23bf…`)

The inline step is implemented as an opt-in: a non-closing `withdraw_simple`/`settle_compounding` that appends the registry's Backer PDAs (slot order), RegistryPage 0 and SnapshotPage 0 as writable remaining accounts allocates the whole ticket in-instruction when `registry_len <= INLINE_THRESHOLD` (8). Measured on real transactions (`tests/inline_alloc.rs`); the end state is byte-identical to the chunked ticket for every N:

| N | packet B | keys | writable | CU | credited | seed |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | 827 | 19 | 13 | 82,483 | 6,600,000 | 6,600,000 |
| 2 | 860 | 20 | 14 | 91,915 | 5,280,020 | 7,919,980 |
| 3 | 893 | 21 | 15 | 101,551 | 5,280,019 | 7,919,981 |
| 4 | 926 | 22 | 16 | 109,636 | 5,940,019 | 7,259,981 |
| 5 | 959 | 23 | 17 | 118,510 | 5,940,019 | 7,259,981 |
| 6 | 992 | 24 | 18 | 127,835 | 5,720,039 | 7,479,961 |
| 7 | 1,025 | 25 | 19 | 137,517 | 5,720,038 | 7,479,962 |
| 8 | 1,058 | 26 | 20 | 147,005 | 5,940,038 | 7,259,962 |

At N = 8 the inline payout costs 147,005 CU: under the 200k default compute budget, so no compute-budget instruction is needed (the §5 envelope estimate of 528k was conservative by design). N = 9 with accounts is refused `InvalidState`; a wrong layout is refused before any cash moves; without remaining accounts the chunked ticket starts as before.

## 6. Keeper transaction count K (pilot: P = 1,000 payouts, C = 10 closes, N = M = 100)

| rows b | ticket tx (N = 100) | close build batches (M = 100) | K measured core | K scenario | every tx fits |
| --- | --- | --- | --- | --- | --- |
| 1 | 303 | 100 | 304,040 | 304,040 | yes |
| 4 | 78 | 26 | 78,300 | 78,290 | yes |
| 8 | 42 | 16 | 42,200 | 42,170 | yes |

Chosen b = 8 (the largest of 8 / 4 / 1 for which every transaction of the N = 100 ticket fits the legacy packet, 64 locks and the 1.4 M compute limit). Measured core K = P × ticket_tx + C × close_core_tx = 42,200 (scenario 42,170); with provisioning (+9 per close) and with observe + cleanup (+23 per close) the extras are reported separately. Base network fee at the documented 5,000-lamport assumption: 0.211 SOL/month core, 0.2126 SOL/month with everything (scenario 0.21085). ICP signing cycles at 26,153,846,153 per signature: 1,103,692,307,656,600 core.

## 7. Decision output (DR-31)

- **Chunk size**: default **8 rows**; the one-row fallback is proven at every N. every phase transaction of every N × rows run is within the legacy packet, 64 locks and compute; worst packet 756 B (N=256 rows=8 prune), worst CU 57,410 (N=256 rows=8 prune); one-row fallback executed at every N.
- **Inline threshold**: IMPLEMENTED (IMPL-39b) and measured: opt-in, non-closing, registry_len <= INLINE_THRESHOLD = 8; real inline payout at N = 8 = 1,058 B / 26 keys / 147,005 CU (fits packet, under the 200k default compute); inline state == chunked state bit for bit for N = 1..8; N = 9 refused; chunks remain the keeper path for larger registries; zero-toll and empty-registry payouts never start a ticket at all (unchanged)
- **Returned to Rob**: none: the inline step is the plan's own step 2(b), bounded by the measured N = 8; no page-capacity or keeper-format change is proposed (DR-28 legacy/v0 format fits everywhere)

## 8. What these numbers are not

Not a cluster measurement, not a rent quote (DR-27: rent is being cut in feature-gated steps; the budget carries the 696-lamport terminal column as well), not a throughput or latency proof (ICP signing and RPC latency against the 600 s deadline are M13 items), and not a fee quote (5,000 lamports per signature is the documented planning assumption).
