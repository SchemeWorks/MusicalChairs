# Accepted native SOL contract and current execution scope

Local date: 2026-10-12. Acceptance: Rob said "I'm happy with those defaults" and requested the Rust economic model and provider/resource probes using cheaper agents, with graph maintenance and Solana explanations. He explicitly said "No deployment yet."

## Accepted policy

D1 fresh native pool/new deposits, no imports or access to legacy reserves; separate legacy withdrawals. D2 existing ICP GUI/PP/Shenanigans/social services and separate existing PP desk remain. D3 native SOL only, zero initial game pot/seed apart from non-game operating collateral. D4 source three plans/gross accrual/35-45-20 Backer half, same operator eligibility, public A on for fresh launch. D5 integer lamports, floor Q18 maturity factors, Simple remainder carry, checked widths, source Backer-open cutoff, wallet network fees without hidden payout deduction, tightened atomic quota. D6 complete inline allocation where measured feasible, otherwise public bounded continuation; sponsor shared/provider/automatic-B storage; no on-chain keeper reward and default-zero owner contribution. D7 one claim-triggered close and one bound draw, immutable 600-second promotion eligibility deadline, no reroll, public no-promotion expiry preserving cash liabilities. D8 autonomous 2-of-3 governance/48-hour delay, separate immediate admission guardian, future-only schedules/providers and disclosed quorum upgrade power. D9 native financial authority with trusted authenticated ICP history/PP outbox; provisional hybrid routine worker/ICP backup pending measured resource/cycle costs.

The [defaults](../coordination/native-solana-defaults.md), [economic contract](../coordination/native-solana-economics.md), [recovery contract](../coordination/native-solana-availability.md) and [spec](../superpowers/specs/2026-10-12-native-solana-design.md) define the details. No capacity cap, cooldown, fee-rate change, PP migration, operator exclusion, alternative draw/provider or manual financial override is introduced by this pass.

## Authorized implementation subset

Implement and test the host-side Rust economic model, allocation/round state models and local provider/schema/transport resource probes. Ordinary development dependency fetches, locked compilation and local synthetic test fixtures are included. These artifacts are not a deployed program, native account authentication proof or oracle-service liveness receipt. This pass does not implement the production custody/instruction ABI, replace the GUI or create operating services.

No mainnet/devnet deployment, live transaction signing/broadcast, wallet/key-file access, funds, oracle registration/funding, governance mutation or legacy canister changes. Local unsigned serialized messages and in-process synthetic fixtures are test data. Later SBF/runtime and separately authorized devnet results must be labeled distinctly.

## Later inputs and measurements

Actual independent governance/guardian voters and vaults, management/sponsor identities, program/cluster/provider IDs, production endpoints/origins, operational hosting, funded reserves and deployment approval remain launch inputs. Packet/account bounds, actual CU, SBF binary size, inline feasibility, latency, provider lifecycle and final operating budget remain measurements. Early probes may rule out a guessed layout without rewriting economic rules.
