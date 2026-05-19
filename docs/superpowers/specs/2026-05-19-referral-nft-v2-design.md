# Referral NFT Cascade — V2.0 Design Spec

**Status:** Approved design, not yet implemented.
**Date:** 2026-05-19.
**Scope:** V2.0 of the NFT-referral migration. NFT mint + Model Y cascade routing + hourly snapshot + minimum admin tooling. Harberger tax, marketplace, player-facing defection, and v2-only shenanigans (Contract Steal/Burn/Raffle) deferred to V2.1+.
**Touches:** `shenanigans/main.mo`, `shenanigans/migration.mo`, plus frontend (`ReferralSection.tsx`, `useQueries.ts`, `DocsPage.tsx`). No changes to `ponzi_math` or `backend`.

## Motivation

The MLM cascade shipped in V1 (2026-05-16, per [mlm-deductive-cascade-design.md](2026-05-16-mlm-deductive-cascade-design.md)) walks a static `referralChain` map. Sponsors who recruited downlines receive a fixed share for life; the right to collect is not tradable, not seizable, and not extinguishable.

The full design vision (per [PONZI_POINTS_REDESIGN.md](../../../../keen-franklin-a424d7/docs/PONZI_POINTS_REDESIGN.md)) is for each downline edge to be represented by a transferable NFT, with the cascade routing through current NFT ownership instead of original-sponsor relationships. V1 was built to support this swap: the function `getPayoutTarget(current : Principal) : Principal` at [shenanigans/main.mo:591](../../../shenanigans/main.mo:591) was explicitly designed as the V2 swap-point, with a `// v2 will swap this to NFT-ownership lookup` comment.

This spec defines V2.0: introduce a contract NFT per downline edge, route the cascade through NFT ownership via hourly snapshots, and add admin tooling for testing. Harberger tax, marketplace, and player-facing defection are deferred to V2.1+ because they're tunable on top of routing — and they're easy to get wrong on first pass. The routing rewrite is not.

## Settled decisions

| # | Decision | Value |
|---|---|---|
| 1 | Scope | V2.0 only: NFT mint + Model Y cascade routing + snapshot + admin tooling. Harberger, marketplace, v2-shenanigans deferred. |
| 2 | NFT state location | **Custom state in `shenanigans/main.mo`**, not a separate ICRC-7 ledger. Rationale: Oisy support suspended; wallet visibility benefit gone. Internet Identity has no token UI; Plug's ICRC-7 support is unclear. ICRC-7 read facade can be added in V2.1 if a wallet ecosystem materializes. |
| 3 | NFT identity | Sequential `Nat`, starting from 1. ICRC-7-shaped naming for future-compat. |
| 4 | NFT metadata | `{ id, downliner, originalSponsor, mintedAt }`. Ownership tracked in a separate map (mutable; metadata is write-once). |
| 5 | Mint trigger | At `registerReferral`. Immediate, before any deposit. Charles accumulates inventory from never-deposited registrants → V2.1 marketplace inventory. |
| 6 | Launch backfill | One-shot bulk mint at upgrade: iterate `referralChain`, mint one NFT per entry to Charles. Functionally identical to sleazy reset for V2.0 cascade behavior (Charles already collects the same payouts); seeds V2.1 marketplace inventory at zero extra runtime cost. |
| 7 | Cascade model | **Model Y**: walk the NFT ownership graph, not the original-sponsor chain. Each hop: `current := ownerOf(NFT for current)`. Buying a contract inserts you into the cascade above that downliner, and the cascade continues UP from you. |
| 8 | `referralChain` map | Kept as historical record. No cascade role. Available for future "Rockstar referrer" rewards or similar historical queries. |
| 9 | Snapshot mechanism | Hourly (admin-tunable). On timer tick, copy `liveOwnership` → `ownershipSnapshot`. Cascade reads `ownershipSnapshot` exclusively. Transfers between snapshots don't affect cascade payouts until next refresh. |
| 10 | Snapshot atomicity | Anchored to internal `snapshotVersion : Nat` incremented on every ownership mutation. Snapshot records the version at capture time for diagnostics. (No external ICRC-3 block index needed since state is in-canister.) |
| 11 | Activity gating | V1 rule preserved (`isActive()` unchanged). In Model Y: skip inactive owners, continue traversal via `ownerOf(NFT_for_inactive_owner)`. End-of-chain → residual to Charles. |
| 12 | Defection in V2.0 | None player-facing. `burnContract` is admin-only. Transfers are free at the canister level but require off-chain coordination since no marketplace exists. V2.1 introduces buy-and-burn via Harberger. |
| 13 | Frontend scope | Medium: snapshot chip on MLM tab + "My Contracts" section listing owned NFTs + DocsPage rewrite. No marketplace UI. |
| 14 | Admin nuke tool | New shenanigan with 100% success / 0% failure / 0% backfire, effect = "transfer all chip-PP from target to Charles, burn their NFT, clear their state." Admin-only. Marked `// TODO(remove-before-decentralization)`. ICP recovery via backdated Rob position + manual withdraw (no automation). |

## Mechanic

### Data model

Six new state declarations in `shenanigans/main.mo`:

```motoko
type NftId = Nat;

type ContractNft = {
    id : NftId;
    downliner : Principal;
    originalSponsor : Principal;
    mintedAt : Time.Time;
};

type Ownership = {
    owner : Principal;
    acquiredAt : Time.Time;
    acquiredVia : { #mint; #transfer; #admin };
};

var contractNfts : Map<NftId, ContractNft>;       // NFT metadata; write-once except via burn
var liveOwnership : Map<NftId, Ownership>;         // current ownership; mutated on transfer/burn
var ownershipSnapshot : Map<NftId, Principal>;     // what the cascade reads; replaced on refresh
var downlinerToNft : Map<Principal, NftId>;        // immutable cache: principal → their NFT id
stable var nextNftId : Nat;
stable var snapshotVersion : Nat;
stable var snapshotVersionCaptured : Nat;          // snapshotVersion at last refresh
stable var snapshotTakenAt : Time.Time;
stable var snapshotIntervalSeconds : Nat;          // default 3600
stable var snapshotTimerId : ?Timer.TimerId;
```

`liveOwnership` is the source of truth for transfers; `ownershipSnapshot` is what the cascade reads. They diverge by up to `snapshotIntervalSeconds`.

### Mint trigger

`registerReferral(referrer)` (existing at [shenanigans/main.mo:423](../../../shenanigans/main.mo:423)) gets a single new line — a call to the internal `mintContract` helper after the existing `referralChain` insert:

```motoko
public shared ({ caller }) func registerReferral(referrer : Principal) : async () {
    switch (principalMap.get(referralChain, caller)) {
        case (?_) { /* already registered, no-op */ };
        case (null) {
            referralChain := principalMap.put(referralChain, caller, referrer);
            // ...existing reverse-index update...
            ignore mintContract(caller, referrer);  // NEW
        };
    };
};

// Idempotent on existing downliners (returns existing NftId without minting).
func mintContract(downliner : Principal, sponsor : Principal) : NftId {
    switch (principalMap.get(downlinerToNft, downliner)) {
        case (?id) { return id };
        case (null) {};
    };
    let id = nextNftId;
    nextNftId += 1;
    contractNfts := natMap.put(contractNfts, id, {
        id; downliner; originalSponsor = sponsor; mintedAt = Time.now();
    });
    liveOwnership := natMap.put(liveOwnership, id, {
        owner = sponsor;
        acquiredAt = Time.now();
        acquiredVia = #mint;
    });
    downlinerToNft := principalMap.put(downlinerToNft, downliner, id);
    snapshotVersion += 1;
    id;
};
```

The mint writes to `liveOwnership` only. The NFT isn't visible to the cascade until the next snapshot tick — same "up to 1h settlement" semantic as transfers.

### Cascade walk (Model Y)

Replace the body of `getPayoutTarget(current)` at [shenanigans/main.mo:591](../../../shenanigans/main.mo:591):

```motoko
// V2.0: walk the NFT ownership graph via the hourly snapshot.
// Returns the owner of `current`'s contract NFT, or house() if
// `current` has no NFT (terminates chain → residual to Charles).
func getPayoutTarget(current : Principal) : Principal {
    switch (principalMap.get(downlinerToNft, current)) {
        case (null) { house() };
        case (?nftId) {
            switch (natMap.get(ownershipSnapshot, nftId)) {
                case (null) { house() };     // NFT minted post-snapshot
                case (?owner) { owner };
            };
        };
    };
};
```

The existing `distributeDeductiveCascade` at [shenanigans/main.mo:1147](../../../shenanigans/main.mo:1147) already handles depth cap (10 hops), cycle detection (visited-set), and activity gating (`isActive()`) generically via `getPayoutTarget`. **No changes required to `distributeDeductiveCascade` itself.** The entire V2 cascade routing is the single function replacement above.

### Activity gating

`isActive(p)` at [shenanigans/main.mo:601](../../../shenanigans/main.mo:601) is unchanged. In Model Y, the existing skip-and-continue logic naturally translates to "skip inactive NFT owner, continue traversal via `ownerOf(NFT_for_that_inactive_owner)`." Dummy farm attacks (attacker creates fake principal, friends sign up via fake's link, fake collects without depositing) remain blocked.

### Snapshot mechanism

```motoko
// Synchronous map copy. No async, no ledger calls. Called by the
// snapshot timer and on demand via admin.
func refreshSnapshot() {
    var newSnapshot = natMap.empty<Principal>();
    for ((nftId, ownership) in natMap.entries(liveOwnership)) {
        newSnapshot := natMap.put(newSnapshot, nftId, ownership.owner);
    };
    ownershipSnapshot := newSnapshot;
    snapshotVersionCaptured := snapshotVersion;
    snapshotTakenAt := Time.now();
};

func startSnapshotTimer<system>() {
    switch (snapshotTimerId) {
        case (?tid) { Timer.cancelTimer(tid) };
        case (null) {};
    };
    snapshotTimerId := ?Timer.recurringTimer<system>(
        #seconds(snapshotIntervalSeconds),
        func() : async () { refreshSnapshot() }
    );
};
```

Re-armed in `postupgrade` following the existing observer pattern at [shenanigans/main.mo:668](../../../shenanigans/main.mo:668).

### Worked example

Setup:
- Alice originally referred by Beady → `NFT_1 { downliner: Alice, originalSponsor: Beady }`, owner = Beady at mint.
- Beady has no upline (registered against Charles directly).
- Rob has no NFT representing him (he joined V2 by acquiring contracts, not by being referred).
- Beady OTC-transfers `NFT_1` to Rob. After the next snapshot tick, `ownershipSnapshot[1] = Rob`.

Alice deposits 1 ICP → mints 200 PP base.
- `cascadeUnits = 20 PP` (10% off the top). Alice's mint = 180 PP net.
- `distributeDeductiveCascade(Alice, 20, eventId)` walks via `getPayoutTarget`:
  - **Hop 1:** `getPayoutTarget(Alice)` → `ownershipSnapshot[NFT_1]` = **Rob**. `isActive(Rob)` = true → pay Rob 50% of 20 = **10 PP**. Remaining = 10. `current := Rob`.
  - **Hop 2:** `getPayoutTarget(Rob)` → `downlinerToNft[Rob]` is empty → returns `house()`. `isActive(Charles)` = true → pay Charles 50% of 10 = **5 PP**. Remaining = 5. `current := Charles`.
  - **Hop 3:** `getPayoutTarget(Charles)` → `downlinerToNft[Charles]` is empty → returns `house()`. Charles is already in `visited` → cycle break.
- Residual: 5 PP → Charles.
- **Final tally:** Alice net 180, Rob 10, Charles 10. Total = 200 ✓.

Beady is bypassed entirely. Selling `NFT_1` removed her from Alice's cascade. This is the core Model Y dynamic: buying a contract inserts you into the cascade *above* that downliner, and the cascade continues UP from you — not from the original sponsor.

## API surface

### Public (player-callable)

| Method | Purpose |
|---|---|
| `registerReferral(referrer)` | Existing signature. Extended to also call `mintContract`. |
| `transferContract(nftId, newOwner) : async Result<(), Text>` | Caller must equal `liveOwnership[nftId].owner`. Updates `liveOwnership`, bumps `snapshotVersion`. Transfer effective at next snapshot. |
| `getMyContracts() : query [ContractDetail]` | Returns NFTs where `caller == liveOwnership[nftId].owner`. Each entry: nftId, downliner principal, original sponsor, mint date, acquired-at, acquired-via. |
| `getContractDetails(nftId) : query ?ContractDetail` | Public read. |
| `getSnapshotMeta() : query { takenAt; intervalSeconds; version; versionCaptured; nextRefreshAt }` | For UI snapshot chip. |
| `getReferralStats(caller) : query ReferralStats` | Modified: walks Model Y chain via snapshot. L1/L2/L3 owners (not original sponsors). |

### Admin-only

| Method | Purpose |
|---|---|
| `refreshSnapshotNow()` | Force snapshot refresh. Useful for testing and recovery. |
| `setSnapshotIntervalSeconds(seconds)` | Validate range [60, 86400]. Re-arms timer. |
| `burnContract(nftId)` | Removes NFT from `contractNfts`, `liveOwnership`, `downlinerToNft`. Bumps `snapshotVersion`. Reserved for V2.1 player-facing defection + the nuke shenanigan. |
| `nuke_player(principal)` | See "PP-drain nuke shenanigan" below. |

## Migration / launch behavior

One-shot upgrade-time function `Migration.runV2_0_nft` (added to `shenanigans/migration.mo`), invoked from `postupgrade` after the existing `runV2` MLM migration:

1. **State init.** All new maps start empty; `nextNftId = 1`; `snapshotIntervalSeconds = 3600`; `snapshotVersion = 0`.
2. **Bulk mint to Charles.** Iterate `referralChain`. For each `(downliner, originalSponsor)` entry, call `mintContract(downliner, house())`. Owner = Charles regardless of original sponsor. `originalSponsor` metadata preserved.
3. **Initial snapshot.** Synchronously call `refreshSnapshot()`. Cascade is live immediately post-migration.
4. **Start timer.** `startSnapshotTimer<system>()` to begin hourly refreshes.

Idempotency: `mintContract` guards on `downlinerToNft` presence, so re-running the migration is safe.

Rollback paths:
- **Pause cascade routing:** set `snapshotIntervalSeconds` to a large value and call `refreshSnapshotNow()` to freeze the snapshot at a known-good state. Cascade continues using frozen snapshot.
- **Revert to V1:** redeploy with V1 `getPayoutTarget` body restored. New state fields are ignored by the old code (Motoko enhanced-orthogonal-persistence trims unknown fields).

## Frontend changes

### `useQueries.ts`
- New `useGetMyContracts()` hook → `getMyContracts`.
- New `useGetSnapshotMeta()` hook → `getSnapshotMeta`.
- Existing `useGetReferralStats` unchanged at the React layer; the canister-side query is rewritten to walk Model Y.

### `ReferralSection.tsx`
- **Snapshot chip** at top of MLM tab. Format: `"Snapshot taken 12:00, next at 13:00"` with a subtle refresh icon. Tooltip: "Contract effects settle on the snapshot. Up to 1h delay between transfer and cascade impact."
- **"My Contracts" section** below the pyramid view. List each owned NFT: downliner name (via `useGetUserNames`), original sponsor name, mint date, acquired-via badge (Mint / Transfer / Admin). Empty state copy: *"You don't hold any contracts yet. Refer someone to receive their NFT, or wait for the marketplace (V2.1)."*
- Pyramid view structure unchanged; data source is now ownership-chain (L1 = owner of caller's NFT, etc.).

### `DocsPage.tsx`
Rewrites (drafts to be reviewed before merging):
- **"The Pyramid (MLM)" section.** Add contract-NFT subsection: what they are, how cascade routes via ownership, transfer mechanics, the 1-hour settlement rule. Match existing MLM/VC tone.
- **"Charles, your eternal upline" section.** Note Charles's expanded role as inventory holder for unminted/abandoned chains.
- **New glossary entries.** "Contract NFT," "Snapshot," "Original Sponsor," "Current Holder."
- Defer V2.1 mentions (no Harberger/marketplace teasers; keep the page accurate to shipped behavior).

## PP-drain nuke shenanigan (testing tool)

New shenanigan registered admin-only, hidden from player-facing UI:

```motoko
// SPELL_ID_NUKE_PLAYER = 100 (out-of-band from real spells 0..11)
// TODO(remove-before-decentralization): admin-only testing tool.
// Wipes a player's chip-PP, burns their own contract NFT, transfers
// any NFTs they OWN back to Charles (to prevent cascade leaking into
// orphaned subaccounts), and clears per-player state. Used to recycle
// sock-puppet accounts during pre-launch testing.
// MUST be removed before shenanigans canister controllers are dropped.
case (100) {
    requireAdmin(caller);

    // 1. Drain target's chip-PP to Charles.
    let bal = await getChipBalance(target);
    if (bal > 0) {
        ignore await chipTransfer(target, house(), bal, "nuke-" # Nat.toText(eventId));
    };

    // 2. Sweep all NFTs the target OWNS back to Charles. Prevents
    //    cascade payouts from accumulating in their orphaned subaccount.
    for ((nftId, ownership) in natMap.entries(liveOwnership)) {
        if (Principal.equal(ownership.owner, target)) {
            liveOwnership := natMap.put(liveOwnership, nftId, {
                owner = house();
                acquiredAt = Time.now();
                acquiredVia = #admin;
            });
            snapshotVersion += 1;
        };
    };

    // 3. Burn target's own contract NFT (where target is downliner).
    switch (principalMap.get(downlinerToNft, target)) {
        case (?id) {
            ignore burnContract(id);
            downlinerToNft := principalMap.delete(downlinerToNft, target);
        };
        case (null) {};
    };

    // 4. Clear per-player game state.
    shieldsActive := principalMap.delete(shieldsActive, target);
    cascadeBoosts := principalMap.delete(cascadeBoosts, target);
    // referralChain entry intentionally left as historical record.
};
```

Odds: 100/0/0. Cost: 0 PP. Caller: admin only. The owned-NFT sweep (step 2) is O(total NFTs); acceptable for an infrequent admin tool.

ICP recovery is out-of-band: admin creates a backdated 30-day Rob position absorbing the pot's ICP, then withdraws normally. No automation in this spec.

## Edge cases & failure modes

| Scenario | Behavior |
|---|---|
| Cascade fires before first snapshot tick | `ownershipSnapshot` empty → `getPayoutTarget` returns `house()` every hop → 100% to Charles. Mitigated: migration runs initial snapshot synchronously. |
| NFT minted between snapshots | Cascade for that downliner routes to Charles (snapshot has no entry) until next refresh. Same documented "up to 1h" rule. |
| Transfer between snapshots | Old owner keeps receiving payouts until next refresh. Same rule. |
| Cycle in ownership graph (Rob owns `NFT_Beady`, Beady owns `NFT_Rob`, third party deposits, chain reaches the cycle) | Existing visited-set in `distributeDeductiveCascade` catches it; cascade breaks to residual. |
| Self-owned NFT (owner buys own contract back via OTC transfer) | Owner pays themselves the cascade hop, effectively keeps the deduction. Edge case in V2.0 since transfers require off-chain coordination. Documented; no special handling. |
| Snapshot timer dies after upgrade | `postupgrade` re-arm pattern from the observer ([shenanigans/main.mo:668](../../../shenanigans/main.mo:668)). `getSnapshotMeta` exposes status for monitoring. |
| `snapshotVersion` overflow | `Nat` is unbounded in Motoko; no overflow. |
| `mintContract` traps mid-`registerReferral` | Motoko per-message atomicity: entire `registerReferral` message rolls back, including the `referralChain` write. State stays consistent. |
| Admin sets `snapshotIntervalSeconds` very high | Snapshot freezes. Cascade continues using last-known snapshot. Recoverable via `refreshSnapshotNow()` + interval reset. |

## Out of scope (deferred to V2.1+)

Per [PONZI_POINTS_REDESIGN.md](../../../../keen-franklin-a424d7/docs/PONZI_POINTS_REDESIGN.md):

- **Harberger tax.** Owner self-declares price, ongoing % to Charles, anyone can force-buy at declared price.
- **Marketplace UI.** Browse, list, buy contracts.
- **Player-facing defection.** Buy own contract at declared price + burn → out of the pyramid.
- **V2.1 shenanigans.** Contract Steal, Contract Burn (player-facing), Raffle. Plus rubber-banding on hold count and idle decay.
- **ICRC-7 read facade.** Expose `icrc7_owner_of` etc. on shenanigans for explorer/wallet visibility — add when a real reader exists.

## Acceptance criteria

This design is implemented correctly when:

1. Every `registerReferral` call mints exactly one NFT (or is a no-op for existing entries). Post-migration: `downlinerToNft.size() == referralChain.size()`.
2. Every pre-existing `referralChain` entry has a corresponding NFT in `contractNfts` with `liveOwnership.owner == house()` immediately after migration.
3. Cascade routing reads exclusively from `ownershipSnapshot`. Verified by mutating `liveOwnership` without refreshing and confirming cascades continue using pre-mutation state.
4. A transfer of an NFT does not affect cascade payouts until `refreshSnapshot()` runs. Verified by transferring, firing a deposit, observing old owner is paid, then refreshing and firing again, observing new owner is paid.
5. The snapshot timer fires at the configured interval; `snapshotTakenAt` advances on each tick.
6. Inactive NFT owners are skipped per V1 `isActive()` rule; cascade continues via the inactive owner's own NFT.
7. The MLM tab in the frontend displays a snapshot chip with sensible "next at X" value; "My Contracts" lists owned NFTs accurately, resolved with user-friendly names.
8. The PP-drain nuke shenanigan is admin-callable, sweeps target's chip-PP to Charles, burns the target's NFT, clears `shieldsActive` / `cascadeBoosts` / `downlinerToNft` for the target. The `// TODO(remove-before-decentralization)` comment is present.
9. Upgrade from current V1 state succeeds without data loss in any existing state (`referralChain`, `lastQualifyingDeposit`, `signupGiftClaimed`, etc.).

## Open items deferred to implementation

These are intentionally left to the writing-plans skill rather than re-litigated here:

- Exact Motoko stable-storage shape for the new maps (whether to use `Map.Map` vs. native `RBTree` vs. another container).
- Whether `Migration.runV2_0_nft` lives in `shenanigans/migration.mo` alongside `runV2` or warrants a separate module.
- Memo format for transfer events (`"contract-transfer-{nftId}-{newOwner}"` or similar).
- Pagination on `getOwnershipSnapshot()` if total NFT count exceeds query-response size limits (probably not needed at launch but worth noting).
- Test scenarios for the smoke test phase: cycle detection across NFT ownership graphs, self-owned NFTs, transfer-between-snapshots, all-inactive-chain, fresh-mint-pre-snapshot, depth-cap hit, residual-to-Charles when chain exhausts.
- DocsPage section wording (draft + review process per existing handling).
- Exact "My Contracts" UI styling (match existing ReferralSection theming, including TIER_THEME colors if applicable).
- Whether `getMyContracts` and `getReferralStats` should include a "snapshot lag warning" field when `snapshotVersion > snapshotVersionCaptured` by a significant margin.
