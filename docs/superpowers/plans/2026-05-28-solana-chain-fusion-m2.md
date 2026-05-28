# Solana Chain Fusion — Milestone M2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Teach the shenanigans observer to poll a **second** ponzi_math canister (`ponzi_math_sol`, the SOL-denominated mirror landed in M1) alongside the existing `ponzi_math` (ICP) source, so SOL deposits and SOL-side backer top-ups mint Ponzi Points at SOL-specific rates and announce in the same chat. PP economy stays unified — one ledger, one minter (shenanigans), one chat — only the *event source* gains a second parallel branch.

**Architecture:** Add a `Denomination = { #icp; #sol }` parameter to the two observer functions (`processNewGames`, `processBackerDeltas`) so each can be invoked once per denomination per tick. Selecting on `Denomination` picks (a) which canister principal to talk to, (b) which observer cursor + retry maps to use, (c) which `MintConfig` rate fields to apply, and (d) what eventId prefix to write (`game-icp-N` vs `game-sol-N`, `backer-icp-...` vs `backer-sol-...`). The ICP path keeps identical behavior; the SOL path is wired only after a new admin setter (`setPonziMathSolPrincipal`) is called post-deploy. Stable state grows in two ways: implicit (new top-level vars get defaults — `ponziMathSolPrincipal`, `solGameIdCursor`, `solBackerSeen`, and matching retry/miss maps) and explicit (a V8 migration in `shenanigans/migration.mo` extends `MintConfig` with four `*PerSol` rate fields and adds `denomination : Denomination` to the `#signup` and `#roundResult` chat-item variants, backfilling existing items with `#icp`).

**Tech Stack:** Motoko (shenanigans canister). Inline migration via `(with migration = Migration.runV8)` per the project's `shenanigans/migration.mo` pattern (V2…V7 precedents). dfx for build + deploy. The mainnet test target is the live `shenanigans` canister talking to `ponzi_math` (existing) + `ponzi_math_sol = spc6q-xyaaa-aaaac-qg2ma-cai` (M1, devnet-RPC-configured).

**Spec reference:** [`docs/superpowers/specs/2026-05-25-solana-chain-fusion-design.md`](../specs/2026-05-25-solana-chain-fusion-design.md) — §"Shenanigans changes" (around line 372) + the M2 milestone block.

**Predecessor:** [`docs/superpowers/plans/2026-05-27-solana-chain-fusion-m1.md`](2026-05-27-solana-chain-fusion-m1.md) — shipped `ponzi_math_sol` with the same public surface (`getAllGames`, `getBackerPositions`, `getCurrentRoundId`) the observer already calls on `ponzi_math`. PR #91 merged at `bb4238c`.

**Out of scope for M2 (deferred to later sessions):**
- Frontend wiring (SIWS sign-in, `BuySOLFlyout`, deposit-address QR, withdrawal target picker). Backend-only PR.
- Real-SOL mainnet rollout (M3) — `ponzi_math_sol` stays on Solana **devnet**.
- M1 follow-ups: (i) `runDepositDetection`'s `if (gid > 0)` excludes gameId 0; (ii) `unwrapMultiSend` treats `Inconsistent` as a hard error. Both belong in a separate small session.
- Per-source mint multiplier semantics: M2 keeps the existing `mintWithEffects` path unchanged for both denominations. Any per-denomination multiplier policy is a separate spec.
- Renaming internal identifiers (`exitToll`, `coverCharge`, etc.) — see CLAUDE.md.

**Done when:**
1. `dfx build shenanigans --check` is clean from the M2 worktree.
2. Mainnet `shenanigans` redeployed via the stop → deploy → start dance documented in memory `project_shenanigans_deploy_lineage`, with `setPonziMathSolPrincipal(spc6q-xyaaa-aaaac-qg2ma-cai)` called post-deploy.
3. Game 0 on `ponzi_math_sol` (tester1's 0.5 SOL `simple21Day` from M1 — see memory `project_ponzi_math_sol_m1_state`) gets PP minted exactly once at `simple21DayPpPerSol × 0.5 = 3_000` PP units (less the 10% cascade deduction).
4. A known ICP-side game (any post-`gameIdCursor` `ponzi_math` game already minting today) continues to mint via the unchanged ICP path — no regression.
5. Event IDs are namespaced cleanly in the PP ledger memo field: `game-icp-N` for ICP-side and `game-sol-N` for SOL-side, with zero collisions across either side's `gameIdCursor`.
6. PR opened against `main` summarizing changes, the migration approach, and verification evidence (mainnet tx hashes from PP ledger plus the `getObserverStatus` snapshot before and after).

---

## Critical hygiene rules

These rules exist because they bit prior milestones; ignoring them re-incurs the cost.

1. **Don't touch `ponzi_math/main.mo`.** M2 is contained in `shenanigans/main.mo` and `shenanigans/migration.mo`. The ICP-side canister is untouched.
2. **Don't touch `ponzi_math_sol/main.mo`.** It already exposes the surface the observer needs (Task 1 Step 4 verifies). Any drift goes through a separate plan.
3. **Don't rename `exitToll` / `coverCharge` / `EXIT_TOLL_*` / `COVER_CHARGE_*`.** Project CLAUDE.md forbids it without explicit user instruction.
4. **Don't deploy without user permission.** Memory `feedback_deploy_safety` records a prior data-loss incident from an unauthorized backend redeploy. Task 16 (deploy) is explicitly gated on a user prompt; do not run it autonomously.
5. **Don't deploy without the stop → deploy → start dance.** Memory `project_shenanigans_deploy_lineage` documents that shenanigans has in-flight observer callbacks; deploying without stopping first risks state corruption. Task 16 follows the exact sequence.
6. **Don't break the existing ICP path.** Every refactor in Tasks 6–11 must keep the existing ICP-side observer behavior bit-identical. Task 14 (verification) checks this explicitly by confirming a known ICP-side mint still lands after deploy.
7. **Don't change the ICP-side event-id format.** The spec (line 451) says "Event IDs become `game-icp-N` and `game-sol-N` (was `game-N`)" but only the SOL side actually needs a prefix to avoid colliding with the existing `game-N` namespace. Renaming the ICP side too is a churn: existing PP-ledger mints have memos like `game-5`, admin tooling and ledger explorers may match on those exact strings, and switching to `game-icp-5` for future ICP mints fragments the historical lookup. The cursor (`gameIdCursor`) already guarantees no game is observed twice, so the dedup argument doesn't apply to the ICP path. Task 7 implements this asymmetry: ICP stays as `game-N` / `backer-...` / `signup-P`; SOL gets `game-sol-N` / `backer-sol-...` / `signup-sol-P`.

---

## File structure

**Modified files:**
- `shenanigans/main.mo` — main observer + state + chat-item type changes. Touches lines 174 (MintConfig), 278 (ChatItemKind), 440–550 (state block), 1084–1090 (getPonziMath), 1110–1140 (observer setup), 1200–1390 (observerTick/processNewGames/processBackerDeltas/primeObserverCursors), 4800–4820 (getObserverStatus + getMintConfig), plus the new `(with migration = Migration.runV8)` attachment on line 39.
- `shenanigans/migration.mo` — append V8 block at end of module (mirrors V7 layout exactly: old types, new types, `runV8` body).

**New files:** none. M2 is a contained refactor.

**Out of repo:**
- One mainnet admin call: `setPonziMathSolPrincipal(principal "spc6q-xyaaa-aaaac-qg2ma-cai")` post-deploy.

---

## Task 1: Verify environment and prerequisite spec sections

**Files:** none — environment check only.

- [ ] **Step 1: Confirm the worktree is on `feature/m2-shenanigans-observer`**

Run: `git rev-parse --abbrev-ref HEAD`
Expected: `feature/m2-shenanigans-observer`. If you see `main`, re-enter the worktree at `.worktrees/m2-shenanigans-observer/`.

- [ ] **Step 2: Confirm M1 is on `main`**

Run: `git log --oneline -1 main -- ponzi_math_sol/`
Expected: a commit touching `ponzi_math_sol/main.mo`, with `bb4238c` (M1 merge) or a descendant as HEAD of `main`. If empty, you don't have M1 — `git fetch && git pull --ff-only origin main` on the main worktree first.

- [ ] **Step 3: Confirm `dfx` and identity**

Run: `dfx --version && dfx identity whoami`
Expected: dfx 0.20+, identity `CharlesPonzi`.

- [ ] **Step 4: Confirm `ponzi_math_sol` exposes the surface the observer expects**

Run:
```bash
dfx canister --network=ic metadata ponzi_math candid:service > /tmp/icp-candid.did
dfx canister --network=ic metadata ponzi_math_sol candid:service > /tmp/sol-candid.did
grep -E "getAllGames|getBackerPositions|getCurrentRoundId" /tmp/icp-candid.did /tmp/sol-candid.did
```
Expected: each method appears on both canisters with identical signatures, e.g.:
```
/tmp/icp-candid.did:  getAllGames : () -> (vec GameRecord) query;
/tmp/sol-candid.did:  getAllGames : () -> (vec GameRecord) query;
/tmp/icp-candid.did:  getBackerPositions : () -> (vec BackerPosition) query;
/tmp/sol-candid.did:  getBackerPositions : () -> (vec BackerPosition) query;
/tmp/icp-candid.did:  getCurrentRoundId : () -> (nat) query;
/tmp/sol-candid.did:  getCurrentRoundId : () -> (nat) query;
```
If any line is missing on the SOL side, stop. The observer cannot bind to a canister that lacks the methods. Surface the gap to the user before proceeding — likely a regression in M1 that needs a separate fix.

- [ ] **Step 5: Confirm the M1 SOL-side game 0 is still in `ponzi_math_sol`'s state**

Run:
```bash
dfx canister --network=ic call ponzi_math_sol getAllGames 2>&1 | head -30
```
Expected: at least one record with `id = 0`, `plan = variant { simple21Day }`, `amount` near 0.48 (post-cover-charge net). This is the M1 smoke-test deposit and it's what Task 14 will verify gets PP minted by the new SOL path. If this record is missing, M1 state didn't survive — surface to the user.

- [ ] **Step 6: Read the spec sections governing M2**

Open `docs/superpowers/specs/2026-05-25-solana-chain-fusion-design.md` and re-read:
- §"Shenanigans changes" — MintConfig additions, second cursor, observer tick changes.
- §"PP mint rates table" — the 30× ratio anchoring SOL-side rates to ICP rates at deploy time.
- §"M2: Wire shenanigans observer to poll `ponzi_math_sol`" — the milestone definition.

If anything contradicts this plan, surface the conflict before proceeding.

- [ ] **Step 7: No commit — environment check only.**

---

## Task 2: Add V8 migration types and `runV8` to `shenanigans/migration.mo`

**Files:**
- Modify: `shenanigans/migration.mo` (append V8 block at end of `module`)

V8 transforms two stable fields:

1. **`mintConfig`** — extends `MintConfig` with four `*PerSol` Nat fields. We default them to spec values (6_000 / 12_000 / 18_000 plus `4 * backerPpPerIcp × 30`-equivalent for `backerPpPerSol`, taking the live `backerPpPerIcp` as the multiplier base — see spec §"PP mint rates table").
2. **`chatItems`** — adds `denomination : Denomination` to the `#signup` and `#roundResult` variant arms, defaulting all historical items to `#icp` (pre-M2 they are all ICP-sourced; not a guess, a fact about pre-M2 data).

Every other state field flows through unchanged because the persistable types of those fields don't change.

- [ ] **Step 1: Read the V7 block to confirm the project pattern**

Read `shenanigans/migration.mo` lines 454–651. Note the structure:
- Type aliases for the new variant constructors (e.g. `V7ShenaniganType`).
- Old type defs (snapshot of the pre-migration shape).
- New type defs (snapshot of the post-migration shape).
- `public func runVN(old : {...}) : {...} { ... }`.

V8 follows the same pattern.

- [ ] **Step 2: Append the V8 block to `shenanigans/migration.mo`**

Open `shenanigans/migration.mo`. Find the closing `};` on line 651 of the module body and insert this block immediately before it:

```motoko

    // ================================================================
    // V8 — Add Solana-side observer support
    //
    // Extends two persisted shapes for M2 (Solana chain fusion):
    //
    // (a) MintConfig gains four PP-per-SOL rate fields paired with the
    //     existing PP-per-ICP rate fields. Defaults pin the 30x ratio
    //     from the design spec at deploy time:
    //         simple21DayPpPerSol       = 6_000
    //         compounding15DayPpPerSol  = 12_000
    //         compounding30DayPpPerSol  = 18_000
    //         backerPpPerSol            = old.backerPpPerIcp * 30
    //     Admin can retune any of them after deploy via the
    //     setSimple21DayPpPerSol / setCompounding15DayPpPerSol /
    //     setCompounding30DayPpPerSol / setBackerPpPerSol per-field
    //     setters (added in M2, mirroring the existing PerIcp setters).
    //
    // (b) ChatItemKind gains a denomination field on the #signup and
    //     #roundResult arms (Denomination = { #icp; #sol }). Every
    //     pre-M2 chat item is ICP-sourced by construction, so the
    //     migration backfills denomination = #icp on every historical
    //     #signup and #roundResult item. Other variants pass through.
    //
    // See docs/superpowers/specs/2026-05-25-solana-chain-fusion-design.md.
    // ================================================================

    type V8Denomination = { #icp; #sol };

    type V8OldMintConfig = V3NewMintConfig;

    type V8NewMintConfig = {
        simple21DayPpPerIcp : Nat;
        compounding15DayPpPerIcp : Nat;
        compounding30DayPpPerIcp : Nat;
        backerPpPerIcp : Nat;
        referralL1Bps : Nat;
        referralL2Bps : Nat;
        referralL3Bps : Nat;
        minDepositPp : Nat;
        cashOutDelaySeconds : Nat;
        observerIntervalSeconds : Nat;
        cascadeInitialBps : Nat;
        cascadePassthroughBps : Nat;
        signupGiftPp : Nat;
        activityRequiresDeposit : Bool;
        activityWindowDays : ?Nat;
        // NEW in V8: SOL-side mint rates.
        simple21DayPpPerSol : Nat;
        compounding15DayPpPerSol : Nat;
        compounding30DayPpPerSol : Nat;
        backerPpPerSol : Nat;
    };

    // V8 reuses the V7 ShenaniganType / ShenaniganOutcome / Reaction shapes
    // because they don't change in this migration.
    type V8ShenaniganType = V7ShenaniganType;
    type V8ShenaniganOutcome = V7ShenaniganOutcome;
    type V8Reaction = V7Reaction;

    type V8OldChatItemKind = V7NewChatItemKind;

    type V8NewChatItemKind = {
        #userMessage : { body : Text; replyTo : ?Nat };
        #spellCast : {
            castId : Nat;
            caster : Principal;
            shenaniganType : V8ShenaniganType;
            target : ?Principal;
            outcome : V8ShenaniganOutcome;
            ppDelta : ?Int;
            affectedCount : ?Nat;
            renameDetail : ?{ oldName : Text; newName : Text };
            shieldDeflected : ?Bool;
        };
        // NEW: denomination field on #signup.
        #signup : { newUser : Principal; denomination : V8Denomination };
        #rankUp : { user : Principal; newRank : Text };
        // NEW: denomination field on #roundResult.
        #roundResult : {
            gameId : Nat;
            winner : Principal;
            winnerPpUnits : Nat;
            denomination : V8Denomination;
        };
        #reginald : { line : Text; triggerKind : Text };
        #pinUpdate : { body : Text };
    };

    type V8OldChatItem = {
        id : Nat;
        author : Principal;
        timestamp : Int;
        kind : V8OldChatItemKind;
        reactions : [V8Reaction];
        deleted : Bool;
    };

    type V8NewChatItem = {
        id : Nat;
        author : Principal;
        timestamp : Int;
        kind : V8NewChatItemKind;
        reactions : [V8Reaction];
        deleted : Bool;
    };

    public func runV8(
        old : {
            var mintConfig : V8OldMintConfig;
            var chatItems : [V8OldChatItem];
        }
    ) : {
        var mintConfig : V8NewMintConfig;
        var chatItems : [V8NewChatItem];
    } {
        let oldCfg = old.mintConfig;
        let newCfg : V8NewMintConfig = {
            simple21DayPpPerIcp = oldCfg.simple21DayPpPerIcp;
            compounding15DayPpPerIcp = oldCfg.compounding15DayPpPerIcp;
            compounding30DayPpPerIcp = oldCfg.compounding30DayPpPerIcp;
            backerPpPerIcp = oldCfg.backerPpPerIcp;
            referralL1Bps = oldCfg.referralL1Bps;
            referralL2Bps = oldCfg.referralL2Bps;
            referralL3Bps = oldCfg.referralL3Bps;
            minDepositPp = oldCfg.minDepositPp;
            cashOutDelaySeconds = oldCfg.cashOutDelaySeconds;
            observerIntervalSeconds = oldCfg.observerIntervalSeconds;
            cascadeInitialBps = oldCfg.cascadeInitialBps;
            cascadePassthroughBps = oldCfg.cascadePassthroughBps;
            signupGiftPp = oldCfg.signupGiftPp;
            activityRequiresDeposit = oldCfg.activityRequiresDeposit;
            activityWindowDays = oldCfg.activityWindowDays;
            // SOL rates anchored at deploy time per the design spec's 30x ratio.
            simple21DayPpPerSol = 6_000;
            compounding15DayPpPerSol = 12_000;
            compounding30DayPpPerSol = 18_000;
            backerPpPerSol = oldCfg.backerPpPerIcp * 30;
        };

        let migrated = Buffer.Buffer<V8NewChatItem>(old.chatItems.size());
        for (item in old.chatItems.vals()) {
            let newKind : V8NewChatItemKind = switch (item.kind) {
                // Pre-M2: every #signup and every #roundResult is ICP-sourced.
                case (#signup({ newUser })) {
                    #signup({ newUser; denomination = #icp });
                };
                case (#roundResult({ gameId; winner; winnerPpUnits })) {
                    #roundResult({ gameId; winner; winnerPpUnits; denomination = #icp });
                };
                // Other variants pass through unchanged.
                case (#userMessage(x)) { #userMessage(x) };
                case (#spellCast(x)) { #spellCast(x) };
                case (#rankUp(x)) { #rankUp(x) };
                case (#reginald(x)) { #reginald(x) };
                case (#pinUpdate(x)) { #pinUpdate(x) };
            };
            migrated.add({
                id = item.id;
                author = item.author;
                timestamp = item.timestamp;
                kind = newKind;
                reactions = item.reactions;
                deleted = item.deleted;
            });
        };

        {
            var mintConfig = newCfg;
            var chatItems = Buffer.toArray(migrated);
        };
    };
```

- [ ] **Step 3: Verify migration.mo parses by itself**

The V8 block references types `V3NewMintConfig`, `V7NewChatItemKind`, `V7ShenaniganType`, `V7ShenaniganOutcome`, `V7Reaction` — confirm each is declared above the V8 block.

Run: `grep -nE "type V3NewMintConfig|type V7NewChatItemKind|type V7ShenaniganType|type V7ShenaniganOutcome|type V7Reaction" shenanigans/migration.mo`
Expected: each appears on a line below 100 and above the V8 block.

- [ ] **Step 4: Type-check via the existing dfx build**

Run: `dfx build shenanigans --check --network=local 2>&1 | tail -30`
Expected: no errors. The actor hasn't been changed yet, so the module compiles in isolation. The compiler may emit a warning about an unused `runV8` (we haven't attached it yet) — ignore.

If you see a type-table error referencing `V3NewMintConfig`, the V3 type definition has drifted since V3 was applied; reconcile with the *current* MintConfig field set declared in `shenanigans/main.mo` line 174–190. The two must agree on every field except the four SOL additions.

- [ ] **Step 5: Commit**

```bash
git add shenanigans/migration.mo
git commit -m "feat(shenanigans): add V8 migration types for SOL observer support

Extends MintConfig with simple21DayPpPerSol, compounding15DayPpPerSol,
compounding30DayPpPerSol, backerPpPerSol (defaults 6k/12k/18k/30x
backerPpPerIcp per the design spec). Extends ChatItemKind.#signup and
ChatItemKind.#roundResult with a denomination tag, backfilling every
pre-M2 chat item to #icp (correct by construction — no SOL events
existed before M2). Migration block is defined here; attachment to
the actor lands in Task 3.

See docs/superpowers/plans/2026-05-28-solana-chain-fusion-m2.md."
```

---

## Task 3: Add `Denomination` type, extend `MintConfig`, attach migration

**Files:**
- Modify: `shenanigans/main.mo` (Denomination type, MintConfig type, mintConfig initializer, migration attachment on the actor)

We can't add new state fields until the migration is attached, otherwise the upgrade fails with M0170. This task lands the type-shape changes and the migration attachment together so the actor compiles + upgrades cleanly.

- [ ] **Step 1: Add the `Denomination` type near other public types**

Open `shenanigans/main.mo`. Find the existing `public type ChatItemKind` declaration at line 278. Immediately *above* it, insert:

```motoko
    /// Asset side of an observable event. Pre-M2 the observer only ever
    /// saw ICP-side ponzi_math state; M2 adds a SOL-side source by adding
    /// this tag to event-bearing chat items and a Denomination parameter
    /// to the observer functions. Denomination is a property of the EVENT,
    /// not of the user — a user with one of each kind of deposit will
    /// surface as two separate events.
    public type Denomination = { #icp; #sol };

```

- [ ] **Step 2: Add the four SOL rate fields to `MintConfig`**

Find `public type MintConfig = {` at line 174. Replace the entire MintConfig block (lines 174-190) with:

```motoko
    /// Mutable mint + economy configuration. All fields admin-tunable.
    public type MintConfig = {
        simple21DayPpPerIcp : Nat;    // initial 1000 (whole PP per ICP)
        compounding15DayPpPerIcp : Nat; // initial 2000
        compounding30DayPpPerIcp : Nat; // initial 3000
        backerPpPerIcp : Nat;          // initial 4000
        referralL1Bps : Nat;           // deprecated; unused by deductive cascade
        referralL2Bps : Nat;           // deprecated; unused by deductive cascade
        referralL3Bps : Nat;           // deprecated; unused by deductive cascade
        minDepositPp : Nat;            // initial 5000 (whole PP)
        cashOutDelaySeconds : Nat;     // initial 604_800
        observerIntervalSeconds : Nat; // initial 10
        cascadeInitialBps : Nat;
        cascadePassthroughBps : Nat;
        signupGiftPp : Nat;
        activityRequiresDeposit : Bool;
        activityWindowDays : ?Nat;
        // M2 (Solana chain fusion): SOL-denominated mint rates. Anchored
        // at deploy time per the design spec's 30x ratio; admin-tunable
        // via the matching per-field setters (setSimple21DayPpPerSol etc.,
        // added alongside the existing setSimple21DayPpPerIcp pattern).
        simple21DayPpPerSol : Nat;     // initial 6_000
        compounding15DayPpPerSol : Nat; // initial 12_000
        compounding30DayPpPerSol : Nat; // initial 18_000
        backerPpPerSol : Nat;          // initial = backerPpPerIcp * 30 (set by V8 migration)
    };
```

- [ ] **Step 3: Extend the `mintConfig` initializer with SOL defaults**

Find the `var mintConfig : MintConfig = {` block at line 527. Replace the entire block (lines 527-543) with:

```motoko
    var mintConfig : MintConfig = {
        simple21DayPpPerIcp = 1000;
        compounding15DayPpPerIcp = 2000;
        compounding30DayPpPerIcp = 3000;
        backerPpPerIcp = 4000;
        referralL1Bps = 800;
        referralL2Bps = 500;
        referralL3Bps = 200;
        minDepositPp = 5000;
        cashOutDelaySeconds = 604_800;
        observerIntervalSeconds = 10;
        cascadeInitialBps = 1000;
        cascadePassthroughBps = 5000;
        signupGiftPp = 500;
        activityRequiresDeposit = true;
        activityWindowDays = null;
        // M2 (Solana chain fusion) — fresh-install defaults match the
        // V8 migration defaults so a fresh deploy has the same MintConfig
        // shape an upgraded deploy gets.
        simple21DayPpPerSol = 6_000;
        compounding15DayPpPerSol = 12_000;
        compounding30DayPpPerSol = 18_000;
        backerPpPerSol = 120_000;  // 4_000 * 30; admin can retune
    };
```

- [ ] **Step 4: Add the migration import + attach the migration block**

Find `import Icrc21 "icrc21";` near the top of `shenanigans/main.mo` (around line 23). Replace the line with:

```motoko
import Icrc21 "icrc21";
import Migration "migration";
```

Find `persistent actor Self {` at line 39. Replace that single line with:

```motoko
(with migration = Migration.runV8)
persistent actor Self {
```

- [ ] **Step 5: Update the V7-applied comment block**

Find the historical migration comments around lines 30-37. Add a V8 line so the file documents the migration sequence. Replace:

```motoko
// Migration V6 (embed spell-cast metadata in #spellCast chat items) was
// applied 2026-05-21. See migration.mo for the historical migration record
// and Migration.runV6.

// Migration V7 (add optional outcome-detail fields to ShenaniganRecord +
// #spellCast chat item) was applied 2026-05-27. See migration.mo for the
// historical migration record and Migration.runV7.
```

with:

```motoko
// Migration V6 (embed spell-cast metadata in #spellCast chat items) was
// applied 2026-05-21. See migration.mo for the historical migration record
// and Migration.runV6.

// Migration V7 (add optional outcome-detail fields to ShenaniganRecord +
// #spellCast chat item) was applied 2026-05-27. See migration.mo for the
// historical migration record and Migration.runV7.

// Migration V8 (Solana chain fusion observer support: MintConfig gains
// *PerSol rates; ChatItemKind.#signup + #roundResult gain a denomination
// tag, backfilled to #icp on all pre-M2 chat items). Applied during the
// M2 deploy — see docs/superpowers/plans/2026-05-28-solana-chain-fusion-m2.md
// and Migration.runV8.
```

(We leave the V8 line in present-tense "Applied during the M2 deploy" until the deploy actually lands; Task 16 updates this to the historical past-tense with the deploy date once mainnet is live.)

- [ ] **Step 6: Verify the build is clean**

Run: `dfx build shenanigans --check --network=local 2>&1 | tail -30`
Expected: no errors. The actor now compiles with the new MintConfig + the migration attachment. The compiler may flag that `Denomination` and `runV8`'s input `chatItems` aren't yet referenced — that's expected (next tasks consume them).

If you see "field `simple21DayPpPerSol` missing in initializer" — your edits to the initializer (Step 3) didn't land. If you see "type field … missing" — the V8 type defs in migration.mo (Task 2 Step 2) don't match the new actor shape; reconcile the two.

- [ ] **Step 7: Commit**

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): extend MintConfig with PerSol rates + attach V8 migration

Adds simple21DayPpPerSol (6_000), compounding15DayPpPerSol (12_000),
compounding30DayPpPerSol (18_000), and backerPpPerSol (120_000 =
backerPpPerIcp * 30) to MintConfig and the fresh-install initializer.
Adds the public Denomination = { #icp; #sol } type. Attaches
'(with migration = Migration.runV8)' so the upgrade transforms
existing MintConfig records and backfills chat-item denomination
tags (no new var state yet — that lands in Tasks 4-5)."
```

---

## Task 4: Add `ponziMathSolPrincipal` state + admin setter + helper

**Files:**
- Modify: `shenanigans/main.mo` (state declaration, admin setter, `getPonziMathSol` helper)

**Note on the existing pattern.** There is no standalone `setPonziMathPrincipal` setter — the ICP-side principal is set inside `initialize(ponziMathCanisterId : Principal)` (around line 765). We do NOT want to overload `initialize` with a SOL parameter (that would change the canister's init signature). Instead, the SOL-side setter follows the admin-setter pattern used by `setHousePrincipal` (line ~4892): a dedicated `public shared` function guarded by `requireAdmin(caller)`.

- [ ] **Step 1: Add the state declaration**

Find `var ponziMathPrincipal : ?Principal = null;` at line 449. Replace with:

```motoko
    var ponziMathPrincipal : ?Principal = null;
    // M2 (Solana chain fusion): second ponzi_math instance, SOL-denominated.
    // null until admin calls setPonziMathSolPrincipal post-deploy. While
    // null, the observer's SOL-side branch is a no-op — no inter-canister
    // call, no state touched. ICP-side path is unaffected.
    var ponziMathSolPrincipal : ?Principal = null;
```

- [ ] **Step 2: Locate the `setHousePrincipal` admin-setter pattern**

Run: `grep -n "setHousePrincipal" shenanigans/main.mo`
Expected: one definition (around line 4892). Read 5 lines around it; confirm the pattern is:
- `public shared ({ caller }) func setXxx(p : Principal) : async ()`
- First line: `requireAdmin(caller);`
- Optional anonymous-principal guard via `Principal.isAnonymous(p)`
- Final line: assign `xxx := ?p;`

- [ ] **Step 3: Add the SOL-side setter next to `setHousePrincipal`**

Find the closing `};` of `setHousePrincipal` (around line 4898). Immediately after, insert:

```motoko
    /// M2: configure the SOL-side ponzi_math canister. null until set;
    /// while null, the SOL-side observer branch no-ops. Admin only.
    /// Mirrors the setHousePrincipal pattern (anonymous-principal guard
    /// + admin guard).
    public shared ({ caller }) func setPonziMathSolPrincipal(p : Principal) : async () {
        requireAdmin(caller);
        if (Principal.isAnonymous(p)) {
            Debug.trap("ponziMathSolPrincipal cannot be the anonymous principal");
        };
        ponziMathSolPrincipal := ?p;
    };
```

- [ ] **Step 4: Add the SOL-side helper next to `getPonziMath`**

Find `func getPonziMath() : PonziMathActor {` at line 1084. Immediately after its closing `};`, insert:

```motoko
    /// M2: returns the SOL-side ponzi_math actor, or null if not configured.
    /// Returning ?actor (instead of trapping like getPonziMath) lets the
    /// observer no-op on un-configured SOL while still trapping when the
    /// ICP path is mis-configured — ICP is required, SOL is optional.
    func getPonziMathSol() : ?PonziMathActor {
        switch (ponziMathSolPrincipal) {
            case (null) { null };
            case (?p) { ?(actor (Principal.toText(p)) : PonziMathActor) };
        };
    };
```

- [ ] **Step 5: Build check**

Run: `dfx build shenanigans --check --network=local 2>&1 | tail -20`
Expected: clean build. `getPonziMathSol` will be flagged as unused — that's fine, it gets consumed in Task 7.

- [ ] **Step 6: Commit**

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): add ponziMathSolPrincipal state + setter + helper

Adds the second ponzi_math canister principal slot (null by default;
admin sets via setPonziMathSolPrincipal post-deploy) and a
getPonziMathSol() helper that returns ?PonziMathActor — null until
the principal is configured. ICP-side path is unaffected; SOL-side
branch will be wired in Task 7."
```

---

## Task 5: Add SOL-side observer cursor and retry/miss state

**Files:**
- Modify: `shenanigans/main.mo` (state declarations near existing observer cursors)

Mirror the ICP-side observer's bookkeeping for the SOL path: cursor, backerSeen, retry counters, miss map.

- [ ] **Step 1: Extend the observer state block**

Find the existing observer-cursor block at lines 545-570. Replace the entire block (`// Observer cursors` through `var missedBackerMints = principalMap.empty<Text>();`) with:

```motoko
    // Observer cursors (ICP-side)
    var gameIdCursor : Nat = 0;                         // next unprocessed game id
    var backerSeen = principalMap.empty<BackerSeen>();  // cumulative ICP minted-for per backer

    // M2: Observer cursors (SOL-side). Namespaced separately from the
    // ICP cursors so the two sources can advance independently. Each
    // ponzi_math canister has its own gameId namespace, so a tick can
    // safely process game 0 on the SOL canister even after the ICP
    // canister already has games up to 50.
    var solGameIdCursor : Nat = 0;
    var solBackerSeen = principalMap.empty<BackerSeen>();

    // Observer lock to prevent concurrent ticks
    transient var observerRunning : Bool = false;
    var observerTimerId : ?Timer.TimerId = null;

    // Per-game mint retry counters. A failed mintWithEffects (#Err) increments;
    // a successful mint clears the entry. After MAX_MINT_RETRIES consecutive
    // failures, the observer gives up and advances past the game, recording it
    // in missedGameMints for admin inspection / manual retry. Transient: on
    // upgrade the counter resets to 0, which is fine — next tick re-tries.
    transient var gameMintRetries = natMap.empty<Nat>();
    transient let MAX_MINT_RETRIES : Nat = 10;  // ~100s @ 10s tick interval

    // Permanently-skipped game ids (cursor advanced past them after exhausting
    // retries). Stable so admin can see missed mints across upgrades and
    // manually retry via adminMint. Maps game.id → last error message.
    var missedGameMints = natMap.empty<Text>();

    // Same pattern for backer-delta mints. Keyed by backer principal because
    // backer rows don't have an id — backerSeen tracks "amount minted-for so far",
    // and a failed delta mint blocks the same principal on subsequent ticks.
    transient var backerMintRetries = principalMap.empty<Nat>();
    var missedBackerMints = principalMap.empty<Text>();

    // M2: SOL-side retry counters + miss map. Separate keys per source so
    // a failed SOL mint doesn't stall the ICP source and vice versa.
    transient var solGameMintRetries = natMap.empty<Nat>();
    var missedSolGameMints = natMap.empty<Text>();
    transient var solBackerMintRetries = principalMap.empty<Nat>();
    var missedSolBackerMints = principalMap.empty<Text>();
```

- [ ] **Step 2: Build check**

Run: `dfx build shenanigans --check --network=local 2>&1 | tail -20`
Expected: clean build. The four new fields are unused so far — that's expected.

- [ ] **Step 3: Commit**

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): add SOL-side observer cursor + retry/miss state

Mirrors the existing ICP-side observer bookkeeping with parallel
SOL-side fields: solGameIdCursor, solBackerSeen, solGameMintRetries
(transient), missedSolGameMints, solBackerMintRetries (transient),
missedSolBackerMints. All initialize empty/zero. ICP-side state is
untouched."
```

---

## Task 6: Add `denomination` field to `#signup` and `#roundResult` chat-item variants

**Files:**
- Modify: `shenanigans/main.mo` (ChatItemKind type, every existing call site that constructs `#signup` or `#roundResult`)

This is the type-shape change the V8 migration backfills. The compiler will catch every existing constructor call missing the new field — we fix them all to use `#icp` since they're the existing ICP-side paths.

- [ ] **Step 1: Update the `ChatItemKind` type**

Find `public type ChatItemKind = {` at line 278. Within that variant block, find the `#signup` and `#roundResult` arms and replace them.

The original lines are:
```motoko
        #signup : { newUser : Principal };
        #rankUp : { user : Principal; newRank : Text };
        #roundResult : { gameId : Nat; winner : Principal; winnerPpUnits : Nat };
```

Replace with:
```motoko
        // M2: denomination tags the asset side of the originating event
        // (#icp for the existing ponzi_math source, #sol for the new
        // ponzi_math_sol source).
        #signup : { newUser : Principal; denomination : Denomination };
        #rankUp : { user : Principal; newRank : Text };
        #roundResult : {
            gameId : Nat;
            winner : Principal;
            winnerPpUnits : Nat;
            denomination : Denomination;
        };
```

- [ ] **Step 2: Find every `#signup({...})` construction**

Run: `grep -nE "#signup\s*\(" shenanigans/main.mo`
Expected: one occurrence at line 1243 (`processNewGames`). That call site will get rewritten in Task 7 with the correct Denomination passed in. For now, we need it to compile, so update it to a placeholder that yields `#icp` (the existing behavior).

Change line 1243 from:
```motoko
                        let _ = appendChatItem(Principal.fromActor(Self), #signup({ newUser = game.player }));
```
to:
```motoko
                        let _ = appendChatItem(Principal.fromActor(Self), #signup({ newUser = game.player; denomination = #icp }));
```

- [ ] **Step 3: Find every `#roundResult({...})` construction**

Run: `grep -nE "#roundResult\s*\(" shenanigans/main.mo`
Expected: one occurrence at line 1282 (`processNewGames`). Update line 1282-1286 from:

```motoko
                        let _ = appendChatItem(Principal.fromActor(Self), #roundResult({
                            gameId = game.id;
                            winner = game.player;
                            winnerPpUnits = playerNet;
                        }));
```

to:

```motoko
                        let _ = appendChatItem(Principal.fromActor(Self), #roundResult({
                            gameId = game.id;
                            winner = game.player;
                            winnerPpUnits = playerNet;
                            denomination = #icp;
                        }));
```

(Both call sites get rewritten to use a parameterized `denomination` in Task 7; for now we hard-code `#icp` to keep the existing behavior bit-identical post-migration.)

- [ ] **Step 4: Build check**

Run: `dfx build shenanigans --check --network=local 2>&1 | tail -30`
Expected: clean build. If you see "field denomination missing in record" — you missed a constructor; the error message names the line. Add `denomination = #icp` to that line.

- [ ] **Step 5: Commit**

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): add Denomination to #signup + #roundResult chat items

ChatItemKind.#signup and ChatItemKind.#roundResult gain a
denomination field (Denomination = { #icp; #sol }). Existing call
sites in processNewGames are updated to hard-code #icp; Task 7
parameterizes them via the new Denomination argument. The V8
migration backfills #icp on every historical chat item (Task 2)."
```

---

## Task 7: Refactor `processNewGames` to accept a `Denomination` parameter

**Files:**
- Modify: `shenanigans/main.mo` (processNewGames body)

The refactor:
1. Add `denomination : Denomination` parameter.
2. Use `denomination` to select the rate fields and the eventId prefix.
3. **Critically:** keep the existing `game-N` and `signup-...` eventId formats for the `#icp` case so the PP ledger doesn't re-mint existing games. Only the `#sol` case gets the new `game-sol-N` and `signup-sol-...` prefixes.
4. Use `denomination` to select which canister to poll, which cursor to read/advance, and which retry/miss maps to update.
5. Pass `denomination` through to the chat-item constructors.

- [ ] **Step 1: Replace `processNewGames` with the parameterized version**

Find `func processNewGames() : async () {` at line 1220. Replace the entire function body (lines 1220-1324, ending at the `};` closing the function) with:

```motoko
    func processNewGames(denomination : Denomination) : async () {
        // Select the right ponzi_math source. ICP is required; SOL is
        // optional and no-ops while unconfigured.
        let ponziMathOpt : ?PonziMathActor = switch (denomination) {
            case (#icp) { ?getPonziMath() };
            case (#sol) { getPonziMathSol() };
        };
        let ponziMath = switch (ponziMathOpt) {
            case (?p) { p };
            case (null) { return };  // SOL not configured — no-op.
        };
        let games = try { await ponziMath.getAllGames() } catch (_) { [] };
        let sorted = Array.sort<PonziMathGameRecord>(games, func(a, b) = Nat.compare(a.id, b.id));
        // Choose the right cursor + rate fields + eventId prefix per denomination.
        // The ICP path keeps the historical 'game-N' / 'signup-...' eventId
        // shapes so PP ledger memo dedup is unaffected. The SOL path uses
        // a 'sol-' infix so the two namespaces can never collide.
        let cursor : Nat = switch (denomination) {
            case (#icp) { gameIdCursor };
            case (#sol) { solGameIdCursor };
        };
        for (game in sorted.vals()) {
            if (game.id >= cursor) {
                let ppPerUnit : Nat = switch (denomination, game.plan) {
                    case (#icp, #simple21Day) { mintConfig.simple21DayPpPerIcp };
                    case (#icp, #compounding15Day) { mintConfig.compounding15DayPpPerIcp };
                    case (#icp, #compounding30Day) { mintConfig.compounding30DayPpPerIcp };
                    case (#sol, #simple21Day) { mintConfig.simple21DayPpPerSol };
                    case (#sol, #compounding15Day) { mintConfig.compounding15DayPpPerSol };
                    case (#sol, #compounding30Day) { mintConfig.compounding30DayPpPerSol };
                };
                let baseUnits = icpFloatToPpUnits(game.amount, ppPerUnit);
                let cascadeUnits = baseUnits * mintConfig.cascadeInitialBps / 10_000;
                let playerNet : Nat = if (baseUnits > cascadeUnits) { baseUnits - cascadeUnits } else { 0 };
                let eventId = switch (denomination) {
                    case (#icp) { "game-" # Nat.toText(game.id) };
                    case (#sol) { "game-sol-" # Nat.toText(game.id) };
                };

                // Announce signup in chat unconditionally on first observation —
                // independent of whether the gift is enabled. This ensures the
                // #signup item fires even when signupGiftPp = 0.
                switch (principalMap.get(signupAnnouncedSet, game.player)) {
                    case (?_) {};
                    case (null) {
                        signupAnnouncedSet := principalMap.put(signupAnnouncedSet, game.player, Time.now());
                        let _ = appendChatItem(Principal.fromActor(Self), #signup({ newUser = game.player; denomination }));
                    };
                };

                // Signup gift — gated on first qualifying game record.
                // Gift itself goes through the deductive cascade (mint event).
                if (mintConfig.signupGiftPp > 0) {
                    switch (principalMap.get(signupGiftClaimed, game.player)) {
                        case (?_) {}; // already claimed
                        case (null) {
                            signupGiftClaimed := principalMap.put(signupGiftClaimed, game.player, Time.now());
                            let giftBase = ppToUnits(mintConfig.signupGiftPp);
                            let giftCascade = giftBase * mintConfig.cascadeInitialBps / 10_000;
                            let giftNet : Nat = if (giftBase > giftCascade) { giftBase - giftCascade } else { 0 };
                            // Signup-gift event id includes the denomination so a
                            // cross-pot user (joins ICP then later joins SOL) cannot
                            // accidentally double-claim the gift via ledger memo
                            // collision. Practically irrelevant today (signupGiftClaimed
                            // gates by principal), but cheap defense-in-depth.
                            let giftEventId = switch (denomination) {
                                case (#icp) { "signup-" # Principal.toText(game.player) };
                                case (#sol) { "signup-sol-" # Principal.toText(game.player) };
                            };
                            switch (await mintWithEffects(game.player, giftNet, giftEventId)) {
                                case (#Ok(_)) {
                                    await distributeDeductiveCascade(game.player, giftCascade, giftEventId);
                                };
                                case (#Err(msg)) {
                                    Debug.print("Signup-gift mint failed for " # giftEventId # ": " # msg);
                                };
                            };
                        };
                    };
                };

                let res = await mintWithEffects(game.player, playerNet, eventId);
                switch (res) {
                    case (#Ok(_)) {
                        await distributeDeductiveCascade(game.player, cascadeUnits, eventId);
                        // Track qualifying deposit for isActive() — observer is the
                        // single source of truth for activity timestamps.
                        if (game.amount >= 0.1) {
                            lastQualifyingDeposit := principalMap.put(lastQualifyingDeposit, game.player, Time.now());
                        };
                        switch (denomination) {
                            case (#icp) {
                                gameMintRetries := natMap.delete(gameMintRetries, game.id);
                                gameIdCursor := game.id + 1;
                            };
                            case (#sol) {
                                solGameMintRetries := natMap.delete(solGameMintRetries, game.id);
                                solGameIdCursor := game.id + 1;
                            };
                        };

                        let _ = appendChatItem(Principal.fromActor(Self), #roundResult({
                            gameId = game.id;
                            winner = game.player;
                            winnerPpUnits = playerNet;
                            denomination;
                        }));

                        let coin = Int.abs(Time.now()) % 7; // ~15%
                        if (coin == 0) {
                            switch (reginaldPickFor("roundResult")) {
                                case (?line) {
                                    let _ = appendChatItem(Principal.fromActor(Self), #reginald({ line; triggerKind = "roundResult" }));
                                };
                                case (null) {};
                            };
                        };
                    };
                    case (#Err(msg)) {
                        let attempts : Nat = switch (denomination) {
                            case (#icp) {
                                switch (natMap.get(gameMintRetries, game.id)) {
                                    case (?n) { n + 1 };
                                    case (null) { 1 };
                                };
                            };
                            case (#sol) {
                                switch (natMap.get(solGameMintRetries, game.id)) {
                                    case (?n) { n + 1 };
                                    case (null) { 1 };
                                };
                            };
                        };
                        if (attempts >= MAX_MINT_RETRIES) {
                            // Exhausted retries — record the miss and advance
                            // past this game so it doesn't block subsequent ones.
                            // Admin can call adminMint to compensate the player.
                            Debug.print("Giving up on " # eventId # " after "
                                # Nat.toText(attempts) # " attempts: " # msg);
                            switch (denomination) {
                                case (#icp) {
                                    missedGameMints := natMap.put(missedGameMints, game.id, msg);
                                    gameMintRetries := natMap.delete(gameMintRetries, game.id);
                                    gameIdCursor := game.id + 1;
                                };
                                case (#sol) {
                                    missedSolGameMints := natMap.put(missedSolGameMints, game.id, msg);
                                    solGameMintRetries := natMap.delete(solGameMintRetries, game.id);
                                    solGameIdCursor := game.id + 1;
                                };
                            };
                            // Fall through — continue to next game in the loop.
                        } else {
                            switch (denomination) {
                                case (#icp) {
                                    gameMintRetries := natMap.put(gameMintRetries, game.id, attempts);
                                };
                                case (#sol) {
                                    solGameMintRetries := natMap.put(solGameMintRetries, game.id, attempts);
                                };
                            };
                            Debug.print("Mint attempt " # Nat.toText(attempts)
                                # "/" # Nat.toText(MAX_MINT_RETRIES)
                                # " failed for " # eventId # ": " # msg);
                            return;  // Try again on next tick.
                        };
                    };
                };
            };
        };
    };
```

- [ ] **Step 2: Build check**

Run: `dfx build shenanigans --check --network=local 2>&1 | tail -30`
Expected: clean build.

If you see "missing field denomination": you missed one of the chat-item constructors. The new ones inside this function pass `denomination` from the function argument.

If you see "function expects 1 argument": the call sites in `observerTick` still call with no arguments. Task 9 fixes that — for now you can run a build with `--check` which won't link the call sites.

- [ ] **Step 3: Commit**

```bash
git add shenanigans/main.mo
git commit -m "refactor(shenanigans): parameterize processNewGames by Denomination

processNewGames now takes a Denomination argument and selects:
- which ponzi_math source (#icp -> getPonziMath; #sol -> getPonziMathSol)
- which gameIdCursor + retry/miss maps to read/advance
- which MintConfig PP-per-unit rate to apply
- which eventId prefix to use (game-N / signup-P stay ICP-side as-is;
  game-sol-N / signup-sol-P for SOL side, so PP-ledger memo dedup
  treats them as distinct events)
- which Denomination tag to pass to #signup and #roundResult chat items

SOL path no-ops while ponziMathSolPrincipal is null. ICP-side
behavior is unchanged."
```

---

## Task 8: Refactor `processBackerDeltas` to accept a `Denomination` parameter

**Files:**
- Modify: `shenanigans/main.mo` (processBackerDeltas body)

Same pattern as Task 7. Cursor: `backerSeen` vs `solBackerSeen`. Rate: `backerPpPerIcp` vs `backerPpPerSol`. EventId: `backer-...` (unchanged for ICP) vs `backer-sol-...`. Retry/miss maps: `backerMintRetries`/`missedBackerMints` vs `solBackerMintRetries`/`missedSolBackerMints`.

- [ ] **Step 1: Replace `processBackerDeltas` with the parameterized version**

Find `func processBackerDeltas() : async () {` at line 1326. Replace the entire function body (1326 through the closing `};`) with:

```motoko
    func processBackerDeltas(denomination : Denomination) : async () {
        let ponziMathOpt : ?PonziMathActor = switch (denomination) {
            case (#icp) { ?getPonziMath() };
            case (#sol) { getPonziMathSol() };
        };
        let ponziMath = switch (ponziMathOpt) {
            case (?p) { p };
            case (null) { return };  // SOL not configured — no-op.
        };
        let backers = try { await ponziMath.getBackerPositions() } catch (_) { [] };
        for (backer in backers.vals()) {
            let seenMap = switch (denomination) {
                case (#icp) { backerSeen };
                case (#sol) { solBackerSeen };
            };
            let seen : Float = switch (principalMap.get(seenMap, backer.owner)) {
                case (null) { 0.0 };
                case (?v) { v };
            };
            if (backer.amount > seen) {
                let delta : Float = backer.amount - seen;
                let ppPerUnit : Nat = switch (denomination) {
                    case (#icp) { mintConfig.backerPpPerIcp };
                    case (#sol) { mintConfig.backerPpPerSol };
                };
                let baseUnits = icpFloatToPpUnits(delta, ppPerUnit);
                let cascadeUnits = baseUnits * mintConfig.cascadeInitialBps / 10_000;
                let playerNet : Nat = if (baseUnits > cascadeUnits) { baseUnits - cascadeUnits } else { 0 };
                let eventId = switch (denomination) {
                    case (#icp) { "backer-" # Principal.toText(backer.owner) # "-" # Float.toText(backer.amount) };
                    case (#sol) { "backer-sol-" # Principal.toText(backer.owner) # "-" # Float.toText(backer.amount) };
                };

                let res = await mintWithEffects(backer.owner, playerNet, eventId);
                switch (res) {
                    case (#Ok(_)) {
                        await distributeDeductiveCascade(backer.owner, cascadeUnits, eventId);
                        if (delta >= 0.1) {
                            lastQualifyingDeposit := principalMap.put(lastQualifyingDeposit, backer.owner, Time.now());
                        };
                        switch (denomination) {
                            case (#icp) {
                                backerSeen := principalMap.put(backerSeen, backer.owner, backer.amount);
                                backerMintRetries := principalMap.delete(backerMintRetries, backer.owner);
                            };
                            case (#sol) {
                                solBackerSeen := principalMap.put(solBackerSeen, backer.owner, backer.amount);
                                solBackerMintRetries := principalMap.delete(solBackerMintRetries, backer.owner);
                            };
                        };
                    };
                    case (#Err(msg)) {
                        let retryMap = switch (denomination) {
                            case (#icp) { backerMintRetries };
                            case (#sol) { solBackerMintRetries };
                        };
                        let attempts = switch (principalMap.get(retryMap, backer.owner)) {
                            case (?n) { n + 1 };
                            case (null) { 1 };
                        };
                        if (attempts >= MAX_MINT_RETRIES) {
                            // Exhausted retries — record the miss and advance
                            // backerSeen so the same delta isn't retried forever.
                            Debug.print("Giving up on backer mint for "
                                # Principal.toText(backer.owner) # " at amount "
                                # Float.toText(backer.amount) # " after "
                                # Nat.toText(attempts) # " attempts: " # msg);
                            switch (denomination) {
                                case (#icp) {
                                    missedBackerMints := principalMap.put(missedBackerMints, backer.owner, msg);
                                    backerMintRetries := principalMap.delete(backerMintRetries, backer.owner);
                                    backerSeen := principalMap.put(backerSeen, backer.owner, backer.amount);
                                };
                                case (#sol) {
                                    missedSolBackerMints := principalMap.put(missedSolBackerMints, backer.owner, msg);
                                    solBackerMintRetries := principalMap.delete(solBackerMintRetries, backer.owner);
                                    solBackerSeen := principalMap.put(solBackerSeen, backer.owner, backer.amount);
                                };
                            };
                        } else {
                            switch (denomination) {
                                case (#icp) {
                                    backerMintRetries := principalMap.put(backerMintRetries, backer.owner, attempts);
                                };
                                case (#sol) {
                                    solBackerMintRetries := principalMap.put(solBackerMintRetries, backer.owner, attempts);
                                };
                            };
                            Debug.print("Backer mint attempt " # Nat.toText(attempts)
                                # "/" # Nat.toText(MAX_MINT_RETRIES)
                                # " failed for " # eventId # ": " # msg);
                        };
                    };
                };
            };
        };
    };
```

- [ ] **Step 2: Build check**

Run: `dfx build shenanigans --check --network=local 2>&1 | tail -30`
Expected: clean build. Same caveat as Task 7 — `observerTick`'s call site won't pass an argument yet; Task 9 fixes it.

- [ ] **Step 3: Commit**

```bash
git add shenanigans/main.mo
git commit -m "refactor(shenanigans): parameterize processBackerDeltas by Denomination

Same shape as processNewGames refactor: Denomination selects source
canister, cursor map (backerSeen / solBackerSeen), rate field
(backerPpPerIcp / backerPpPerSol), eventId prefix (backer- / backer-sol-),
and retry/miss maps. ICP-side behavior unchanged."
```

---

## Task 9: Update `observerTick` to call both ICP and SOL paths

**Files:**
- Modify: `shenanigans/main.mo` (observerTick body)

- [ ] **Step 1: Update `observerTick`**

Find `func observerTick() : async () {` at line 1203. Replace the function body (lines 1203-1218) with:

```motoko
    /// Single observer pass. Mints PP for new deposits and dealer top-ups
    /// from BOTH ponzi_math (ICP) and ponzi_math_sol (SOL). Each call to
    /// processNewGames / processBackerDeltas advances only its own
    /// denomination's cursor, so a failure on one side doesn't stall the
    /// other. The SOL-side calls no-op while ponziMathSolPrincipal is null.
    func observerTick() : async () {
        if (observerRunning) return;
        // Upgrade-safety: refuse to mint until seedMigrationV2 has
        // grandfathered existing players. Without this gate, the first
        // post-upgrade tick would treat every existing player as a brand-
        // new signup and mint them all the 500 PP gift.
        if (not bootstrapped) return;
        observerRunning := true;
        try {
            await processNewGames(#icp);
            await processNewGames(#sol);
            await processBackerDeltas(#icp);
            await processBackerDeltas(#sol);
        } catch (e) {
            Debug.print("Observer tick error: " # Error.message(e));
        };
        observerRunning := false;
    };
```

- [ ] **Step 2: Build check**

Run: `dfx build shenanigans --check --network=local 2>&1 | tail -30`
Expected: clean build, no warnings about unused functions.

- [ ] **Step 3: Commit**

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): observerTick polls both ICP and SOL sources

Each tick now invokes processNewGames(#icp), processNewGames(#sol),
processBackerDeltas(#icp), processBackerDeltas(#sol). SOL calls
no-op until ponziMathSolPrincipal is set (admin-only setter, see
Task 4). ICP-side path is unchanged."
```

---

## Task 10: Update `primeObserverCursors` to seed both sources

**Files:**
- Modify: `shenanigans/main.mo` (primeObserverCursors body)

The existing admin-only primer fast-forwards the cursors past existing games at cutover time so the observer doesn't try to retroactively mint for everything already on-chain. M2 adds a SOL-side seed step that's a no-op if `ponziMathSolPrincipal` is null.

- [ ] **Step 1: Update `primeObserverCursors`**

Find `public shared ({ caller }) func primeObserverCursors() : async () {` at line 1380. Replace the entire function body with:

```motoko
    /// One-shot catch-up primer. Admin only. Call immediately after the
    /// cutover upgrade completes, before unpausing user traffic. M2:
    /// also seeds the SOL-side cursors if ponziMathSolPrincipal is set
    /// (otherwise the SOL block is a no-op — admin can call this again
    /// after configuring the SOL principal to back-fill cursors).
    public shared ({ caller }) func primeObserverCursors() : async () {
        requireAdmin(caller);

        // ICP side — existing behavior.
        let ponziMathIcp = getPonziMath();
        let icpGames = await ponziMathIcp.getAllGames();
        var maxIcpId : Nat = 0;
        for (g in icpGames.vals()) { if (g.id >= maxIcpId) { maxIcpId := g.id + 1 } };
        gameIdCursor := maxIcpId;

        let icpBackers = await ponziMathIcp.getBackerPositions();
        for (b in icpBackers.vals()) {
            backerSeen := principalMap.put(backerSeen, b.owner, b.amount);
        };

        // SOL side — only run if configured. Safe to re-call this whole
        // function any time after setPonziMathSolPrincipal lands.
        switch (getPonziMathSol()) {
            case (null) {};
            case (?ponziMathSol) {
                let solGames = await ponziMathSol.getAllGames();
                var maxSolId : Nat = 0;
                for (g in solGames.vals()) { if (g.id >= maxSolId) { maxSolId := g.id + 1 } };
                solGameIdCursor := maxSolId;

                let solBackers = await ponziMathSol.getBackerPositions();
                for (b in solBackers.vals()) {
                    solBackerSeen := principalMap.put(solBackerSeen, b.owner, b.amount);
                };
            };
        };
    };
```

**Important:** for M2's mainnet deploy we do **NOT** want to prime the SOL cursors past game 0 — game 0 is exactly the deposit Task 14 will verify mints PP. The deploy procedure (Task 13) deliberately calls `primeObserverCursors` only on the ICP side path *before* setting the SOL principal, then sets the SOL principal, and lets the observer pick up game 0 from a starting cursor of 0. The new SOL block in `primeObserverCursors` is for future cutover scenarios (e.g., bulk SOL backfill), not the M2 first-light deploy.

- [ ] **Step 2: Build check**

Run: `dfx build shenanigans --check --network=local 2>&1 | tail -20`
Expected: clean build.

- [ ] **Step 3: Commit**

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): primeObserverCursors seeds SOL cursors when configured

ICP-side priming is unchanged. SOL-side block runs only if
ponziMathSolPrincipal is set, so calling primeObserverCursors before
SOL is configured is safe — the SOL cursors stay at 0 and the
observer picks up game 0 on first SOL tick after configuration.
Used by future cutover scenarios (bulk SOL backfill); M2 first-light
deploy deliberately skips SOL priming so game 0 mints normally."
```

---

## Task 11: Extend admin tooling — `getObserverStatus` + SOL rate setters

**Files:**
- Modify: `shenanigans/main.mo` (getObserverStatus return type + body, plus four new per-field SOL rate setters)

**Two changes in one task** (both touch the admin-tooling block around lines 4800-4940):

(a) Extend `getObserverStatus`'s return record with the SOL-side cursor + map size + principal slot, so the admin dashboard / deploy-verification flow can read SOL state at a glance.

(b) Add four per-field SOL-rate admin setters mirroring the existing ICP setters at lines 4855-4870 (`setSimple21DayPpPerIcp`, `setCompounding15DayPpPerIcp`, `setCompounding30DayPpPerIcp`, `setBackerPpPerIcp`). The V8 migration seeds the initial SOL rate values; these setters let admin retune them after deploy without another upgrade.

- [ ] **Step 1: Locate `getObserverStatus`**

Run: `grep -n "getObserverStatus\b" shenanigans/main.mo`
Expected: one definition (probably around line 4807) plus any call sites. Read the existing definition (the surrounding 20 lines).

- [ ] **Step 2: Extend the return record**

The existing return shape is:
```motoko
        gameIdCursor : Nat;
        backerSeenCount : Nat;
```

Find that record in the `getObserverStatus` function. Replace with:

```motoko
        gameIdCursor : Nat;
        backerSeenCount : Nat;
        // M2 (Solana chain fusion): SOL-side observer state.
        solGameIdCursor : Nat;
        solBackerSeenCount : Nat;
        ponziMathSolPrincipal : ?Principal;
```

In the same function's body, find the corresponding record-literal construction:
```motoko
            gameIdCursor;
            backerSeenCount = principalMap.size(backerSeen);
```

Replace with:

```motoko
            gameIdCursor;
            backerSeenCount = principalMap.size(backerSeen);
            solGameIdCursor;
            solBackerSeenCount = principalMap.size(solBackerSeen);
            ponziMathSolPrincipal;
```

- [ ] **Step 3: Add the four SOL-rate setters**

Find the existing setter block at lines 4855-4870. Locate the closing `};` of `setBackerPpPerIcp` (around line 4870). Immediately after that `};`, before the `setReferralBps` comment block, insert:

```motoko
    public shared ({ caller }) func setSimple21DayPpPerSol(v : Nat) : async () {
        requireAdmin(caller);
        mintConfig := { mintConfig with simple21DayPpPerSol = v };
    };
    public shared ({ caller }) func setCompounding15DayPpPerSol(v : Nat) : async () {
        requireAdmin(caller);
        mintConfig := { mintConfig with compounding15DayPpPerSol = v };
    };
    public shared ({ caller }) func setCompounding30DayPpPerSol(v : Nat) : async () {
        requireAdmin(caller);
        mintConfig := { mintConfig with compounding30DayPpPerSol = v };
    };
    public shared ({ caller }) func setBackerPpPerSol(v : Nat) : async () {
        requireAdmin(caller);
        mintConfig := { mintConfig with backerPpPerSol = v };
    };
```

(Pattern matches `setSimple21DayPpPerIcp` etc. exactly — `requireAdmin` then a record-update assignment, no extra guards. The MintConfig type already has these fields from Task 3 Step 2.)

- [ ] **Step 4: Build check**

Run: `dfx build shenanigans --check --network=local 2>&1 | tail -20`
Expected: clean build. If you see "field type mismatch" on `getObserverStatus`: check that the new field names in the return type match the names in the body construction. If you see "field not found" on one of the new setters: verify Task 3 Step 2's MintConfig edit landed (the four `*PerSol` fields).

- [ ] **Step 5: Commit**

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): admin tooling for SOL observer — status query + rate setters

getObserverStatus return record gains solGameIdCursor,
solBackerSeenCount, and ponziMathSolPrincipal so the admin dashboard
and deploy-verification flow can read SOL state at a glance.

Adds four per-field SOL rate setters (setSimple21DayPpPerSol,
setCompounding15DayPpPerSol, setCompounding30DayPpPerSol,
setBackerPpPerSol) mirroring the existing PerIcp setter pattern.
The V8 migration seeds initial values; these let admin retune
without another upgrade."
```

---

## Task 12: Local build clean + lint-style review

**Files:** none — verification only.

- [ ] **Step 1: Full local build**

Run: `dfx build shenanigans --check --network=local 2>&1 | tail -50`
Expected: no errors, no `[M0170]` compatibility errors, no missing-field errors. Warnings on unused imports are tolerable but rare.

- [ ] **Step 2: Grep for any lingering `game-` references missing namespacing**

Run: `grep -nE "\"game-\"|\"backer-\"|\"signup-\"" shenanigans/main.mo`
Expected: each occurrence is either inside `processNewGames` / `processBackerDeltas` already wrapped in the `switch (denomination)` block, or in admin tooling where the prefix doesn't matter (e.g., display strings). Eyeball each match.

If you find a non-denomination-aware `"game-"` event-id construction somewhere unexpected, that's a bug — it'll write SOL events with the ICP prefix and the ledger will reject them as duplicates. Fix and re-commit.

- [ ] **Step 3: Confirm no untracked test file or stray script**

Run: `git status`
Expected: clean working tree, only the commits from Tasks 2–11 visible in `git log main..HEAD --oneline`.

- [ ] **Step 4: No commit — verification only.**

---

## Task 13: Push branch + open draft PR for review

**Files:** none — PR creation.

- [ ] **Step 1: Push the branch**

Run: `git push -u origin feature/m2-shenanigans-observer`
Expected: branch created on remote.

- [ ] **Step 2: Open a DRAFT PR**

We open as draft because Tasks 14-16 (mainnet deploy + verification) gate the merge.

Run:
```bash
gh pr create --draft --title "feat(shenanigans): M2 Solana chain-fusion — observer polls ponzi_math_sol" --body "$(cat <<'EOF'
## Summary

- Observer learns to poll a second ponzi_math source (`ponzi_math_sol`, M1) alongside the existing ICP source. Each `observerTick` now invokes `processNewGames` and `processBackerDeltas` once per `Denomination = { #icp; #sol }`.
- Adds `MintConfig` fields `simple21DayPpPerSol = 6_000`, `compounding15DayPpPerSol = 12_000`, `compounding30DayPpPerSol = 18_000`, `backerPpPerSol = 120_000` (= 4_000 × 30 per spec 30× anchor). Admin-tunable.
- Adds `denomination` to `#signup` and `#roundResult` chat-item variants so the frontend can render `"Bob joined with 1.5 SOL"` vs `"Alice joined with 5 ICP"`. Pre-M2 chat items backfilled to `#icp` via V8 migration.
- Adds `ponziMathSolPrincipal` slot + admin setter (`setPonziMathSolPrincipal`). SOL branch is a no-op while the slot is null.
- Event IDs namespace cleanly: existing `game-N` / `backer-...` strings unchanged on the ICP path; new `game-sol-N` / `backer-sol-...` / `signup-sol-P` on the SOL path. PP-ledger memo dedup unaffected.
- `getObserverStatus` extended with `solGameIdCursor`, `solBackerSeenCount`, `ponziMathSolPrincipal` for deploy verification + admin dashboard.

## Migration

V8 in `shenanigans/migration.mo` — extends `MintConfig` with the four SOL-rate fields (defaulting `backerPpPerSol = old.backerPpPerIcp * 30`) and adds `denomination = #icp` to every historical `#signup` / `#roundResult` chat item. No data loss: every pre-M2 chat item is ICP-sourced by construction.

## Deploy plan

Mainnet only — `ponzi_math_sol` was deployed to mainnet in M1 (configured against Solana **devnet**). Sequence (also documented in memory `project_shenanigans_deploy_lineage`):

1. `dfx canister --network=ic stop shenanigans`
2. `dfx deploy shenanigans --network=ic` (triggers V8 migration)
3. `dfx canister --network=ic start shenanigans`
4. `dfx canister --network=ic call shenanigans setPonziMathSolPrincipal '(principal "spc6q-xyaaa-aaaac-qg2ma-cai")'`

## Test plan

- [ ] `dfx build shenanigans --check` clean from `feature/m2-shenanigans-observer`.
- [ ] V8 migration applies on mainnet upgrade (no `M0170` error in deploy log).
- [ ] Post-deploy, `getObserverStatus` shows `solGameIdCursor = 0` and `ponziMathSolPrincipal = ?spc6q-...`.
- [ ] One full observer tick (~10s) elapses; `getObserverStatus.solGameIdCursor` advances to 1.
- [ ] PP ledger shows a fresh mint to `tester1` with memo `game-sol-0` and amount = `simple21DayPpPerSol × 0.5 × (1 - cascadeInitialBps/10_000)` PP units. (3_000 base × 0.9 = 2_700 PP units net for tester1; 300 PP units cascaded.)
- [ ] An ICP-side game minted in the same tick window still shows up with the unchanged `game-N` memo format. (No regression on the ICP path.)
- [ ] Chat shows a `#signup` item for tester1 with `denomination = #sol` (visible via admin getRecentChat / getChatPage).

## Out of scope

- Frontend M2 (SIWS sign-in, `BuySOLFlyout`, deposit-address QR). Separate PR.
- Real-SOL mainnet (M3). `ponzi_math_sol` stays on Solana devnet for now.
- M1 follow-ups (gid=0 detector edge case, multi-send Inconsistent handling). Separate small session.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

Expected: PR URL returned. Note the URL for Task 16's update.

- [ ] **Step 3: No commit — PR open.**

---

## Task 14: Mainnet deploy dance (USER APPROVAL REQUIRED)

**Files:** none — deploy operation.

> **⚠️ STOP. Do not run this task without explicit user approval.**
> Memory `feedback_deploy_safety` records a prior data-loss incident from an unauthorized backend redeploy. The deploy commands in this task are gated by an interactive confirmation. Get user approval *for this specific task* before invoking any `dfx deploy` command.

- [ ] **Step 1: Surface the deploy plan to the user**

Tell the user: "Tasks 2-11 are committed and `dfx build --check` is clean. Ready to deploy shenanigans to mainnet with the V8 migration? This will trigger the stop / deploy / start dance and run the migration. Confirm with 'yes deploy m2' before I proceed."

Wait for explicit confirmation. If the user requests changes or asks questions, address them and re-confirm before running any deploy command.

- [ ] **Step 2: Snapshot pre-deploy observer state**

Run:
```bash
dfx canister --network=ic call shenanigans getObserverStatus 2>&1 | tee /tmp/obs-pre.txt
```
Expected: text output with `gameIdCursor = <some Nat>`, `backerSeenCount = <some Nat>`. Note both values — Task 15 confirms the ICP-side ones are unchanged or only advance forward (never reset).

- [ ] **Step 3: Stop shenanigans**

Run: `dfx canister --network=ic stop shenanigans`
Expected: `Stopping code for canister shenanigans`. Wait 5-10 seconds for any in-flight observer callbacks to drain.

- [ ] **Step 4: Deploy with the V8 migration**

Run:
```bash
dfx deploy shenanigans --network=ic 2>&1 | tee /tmp/deploy-m2.txt
```
Expected output excerpt:
- `Module hash <new sha>` (new wasm).
- No `[M0170]` lines. If you see `[M0170]`, the migration didn't catch a stable-state shape change — STOP, debug, do NOT proceed.
- `Upgraded code for canister shenanigans`.

If the deploy emits any warning about stable variables losing data, STOP and read the warning carefully. The V8 migration is supposed to be lossless except for the optional types we explicitly added.

- [ ] **Step 5: Start shenanigans**

Run: `dfx canister --network=ic start shenanigans`
Expected: `Started code for canister shenanigans`.

- [ ] **Step 6: Configure the SOL principal**

Run:
```bash
dfx canister --network=ic call shenanigans setPonziMathSolPrincipal '(principal "spc6q-xyaaa-aaaac-qg2ma-cai")'
```
Expected: `(())` (Motoko unit return).

- [ ] **Step 7: No commit — deploy is the change.**

---

## Task 15: Verify M2 end-to-end on mainnet

**Files:** none — observation + log capture.

- [ ] **Step 1: Snapshot post-deploy observer state**

Run:
```bash
dfx canister --network=ic call shenanigans getObserverStatus 2>&1 | tee /tmp/obs-post.txt
```
Expected:
- `gameIdCursor` ≥ value from `/tmp/obs-pre.txt` (forward-only).
- `backerSeenCount` ≥ value from `/tmp/obs-pre.txt`.
- `solGameIdCursor = 0` (fresh — SOL side hasn't ticked yet).
- `solBackerSeenCount = 0`.
- `ponziMathSolPrincipal = opt principal "spc6q-xyaaa-aaaac-qg2ma-cai"`.

If `gameIdCursor` or `backerSeenCount` *decreased*, the migration corrupted state. STOP and surface to the user.

- [ ] **Step 2: Wait one observer interval, then resnapshot**

Run:
```bash
sleep 15
dfx canister --network=ic call shenanigans getObserverStatus 2>&1 | tee /tmp/obs-post-tick.txt
```
Expected: `solGameIdCursor = 1` (game 0 processed). If still 0 after 30s, check `Debug.print` output in canister logs via `dfx canister --network=ic logs shenanigans 2>&1 | tail -50`.

- [ ] **Step 3: Confirm the PP mint for game 0 hit the ledger**

Run:
```bash
dfx canister --network=ic call pp_ledger get_account_transactions '(record { account = record { owner = principal "<tester1-principal>"; subaccount = null }; start = null; max_results = 5 })' 2>&1 | tee /tmp/pp-mint.txt
```
Replace `<tester1-principal>` with the value from `dfx canister --network=ic call ponzi_math_sol getAllGames` (the `player` field on the game with `id = 0`).

Expected: at least one mint with `memo = "game-sol-0"` and amount close to `2_700_00000000` units (PP has 8 decimals; 2_700 PP whole = 270_000_000_000 units… verify by reading the existing ICP-side mint amounts to confirm scale). Actual amount = `simple21DayPpPerSol × 0.5 (the SOL amount) × (10_000 - cascadeInitialBps) / 10_000` = `6_000 × 0.5 × 0.9 = 2_700` PP.

If memo is `game-0` (not `game-sol-0`), the prefix logic in `processNewGames` is wrong — STOP and debug.

- [ ] **Step 4: Confirm ICP-side path didn't regress**

Run:
```bash
dfx canister --network=ic call ponzi_math getAllGames 2>&1 | tail -10
```
Find any game with `id >= <pre-deploy gameIdCursor>`. Confirm its player got a `game-N` mint via the PP ledger (same `get_account_transactions` query, but with `memo = "game-<N>"` — no `sol` infix).

If a known ICP-side game *didn't* mint after the deploy, the ICP path is regressed — STOP and surface.

- [ ] **Step 5: Confirm chat shows the SOL-side signup**

Run:
```bash
dfx canister --network=ic call shenanigans getRecentChat '(50)' 2>&1 | grep -A 5 "<tester1-principal>"
```
Replace `<tester1-principal>` as in Step 3.

Expected: a `#signup` variant with `denomination = variant { sol }` for tester1, plus a `#roundResult` for the game 0 win with `denomination = variant { sol }`.

- [ ] **Step 6: Save the verification evidence**

The PR description's test-plan checkboxes (Task 13 Step 2) need to be checked off with evidence. Append the contents of `/tmp/obs-pre.txt`, `/tmp/obs-post.txt`, `/tmp/obs-post-tick.txt`, `/tmp/pp-mint.txt` to a comment on the PR using:

```bash
gh pr comment <pr-number> --body "$(cat <<'EOF'
## M2 verification on mainnet

### Observer state
Pre-deploy: $(cat /tmp/obs-pre.txt | head -10)
Post-deploy: $(cat /tmp/obs-post.txt | head -10)
Post-tick: $(cat /tmp/obs-post-tick.txt | head -10)

### PP mint (game-sol-0)
$(cat /tmp/pp-mint.txt | head -20)
EOF
)"
```

- [ ] **Step 7: No commit — verification only.**

---

## Task 16: Mark PR ready + update migration comment to past-tense

**Files:**
- Modify: `shenanigans/main.mo` (V8 migration comment becomes past-tense)

- [ ] **Step 1: Update the migration comment to past-tense**

In `shenanigans/main.mo`, find the V8 migration comment added in Task 3 Step 5. Replace the present-tense line with past-tense + the actual deploy date:

From:
```motoko
// Migration V8 (Solana chain fusion observer support: MintConfig gains
// *PerSol rates; ChatItemKind.#signup + #roundResult gain a denomination
// tag, backfilled to #icp on all pre-M2 chat items). Applied during the
// M2 deploy — see docs/superpowers/plans/2026-05-28-solana-chain-fusion-m2.md
// and Migration.runV8.
```

To:
```motoko
// Migration V8 (Solana chain fusion observer support: MintConfig gains
// *PerSol rates; ChatItemKind.#signup + #roundResult gain a denomination
// tag, backfilled to #icp on all pre-M2 chat items) was applied
// <YYYY-MM-DD from Task 14 deploy log>. See migration.mo for the
// historical migration record and Migration.runV8.
```

(Use the actual deploy date from `/tmp/deploy-m2.txt`'s timestamp.)

- [ ] **Step 2: Commit**

```bash
git add shenanigans/main.mo
git commit -m "docs(shenanigans): mark V8 migration as deployed

V8 migration block was attached to actor Self for the M2 deploy on
<YYYY-MM-DD>. Past-tense comment matches the project pattern (see
V6/V7 historical migration comments)."
```

- [ ] **Step 3: Push + mark PR ready for review**

Run:
```bash
git push origin feature/m2-shenanigans-observer
gh pr ready <pr-number>
```

- [ ] **Step 4: Surface to the user**

Tell the user: "M2 deployed to mainnet, verification passed, PR <#> marked ready for review. Frontend M2 work (SIWS + BuySOLFlyout + QR + withdrawal picker) and the M1 follow-ups (gid=0 detector edge case, multi-send Inconsistent handling) remain undisturbed for separate sessions."

- [ ] **Step 5: Update relevant memory pointers**

After PR merge (NOT before), the user may ask you to update memory:
- `project_shenanigans_deploy_lineage` — add the M2 V8 deploy date.
- `project_ponzi_math_sol_m1_state` — note that M2 observer wiring is now live.

Wait for user instruction before writing memory updates.
