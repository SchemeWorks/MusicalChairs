# Referral NFT Cascade V2.0 — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement V2.0 of the NFT referral cascade per [the spec](../specs/2026-05-19-referral-nft-v2-design.md). Introduce a contract NFT per downline edge, route the cascade through NFT ownership via hourly snapshots, add public transfer + admin burn + admin nuke tooling, and surface contract data in the frontend.

**Architecture:** All canister work lives in `shenanigans/main.mo` (one canister, one file). Custom NFT state — no external ICRC-7 ledger. Cascade reads from an in-canister snapshot map (`ownershipSnapshot`) refreshed by a recurring timer; live state (`liveOwnership`) is mutated by transfers/burns but isn't consulted by `getPayoutTarget`. Migration is admin-triggered (`seedMigrationV2_NFT`), idempotent, bulk-mints existing `referralChain` entries to Charles. Frontend changes are scoped to `ReferralSection.tsx`, `useQueries.ts`, and `DocsPage.tsx`. No changes to `ponzi_math`, `backend`, or `dfx.json`.

**Tech Stack:** Motoko (`OrderedMap` based on `mo:base`), dfx, candid, React/TypeScript with `@tanstack/react-query`. No test directory — verification is `dfx build` (typecheck) + local-replica deploy + `dfx canister call` smoke tests.

---

## File map

| File | Action | Responsibility |
|---|---|---|
| `shenanigans/main.mo` | Modify | All canister logic: NFT types & state, `mintContract` helper, `transferContract` + `burnContract` public/admin methods, snapshot timer + refresh + admin tunables, replaced `getPayoutTarget` body, extended `registerReferral`, new query methods (`getMyContracts`, `getContractDetails`, `getSnapshotMeta`), modified `getReferralStats`, `adminNukePlayer`, `seedMigrationV2_NFT` admin one-shot. |
| `shenanigans/migration.mo` | No change | The existing migration framework (state-shape transforms) isn't used — V2.0 only adds new state, doesn't reshape existing state. Seeding is admin-callable. |
| `frontend/src/hooks/useQueries.ts` | Modify | Add `useGetMyContracts`, `useGetSnapshotMeta` hooks. Existing `useGetReferralStats` unchanged at React layer; canister-side semantics change. |
| `frontend/src/components/ReferralSection.tsx` | Modify | Add snapshot-time chip near top of MLM tab. Add "My Contracts" section below pyramid view. |
| `frontend/src/components/DocsPage.tsx` | Modify | Rewrite "The Pyramid (MLM)" section for NFT routing. Add glossary entries. Update Charles section. |
| `frontend/src/declarations/shenanigans/*` | Regenerated | `dfx generate` output after canister API changes. |

No changes to `ponzi_math/`, `backend/`, or `dfx.json`. Mainnet canister IDs unchanged.

## Verification approach

The project has no test suite. Each task ends in a build/smoke check:

- **Typecheck:** `dfx build shenanigans` from project root. Must exit 0.
- **Local deploy + smoke test:** `dfx start --background --clean` (once at session start), then `dfx deploy shenanigans` + `dfx canister call shenanigans <method> '(<args>)'` to exercise behavior.
- **Commits:** every task ends with a commit. Use the project's existing conventional-commits style (`feat(shenanigans): ...`, `chore: ...`, etc.) matching `git log --oneline -5`.

**Critical safety:** DO NOT deploy to mainnet from this plan. Per project memory, mainnet deploys require explicit user permission. The mainnet shenanigans canister ID is `j56tm-oaaaa-aaaac-qf34q-cai`.

---

## Task 1: Add NFT types and stable state to `shenanigans/main.mo`

**Files:**
- Modify: `shenanigans/main.mo` (add types near other type declarations; add state vars in the state block)

- [ ] **Step 1: Add the three new types**

Find the types section (look for `type ShenaniganConfig`, `type ReferralEarnings`, etc.). Add these three types alongside them:

```motoko
type NftId = Nat;

type ContractNft = {
    id : NftId;
    downliner : Principal;
    originalSponsor : Principal;
    mintedAt : Int;  // Time.now() value
};

type Ownership = {
    owner : Principal;
    acquiredAt : Int;
    acquiredVia : { #mint; #transfer; #admin };
};
```

- [ ] **Step 2: Add the new state vars**

Find the state block (lines around 240–310, near `var referralChain = principalMap.empty<Principal>();`). Add the new state vars:

```motoko
// V2.0 NFT cascade state. See docs/superpowers/specs/2026-05-19-referral-nft-v2-design.md.
var contractNfts = natMap.empty<ContractNft>();
var liveOwnership = natMap.empty<Ownership>();
var ownershipSnapshot = natMap.empty<Principal>();
var downlinerToNft = principalMap.empty<NftId>();
var nextNftId : Nat = 1;
var snapshotVersion : Nat = 0;
var snapshotVersionCaptured : Nat = 0;
var snapshotTakenAt : Int = 0;
var snapshotIntervalSeconds : Nat = 3600;
var snapshotTimerId : ?Timer.TimerId = null;
```

Bare `var` matches the surrounding pattern — enhanced orthogonal persistence makes these stable.

- [ ] **Step 3: Typecheck**

Run: `dfx build shenanigans`
Expected: exits 0 with no errors. Warnings about unused variables are acceptable at this stage (helpers added in later tasks).

- [ ] **Step 4: Commit**

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): add NFT types and stable state for V2.0 cascade"
```

---

## Task 2: Internal `mintContract` helper

**Files:**
- Modify: `shenanigans/main.mo` (add helper in the internal-functions section, near `getPayoutTarget`)

- [ ] **Step 1: Add the `mintContract` helper**

Add this function near the existing `getPayoutTarget` (around line 591). Place it before `getPayoutTarget` so it's available when we wire it up:

```motoko
// Internal. Mints exactly one contract NFT representing `downliner`.
// Idempotent: if an NFT already exists for `downliner`, returns its
// existing NftId without minting a duplicate. Initial owner = `sponsor`.
// Writes only to liveOwnership; the cascade won't see this NFT until
// the next snapshot refresh (up-to-1h settlement rule).
func mintContract(downliner : Principal, sponsor : Principal) : NftId {
    switch (principalMap.get(downlinerToNft, downliner)) {
        case (?id) { return id };
        case (null) {};
    };
    let id = nextNftId;
    nextNftId += 1;
    let now = Time.now();
    contractNfts := natMap.put(contractNfts, id, {
        id;
        downliner;
        originalSponsor = sponsor;
        mintedAt = now;
    });
    liveOwnership := natMap.put(liveOwnership, id, {
        owner = sponsor;
        acquiredAt = now;
        acquiredVia = #mint;
    });
    downlinerToNft := principalMap.put(downlinerToNft, downliner, id);
    snapshotVersion += 1;
    id;
};
```

- [ ] **Step 2: Typecheck**

Run: `dfx build shenanigans`
Expected: exits 0. Helper is unused but no warnings should block.

- [ ] **Step 3: Commit**

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): add internal mintContract helper (idempotent)"
```

---

## Task 3: Snapshot mechanism — refresh, timer, admin tunables

**Files:**
- Modify: `shenanigans/main.mo` (add refresh + timer functions; extend `postupgrade`; add admin methods)

- [ ] **Step 1: Add `refreshSnapshot` and `startSnapshotTimer`**

Add these near the existing `startObserver` (around line 647). Synchronous map copy — no async, no inter-canister calls:

```motoko
// V2.0 snapshot refresh. Copies liveOwnership.owner into
// ownershipSnapshot atomically (single-message). Records the
// snapshotVersion at capture time for diagnostics. Called by the
// snapshot timer and admin.
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

- [ ] **Step 2: Extend `postupgrade` to re-arm the snapshot timer**

Find the existing `postupgrade` system function (around line 668). It currently re-arms `startObserver`. Add the snapshot timer re-arm alongside it:

```motoko
system func postupgrade() {
    switch (adminPrincipal) {
        case (?_) {
            startObserver<system>();
            startSnapshotTimer<system>();  // V2.0
        };
        case (null) {};
    };
};
```

- [ ] **Step 3: Add the two admin tunables and the manual-refresh endpoint**

Add these as public shared methods near the existing admin functions (look for `requireAdmin` callers, e.g., `rotateAdmin`):

```motoko
public shared ({ caller }) func refreshSnapshotNow() : async () {
    requireAdmin(caller);
    refreshSnapshot();
};

public shared ({ caller }) func setSnapshotIntervalSeconds(seconds : Nat) : async () {
    requireAdmin(caller);
    if (seconds < 60 or seconds > 86_400) {
        Debug.trap("snapshotIntervalSeconds out of range [60, 86400]");
    };
    snapshotIntervalSeconds := seconds;
    startSnapshotTimer<system>();
};
```

- [ ] **Step 4: Typecheck**

Run: `dfx build shenanigans`
Expected: exits 0.

- [ ] **Step 5: Local deploy + smoke test**

If no local replica running: `dfx start --background --clean`
Then: `dfx deploy shenanigans`

Then (using your admin identity):
```bash
dfx canister call shenanigans refreshSnapshotNow '()'
dfx canister call shenanigans setSnapshotIntervalSeconds '(7200)'
dfx canister call shenanigans setSnapshotIntervalSeconds '(30)'  # should trap
```
Expected: first two succeed (return `()`); third traps with "snapshotIntervalSeconds out of range".

- [ ] **Step 6: Commit**

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): add snapshot refresh, timer, admin tunables"
```

---

## Task 4: Swap `getPayoutTarget` body to read snapshot

**Files:**
- Modify: `shenanigans/main.mo:591` (replace function body)

- [ ] **Step 1: Replace the body of `getPayoutTarget`**

Find the existing function at line 591:

```motoko
// v1: referralChain.get(current) ?? house(). v2 will swap this to
// NFT-ownership lookup — keep the function signature stable.
func getPayoutTarget(current : Principal) : Principal {
    switch (principalMap.get(referralChain, current)) {
        case (?p) { p };
        case (null) { house() };
    };
};
```

Replace its body and comment with:

```motoko
// V2.0: walk the NFT ownership graph via the hourly snapshot.
// Returns the owner of `current`'s contract NFT, or house() if
// `current` has no NFT (terminates chain → residual to Charles).
// referralChain is preserved as a historical record but not consulted.
func getPayoutTarget(current : Principal) : Principal {
    switch (principalMap.get(downlinerToNft, current)) {
        case (null) { house() };
        case (?nftId) {
            switch (natMap.get(ownershipSnapshot, nftId)) {
                case (null) { house() };  // NFT minted post-snapshot
                case (?owner) { owner };
            };
        };
    };
};
```

- [ ] **Step 2: Typecheck**

Run: `dfx build shenanigans`
Expected: exits 0.

- [ ] **Step 3: Local deploy + smoke test of cascade still firing**

Run: `dfx deploy shenanigans`

Then with admin identity, trigger a cascade by simulating a deposit (or, simpler, observe that a deposit-triggered cascade still completes without trapping). At this stage there are no NFTs, so every cascade hop should route to `house()` immediately and the cascade should terminate cleanly.

Verify state is unchanged:
```bash
dfx canister call shenanigans getSnapshotMeta '()'  # if Task 8 not yet done, skip
```

If you can't easily trigger a deposit in the local replica, smoke-check the canister responds to admin calls without trapping:
```bash
dfx canister call shenanigans refreshSnapshotNow '()'
```

- [ ] **Step 4: Commit**

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): swap getPayoutTarget to NFT-ownership snapshot (Model Y)"
```

---

## Task 5: Public `transferContract`

**Files:**
- Modify: `shenanigans/main.mo` (add public shared method near other public methods)

- [ ] **Step 1: Add `transferContract`**

Add this public method alongside other public-shared methods:

```motoko
public type TransferContractResult = {
    #Ok;
    #Err : Text;
};

public shared ({ caller }) func transferContract(
    nftId : NftId,
    newOwner : Principal
) : async TransferContractResult {
    if (Principal.isAnonymous(caller)) {
        return #Err("anonymous principals cannot transfer");
    };
    switch (natMap.get(liveOwnership, nftId)) {
        case (null) { #Err("nft does not exist") };
        case (?ownership) {
            if (not Principal.equal(ownership.owner, caller)) {
                return #Err("only the current owner can transfer");
            };
            liveOwnership := natMap.put(liveOwnership, nftId, {
                owner = newOwner;
                acquiredAt = Time.now();
                acquiredVia = #transfer;
            });
            snapshotVersion += 1;
            #Ok;
        };
    };
};
```

- [ ] **Step 2: Typecheck**

Run: `dfx build shenanigans`
Expected: exits 0.

- [ ] **Step 3: Local deploy + smoke test**

Run: `dfx deploy shenanigans`

Mint a contract by calling `registerReferral` (after Task 7) OR by calling a temporary helper. For now, smoke-test the error paths:

```bash
dfx canister call shenanigans transferContract '(999, principal "aaaaa-aa")'
# Expected: (variant { Err = "nft does not exist" })
```

Once Task 7 wires registerReferral, return to validate the success path: register a referral to mint NFT, then transfer it, then attempt transfer from the original owner (should fail with "only the current owner can transfer").

- [ ] **Step 4: Commit**

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): add public transferContract (owner-only)"
```

---

## Task 6: Admin `burnContract`

**Files:**
- Modify: `shenanigans/main.mo` (add admin method near other admin methods)

- [ ] **Step 1: Add `burnContract`**

```motoko
// Admin-only in V2.0. V2.1 will gate this behind player buy-and-burn
// via Harberger price. Removes the NFT from all three maps and bumps
// snapshotVersion. The snapshot itself isn't mutated — next refresh
// reflects the deletion (up-to-1h settlement applies).
public shared ({ caller }) func burnContract(nftId : NftId) : async () {
    requireAdmin(caller);
    switch (natMap.get(contractNfts, nftId)) {
        case (null) { Debug.trap("nft does not exist") };
        case (?nft) {
            contractNfts := natMap.delete(contractNfts, nftId);
            liveOwnership := natMap.delete(liveOwnership, nftId);
            downlinerToNft := principalMap.delete(downlinerToNft, nft.downliner);
            snapshotVersion += 1;
        };
    };
};
```

- [ ] **Step 2: Typecheck**

Run: `dfx build shenanigans`
Expected: exits 0.

- [ ] **Step 3: Smoke test**

```bash
dfx deploy shenanigans
dfx canister call shenanigans burnContract '(999)'
# Expected: trap "nft does not exist"
```

- [ ] **Step 4: Commit**

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): add admin burnContract"
```

---

## Task 7: Wire `registerReferral` to mint an NFT

**Files:**
- Modify: `shenanigans/main.mo:423` (existing `registerReferral` function)

- [ ] **Step 1: Add `mintContract` call to the success branch**

Find the existing `registerReferral` (around line 423). It currently looks roughly like:

```motoko
public shared ({ caller }) func registerReferral(referrer : Principal) : async () {
    switch (principalMap.get(referralChain, caller)) {
        case (?_) { /* already registered */ };
        case (null) {
            referralChain := principalMap.put(referralChain, caller, referrer);
            // ...existing reverse-index update via referrerToDownline...
        };
    };
};
```

Add the `mintContract` call inside the `(null)` branch, after the reverse-index update:

```motoko
public shared ({ caller }) func registerReferral(referrer : Principal) : async () {
    switch (principalMap.get(referralChain, caller)) {
        case (?_) { /* already registered, no-op */ };
        case (null) {
            referralChain := principalMap.put(referralChain, caller, referrer);
            // ...existing reverse-index update (DO NOT modify)...
            ignore mintContract(caller, referrer);  // V2.0: mint NFT to referrer
        };
    };
};
```

Per Motoko per-message atomicity: if any line in this branch traps, the entire `registerReferral` message rolls back including the `referralChain` insert. State stays consistent.

- [ ] **Step 2: Typecheck**

Run: `dfx build shenanigans`
Expected: exits 0.

- [ ] **Step 3: Local deploy + smoke test mint-on-register**

```bash
dfx deploy shenanigans
# As Alice's identity:
dfx canister call shenanigans registerReferral '(principal "<Beady-principal>")'
# Verify NFT minted:
dfx canister call shenanigans getContractDetails '(1)'  # after Task 8
# Or query state directly via getMyContracts as Beady (after Task 8).
```

If Task 8 isn't done yet, validate via a refreshSnapshot + getPayoutTarget side effect (deposit-triggered cascade should now route to Beady).

Verify idempotency: call `registerReferral` again with the same `caller` — should be a no-op (existing branch). NFT count unchanged.

- [ ] **Step 4: Commit**

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): mint contract NFT on registerReferral"
```

---

## Task 8: Read queries — `getMyContracts`, `getContractDetails`, `getSnapshotMeta`, modified `getReferralStats`

**Files:**
- Modify: `shenanigans/main.mo` (add three new query methods + replace `getReferralStats` body)

- [ ] **Step 1: Add `ContractDetail` type and `getMyContracts`**

```motoko
public type ContractDetail = {
    id : NftId;
    downliner : Principal;
    originalSponsor : Principal;
    currentOwner : Principal;
    mintedAt : Int;
    acquiredAt : Int;
    acquiredVia : { #mint; #transfer; #admin };
};

public query ({ caller }) func getMyContracts() : async [ContractDetail] {
    let buf = List.nil<ContractDetail>();
    var acc : List.List<ContractDetail> = buf;
    for ((nftId, ownership) in natMap.entries(liveOwnership)) {
        if (Principal.equal(ownership.owner, caller)) {
            switch (natMap.get(contractNfts, nftId)) {
                case (null) {};  // shouldn't happen but skip
                case (?nft) {
                    acc := List.push({
                        id = nft.id;
                        downliner = nft.downliner;
                        originalSponsor = nft.originalSponsor;
                        currentOwner = ownership.owner;
                        mintedAt = nft.mintedAt;
                        acquiredAt = ownership.acquiredAt;
                        acquiredVia = ownership.acquiredVia;
                    }, acc);
                };
            };
        };
    };
    List.toArray(acc);
};
```

- [ ] **Step 2: Add `getContractDetails`**

```motoko
public query func getContractDetails(nftId : NftId) : async ?ContractDetail {
    switch (natMap.get(contractNfts, nftId)) {
        case (null) { null };
        case (?nft) {
            switch (natMap.get(liveOwnership, nftId)) {
                case (null) { null };
                case (?ownership) {
                    ?{
                        id = nft.id;
                        downliner = nft.downliner;
                        originalSponsor = nft.originalSponsor;
                        currentOwner = ownership.owner;
                        mintedAt = nft.mintedAt;
                        acquiredAt = ownership.acquiredAt;
                        acquiredVia = ownership.acquiredVia;
                    };
                };
            };
        };
    };
};
```

- [ ] **Step 3: Add `getSnapshotMeta`**

```motoko
public type SnapshotMeta = {
    takenAt : Int;
    intervalSeconds : Nat;
    version : Nat;
    versionCaptured : Nat;
    nextRefreshAt : Int;  // takenAt + intervalSeconds * 1e9
};

public query func getSnapshotMeta() : async SnapshotMeta {
    {
        takenAt = snapshotTakenAt;
        intervalSeconds = snapshotIntervalSeconds;
        version = snapshotVersion;
        versionCaptured = snapshotVersionCaptured;
        nextRefreshAt = snapshotTakenAt + snapshotIntervalSeconds * 1_000_000_000;
    };
};
```

- [ ] **Step 4: Modify `getReferralStats` to walk Model Y**

Find the existing `getReferralStats` query. The L1/L2/L3 fields need to reflect ownership-chain principals (not original-sponsor chain). Replace the body's tier-resolution logic. The shape of `ReferralStats` itself can stay the same — only the source of L1/L2/L3 principals changes.

If the existing function reads `referralChain.get(player)` to find L1, replace those reads with the snapshot-based resolution. Pattern:

```motoko
// V2.0: L1 = owner of caller's NFT, L2 = owner of L1's NFT, etc.
func snapshotL1L2L3(player : Principal) : (?Principal, ?Principal, ?Principal) {
    let l1 = switch (principalMap.get(downlinerToNft, player)) {
        case (null) { null };
        case (?id) {
            switch (natMap.get(ownershipSnapshot, id)) {
                case (null) { null };
                case (?o) { ?o };
            };
        };
    };
    let l2 = switch (l1) {
        case (null) { null };
        case (?p1) {
            switch (principalMap.get(downlinerToNft, p1)) {
                case (null) { null };
                case (?id) { natMap.get(ownershipSnapshot, id) };
            };
        };
    };
    let l3 = switch (l2) {
        case (null) { null };
        case (?p2) {
            switch (principalMap.get(downlinerToNft, p2)) {
                case (null) { null };
                case (?id) { natMap.get(ownershipSnapshot, id) };
            };
        };
    };
    (l1, l2, l3);
};
```

Then inside `getReferralStats`, call `snapshotL1L2L3(caller)` and use the three principals as L1/L2/L3 identifiers. Earnings buckets (`referralEarnings`) already accumulate to the cascade recipients (NFT owners post-Task 4) — no change to that field.

- [ ] **Step 5: Typecheck**

Run: `dfx build shenanigans`
Expected: exits 0.

- [ ] **Step 6: Local deploy + smoke test**

```bash
dfx deploy shenanigans
dfx canister call shenanigans getSnapshotMeta '()'
# Expected: record with takenAt, intervalSeconds = 3600, version, versionCaptured, nextRefreshAt
dfx canister call shenanigans getMyContracts '()'
# Expected: vec {} (empty until you mint via registerReferral)
```

- [ ] **Step 7: Commit**

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): add NFT query methods, walk Model Y in getReferralStats"
```

---

## Task 9: Admin `adminNukePlayer` (testing tool, removable)

**Files:**
- Modify: `shenanigans/main.mo` (add admin method)

Implementation note: the spec describes this conceptually as a "shenanigan with 100% odds." Implementing literally as a registered shenanigan adds plumbing (config registry entry, dispatch in `castShenanigan`) without benefit. We implement as a plain admin function with the same effect.

- [ ] **Step 1: Add `adminNukePlayer`**

```motoko
// TODO(remove-before-decentralization): admin-only testing tool.
// Wipes a player's chip-PP, burns their own contract NFT, transfers
// any NFTs they own to Charles (prevents cascade leaking into orphaned
// subaccounts), and clears per-player game state. Used to recycle
// sock-puppet accounts during pre-launch testing. MUST be removed
// before shenanigans canister controllers are dropped.
public shared ({ caller }) func adminNukePlayer(target : Principal) : async () {
    requireAdmin(caller);

    // 1. Drain target's chip-PP to Charles.
    let bal = await getChipBalance(target);
    if (bal > 0) {
        ignore await chipTransfer(target, house(), bal, "nuke-" # Principal.toText(target));
    };

    // 2. Sweep all NFTs the target OWNS back to Charles.
    let now = Time.now();
    for ((nftId, ownership) in natMap.entries(liveOwnership)) {
        if (Principal.equal(ownership.owner, target)) {
            liveOwnership := natMap.put(liveOwnership, nftId, {
                owner = house();
                acquiredAt = now;
                acquiredVia = #admin;
            });
            snapshotVersion += 1;
        };
    };

    // 3. Burn target's own contract NFT (where target is downliner).
    switch (principalMap.get(downlinerToNft, target)) {
        case (?id) {
            switch (natMap.get(contractNfts, id)) {
                case (?_) {
                    contractNfts := natMap.delete(contractNfts, id);
                    liveOwnership := natMap.delete(liveOwnership, id);
                    snapshotVersion += 1;
                };
                case (null) {};
            };
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

Note: `getChipBalance` (main.mo:1128) and `chipTransfer` (main.mo:1105) are existing helpers. `shieldsActive` (main.mo:370) and `cascadeBoosts` (main.mo:372) are existing state.

- [ ] **Step 2: Typecheck**

Run: `dfx build shenanigans`
Expected: exits 0.

- [ ] **Step 3: Smoke test**

```bash
dfx deploy shenanigans
# Register sock puppet, then nuke:
dfx canister call shenanigans adminNukePlayer '(principal "<sock-puppet-principal>")'
# Expected: ()
dfx canister call shenanigans getMyContracts '()'  # as the sock puppet
# Expected: vec {} (their NFTs swept to Charles)
```

- [ ] **Step 4: Commit**

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): add adminNukePlayer testing tool (admin-only)"
```

---

## Task 10: Seed migration — `seedMigrationV2_NFT`

**Files:**
- Modify: `shenanigans/main.mo` (add admin one-shot)

Pattern mirrors the existing `seedMigrationV2` from the MLM cascade work — admin-callable, idempotent, no auto-trigger on postupgrade. Admin upgrades the canister, then explicitly calls this once to initialize V2.0 state.

- [ ] **Step 1: Add `seedMigrationV2_NFT`**

```motoko
public type SeedMigrationV2NftResult = {
    referralChainEntries : Nat;
    nftsMinted : Nat;
    nftsSkipped : Nat;  // already minted (idempotency)
    snapshotEntries : Nat;
};

public shared ({ caller }) func seedMigrationV2_NFT() : async SeedMigrationV2NftResult {
    requireAdmin(caller);

    var entries : Nat = 0;
    var minted : Nat = 0;
    var skipped : Nat = 0;

    // Bulk-mint for every existing referralChain entry.
    // mintContract is idempotent on `downliner`, so re-running is safe.
    //
    // OWNER-TARGET — pick ONE based on production state before deploying:
    //   Mode A (Charles consolidates):       mintContract(downliner, house())
    //   Mode B (preserve original sponsor):  mintContract(downliner, originalSponsor)
    //
    // Default below is Mode A per spec's brainstorm-time assumption of
    // sock-puppet-only referralChain. If V2.0 is shipping AFTER V1 has
    // accumulated real referral relationships, switch to Mode B before
    // deploy. See spec's "Owner-target selection" section.
    for ((downliner, originalSponsor) in principalMap.entries(referralChain)) {
        entries += 1;
        let preExisting = principalMap.get(downlinerToNft, downliner);
        ignore mintContract(downliner, house());  // Mode A — switch to `originalSponsor` if real users exist
        switch (preExisting) {
            case (?_) { skipped += 1 };
            case (null) { minted += 1 };
        };
    };

    // Initial snapshot.
    refreshSnapshot();

    // Start the snapshot timer if not already running.
    startSnapshotTimer<system>();

    {
        referralChainEntries = entries;
        nftsMinted = minted;
        nftsSkipped = skipped;
        snapshotEntries = natMap.size(ownershipSnapshot);
    };
};
```

Note: bulk mint uses `house()` as the owner regardless of `originalSponsor`. The `originalSponsor` metadata field captures the historical sponsor for any future "Rockstar referrer" reward feature.

- [ ] **Step 2: Typecheck**

Run: `dfx build shenanigans`
Expected: exits 0.

- [ ] **Step 3: Local deploy + smoke test the migration**

```bash
dfx deploy shenanigans
# Pre-populate referralChain by registering a few sock puppets:
dfx canister call shenanigans registerReferral '(principal "<beady>")'  # as alice
dfx canister call shenanigans registerReferral '(principal "<carl>")'   # as beady
# Now call seed:
dfx canister call shenanigans seedMigrationV2_NFT '()'
# Expected: record { referralChainEntries = 2; nftsMinted = 2; nftsSkipped = 0; snapshotEntries = 2 }
# Re-run to verify idempotency:
dfx canister call shenanigans seedMigrationV2_NFT '()'
# Expected: record { referralChainEntries = 2; nftsMinted = 0; nftsSkipped = 2; snapshotEntries = 2 }
```

- [ ] **Step 4: Commit**

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): add seedMigrationV2_NFT one-shot (idempotent)"
```

---

## Task 11: Regenerate frontend declarations

**Files:**
- Regenerate: `frontend/src/declarations/shenanigans/*`

- [ ] **Step 1: Run `dfx generate`**

```bash
dfx generate shenanigans
```
Expected: regenerates `shenanigans.did`, `shenanigans.did.js`, `shenanigans.did.d.ts` under `frontend/src/declarations/shenanigans/`.

- [ ] **Step 2: Verify new types appear in `.did`**

```bash
grep -E "ContractDetail|SnapshotMeta|transferContract|getMyContracts" frontend/src/declarations/shenanigans/shenanigans.did
```
Expected: each name appears.

- [ ] **Step 3: TypeScript typecheck**

```bash
cd frontend && npx tsc --noEmit
```
Expected: exits 0. If existing code references something that has been renamed or moved, fix those references inline before commit.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/declarations/shenanigans/
git commit -m "chore(frontend): regenerate shenanigans declarations for V2.0 NFT"
```

---

## Task 12: Frontend snapshot chip

**Files:**
- Modify: `frontend/src/hooks/useQueries.ts` (add hook)
- Modify: `frontend/src/components/ReferralSection.tsx` (add chip near top of MLM tab)

- [ ] **Step 1: Add `useGetSnapshotMeta` hook**

In `useQueries.ts`, add alongside the other `useGet*` hooks:

```typescript
export function useGetSnapshotMeta() {
  const actor = useShenaniganActor();
  return useQuery({
    queryKey: ['snapshotMeta'],
    queryFn: async () => {
      if (!actor) return null;
      const meta = await actor.getSnapshotMeta();
      return {
        takenAt: Number(meta.takenAt) / 1_000_000,        // ns → ms
        intervalSeconds: Number(meta.intervalSeconds),
        version: Number(meta.version),
        versionCaptured: Number(meta.versionCaptured),
        nextRefreshAt: Number(meta.nextRefreshAt) / 1_000_000,
      };
    },
    enabled: !!actor,
    refetchInterval: 60_000,  // refresh display once a minute
  });
}
```

- [ ] **Step 2: Add the snapshot chip to `ReferralSection.tsx`**

Import the hook at the top of `ReferralSection.tsx`:

```typescript
import { useGetSnapshotMeta } from '../hooks/useQueries';
```

Inside the `ReferralSection` component (near other `useGet*` calls), add:

```tsx
const { data: snapshotMeta } = useGetSnapshotMeta();
```

Render the chip near the top of the rendered output (just below any tab header, above the existing referral-link UI). Format times in HH:MM:

```tsx
{snapshotMeta && (
  <div
    className="inline-flex items-center gap-2 rounded-full px-3 py-1 text-xs"
    style={{
      background: 'rgba(255, 255, 255, 0.04)',
      border: '1px solid rgba(255, 255, 255, 0.12)',
      color: 'rgba(255, 255, 255, 0.7)',
    }}
    title="Contract effects settle on the hourly snapshot. Up to 1h delay between an NFT transfer and the cascade routing accordingly."
  >
    <span
      className="inline-block rounded-full"
      style={{ width: 6, height: 6, background: 'var(--mc-neon-green)' }}
    />
    <span>
      Snapshot {new Date(snapshotMeta.takenAt).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
      {' · next '}
      {new Date(snapshotMeta.nextRefreshAt).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
    </span>
  </div>
)}
```

- [ ] **Step 3: TypeScript typecheck + Vite build**

```bash
cd frontend && npx tsc --noEmit && npm run build
```
Expected: both exit 0.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/hooks/useQueries.ts frontend/src/components/ReferralSection.tsx
git commit -m "feat(frontend): add snapshot-time chip on MLM tab"
```

---

## Task 13: Frontend "My Contracts" section

**Files:**
- Modify: `frontend/src/hooks/useQueries.ts` (add hook)
- Modify: `frontend/src/components/ReferralSection.tsx` (add section below pyramid view)

- [ ] **Step 1: Add `useGetMyContracts` hook**

In `useQueries.ts`:

```typescript
export function useGetMyContracts() {
  const actor = useShenaniganActor();
  return useQuery({
    queryKey: ['myContracts'],
    queryFn: async () => {
      if (!actor) return [];
      const contracts = await actor.getMyContracts();
      return contracts.map((c: any) => ({
        id: Number(c.id),
        downliner: c.downliner.toText(),
        originalSponsor: c.originalSponsor.toText(),
        currentOwner: c.currentOwner.toText(),
        mintedAt: Number(c.mintedAt) / 1_000_000,
        acquiredAt: Number(c.acquiredAt) / 1_000_000,
        acquiredVia: 'mint' in c.acquiredVia ? 'Mint' : ('transfer' in c.acquiredVia ? 'Transfer' : 'Admin'),
      }));
    },
    enabled: !!actor,
  });
}
```

- [ ] **Step 2: Add the "My Contracts" section to `ReferralSection.tsx`**

Import the hook:

```typescript
import { useGetMyContracts } from '../hooks/useQueries';
```

Add inside the component:

```tsx
const { data: myContracts, isLoading: contractsLoading } = useGetMyContracts();
```

Render this section below the existing pyramid view. Use the existing `displayFor` helper (already defined in the file) for friendly names:

```tsx
<div className="mt-6">
  <h3 className="text-sm font-semibold mb-3" style={{ color: 'var(--mc-cyan)' }}>
    My Contracts
  </h3>
  {contractsLoading ? (
    <LoadingSpinner />
  ) : !myContracts || myContracts.length === 0 ? (
    <div
      className="rounded-lg px-4 py-6 text-xs text-center"
      style={{ background: 'rgba(255, 255, 255, 0.03)', color: 'rgba(255, 255, 255, 0.5)' }}
    >
      You don't hold any contracts yet. Refer someone to receive their NFT, or wait for the marketplace (V2.1).
    </div>
  ) : (
    <div className="space-y-2">
      {myContracts.map((c) => (
        <div
          key={c.id}
          className="rounded-lg px-3 py-2 text-xs flex items-center justify-between"
          style={{
            background: 'rgba(255, 255, 255, 0.04)',
            border: '1px solid rgba(255, 255, 255, 0.1)',
          }}
        >
          <div className="flex items-center gap-3">
            <span className="font-mono opacity-50">#{c.id}</span>
            <span>
              Downliner: <span className="font-semibold">{displayFor(c.downliner, allNames)}</span>
            </span>
            <span className="opacity-60">
              Original: {displayFor(c.originalSponsor, allNames)}
            </span>
          </div>
          <span
            className="px-2 py-0.5 rounded text-[10px] uppercase tracking-wide"
            style={{
              background: c.acquiredVia === 'Mint' ? 'rgba(57, 255, 20, 0.1)' :
                          c.acquiredVia === 'Transfer' ? 'rgba(34, 211, 238, 0.1)' :
                          'rgba(168, 85, 247, 0.1)',
              color: c.acquiredVia === 'Mint' ? 'var(--mc-neon-green)' :
                     c.acquiredVia === 'Transfer' ? 'var(--mc-cyan)' :
                     'var(--mc-purple)',
            }}
          >
            {c.acquiredVia}
          </span>
        </div>
      ))}
    </div>
  )}
</div>
```

Make sure `allNames` (the existing principal→name map used elsewhere in this file) covers contract principals — extend the existing `allPrincipals` memo to include `myContracts` entries:

```typescript
const allPrincipals = useMemo(() => {
  // existing pyramid principals...
  const extra = (myContracts ?? []).flatMap((c) => [c.downliner, c.originalSponsor]);
  return Array.from(new Set([...existing, ...extra]));
}, [/* deps + */ myContracts]);
```

(Exact existing form of `allPrincipals` already defined around line 80 of `ReferralSection.tsx`; merge contract principals into it.)

- [ ] **Step 3: TypeScript typecheck + Vite build**

```bash
cd frontend && npx tsc --noEmit && npm run build
```
Expected: both exit 0.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/hooks/useQueries.ts frontend/src/components/ReferralSection.tsx
git commit -m "feat(frontend): add My Contracts section on MLM tab"
```

---

## Task 14: DocsPage rewrites

**Files:**
- Modify: `frontend/src/components/DocsPage.tsx`

The voice is "formal/business with knowing satire, MLM/VC jargon over casino" per `PONZI_POINTS_REDESIGN.md`. Draft once, then user reviews before commit.

- [ ] **Step 1: Identify the sections to rewrite**

Open `frontend/src/components/DocsPage.tsx` and locate:
- "The Pyramid (MLM)" section (or equivalent — search for "MLM" or "Downline")
- "Charles" section (search for "Charles")
- Glossary section (search for "Glossary" or specific glossary entries)

- [ ] **Step 2: Rewrite "The Pyramid (MLM)" section**

Add a new subsection titled **"Your downline, contractually."** Draft text (review before commit):

> Every time you refer someone, the protocol mints a **Contract NFT** to your account. The NFT represents your right to skim from that person's future earnings — when they deposit, a 10% Front-End Load is split among the holders of the contract chain above them. You don't own *them*. You own a tradable instrument whose cash flows track theirs. Treat it like one.
>
> **Holding ≠ keeping.** Contracts are transferable. If you sell your Contract NFT for Beady to someone else, the buyer steps into your place in Beady's cascade. You're out; they're in. Beady doesn't get a notification. The pyramid does not consult sentiment.
>
> **Effects settle hourly.** When a contract changes hands, the cascade routing reflects the new owner only at the next **Snapshot** (every hour). Until then, the previous owner continues to collect. Plan transfers accordingly. Snapshots are an industry-standard settlement window.

- [ ] **Step 3: Update the "Charles" section**

Add a paragraph noting Charles's expanded role:

> **Charles holds every contract no one else does.** When a new account is registered without a sponsor, the Contract NFT representing that person is minted to Charles. When the upstream chain runs out, residual cascade flows to Charles. Some of these contracts will be made available through future channels at terms determined exclusively by Charles.

- [ ] **Step 4: Add glossary entries**

Wherever the existing glossary lives, add four entries:

> **Contract NFT** — A transferable instrument minted at the moment of a referral. Whoever currently holds the Contract NFT for a player receives the cascade share from that player's deposits.
>
> **Snapshot** — A periodic record of contract ownership (default: hourly). The cascade routes payouts based on the snapshot, not live ownership. Transfers take effect at the next snapshot.
>
> **Original Sponsor** — The principal who originally referred a given player, recorded in the Contract NFT's metadata. Distinct from the current holder. Provided as a historical reference; carries no payout right by itself.
>
> **Current Holder** — The principal who currently owns a player's Contract NFT and therefore receives the cascade share from that player's deposits. Subject to change at any time via transfer.

- [ ] **Step 5: User review of voice**

Tell the user the draft is ready in the file and ask them to review before commit. **Do not commit before user approval.** Per `PONZI_POINTS_REDESIGN.md`: "Drafts to be reviewed by user before merging."

- [ ] **Step 6: TypeScript typecheck + Vite build (post-approval)**

```bash
cd frontend && npx tsc --noEmit && npm run build
```
Expected: both exit 0.

- [ ] **Step 7: Commit (post-approval)**

```bash
git add frontend/src/components/DocsPage.tsx
git commit -m "docs(frontend): rewrite Pyramid/Charles/Glossary sections for V2.0 NFT"
```

---

## Task 15: End-to-end smoke test against acceptance criteria

**Files:** none modified unless gaps found.

Walk through the spec's 9 acceptance criteria. For each, perform the verification and record pass/fail. Fix any gaps inline.

- [ ] **Step 1: Fresh local deploy**

```bash
dfx start --background --clean
dfx deploy
```

- [ ] **Step 2: AC1 — every `registerReferral` mints exactly one NFT (idempotent)**

```bash
dfx canister call shenanigans registerReferral '(principal "<beady>")'  # as alice
dfx canister call shenanigans registerReferral '(principal "<beady>")'  # as alice, again
dfx canister call shenanigans getMyContracts '()'                       # as beady
# Expected: vec with exactly one entry (NFT_1, downliner = alice)
```

- [ ] **Step 3: AC2 — bulk migration mints to Charles for pre-existing entries**

```bash
# Pre-populate before seeding:
dfx canister call shenanigans registerReferral '(principal "<x>")'  # as a
dfx canister call shenanigans registerReferral '(principal "<y>")'  # as b
# Hard-reset state by redeploying with --mode reinstall (TESTING ONLY, never on mainnet):
# Then manually populate referralChain via earlier calls, then:
dfx canister call shenanigans seedMigrationV2_NFT '()'
# Expected: nftsMinted == referralChainEntries
# Verify owner is house():
dfx canister call shenanigans getMyContracts '()'  # as the admin/Charles
# Expected: vec with all NFTs
```

- [ ] **Step 4: AC3 — cascade reads only from snapshot**

Manually mutate state: register a referral (mints NFT, updates `liveOwnership`), do NOT call `refreshSnapshotNow`, then trigger a deposit. The cascade should route to `house()` (snapshot empty for new NFT) instead of to the freshly-minted owner.

```bash
dfx canister call shenanigans registerReferral '(principal "<beady>")'  # as alice
# Without refresh: trigger deposit (or simulate via getReferralStats query as alice)
dfx canister call shenanigans getReferralStats '(principal "<alice>")'
# Expected: L1 = null (snapshot doesn't know about NFT_1 yet)
dfx canister call shenanigans refreshSnapshotNow '()'
dfx canister call shenanigans getReferralStats '(principal "<alice>")'
# Expected: L1 = beady
```

- [ ] **Step 5: AC4 — transfer not reflected until snapshot**

```bash
dfx canister call shenanigans transferContract '(1, principal "<rob>")'  # as beady
dfx canister call shenanigans getReferralStats '(principal "<alice>")'
# Expected: L1 = beady (pre-transfer, snapshot lag)
dfx canister call shenanigans refreshSnapshotNow '()'
dfx canister call shenanigans getReferralStats '(principal "<alice>")'
# Expected: L1 = rob
```

- [ ] **Step 6: AC5 — snapshot timer fires**

Set interval low for testing:
```bash
dfx canister call shenanigans setSnapshotIntervalSeconds '(60)'
# Wait 70 seconds
sleep 70
dfx canister call shenanigans getSnapshotMeta '()'
# Expected: takenAt advanced from previous check
# Restore default:
dfx canister call shenanigans setSnapshotIntervalSeconds '(3600)'
```

- [ ] **Step 7: AC6 — inactive owner skipped, traversal continues**

This requires `isActive` returning false for a holder. Set `activityRequiresDeposit = true` (default), register a referral, do NOT have the referrer deposit, then trigger a cascade. The inactive referrer's NFT cascade should skip them and continue to `house()` (or the next active holder).

```bash
# (Use existing isActive admin config + observed-deposit state.)
# Trigger a cascade; verify the inactive-but-NFT-owning principal received nothing
# and the cascade payout went to the next active level.
```

- [ ] **Step 8: AC7 — frontend snapshot chip + My Contracts display**

```bash
cd frontend && npm run dev
# Open browser to local URL, navigate to MLM tab. Verify:
# - Snapshot chip shows "Snapshot HH:MM · next HH:MM"
# - "My Contracts" section appears with correct contracts (or empty-state copy)
```

- [ ] **Step 9: AC8 — nuke shenanigan sweeps PP + NFTs + state**

```bash
# Setup: register a sock puppet, mint some chip-PP to them, transfer them an NFT.
# Then:
dfx canister call shenanigans adminNukePlayer '(principal "<sock-puppet>")'
# Verify:
dfx canister call shenanigans getMyContracts '()'                                  # as sock puppet → empty
dfx canister call shenanigans getReferralStats '(principal "<sock-puppet>")'       # state cleared
# Chip balance check: query pp_ledger for sock puppet's chip subaccount → 0
```

- [ ] **Step 10: AC9 — upgrade preserves existing state**

```bash
# Take notes of current state (referralChain size, lastQualifyingDeposit, etc.).
dfx deploy shenanigans  # without --mode reinstall → preserves state
# Re-verify state matches pre-upgrade.
```

- [ ] **Step 11: Record results and fix gaps**

If any AC fails, treat as a bug — patch the implementation, recommit with the fix, re-run the failed AC. Do not move on with known failures.

- [ ] **Step 12: Final commit (if any fixes)**

```bash
git add <fixed files>
git commit -m "fix(shenanigans): <issue> uncovered by V2.0 acceptance smoke test"
```

---

## Done

All 15 tasks complete. The implementation matches the spec at [docs/superpowers/specs/2026-05-19-referral-nft-v2-design.md](../specs/2026-05-19-referral-nft-v2-design.md). Mainnet deployment is a separate step requiring explicit user permission (per project memory). After mainnet deploy: admin manually calls `seedMigrationV2_NFT` once to backfill NFTs to Charles and start the snapshot timer.
