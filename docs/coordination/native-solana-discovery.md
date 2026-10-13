# Native Solana discovery evidence and team handoff

Date: 2026-10-12. Coordinator: `/root`. Checkout: `/Users/robertripley/coding/musicalchairs`. Source was inspected initially on `main` at `f96b33ff95020458be754bfa0d7471b72a7403af`; documentation branch is `codex/native-solana-discovery-20261012` with those source files unchanged.

Scope: source-based discovery and draft specification/roadmap. No product edits, dependency installs, builds, tests, deployments, chain transactions or financial actions. Existing untracked agent configuration was preserved. No production-state or served-byte proof was collected.

Rob's subsequent scope answers, incorporated 2026-10-12: fresh native SOL launch; legacy positions remain withdrawable separately; SOL financial game first while PP and Shenanigans remain on ICP. The revised roadmap removes balance imports and native PP migration, preserves legacy access and existing desk isolation, and adds finalized native-event PP rewards without replacing legacy observers. These answers settle D1/D2 only; they do not approve production implementation or operational changes.

Second authorized cheaper-agent pass: [economic and lifecycle recommendation](native-solana-economics.md) records concrete formulas, source/display differences, integer vectors, complete bounded Backer tickets, active registry/lazy credits and one authenticated durable draw per closing round. Independent native review accepted the direction; its width/conservation and request-initiation findings were corrected and rechecked. Recommendations remain unapproved and runtime/provider cost/liveness unproved; no product implementation started.

Third authorized cheaper-agent pass: [processing and randomness recovery](native-solana-availability.md) narrows pauses, reserves finite work units independently of chunk size, permits replacement/self-funded progress and proposes one close+600-second promotion deadline. It supersedes the prior indefinite oracle wait under Rob's explicit requirement. Normal deposits remain available during nonclosing allocation; closing financial work remains protected. Timely results have bounded deterministic mapping, while missing/late results permit public no-promotion expiry and full seed carry only after cash commit. The design discloses withholding/omission risk and does not promise wall-clock reopening during chain halt, absent transactions or a cash-processing defect. No runtime/provider availability claim is made.

Native `gpt-5.6-luna` workers SOL-09–SOL-11 and independent SOL-12 reviewer were explicitly selected for this pass. The coordinator checked source skip-on-`raw_rand`-failure behavior, cross-document permissions and the public ORAO callback lifecycle. [Skills assessment](solana-skill-assessment.md) recommends refreshing the older existing Foundation skill; Mindrally's guidance is already installed locally. Public source and local entrypoint inspection only; no skill update/install or external private-content review occurred. Proposed persona lessons remain handoff recommendations, with no memory/catalog edits.

Fourth authorized pass: Foundation skill **refreshed to v2.6.0** at pinned upstream commit, existing symlink/old-tree rollback and all 33 files verified in [receipt](solana-skill-assessment.md). Four read-only native Luna workers recommended remaining defaults; coordinator [decision register](native-solana-defaults.md) separates policy recommendations, implementation proofs and launch-only inputs. Recommended public A flag differs from legacy off; operator eligibility stays source-equivalent; sponsor reserve/zero keeper reward/B rent distinctions are explicit; governance is 2-of-3/48h with separate immediate admission guardian. Exact published ORAO Callback manifest was checked instead of its generic tutorial; authenticated two-RPC ICP rewards remain a disclosed trust boundary with per-leg ledger recovery. No product code/dependency/build/test/deploy/transaction or external private-content call occurred. Independent document review is recorded on the board.

Fifth authorized investigation: [ICP placement and pilot budget](native-solana-icp-budget.md) answers keeper/component hosting and rough operating SOL before implementation. Rob selected ~100 active Backers/1,000 payouts/10 closes monthly; participant/row/binary/layout assumptions remain separate. Native Luna workers researched ICP execution/signing, exact provider/governance costs and resource counts. Coordinator independently validated [public price receipt](evidence/2026-10-12-native-solana-costs.json), corrected legacy rent coefficient and included both candidate lists, payer separation, temporary deployment liquidity and retained-provider failure costs. ICP keepers are viable; modeled signing bill favors hybrid external primary/ICP backup until actual batching is measured. No product code/build/deployment/sign/send/funding or current-user-canister query occurred; raw public price/account reads are not a Musical Chairs transaction receipt.

Skills: local-persona-catalog, team-agent-orchestration, codex-kanban, brainstorming, solana-dev, writing-plans. The user explicitly requested cheaper subagents. Three narrow native workers ran with requested model `gpt-5.6-luna` (spawn dispatch confirmed). Sonnet 5 availability was confirmed by a contents-free check, but automatic approval review rejected the subsequent source/document review because private repository disclosure to external Claude lacked provider-specific authorization. No external review ran. A fourth native Luna worker reviews the coordinator-authored documents as the safe fallback. Model selection does not prove financial readiness.

All workers queried the existing graph first, applied the relevant loaded persona, inspected source read-only and returned handoffs. The coordinator checked core source references and distinguishes historical proposals from current implementation.

## SOL-01 — economy inventory

Owner `/root/economy_inventory`; persona `musicalchairs-ponzi-economy-auditor`.

| Observation | Evidence |
| --- | --- |
| Lazy capped accrual; three plan rates | `ponzi_math_sol/main.mo:1062–1108` |
| Float truncation to eight decimals even for SOL | `ponzi_math_sol/main.mo:532–535` |
| 4% integer entry fee; position records gross, pot records net | `ponzi_math_sol/main.mo:2782–2834` |
| Toll tier rules | `ponzi_math_sol/main.mo:569–589` |
| Backer half implements 35/45/20 and caps/folds, senior first | `ponzi_math_sol/main.mo:649–785` |
| Frontend still describes 35/25/40 | `frontend/src/lib/gameConstants.ts:39–44` |
| Backer credit advances claimable and lifetime; cap is based on lifetime credit | `ponzi_math_sol/main.mo:595–639` |
| Partial payout when earnings exceed pot | `ponzi_math_sol/main.mo:1257–1334` |
| Reset scans all history, retains IDs and history, carries seed | `ponzi_math_sol/main.mo:1018–1055` |
| Current-round loser aggregation includes closed winning positions | `ponzi_math_sol/Promotion.mo:25–57`; `ponzi_math/test/promotion.test.mo:24–62` |
| Deposit/backer minima | `ponzi_math_sol/main.mo:410–416, 2973–2980, 3047–3080` |
| ICP conservation script names pot, seed, repayment and management partitions | `ponzi_math/scripts/verify-fee-invariant.sh:4–10` |

Source concerns to treat as migration decisions, not proof of a live incident:

- `sweepToPool` reserves 10,000 lamports (`main.mo:2477–2487`), while the deposit credit partitions the full inbound amount (`main.mo:2782–2825`). The native design needs explicit physical conservation and separate fee/rent sinks. Do not carry this accounting shape forward blindly.
- Legacy `coverChargeBalance` and current `coverChargeAccrualLamports` coexist; avoid inheriting stale similarly named admin surfaces (`main.mo:228–255, 1784–1786`).
- Small payouts can skip transfer while accounting still advances (`main.mo:1291–1317, 1427–1453`). The native dust rule must be explicit.
- The current executable split is stronger evidence than stale nearby 35/25/40 comments or frontend constants.

## SOL-02 — integration and authority

Owner `/root/native_solana_gap`; persona `musicalchairs-solana-siws-integrator`.

| Observation | Evidence |
| --- | --- |
| SOL backend is Motoko, SIWS provider is ICP custom canister | `dfx.json:11–13, 23–27` |
| No `Cargo.toml`, `Anchor.toml`, native `.rs` artifacts found | Worker file inventory; coordinator repository inventory |
| ICP threshold-derived pool/per-principal addresses | `ponzi_math_sol/main.mo:788–825, 2895–2910` |
| ICP signs and submits pool payouts | `ponzi_math_sol/main.mo:1114–1188` |
| Prepare/observe/credit/sweep, rather than atomic program deposit | `ponzi_math_sol/main.mo:2973–3045, 2543–2874` |
| Deposit/backer/desk matching shares ±5% windows and cross-kind guards | `ponzi_math_sol/main.mo:2591–2767, 2923–2969` |
| EEC uses ICP raw_rand and prefers players without Backer entries | `ponzi_math_sol/main.mo:903–1006` |
| PP inventory/proceeds are distinct from game pot | `ponzi_math_sol/main.mo:322–407, 3283–3547` |

Recommended boundaries: native program controls financial state and SOL; ICP mirrors finalized history only; native instructions replace transfer-intent matching. Backer indexes and round candidate selection remain unproved design candidates. A caller-supplied subset of Backers/players cannot establish complete allocation/selection.

Historical context: `docs/superpowers/specs/2026-05-23-solana-port-design.md:1–4` is a draft thought experiment; `2026-05-25-solana-chain-fusion-design.md:1–11` supersedes it. Its old omissions and hypothetical premortem must not be silently revived.

## SOL-03 — frontend and shared social boundaries

Owner `/root/frontend_preservation`; persona `musicalchairs-frontend-release-verifier`.

| Observation | Evidence |
| --- | --- |
| React/Vite, wallet-adapter, web3.js, SIWS already present | `package.json` |
| Frontend asset canister hosts frontend/dist | `dfx.json:29–33` |
| Reusable app/tab/dashboard shells | `frontend/src/App.tsx:292–390, 409–713`; `components/Dashboard.tsx:116–220` |
| Current SOL panels use intent and observer workflow | `components/SolBackerPanel.tsx:35–190, 260–350`; `components/SolInvestPanel.tsx`; `hooks/useQueries.ts:2451–2695` |
| Sender returns submission only; CSP blocks its websocket-confirmation path | `frontend/src/solana/sendSolDeposit.ts:92–104`; `frontend/public/.ic-assets.json:19` |
| SIWS session derives ICP identity; different from Solana key | `frontend/src/lib/siwsSigner.ts:143–237`; `hooks/useWallet.tsx:1–35, 135–147` |
| PP balances/transfers/chips live on ICP | `frontend/src/hooks/useQueries.ts:1333–1358, 1555–1742` |
| Shared reward observer currently polls financial canisters | `shenanigans/main.mo:1400–1457` |
| Desk/admin PP routes remain ICP/cross-chain | `frontend/src/components/PpDeskPanel.tsx:23–105`; `hooks/useDeskAdmin.ts:1–138` |

Coordinator adjustment to worker's legacy-flow suggestions: on the **native** rail, chain finality replaces canister credit as financial truth. Native panels must not retain a plain-transfer manual fallback. Optional mirroring follows finality and does not authorize balances or block payout. Legacy observer/recovery UI may remain on the legacy rail.

## Proposed durable persona lessons

No catalog files were edited. Consider these updates after review:

1. Reconcile physical SOL and logical partitions with network fees, rent and donations explicitly; theoretical entitlement is not liquid reserve.
2. Treat SIWS principal, wallet pubkey, submitted signature, finality and mirror credit as separate identities/proof states.
3. Native Solana migration needs a complete bounded recipient/candidate representation; generic reward indexes are not evidence of parity with senior-first, capped dynamic entitlement allocation.
4. Preserve frontend presentation while replacing financial authority; reject direct-transfer observer fallbacks on native program rails.

## Verification receipt

Performed: `git status --short`, `git rev-parse HEAD`, file inventories, scoped `graphify query`, `rg`, numbered source reads and official Solana/Anchor/ICP documentation review. Checked numeric fee example against source percentages. Documentation review results are recorded on the board.

Independent native Luna review accepted the artifacts as discovery documents with corrections, explicitly not as an executable/launch plan. Coordinator applied substantive corrections: alternate payout/recovery classification, randomness failure policy, per-account rent payer/refund/progress contract and replay-safe SIWS reward binding. Provenance now distinguishes initial main source from the documentation branch. External-provider recommendation is explicitly conditional on disclosure authorization. One review observation referred to the pre-update Sonnet status; the final metadata clearly says no external review completed.

Not performed: executable economic model, current-canister queries, live RPC balance audit, frontend build/browser proof, local validator, devnet transaction, independent implementation audit. The documents do not establish migration funding or launch readiness.
