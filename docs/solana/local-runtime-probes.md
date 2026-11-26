# Local Solana runtime probes

Date: 2026-10-13. Baseline: `830d57377e71aa4f15ad0c7fa86ff2d6b3ff4e0c`. Branch: `codex/native-solana-runtime-probes`. Rob authorized local compute/account/provider execution and graph maintenance. No deployment, wallet access, live signing/broadcast, governance or funds action is authorized.

## What was executed

`solana/probes/runtime` builds a six-instruction Anchor probe into actual SBF and loads it in LiteSVM alongside the captured real ORAO Callback executable. The unchanged SDK vendor supplies ABI/CPI types; its placeholder handlers are never used as the provider. The numerical probe directly imports the accepted `earnings.rs` and `fees.rs` source. The row kernel exercises PDA validation, fixed-size serialization, checked U256 weighting/capping and transaction rollback. It deliberately does not implement the full Backer allocation, cash vault, promotion mapping or rollover.

The matrix contains 88 local transactions: 45 asserted successes and 43 asserted failures. Each case checks its intended result and state effect. Signer, authority, writable flag, owner, discriminator, canonical PDA/bump, version, row alias/index/size, captured-N bound, stale cursor and arithmetic rollback cases run against compiled SBF. Expected failures preserve account data, owners, executable flags and non-payer balances; the payer loses the calculated local signature fee, or zero on a pre-execution failure with zero CU. The fixture uses separate synthetic payer, app authority, provider treasury and oracle authority roles. Local signing uses in-memory test keys only.

The [runtime observations](evidence/2026-10-13-runtime-probes.json) preserve logs, actual CU, packet bytes, message/writable account counts, pre-transaction declared-account bytes, byte differences, account balance/hash deltas and return data. Byte differences are not physical writes. LiteSVM does not expose runtime-loaded account count here; that field is null with a reason. Message keys and declared bytes must not be relabeled as actual runtime load measurements.

## Pinned execution profile

Host Rust/Cargo 1.92.0; Anchor 0.32.1; Solana SDK 2.3.1; LiteSVM 0.7.1 with locked Agave 2.3.13 and its `all_enabled` feature profile. System interface 1.0.0 and loader-v3 interface 3.0.0 match the SDK 2 types. LiteSVM 0.6.0 had runtime API/type failures, and 0.6.1 could not resolve compatible hashes with SDK 2.3.1. The failed candidates do not justify changing the accepted SDK/frontend stack.

SBF builder 3.1.0-beta.0, platform-tools v1.48 with forked Rust 1.84.1-dev/Cargo 1.84.0, SBPFv0. The standalone program lock keeps dependencies compatible with that older compiler; the host harness has a separate lock. The release profile requests overflow checks/fat LTO/one codegen unit; the compiler warns that dual `cdylib`/`lib` crate types disable LTO, so no actual LTO claim is made. Tools were installed only in an owned temporary directory, without a global CLI/Anchor install or rustup override.

The [build receipt](../../solana/probes/runtime/fixtures/build-provenance.json) pins the exact source/lock/artifact/tool hashes. The harness verifies the SBF artifact and source hash before executing. ORAO's 616,464-byte binary has SHA256 `6d180b1e91be7d5c541f75e2413146beb047d9beeb76ac8eb1d7c883200c6b8b`; its [provenance](../../solana/probes/runtime/fixtures/orao-provenance.json) preserves separate finalized program and ProgramData read slots, loader ownership, linkage, deployment slot and SDK/public-API pins. These separate reads are not an atomic chain snapshot or a continuing provider guarantee.

## Measured compute and packet envelope

All CU values include the compute-budget instruction. No priority fee is requested. The 1,400,000-CU test ceiling isolates measured usage; it is not a recommended production request budget.

| Executed path | Actual CU | Legacy packet bytes | Message keys / writable |
| --- | ---: | ---: | ---: |
| Arithmetic: 1 repetition | 9,620 | 252 | 4 / 2 |
| Arithmetic: 4 repetitions | 26,779 | 252 | 4 / 2 |
| Arithmetic: 8 repetitions | 49,659 | 252 | 4 / 2 |
| Arithmetic: 16 repetitions | 95,418 | 252 | 4 / 2 |
| Arithmetic: 32 repetitions | 186,924 | 252 | 4 / 2 |
| Row kernel: 1 row | 16,432 | 287 | 5 / 3 |
| Row kernel: 4 rows | 28,621 | 386 | 8 / 6 |
| Row kernel: 8 rows | 43,377 | 518 | 12 / 10 |
| Row kernel: 16 rows | 89,617 | 782 | 20 / 18 |
| Row kernel: 32 rows | 192,009 | 1,310 | 36 / 34 |
| ORAO registration, no callback | 18,819 | 442 | 9 / 3 |
| App request through ORAO CPI | 35,865 | 451 | 10 / 6 |
| ORAO fulfillment with Ed25519 precompile | 19,747 | 628 | 10 / 4 |
| Timely app capture | 9,428 | 284 | 5 / 2 |
| Promotion expiry | 3,937 | 251 | 4 / 2 |

The row matrix executes the same chunks at captured N=100 and N=256, touching only supplied rows. Equal CU across these N values demonstrates the representative kernel's bounded access, not the complete game's population independence. The 32-row packet exceeds the accepted legacy/v0 1,232-byte envelope even though LiteSVM executes it. The report explicitly marks it ineligible for that packet envelope. Sixteen rows fit only this stripped-down kernel; keep the eight-row production candidate provisional until real allocation pages, vaults, snapshots and fixed accounts are measured. No population cap or final inline/chunk threshold is approved by these probes.

## Provider lifecycle and recovery

Actual captured ORAO SBF executes Initialize → Configure with a local synthetic fulfill authority → Register against loader-shaped client program/ProgramData fixtures → Fund Client → Request CPI → Ed25519 Fulfill → direct app Capture. The loader fixtures contain the actual probe ELF and are injected in memory; they are not local-validator or live deployments. Requests use the same client/seed PDA on retries. The configured oracle signature covers client public key plus seed; the genuine Ed25519 precompile remains enabled. Full 64-byte results are compared without truncation.

Underfunded client requests, unauthorized fulfill authorities, existing seed/PDA collisions and missing app-PDA CPI signing fail atomically. The request path also rejects substituted client/network/treasury/request/provider accounts, malformed empty idempotent requests and first requests after the deadline before provider fees/rent or CPI. The local network uses one synthetic fulfill authority; mainnet quorum/multiple responses are not measured. Capture rejects pending, wrong owner/address/client/seed/bump and malformed accounts. The capture boundary is inclusive at 600 seconds; expiry requires a later Clock time. Duplicate capture stays idempotent after the deadline. With no provider executable and no request ever created, expiry still preserves pending cash. A second genuine provider lifecycle expires before fulfillment, then fulfills and captures after expiry: the capture is a successful no-op. Even a backdated test Clock cannot revive the skipped state. Retrying the already-created request after expiry does not draw again.

Local default Rent uses `6960*(data_bytes+128)`, distinct from the earlier dated public quote. One pending request receives 6,173,520 lamports of storage balance; the Client loses exactly that plus the configured 300,000-lamport provider fee. The separate treasury receives the fee, and the payer spends 5,000 lamports for its transaction signature. Actual fulfillment shrinks 759 bytes to 147 and refunds 4,259,520 lamports to the Client. Its payer pays 10,000 lamports, including the Ed25519 verification signature. Refund behavior and payer roles are now locally executed; live prices, service latency and authority availability remain unproved.

No-callback/direct capture is the accepted candidate. Configured callback delivery/failure/alternate callback, provider upgrades/termination, real oracle service availability, actual production accounts and cluster feature/rent parity are outside this executed matrix. These are explicit limits, not hidden launch-readiness claims.

## Runtime defect and regression

The first actual arithmetic run failed at 16 repetitions after 151,522 CU: eager `.ok_or(error!(...))` constructed heap-allocated Anchor errors on successful additions. Solana's default 32 KiB heap uses a bump allocator, so dropping them did not recover allocation space. Changing only that error branch to `ok_or_else` preserves arithmetic and permits all 32 repetitions at the default heap. The [before-fix receipt](evidence/2026-10-13-runtime-heap-before.json) and build provenance preserve the old artifact/source hashes and failure. Independent integer golden vectors also caught a fixture calculation that omitted the accepted Simple 21-day accrual cap; the economic source was unchanged.

## Reproduce and next work

Set `MC_SBF_BUILDER`, `MC_SBF_SDK`, `MC_PLATFORM_TOOLS` to the pinned locally obtained tools, optionally `MC_SBF_TARGET_DIR` to an owned cache, then run `solana/probes/runtime/scripts/build-sbf.sh`. It checks versions, builds locked SBF and removes only its exact generated keypair without reading it; it refuses to overwrite a pre-existing keypair path. Refresh the build receipt after intentional source/tool changes. Then, from `solana/probes/runtime`:

```sh
python3 scripts/verify-fixtures.py
cargo run --locked --bin musical-chairs-runtime-probes -- all
cargo test --locked --workspace
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
```

Optional runner selectors are `permissions`, `compute`, `provider`; `MC_OBSERVATION_LOG` appends local JSONL diagnostics. Generated targets/artifacts are ignored. The checked JSON is an evidence snapshot, not a production configuration.

Next authorized implementation work should bind the host oracle to production-owned accounts and complete bounded financial instructions, then measure those full paths. The current probes supply a compatible local toolchain, tested account/CPI primitives and a concrete provider/recovery route. They do not provide a production custody program, final ABI/IDL, whole-game compute cost, launch funding, network finality, keeper service, frontend integration or deployment authorization. The provisional operating reserve and eight-row budget scenario remain provisional. ICP hosting, GUI, legacy withdrawals and PP/Shenanigans remain unchanged.
