# Native Solana current-state and evidence inventory — 2026-10-13

Author: Claude (planning coordinator). Purpose: exact record of what exists, what was executed, what was not, and what remains unproved, so Codex can review the [reconciled spec](../superpowers/specs/2026-10-13-native-solana-reconciled-spec.md), [decision register](native-solana-decision-register-20261013.md) and [implementation plan](../superpowers/plans/2026-10-13-native-solana-implementation.md) without reconstructing this session. Nothing in this pass built, tested, deployed, signed, funded or merged anything. All hashes are SHA-256 of working-tree bytes at the times stated. Workers used for bounded read-only inventory: three Sonnet-class and one Haiku-class subagent plus one public-docs research agent; the coordinator spot-checked every financial source line cited in the spec.

## 1. Checkouts, branches, PRs

| Item | Value |
| --- | --- |
| Primary checkout | `/Users/robertripley/coding/musicalchairs`, branch `codex/native-solana-discovery-20261012`, HEAD `38cbf153a5ff46487fdbe80314f93acfa7328e63`; untracked and preserved: `.claude/skills/`, `.codex/`, `AGENTS.md`, `docs/coordination/claude-native-solana-planning-prompt.md`, `solana/` |
| Fresh worktree (all planning work here) | `/Users/robertripley/.codex/worktrees/native-solana-model-probes/musicalchairs`, branch `codex/native-solana-accounts-payouts`, HEAD `c99a636b9f040c9bf0d18d3698328d9bbd33b7b2` (not on origin) |
| Worktree dirty state at start (21 paths, untouched by this pass) | modified: `docs/coordination/kanban.md` (+19 lines, production-pass board), `solana/Cargo.toml` (adds `programs/musical_chairs` and `tests/financial-runtime` to `exclude`); untracked: `docs/solana/production-accounts.md`, `solana/programs/musical_chairs/{Cargo.toml,Cargo.lock,.cargo/config.toml,.gitignore,src/{lib,state,controls,positions,allocation}.rs}`, `solana/tests/financial-runtime/{Cargo.toml,Cargo.lock,.cargo/config.toml,.gitignore,scripts/build-sbf.sh,src/{lib,main,runtime,financial}.rs}` |
| Gitignored artifact present | `solana/programs/musical_chairs/artifacts/musical_chairs_program.so`, 615,848 B, SHA-256 `c9f6016e5c96b5eb774a6bfb935b3a9f32c2115221ea6fcb274e0d0a3fbd3c43`, **no provenance record**, mtime earlier than the last edits to `controls.rs`, `financial.rs`, runtime `lib.rs`/`main.rs` |
| PR #123 | `SchemeWorks/MusicalChairs` "Add native SOL economic model and local provider probes", OPEN, not draft, base `main`, head `codex/native-solana-model-probes` @ `830d57377e71aa4f15ad0c7fa86ff2d6b3ff4e0c`, mergeable, **not merged** |
| PR #124 | "Probe local SBF permissions, compute and ORAO recovery", OPEN, **draft**, base `codex/native-solana-model-probes`, head `codex/native-solana-runtime-probes` @ `c99a636…`, mergeable, **not merged** |
| `origin/main` | `f96b33ff95020458be754bfa0d7471b72a7403af`; neither probe branch is an ancestor of main |
| Running processes at start | none matching cargo/anchor/solana/litesvm/graphify/dfx |

Financial source (`ponzi_math_sol/`, `shenanigans/`, frontend) is unchanged from `f96b33f` on every branch above.

## 2. Status ladder (replaces stale "draft / no proof" wording)

| Layer | Content | Evidence | Executed in this pass? |
| --- | --- | --- | --- |
| Accepted policy | D1–D9, defaults register, launch decisions | `docs/solana/launch-decisions.md`, `docs/coordination/native-solana-defaults.md` | n/a |
| Host reference model | `solana/crates/economics` (admission, earnings, fees, backers, rounds) + fixture | `docs/solana/evidence/2026-10-12-model-review.json` records `economic_tests: 72` (unit 5, admission 5, allocation 8, backer_transitions 10, conservation 7, end_to_end 3, rounds 23, vectors 11) and `provider_tests: 5`; three review rounds FAIL/FAIL/PASS | **No** — counts read from receipt |
| Provider/schema probe | `solana/probes/provider-resources` | `docs/solana/evidence/provider-resource-probes.json`; vendored ORAO 0.4.0 archive SHA-256 `a036bfe6…d9a7` | No |
| Local SBF runtime probes | `solana/probes/runtime` (6-instruction probe program + LiteSVM harness + captured ORAO executable) | `2026-10-13-runtime-review.json`: 88 cases, 45 success, 43 expected failure, 88 unique labels; `runtime_ci_executed: false`; `live_transactions_or_deployment: false` | No |
| Production tranche | `solana/programs/musical_chairs`, `solana/tests/financial-runtime` | uncommitted; 7 control handlers implemented, **10 handlers return `NotImplemented`**; harness does not compile (see §5) | No |
| Out-of-repo drafts | `/private/tmp/mc-*-20261013.*` | proposals only | No |

## 3. Evidence receipts (hashed 2026-10-13)

| Path | Bytes | SHA-256 |
| --- | ---: | --- |
| docs/coordination/evidence/2026-10-12-native-solana-costs.json | 8,901 | d026d30cb84d01b866cae0ecfa4aa947a7ec8c44e1b2a5a1f90728f70d67c265 |
| docs/coordination/evidence/2026-10-12-native-solana-graph-fragment.json | 196,350 | f12dd99afd69c48a1e914f971f83c858027457299189ae7ad6d24a59e2eed672 |
| docs/coordination/evidence/2026-10-12-native-solana-graph-refresh.json | 4,457 | 2d48c2d2d08486d44be62400e1a0863ea64d175939567aa9e4e123d60ae73582 |
| docs/solana/evidence/2026-10-12-model-graph-fragment.json | 462,604 | 41b9d8659cacd1d14e66f180fcb5e531a89567eea755627ba6704df71bf4dad3 |
| docs/solana/evidence/2026-10-12-model-graph-refresh.json | 14,046 | c3d9e2e922104c86296e2341fffc27a79bb2274b672837a94681e45a64ade1af |
| docs/solana/evidence/2026-10-12-model-review.json | 4,932 | fd148449693459a28e8114533b503d1f121c8cb467fdf461e059c0b4e654186d |
| docs/solana/evidence/2026-10-13-runtime-graph-fragment.json | 249,697 | c5f97fbed8d81a466f128cb66f1e2867a93a34def962ffdc983237a69d2a0d43 |
| docs/solana/evidence/2026-10-13-runtime-graph-refresh.json | 14,484 | d75828bfe6e4f4edb5ce56861b6e93f28f52938da3dea188525fd4281f6eb201 |
| docs/solana/evidence/2026-10-13-runtime-heap-before.json | 1,786 | 24d2dc219bf7b7a723e80b699d1ffad2f0826685fc1ee67d820bb654cd159f3d |
| docs/solana/evidence/2026-10-13-runtime-probes.json | 202,895 | 80ac81fcc22eaf8b1f323ef54c27f52733c38797400a1703925ff5a64e4bb07c |
| docs/solana/evidence/2026-10-13-runtime-review.json | 4,034 | d5a28c659a8202f31c2c651544726e9607ae0de816f5584b9342a5c10185034f |
| docs/solana/evidence/provider-resource-probes.json | 21,245 | a9a1013e578de3003cd92dbd70e139f7bf5791e986719ff2422d8cf0ded9062c |

Fixture provenance: ORAO executable `solana/probes/runtime/fixtures/orao-callback-mainnet-finalized.so` (616,464 B) SHA-256 `6d180b1e91be7d5c541f75e2413146beb047d9beeb76ac8eb1d7c883200c6b8b` **matches** `orao-provenance.json`; probe program `musical_chairs_runtime_probe_program.so` (351,600 B) `256dec616a7074f20cae2278bafb5874e1b96b57206adb8196dcc8ebd55409d5` **matches** `build-provenance.json`. Runtime receipt details: 16-row kernel chunk 782 B / 89,617 CU fits; 32-row chunk 1,310 B / 192,009 CU does **not** fit the 1,232-byte legacy/v0 envelope.

## 4. Toolchain cache `/private/tmp/mc-solana-runtime-tools-20261013/` (1.9 GB)

| Tool | Established | Note |
| --- | --- | --- |
| cargo-build-sbf | 3.1.0-beta.0; binary SHA-256 `2e999118c4e865e02ed07ad9effc36e4f8f1c2630bb61334fbdb914269170fd6` matches build provenance | OK |
| platform-tools rustc | 1.84.1-dev | matches provenance |
| **platform-tools version** | `cargo-build-sbf --version` prints **v1.52**, which is the builder's *default* tools version when none is requested. The build wrapper passes `--tools-version v1.48 --skip-tools-install` and exports the installed `platform-tools/rust/bin/rustc` (1.84.1-dev) explicitly (`scripts/build-sbf.sh:56–57, 62–70`), so the displayed default does **not** show which compiler was used. Build provenance records v1.48 (archive SHA-256 `799e4cfcee38e227155c83fb2294f96f94a843d42e01c0359fb7128e2216d2dd`, not present in `downloads/`, which is empty); `platform-tools/version.md` holds only commit hashes | **Corrected 2026-10-13b:** the earlier "unresolved pin" wording over-read the version banner. Plan M0 verifies the installed tree's identity (commit stamps in `version.md` against the v1.48 release, binary hashes) before any re-download, and the prior 88-case runtime evidence is not treated as superseded by the banner alone |
| LiteSVM | repo runtime probe lock pins 0.7.1; scratch `litesvm-probe/Cargo.toml` pins 0.6.0; no litesvm crate in `cargo-home` | scratch probe is historical |
| Stray artifacts | `actual-program-output/musical_chairs_runtime_probe_program.so` SHA-256 `f45be9eb…f875` does **not** match the repo artifact; `sbf-output/mc_sbf_runtime_probe.so` unreferenced | historical build outputs; not evidence |

## 5. Unfinished production tranche — exact inventory

Hashes of the uncommitted files (full list with line counts):

| File | Lines | SHA-256 |
| --- | ---: | --- |
| solana/programs/musical_chairs/src/lib.rs | 118 | c84ce7f8c8d6a830121d4ccf7e66c10c524a7cfbfae9224c8adf0a26af62ec0a |
| …/src/state.rs | 373 | c8841462eb3af0d359478afc78441f81d5728077c6a1486ad331133a903b3ee3 |
| …/src/controls.rs | 997 | 6fc9a9f79db5ba98bcf03a7b8becea9b142836930e8e2dd0849dd1a87a55cb26 |
| …/src/positions.rs | 379 | 00ecf3b477e6ef9a7e433df169fb57e62d2fc1a367be35393a7baa05fa154484 |
| …/src/allocation.rs | 475 | 1fb6c2e3e213d85198d1a5f80f5a7d4c47c2b806c4bf122a6b5d2efd43a53ba1 |
| …/Cargo.toml | 31 | 3311cf750a5c712263a0380cb917c694bb69ec59257f3e3582ea2eaa37e0bfd0 |
| …/Cargo.lock | — | 15b2592cfb56e28185ebc7ba0aed75d40bfac1e70af1b154078468b8523890b8 |
| solana/tests/financial-runtime/src/lib.rs | 78 | 1de4b90fed015bb092698cffe84f38e3126177bfe74e9bf39e8a9f0830b485ce |
| …/src/main.rs | 39 | 2487edec99d706bbfcc7653fb5e5aefc6c433e472af768120c28f93f3cab23f8 |
| …/src/runtime.rs | 733 | 1a955cd7920825d2f1e8f222b8e75b3dd5b42ed9df960eab02587908e8eccdec |
| …/src/financial.rs | 497 | d2f4cdff59a5971736c4c495d354a2fdac3cd4c7d867f899a38acc97baf36447 |
| …/Cargo.toml | 30 | a1085e5d53c45705bb7e723a6c05ec38448ef45662d55cb6493497b57e55cd3e |
| …/Cargo.lock | — | b13930fa4c96c6d794ee580002e6c4016d321e7df7083e7f5c4e93c409295d10 |
| …/scripts/build-sbf.sh | 79 | 37e683426b300bf45ffa6b1000928fa95248e5eb661affd99468fa78b0b03c83 |
| solana/Cargo.toml (modified) | 13 | e6fde7c4ba1cc1671423997fec389a3874204caab58227430d665ca5c875c040 |
| docs/solana/production-accounts.md | 23 | bfa6825af635204fb0540e175419ef4c1d473ff39adb2cf917302f6efd231f22 |
| /private/tmp/mc-positions-handlers-20261013.rs | 305 | cb51fa12720dc4b667efae853b7e5ebc8c2384415e72e03d80371898ed02c0ff |
| /private/tmp/mc-allocation-handlers-20261013.rs | 606 | e667b2fd7789ec7b1d6933196b37b4f9a3a57656a00c4d857b5b997e47c22a60 |
| /private/tmp/mc-production-accounts-contract.md | 886 | 6e9ec51770f9ebc327b184b324abca0a303ad428729b2319e3947fb018742e78 |
| /private/tmp/mc-production-payout-contract.md | 467 | df5d207fdb912722b086ff8b692af7b171afbbde8741ec75dde66306b34839d1 |
| /private/tmp/mc-production-allocation-contract.md | 751 | 470f61c76b71280adc4dd68416cf667c352c7549d846a54d865cc33a7906c7ca |

### 5.1 What the tranche contains

- `state.rs`: 11 `#[account]` types with Anchor default discriminators, `InitSpace`, bump-first layout and a layout-size test (`state.rs:309–322`). Seeds `pool, admission, game-vault, ops, ops-cash, ticket, user, user-round, position, backer, registry, snapshot`. `PAGE_CAPACITY=8`, quota 3/3,600 s, `BACKER_REMAINING_CUTOFF=10`. Widths match the accepted table except `Pool.funded_claims: u64` (accepted says u128; see spec DR-05). **Missing** versus the accepted contract: config-version pinning, retained per-close record, rent-attribution components, participant/candidate pages, provider binding. Closing time/deadline live in `Pool` (`closing`, `closed_at`, `deadline`).
- `lib.rs`: 17 entrypoints. Implemented: `initialize`, `set_pause`, `fund_operations`, `withdraw_operations`, `withdraw_management`, `provision_snapshot`, `provision_registry`. **Stubs returning `NotImplemented`**: `open_position`, `fund_series_a`, `claim_position` (`positions.rs:319, 323, 331`); `discover_row`, `senior`, `seal_weights`, `allocate_row`, `prune_row`, `commit`, `claim_backer` (`allocation.rs:357–381`).
- Pure helpers with golden tests: `fold_slices`, `cap_senior`, `floor_proportional`, U256 codec (`allocation.rs`); `allocation.rs:389–394` golden `T=10,000 → seed 5,000 / senior 1,750 / A 2,250 / B 1,000` (pre-fold slices).
- Toll split constants are **35/45/20 everywhere in production source** (`state.rs:30–33`, `allocation.rs:52–64`, `financial.rs:81–88`). The out-of-repo `mc-production-accounts-contract.md:69–78` **reverses senior and A (45/35)** and is stale. No 35/25/40 anywhere in the tranche.
- `orao-solana-vrf-cb` is **not** a dependency of the production program or harness (only of the probes).

### 5.2 Defects found by static reading (not by build)

| # | Location | Defect | Severity for reuse |
| --- | --- | --- | --- |
| 1 | `controls.rs:438`, test fixture `:711` | Squads `Multisig` discriminator last byte `0xc8`; `sha256("account:Multisig")[0..8]` ends `…0xec`. Real Squads multisigs would be rejected; unit tests pass only because fixtures use the same constant | **Blocker** for governance authentication |
| 2 | `controls.rs:446–447` | `config_authority` parsed as `Option<Pubkey>`; Squads v4 source stores a plain `Pubkey` with `Pubkey::default()` meaning autonomous (confirmed from public source 2026-10-13). An autonomous multisig would desynchronize the parse | **Blocker** |
| 3 | `financial-runtime/src/runtime.rs:293` | `signature.map(...)` on a plain `Signature`; compile error; fix `Some(signature.to_string())` | Blocks harness build |
| 4 | `runtime.rs:355–358` | Fixture `Ticket` default phase 0 (DISCOVER) vs genesis IDLE (6); `Pool.snapshot_pages=0` despite injected page; `close_reserve=0`; `next_sequence=1` vs seeded sequences 1..4 | Fixture |
| 5 | `runtime.rs:489, 500, 502` | Registry/snapshot keys use owner wallet pubkeys; drafts compare Backer PDAs | Fixture |
| 6 | `financial.rs:44` | asserts `next_sequence == 2` after `open_position`; neither draft nor reference increments it on game open | Test defect |
| 7 | `financial.rs:101–109`, `runtime.rs:480–488` | Discover fixtures preload counts/slices that `discover_row` accumulates itself | Fixture |
| 8 | `/private/tmp/mc-allocation-handlers-20261013.rs:464–515` | Same-page swap-delete returns `WrongAccount` when both slots share a page (stalls Prune) | Draft logic bug |
| 9 | same `:517–530` | Alias branch never writes the pruned Backer (`active=false, slot=0`) — inverted condition | Draft logic bug |
| 10 | same `:11–22` | `Pool` not imported; `use super::` path wrong for `allocation.rs` | Draft compile |
| 11 | `state.rs` + drafts | `User.b_entitlement` and `User.ever_b` never written | Gap (promotion not in tranche) |
| 12 | `controls.rs:246–249` | management shortfall returns `InsufficientOperations` | Naming |
| 13 | `positions.rs:20–21` / `allocation.rs:19–20` | duplicate `SERIES_A/B` consts glob-imported | Hygiene |
| 14 | positions draft `:291` | hard-coded `600` instead of `rounds::PROMOTION_DEADLINE_SECS` | Hygiene |
| 15 | positions draft `:126–129` | inline seed credit when registry empty / all slices zero (matches reference model shortcut; payout-contract wording differs) | Reconcile in spec (adopted: allowed, as reference) |
| 16 | harness RED tests `lib.rs:32–59` | `open_moves_real_sol`, `claim_moves_real_sol`, `allocation_golden_t10000` assert success against stubs — deliberate RED, never executed | Expected |

No eager `.ok_or(error!())` in loops was found in the tranche or drafts (the heap lesson is already applied). The alias-safe `PruneRow` shape exists but neither bug above is exercised by fixtures (all seeded rows have remaining ≥ 100,000; harness prune loop aliases moved row with pruned row).

### 5.3 Draft deferrals (tranche boundaries, not accepted scope changes)

`production-accounts.md:3,11,13,17,19`: candidate sealing, automatic B, ORAO integration, reopening, config activation, retirement/refunds, wallet/ICP integration, real Squads execution, owner rent fallback. `mc-production-payout-contract.md:433–462` leaves seven binding choices open (page capacity, inline threshold, provider PDA derivation, position reclamation, rent-payer mechanics, reserve values, event/IDL encoding) — each is resolved or scheduled in the decision register.

## 6. Current deployed-architecture source map (ICP Chain Fusion rail; unchanged)

Verified file:line facts the spec and plan rely on (coordinator spot-checked each line):

| Fact | Cite |
| --- | --- |
| Front-End Load 400 bps; `game.amount` = gross; pot credited net | `ponzi_math_sol/main.mo:2444–2449, 2783–2787, 2808, 2824` |
| Simple 11 %/day, 21-day cap, Float, no remainder carry | `main.mo:1063–1078` |
| Carried Interest 12 % <7 d, 7.5 % <14 d, 3 % | `main.mo:575–589` |
| Compound 12 %/15 d, 9 %/30 d via `Float.pow`; Jackpot 9 %/13 %; no Q18 in Motoko | `main.mo:1367–1387, 577–581` |
| Minima 10,000,000 / 50,000,000 lamports | `main.mo:411, 416, 2978–2980, 3059–3061` |
| `getMaxDepositLimit` query only, never enforced | `main.mo:1675–1677` |
| Quota checked at intent prepare, recorded at successful credit | `main.mo:2989–3002, 2837–2855` |
| Series A ×1.24, same-owner merge keeps `firstDepositDate`; `selfServeBackingEnabled=false` | `main.mo:2666–2688, 423, 3056–3058` |
| Series B ×1.16; preferred = losers without any A/B entry; fallback all losers; `raw_rand`; no operator filter | `main.mo:925–942, 944–962, 968–993`; `Promotion.mo:39–57` |
| Toll: half seed; 35/45/20; folds; overshoot → seed; stale "35/25/40" comment at `:592` | `main.mo:656–659, 707–716, 734, 772` |
| Strict insolvency `earnings > pot`; empty pot resets without payout; equality leaves pot 0 | `main.mo:1258–1268` |
| 5,000-lamport deduction and silent tiny-payout success | `main.mo:1291–1317` |
| Canister-derived fallback destination | `main.mo:1217–1221` |
| Threshold Ed25519 `sign_with_schnorr`, `key_1`; sol-rpc canister `tghme-zyaaa-aaaar-qarca-cai`; default provider set, no explicit consensus | `SolSigner.mo:41–58`; `SolRpc.mo:21, 66–105` |
| Deposit detection 60 s timer, 2 h intent TTL, `maxSupportedTransactionVersion=?0` | `main.mo:444, 436, 2570–2578` |
| Shenanigans polls `getAllGames`/`getBackerPositions`; `mintInternal` stamps fresh `created_at_time`; `mintedEventIds` written after await; `solBackerSeen` keyed by owner only; no SOL round namespace | `shenanigans/main.mo:1431–1442, 2015–2030, 2917–2923, 1612, 1276–1292` |
| Frontend stale 35/25/40 constants; net-based projection; first-installed wallet selection; no `wss://` in CSP | `frontend/src/lib/gameConstants.ts:42–44`; `SolInvestPanel.tsx:70–76`; `hooks/useWallet.tsx:388–419`; `frontend/public/.ic-assets.json:19` |
| Frontend versions (lock) | web3.js 1.98.4; wallet-adapter-react 0.15.39; React 18.3.1; Vite 7.3.1; TS 5.6.3; vitest 4.1.7 |
| Mainnet ids | `ponzi_math_sol spc6q-xyaaa-aaaac-qg2ma-cai`, `shenanigans j56tm-oaaaa-aaaac-qf34q-cai`, `pp_ledger 5xv2o-iiaaa-aaaac-qeclq-cai`, `siws_provider tcm26-yqaaa-aaaac-qg2lq-cai`, `frontend 5qu42-fqaaa-aaaac-qecla-cai` |

Correction to the handoff text: `createBackdatedGame` exists in `ponzi_math` (ICP), not in `ponzi_math_sol`; the SOL canister's TEST_ADMIN hatches are `adminMergeBackerPosition`, `adminClearAllBackerPositions`, `adminSetBackerLifetimeRepaid`, `adminSweepUntracked`, `adminForceReset` and four `adminTest*` (`main.mo:2044–2225, 3423–3464`). Legacy capabilities in source are not proof of deployed behavior; no live canister was queried.

## 7. Knowledge graph coverage

| Check | Result |
| --- | --- |
| Worktree `graphify-out/graph.json` | 7,149 nodes, 9,594 links, 21 hyperedges, `built_at_commit 18b7811…`; SHA-256 `513eb4d596ae1c4cb21f8a7079da0176a4607d4b87a6ee319e10b7d95f400d05`; **identical** to `/private/tmp/mc-production-graph-base-20261013/graph.json` |
| `manifest.json` | 372 entries, per-file `mtime`/`ast_hash`/`semantic_hash`; SHA-256 `08a8a889…b043`; identical to backup |
| Nodes under `solana/programs/musical_chairs/` or `solana/tests/financial-runtime/` | **0** (uncovered production source) |
| Nodes for `docs/solana/production-accounts.md` | 0 at start of the planning pass; covered by the planning fragment since |
| Codex review artifact | `docs/coordination/native-solana-codex-plan-review-20261013.md`, SHA-256 `d9b59efdeed52842426bc4e4d3213bdda91d888da5266b94481bb8f20bd6b15a`, 126 lines, reviewed commit `db48ce33a40e10d0a1153d738926eb051bb3b033`; preserved unchanged |
| Codex round-2 review artifact | `docs/coordination/native-solana-codex-plan-review-round2-20261013.md`, SHA-256 `01a0c2d31a41a650eda0dba9979c15eae2706551a19a5bdba608d2d1da2b9138`, 62 lines, reviewed commit `1313b52e5f3f3b2c103d31a9ac097ebd8b930f2c`; preserved unchanged; appended to the graph by the 2026-10-13c revision receipt |
| Required queries run (allocation phases; claim-triggered close/expiry; ICP boundary; costs) | all return relevant nodes from host models and October contracts; ICP-boundary and cost queries surface mostly May 2026 Chain Fusion nodes, which are legacy-rail context, not native design |

Graph maintenance performed by this pass is recorded in `docs/solana/evidence/2026-10-13-planning-graph-refresh.json`; Codex's review append is `2026-10-13-codex-plan-review-graph-{fragment,refresh}.json` (7,921 → 7,928 nodes, 11,080 → 11,089 edges, 27 → 28 hyperedges); the round-2 revision pass is `2026-10-13-planning-revision-graph-{fragment,refresh}.json` (7,928 → 7,980 / 11,089 → 11,210 / 28 → 30); the round-3 pass is `2026-10-13-planning-revision2-graph-{fragment,refresh}.json` (see review packet).

## 8. Public snapshot deltas observed 2026-10-13 (public sources only; no private content sent)

| Claim in record | Observed today | Effect |
| --- | --- | --- |
| ORAO `orao-solana-vrf-cb` 0.4.0 latest, Anchor 0.32.1 | Confirmed; program `VRFCBePmGTpZ234BhbzNNzmyg39Rgdd6VgdfhHwKypU` on devnet and mainnet; callback deadline in slots; fulfilled requests appear retained (no close instruction documented; inference) | Spec §7.4 keeps direct capture; retention to be proved on devnet |
| Anchor 0.32.1 pinned | 0.32.1 published and not yanked; **0.32.2** (2026-09-14) is a patch on Solana 2.x; 1.2.1 is latest stable on Solana 3.x; 2.0.0-rc.2 prerelease | Stay on 0.32.1 (DR-26); 0.32.2 is an optional patch if a probe needs it |
| LiteSVM 0.7.1 | still published (Agave 2.3-era); latest 0.18.0 is Agave 4.3 | Keep 0.7.1 for the pinned tuple |
| Squads v4 claims | all five confirmed in source: `time_lock: u32` seconds; config transactions time-locked; `config_authority == Pubkey::default()` = autonomous; spending limits bypass; program `SQDS4ep65T869zMMBKyuUq6aD6EgTu8psMjkvj52pCf`. Caveat: controlled multisigs have a direct, un-timelocked config path (not applicable to autonomous) | Confirms accepted D8; fixes draft defects #1–#2 |
| Solana limits | 1,232 B legacy/v0 confirmed; **v1 transactions (4,096 B) live on mainnet** since Agave 4.2; 200 ms slots live; 96-account v1 limit not yet active (Agave 4.5 planned); ComputeBudget instructions do not configure v1 | GUI stays v0 (accepted); keeper may later use v1 (DR-28, measure) |
| Rent 5,080 lamports/byte quote | **SIMD-0437 rent reduction in progress**: 6,333 live 2026-09-03; 5,080 live on testnet, mainnet mid-Sept (secondary source: epoch 1033); 2,575 / 1,322 / 696 expected with Agave 4.4 (Nov 2026) | Budget rebuild must use the step schedule (DR-27); do not hard-code a coefficient |
| ICP SOL RPC canister | docs guide prints an invalid principal; `dfinity/sol-rpc-canister` README gives `tghme-zyaaa-aaaar-qarca-cai` (valid, matches repo `SolRpc.mo:21`), module `1733b1e5…3115`, controllers NNS Root + self | Confirms repo binding; verify Candid before new bindings |
| Threshold Ed25519 price | `key_1` 26,153,846,153 cycles; `test_key_1` 10B; XDR conversion dated 2026-05-22 | Confirms budget input |
| solana-dev skill 2.6.0 @ `491a19a4` | HEAD of main unchanged (2026-10-08) | Skill current; its Kit/v1 defaults remain non-binding |

## 9. Remaining proof gaps (what no one has shown yet)

1. No production financial instruction has executed in SBF; the 10 stubs return `NotImplemented`; the harness does not compile.
2. No RED→GREEN proof exists for any production handler; the RED tests exist but were never run.
3. Governance authentication code has two blockers (§5.2 #1–#2) and `initialize` has never run under LiteSVM.
4. Inline allocation feasibility at N≈100, production CU/packet/rent per instruction, and keeper transaction counts are unmeasured; the kernel figures are not production figures.
5. ORAO devnet latency, mainnet quorum behavior and fulfilled-request retention are unproved; local lifecycle used one synthetic fulfill authority.
6. Candidate completeness, promotion apply, expiry and finalization have no production code.
7. Resource reservation, locked partitions, refunds and cleanup have no production code.
8. Frontend adapter, IDL, ICP bridge canister and keepers do not exist.
9. Installed platform-tools identity not yet verified against the v1.48 release stamps (the v1.52 banner is only the builder default; see §4).
10. Rent economics are changing; every budget number is a dated scenario.
