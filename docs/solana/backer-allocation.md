# Native SOL Backer allocation model

This document describes the host reference model in
`solana/crates/economics/src/backers.rs`. It is an integer oracle and state
transition probe for the native-SOL design. It does not build an Anchor
program, access a wallet, sign or broadcast a transaction, or establish a
deployed account layout.

The model uses one persistent row per `(owner, series)`. A row records its
`u128` entitlement and lifetime credit, its checked `u64` funded and pending
claim balances, its immutable first-funding sequence, and its optional slot in
the dense active registry. Permanent A/B existence history is retained after a
row leaves the registry. Reopening appends the row while retaining its first
sequence, so senior order is deterministic and does not depend on a clock.

## Integer oracle

For a toll `T`, the model computes `seed0 = floor(T / 2)`, then floors the
remaining half into senior/A/B slices at 35/45/20 percent. The percentage
rounding remainder goes to seed. Membership is captured before senior credit:

- when B is absent, its slice folds into proportional A;
- when A is absent, its proportional slice folds into equal B;
- when A is absent, the senior slice goes to seed;
- when both are absent, the complete toll goes to seed.

The oldest open A receives senior credit first, capped by its remaining
entitlement. The cap overshoot goes to seed. The proportional A denominator is
then sealed from the captured remainders minus only that senior credit. Every
captured A row, including the senior row after it is capped, keeps its sealed
weight. B receives an equal floor share among the captured open B rows. Cap
overshoot and division residue always go to seed.

The accepted vector `T=10,000`, A remainders `4,000` and `6,000`, and two open
B rows produces seed `5,001`, A credits `2,363` and `1,636`, and B credits
`500` each. The no-B vector with A remainders `1,000` and `10,000` produces
seed `5,750`, credits `1,000` and `3,250`. These are lamports, so no floating
point or decimal conversion enters the model.

## Serialized ticket

`Book::start_ticket` freezes registry version and length. A caller must advance
the following phases in order:

1. **Discover** visits every captured slot exactly once and stores immutable
   key, original slot, series, and remainder snapshots. Skips, reordering,
   stale slots, and substitutions fail without changing the book.
2. **Senior** applies the oldest-A slice once.
3. **SealWeights** computes the checked post-senior A denominator and each
   immutable row weight.
4. **Allocate** visits each snapshot exactly once. Replaying a completed row
   cannot credit it again. Chunks of one, four, eight, or 32 rows use the same
   oracle.
5. **Prune** removes newly capped rows in descending original-slot order by
   swap-delete and updates the moved row's active slot. Permanent history is
   unaffected.
6. **Commit** checks `credited + seed_destination == toll`, moves the pending
   toll into seed and global funded claims, and advances one scalar finalized
   ticket watermark.

The host implementation clones staged state around every public mutation so a
failed checked operation leaves the input unchanged. That O(N) clone is a
testing convenience. The eventual account program must use account-local
preflight and bounded cursor transactions; the model does not claim constant
total allocation cost or Solana compute fit. `Book::allocate_all_current` and
`Book::prune_all` use saturating cursor bounds, so a continuation with the
default `usize::MAX` chunk remains bounded after partial progress.

## Claims and custody

During an active ticket, a row can claim only previously funded balance. The
current ticket's pending credit remains unavailable. After commit, the pending
credit is lazily materialized into that row's funded balance on the next claim
or next allocation access. Materialization changes the local representation
only; it does not increment global claims a second time. Claims reduce funded
local and global balances but never reduce entitlement or lifetime credit.

The book tracks pot, seed, management, pending toll, global funded claims and
physical vault lamports. Its conservation check is:

```text
vault - protected_rent
  = pot + seed + management + global_funded_claims + pending_toll
```

Backer entitlement that has not been credited is not a cash liability in this
identity. Operations/provider/rent reserves are outside this host model and
must remain separate in the program.

`Book::seed_funded_fixture` is a test setup helper only. It adds matching
lamports to the modeled vault before adding a funded claim, increments the
row's lifetime credit by the same amount, and checks both values against the
row entitlement. This consumes the lifetime cap, so a later allocation cannot
pay past the entitlement. The helper rejects active-ticket mutation and
removes a row from the dense registry when the new remaining entitlement is
10 lamports or less. It is not a native financial operation; every claim path
still reduces both the funded liability and physical vault lamports, and the
conservation identity is checked before and after fixture claims.

## Availability barrier

While a ticket is active, A funding/top-ups, registry mutation, and subsequent
game payouts are rejected. Ordinary game deposits remain available while the
round is not closing because they change pot/management state and do not touch
the frozen ticket. Closing rejects deposits, later game payouts, and the pure
Backer A-funding helper. The helper is a model-only primitive for the canonical
Round admission path; it is not a standalone native-money entrypoint. Funded
claims and permissionless ticket progress remain available. This mirrors the
accepted narrow economics barrier; it is not a production authorization or
liveness proof.

The relevant Rust widths are deliberate: transferable cash and each local
funded/pending credit use checked `u64`; entitlement, lifetime credit, and
global funded claims use checked `u128`; pro-rata multiplication and division
use `ethnum::U256`.
