# Codex planning re-review — round 2 (2026-10-13)

Decision: **REQUEST FOCUSED DOCS-ONLY REVISIONS**, not implementation yet. Reviewed pushed commit `1313b52e5f3f3b2c103d31a9ac097ebd8b930f2c` on `codex/native-solana-plan-20261013`. Root adjudicated two fresh independent Luna reviews; no builds, financial tests, deployment or live mutations were run. Original R1–R6 mechanisms are substantially repaired. Remaining findings below are concrete contradictions within those contracts or accepted defaults, not new product scope.

## Verified preservation

All ten commit blobs equal their worktree counterparts; all 21 protected production/draft/board/manifest baseline file hashes remain unchanged. Six revision pins and fragment hash agree. Graph 7,980 nodes / 11,210 directed relationships / 30 hyperedges preserves every prior 7,928 / 11,089 / 28 identity, no dangling references or duplicate IDs; 173 new line anchors are in range. Exact resolution-node explain works. This proves documentation/graph preservation, not implementation.

## C1 — Make captured reserves executable and sized per candidate set (P1)

Spec lines 269, 297–298, 329, 350–367; plan M2 line 168, M7/M8. `build_candidates` cannot create accounts, and `provision_page` spends free Ops or caller funding. A close that locks all free Ops for missing pages is accepted but no instruction can spend that lock to create its CandidatePages. Provide one explicit bounded `provision_close_page(close_id,set_tag,index)` path spending captured partitions with recorded beneficial rent components before candidate traversal; it may be a documented mode of the existing provisioning instruction. It must not require free Ops again or atomically create an unbounded population of pages at close.

Use per-set sequential existing capacity: `missing_preferred = max(required_per_set - existing_preferred, 0)` and the analogous fallback expression, checked u64 counts/arithmetic. Current aggregate subtraction can underfund one set or underflow when overprovisioned. Widen the two u16 count fields: 262,145 participants already exceeds a u16 aggregate page count, conflicting with spec's no population cap and DR-05's u64 counts. Excess pages remain attributed but never count as capacity of another set.

Add tests: uneven preprovisioning (preferred overprovisioned, fallback absent); overprovisioning without underflow; count >u16 with arithmetic-only fixture; all free Ops locked then permissionless bounded page provisioning succeeds; wrong close/set/index and repeated provision cannot debit twice. Update account table, ABI, reserve formula, M4/M7/M8 test matrix together.

## C2 — One conservation contract for residual locks, provider cash and fee changes (P1)

Spec lines 169, 257–258, 298, 330, 365–371; plan M7/M8. `locked_*` must mean **remaining unspent** cash: decrement exactly when account rent or provider transfers leave OpsCash; record spent category/source amounts separately. Otherwise terminal settlement can refund spent money. `provider_prepaid` cannot increase by fee+rent while defined as unconsumed Client cash: ORAO consumes fee to Treasury and pending rent to RequestAccount during the same request. Describe cash in OpsCash, Client and RequestAccount separately, with disjoint source attribution and idempotent reconciliation after fulfillment, late fulfillment, expiry and actual G withdrawals. Reimbursed rent stays provider-side until physically returned; it must never silently become free Ops or an owner refund. G can mutate Client outside our program, so cached Client values are observations, not authority to mint accounted cash. Define authenticated reconciliation and deficit behavior; deficits cannot block game cash commit/expiry/finalize or invade game funds.

The top-up guard is unreachable after a fully funded lock: live fee rises do not lower `locked_sponsor + locked_owner` below the original `required_reserve`, and the request's old captured fee cap also rejects after top-up. Specify an atomic, permissionless increase of the **provider-only budget/approved cap** from explicitly accounted new sponsor funding, before the immutable deadline. No seed/deadline/candidate/weight change; owner quote is never increased implicitly. Ensure newly funded budget is consumed/refunded exactly once.

Preserve accepted owner fallback (defaults lines 54–57, 69–71): owner may opt in to quoted storage/provider cost. DR-32 silently narrows this to storage-only. Restore that accepted allowance, with sponsor-first consumption and exact category/source attribution for consumed fee, request rent, refundable provider returns and storage. If you believe the pinned adapter makes safe attributable provider-rent recovery impossible, return that precise evidence instead of silently narrowing the contract. Keep bounded fixed per-close sponsor/one-owner components, no donor registry or arbitrary owner sweep. Provider refunds may remain recorded/blocked until a real G-authorized return is verified; round reset and claims cannot depend on that return.

Anonymous transfers to system-owned OpsCash also need a segregated excess term and reconciliation rule (accepted defaults line 67), just like GameVault. No spending/sweep/credit of anonymous excess in first release.

Add state-delta conservation tests covering request CPI failure/success, candidate rent, higher fee then funded top-up, treasury consumption, fulfillment reimbursement, expired and late-fulfilled request, observed G withdrawal/returned funds, unsolicited Client/OpsCash credits, source attribution and no double refund. Use explicit equations and transitions rather than an extra cost service.

## C3 — Complete the PP protocol for unbound identity and recovery (P1)

Spec lines 386–388; plan M12 lines 639–643; accepted defaults lines 85, 90, 94. An unbound pubkey cannot already have every immutable principal/cascade leg computed. Separate a durable validated basis/policy snapshot (`ObservedUnbound`/equivalent) from once-only bound effect/cascade/signup reservation. Shenanigans still owns policy/effects/ledger execution; bridge owns observation/outbox. Define precisely when first validated observation completes and how its policy snapshot survives delivery delay, restart and later SIWS binding. Do not silently replace accepted first-validation freeze with policy chosen on a delayed retry. Versioned policy or an idempotent observation acknowledgement is fine; explain the semantics and align defaults/spec/plan.

Upon first immutable binding, calculate/reserve effects/cascade exactly once, write canonical signup marker before await, and freeze each leg's transfer fields. Use the existing shared principal signup marker so native→legacy and legacy→native ordering cannot give two signup gifts. No historical rebind or relayer-selected recipient.

The bridge may advance past a completed reservation only on `#Reserved` or `#AlreadyReserved`, not merely because `#Rejected` returned. It may checkpoint verified discovery into a durable pending outbox, but never erase/skip the delivery gap. Permanent validation failures quarantine with a reason; transient failures retain retry.

Add `Retryable` and explicit transfer-error semantics: definite rejection vs unknown outcome, identical-fields dedup retry, expiration/quarantine and a proof-bearing ledger-history reconciliation API/procedure. A committed leg requires an actual verified matching ledger receipt; no fresh timestamp and blind remint, no generic manual mark-committed. Validate actual PP ledger interfaces in local integration. Add tests for policy change before binding/delivery, failed observation/reservation, canonical signup both orderings, two simultaneous native first events, explicit rejection, ledger errors/history recovery, forged receipt, upgrade at every durable boundary and one failed sibling after another committed.

## C4 — Bind verified events to genuine program provenance (P1)

Spec lines 383–387; plan M12 lines 641–643. Matching RPC bytes alone does not authenticate a relayer descriptor. Require successful finalized transaction, exact configured program ID, canonical event-authority PDA and Anchor self-CPI discriminator/encoding, instruction ancestry/index, event schema/cluster, and exact equality of descriptor fields with decoded owner/beneficiary/basis/round. Decode signer provenance appropriate to the instruction; promotion's beneficiary comes from program event state, never keeper signer. Test forged program, forged event authority, failed transaction, wrong ancestry/index, schema/cluster and mismatched recipient/basis descriptor. This is part of already accepted finalized native-event verification, not a new trustless proof project.

Extend M14/M15's existing separately authorized acceptance evidence to cover the PP path: finalized genuine event → two observations → unbound/bound identity → frozen reservation → ledger receipt/reconciliation → no legacy duplicate. All real ICP changes remain separately authorized; local fixtures prove only local paths.

## C5 — Exact source/artifact provenance (P1 plan correction, source fix in M0)

Plan lines 66, 99, 604, 667 promises a source-tree hash; current draft `build-sbf.sh:9–10,75–79` hashes only lib.rs. Define one canonical deterministic digest over the full production source set plus relevant manifests, lockfiles and build inputs, with ordered relative paths and content hashes, and use it in build/CI/release receipts. Changing state.rs/allocation.rs must change it. Schedule the script fix in M0; do not edit production source during these revisions.

M14: hash exactly the reviewed ELF length after the verified loader metadata header, then separately check remaining ProgramData bytes are expected allocation padding. Never blindly trim trailing zero bytes from an ELF; record expected length and loader format. Apply the same rule in M15.

## C6 — Small coherent wording corrections and delegated authorization

- Spec line 272 must say ORAO account owner / Client.owner=G / Client.state=RngAuthority; audit stale launch/readback role wording.
- DR-04 line 51 holds cost-schedule inputs, not a flat close reserve amount.
- Correct stale spec R15 toolchain wording against the repaired M0 distinction.
- Clarify frontend `advanceTicket` (plan line 626) and old availability companion as client orchestration over six typed instructions, not a restored generic on-chain ABI.
- `close_simple_position` with owed zero creates no cash/close reserve; state explicitly it needs no contribution argument, reconciling DR-09's broad wording.
- Plan lines 3/19 no longer need another Rob local-stage go-ahead. Rob delegated settlement and implementation authorization to Codex after agreement. Record DR-22 native default + accessible labeled legacy withdrawals as Codex's routine accepted choice. Still wait for Codex's exact final acceptance/go-ahead; devnet/mainnet/production ICP/service activation/PR merge remain unauthorized.

## Bounded return and next decision

Claude owns the docs-only amendments and graph refresh. Use the same plan branch/coordinator/kanban; preserve every dirty implementation file and prior raw graph identity. Provide one pushed successor with spec/plan/decision-register/packet and scoped hash/graph receipt; include this review as readable evidence. No builds or source edits yet. Return item-by-item C1–C6 resolution or concrete disproof, exact commit and hashes, and acknowledge whether you agree the final plan is executable. Codex will verify the successor and authorize bounded local implementation once the concrete contracts pass. No additional exploration, optional-scope gates or new coordinator.
