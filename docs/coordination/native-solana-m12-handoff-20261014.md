# Native Solana M12 handoff — ICP bridge canister and PP exactly-once, local replica only (2026-10-14)

Branch `codex/native-solana-production-core`. Receipts: `docs/solana/evidence/2026-10-14-m12-bridge.json`
(coordinator), `native_sol_bridge/test/replica/receipt-2026-10-14.json` (the full replica run, ~280 KB, every call
and reply). Kanban: IMPL-47a (prerequisites), 47b (bridge), 47c (Shenanigans), 47d (replica integration).

## Commits

| commit | what |
| --- | --- |
| `662a28e` | fixtures: 29 real finalized getTransaction captures from the local validator; pinned SOL RPC canister candid; pinned Motoko rig (IMPL-47a) |
| `faed379` → `cd0630d` | gated local SIWS login test: the closing-fixture owner Bow1CG… bound to m7ydg-… on the real ic-siws wasm; full delegation flow + browser-session export |
| `a33e688` | feat(icp): native_sol_bridge — pinned SOL RPC types, event identity, wire/Borsh decoding, provenance rules, durable outbox + cursor, actor glue (IMPL-47b) |
| `6a0feeb` | feat(shenanigans): NativeRewards pure state machine + additive actor glue, exactly-once legs (IMPL-47c) |
| `ca00bbc` | fix(bridge): a successful forward clears the attempt cooldown so releasePending can bind immediately (47d finding 1) |
| `c2b6454` | test(icp): SOL RPC stub at the pinned id, replica driver and receipt; fixture generator moved into the repo (IMPL-47d) |

## What M12 delivered

1. **`native_sol_bridge`** (spec §9): event identity (cluster_domain, program_id, signature, instruction_index, inner_instruction_index, event_schema_version); two `getTransaction` reads through the SOL RPC canister against two explicit distinct `#Custom` provider lists (commitment finalized, base64, `maxSupportedTransactionVersion = 0`), agreement checked in the canister; provenance rules (program id, event-authority PDA, stack height > 1, success, ancestry / inner index, schema and cluster, recipient / basis / round / close consistency, owner among signers for owner events, promotion beneficiary from the event, plus `plan` and `kind`); the forwarded payload always re-derived from the decoded event; a durable outbox whose cursor is derived from the open entries (Pending / Unverified / Verified-unacknowledged block it; Forwarded / Quarantined / Invalid let it pass); relayer-allowlisted `submitEventDescriptor`, permissionless `verifyAndForward` (30 s per-event cooldown) and `releasePending`; queries without RPC sources.
2. **Shenanigans boundary** (R6 / C3): `NativeRewards.mo` pure module (canonical event key, versioned pinned policy, leg computation under the pinned policy with the legacy cascade and effects, frozen memo = sha256("musicalchairs/native-reward-leg/v1" ‖ key ‖ be32 index) and created_at_time, Ok / Duplicate / Retryable / Quarantined mapping, dedup-window rule, reconcile only from the pinned ledger or a listed archive with kind == mint and exact fields) and an additive actor block (+366/−0; legacy mint paths pinned by SHA-256): `observeNativeEvent` (bridge allowlist), `bindAndReserve` (siws_provider `get_principal`, state re-read after the await, signup marker claimed with the reservation before any ledger await), `executeNativeRewardLeg` (SubmittedUnknown persisted before the single icrc1_transfer), `retryNativeRewardLeg`, `reconcileLegFromLedger`, queries, admin allowlist / policy appends. Four new stable vars, stable-compatible.
3. **Replica integration** (IMPL-47d, the first cross-canister evidence, LOCAL only): a SOL RPC stub at the pinned mainnet id serving the real captured fixtures; shenanigans at the ledger's minting account id; the real pp_ledger and the real ic-siws provider. closing-012 (PositionOpened, the bound owner): submit → verify (both providers) → forward (Observed) → bind (Reserved through the real siws_provider) → four legs executed → Committed at ledger blocks 19–22 as exact mints → reconcile no-op; closing-009 (PromotionApplied) with the snapshot-rollback reconcile; flows-007 (unbound) stays pubkey-pending and never blocks the cursor; four disagreement / gap modes leave the event Unverified with the cursor stuck; a forged descriptor is Invalid and the corrected one accepted; idempotency at every call; four upgrade points with identical state (incl. a SubmittedUnknown leg across an upgrade with the ledger stopped, then committed by the identical retry); phase-2 concurrency (one gift, one reservation, one mint; quarantine past the window; recovery from the real ledger). 115 checks, 0 failed, in the final full run.

## Findings from the replica run

1. `releasePending` answered `#Cooldown` for up to 30 s after a forward — fixed (`ca00bbc`, regression in `outbox.test.mo`).
2. The local ICRC-1 ledger TRAPS on an over-long memo (IC0503) instead of returning an `Err`, so Shenanigans' "memo" GenericError → permanent heuristic never fires there (a trap reaches the catch as CallFailed → SubmittedUnknown until the window passes). Unreachable today: memos are always the 32-byte hash. Recorded for Codex.
3. The bridge never calls `executeNativeRewardLeg`; the admin identity executed every leg in this run. `executeNativeRewardLeg` is bridge-allowlist-or-admin; `retryNativeRewardLeg` (any non-anonymous caller) only re-presents submitted legs. OPEN for Codex: have `releasePending` execute the reserved legs, or allowlist the keeper.

## Decisions recorded for Codex

- Shenanigans (worker B): the residual legacy→native double-gift window (the legacy observer writes its marker after its own mint await; closing it is a one-line legacy edit, and legacy mint paths were to stay untouched); PromotionApplied uses the Backer rate as the B-loss basis (assumption); `lastQualifyingDeposit` is not written for native deposits (native-only uplines inactive in cascades); a pending leg executed after the window is quarantined recoverable with nothing minted; the signup marker is claimed with the reservation (the brief's "before the get_principal await" is impossible: the principal does not exist before it).
- Bridge (worker A): `maxSupportedTransactionVersion = 0` (highest decodable); `plan` and `kind` compared beyond the spec's field list (a relayer could otherwise claim Compound30 for a Simple position); durable `Invalid` records the cursor may pass (one bad descriptor cannot wedge the cursor or squat an event id); `Config.event_authority` configured and verified offline as findProgramAddress(["__event_authority"]).
- Integration (47d): executor identity (finding 3); `rpcCycles` 25e9 per call and the 128 KB response estimate are placeholders (the stub refunds everything).

## Not proven

The real SOL RPC canister wasm; the archive reconcile path (no local archives); reconcile against a transfer/burn block; the wrong-deployment guard; bridge-driven execution; whether the phase-2 concurrent calls interleaved inside the canister; a bridge crash between Shenanigans' reply and `applyForward` (simulated only); the bridge's await paths beyond this stub; anything on mainnet or a public network. Local side effects: pp_ledger stopped for seconds three times (restarted by the run's trap); shenanigans / bridge / stub reinstalled each run; local ledger length 30.

## Proof level

Standalone `moc -r` suites (bridge 48 / 94 / 92, Shenanigans 83 / 75 / 55 + legacy-untouched) ALL PASS under the pinned rig; actor glue type-checked and built by dfx's moc 0.16.2 and exercised on the local replica by the 115-check run. No deployment, no live keys, no public network, no merge.

## Staffing

Coordinator: prerequisites (fixtures, candid, rig, local replica, SIWS binding), briefs, verification runs, the cooldown fix + regression, receipts, handoff, graph. Two Sonnet Motoko workers (47b bridge → 47d integration; 47c Shenanigans).

## Next

M13 is delivered in parallel (`native-solana-m13-handoff-20261014.md`). M14 devnet acceptance (incl. the PP path acceptance C4 with the real SOL RPC canister) is an AUTHORIZATION GATE.
