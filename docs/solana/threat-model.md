# Musical Chairs native Solana program — threat model (M10, 2026-10-14)

Program `7aSvuR19bh9WBHoPC5ceLNvP6txxFkowTJ2FSi9fnf3o` (Anchor 0.32.1), reviewed at commit
`3c3003c`/`37185ea` by two read-only personas
([security review](evidence/2026-10-14-m10-security-review.md),
[economy audit](evidence/2026-10-14-m10-economy-audit.md)); repairs, accepted items and the
decisions returned to Codex are in
[`docs/coordination/native-solana-m10-review-triage-20261014.md`](../coordination/native-solana-m10-review-triage-20261014.md).
Every runtime claim below is LiteSVM 0.7.1 on the production ELF unless marked "cluster rule"
(a reading of Agave source, not a measurement). Spec =
`docs/superpowers/specs/2026-10-13-native-solana-reconciled-spec.md`.

## 1. Assets

| asset | where | who may move it |
| --- | --- | --- |
| Game funds: pot, seed, management, pending toll, funded Backer claims | `GameVault` PDA (9 B, program-owned) partitioned by `Pool` | only `send_game` (payouts to the owner's destination, Backer claims, management withdrawal); conservation `vault − rent == pot + seed + management + pending_toll + funded_claims + excess` after every instruction |
| Operations cash: free balance, locked close partitions (sponsor / owner remaining), unallocated excess | `OpsCash` PDA (system-owned, data-less) partitioned by `Ops` and per-close `CloseRecord` | `transfer_from_cash` for close costs (pages, request rent, oracle fee) and the sponsor-signed `withdraw_operations` (free balance only); `locked_remaining_total == Σ partitions` |
| Owner refunds | `CloseRecord.locked_owner_remaining` (deferred when undeliverable) | the recorded `owner_refund` key only; permissionless delivery |
| Provider cash | ORAO `Client` PDA (program-registered, G-owned) and the request accounts | ORAO program; returns via `withdraw_provider_return` (G) with the owner share preflighted |
| Rent deposits | pages, request accounts, CloseRecords (permanent) | cleanup returns page rent to the recorded funder/owner; CloseRecord rent is retained by design (DR-02) |
| Authority state | `Pool.governance` (G vault), `Admission.guardian_vault` (P vault), recipients, `ConfigVersion`s, `RngAuthority.client`, `ProgramData.upgrade_authority` | G (Squads 2-of-3, 172,800 s delay) for config/recipients/guardian; P for pause; nobody for `RngAuthority.client` (genesis-only, validated — SEC-4) |

## 2. Actors and trust

- **Position owners / Backers**: untrusted; sign their own instructions; choose destinations
  (validated, never a protected account); bounded by admission gates and pinned config.
- **Keepers / any payer**: untrusted; drive permissionless instructions (ticket phases, close
  pipeline, provisioning, cleanup, delivery, reconciliation). They can only spend fees and
  choose *when*; every state transition is deterministic from on-chain state and the
  ORAO fulfilment. Griefing surface: forcing tickets (toll ≥ 5) and timing.
- **G (governance Squads)**: trusted for configuration within the pins; holds the upgrade
  authority (spec L243/L249: can shorten its own delay and upgrade arbitrary code after
  it). Not trusted to spend game funds: no instruction lets G move pot, seed, claims or locked
  partitions; `withdraw_operations` is sponsor-signed and free-balance only.
- **P (guardian Squads)**: trusted only to pause admission; cannot block exits (payouts,
  claims, position close, ticket progress all run while paused).
- **ORAO VRF**: trusted oracle for the draw; late/duplicate/forged fulfilment handled
  (capture gates, AlreadyObserved, Ed25519 verification inside ORAO).
- **Solana runtime**: rent-state rule for data-less destinations (cluster rule) differs from
  LiteSVM (gap pinned by `tests/transfer_constraints.rs`).

## 3. Per-instruction authority and hand-validated accounts

Anchor-typed accounts are PDA/discriminator/owner-checked by the framework. `UncheckedAccount`s
are validated in the handler as listed (file:line anchors in the security review's checklist row 1).

| instruction (accounts struct) | signers | authority | hand-validated accounts | how |
| --- | --- | --- | --- | --- |
| `DiscoverRows` (`allocation.rs:1313`) | none | permissionless (keeper) | — | Ticket PDA; expected_ticket_id / cursor; pages by PDA; phase gate |
| `ApplySenior` (`allocation.rs:1323`) | none | permissionless | `backer` | Ticket; the tracked oldest open A Backer PDA |
| `SealWeights` (`allocation.rs:1336`) | none | permissionless | — | Ticket phase |
| `AllocateRows` (`allocation.rs:1343`) | none | permissionless | — | Ticket; cursor; Backer PDAs from registry rows; duplicate writable refused |
| `PruneRows` (`allocation.rs:1352`) | none | permissionless | — | Ticket; cursor; swap-delete rows with placeholder rule |
| `CommitTicket` (`allocation.rs:1363`) | none | permissionless | — | Ticket phase; optional CloseRecord for closing tickets |
| `ClaimBacker` (`claims.rs:24`) | owner | Backer owner | `destination` | owner Signer; Backer PDAs (owner, A/B); destination validate_destination + protected list; pays finalized funded only (M9-F1 accepted: no deliverability preflight) |
| `BuildCandidates` (`close.rs:772`) | none | permissionless (keeper) | `preferred_page`, `fallback_page`, `ops_cash`, `owner_refund` | CloseRecord; participant + candidate page PDAs; cursor; batches within page edges (M8-F2); owner_refund = recorded key when paying |
| `RequestRoundDraw` (`close.rs:797`) | payer | permissionless payer (fee-only) | `ops_cash`, `provider_program`, `network_state`, `treasury`, `client`, `request` | validate_provider_tuple (config tuple, ORAO program id, Client PDA + decode, treasury, network state); RngAuthority.client; request PDA = request_binding(client, protocol_seed); deadline; fee cap; locked partitions sponsor-first |
| `CaptureRoundDraw` (`close.rs:835`) | none | permissionless | `request`, `client` | request/client decoded and bound; terminal-first then deadline (M7-F2); LateCapture no-op |
| `ApplyPromotion` (`close.rs:852`) | payer | permissionless payer | `ops_cash`, `candidate_page`, `winner`, `winner_user`, `winner_user_round`, `winner_backer_b`, `registry_tail`, `owner_refund` | candidate page + digest chain; winner PDAs derived from the drawn candidate (User/UserRound/Backer B/registry tail); owner_refund = recorded key; B entitlement 116% |
| `ExpirePromotion` (`close.rs:892`) | none | permissionless | `ops_cash`, `owner_refund` | now > deadline; draw not captured; owner_refund = recorded key; settle_books |
| `FinalizeRound` (`close.rs:910`) | none | permissionless | — | terminal promotion; carries the seed once; round_id + 1; idempotent |
| `IncreaseProviderBudget` (`close.rs:918`) | none | permissionless (DR-33) | `ops_cash` | before deadline; ops_cash PDA; raises the approved fee cap from free balance |
| `ObserveRequestFulfilled` (`close.rs:930`) | none | permissionless | `request`, `client` | request decoded (fulfilled); eligibility = funded − retained; AlreadyObserved |
| `WithdrawProviderReturn` (`close.rs:944`) | governance | G vault | `governance_multisig`, `ops_cash`, `provider_program`, `client`, `owner_refund` | require_governance (SEC-5); validate_client; Client.owner == G vault; owner share preflighted BEFORE the ORAO CPI, never re-locked |
| `ReconcileOpsCashWithClient` (`close.rs:974`) | none | permissionless | `ops_cash`, `client` | sync_ops + client balance view |
| `DeliverOwnerRefund` (`close.rs:988`) | none | permissionless | `ops_cash`, `owner_refund` | reserve_settled; deliverable (credit_is_deliverable) else RefundNotDeliverable |
| `Initialize` (`controls.rs:1545`) | sponsor, governance | G vault (CPI signer via Squads) + sponsor | `ops_cash`, `squads_program`, `governance_multisig`, `guardian_multisig`, `guardian`, `self_program`, `program_data` | ProgramData.upgrade_authority == G vault; G and P Squads profiles read and validated (2-of-3, delay / zero-delay); provider_client == derived ORAO Client PDA (SEC-4) |
| `StageConfigVersion` (`controls.rs:1590`) | governance | G vault | `governance_multisig`, `ops_cash`, `config` | require_governance (profile + vault derivation + signer); config PDA = ["config", version]; ConfigTerms::validate pins (rates, entitlements, seed half, cutoff, quota, deadline, Carried Interest table — SEC-1) |
| `ActivateConfigVersion` (`controls.rs:1612`) | none | permissionless | — | version == active + 1 == expected_current + 1; round_id >= effective_round_id; Active, no ticket, not activated |
| `RotateRecipient` (`controls.rs:1625`) | governance | G vault | `governance_multisig` | require_governance; new recipient not a protected account |
| `RotateGuardian` (`controls.rs:1637`) | governance | G vault | `governance_multisig`, `new_guardian_multisig` | require_governance; new multisig passes the P profile |
| `Pause` (`controls.rs:1652`) | guardian | P vault | — | Admission.guardian_vault == signer vault; touches Admission only |
| `FundOperations` (`controls.rs:1660`) | sender | any sender | `ops_cash` | OpsCash PDA key; free_balance += amount |
| `WithdrawOperations` (`controls.rs:1673`) | sponsor | sponsor recipient | `ops_cash` | Pool.sponsor_recipient; amount <= free_balance and cash >= rent + locked + amount |
| `WithdrawManagement` (`controls.rs:1686`) | recipient | management recipient | `ops_cash` | Pool.management_recipient; amount <= pool.management; send_game |
| `ProvisionPage` (`controls.rs:1700`) | payer | any payer | `ops_cash`, `page` | page PDA re-derived from (kind, index); sequential index; fresh system account; funding source Ops or caller |
| `ProvisionCloseRecord` (`controls.rs:1720`) | payer | any payer | `ops_cash`, `close_record` | close PDA = ["close", next_close_id]; fresh account |
| `CloseSharedAccount` (`controls.rs:1737`) | none | permissionless | `page`, `ops_cash`, `owner_refund` | closable predicate (decoded discriminator, re-derived key, canonical record); refund to the RECORDED owner_refund key; Registry/Snapshot never closable |
| `ReconcileOpsCash` (`controls.rs:1755`) | none | permissionless | `ops_cash` | sync_ops only (floating excess — ECO-3) |
| `FundSeriesA` (`positions.rs:238`) | owner | owner | `registry` | owner Signer; Backer PDA (owner, A); registry page PDA; gates (ticket, closing, min_a) |
| `Payout` (`positions.rs:304`) | owner | owner (withdraw_simple / settle_compounding) | `ops_cash`, `user_round`, `destination` | owner Signer + position has_one; destination validate_destination + protected list; ops_cash PDA; user_round decoded and bound to owner/round; optional CloseRecord = ["close", next_close_id]; inline remaining accounts = exact layout or refused |
| `CloseSimplePosition` (`positions.rs:378`) | owner | owner | — | owner Signer; owed == 0; maturity |

## 4. Checklist (plan M10 step 2): control → evidence

| # | item | control | evidence (harness row / unit test) | status |
| --- | --- | --- | --- | --- |
| 1 | signer / owner / PDA / discriminator substitution on every instruction | Anchor constraints on every struct + the hand validations above | M3 controls rows (every G/P instruction with rogue multisig, wrong vault, wrong profile), M5 admission rows, M6 `stale_ticket_id_is_rejected_on_every_phase_instruction`, M7 close tamper rows (`withdraw_rejects_…`, wrong client/program/destination/config), M8 reserve/cleanup rows; **new** IMPL-40b `claim_backer_substitutions_are_refused`, `wrong_ops_cash_and_wrong_config_pda_are_refused`, `open_and_fund_series_a_need_their_provisioned_pages`, `wrong_owner_refund_is_refused_while_an_owner_remainder_is_locked`; `initialize…wrong_provider_client` (SEC-4) | covered |
| 2 | duplicate mutable aliases | `DuplicateWritableAccount` on remaining accounts (allocation, inline, candidates); `ClaimBacker.backer_a/b` unreachable alias via series check | M6 `duplicate_independent_backer_rows_are_rejected_but_legitimate_prune_aliases_are_decoded_once`, inline `n4.duplicate`, compounding `n4.duplicate`; IMPL-40b claim same-Backer-both-slots → 6005 | covered |
| 3 | reinitialization | `init` for Pool/Position (nonce tombstone), `init_if_needed` only on User/UserRound/Backer (never closed); fresh-account checks for pages/CloseRecord/config; version PDA exists → fail | M4 provisioning rows (`provision…already_exists`), M5 `same_nonce_cannot_open_a_second_position_after_settlement`, M3 staging `version exists` | covered |
| 4 | arithmetic overflow | `overflow-checks = true` (release); checked helpers; U256 for weights; `payout_props` property tests | unit tests on every helper; `tests/payout_props.rs` (8); M6 `discovery_accumulates_a_remainders_above_u128_in_u256` | covered |
| 5 | replay of permissionless instructions | ticket id + cursor gates; close state machine; idempotent finalize/observe/reconcile; delivery zeroes | M6 `replaying_a_completed_discover_chunk_fails_without_credit`, M7 `two_partial_provider_returns_then_replay…`, `finalize…second_call_no_op`, M8 `deliver_owner_refund` NothingDeferred, observe AlreadyObserved | covered |
| 6 | double claims | `take_funded` + `funded_claims` checked_sub; `book_delivery` zeroes; management bound | M6 `claim_during_ticket_pays_only_finalized_and_commit_adds_once`, `claim_never_restores_cap_headroom…`; M7 provider replay; owner_refund rows | covered |
| 7 | barrier bypass (closing / A funding / payout) | `payout_gate`, `open_gate`, `series_a_gate`, activation gate, ticket start gate | M6 `normal_deposit_allowed_during_nonclosing_ticket_but_a_funding_and_payout_wait`, M5/M7 closing rows, M3 activation rows; **new** IMPL-40b `position_keeps_its_pinned_config_across_a_later_activation`, `exit_paths_work_while_paused`, `payout_with_registry_beyond_the_snapshot_pages_is_refused_at_runtime` | covered |
| 8 | destination substitution to protected accounts | `validate_destination` + per-instruction protected lists (vault, OpsCash, Pool, pages, CloseRecord) | M5 payout protected-destination rows, M7 `wrong_destination`, M8 cleanup recorded-key rows; IMPL-40b claim protected destinations → 6008 | covered |
| 9 | third-party fee payer authority | owner-signed user instructions carry `has_one`/seeds on the owner; keeper instructions carry no signer authority; `request_round_draw.payer` is fee-only | M3/M5 rows with a foreign payer; close suite drives every keeper step from unprivileged payers | covered |
| 10 | config activation races | adjacent version + `expected_current`; `effective_round_id`; idle/no-ticket; positions pin `config_version`; `StaleConfig` on open | M3 `activate…` rows, M5 `expected_config` rows; IMPL-40b T5 pinned config across a later activation | covered (T5 added) |
| 11 | late / duplicate ORAO fulfilment | capture gates (terminal-first, deadline), `AlreadyObserved`, request binding | M7 `late_authentic_fulfillment_after_skip_cannot_mint_or_revive`, `fulfill_with_wrong_authority`, observe rows | covered |
| 12 | expiry vs capture at the deadline | `now <= deadline` capture / `now > deadline` expire; captured draw not expirable | M7 `capture_at_deadline_equality_is_timely_and_capture_one_second_later_is_ignored`, expire rows | covered |
| 13 | sponsor withdrawal vs locked reserve | free-balance cap + `cash >= rent + locked + amount`; `locked_permission` requires `!reserve_settled` (G1) | M8 `sponsor_withdrawal_cannot_touch_locked` (8 variants), row 20 G1 runtime proof | covered |
| 14 | cleanup predicates | `closable`, canonical record, recorded refund key, Registry/Snapshot never closable | M8 cleanup rows (`close_snapshot_and_registry_pages_always_refused`, immutable destination rollback M8-F1); IMPL-40b `caller_funded_snapshot_page_is_not_closable` | covered |
| 15 | CU exhaustion on worst-case rows | every loop capped (batch 1..=8, inline ≤ 8, Squads members == 3); measured in `measurements-2026-10.md` (worst chunk 57,410 CU at N = 256; inline 147,005 CU at N = 8) | M9 measure suite; `tests/inline_alloc.rs`; IMPL-40b inline N = 8 all capped | covered by execution; governance CPIs through the real Squads program are measured only at M14 (documented) |
| 16 | heap use in loops | no allocation inside per-row loops (pre-sized Vecs outside; one realloc in prune; one 5-element vec in claim) | static review (security review row 16); 32 KiB heap never exceeded in 9,265 CLI observations | documented reason: no heap watermark instrumented locally |

## 5. Accepted limitations (decided, documented, no mechanism added)

- **M7-F1** pre-funded request PDA collision fails closed (griefing cost: local estimate).
- **M8-F1** cleanup rolls back until the immutable owner destination is fundable (no loss).
- **M8-F2** candidate batches must stay inside participant AND candidate page edges (keeper rule).
- **M9-F1 / SEC-7 / ECO-4** `claim_backer` has no deliverability preflight; a cluster refusal
  reverts atomically and the credit stays claimable (clients preflight with `credit_is_deliverable`).
- **ECO-5** candidacy needs only `loss > 0`; dust losses can create permanent Series B rows.
- **ECO-1 general case** any toll ≥ 5 starts a ticket (6..387 keeper transactions, `AllocationPending`
  until finished): keeper priority + one-row fallback (M11); no minimum toll, cooldown or cap.
- **LiteSVM rent gap**: data-less destination credits below the exempt minimum succeed locally;
  the cluster rule (Agave `transition_allowed`) is pinned by `tests/transfer_constraints.rs`.

## 6. Lamports with no owner and no claim (ECO-6 inventory)

Anonymous credits to the vault or OpsCash (floating `excess` / `unallocated_excess`, never
spendable); rent-floor deltas (now floating, ECO-3); prefunded page remainders; the genesis
OpsCash prefund; caller-funded permanent Registry/Snapshot page rent (`NotClosable`); donations
to a request account (cut `eligible` 1:1); ORAO-retained request rent and the consumed oracle
fee; the permanent `CloseRecord` rent (DR-02). Nothing owned by a user or sponsor lands in these
buckets; owner refunds defer in `locked_owner_remaining` and are never converted.

## 7. Repairs in this milestone and decisions returned

Repaired with failing tests first (details and RED→GREEN pins in the triage document and
`evidence/2026-10-14-m10-fixes.json`): SEC-1/ECO-2 Carried Interest table pinned at staging;
SEC-3/ECO-1 dust tolls seed directly (host parity, plan L647 zero-slice shortcut); ECO-3 floating
excess; SEC-4 genesis provider client == derived ORAO Client PDA; SEC-5 full governance check on
`withdraw_provider_return`; SEC-6 comment; SEC-8 CI stack-offset and IDL checks.
Adjudicated by Codex (not applied, documented): SEC-2 governance-tunable bounds stay prospective
by design (DR-04; only fixed-helper values are restate-only pinned; every future G economics
proposal needs an explicit review/preflight), and the `effective_round_id` mis-staging hazard is
accepted with a G preflight/runbook check rather than an on-chain horizon or re-stage.

## 8. Residual risks

The closing position's pinned config sets the close terms; B-promotion depends on keeper throughput
inside the 600 s window; no emergency stop for outflows (by design — exits must never be
blockable); ORAO and G trust while G holds the upgrade authority (spec L249); rent-schedule
changes (DR-27, now absorbed within classified excess); a staged provider tuple is validated only
at request time; the spec's "accepted bounds" for prospective economics are unspecified, so a
mis-staged (or unreachable `effective_round_id`) version is caught only by G's off-chain preflight; `increase_provider_budget` is permissionless (DR-33); keeper economics under an
adversarial payout rate are not in the ICP budget yet (M11).
