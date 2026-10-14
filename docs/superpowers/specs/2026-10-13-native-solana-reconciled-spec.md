# Musical Chairs native Solana — reconciled specification

Date: 2026-10-13. Author: Claude (planning coordinator), succeeding Codex. Status: **reconciled specification for Codex review; not an authorization to implement, deploy, fund or sign.**

This document supersedes the status wording of the [2026-10-12 discovery specification](2026-10-12-native-solana-design.md) and consolidates the accepted contracts in [launch decisions](../../solana/launch-decisions.md), [defaults](../../coordination/native-solana-defaults.md), [economics](../../coordination/native-solana-economics.md), [availability](../../coordination/native-solana-availability.md), [ICP budget](../../coordination/native-solana-icp-budget.md) and the host-model contracts under `docs/solana/`. Those documents remain the evidence history. Where they disagree with each other or with current source, section 2 says which reading this specification adopts and why. Companions: [current-state inventory](../../coordination/native-solana-state-inventory-20261013.md), [decision register](../../coordination/native-solana-decision-register-20261013.md) (DR-nn references below), [implementation plan](../plans/2026-10-13-native-solana-implementation.md), [Codex review packet](../../coordination/native-solana-codex-review-packet-20261013.md).

Legend used throughout:

- **ACCEPTED** — policy Rob accepted (D1–D9 and the defaults register). Not reopened here.
- **MODELED** — behavior implemented and tested in the host reference model (`solana/crates/economics`, 72 tests recorded) or the provider probe (5 tests). Not SVM or production proof.
- **PROBED** — behavior executed in compiled SBF under LiteSVM in the runtime probes (88 recorded cases). Not production code.
- **DRAFT** — present in the uncommitted production tranche; unreviewed, untested, and in places explicitly `NotImplemented`.
- **PROPOSED** — this specification's recommendation where no accepted text decides the point. Each carries a DR reference.
- **LAUNCH** — a concrete value supplied at launch preparation.

**Revision 2026-10-13b (after Codex review round 1, [review](../../coordination/native-solana-codex-plan-review-20261013.md)):** changed sections are 3.2 (accrual increment and carried unpaid earnings), 4 (immutable cluster domain), 5 (ORAO Client roles; guardian key in Admission), 6 (Pool.cluster_domain, Admission.guardian_vault, Position permanence, Ticket U256 accumulator, ConfigVersion close cost schedule, CloseRecord captured reserve), 7.1 (rotate_guardian, provisioning kinds, top_up_close_reserve, closable kinds), 7.2 (no Position rent reclamation), 7.3 (six typed phase instructions, alias exceptions, stale-call rejection), 7.4 (Client funding and attribution), 7.5 (event-cpi accounts), 7.6, 8 (required close reserve), 9 (Shenanigans-owned reservation protocol), 12. The item-by-item resolution matrix is in the [review packet](../../coordination/native-solana-codex-review-packet-20261013.md).

---

## 1. Scope and authority boundary

### 1.1 Product scope (ACCEPTED, D1–D2)

A fresh native SOL game with new deposits. No import of legacy positions, Backer promises, balances or reserves. Legacy ICP Chain Fusion positions (`ponzi_math_sol`, mainnet `spc6q-xyaaa-aaaac-qg2ma-cai`) keep their existing withdrawal, settlement, Backer-claim and desk paths, separately labeled; nothing in this specification pauses, drains, upgrades or shuts them down. Ponzi Points, Shenanigans, the Founder's Allocation PP desk, profiles, chat and referrals stay on ICP. The React/Vite GUI stays on ICP certified asset hosting with its look, navigation, terminology and major screens.

### 1.2 Authority boundary (ACCEPTED, D9; MODELED in part; DRAFT in part)

| Decides / holds | Solana program | Browser | ICP |
| --- | --- | --- | --- |
| Pot, seed, management, pending toll, funded Backer claims, unallocated excess (physical SOL) | **Yes** (program-owned PDAs) | No | No |
| Admission, payout eligibility, insolvency close, Backer allocation, promotion winner, round lifecycle | **Yes** | No | No |
| Transaction building, simulation, wallet signature, submission, status reads | No | **Yes** | No |
| Canonical financial reads for the GUI | Source | Reads directly via RPC | Not required |
| PP minting for native events, feeds, history enrichment, social identity | No | No | **Yes**, via a disclosed trust boundary (§9) |
| Keeper continuation of accepted work | Verifies | May prepare | May submit from a fee-only key (§10) |

ICP never holds a pool spending key, never signs ordinary payouts and never decides solvency or winners. A SIWS delegation is social identity, not financial authority.

Solana note for Rob: on ICP the canister *is* the account that holds ICP and the code that decides; one principal, one upgradeable unit. On Solana code and data are separate: the executable lives in a `Program`/`ProgramData` pair controlled by the loader's upgrade authority, and every piece of state is a separate account that must be listed in the transaction. The "canister" here is therefore a program plus a fixed set of PDAs whose seeds are a public contract (§6). Nothing can be written that is not in the transaction's account list, which is why keepers only need the account list and no credential.

### 1.3 What "superseded" means in the older documents

The 2026-05-23 Anchor thought experiment and its premortem are not requirements. The 2026-10-12 spec/plan/economics/availability documents carry "draft / no runtime proof / host-only subset" wording that was true when written. This specification replaces that wording as follows: policy D1–D9 and the defaults register are **accepted**; the economics/allocation/round models are **modeled** (72 host tests, 5 probe tests recorded); permissions, representative compute and the ORAO lifecycle are **probed** (88 cases recorded); the production account/instruction tranche is **draft**. No build, test or probe was re-run for this specification; counts are read from saved receipts hashed in the inventory.

---

## 2. Reconciliation decisions

Each row names a disagreement found between documents or between documents and current source, and the reading adopted here.

| # | Disagreement | Adopted reading | Why |
| --- | --- | --- | --- |
| R1 | Frontend constants `BACKER_OLDEST_UPSTREAM_SHARE=0.35 / OTHER=0.25 / ALL=0.40` (`frontend/src/lib/gameConstants.ts:42–44`, `DocsPage.tsx:230–232`) vs executable `0.35/0.45/0.20` (`ponzi_math_sol/main.mo:707–709`) | **35 % senior / 45 % proportional A / 20 % equal B** | Executable source and accepted D4; the frontend text is a display defect to fix in the adapter milestone |
| R2 | First Codex account proposal reversed senior and proportional slices | 35/45/20 as above | Verified against `main.mo:707–709` and the host model goldens |
| R3 | GUI projects ROI on net deposit (`SolInvestPanel.tsx:70–76`) vs canister accrual on gross (`main.mo:2808`, `:1077`) | **Gross accrual**; GUI projection corrected | Accepted D4; source |
| R4 | Legacy 5,000-lamport fee deduction and silent tiny-payout success (`main.mo:1291–1317`) vs accepted wallet-paid fees | **No hidden deduction; every positive funded lamport is claimable; no silent success** | Accepted D5 |
| R5 | Legacy quota checked at intent, recorded at credit (`main.mo:2989–3002`, `:2837–2855`) vs accepted atomic quota | **Three successful `open_position`/`fund_series_a` per owner per rolling 3,600 s, enforced atomically; a timestamp exactly 3,600 s old has expired; failed calls and claims do not count** | Accepted D5; MODELED |
| R6 | Legacy `getMaxDepositLimit` (20 % of pot or 5 SOL, `main.mo:1675–1677`) is a query only, never enforced on admission | **No dynamic cap in native** | Accepted D5 and source behavior |
| R7 | Legacy `selfServeBackingEnabled=false` (`main.mo:423`) vs accepted public A on | **`public_series_a_enabled=true` at fresh launch**, disclosed as a launch-flag difference | Accepted D4 |
| R8 | Legacy equality `earnings == pot` empties the pot and does not close until the next attempt (`main.mo:1258`) vs host model | **Strict `owed > pot` closes; equality is solvent even when it empties the pot; a later eligible positive claim with minimum zero may close a zero pot** | Accepted D7; MODELED |
| R9 | Legacy Simple accrual has no remainder carry (Float, `main.mo:1073–1078`) vs accepted integer model | **Integer numerator with carried remainder, 21-day clamp, backwards clock rejected** | Accepted D5; MODELED |
| R10 | Economics doc table said compound-30 net 10,672,**9**80,269 | **10,672,880,269** | `docs/solana/economics.md` records the typo; fixture is authoritative |
| R11 | Draft stores close time/deadline in `Pool`; accepted text requires retained close/request bindings | **Retained `CloseRecord` per close** (DR-02) | Old callbacks must not attach to later rounds |
| R12 | Draft defers owner work-contribution and full closing/provider/reopening integration | **Tranche boundaries only; full release implements them** | Handoff §6 |
| R13 | Handoff text names `createBackdatedGame` among SOL hatches; it exists only in `ponzi_math` (ICP), not `ponzi_math_sol` | **Legacy hatch list for the SOL canister is §A11 of the source map**; native has no manual-credit ABI | Source |
| R14 | Budget doc assumes 96-byte candidate rows | **32-byte owner rows; loss re-read from `UserRound`** (DR-03) | Smaller and no duplicated P/L |
| R15 | Platform-tools recorded as v1.48 in build provenance; the cached `cargo-build-sbf --version` prints v1.52 | **Unresolved toolchain pin; plan Task 0 re-pins and re-records before any build** | Inventory finding; no build claim is made here |
| R16 | Draft `controls.rs:438` Squads `Multisig` discriminator ends `0xc8`; `sha256("account:Multisig")` ends `0xec`. Draft `:446` parses `config_authority` as `Option<Pubkey>`; Squads v4 stores a plain `Pubkey` with `Pubkey::default()` meaning autonomous (public source, 2026-10-13) | **Fix both before any governance test is believed**; the four draft unit tests are circular | Inventory §5.2 #1–#2 |
| R17 | Draft `Pool.funded_claims: u64`; accepted width table says global funded-claim liability is checked u128 | **u128 field, asserted ≤ vault spendable lamports** (DR-05) | Accepted text; cost is 8 bytes |
| R18 | Budget doc uses a fixed 5,080 lamports/byte rent coefficient | **Rent is mid-reduction (SIMD-0437: 6,333 live 2026-09-03; 5,080 step 2; 2,575/1,322/696 expected Nov 2026). Treat every rent figure as a dated scenario and re-quote at measurement and launch** (DR-27) | Public snapshot 2026-10-13 |
| R19 | Record assumes 1,232-byte envelope everywhere | **GUI stays legacy/v0 at 1,232 B (accepted). v1 4,096-byte transactions are live on mainnet; a keeper may use them later only after a separate measured decision (DR-28); the ICP bridge must decode observed v1 transactions regardless** | Public snapshot 2026-10-13 |

---

## 3. Economic contract (ACCEPTED; MODELED)

All values are lamports; all division floors; all arithmetic is checked. `P` is gross principal.

### 3.1 Admission and entry

```text
front_end_load = floor(P * 400 / 10_000)          -- 4 % to management
pot_credit     = P - front_end_load                -- 96 % to pot
Position.gross = P                                  -- accrual base is GROSS
min_game  = 10_000_000 lamports (0.01 SOL)
min_A     = 50_000_000 lamports (0.05 SOL)
quota     = 3 successful open_position/fund_series_a per owner per rolling 3_600 s;
            exactly-3_600-s-old timestamp has expired; failures and claims do not count
```

Series A funding adds the **full** gross to the pot with no Front-End Load and grants `floor(P * 124 / 100)` entitlement. Series B grants `floor(loss * 116 / 100)` entitlement and adds no cash. Before any game deposit is accepted the program verifies in U256 that `existing_B_entitlement + floor(max(gross_deposits_after - net_payouts, 0) * 116 / 100)` fits u128, so a later promotion can never fail on width.

### 3.2 Simple plan

```text
rate 11 %/day, cap 21 days = 1_814_400 s
elapsed        = min(now - last_accrued_at, max(0, expires_at - last_accrued_at))
numerator      = P * 11 * elapsed + remainder          (remainder < 8_640_000)
increment      = floor(numerator / 8_640_000)          -- accrual increment for this step
remainder'     = numerator mod 8_640_000
accrued_gross' = accrued_gross + increment              -- cumulative gross earnings, u128
owed           = accrued_gross' - gross_paid            -- claimable now; unpaid earnings carry forward
```

A claim pays `min(owed, pot)` and adds the gross actually paid to `Position.gross_paid`; a minimum-receive failure reverts atomically, leaving `accrued_gross`, `remainder`, `last_accrued_at` and `gross_paid` unchanged, so a partial or failed claim never loses unpaid Simple earnings.

Backwards clock (`now < last_accrued_at`) is rejected without mutation. Normal claims may be partial and repeat; maturity (`now >= expires_at`) terminates the position, including with zero remaining owed. `close_simple_position` requires maturity. Carried Interest on the gross actually paid:

| Elapsed since open | bps |
| --- | ---: |
| `< 7 days` | 1,200 |
| `>= 7 days` and `< 14 days` | 750 |
| `>= 14 days` | 300 |

### 3.3 Compounding plans

```text
Compound15: 12 %/day, 15 days, ROI15_Q18 = 4_473_565_759_257_037_859, Jackpot Fee 900 bps
Compound30:  9 %/day, 30 days, ROI30_Q18 = 12_267_678_469_131_246_220, Jackpot Fee 1_300 bps
owed = floor(P * ROI_Q18 / 10^18)     settlement only at/after maturity; principal not returned
```

### 3.4 Payout, insolvency and toll

```text
gross_paid = min(owed, pot)
closing    = owed > pot                 -- strict; equality is solvent
toll       = floor(gross_paid * bps / 10_000)
net        = gross_paid - toll          -- must satisfy net >= minimum_receive, else atomic failure
pot       -= gross_paid;  pending_toll += toll;  UserRound.net_payouts += net
```

Network fees are paid by the transaction fee payer (owner by GUI default); account rent is paid by the recorded rent payer. Neither is deducted from `net`. A zero toll creates no allocation ticket.

### 3.5 Toll split (ACCEPTED; MODELED with goldens)

```text
seed0 = floor(T/2);  H = T - seed0
senior = floor(H*35/100);  a_pool = floor(H*45/100);  b_pool = floor(H*20/100);  H - sum -> seed
membership captured BEFORE senior credit:
  no open B -> b_pool folds into a_pool;  no open A -> a_pool folds into b_pool and senior -> seed
  no open Backer at all -> entire T -> seed
senior credited first to the oldest open A (by first_funding_seq), capped by remaining; overshoot -> seed
sealed A denominator = sum(captured A remainders) - senior_credit   (post-senior, sealed once)
each captured A row weight = its captured remainder minus senior_credit only on the senior row
A shares pro-rata on sealed denominator (U256), capped; B equal floor share per captured open B, capped
all cap overshoot and division residue -> seed; "open" means remaining > 10 lamports
if senior closes the only A, its a_pool has denominator zero and goes to seed (not re-folded to B)
```

Goldens (MODELED): `T=10,000; A remainders 4,000/6,000; two uncapped B` → seed 5,001; A 2,363 (1,750 senior + 613) / 1,636; B 500/500. `T=10,000; A 1,000/10,000; no B` → seed 5,750; A 1,000 / 3,250. `T=10,000; only A1=1,000 and B1` → seed 8,000; A1 1,000; B1 1,000.

### 3.6 Width contract (ACCEPTED)

| Value | Width |
| --- | --- |
| Transferred SOL, pot, seed, management, pending toll, each local funded/pending credit | checked u64 |
| Owed earnings, cumulative counters, Backer amount/entitlement/lifetime credited, UserRound gross/net, global funded claims | checked u128 |
| Sealed A denominator, `slice × headroom` products | fixed U256 (ethnum), serialized 32-byte little-endian |

Owed may exceed u64; compare to pot before narrowing. A narrower draft width is a defect (DR-05).

### 3.7 Conservation identities (ACCEPTED; MODELED for the game vault)

```text
game_vault.lamports - game_vault.protected_rent
  == pot + seed + management + pending_toll + global_funded_claims + unallocated_excess
ops_cash.lamports - ops_cash.protected_rent
  == ops.free_balance + Σ CloseRecord.locked_sponsor (unsettled)  (+ Σ locked_owner, tracked separately)
```

Entitlements are promises, not cash. Pending credits and pending seed are subdivisions of `pending_toll` until commit. A claim lowers `global_funded_claims` and physical lamports by the same amount. Anonymous transfers into the game vault raise `unallocated_excess` and nothing else; no instruction spends them in first release.

### 3.8 Parity matrix (reconciled)

| Feature | First-release contract | Class |
| --- | --- | --- |
| Three plans, gross accrual, no principal refund | §3.2–3.3 | Preserve |
| Front-End Load | floor(P·400/10,000) → management | Preserve |
| Carried Interest / Jackpot Fee | 1,200/750/300 bps by day 7/14; 900/1,300 bps | Preserve |
| Backer toll | half seed; 35/45/20 senior/post-senior-A/equal-B with folding, caps, residue → seed | Preserve (frontend text stale) |
| Integer precision | 1-lamport floors, remainder carry, floor Q18 | Accepted difference from legacy Float |
| Insolvency | strict owed > pot; toll on actual gross paid; equality solvent; owner zero-pot close with min 0 | Preserve trigger; accepted integer partition |
| Network fees | fee payer pays; no 5,000-lamport deduction; no silent tiny success | Accepted difference |
| Backer history | same-owner aggregation; first-funding seniority on reopen; lifetime caps survive claims; permanent A/B existence | Preserve; sequence replaces subsecond timestamp |
| Public A | enabled at fresh launch | Accepted difference from legacy default-off |
| Minimum / quota | 0.01 / 0.05 SOL; 3 successes per rolling hour; no dynamic cap, cooldown or population cap | Preserve minima; accepted atomic tightening |
| Promotion | preferred = losers with no native A/B history, else all losers; P/L = gross game deposits − net game payouts incl. closed positions; B = floor(loss·116/100) | Preserve |
| Randomness outage | one bound draw; capture ≤ close+600 s; expire >; skip reasons Deadline/NoCandidates/EntropyExhausted; seed carries once after cash commit | Accepted availability refinement |
| Processing | measured inline or bounded permissionless chunks; one-row fallback; narrow barrier | Accepted availability difference |
| Operating costs | sponsor storage/provider/B reserve; zero keeper reward; bounded owner contribution default 0 | Accepted infrastructure difference |
| Unsolicited SOL / recovery | unallocated excess; no credit/refund/sweep/winner/reset ABI | Accepted native restriction |
| PP / social / desk | ICP; authenticated two-RPC event ingestion with per-leg outbox; legacy desk separate | Separate rail, disclosed trust |
| GUI | existing screens and wallet stack behind a typed adapter; gross projection; correct split text | Preserve presentation |

---

## 4. Round lifecycle (ACCEPTED; MODELED)

```text
Active
  --(owner payout with owed > pot)--> Closing{cash: AllocatingClosingToll | Committed}
  --(closing ticket Commit, or zero toll)--> Closing{candidates: Counting}
  --(cursor == participant_count)--> Closing{candidates: Sealed, draw: NotRequested}
  --(request_round_draw, Clock <= deadline)--> draw: Requested
  --(capture_round_draw, Clock <= deadline)--> draw: Captured
  --(apply_promotion)--> promotion: Applied | Skipped(EntropyExhausted)
  --(expire_promotion, Clock > deadline, no capture)--> promotion: Skipped(Deadline)
  --(sealed with selected_count == 0)--> promotion: Skipped(NoCandidates)
  --(finalize_round: cash Committed && promotion terminal)--> Active(round_id + 1)
```

Rules (all ACCEPTED): one immutable `promotion_deadline = close_started_at + 600` Clock seconds pinned at the closing payout; never reset. Timely capture (`Clock <= deadline`, equality inclusive) is immutable and may be applied later; expiry requires `Clock > deadline` and no timely capture. No reroll, no second seed, no fallback winner, no admin unlock. Candidate-only work may be abandoned at expiry; cash allocation, pending credits, funded claims and seed may not. `finalize_round` carries the **full** seed exactly once, resets round-local counters and reopens admission; duplicate finalization of the same close is a no-op returning the original result. Direct `open_position` against a Closing round fails; a client may compose `expire_promotion → finalize_round → open_position(expected_round_id = old + 1)` in one transaction.

Ordinal mapping (ACCEPTED; MODELED): `r_c = u64_le(SHA256("MC/EEC/INDEX/v1" || output_64 || round_id_le || c_le)[0..8])`, `limit = floor(2^64 / N) * N`, counters `c = 0..63`, first `r_c < limit` is mandatory, ordinal `r_c mod N`; all 64 failing → `Skipped(EntropyExhausted)`.

Protocol seed (ACCEPTED; MODELED): `SHA256("SchemeWorks/MC/EEC/v1" || cluster_domain_32 || program_id_32 || round_id_le || close_id_le || set_tag_u8 || selected_count_le || sealed_digest_32)`. `cluster_domain_32` is `Pool.cluster_domain`, written once by `initialize` from the genesis argument and never writable afterwards; devnet and mainnet deployments carry different values, so a request binding cannot be replayed across clusters and no instruction can rebind an active round. Plan M7 proves the exact seed bytes against a host-computed vector for both domains.

### 4.1 Permission matrix during processing (ACCEPTED)

| Operation | Nonclosing ticket active | Closing | Admission paused |
| --- | --- | --- | --- |
| `open_position` | Allow | Reject | Reject |
| `fund_series_a`; registry mutation | Reject | Reject | Reject |
| another payout | Reject (wait for commit) | Reject | Allow if Active and no ticket |
| `claim_backer` (finalized funds) | Allow | Allow | Allow |
| phase instructions (`discover_rows` … `commit_ticket`), provisioning, cleanup | Allow (any payer) | Allow | Allow |
| candidates / draw / expire / finalize | n/a | Allow per state | Allow |
| `withdraw_management` | Allow | Allow | Allow |
| config activation | Reject | Reject | Allow if Active and no ticket |

---

## 5. Governance and operations (ACCEPTED, D8/D6)

- **G** — autonomous Squads v4 2-of-3, `time_lock = 172,800 s`, `config_authority = Pubkey::default()`, no spending limits. Its Vault PDA is the program upgrade authority and the only signer for `stage_config_version`, recipient and guardian rotation. Application instructions require the Vault as a CPI signer; member signatures are collected upstream in Squads. G is also the ORAO registration payer: the pinned SDK's `Register` requires the registered program's upgrade authority to sign as `payer` (`solana/probes/provider-resources/vendor/orao-solana-vrf-cb/src/instructions/register/mod.rs:85–92`), so three distinct roles result: the account's `AccountInfo.owner` is the ORAO program; `Client.owner = G Vault`, which may withdraw Client funds or transfer the Client under ORAO rules (`state/client.rs:14–19`) and does so only through a G-delayed Squads transaction; `Client.state = RngAuthority`, the program PDA that CPI-signs requests (`client.rs:20–23`). Registration, like every G action, is a Squads vault transaction.
- **P** — separate immediate 2-of-3 Squads; its Vault alone may `set_admission_pause`; the instruction's account list contains only `Admission` and the P Vault signer. `Admission.guardian_vault` is the authoritative key the pause instruction checks; `rotate_guardian` (G) validates the replacement multisig's 2-of-3 / zero-delay profile and writes `Admission.guardian_vault` and the `Pool.guardian_vault` copy in one instruction, so after rotation the old P is rejected, the new P is accepted, and G can recover a lost P quorum without widening the pause surface.
- **Management recipient** signs `withdraw_management` to itself, bounded by accrued Front-End Load.
- **Sponsor recipient** signs `withdraw_unreserved_operations` to itself, bounded by free accounted balance.
- **Initialization** requires `ProgramData.upgrade_authority == G Vault` and the G Vault signature; no first-caller enrollment; pot and seed start at zero. The profile check reads the Squads `Multisig` account with the correct 8-byte discriminator (`sha256("account:Multisig")[0..8]`), a plain 32-byte `config_authority` that must equal `Pubkey::default()`, `threshold == 2`, exactly three voting members, `time_lock == 172_800` for G and `0` for P, and derives the Vault PDA from the multisig with vault index 0 to compare against the signer. Squads program ID is pinned to `SQDS4ep65T869zMMBKyuUq6aD6EgTu8psMjkvj52pCf` on mainnet and devnet.
- **Prospective change only:** a staged `ConfigVersion` activates no earlier than `effective_round_id`, only in `Active` with no ticket; `Position`, `Ticket` and `CloseRecord` pin the terms they were created under.
- **Disclosure:** the 48-hour delay holds while Squads settings and Vault ownership are intact; a quorum can shorten its own delay and can upgrade arbitrary code after the delay. Renounced upgrades and custom timelocks are deferred options, not promises.

Solana note: Squads is a program, not a wallet. The "Vault" is a PDA of the Squads program; when a Squads transaction executes, the Squads program signs for that PDA via CPI into ours. Our `initialize` therefore checks that the loader's recorded `upgrade_authority` *is* that PDA, which is the Solana analogue of checking a canister's controller list, except the check is against the loader's account rather than our own settings.

---

## 6. Account contract (DRAFT names adopted per DR-01; additions PROPOSED)

All accounts use Anchor's 8-byte discriminator, store their bump, and are owned by the program unless stated. Sizes are **to be measured**; the budget doc's byte assumptions are planning inputs only. Seeds are byte strings; integers are little-endian.

| Account | Seeds | Rent payer / refund | Lifetime | Key fields (widths per §3.6) |
| --- | --- | --- | --- | --- |
| `Pool` | `["pool"]` | sponsor (init) | permanent | `cluster_domain [u8;32]` (immutable genesis), `round_id u64`, `phase`, `active_config_version u32`, `pot u64`, `seed u64`, `management u64`, `pending_toll u64`, `global_funded_claims u128`, `unallocated_excess u64`, `protected_rent u64`, `registry_len u64`, `registry_version u64`, `next_first_funding_seq u64`, `next_ticket_id u64`, `last_finalized_ticket u64`, `ticket_active bool`, `snapshot_pages u64`, `registry_pages u64`, `participant_count u64`, `participant_pages u64`, `next_close_id u64`, `current_close_id Option<u64>`, `governance_vault`, `guardian_vault`, `management_recipient`, `sponsor_recipient`, stats `total_gross_deposits u128`, `total_net_payouts u128` |
| `Admission` | `["admission"]` | sponsor | permanent | `guardian_vault Pubkey` (authoritative; written only by `initialize` and `rotate_guardian`), `paused bool`, `changed_at i64` |
| `GameVault` | `["game-vault"]` | sponsor | permanent | zero data; program-owned lamport holder; lamports moved by direct debit/credit |
| `Ops` | `["ops"]` | sponsor | permanent | `free_balance u64`, `locked_total u64`, `protected_rent u64`, `provider_prepaid u64` (lamports held in the ORAO Client: funded fees not yet consumed plus reimbursed pending rent; withdrawable to OpsCash only by G via ORAO `withdraw`), `total_contributed u128`, `total_withdrawn u128`, `total_provider_consumed u128`, `total_rent_funded u128` |
| `OpsCash` | `["ops-cash"]` | sponsor | permanent | **system-owned**, zero data; pays shared rent via `invoke_signed` (DR-06) |
| `RngAuthority` | `["rng-authority"]` | sponsor | permanent | ORAO `Client.state` request authority; CPI-signs `request` only; records `client Pubkey`. It is **not** `Client.owner` (that is G) and not the account owner (that is the ORAO program) |
| `ConfigVersion` | `["config", version u32]` | sponsor (G stages) | permanent | `effective_round_id u64`, `staged_at`, `activated_at Option`, plan table (rate bps/day, duration s, fee tiers), `front_end_load_bps=400`, `min_game`, `min_a`, `quota_count=3`, `quota_window=3600`, `seed_bps=5000`, `senior_bps=3500`, `a_bps=4500`, `b_bps=2000`, `a_entitlement_bps=12400`, `b_entitlement_bps=11600`, `open_cutoff=10`, `deadline_seconds=600`, close cost schedule `{ provider_request_fee_lamports (LAUNCH, from NetworkState), provider_fee_headroom_bps (LAUNCH, e.g. 2_000), pending_request_bytes=759, fulfilled_request_bytes=147 }` (account sizes for CandidatePage, RegistryPage, Backer come from the program's `LEN` constants and rent from `Rent::get()`), provider tuple (`program`, `network_state`, `treasury`, `client`), `public_series_a_enabled`, `page_capacity=8` |
| `User` | `["user", owner]` | owner | permanent | `owner`, `has_ever_a bool`, `has_ever_b bool`, `quota_timestamps [i64;3]`, `created_at` |
| `UserRound` | `["user-round", round u64, owner]` | owner | permanent (history) | `round_id`, `owner`, `gross_deposits u128`, `net_payouts u128`, `participant_slot u64` |
| `Position` | `["position", owner, nonce u64]` | owner | **permanent** (no rent reclamation in first release, DR-30; the PDA itself is the nonce tombstone) | `owner`, `nonce`, `round_id`, `config_version`, `plan u8`, `gross u64`, `opened_at`, `expires_at`, `last_accrued_at`, `remainder u32`, `accrued_gross u128`, `gross_paid u128`, `net_paid u128`, `status`, `closed_at` |
| `Backer` | `["backer", series u8, owner]` | owner (A); ops + optional owner component (B) | permanent | `owner`, `series`, `amount u128`, `entitlement u128`, `lifetime_credited u128`, `funded_claimable u64`, `pending_ticket_id u64`, `pending_credit u64`, `first_funding_seq u64`, `registry_slot Option<u64>`, `rent_funded_by_ops u64`, `rent_funded_by_owner u64` |
| `RegistryPage` | `["registry", page u64]` | ops (+ owner component) | permanent (never closable) | `page u64`, `rows [ {owner, series u8} ; 8 ]`, rent components |
| `SnapshotPage` | `["snapshot", page u64]` | ops (+ owner) | permanent, reusable, **never closable** in first release (R3): `pool.snapshot_pages` only grows, so the capacity check `snapshot_pages·8 >= registry_len` is a live invariant | `ticket_id u64`, `page u64`, `rows [ {owner, series, original_slot u64, remainder u128, weight u128, credited u64, done bool} ; 8 ]`, rent components |
| `Ticket` | `["ticket"]` | sponsor | permanent singleton (reused) | `ticket_id`, `phase`, `closing bool`, `close_id Option`, `toll u64`, `seed0`, `senior_slice`, `a_slice`, `b_slice`, `captured_registry_version`, `captured_len`, `discover_cursor`, `allocate_cursor`, `prune_cursor`, `initial_a_count`, `initial_b_count`, `a_remainder_total U256` (discovery accumulator; a sum of u128 remainders can exceed u128), `oldest_a {owner, seq, slot}`, `senior_credit u64`, `sealed_a_denominator U256`, `credited_total u64`, `seed_destination u64`, `work_units_total`, `work_units_done`, pinned split bps |
| `ParticipantPage` | `["participants", round u64, page u64]` | ops (+ owner) | until the round's `CloseRecord.finalized` (identity-bound predicate), then closable | `round_id`, `page`, `rows [owner ; 8]`, rent components |
| `CloseRecord` | `["close", close_id u64]` | ops (+ owner) | permanent | `close_id`, `round_id`, `config_version`, `close_started_at`, `promotion_deadline`, `closing_ticket_id Option`, `cash_committed bool`, `participant_count`, `candidate_cursor`, `preferred_count`, `fallback_count`, `preferred_digest [u8;32]`, `fallback_digest`, `selected_set_tag u8`, `sealed bool`, `draw_state`, `protocol_seed [u8;32]`, `request_account`, `randomness [u8;64]`, `captured_at`, `promotion` (Pending / Applied{winner, entitlement} / Skipped{reason, diagnostic_stage}), `finalized bool`, **captured reserve** `required_reserve u64`, `reserve_schedule { per_candidate_page_rent, per_registry_page_rent, b_account_rent, pending_request_rent, provider_fee, headroom_bps }` (snapshot at close), `candidate_pages_required u16`, `candidate_pages_existing u16`, `registry_extension_required bool`, `locked_sponsor u64`, `locked_owner u64`, `owner_refund_destination`, `client_funded u64`, `provider_consumed u64`, `reserve_settled bool`, rent components |
| `CandidatePage` | `["candidates", close_id u64, set_tag u8, page u64]` | ops (+ owner) | until `CloseRecord(close_id).promotion` is terminal (identity-bound predicate), then closable | `close_id`, `set_tag`, `page`, `rows [owner ; 8]`, rent components |

Provider-owned accounts (not ours): ORAO `NetworkState`, `Treasury`, registered `Client` (owned by `RngAuthority`), `RequestAccount` (pending 759 B → fulfilled 147 B, PROBED). ORAO program ID `VRFCBePmGTpZ234BhbzNNzmyg39Rgdd6VgdfhHwKypU` on devnet and mainnet (public README, 2026-10-13); the published SDK exposes no close instruction, so a fulfilled request is assumed retained on chain until devnet proves otherwise.

Draft-to-spec delta for `state.rs` (inventory §5.1): add `ConfigVersion`, `CloseRecord`, `ParticipantPage`, `CandidatePage`, `RngAuthority`; move `closing/closed_at/deadline` out of `Pool` into `CloseRecord`; add `Pool.cluster_domain`; keep the draft's `Admission.guardian` as the authoritative `guardian_vault` and make `rotate_guardian` write it; add `config_version` and `accrued_gross` to `Position`; widen `Ticket.a_remaining` accumulation to U256 (the draft's `[u8;32]` field is the right width; the accumulation path must use it); add the two rent components to every sponsor-created account; widen `Pool.funded_claims` to u128; add the captured reserve fields to `CloseRecord`; write `User.has_ever_b` and the B entitlement at promotion. Every other draft field maps one-to-one onto the table above.

Solana note: "rent" is a refundable deposit that keeps an account alive, roughly 5,080 lamports per byte at the dated public quote plus a 128-byte overhead. It returns to whoever is recorded as the refund destination when the account is closed. That is why every shared page above records *who funded it* in at most two components (DR-13); the actual payer of the `create_account` CPI is the `OpsCash` PDA, which is not the same thing as the beneficial funder.

---

## 7. Instruction contract

Conventions: **S** = required signer; **W** = writable; **P** = payer of new rent; **CPI** = cross-program invocation performed. Every instruction fails atomically on any check; Solana reverts all account writes in a failed transaction, so "rollback" below only lists state that must *not* be partially observable across instructions. Replay rules state what a second identical submission does.

### 7.1 Control

| Instruction | Signers / payers | Writable | CPI | Preconditions | Postconditions | Replay |
| --- | --- | --- | --- | --- | --- | --- |
| `initialize(genesis)` | S: G Vault (via Squads CPI), S/P: sponsor payer | Pool, Admission, GameVault, Ops, OpsCash, Ticket, RngAuthority, ConfigVersion#1 | system create ×7 | `Program`/`ProgramData` passed; `ProgramData.upgrade_authority == G Vault`; `Pool` absent; genesis config complete; pot = seed = 0 | all singletons exist; `active_config_version = 1`; `round_id = 1`; `next_close_id = 1` | fails: `Pool` exists |
| `stage_config_version(terms)` | S: G Vault; P: OpsCash | ConfigVersion#(n+1), Ops | system create | `effective_round_id >= pool.round_id + 1`; terms within accepted bounds | staged version exists | fails: version exists |
| `activate_config_version(v, expected_current)` | none (any payer) | Pool | — | `v == expected_current + 1 == pool.active + 1`; `pool.round_id >= effective_round_id`; phase Active; `!ticket_active`; `!admission.paused` or paused (allowed) | `active_config_version = v` | no-op fail: already active |
| `set_admission_pause(paused)` | S: P Vault | Admission **only** | — | `signer == admission.guardian_vault` | `paused` set | idempotent |
| `rotate_recipient(kind, new)` | S: G Vault | Pool | — | — | recipient updated; takes effect immediately for future calls | idempotent |
| `rotate_guardian(new_multisig)` | S: G Vault | Admission, Pool; reads new Squads `Multisig` | — | new multisig passes the P profile (autonomous, threshold 2, three voters, `time_lock == 0`); new vault != G vault | `admission.guardian_vault` and `pool.guardian_vault` both set to the new vault in one instruction; old P rejected from the next slot | idempotent |
| `withdraw_management(amount)` | S: management recipient | Pool, GameVault, recipient | — | `amount <= pool.management`; recipient == stored | `management -= amount`; lamports moved to recipient | fails when insufficient |
| `fund_operations(amount)` | S: any | Ops, OpsCash, funder | system transfer | `amount > 0` | `free_balance += amount`; `total_contributed += amount`; **no** Position/PP/quota/Backer effect | repeatable |
| `withdraw_unreserved_operations(amount)` | S: sponsor recipient | Ops, OpsCash, recipient | system transfer (signed by OpsCash) | `amount <= free_balance`; `ops_cash.lamports - amount >= protected_rent + locked_total` | `free_balance -= amount` | fails when insufficient |
| `provision_page(kind, index)` / `provision_close_record(close_id)` | none; P: OpsCash (or S: caller as self-funder) | new page/record, Ops, Pool | system create (signed by OpsCash) | `kind ∈ {Registry, Snapshot, Participant{round == pool.round_id}, Candidate{close_id ∈ {current_close_id, next_close_id}, set_tag}}`; index is the next sequential one for that kind; `free_balance >= rent` when ops-funded | account exists; `pool.registry_pages`/`snapshot_pages` grow monotonically; candidate pages provisioned for `next_close_id` reduce the next close's `required_reserve`; rent components recorded | fails: exists |
| `top_up_close_reserve(close_id)` | none | CloseRecord, Ops | — | Closing; `promotion == Pending`; `Clock <= deadline`; `locked_sponsor + locked_owner < required_reserve` (e.g. after a live provider fee rise) | moves exactly the shortfall from `ops.free_balance` into `locked_sponsor`; cannot change seed, deadline, candidates or weights | no-op fail when not short |
| `close_shared_account(kind)` | none | account, Ops, OpsCash, owner refund destination | system transfer | **only** `ParticipantPage{round}` when that round's `CloseRecord.finalized`, or `CandidatePage{close_id}` when `CloseRecord(close_id).promotion` is terminal; both predicates are bound to the page's own seeds, never to a reusable tag. `Snapshot`, `Registry`, `CloseRecord`, `Backer`, `User`, `Position` → `NotClosable` | lamports returned by component exactly once; account closed | fails: already closed |

### 7.2 Game

| Instruction | Signers / payers | Writable | CPI | Preconditions | Postconditions | Replay |
| --- | --- | --- | --- | --- | --- | --- |
| `open_position(plan, gross, client_nonce, expected_round_id, expected_config)` | S: owner (lamport source; GUI default fee payer); P: owner for User/UserRound/Position | Pool, GameVault, User, UserRound, Position, ParticipantPage(current), ConfigVersion(active, read) | system transfer owner→GameVault; system create (owner-paid) | `!paused`; phase Active (nonclosing ticket OK); `round_id == expected`; `active_config == expected`; `gross >= min_game`; quota has < 3 live successes; prospective-B width check; Position PDA absent; participant page has capacity | `management += load`; `pot += gross - load`; Position open with pinned terms; `UserRound.gross += gross`; first position in round appends owner to participants; quota timestamp pushed; `PositionOpened` | fails: Position exists (nonce reuse is safe) |
| `fund_series_a(gross, expected_round_id)` | S: owner; P: owner for User/Backer A | Pool, GameVault, User, Backer(A), RegistryPage(tail) | system transfer; system create | `public_series_a_enabled`; `!paused`; Active; `!ticket_active`; `gross >= min_a`; quota; entitlement increase fits u128; registry tail page has capacity | `pot += gross` (no load); `Backer.amount += gross`; `entitlement += floor(gross·124/100)`; first funding sets `first_funding_seq`; if not in registry and remaining > 10 → append with original seq; `has_ever_a = true`; quota pushed; `SeriesAFunded` | repeatable (merge) |
| `withdraw_simple(position, minimum_receive, destination, max_work_contribution)` | S: owner; P: owner only if contributing | Pool, GameVault, Position, UserRound, destination, Ticket (if toll>0), CloseRecord(next) (if closing), Ops (if reserve), SnapshotPages (read for provisioning check) | — | Position open & Simple & `round_id == pool.round_id` (a stale-round Position is terminal by round validity with no write and fails `PositionRoundClosed`); `!closing`; `!ticket_active`; `now >= last_accrued_at`; computed `net >= minimum_receive`; destination is not GameVault/OpsCash/any program PDA; if toll > 0: `snapshot_pages·8 >= registry_len`; if closing: CloseRecord(next) exists and `required_close_reserve` (§8) is fully lockable, sponsor first then owner up to `max_work_contribution` (DR-12) | §3.4 effects; Position remainder/last_accrued/status updated; if toll > 0 ticket captured (`registry_version`, `registry_len`, work units); if closing: phase Closing, CloseRecord pinned with deadline, `participant_count` copied, locked partitions set; `PayoutPaid`, optionally `RoundClosed` | second call recomputes from new state; cannot double-pay |
| `settle_compounding(position, minimum_receive, destination, max_work_contribution)` | same | same | — | as above but `now >= expires_at`; plan Compound | owed = floor(P·Q18/10^18); fee 900/1,300 bps; Position Closed; same pipeline | fails: Closed |
| `close_simple_position(position)` | S: owner | Position, UserRound | — | Simple, `now >= expires_at`, owed == 0 (else use withdraw) | Position Closed; `PositionClosed` | fails: Closed |

Position accounts are permanent in the first release (R1, DR-30): the PDA at `["position", owner, nonce]` is itself the durable nonce tombstone, so a client that retries an open after an unknown outcome either finds its Position (success) or opens it exactly once; the same nonce can never create a second funded Position after settlement because nothing can close the account. A stale-round Position needs no stale-to-closed path: every payout path rejects it without writing. Compact tombstones with rent reclamation are deferred (DR-30).

### 7.3 Allocation and claims

| Instruction | Signers / payers | Writable | CPI | Preconditions | Postconditions | Replay |
| --- | --- | --- | --- | --- | --- | --- |
| `discover_rows(expected_ticket_id, expected_cursor, rows)` | none (any payer) | Ticket, Pool (read), RegistryPage(s) covering the slots, SnapshotPage(s) covering the slots, Backer×rows (read) | — | `ticket_active`; `phase == Discover`; `ticket.ticket_id == expected_ticket_id`; `discover_cursor == expected_cursor`; `1 <= rows <= page_capacity`; each supplied Backer re-derives from the registry row at its slot; `pool.registry_version == captured_registry_version` | snapshot rows written with `ticket_id`, original slot and captured remainder; `a_remainder_total (U256) += remainder` for A rows; initial A/B counts; oldest open A tracked; cursor advanced; at `cursor == captured_len` → `Senior` | stale ticket → `StaleTicket`; stale cursor → `StaleCursor`; no re-write |
| `apply_senior(expected_ticket_id)` | none | Ticket, Backer(oldest A) | — | `phase == Senior`; ticket id matches; Backer is the tracked oldest open A | senior slice credited up to remaining (`pending_credit`, `lifetime_credited`), overshoot → `seed_destination`; → `SealWeights` | fails: phase |
| `seal_weights(expected_ticket_id)` | none | Ticket | — | `phase == SealWeights`; id matches | `sealed_a_denominator = a_remainder_total - senior_credit` (U256); → `Allocate` (or `Prune` when no rows) | fails: phase |
| `allocate_rows(expected_ticket_id, expected_cursor, rows)` | none | Ticket, SnapshotPage(s), Backer×rows (writable) | — | `phase == Allocate`; id and `allocate_cursor` match; each Backer re-derives from its snapshot row; **no duplicate Backer key within the batch** | A rows: `floor(a_slice · weight / denominator)` capped; B rows: equal floor share capped; `pending_credit`, `lifetime_credited`, `credited_total`, residue → `seed_destination`; cursor advanced; → `Prune` | stale → `StaleCursor`; no re-credit |
| `prune_rows(expected_ticket_id, expected_cursor, rows)` | none | per logical row: SnapshotPage, Backer(pruned), RegistryPage(pruned slot), RegistryPage(last live slot), Backer(moved) | — | `phase == Prune`; id and descending `prune_cursor` match; pruned row is newly capped (`remaining <= 10`). **Alias rules:** the two RegistryPages may be the same account and are decoded and written once; when the pruned slot is the last live slot, `Backer(moved)` aliases `Backer(pruned)` and no moved write occurs; any other duplicate writable Backer key within the batch → `DuplicateWritableAccount` | swap-delete; moved row's `registry_slot` updated; pruned row `registry_slot = None`, history bits untouched; `registry_len` decremented; cursor advanced; → `Commit` | stale → `StaleCursor` |
| `commit_ticket(expected_ticket_id)` | none | Ticket, Pool, CloseRecord (if closing) | — | `phase == Commit`; id matches; `credited_total + seed_destination == toll` | `pending_toll -= toll`; `seed += seed_destination`; `global_funded_claims += credited_total`; `last_finalized_ticket = ticket_id`; `registry_version += 1` if any prune; `ticket_active = false`; closing → `CloseRecord.cash_committed = true`; `TicketCommitted` | second call fails: not active |
| `claim_backer(destination)` | S: owner | Pool, GameVault, User, Backer A (if `has_ever_a`), Backer B (if `has_ever_b`), destination | — | at least one Backer present; destination not a protected account | for each present Backer: if `pending_ticket_id <= last_finalized_ticket` then `funded += pending; pending = 0`; `amount = Σ funded`; `funded = 0`; `global_funded_claims -= amount`; lamports moved; lifetime unchanged; `BackerClaimed` | second call pays zero (fails with NothingToClaim) |

### 7.4 Close, candidates, randomness, finalization (all permissionless)

| Instruction | Writable | CPI | Preconditions | Postconditions | Replay |
| --- | --- | --- | --- | --- | --- |
| `build_candidates(rows)` | CloseRecord, CandidatePages (preferred/fallback tails), reads ParticipantPage rows, UserRound and User per row | — | Closing; `cash_committed` (or zero-toll close); `!sealed`; supplied rows re-derive to participant slots `cursor..cursor+rows` | each owner classified once; appended to preferred (loss>0 and no A/B history) and/or fallback (loss>0); cursor advanced; at `cursor == participant_count`: digests/counts sealed, `selected_set_tag` chosen; `selected_count == 0` → `promotion = Skipped(NoCandidates)`; `CandidatesSealed` | cursor mismatch fails |
| `request_round_draw()` | CloseRecord, RngAuthority, Ops, OpsCash; ORAO NetworkState, Treasury, Client, RequestAccount | (1) system transfer OpsCash→Client of `live_request_fee + pending_request_rent`, debited from `locked_sponsor` only (never `locked_owner`); (2) ORAO `request` CPI signed by RngAuthority | sealed; `selected_count > 0`; `draw_state == NotRequested`; `Clock <= deadline`; provider tuple == pinned config; `Client.state == RngAuthority`, `Client.owner == G Vault`, `AccountInfo.owner == ORAO program`; live `NetworkState.request_fee <= captured_fee · (1 + headroom_bps/10_000)` and funds lockable, else `ProviderFeeExceeded` (use `top_up_close_reserve` or let expiry run) | `protocol_seed` derived; request created; marker `Requested` written atomically with CPI success; `client_funded += transferred`; `provider_consumed += live fee`; `ops.provider_prepaid += transferred`; `DrawRequested` | fails: Requested; unexpected pre-existing request account → collision error, no alternate seed |
| `capture_round_draw()` | CloseRecord; reads RequestAccount | — | `Requested`; account owner == ORAO program; address == stored; client/seed/bump match; fulfilled; `Clock <= deadline` | `randomness` copied (64 bytes); `Captured`; `DrawCaptured` | idempotent after capture; late (> deadline) → `IgnoredLate` no-op |
| `apply_promotion()` | CloseRecord, Pool, winner User, winner Backer B (init from locked reserve or merge), RegistryPage tail (new page if `registry_extension_required`, pre-funded from the captured reserve), reads CandidatePage(ordinal), winner UserRound | system create (B, registry page) | `Captured`; `promotion == Pending` | counters 0..63 mapped; winner from `CandidatePage[ordinal/8][ordinal%8]`; `entitlement += floor(loss·116/100)`; `has_ever_b`; registry append if open; `Applied` (or `Skipped(EntropyExhausted)`); locked reserve settled (DR-14); `PromotionApplied`/`PromotionSkipped` | fails: terminal |
| `expire_promotion()` | CloseRecord, Ops | — | Closing; `promotion == Pending`; `Clock > deadline`; `draw_state != Captured` | `Skipped(Deadline)` with diagnostic stage ∈ {`CandidatesIncomplete`, `Unrequested`, `ProviderFeeExceeded`, `RequestedUnfulfilled`}; locked reserve settled (§8); `PromotionSkipped` | idempotent fail: terminal |
| `finalize_round()` | Pool, CloseRecord | — | Closing; `cash_committed`; promotion terminal; `!finalized` | `pot += seed; seed = 0`; `round_id += 1`; `participant_count = 0`; `current_close_id = None`; phase Active; `finalized = true`; `RoundFinalized` | same close → Ok no-op (original result) |

### 7.5 Events (PROPOSED DR-11, `emit_cpi!`)

`PositionOpened{owner, nonce, round, plan, gross, load, config}`, `SeriesAFunded{owner, gross, entitlement_after}`, `PayoutPaid{owner, nonce, gross_paid, toll, net, destination, closing, ticket_id}`, `PositionClosed`, `TicketCommitted{ticket_id, toll, credited, seed}`, `BackerClaimed{owner, amount, a_part, b_part}`, `RoundClosed{round, close_id, deadline}`, `CandidatesSealed{close_id, tag, count, digest}`, `DrawRequested`, `DrawCaptured`, `PromotionApplied{close_id, winner, loss, entitlement}`, `PromotionSkipped{close_id, reason, stage}`, `RoundFinalized{close_id, new_round, seed_carried}`, `OperationsFunded`, `OperationsWithdrawn`, `ManagementWithdrawn`, `ConfigStaged`, `ConfigActivated`, `AdmissionPauseSet`. Every event carries `schema_version: u8`. The program enables Anchor's `event-cpi` feature; every emitting instruction's `Accounts` struct carries `#[event_cpi]`, which adds the `event_authority` PDA (`["__event_authority"]`) and the program account to its account list. The generated IDL carries the events and these accounts; M2 commits an encoding fixture per event and M9 measures the added accounts and CU per instruction.

### 7.6 Runtime lessons carried into the contract (PROBED)

- Construct errors lazily (`ok_or_else`) in every loop; eager `ok_or(error!())` exhausted the 32 KiB bump heap at 16 repetitions in the probe.
- Decode every account once: in `allocate_rows` and `prune_rows`, duplicate writable Backer keys across independent rows are rejected (`DuplicateWritableAccount`); within one prune row the legitimate aliases (same RegistryPage for pruned and last slot; `moved == pruned` when the pruned slot is the last live slot) are detected by key equality and the account is decoded and written exactly once. Two decoded copies of one account would serialize stale state over each other.
- Measured kernel packets: 8 rows 518 B, 16 rows 782 B, 32 rows 1,310 B (> 1,232 envelope) on a *stripped* kernel. Production rows carry more accounts; eight is the provisional transport candidate and one row is the guaranteed fallback.

---

## 8. Resource reservation, refunds and cleanup (ACCEPTED; DRAFT partially; PROPOSED DR-12–DR-14, revised for R2/R3)

1. **Required close reserve is computed, not configured.** At a payout that would close, the program computes, with checked arithmetic and `Rent::get()`:

```text
pages(n)                  = ceil(n / page_capacity)
candidate_pages_required  = 2 · pages(participant_count)           -- preferred and fallback may each hold every loser
candidate_pages_existing  = pages already provisioned for next_close_id (both sets)
registry_extension        = (registry_len mod page_capacity == 0) ? 1 : 0   -- automatic B may need a new tail page
required_reserve = rent(CandidatePage) · (candidate_pages_required − candidate_pages_existing)
                 + rent(RegistryPage) · registry_extension
                 + rent(Backer)                                       -- one automatic B account, always reserved
                 + pending_request_rent + provider_request_fee · (1 + headroom_bps/10_000)
```

`CloseRecord(next_close_id)` must already exist (sponsor-provisioned). The result scales with the captured participant count and registry tail; nothing in it caps population. The schedule values are snapshotted into `CloseRecord.reserve_schedule`.

2. **Lock before cash moves.** `locked_sponsor` takes `min(required_reserve, ops.free_balance)`; `locked_owner` takes `min(remaining, max_work_contribution_lamports)`; if `locked_sponsor + locked_owner < required_reserve` the payout fails before any mutation. Spare sponsor cash is never mistaken for a smaller obligation: the lock is exactly `required_reserve`. The owner partition may fund **program-owned storage only** (candidate pages, registry page, B account), never provider fees or provider rent.

3. **Consumption** is sponsor-first by category: provider fee and pending-request rent are transferred OpsCash→Client at `request_round_draw` from `locked_sponsor`; candidate pages, the registry extension page and the B account are created from `locked_sponsor` then `locked_owner`. A timely captured result therefore always has the storage it needs: the candidate pages exist before sealing, and the B/registry rent is already locked, so promotion never depends on a later sponsor top-up.

4. **Provider cash attribution (disclosed consumption).** The ORAO Client is provider-side working capital owned by G (`Client.owner`). Fulfillment reimburses the excess pending-request rent to the Client, not to Ops or any owner (PROBED locally: 6,173,520 lamports pending, 4,259,520 reimbursed to Client, 300,000 fee to treasury). The program tracks that balance as `ops.provider_prepaid`; G may move it back to OpsCash through ORAO `withdraw` under its delay. Expired, never-fulfilled requests retain their pending rent in the provider account. No Client reimbursement is ever reported or paid as an owner refund.

5. **Settlement** happens once when promotion becomes terminal: unconsumed `locked_sponsor` returns to `ops.free_balance`; unconsumed `locked_owner` returns to the recorded destination; consumed amounts are recorded by category. If the live provider fee rises after capture, `request_round_draw` fails with `ProviderFeeExceeded`; anyone may `fund_operations` and `top_up_close_reserve(close_id)` before the deadline, otherwise expiry records the stage and settles.

6. **Snapshot and registry pages are permanent (R3).** `pool.snapshot_pages` and `pool.registry_pages` only grow; `close_shared_account` refuses them. Their rent is retained sponsor working capital, reclaimable only by a future reviewed upgrade. Capacity accounting is therefore a live invariant: `snapshot_pages · 8 >= registry_len` checked at payout is exactly the storage Discover will touch.

7. **Closable pages are identity-bound.** `ParticipantPage{round}` closes only after that round's `CloseRecord.finalized`; `CandidatePage{close_id, set}` only after `CloseRecord(close_id).promotion` is terminal. The predicate reads the page's own seeds, never a reusable tag, so a page from an older close cannot be closed under a newer ticket by mistake. Rent components return per DR-13. Cleanup is never a precondition for commit, finalization or reopening.

8. **Zero keeper reward.** Any payer may progress. Work units are captured for diagnostics and future pricing, not paid. Cash progress (`commit_ticket`, `finalize_round`, claims) never depends on `ops.free_balance`.

## 9. ICP PP ingestion and social services (ACCEPTED; PROPOSED DR-24/DR-25)

A new `native_sol_bridge` canister (DR-24) owns:

- **Event identity**: `(cluster_domain, program_id, signature, instruction_index, inner_instruction_index, event_schema_version)`, plus `round_id`/`close_id` as source-round provenance kept separate from ICP PP round buckets.
- **Verification**: an authenticated relayer proposes finalized event descriptors; the canister independently fetches the transaction from **two configured RPC providers** via the SOL RPC canister with `maxSupportedTransactionVersion` set to the highest version the raw decoder supports, decodes the self-CPI event, and requires both observations to agree and be `finalized`. Disagreement or unsupported version leaves the event **pending**, never skipped; the cursor advances only over durable outbox entries. Two RPCs are a trusted observation boundary, not consensus.
- **Beneficiary**: the signed owner for `PositionOpened`/`SeriesAFunded`; the selected loser for `PromotionApplied`; never the keeper or relayer signer. Missing SIWS binding leaves a pubkey-keyed pending entitlement; binding later releases it without rebinding history.
- **Policy and effects have one owner: Shenanigans (R6).** The bridge owns observation and the event outbox; Shenanigans owns reward policy, effect/siphon reservation and ledger-leg execution. Protocol: (1) the bridge, after two-RPC finalized verification, calls `shenanigans.reserveNativeReward(event)` (caller allowlisted to the bridge canister). (2) `reserveNativeReward` is idempotent on the immutable event id; on first call it computes **every leg once** under the policy in force at that moment (player base = gross game/A amount or B loss × plan rate, mint multiplier, siphon, cascade legs by then-current upline state, house residual, signup gift if first), **applies the effect consumption and siphon-cap updates at reservation time**, assigns each leg an immutable `{ recipient, amount, fee: null, memo = sha256(domain || event_id || leg_index), created_at_time = reservation time }`, stores `NativeRewardObligation { event_id, legs[], status Reserved }` in stable state and returns without any ledger await. (3) `shenanigans.executeNativeRewardLeg(event_id, leg_index)` (bridge or keeper) executes one leg: `Pending → SubmittedUnknown` before the single `icrc1_transfer` with the frozen fields; `Ok(block) → Committed{block}`; `Duplicate{of} → Committed{of}`; a trap or timeout leaves `SubmittedUnknown` and the next call re-presents **identical** fields inside the ledger dedup window; `TooOld`/`CreatedInFuture` or any outcome past the window → `Quarantined` for ledger-history reconciliation. (4) The bridge advances its cursor only after `reserveNativeReward` returned; leg execution is retried independently. Legacy `mintWithEffects`/`mintInternal` are untouched and never called on the native path.
- **Exactly-once properties this yields**: effects cannot be applied twice because they are consumed at reservation and reservation is idempotent; a committed leg is never re-minted because its status is written before any await and `Duplicate` reconciles to the original block; a failed sibling leg never strands or repeats another leg because legs are executed and recorded independently; no fresh timestamp is ever stamped on retry (contrast `shenanigans/main.mo:2015–2030`) and there is no write-after-await dedup gap (contrast `:2917–2923`). Beneficiary principal resolution uses the pinned `siws_provider` (`tcm26-yqaaa-aaaac-qg2lq-cai`, ic-siws v0.1.0) address→principal lookup; an unresolved address leaves the obligation `Reserved` keyed by pubkey until a permissionless `releasePending(event_id)` resolves it. The exact ic-siws query name is verified against its Candid in M12.
- **Shenanigans touch**: add the two methods above plus stable `NativeRewardObligation` storage and a bridge allowlist; reuse the SOL plan rates (`simple21DayPpPerSol=6_000`, `compounding15DayPpPerSol=12_000`, `compounding30DayPpPerSol=18_000`, `backerPpPerSol=120_000` as currently configured) and the existing referral/effect policy tables read-only at reservation. Legacy observers are unchanged; the existing `solBackerSeen` watermark (owner-keyed only, `shenanigans/main.mo:1612`) is a legacy-rail concern recorded in the inventory, not fixed here.
- **History/feeds**: the bridge's verified event store serves the GUI's native history and feeds (DR-25). Live financial state is never read from ICP.

Solana note: Solana has no `raw_rand`, no certified queries and no inter-canister calls to a ledger. Everything ICP learns about the native game comes from RPC reads of finalized transactions, which is why the bridge's trust statement is "two RPC providers agree," not "the chain told us."

---

## 10. Keepers and monitoring (ACCEPTED D9; DR-21)

- **Primary**: an external routine worker holding one fee-only key with a capped balance, submitting `discover_rows`/`apply_senior`/`seal_weights`/`allocate_rows`/`prune_rows`/`commit_ticket`, `build_candidates`, `request_round_draw`, `top_up_close_reserve`, `capture_round_draw`, `apply_promotion`, `expire_promotion`, `finalize_round`, provisioning and cleanup. It has no financial authority.
- **Watchdog/backup**: an ICP canister (`native_sol_keeper`) with a threshold-Ed25519 fee-only address, timers (idle backoff 1/5/15 min, active 30–60 s), a bounded transaction journal, unknown-outcome reconciliation and re-armed timers after upgrade. It signs only when progress is stale.
- **Alerts**: 60 s without progress → alert; 300 s → escalate (replacement keeper, smaller chunk, self-funded progress, ops top-up). These are operational objectives, not protocol guarantees.
- **Any wallet** can submit the same instructions; the GUI exposes a "Resume processing" builder (§11).
- **Costs** are rebuilt from measured full-path counts (plan Task 11); the 42,170-transaction/month and 12-SOL figures are conditional scenarios.

Solana note: ICP timers and threshold signing can run a keeper, but each signature is a management-canister call (about 26 billion cycles at the dated `key_1` price) and each RPC read is an HTTPS outcall with consensus. That is fine for a watchdog that signs rarely; it is expensive for a worker that signs thousands of times a month, which is why the default is hybrid until measurements say otherwise.

---

## 11. Frontend native adapter (ACCEPTED scope; PROPOSED DR-22/DR-23)

- **Stack unchanged**: `@solana/web3.js 1.98.4`, wallet-adapter 0.15.39, React 18.3.1, Vite 7.3.1, TypeScript 5.6.3, vitest 4.1.7 (from `package-lock.json`). Legacy/v0 transactions. No Kit/v1 migration. Never add `@rollup/rollup-darwin-*`.
- **Module boundary**: `frontend/src/solana/native/{client,accounts,transactions,status,types}.ts` and `frontend/src/hooks/useNativeSolana.ts`. The generated Anchor IDL is checked in; a typed client wraps it. Cache keys include `(cluster, programId)`.
- **Wallet**: re-read the user-selected wallet pubkey at send time (today `useWallet.tsx:388–419` picks the first installed adapter and `sendSolDeposit.ts:62–68` hard-fails on mismatch; the native path keeps the hard-fail and adds a pre-sign check). Simulate before prompting. Keep signature and `client_nonce` across unknown outcomes; never resubmit a new nonce on timeout.
- **Status**: `submitted → confirmed → finalized → mirrored`; HTTPS polling only (CSP `connect-src` in `frontend/public/.ic-assets.json:19` allows no `wss://`); null/timeout is "unknown," not failure.
- **Screens touched, layout preserved**: `GamePlans`/`SolInvestPanel` (native `open_position`; gross-based projection), `GameTracking` (`withdraw_simple`/`settle_compounding`/`close_simple_position`; minimum receive; advanced `max_work_contribution` default 0), `HouseDashboard`/`AddBackerMoney`/`SolBackerPanel` (`fund_series_a`, combined `claim_backer`, 35/45/20 text), `GameStatusBar` (native pot/round), `WalletConnectModal`/`WalletDropdown` (cluster indicator). Legacy Chain Fusion positions render in a labeled secondary section (DR-22, Rob).
- **Processing UX**: show ticket phase/cursor, closing deadline countdown, skipped reason; "Resume processing" prepares the next valid phase instruction (`discover_rows`/`allocate_rows`/`prune_rows` with the current `expected_ticket_id` and cursor, default 8 rows, one-row fallback; or the constant-size phase) or the composed expire/finalize/open transaction for any connected wallet. No auto-signing.
- **Reads**: positions, Backer balances, pot, round and ticket state from Solana accounts directly; ICP mirror only for history/feeds. SIWS failure never gates a SOL transaction.
- **Legacy**: `usePrepareSolDeposit`, `pokeMyDeposit`, derived deposit addresses and manual-transfer fallbacks remain only on the legacy rail.

---

## 12. Testing and proof obligations summary

| Area | Failing-first tests (plan task) | Runtime proof | Status today |
| --- | --- | --- | --- |
| Economics oracle | fixture vectors, boundaries ±1 s, remainder carry, widths | host (`cargo test`) | MODELED (72) |
| Allocation | goldens, folds, caps, chunk 1/4/8 equivalence, prune order, lazy credit, claim during ticket | host + SBF | MODELED (host), DRAFT (SBF) |
| Permissions | wrong signer/owner/PDA/alias/discriminator/version | SBF | PROBED (kernel), DRAFT (production) |
| Custody conservation | identity after every instruction in randomized sequences | SBF | DRAFT |
| Close/candidates/draw | completeness, seal, deadline ±1, late capture, 64-counter exhaustion, expire paths | SBF with captured ORAO | PROBED (lifecycle), DRAFT (production) |
| Resource/refund | one-lamport-short rejection, sponsor-first, once-only refund, cleanup predicates | SBF | not started |
| Frontend | wallet mismatch, unknown outcome, nonce retention, gross projection, split text | vitest + browser | not started |
| Bridge | two-RPC disagreement, version gap, duplicate/unknown ledger results, missing SIWS | Motoko tests + local ledger | not started |
| Nonce permanence (R1) | same nonce after settlement rejected; different nonce valid; unknown-outcome retry idempotent | SBF | not started |
| Captured close reserve (R2) | participants 8/9, registry tail 8/9, both lists full, one-lamport short, spare cash not mistaken, timely capture then B creation without new sponsor dependency | SBF | not started |
| Permanent snapshots / identity-bound cleanup (R3) | snapshot/registry close → `NotClosable`; candidate/participant close only by own-seed predicate; depleted free ops still allows cash progress | SBF | not started |
| Guardian key (R4) | old P rejected after rotation; new P accepted; G recovers lost quorum; pause metas have no writable financial accounts | SBF | not started |
| ORAO roles (R5) | wrong `Client.owner`/`Client.state`/account owner rejected; G registration path; prospective provider change without rebind | SBF + devnet | PROBED (lifecycle), not started (roles) |
| Shenanigans reservation protocol (R6) | effects frozen at reservation; duplicate/concurrent delivery; unknown primary outcome; failed sibling leg; upgrade between calls | Motoko + local ledger | not started |
| Devnet | full lifecycle receipts, G/P readback, ORAO latency, on-chain byte/authority/IDL verification | separately authorized | not started |
