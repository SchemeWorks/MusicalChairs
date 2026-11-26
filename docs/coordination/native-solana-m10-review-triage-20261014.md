# Native Solana M10 — review triage, applied repairs and returned decisions (2026-10-14)

Coordinator triage of the two IMPL-40 persona reviews, published verbatim as
[`docs/solana/evidence/2026-10-14-m10-security-review.md`](../solana/evidence/2026-10-14-m10-security-review.md)
(SEC-1..8, checklist, test gaps T1–T12) and
[`docs/solana/evidence/2026-10-14-m10-economy-audit.md`](../solana/evidence/2026-10-14-m10-economy-audit.md)
(ECO-1..6, invariant ledger, tests T1–T10). Both ran read-only against commit `3c3003c`
(program sources identical through `37185ea`). Neither found a Critical or High issue or a
conservation break. Codex adjudicates the returned decisions from this file and the two
reports; nothing here asks Rob for a routine-repair approval.

Spec = `docs/superpowers/specs/2026-10-13-native-solana-reconciled-spec.md`; plan =
`docs/superpowers/plans/2026-10-13-native-solana-implementation.md`.

## 1. Applied in this tranche (`fix(program): security review findings`, failing test first)

Every repair kept the frozen contract: no ABI, SPACE, event or error-code change; the IDL is
byte-identical. RED was taken on the pre-fix ELF `d77b23bf…` (LiteSVM rows) or the pre-fix
source (unit rows); GREEN on the rebuilt ELF is in `docs/solana/evidence/2026-10-14-m10-fixes.json`.

| id | finding | repair | RED → GREEN |
| --- | --- | --- | --- |
| SEC-1 = ECO-2 | `ConfigTerms::validate` accepted a Carried Interest table other than the economics crate's (1,200/750/300 bps, day 7/14; spec L187 "Preserve") while `positions::simple_toll` refuses any other tier at payout: every Simple position opened under such a version became unclaimable (`InvalidState`), with no rescue because positions pin their config (spec L248). | `validate` pins `simple_toll_bps == [fees::simple_toll_bps(0), (7·DAY), (14·DAY)]` and `simple_toll_day_bounds == [7, 14]` (restate-only, like the IMPL-41/42 pins). | unit `validate_pins_the_simple_toll_table_and_day_bounds`; staging variants `noncanonical_toll_table` / `noncanonical_toll_bounds` on `stage_config_requires_g_and_future_round` ("unexpectedly succeeded" on `d77b23bf`). |
| SEC-3 = ECO-1 (dust subset) | `toll_route` started a full allocation ticket for ANY positive toll with a non-empty registry. The host reference model `Book::allocate` (economics crate) seeds a toll whose three Backer slices of the half all floor to zero (toll ≤ 4 at 35/45/20) with no ticket; plan L647 keeps "the zero-toll and zero-slice shortcuts" inline regardless. The pre-fix ticket credited nobody and seeded the whole toll (recorded: `toll=3 seed_delta=3 funded_claims_delta=0 backers_unchanged=true`), at 6..387 keeper transactions and an `AllocationPending` barrier per lamport-sized claim. | `toll_route(toll, registry_len, (senior, a, b))` mirrors the host rule with the ticket's own tiers: all slices zero → `Seed`. Identical end state; no economics change. | unit `dust_toll_routes_to_seed_like_the_host_book` (1..=64 vs the host rule); LiteSVM `a_toll_whose_slices_all_floor_to_zero_goes_to_the_seed_without_a_ticket` (toll 3 → seed, no ticket, `ticket_id == 0`) and `a_toll_of_five_or_more_still_starts_the_ticket` (toll 6). |
| ECO-3 | `sync_game`/`sync_ops` accumulated a rent-floor DECREASE into `excess` and then, because `recorded` included that excess, refused every instruction (`InvalidState` / `Overflow`) once the floor rose again — a liveness freeze of the whole program on a rent-schedule reversal. | `excess` / `unallocated_excess` float: re-derived every sync as the residual over the OWNED partitions at the live floor. Nothing can spend excess, so the economics are unchanged; a shortfall against owned partitions fails exactly as before. | LiteSVM `a_rent_floor_rise_within_the_classified_excess_does_not_freeze_the_vault` (open under 1/10 floor, restore, open again: `Custom(6001)` on `d77b23bf`). |
| SEC-4 | `initialize` wrote `RngAuthority.client = args.provider_client` unvalidated; `request_round_draw` compares against it forever and `RngAuthority` is never rewritten, so a wrong genesis tuple disabled the draw until a program upgrade (fail closed, funds safe). | `provider::expected_client()` (= ORAO `Client::find_address(program, rng_authority, orao)`); `initialize` requires `args.provider_client == expected_client()` → `WrongProviderTuple`. Harness genesis fixtures now use the derived address. | `initialize_rejects_one_of_three_profile.wrong_provider_client` (fresh rig, canonical G profile, `key(0xE4)`). |
| SEC-5 | `withdraw_provider_return` checked only the vault signer + `pool.governance`, not the Squads profile every other G instruction checks via `require_governance`. | `require_governance` (now `pub(crate)`). Side effect: a rogue multisig key with no Squads account is now `WrongAccount` (6005, as on every other G instruction) instead of `NotAuthorized`. | `withdraw_rejects_…degraded_g_profile` (threshold-1 G profile → `WrongGovernanceProfile`; the honest call "unexpectedly succeeded" on `d77b23bf`). |
| SEC-6 | `error.rs` range comment stopped at 6049. | Comment names 6050/6051 (IMPL-43). | — |
| SEC-8 | CI never asserted the SBF build log is free of `Stack offset` warnings and never ran `solana/scripts/check-idl.sh`. | `production-sbf` job: `grep -c 'Stack offset' == 0` on the build log; IDL regeneration diff against the committed IDL. | CI run of the fix commit. |

## 2. Accepted and documented (no code change)

- **SEC-7 = ECO-4 = M9-F1** (decided by Codex): `claim_backer` has no deliverability
  preflight. The owner signs and chooses the destination; a cluster rent-rule refusal
  reverts the whole transaction including the `funded` take, so the credit stays claimable
  to a fundable destination. Same accepted limitation as page cleanup (M8-F1). Clients
  preflight with `credit_is_deliverable`. Local LiteSVM does not enforce the rule for
  data-less accounts (`tests/transfer_constraints.rs`).
- **ECO-5**: candidacy needs only `loss > 0`; a 12-lamport loss can become a 13-lamport
  Series B row (`loss·116/100 > 10`) at ~4.98 M lamports of permanent rent. Accepted game
  design; Codex: no loss floor.
- **ECO-6**: inventory of lamports with no owner and no claim (anonymous credits, rent-floor
  deltas — now floating per ECO-3 —, prefunded page remainders, genesis OpsCash prefund,
  caller-funded permanent Registry/Snapshot pages, request-account donations, ORAO-retained
  request rent and the consumed fee). Nothing owned by a user or sponsor lands there; owner
  refunds defer in `locked_owner_remaining`. Carried into `docs/solana/threat-model.md`.
- **ECO-1 general case** (toll ≥ 5 starts a ticket; one 5,000-lamport claim costs the
  community 6..387 keeper transactions and holds `AllocationPending` until the ticket
  finishes): Codex keeps the accepted economics — **no minimum ticket-bearing toll, no
  cooldown**. Documented as a keeper requirement for M11: ticket completion is the keeper's
  top priority, one-row fallback always available, and the budget must model an adversarial
  payout rate (the ICP budget does not today). Anyone can progress a ticket, so it is a cost
  and ordering effect, never a lock.
- **Residual risks** listed by the security review (closing position's pinned config sets the
  close terms; B-promotion depends on keeper throughput inside the 600 s window; no emergency
  stop for outflows by design; ORAO and G trust while G holds the upgrade authority; rent
  schedule DR-27; `increase_provider_budget` permissionless per DR-33; staged provider tuple
  validated only at request time) are carried into the threat model unchanged.

## 3. Returned to Codex (contract/integration decisions; not applied)

Each case: what the program accepts today, where the spec freezes or leaves it open, an exact
reproducer, the minimal recommendation, and Codex's adjudication (2026-10-14). Reproducers use the harness `Rig::governed()`
fixture with `stage_ix(&rig.rt, g_multisig, g_vault, 2, terms)` through `via_g` (same
builders as `stage_config_requires_g_and_future_round`); "accepted" means the staging
transaction succeeds on the current ELF and the version is then activatable at round 2.

### R-1 Stageable economics are almost unbounded (SEC-2; ECO-2 "related governance surface")

| term | validated today (`controls.rs` `ConfigTerms::validate`) | frozen in the spec | reproducer (terms = canonical v1, one field changed) | effect if activated | minimal recommendation |
| --- | --- | --- | --- | --- | --- |
| `plan_durations_s` (Simple/Compound15/Compound30) | each `> 0` | L124–125: 15 days / 30 days | `terms.plan_durations_s[1] = 1` → accepted | a Compound15 settles 4.47× its deposit one second after opening; colluding positions drain the pot | restate-only pins `[?, 15·DAY, 30·DAY]` (Simple duration as the spec fixes it) |
| `compound_factors_q18` | none | L124–125: `ROI15_Q18 = 4_473_565_759_257_037_859`, `ROI30_Q18 = 12_267_678_469_131_246_220` | `terms.compound_factors_q18 = [u64::MAX, u64::MAX]` → accepted | owed = `floor(P·factor/10^18)` ≈ 18,446·P | restate-only pins |
| `front_end_load_bps` | `≤ 10_000` | L88: 400 (4 %), L186 "Preserve" | `terms.front_end_load_bps = 10_000` → accepted | every deposit goes to management, pot credit 0 | restate-only pin 400, or a cap if Codex wants it tunable |
| `jackpot_bps` | each `≤ 10_000` | L124–125: 900 / 1,300; L187 "Preserve" | `terms.jackpot_bps = [10_000, 10_000]` → accepted | compounding settlement pays net 0 | restate-only pins |
| `senior_bps / a_bps / b_bps` | sum `== 10_000` | L145: 35/45/20; L188 "Preserve" | `terms.senior_bps = 10_000; a_bps = 0; b_bps = 0` → accepted; the ticket pins them (`positions.rs` ticket start) and `fold_slices_with_bps` applies them | the whole Backer half goes to the oldest A; the host model (hard-coded 35/45/20) and the program diverge | restate-only pins 3,500/4,500/2,000 (parity with the host model) |
| `min_game`, `min_a` | `> 0` | L91–92: 10,000,000 / 50,000,000 | `terms.min_game = 1` → accepted | 1-lamport positions; quota is the only admission brake | restate-only pins, or floor at the spec values |
| `provider_request_fee_lamports`, `provider_fee_headroom_bps` | headroom `≤ 10_000` | spec §4 / DR-33 (fee cap approved at the closing payout) | — (validated at request time, `ProviderFeeExceeded`) | none beyond today's cap | none |

Context: spec L296 says "terms within accepted bounds" without listing them; L243/L249 note
that G (2-of-3, 172,800 s delay) can also upgrade arbitrary code after the delay, so every
pin above protects against a mis-staged configuration, not against a hostile G. The security
review rates this Medium **if the upgrade authority is ever revoked**, Low while G holds it.
Recommendation: restate-only pins for everything the spec marks "Preserve" or fixes
numerically (durations, factors, load, jackpot, split, minimums), exactly like the M5/M6/M7
pins; none of them changes the launch terms. Decide before any authority revoke.

**Codex adjudication (2026-10-14): NOT pinned.** Retain the canonical genesis `default_v1` and
the fixed-helper restate-only pins (Simple rate, A/B entitlement, seed half, cutoff, quota,
deadline, page capacity and — this tranche — the Carried Interest table, all of them consumed
by fixed helpers that would otherwise ignore or refuse the staged value). Do **not**
unconditionally pin future durations, compound factors, load, jackpot, split or minimums:
DR-04 (decision register L51–53), spec L248 and L296–297 and the existing
accepts-later-variation staging test intentionally support genuinely consumed prospective
`ConfigVersion` terms. The exact launch economics are canonical v1; no non-v1 economic
schedule is authorized by this supervision. Documented risk: the spec leaves "accepted bounds"
(L296) unspecified, so any future G economics proposal needs an explicit review and preflight
(simulate the staged terms against the economics crate, publish the delta, respect the 48 h
disclosure) before staging; blanket pins remain optional future governance scope. Provider cost
inputs (`provider_request_fee_lamports`, headroom) stay tunable.

### R-2 `effective_round_id` staging deadlock (SEC-2)

- Today: `stage_config_version` requires `version == active + 1` (a different version fails;
  re-staging the same version fails because the PDA exists) and
  `effective_round_id >= pool.round_id + 1`; `activate_config_version` requires
  `pool.round_id >= effective_round_id`. There is no cancel and no upper bound.
- Reproducer: `terms.effective_round_id = u64::MAX`; `stage_ix(.., 2, terms)` → accepted.
  Then `stage_ix(.., 2, canonical)` → fails (account exists); `stage_ix(.., 3, canonical)` →
  `ConfigNotActivatable` (not `active + 1`); `activate(2)` → `ConfigNotActivatable` forever.
  Configuration is frozen at v1 for the life of the program (funds and play unaffected).
- Minimal recommendation (pick one): (a) a horizon `effective_round_id <= pool.round_id + H`
  (e.g. H = 1,000 rounds) — a new rule but no new instruction; (b) allow G to **re-stage an
  unactivated version in place** (overwrite the PDA's terms, reset `staged_at`), keeping
  append-only semantics after activation — restores recoverability without a cancel
  instruction; the 48 h disclosure delay restarts from the new `staged_at`. (b) is the
  smaller contract change if "a staged version is immutable" was never a promise; (a) if it was.

**Codex adjudication (2026-10-14): accepted and documented, no on-chain gate.** The exact
`u64::MAX` (or any unreachable) `effective_round_id` mis-staging stalls the future config
chain; current play, funds and claims remain available. Spec L296 explicitly says an existing
version fails and the M3 definition of done promises no re-stage, cancel or horizon, so an
arbitrary horizon or in-place re-stage is deferred as a future contract amendment. Mitigation:
G preflight and runbook checks before `stage_config_version` (effective round within the
intended horizon, version == active + 1, terms validated off-chain against `ConfigTerms::validate`
and the economics crate). Carried into `docs/solana/operations.md` (M13).

### R-3 Series B existence from dust losses (ECO-5) — accepted, listed for completeness

Decided: no loss floor. Recorded here so the adjudication trail is complete.

### R-4 Keeper requirements carried to M11 (no program change)

From ECO-1 general case, M8-F2 and the security review's residual list: ticket completion
first; batches aligned with participant AND both candidate page edges, one-row fallback;
budget for an adversarial payout rate; a fulfilled-but-uncaptured request is expirable at
`deadline + 1`; preflight `credit_is_deliverable` on every user-chosen destination.

## 4. Test-gap rows (IMPL-40b, no program change)

The security review's T1–T7 and the economy audit's T5/T6/T8/T10 are being added as
`financial.review.*` rows (`src/review_gaps.rs`): wrong `owner_refund` while an owner
remainder is locked (highest value), `claim_backer` substitutions, unprovisioned/wrong pages
on open and A funding, registry beyond snapshot pages at runtime, pinned config across a later
activation, exit paths while paused, wrong `ops_cash` / config PDA, per-Backer `funded` sum
== `funded_claims` after every fuzz step, inline N = 8 all capped, dust-loss candidate cutoff,
caller-funded Snapshot page `NotClosable`.
