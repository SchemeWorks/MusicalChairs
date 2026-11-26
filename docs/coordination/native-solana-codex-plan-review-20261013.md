# Codex review of Claude's native-Solana planning package

Reviewed artifact: `db48ce33a40e10d0a1153d738926eb051bb3b033`, branch `codex/native-solana-plan-20261013`. Review date: 2026-10-13 local / 2026-10-14 UTC. **Verdict: request revisions before implementation authorization.** The scope and most economics are preserved; the issues below are specification/plan corrections, not authorization to fix production code. No source, dependency, existing planning document, deployment, wallet or funds changes were made in this review.

The review combines coordinator source checks and two independent native Luna reviews. Initial worker severity labels were not adopted automatically: missing feature flags and test outlines are not P0; temporary draft paths and optional stronger Fund A nonce support are not prerequisites. Confirmed defects below are tied to accepted contracts and concrete scenarios.

## Verified baseline and what passed

- All nine files in the planning commit match the current worktree copies. The remote-tracking planning ref and local planning ref resolve to the reviewed commit. This is not a new remote fetch.
- All 17 scoped SHA256 pins and the planning fragment hash match. The graph has 7,921 nodes, 11,080 raw directed links and 27 hyperedges. All 7,149 prior node IDs, 9,594 prior raw directed edge identities and 21 prior hyperedges are preserved; no duplicate node IDs or dangling edges were found. 2,238 fragment node/edge line anchors are within existing files. Historical/stale coverage is explicitly distinguished.
- Fresh launch, ICP GUI/PP/Shenanigans, separately preserved legacy access, gross accrual, strict insolvency, 35/45/20 goldens, rolling quota, permissionless progress, immutable 600-second deadline and no reroll are carried forward.
- Claude correctly identified defects in Codex's unfinished drafts: the Squads discriminator/authority parser, harness compilation and pruning logic need repair and genuine runtime tests. Those drafts must not be treated as tested production code. The expected Multisig discriminator independently computes to `e07479ba44a14fec`.
- Host-model and prior SBF-probe evidence is distinguished from production financial proof. This review did not build or run financial tests. No deployment is authorized.

Paths/line references below are relative to `/Users/robertripley/.codex/worktrees/native-solana-model-probes/musicalchairs`. “Spec” means `docs/superpowers/specs/2026-10-13-native-solana-reconciled-spec.md`; “Plan” means `docs/superpowers/plans/2026-10-13-native-solana-implementation.md`; “DR” means `docs/coordination/native-solana-decision-register-20261013.md`.

## Required contract revisions

### R1 — Persistent nonce identity must survive Position rent reclamation (P1)

**Evidence:** Spec 255–257, 297, 302; Plan 229, 332–338, 357; DR-23 at 167–169. Accepted unknown-outcome protection appears in `docs/coordination/native-solana-defaults.md:82` and the previous spec at 122.

The new Position lifetime ends at owner rent cleanup, but replay protection is only “Position PDA absent/present,” and User has no durable used-nonce state. After a legitimate open, settlement and account reclamation, a freshly signed retry using the old nonce can create and fund another Position. Existing replay tests only cover the account while it still exists. This is a client-retry identity failure, not a claim that the same expired transaction signature can replay forever.

**Change:** either retain the Position/compact nonce tombstone permanently, or specify a durable per-owner monotonic nonce/receipt scheme with its concurrency and failed-call behavior. Do not enable Position reclamation until that invariant exists. Clarify stale-round rent cleanup separately: Plan 357 expects it, but Spec 302 only permits status Closed and defines no stale-to-closed path.

**Add tests:** open → claim/round invalidation → rent reclamation → same-nonce resubmission rejects atomically; different nonce remains valid; unknown-outcome recovery; failed opens do not consume a nonce incorrectly.

### R2 — Closing reserve must be calculated from captured work, not a pilot-sized flat allowance (P1)

**Evidence:** DR-12 at 93–95; Spec 254, 263–264, 315–318, 336–339; Plan 497, 509. Accepted reserve-before-cash contract: `native-solana-defaults.md:53–71`; no population cap: Plan 15 and Spec 181.

`close_reserve_lamports` is a single launch configuration value but is said to cover candidate storage for any captured participant count. Both preferred and fallback lists may contain every loser, requiring two candidate sets. Automatic B promotion may also append beyond a full registry page. Neither a fixed pilot quote nor reservation for only the B account guarantees those costs. A timely captured result cannot simply be deadline-skipped later when promotion discovers missing mandatory storage.

**Change:** define a checked `required_close_reserve(captured participants, registry tail, existing accounts, captured cost schedule)` or a proof that all required accounts are already funded. Cover both candidate sets, provider fee/request rent, automatic B and possible registry extension, plus every remaining required account. Lock the exact obligation before cash moves, sponsor-first and bounded owner second. State the behavior if external provider fees change after the captured quote. A base allowance/per-item cost schedule is fine; a flat maximum that silently caps population is not.

**Add tests:** participants 8/9 and registry tail 8/9; all preferred also in fallback; missing B account; spare sponsor cash cannot be mistaken for a smaller locked obligation; one-lamport shortfall; timely capture followed by B creation/registry append without a new sponsor dependency.

### R3 — Reusable-page cleanup needs a live capacity invariant (P1)

**Evidence:** DR-08 at 71–73 and DR-13 at 97–99; Spec 248, 260, 290–291, 299, 336; Plan 219 and 509.

Payout admission trusts `snapshot_pages`, while cleanup can close any terminal SnapshotPage. No decrement, hole handling or contiguous live-prefix invariant is defined. Closing a page leaves a count that can accept the next payout even though Discover cannot access its captured storage. Closing a middle page is not solved merely by subtracting one from the count. Likewise, closure must check the current active ticket, not a stale page tag from an older completed ticket.

**Change:** make reusable snapshots permanent in the first release, or define an atomic safe tail-only close/reprovision scheme maintaining a contiguous provisioned prefix. Require a validated current-ticket predicate; preserve pre-funded capacity throughout every accepted allocation. Specify counter changes and namespace generation/reinitialization behavior. The same reasoning applies to any reusable page kind the cleanup ABI exposes.

**Add tests:** close tail and middle attempts, next payout before/after reprovisioning, attempt to close an old-tag page immediately after a new ticket starts, and full cash progress under depleted free Ops funds.

### R4 — Guardian authorization and rotation must share the same stored key (P1)

**Evidence:** Spec 248–249 and 285–286; DR-20 at 137–139; Plan 174–204. Accepted G recovery/guardian rotation: `native-solana-defaults.md:37`.

Pause is specified as Admission plus P signer only, but Admission has no guardian key; rotation writes only Pool. Therefore the pause instruction cannot validate the new key under its own stated ABI, or the draft's retained Admission key continues authorizing the old P after rotation.

**Change:** store the authoritative guardian Vault in Admission and update it atomically during G-authorized guardian rotation (and keep any Pool copy synchronized), or explicitly revise the read-account contract without granting P writable financial accounts. The former preserves the proposed narrow two-account pause surface. Validate the replacement guardian profile.

**Add tests:** old P rejected/new P accepted after rotation; arbitrary signer rejected; G rotation can recover lost P quorum; pause metas contain no writable financial accounts.

### R5 — Separate ORAO Client management ownership from request authority (P1)

**Evidence:** DR-17 at 117–119; Spec 230, 253, 266; accepted G-owned provider Client at `native-solana-defaults.md:35,42,45`. Pinned SDK `solana/probes/provider-resources/vendor/orao-solana-vrf-cb/src/state/client.rs:14–24` defines two separate fields; `src/instructions/register/mod.rs:85–110` requires the registered program's upgrade authority as payer.

DR-17 assigns Client ownership to RngAuthority, while the accepted owner is G. ORAO's `Client.owner` can manage/withdraw/transfer the Client; `Client.state` is the program PDA that signs requests. The Solana account itself is owned by the ORAO program. These are three different meanings of owner/authority.

**Change:** specify `AccountInfo.owner = ORAO program`, `Client.owner = G Vault` and `Client.state = RngAuthority`. Preserve the registered program, namespace and captured provider tuple. Fix the related registration/launch wording rather than introducing an undeclared ownership policy.

**Add tests/readback:** distinct owner/state roles, wrong owner and wrong state rejection, G-Squads registration path, request PDA CPI signing, and prospective provider changes without active rebinding.

### R6 — Freeze PP policy once and give ledger-leg execution one authoritative owner (P1)

**Evidence:** Spec 349–354; DR-24 at 171–173; Plan 554–559. Accepted durable outbox/effects/cascade contract: `native-solana-defaults.md:90–96`. Current helper `shenanigans/main.mo:2871–2935` dynamically applies multipliers/siphons, updates siphon caps and marks primary dedup after await; `mintInternal:2015–2028` generates a new timestamp.

The spec says the bridge freezes every leg, but M12 calls `mintFromBridge(leg)` reusing `mintWithEffects` policy. A precomputed final leg sent through that helper can have effects applied again; a retry can consume mutable effects again or strand a siphon leg when the primary was marked complete. Passing a timestamp alone does not repair policy ownership or per-leg reconciliation.

**Change:** choose a concrete cross-canister protocol. For example, Shenanigans durably prepares/reserves native-event reward obligations once under the immutable event ID, and a bounded executor mints exact frozen legs; or the bridge owns exact legs and an authorized Shenanigans API executes them without recomputing effects. Specify which canister owns policy/effect reservations, Pending/SubmittedUnknown/Committed states, immutable transfer fields and history reconciliation. Preserve legacy behavior and actual ledger/subaccount authority.

**Add tests at the Shenanigans boundary:** effects change after freeze; duplicate/concurrent delivery; unknown primary ledger outcome; failed sibling/siphon leg; upgrade/restart between calls; all retries reuse identical fields and never reapply effects/cascade or remint a committed leg.

## Additional plan corrections to make before freezing the ABI

These are concrete corrections, not new feature projects:

- **U256 discovery accumulation (P2):** Spec 261 stores `a_remainder_total u128`, contradicting M1's sum-above-u128 requirement and the sealed U256 contract. Widen the accumulator, not only the final denominator; add a production discovery/seal fixture with a sum above u128. Spec 95–99 should call its per-step quotient an accrual increment and state cumulative accrued minus already-paid earnings; avoid losing unpaid Simple earnings on partial claims.
- **Immutable cluster domain (P2):** Spec 211 requires `cluster_domain_32` but no account in 248–264 or M2 schema stores it. Add immutable genesis domain/config state and prove exact seed bytes and devnet/mainnet separation in M7; forbid active rebind.
- **Provider cash/rent attribution (P2, required in M7/M8 acceptance):** define OpsCash → Client funding, request fee consumption, retained pending/fulfilled rent, and the actual refund back to Client on fulfillment. The local receipt shows that reimbursement goes to Client, not automatically to Ops or the contributing owner. Test asynchronous fulfillment after expiry and retained B/request storage. Choose explicit beneficial attribution or disclosed consumption; do not report a Client reimbursement as an owner refund. Use current runtime receipt amounts, not the older public-rent scenario.
- **Prune aliases (P2):** Spec 308/329's duplicate-writable-key rule needs explicit exceptions for validated same-page and moved==pruned aliases within a logical prune row; M6 explicitly tests those cases. Reject duplicate independent Backer rows across a batch, but decode/write legitimate shared pages once. Keep expected ticket/phase/cursor arguments or define equally strong stale-request rejection for the new dispatcher ABI.
- **Inline default (P2):** Plan 519 excludes all first-release inline paths if N=16 fails. Accepted D6 is conditional on the actual path being feasible; N=1 may fit even if N=16 does not. Preserve N=0/zero-slice shortcuts, implement/measure complete inline at every feasible tested small N, and select an explicit measured threshold. Add an implementation step if the existing handlers only support chunks.

## Execution-proof and release corrections

1. **M0 RED:** Plan 99 invokes the observation CLI, which collects failed transactions into JSON and exits successfully; meaningful failing assertions live in `financial-runtime/src/lib.rs:31–57`. Distinguish receipt generation from `cargo test` failing behavior. Explicitly build/load/pin the current stub ELF before the RED run, repair fixture validity and assert exact `NotImplemented` and fee-only rollback rather than accepting account/PDA/ELF failures as RED. M3 183 also calls an unavailable Anchor `solana_program::hash` re-export in the pinned tuple; use an independent compatible SHA implementation.
2. **Toolchain F6:** builder `--version` prints v1.52, but the wrapper explicitly selects `--tools-version v1.48` and exports the installed Rust 1.84.x path. The displayed builder default alone does not establish that v1.52 was the compiler used. Verify installed compiler/tool hashes/commit stamps and actual invocation before requiring a re-download or treating the prior 88-case evidence as superseded. Also use `MC_SBF_BUILDER` consistently: wrappers treat it as executable, M0 81 treats it as a directory.
3. **Event ABI:** M2 must enable Anchor `event-cpi`, add the emitting contexts' `#[event_cpi]` accounts and generate matching IDL/client fixtures. Test authenticated executed self-CPI event decoding and include the extra accounts/CU in packet measurements. It is ordinary work for the chosen event strategy, not a P0 architecture issue.
4. **Production SBF CI:** M0 says SBF CI stays local until M9, but M9 does not actually add the locked production SBF build and instruction-test CI job. Add it explicitly with exact source/lock/tool/artifact pins; host compilation alone cannot provide that final proof.
5. **Release evidence:** M14/M15 must verify on-chain ProgramData bytes (ELF versus loader header/padding handled correctly), loader authority, source/lock/artifact/IDL hashes and actual ICP Wasm/served frontend version after separately authorized deployments. Distinguish uploaded/deployed bytes, activation/config, broadcasts/finality and reconciled balances. Add authorized ICP deployment/upgrade steps for bridge, keeper and frontend; SOL-only devnet deployment language does not cover those actions.

The 64-counter exhaustion test cannot search for a normal small-N all-rejection output in practical time. Split the numeric boundary proof into a pure sampler test with constructed sample words or a near-2^63 count, and test the production terminal transition using clearly labeled local synthetic state. Do not add a production test-only RNG/winner override.

## Answers to Claude's eight review questions

1. Keep separate phase instructions initially: typed account validation and expected ticket/cursor arguments make auditing and runtime tests simpler. A client can dispatch by phase. A unified dispatcher is acceptable if its exact remaining-account schema, alias exceptions and stale-call behavior are proved; this is a routine choice.
2. A retained CloseRecord is a reasonable way to preserve request/expiry/finalization identity. Define current/next close pointer changes explicitly and budget permanent storage; alternatives are optional, not a prerequisite.
3. Self-CPI events are reasonable for the bridge, provided M2 includes the feature/accounts/IDL and M9 measures overhead. Events are transaction metadata, not a certified ICP message; RPC trust and inner-call provenance checks remain.
4. Owner-only candidate rows are reasonable when closing seals UserRound P/L and the relevant A/B-history flags until selection. Test same-owner multi-position aggregation and immutability rather than assuming it.
5. Keep the accepted u128 global funded-claim field, with the physical vault bound asserted. Change discovery accumulation to U256 as above.
6. Reject the N=16 blanket rule. Choose inline/chunk behavior from each actual measured production path, preserving feasible smaller paths and the one-row fallback.
7. An owned local validator for browser testing is appropriate local test tooling. Label local installation/synthetic fixtures distinctly from devnet/mainnet deployment. No global install or live deployment permission follows from it.
8. A separate native bridge is a reasonable isolation choice, but R6's effects/outbox/ledger authority protocol must be explicit. Isolation alone does not produce exactly-once minting.

DR-22 remains a user-visible preference; recommendation: native SOL view by default with clearly labeled, accessible legacy withdrawals. It can be decided before M11 and should not block earlier core planning. Effort estimates are conditional planning ranges, not delivery commitments. Preserve the current GUI; no framework migration is needed.

## Nonblocking worker suggestions and current facts

- Temporary drafts are useful references, not implementation authority. Preserve/hash them if useful, but implementations can be re-derived from the checked-in accepted oracle. A pushed draft-code snapshot is not an extra required gate for plan approval.
- Additional Fund A nonce support is optional. The client must preserve the original signature on unknown outcomes and resolve finality/history before any deliberate new funding; add an explicit Fund A retry test without silently changing the accepted repeatable ABI.
- Existing host vector tests and M5/M6 test outlines cover many cases. Make the production differential matrix explicit (repeated Simple claims, backwards time, fee boundaries, clamp, Compound30, u128 owed, only-B folds/caps/dust); do not call lack of every expanded example a separate P1 by itself.
- Current [Solana versioned-transaction docs](https://solana.com/docs/core/transactions/versioned-transactions) confirm v1 availability; retaining the approved legacy/v0 GUI is consistent. Newer observed transactions still need raw history decoding.
- Rent activation should be freshly quoted from RPC/feature state at measurement and launch. The [official step-2 feature page](https://explorer.solana.com/address/61BtM7BkDEE8Yq5fskEVAQT9mYA8qCejJWoLe5apqg81) reports mainnet activation; the [Foundation rollout page](https://solana.com/el/upgrades/reduced-rent) still lists testnet for that step. Do not treat an older narrative page as a precise current cluster quote.

## Follow-up prompt for Claude

> Review Codex's `docs/coordination/native-solana-codex-plan-review-20261013.md` in the fresh managed worktree. Amend only the specification, decision register, implementation plan, review packet and graph evidence. Resolve R1–R6 with explicit account/state/authority/resource contracts and the listed regression tests. Apply the concrete ABI/proof corrections, including U256 discovery sums, immutable cluster domain, provider refund attribution, safe aliases, feasible-small-N inline behavior, meaningful RED tests, event-CPI accounts, production SBF CI and exact deployment-byte evidence. Correct rather than repeat the unsupported installed-v1.52 inference. Preserve all unfinished source unchanged. Push a docs-only successor commit and summarize each review item as fixed or disproven with exact lines/evidence. Do not implement, deploy, fund, sign or merge. Return the revised package for Codex re-review and Rob's subsequent implementation authorization.

## Scope and remaining review

This is one isolated dual-review round followed by coordinator confirmation/triage, with focused follow-ups to both reviewers. No revised package exists yet, so no second readiness review was run. The verdict remains request revisions. The new review artifact receives a scoped graph fragment/receipt; existing planning and source SHA pins remain unchanged. No message was sent to Claude.
