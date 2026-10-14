# Codex review packet — native Solana discovery, specification and plan (2026-10-13)

Prepared by Claude (planning coordinator) for Codex's independent review and Rob's decision. **No implementation, build, test run, deployment, funding, signing, merge or canister change occurred in this pass.** The uncommitted production tranche was read, hashed and left exactly as found (unstaged, unmodified).

## 1. What to read, in order

| # | Document | Lines | SHA-256 (working tree, 2026-10-13) |
| --- | --- | ---: | --- |
| 1 | [Current-state and evidence inventory](native-solana-state-inventory-20261013.md) | 188 | `a04d6aaa82819bb90cc11515e28f1a7187551938e39d367465a8294cd24b346f` |
| 2 | [Reconciled specification](../superpowers/specs/2026-10-13-native-solana-reconciled-spec.md) | 398 | `576879351052904b2dde36116f3dba1936c78e2224f52bc2d8741da88a163a2e` |
| 3 | [Decision register](native-solana-decision-register-20261013.md) | 203 | `a9bb1b08b582b47ce70bcdfff4bc7dcea32a830afd893be92e3f043ca8dd2893` |
| 4 | [Implementation plan](../superpowers/plans/2026-10-13-native-solana-implementation.md) | 632 | `74461a4388da63f6dd787c19ff16ca79493983335cbc60c0bd6ccb00f6d9f95a` |
| 5 | This packet | — | recorded in the graph refresh receipt |
| 6 | [Planning graph refresh receipt](../solana/evidence/2026-10-13-planning-graph-refresh.json) and [fragment](../solana/evidence/2026-10-13-planning-graph-fragment.json) | — | hashes inside the receipt |

Banners were added at the top of `docs/superpowers/specs/2026-10-12-native-solana-design.md` and `docs/superpowers/plans/2026-10-12-native-solana.md` pointing to their successors; no other line in those files changed. All other files in the worktree are untouched by this pass. Verify with:

```bash
cd /Users/robertripley/.codex/worktrees/native-solana-model-probes/musicalchairs
git status --porcelain
shasum -a 256 docs/coordination/native-solana-state-inventory-20261013.md docs/superpowers/specs/2026-10-13-native-solana-reconciled-spec.md docs/coordination/native-solana-decision-register-20261013.md docs/superpowers/plans/2026-10-13-native-solana-implementation.md
```

The expected `git status` shows Codex's prior dirty paths (`docs/coordination/kanban.md`, `solana/Cargo.toml`, `docs/solana/production-accounts.md`, `solana/programs/`, `solana/tests/`) plus the five new planning documents, the two banner edits and the two evidence JSON files. Hashes of every pre-existing dirty file are in inventory §5 and must still match.

## 2. Reconciliation summary (what changed in meaning, not just wording)

1. Status wording is replaced by a four-rung ladder: accepted policy / host-modeled (72 + 5 tests recorded) / SBF-probed (88 cases recorded) / draft (10 handlers `NotImplemented`). No count was re-run.
2. Nineteen document/source disagreements are resolved in spec §2 (R1–R19). None changes an accepted invariant. Two correct the handoff text itself: `createBackdatedGame` is not in the SOL canister (R13); candidate rows are 32 bytes (R14).
3. Draft deferrals are confirmed as tranche boundaries: owner contribution in the ABI from day one (DR-09), full close/provider/reopen (M7), reserve/refund/cleanup (M8), config versioning (DR-04), retained `CloseRecord` (DR-02).
4. One wording deviation is declared (DR-14: locked-reserve settlement at terminal promotion, rent components at safe closure); behavior and conservation unchanged.

## 3. Findings Codex should check first

| # | Finding | Where | Why it matters |
| --- | --- | --- | --- |
| F1 | Squads `Multisig` discriminator constant ends `0xc8`; correct last byte is `0xec` | `controls.rs:438`, fixture `:711` | Real governance accounts would be rejected; the draft's four unit tests are circular |
| F2 | `config_authority` parsed as `Option<Pubkey>`; Squads v4 stores a plain `Pubkey` (default = autonomous) | `controls.rs:446–447` | Parse desynchronizes for exactly the accepted autonomous profile |
| F3 | Harness does not compile (`Signature.map`) | `financial-runtime/src/runtime.rs:293` | No financial test has ever executed; RED baseline must be captured first (plan M0) |
| F4 | Allocation draft prune: same-page swap-delete errors; alias branch never deactivates pruned Backer | `/private/tmp/mc-allocation-handlers-20261013.rs:464–530` | Would stall tickets or leave phantom active rows; regression tests specified in M6 |
| F5 | `mc-production-accounts-contract.md` reverses senior/A (45/35) | `:69–78` | Stale proposal; production source is correct at 35/45/20 |
| F6 | Platform-tools pin conflict v1.48 (provenance) vs v1.52 (cached binary) | inventory §4 | Must be resolved before any build claim (M0 step 2) |
| F7 | Rent reduction SIMD-0437 in progress; v1 4,096-byte transactions live on mainnet | inventory §8 | Budget inputs are dated scenarios (DR-27); GUI stays v0, keeper format is a later measured decision (DR-28) |
| F8 | ICP docs guide prints an invalid SOL RPC principal; repo binding `tghme-zyaaa-aaaar-qarca-cai` is the valid one | inventory §8 | Verify live Candid before the bridge binds (M12) |

## 4. Sharp questions for Codex

1. Spec §7.3 collapses the draft's seven allocation entrypoints into one `advance_ticket(rows)` dispatching by phase. Do you see a reason to keep separate instructions (e.g., simpler account validation per phase or smaller instruction data)? The plan tolerates either; the ABI must be frozen in M2.
2. DR-02 moves close state into a retained `CloseRecord` provisioned ahead of time. Is there a cheaper way to satisfy "old callbacks cannot attach to a later round" that you prefer?
3. DR-11 picks `emit_cpi!` for events because the ICP bridge decodes transactions via RPC and logs are truncatable. Any objection to the extra CU and the self-CPI account in every financial instruction?
4. DR-03 stores candidate rows as owner pubkeys and re-reads `UserRound` at apply. Agree that `UserRound` is immutable once the round is sealed, so this is safe?
5. R17: accepted text says global funded claims are u128; the draft used u64. Keep u128 as specified, or do you want to argue u64 on the grounds that the identity bounds it by vault lamports?
6. M9 may show that no inline path fits at N = 16 with production accounts. Are you content that this yields a "not a first-release path" record rather than any economics change?
7. The plan runs the frontend browser verification against a locally installed `solana-test-validator` (Agave 2.3.13) in an owned temp dir. Acceptable dev tooling under the no-global-install rule, or do you prefer LiteSVM-only until devnet?
8. DR-24 proposes a new `native_sol_bridge` canister rather than extending `shenanigans`. Do you agree that isolating the RPC trust boundary justifies a new canister, given the extra controller/cycles surface?

## 5. Risks carried into implementation

- **Packet fit:** production rows carry more accounts than the probe kernel; eight rows may not fit 1,232 bytes. One-row fallback is mandatory and measured in M6/M9.
- **Provider liveness:** local lifecycle used one synthetic fulfill authority; devnet latency and mainnet quorum are unproved until M14.
- **Governance parse:** F1/F2 show the draft cannot be trusted on Squads layout; M3 derives fixtures from the pinned Squads source commit.
- **Heap/CU:** lazy errors are applied in the tranche; M9 measures worst-case rows.
- **Rent volatility:** every lamport figure in budgets is a dated scenario.
- **Legacy observer defects** (owner-only `solBackerSeen`, fresh `created_at_time` per mint) are recorded but out of scope; the bridge does not inherit them.

## 6. Proposed acceptance checklist for the plan (before Rob authorizes)

- [ ] Codex confirms spec §2 reconciliations R1–R19 do not alter any accepted invariant.
- [ ] Codex confirms DR-01–DR-28 classifications (ROUTINE / ROB / MEASURE / LAUNCH / DEFERRED) and flags any it believes needs Rob.
- [ ] Rob decides DR-22 (native default view with labeled legacy section).
- [ ] Codex agrees the test matrix covers every §3–§8 invariant or names gaps.
- [ ] Codex agrees M0's RED baseline + toolchain re-pin precede any handler work.
- [ ] Both agree the authorization gates (local / devnet / mainnet) are correctly placed.

## 7. Optional backlog (explicitly not prerequisites)

Native PP/SPL and desk; refund of anonymous excess; paid keeper rewards; frontend v1/Kit migration; Pinocchio/custom accumulators; more governance signers, custom timelock, renounced upgrades; ICP `raw_rand` alternative; account merging; legacy observer fixes (`solBackerSeen` keying, mint timestamp); candid sync of `getBackerLifetimeRepaid`/`getOutstandingBackerDebt` in the checked-in `.did`.

## 8. Exact inventory of changes made by this pass

Created: the five documents in §1 plus `docs/solana/evidence/2026-10-13-planning-graph-fragment.json` and `docs/solana/evidence/2026-10-13-planning-graph-refresh.json`. Modified: two banner lines (top of the 2026-10-12 spec and plan). Graph: `graphify-out/` (gitignored) updated by merge with the fragment; pre-merge copies kept at the session scratchpad and at `/private/tmp/mc-production-graph-base-20261013/`. Not touched: `docs/coordination/kanban.md` (carries Codex's uncommitted board lines), `solana/Cargo.toml`, all `solana/programs/**` and `solana/tests/**`, every `/private/tmp/mc-*` draft, every frontend/Motoko file, memory/persona catalogs.

Git artifact: planning documents and graph evidence only are committed to branch `codex/native-solana-plan-20261013` (created from `c99a636` without changing the worktree's checked-out branch); the branch is pushed for review; no existing branch is force-pushed; nothing is merged.

## 9. Stop

Implementation begins only after Codex's review and Rob's explicit authorization. The first implementation commit (plan M0 step 1) adopts the tranche verbatim so these hashes remain the review baseline.
