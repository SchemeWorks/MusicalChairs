# Native SOL round state machine

This document describes the host reference model in
`solana/crates/economics/src/rounds.rs`. It is an economic and state-transition
model for the accepted first-release contract. It is not an Anchor program, an
SVM execution proof, a provider integration, a signing flow, or evidence that
any live account exists.

## Close trigger and current-round accounting

The model starts each round in `Active`. Opening a position appends its owner to
the current round's canonical participant map and records gross deposit. The
accepted front-end load is credited to management, while only the validated pot
credit enters the round pot. Every position payout adds only the actually paid
net amount to that owner's current round record. Claim toll is held in a
separate pending escrow and blocks later payouts until a validated allocation
commit partitions it. The aggregate loss is checked as

```text
loss = max(gross_deposit - net_payout, 0)
```

This includes positions that have already been claimed or matured. A winning
owner therefore has zero loss and cannot become a promotion candidate.

Each opened position stores a pinned `earnings::Plan`; Simple positions also
store `SimpleAccrual` so repeated claims derive owed earnings from execution
time and carry the integer remainder. Compound claims derive maturity from the
plan and cannot accept a caller maturity flag or arbitrary owed amount.
Simple claims remain open for repeated claims before their pinned maturity. A
Simple settlement at maturity, including an explicit `close_position`,
terminates that position even if its remaining owed amount is zero;
`close_position` is unavailable for Compound positions. Round closure is
triggered only by strict insolvency (`owed > pot`); maturity, an explicit
position close, and a solvent Compound claim do not close the round. A
minimum-receive failure is evaluated on a cloned state, so it leaves the
position, pot, phase, and close counter unchanged.

The model keeps historical position records after a round advances and tags
each record with its opening round. A later round cannot claim a stale position;
this represents phase/round validity closing old positions without requiring a
history scan or an update to every old account. Persistent A/B existence flags
also survive round advancement and capped/closed membership.

Series A funding enters through `Round::fund_series_a` with the same rolling
admission ledger used for game openings. The host operation requires matching
Round/Backer pot, pending-toll, and management partitions; it adds 100% of the
cash to both pots, grants the Book's pinned 124% Series A entitlement, and
marks persistent A existence. It has no Front-End Load and does not add to a
game participant's deposit or loss record. Admission pauses, closing rounds,
pending toll, and an active Book allocation ticket reject it atomically.
`record_backer_exists_fixture` is retained only to construct host fixtures for
historical A/B state; production-shaped funding uses the canonical Series A
operation, while a successful promotion itself records persistent B history.

## Phases and ordering

```text
Active
  -> AllocatingClosingToll
  -> CountingCandidates
  -> AwaitingRandomness
  -> PromotionApplied
  -> ResetReady (conceptual terminal cash-ready state)
  -> Active(next round)

AwaitingRandomness -> PromotionSkipped(Deadline)
CountingCandidates -> PromotionSkipped(NoCandidates)
AwaitingRandomness -> PromotionSkipped(EntropyExhausted)
```

Deadline expiration is also callable during `AllocatingClosingToll` and
`CountingCandidates`; it abandons only candidate/draw work and preserves the
cash-commit requirement before finalization. Per-owner game admission applies
the pinned minimum and rolling three-call hourly quota before any pot or
participant mutation.

`commit_allocation(seed_credit, funded_credit)` is the cash barrier for a
closing round. Its two credits must sum exactly to pending toll escrow; it
cannot manufacture seed or silently absorb a shortfall. Candidate lists and a
draw can be prepared only after this barrier, and `finalize_round` requires
both committed cash and a terminal promotion result. The complete seed reserve
is carried exactly once during finalization; promotion itself creates or merges
the selected owner's `floor(loss * 11_600 / 10_000)` B entitlement and adds no
cash.
Duplicate finalization of the same `close_id` returns the original next-round
result without changing the round again. A stale close ID is rejected.

An admission pause rejects new positions while leaving cash commit, candidate
progress, expiration, finalization, and already-authorized claims available.
The composed `expire_finalize_open(close_id, expected_round_id, execution_now,
OpenPositionArgs)` helper performs all checks on a clone, applies a timely
captured result before finalization, and opens only the expected next round
after the cash barrier. `OpenPositionArgs` contains the owner, position ID,
gross deposit, and plan kind; it has no independent opening timestamp, so the
new position uses the single instruction execution time. It cannot deposit
while cash is still pending: an incomplete allocation makes the composed
operation roll back, while standalone expiration can persist its deadline skip
until cash commit later permits finalization.

## Canonical participants and candidates

Participant rows are keyed and ordered by owner, which supplies one stable
program-owned traversal order in the host model. `participant_count`,
`participant_chunk`, and `participant_digest` expose the count, bounded slices,
and a SHA-256 digest over the complete canonical rows. Each row binds owner,
gross, net, loss, and the persistent A/B existence bits. Candidate sealing
processes every row once into two immutable lists:

* Preferred: `loss > 0` and neither A nor B has ever existed for the owner in
  this native game, including closed or capped history.
* Fallback: every owner with `loss > 0`.

Preferred is selected when nonempty; otherwise the fallback list is selected.
Winning owners are absent from both lists. The selected set tag (`0` preferred,
`1` fallback), count, and set digest are frozen before any draw request. The
host model's BTreeMap prevents duplicate owner rows; a deployed program must
obtain the same guarantee from page ownership, cursor checks, and a complete
traversal proof.

## One bound draw and deadline

At close acceptance, the model stores one immutable `close_id`,
`close_started_at`, and `promotion_deadline = close_started_at + 600`. Retries,
candidate processing, provider failures, fee funding, and keeper replacement do
not reset the deadline. Request creation and first capture require
execution-time `Clock <= deadline`, while a retry of an already marked request
remains idempotent. A timely capture remains stored and may be applied after
the deadline, so expiration cannot overwrite it. An authenticated late result
is a no-op, including a backdated input after a terminal deadline skip. A
result with the wrong provider owner, client, config, request address, or seed
is rejected.

The single protocol seed is exactly the digest of these fixed-width fields:

```text
SHA256(
  ASCII("SchemeWorks/MC/EEC/v1") ||
  cluster_domain_32 || native_program_id_32 ||
  round_id_u64_le || close_id_u64_le ||
  selected_set_tag_u8 || selected_count_u64_le ||
  sealed_candidate_digest_32
)
```

The request marker is written only together with a successful simulated provider
creation. A failed creation leaves no marker. An unexpected pre-existing
request account causes a collision error; the model never substitutes a new
seed or alternate request. The request binding stores the full provider tuple
and computes a deterministic SHA-256 request-address stand-in from the
registered client and protocol seed. That address is a host-model placeholder
for future provider SDK/PDA derivation; it is not a real PDA, account-ownership,
CPI-authentication, or cryptographic VRF proof. The provider probe preserves
the full `[u8; 64]` fulfilled output; the model does not truncate it to 32
bytes.

## Bounded unbiased ordinal mapping

The captured output is expanded with at most 64 counters. For counter `c`, the
model hashes

```text
SHA256(ASCII("MC/EEC/INDEX/v1") || output_64 || round_id_u64_le || c_u64_le)
```

and interprets the first eight bytes as a little-endian `u64`. With `N`
candidates it computes `limit = floor(2^64 / N) * N`, accepting the first
sample `r < limit` and selecting `r % N`. It never rerolls with a new output or
skips an accepted sample. If all counters fail, promotion terminalizes as
`PromotionSkipped(EntropyExhausted)`. An empty selected set skips as
`PromotionSkipped(NoCandidates)` without requesting randomness.

The deterministic mapping helper used to exercise rejection boundaries is
kept separate from production state transitions and is compiled only for unit
tests. No public instruction accepts caller-chosen entropy.

## Availability and custody boundary

The deadline removes a mandatory provider wait; it does not erase pending cash
allocation, funded claims, or the seed reserve. Candidate-only work may be
discarded after expiry, while financial state remains protected until cash is
committed. The model has no keeper lease, no operator-only timeout, no
caller-selected winner, and no funds movement. Solana concepts that remain for
the future implementation are separate program/data accounts, PDA ownership
constraints, CPI signer authentication, and provider result retention. Those
must be proved with matched local SVM/provider probes before production
instructions are accepted.
