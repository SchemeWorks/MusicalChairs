# Native Solana — M7 contract supplement (2026-10-14)

Scope: the three items from Codex's independent M7 static review at `8fdade5`. Item 1 is a wording clarification of the plan; items 2 and 3 record the two bounded program corrections (kanban IMPL-42 and IMPL-43) and, for item 3, the narrow deferred-refund mechanism that is **proposed behaviour pending Codex review**. Nothing here reopens the accepted architecture (D1–D9), the decision register or the economics; no custody or admin override, no new fee, no population cap. Local work only.

## 1. Canonical candidate digest (clarifies plan M7 step 1, L583)

The plan's test sketch says the sealed digest "equals SHA-256 over the ordered pubkeys". The implemented and canonical digest (`solana/programs/musical_chairs/src/sampling.rs`, `candidate_digest_start` / `candidate_digest_next`) is a **chain over (owner, loss) in participant order**:

```
D_0     = SHA256("MC/EEC/CANDIDATES/v1" || set_tag_u8)
D_{i+1} = SHA256(D_i || owner_i (32 bytes) || loss_i (u128, little-endian, 16 bytes))
```

The sealed digest of the selected set is `D_n`; it is the `sealed_digest_32` input of the protocol seed (spec §4) and is pinned by `solana/fixtures/sampling.json`. Binding each candidate's loss figure into the chain is deliberate: the sealed set then commits to the amounts the promotion entitlement is computed from, not only to the identities. The plan wording "ordered pubkeys" is superseded by this definition; no behaviour changes.

## 2. `deadline_seconds` is frozen at 600 s (IMPL-42)

Spec §4 (L220): `promotion_deadline = close_started_at + 600`, pinned once at the closing payout, never reset. The closing payout copies `ConfigVersion.deadline_seconds` verbatim, so before IMPL-42 a staged value of 1 would have re-timed every close to a one-second window. `ConfigTerms::validate` now requires `deadline_seconds == PROMOTION_DEADLINE_SECS` (600; `state.rs`) and rejects anything else with `InvalidState`, fee-only, like the quota and the five fixed-helper pins before it.

Evidence (receipt `docs/solana/evidence/2026-10-14-m7-deadline-pin.json`): unit cases 599 and 601 RED on the old validator ("validator accepted the terms") → GREEN; harness staging variants `noncanonical_deadline_short` / `_long` (G-signed through the Squads stand-in) RED on the old ELF `27de0cfc…` (staging accepted) → GREEN on the patched ELF; the positions suite's closing row asserts `promotion_deadline == close_started_at + 600` exactly.

## 3. Undeliverable owner refunds are deferred, never forfeited (IMPL-43) — proposed, pending Codex review

### The defect

`close::settle_books` (settlement at terminal promotion), `book_return` and `withdraw_provider_return` treated an owner remainder that the runtime could not deliver to the immutable `owner_refund_destination` as forfeited: the lamports were moved into `ops.unallocated_excess` and the owner liability zeroed. Trigger: a zero-pot (net-0) closing payout to a fresh destination with an owner reserve contribution below the rent-exempt minimum, or a destination that fails `validate_destination`. This violates spec §8.5 ("`locked_owner_remaining` is transferred … to the recorded owner destination") and DR-14's owner-attributed return. Captured RED (`2026-10-14-m7-owner-refund.json`): after `expire_promotion` the owner's 500,000 lamports were neither paid nor owed ("owed 0, paid 0").

### The runtime rule, and what the local runtime can prove

Agave's rent-state rule (`svm/src/rent_calculator.rs`, copied verbatim into LiteSVM 0.7.1 `src/utils/rent.rs::transition_allowed`, commit `6fbbaf67`): a writable account may end a transaction rent-paying (0 < lamports < minimum) only if it was rent-paying before, kept its data size and was **not credited**. For a data-less destination holding `c` lamports, a credit of `a > 0` therefore succeeds iff `c + a >= minimum(0)`. The old predicate `c > 0 || a >= minimum` wrongly called a legacy rent-paying destination deliverable; on the cluster that transfer fails and, because settlement runs inside `expire_promotion` / `apply_promotion` / `build_candidates`, it would have blocked round recovery.

LiteSVM 0.7.1 applies the check only to accounts **with data** (`check_accounts_rent` skips `account.data().is_empty()`), so system wallets are never checked locally: `tests/transfer_constraints.rs` shows every probe succeeding in LiteSVM although the cluster rejects three of them. The predicate is therefore validated against the rule text (restated in the test and compared on a dense grid), not against local execution. This limit is recorded in the receipt and in the test's module docs; a LiteSVM upgrade that closes the gap flips the pinning test.

### The correction (smallest contract-preserving change)

1. `credit_is_deliverable(c, a, min) := c + a >= min` (saturating). Conservative under the rule: when it is false the credit is deferred, never attempted.
2. `settle_books`: the sponsor remainder returns to `free_balance` as before; the owner remainder is **paid when deliverable, otherwise it stays in `CloseRecord.locked_owner_remaining`** (held in OpsCash, still counted in `ops.locked_remaining_total`). `reserve_settled` is set either way, so `finalize_round` and every claim path proceed. Nothing is ever added to `unallocated_excess` from owner lamports.
3. `book_return` / `withdraw_provider_return`: an undeliverable owner share of a provider return is re-locked into the close's owner partition (`locked_owner_remaining += w`, `locked_remaining_total += w`); `owner_request_rent_refundable_remaining` is still decremented, so the share is accounted exactly once. Before settlement it funds owner-attributed work like the rest of the partition; after settlement it is the deferred refund.
4. New permissionless instruction `deliver_owner_refund()` (accounts: `close_record`, `ops`, `ops_cash`, `owner_refund`, `system_program`; event-CPI): requires `reserve_settled`; pays the whole `locked_owner_remaining` to the **same immutable destination** once `credit_is_deliverable` holds; `RefundNotDeliverable` (6051, fee-only) while it does not; `NothingDeferred` (6050) on replay or when nothing is owed; `WrongAccount` for any other destination. Emits `OwnerRefundDelivered { round_id, close_id, amount, destination }` (event 20 of the frozen schema; fixture regenerated).

Accounting: the OpsCash identity `lamports − protected_rent == free + L + unallocated_excess` is unchanged (the deferred lamports are inside `L`). The invariant "`reserve_settled ∧ locked_owner_remaining > 0` ⇔ a deferred owner refund is owed" is the single source of truth; delivery zeroes it exactly once. Conservation-table rows (spec §8.6) affected: settlement gains an *undeliverable* variant (OpsCash 0, free +s, s → 0, o unchanged, L −s); a new row `deliver_owner_refund` (OpsCash −o, L −o, o → 0); `withdraw_provider_return` with an undeliverable owner share w (OpsCash +amount, free +(amount − w), o +w, L +w).

What it is not: no admin or sponsor can redirect, sweep or spend a deferred refund (the destination is immutable and the partition is excluded from `free_balance`); no fee; no change to admission, payouts, allocation or the promotion economics. Limit: if the destination becomes permanently unable to receive lamports (for example, assigned to another program by its own authority), the refund stays locked to the owner indefinitely. That is the price of destination immutability, which the accepted contract requires; the alternative (any redirect authority) is exactly what Codex excluded.

Evidence (`2026-10-14-m7-owner-refund.json`): RED on the IMPL-32 ELF; unit tests inverted RED → GREEN (settlement, provider return, delivery, predicate table); LiteSVM rows `fresh_destination` (deferred → `RefundNotDeliverable` → funded → delivered once → `NothingDeferred`; wrong destination `WrongAccount`), `legacy_rent_paying_destination` (expiry not blocked, deferred, delivered), `deliverable_destination` (paid at settlement, nothing deferred), `pending_close` (`InvalidState`); every row asserts fee-only rollback on rejection and the OpsCash identity after each step.

Open for Codex: (a) accept "settled close with `locked_owner_remaining > 0` = deferred owner refund" as the contract reading of §8.5, or ask for a dedicated field (schema change to the frozen `CloseRecord` layout; not done here); (b) whether `deliver_owner_refund` should remain permissionless (recommended: the destination is fixed, so a keeper can only help).
