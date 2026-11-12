# Native SOL economic model and provider/resource probes

Date: 2026-10-12. Branch: `codex/native-solana-model-probes`; baseline `38cbf153a5ff46487fdbe80314f93acfa7328e63`. Rob accepted D1–D9 and authorized these local models/probes, cheaper agents and graph maintenance; no deployment or live financial action.

## Delivered scope

The Rust reference workspace contains pinned earnings/fees/admission, phased Backer allocations and claim-triggered rounds. The joined harness connects actual round deposits and payouts to the Backer book, maintaining one physical-vault identity through toll escrow and allocation commit. [Accepted policy](../solana/launch-decisions.md), [economic parity](../solana/parity-matrix.md), [economics](../solana/economics.md), [Backers](../solana/backer-allocation.md), [rounds](../solana/round-state-machine.md).

The isolated [provider/resource probe](../solana/provider-resource-probes.md) locks Anchor 0.32.1, ORAO Callback 0.4.0 CPI-only and Solana SDK 2.3.1. It checks SDK types, generated CPI account metas, canonical client/request PDAs, full 64-byte output, synthetic account decoding and unsigned legacy/v0 packet serialization. Its 57-file public SDK vendor is unchanged. The CLI snapshot is reproducible; SDK-default rent and the saved dated public rent quote are distinguished.

Native `gpt-5.6-luna` workers handled math, Backers, rounds, probes, integration, semantic graph extraction and bounded reviews. A focused `gpt-5.6-terra` round pass addressed repeated lifecycle integration failures. The coordinator owns contracts, integration acceptance, graph merge and source verification; no private repository content was sent to external providers.

## Verification and corrections

Coordinator checks passed: 72 economic tests, 5 provider tests, both formatting checks, both locked all-target Clippy checks with warnings denied, and probe output equality with the checked JSON. The economic tests include width/time boundaries, missing tiers and caps, chunk equivalence, replay/error rollback, old funded claims during allocation, joined cash conservation, position-versus-round closure, early deadline recovery and bounded entropy exhaustion. Focused independent reviews accepted the corrected math/Backer/provider scopes. After two whole-pass review rounds found admission, continuation, barrier and promotion-history defects, a third fresh pair independently returned PASS on the corrected host artifacts. [Review receipt](../solana/evidence/2026-10-12-model-review.json).

Review corrected supplied payout/maturity inputs, repeated Simple claims, fixture backing/lifetime caps, solvent position closure incorrectly closing rounds, allocation overflow mutation, candidate-counting expiry, duplicate capture after promotion, the composed rollover timestamp/rollback, shared game/A admission, closing payout barriers and oversized continuation arithmetic and persistent B history after automatic promotion. A compound-30 net example arithmetic typo was corrected without changing rates. The unsupported provider creation fee was removed from probe data.

## Evidence limits and next work

These are host reference models and local SDK/packing fixtures. Rust maps, test owner IDs, cloned state for rollback and host request hashing are not native account layouts, runtime authentication or a constant-size on-chain commit proof. The provider probe supplies the actual SDK PDA/schema boundary independently. The Round and Backer models remain separate outside the explicitly joined cash paths; automatic B entitlements and raw capture results are modeled in Round and must be connected to authenticated Backer/provider accounts later. No custody instructions, production IDL or frontend adapter have been built.

`anchor`, `solana`, `cargo-build-sbf` and `surfpool` were absent. SBF build/runtime compatibility, actual compute units and account constraints, full inline feasibility, callback/direct-capture lifecycle and latency remain unmeasured. The representative 256-row fixture fits 8 legacy rows at 1,229 bytes; 16/32 exceed 1,232. Synthetic ALT packets shrink bytes but still load accounts. These fixtures do not establish a page size, population cap or final operating budget.

The next roadmap step is to pin and exercise the local SBF/runtime harness, then unify the reference models into program-owned accounts and bounded instructions using this reference oracle. Governance identities, provider/cluster IDs, endpoints and funded reserves remain launch inputs; no deployment is implied. ICP hosting, the existing GUI, legacy positions and PP/Shenanigans were untouched.

## Graph maintenance

[Graph workflow](native-solana-knowledge-graph.md) remains part of each coherent implementation handoff. Structural Rust extraction and semantic contract refresh are merged at repository scope, with source hashes and queries; unrelated historical source coverage remains historical. Current model/probe fragments and the refresh receipt live under `docs/solana/evidence/`. Graph relationships guide navigation and do not establish financial correctness or deployed behavior.
