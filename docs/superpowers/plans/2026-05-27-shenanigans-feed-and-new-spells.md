# Shenanigans Forward Plan — Feed Robustness, Cease & Desist Redesign, New Spells

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the live feed informative ("Bonsai Capital stole 47 PP from RatPoison Investments", "Whale.foo was renamed Bonsai Capital → Pre-Seed Baghead", "AoE drained 312 PP across 8 players"), invert Cease & Desist defaults to a pool-pick fast path with a premium pay-to-name toggle, ship three new spells (Tender Offer + the Stimulus Check / Bear Raid social pair), enable the "This Round" leaderboard filter, and delete dead leaderboard hooks.

**Architecture:** Extend `ShenaniganRecord` and the `#spellCast` `ChatItemKind` variant with optional outcome-detail fields (`ppDelta`, `affectedCount`, `renameDetail`, `shieldDeflected`). Forward-only: old records carry `null` for these fields and render in the existing terse format; new casts populate them and render the rich format. No state migration. New spells follow the existing `ShenaniganType` variant + admin-editable `ShenaniganConfig` pattern, with one-shot post_upgrade seeding to add the new config entries on the next deploy. "This Round" leaderboard adds a new per-round burn-tally map keyed by `currentRoundId`.

**Tech Stack:** Motoko (`shenanigans/main.mo`), TypeScript/React (frontend), pp_ledger canister for transfers/mints.

**Scope notes (decided 2026-05-27):**
- Insurance with Reginald is OUT — debt-adjacent, paused alongside the debt feature.
- The MLM rank ladder, bounties, Dokapon curses/events, and the dedicated public-feed UX layer (banter, streaks, daily digest) are OUT of this plan. Backlog memory entry `project_shenanigan_feature_backlog` carries them forward.
- Hero/Villain leaderboards (Stimulus/Bear Raid social layer) are OUT of v1 — included only as a follow-up note in Phase 4.

**Critical context for the implementer:**
- This repo has **no automated test suite**. Each task's verification step uses `dfx build`, `npm run build`, the icp CLI, or visual inspection on the dev server.
- The shenanigans canister mainnet deploy procedure requires the **stop → deploy → start** dance because in-flight async callbacks block `pre_upgrade`. See memory `shenanigans_deploy_lineage`.
- **DO NOT DEPLOY TO MAINNET** as part of executing this plan. Each phase's verification is local-build-only. The user controls mainnet deploys.
- The user-facing name for the "rename" spell is **Cease & Desist**. Internal identifier is `#renameSpell`. Don't rename the identifier (see project CLAUDE.md).
- Existing 11 spell IDs: 0=moneyTrickster (MEV Attack), 1=aoeSkim (Contagion), 2=renameSpell (Cease & Desist), 3=mintTaxSiphon (Trailing Commission), 4=downlineHeist (Crossline Poach), 5=magicMirror (Poison Pill), 6=ppBoosterAura (Yield Boost), 7=purseCutter (Bridge Exploit), 8=whaleRebalance (Wealth Tax), 9=downlineBoost (Override Bonus), 10=goldenName (Whitelisted). New IDs: 11=tenderOffer, 12=stimulusCheck, 13=bearRaid.

---

## File Structure

**Backend (Motoko):**
- `shenanigans/main.mo` — extend type defs (`ShenaniganRecord`, `ChatItemKind`, `ShenaniganType`), populate detail in `castShenanigan` + spell handlers, add new variants, add new state maps, add `getRoundBurnedLeaderboard` query, extend post_upgrade seeding.
- No new Motoko files needed.

**Frontend (TypeScript/React):**
- `frontend/src/declarations/shenanigans/` — regenerated after each backend change via `dfx generate shenanigans` then copying outputs.
- `frontend/src/components/Shenanigans/LiveFeedPanel.tsx` — extend `LiveFeedRow` to render the new detail.
- `frontend/src/components/trollbox/rows/SpellRow.tsx` — extend to render the new detail.
- `frontend/src/components/Shenanigans.tsx` — extend `SHEN_VARIANT_ORDER`, spell registry arrays, add Cease & Desist premium toggle in the cast confirmation modal.
- `frontend/src/components/HallOfFame.tsx` — wire the "This Round" toggle to the new endpoint.
- `frontend/src/hooks/useQueries.ts` — add `useGetRoundBurnedLeaderboard`; delete `useGetTopPonziPointsHolders` and `useGetHallOfFame`.
- `frontend/src/components/trollbox/spellFlavorDefaults.ts` — no new entries needed (spells share the global success/fail/backfire pools).

---

## Phase 1 — Feed Robustness Schema + Rendering

The headline gap: `ShenaniganOutcomeDetail` is computed on every cast and returned to the caster, but `ShenaniganRecord` and the `#spellCast` chat item only persist the bare outcome tag. This phase fixes the storage and rendering for new casts. Old records keep rendering in the terse format because the new fields are optional.

### Task 1: Extend `ShenaniganRecord` with optional detail fields

**Files:**
- Modify: `shenanigans/main.mo:75-83`

- [ ] **Step 1: Update the `ShenaniganRecord` type**

Find the existing block at line 75:

```motoko
public type ShenaniganRecord = {
    id : Nat;
    user : Principal;
    shenaniganType : ShenaniganType;
    target : ?Principal;
    outcome : ShenaniganOutcome;
    timestamp : Int;
    cost : Float;
};
```

Replace it with:

```motoko
public type ShenaniganRecord = {
    id : Nat;
    user : Principal;
    shenaniganType : ShenaniganType;
    target : ?Principal;
    outcome : ShenaniganOutcome;
    timestamp : Int;
    cost : Float;
    // Optional outcome detail captured at cast time. `ppDelta` is the net
    // PP-unit change attributable to the spell effect (excludes the cost
    // burn — positive for caster gain, negative for caster loss).
    // `affectedCount` is the number of distinct principals materially hit
    // (AoE/Whale Rebalance set > 1). `renameDetail` is populated only on
    // successful pool-pick Cease & Desist casts. `shieldDeflected` is true
    // when a hostile spell landed but a Poison Pill consumed the effect.
    // All are null for pre-2026-05-27 records.
    ppDelta : ?Int;
    affectedCount : ?Nat;
    renameDetail : ?{ oldName : Text; newName : Text };
    shieldDeflected : ?Bool;
};
```

- [ ] **Step 2: Run `dfx build shenanigans` and verify type check passes**

Run: `dfx build shenanigans 2>&1 | tail -20`
Expected: build succeeds; no type errors. Existing record construction at line 2078 will fail because it doesn't provide the new fields — that's expected and fixed in Task 3.

- [ ] **Step 3: Do not commit yet — Task 3 lands the full chain together**

### Task 2: Extend `#spellCast` ChatItemKind with the same fields

**Files:**
- Modify: `shenanigans/main.mo:249-255`

- [ ] **Step 1: Update the `#spellCast` variant body**

Find the existing block at line 249:

```motoko
#spellCast : {
    castId : Nat;
    caster : Principal;
    shenaniganType : ShenaniganType;
    target : ?Principal;
    outcome : ShenaniganOutcome;
};
```

Replace it with:

```motoko
#spellCast : {
    castId : Nat;
    caster : Principal;
    shenaniganType : ShenaniganType;
    target : ?Principal;
    outcome : ShenaniganOutcome;
    // Forward-only detail fields — same semantics as on ShenaniganRecord.
    // Old chat items have null here and render in the terse format.
    ppDelta : ?Int;
    affectedCount : ?Nat;
    renameDetail : ?{ oldName : Text; newName : Text };
    shieldDeflected : ?Bool;
};
```

- [ ] **Step 2: Run `dfx build shenanigans` and verify type check**

Run: `dfx build shenanigans 2>&1 | tail -20`
Expected: build still failing on record construction sites. Task 3 fixes them.

### Task 3: Populate detail fields in `castShenanigan`

**Files:**
- Modify: `shenanigans/main.mo:2063-2098`

- [ ] **Step 1: Capture shield-deflected flag from `applySuccessEffect`**

The existing handlers (`applySuccessEffect` at line 2144, return type `{ ppDeltaCaster : Int; affectedTarget : ?Principal; affectedCount : Nat }`) need to also return a shield-deflected flag. Update the return type and each return site.

Change the function signature at line 2152:

```motoko
) : async { ppDeltaCaster : Int; affectedTarget : ?Principal; affectedCount : Nat; shieldDeflected : Bool } {
```

Then update every existing `return { ppDeltaCaster = ...; affectedTarget = ...; affectedCount = ... }` site (lines ~2161, 2165, 2176, 2179, 2209, 2214, 2243, 2250, 2254, 2268, 2275, 2313, 2318, 2320, 2342, 2359 and any others — search for `affectedCount` in the function body).

For the shield-deflected sites (where `consumeShieldIfActive(t)` returned true) — currently they return `{ ppDeltaCaster = 0; affectedTarget = ?t; affectedCount = 0 }`. Update to `shieldDeflected = true`:

```motoko
if (consumeShieldIfActive(t)) {
    return { ppDeltaCaster = 0; affectedTarget = ?t; affectedCount = 0; shieldDeflected = true };
};
```

For every OTHER return site (no shield consumed), append `shieldDeflected = false`:

```motoko
return { ppDeltaCaster = amount; affectedTarget = ?t; affectedCount = 1; shieldDeflected = false };
```

The grep to verify completeness: `grep -n "affectedCount = " /Users/robertripley/coding/musicalchairs/shenanigans/main.mo` — every line must also set `shieldDeflected`.

- [ ] **Step 2: Update the `#fail` arm of `castShenanigan`**

Find the existing `#fail` arm at line 2070:

```motoko
case (#fail) {
    { ppDeltaCaster = 0; affectedTarget = null; affectedCount = 0 };
};
```

Change to:

```motoko
case (#fail) {
    { ppDeltaCaster = 0; affectedTarget = null; affectedCount = 0; shieldDeflected = false };
};
```

- [ ] **Step 3: Update the local `detail` type binding**

At line 2063, the type annotation reads:

```motoko
let detail : { ppDeltaCaster : Int; affectedTarget : ?Principal; affectedCount : Nat } = switch (outcome) {
```

Change to:

```motoko
let detail : { ppDeltaCaster : Int; affectedTarget : ?Principal; affectedCount : Nat; shieldDeflected : Bool } = switch (outcome) {
```

- [ ] **Step 4: Populate the new fields on `ShenaniganRecord` construction (line ~2078)**

Replace the existing block:

```motoko
let newShenanigan : ShenaniganRecord = {
    id = castId;
    user = caller;
    shenaniganType;
    target;
    outcome;
    timestamp = Time.now();
    cost = actualCostFloat;
};
```

With:

```motoko
let newShenanigan : ShenaniganRecord = {
    id = castId;
    user = caller;
    shenaniganType;
    target;
    outcome;
    timestamp = Time.now();
    cost = actualCostFloat;
    ppDelta = ?detail.ppDeltaCaster;
    affectedCount = ?detail.affectedCount;
    renameDetail = null; // populated by the rename handler — see Task 4
    shieldDeflected = ?detail.shieldDeflected;
};
```

- [ ] **Step 5: Populate the new fields on `#spellCast` chat item (line ~2091)**

Replace the existing block:

```motoko
let _ = appendChatItem(
    Principal.fromActor(Self),
    #spellCast({
        castId;
        caster = caller;
        shenaniganType;
        target;
        outcome;
    })
);
```

With:

```motoko
let _ = appendChatItem(
    Principal.fromActor(Self),
    #spellCast({
        castId;
        caster = caller;
        shenaniganType;
        target;
        outcome;
        ppDelta = ?detail.ppDeltaCaster;
        affectedCount = ?detail.affectedCount;
        renameDetail = null; // wired up in Task 4
        shieldDeflected = ?detail.shieldDeflected;
    })
);
```

- [ ] **Step 6: Update the `ShenaniganOutcomeDetail` return value to include the new flag**

The function returns its `ShenaniganOutcomeDetail` to the caster (line 2128). Update the return type (line 68) and the return value to include `shieldDeflected`:

At line 68:

```motoko
public type ShenaniganOutcomeDetail = {
    outcome : ShenaniganOutcome;
    ppDeltaCaster : Int;
    affectedTarget : ?Principal;
    affectedCount : Nat;
    shieldDeflected : Bool;
};
```

At line 2128:

```motoko
{
    outcome;
    ppDeltaCaster = detail.ppDeltaCaster;
    affectedTarget = detail.affectedTarget;
    affectedCount = detail.affectedCount;
    shieldDeflected = detail.shieldDeflected;
};
```

- [ ] **Step 7: Build the canister**

Run: `dfx build shenanigans 2>&1 | tail -20`
Expected: build succeeds with no warnings or errors.

- [ ] **Step 8: Commit**

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): persist outcome detail on cast records + chat items

Add optional ppDelta, affectedCount, renameDetail, and shieldDeflected
fields to ShenaniganRecord and the #spellCast chat item variant. Capture
shield-deflected flag from applySuccessEffect so the feed can distinguish
a successful cast that was blocked by a Poison Pill from one that landed.

Forward-only: old records have null for the new fields and continue to
render in the terse 'Caster cast SpellName on Target — landed clean'
format. No state migration."
```

### Task 4: Capture rename detail on `#renameSpell` pool-pick path

For v1, only pool-pick renames (the default after Cease & Desist redesign in Phase 2) populate `renameDetail`. Premium pay-to-name casts leave `renameDetail` null in v1; the caster's choice is private until the target's name is read elsewhere. We can iterate later.

**Files:**
- Modify: `shenanigans/main.mo:2211-2246` (#renameSpell handler in applySuccessEffect)
- Modify: `shenanigans/main.mo:2063-2098` (castShenanigan, to plumb the rename detail through)

- [ ] **Step 1: Add a display-name reader helper**

Search for an existing helper that resolves a Principal to a current display name (it should consult `customDisplayNames` first, then fall through to a stored profile name). Grep for `customDisplayNames` and look for a function that wraps it. If none exists, add at the top of `main.mo`'s helpers section (search for `pickRenameName` to find a good neighbor):

```motoko
/// Resolve a principal's current effective display name. Order:
///   1. customDisplayNames (rename-spell overlay, current within expiry)
///   2. (stored user-set name from getUserProfile, if a per-canister
///      registry exists — wire up as available; otherwise omit step 2)
///   3. "Player." # short-principal — matches frontend default
/// Used to snapshot the target's "old name" at rename-spell cast time
/// so the feed can show "renamed X → Y".
func effectiveDisplayName(p : Principal) : Text {
    let now = Time.now();
    switch (principalMap.get(customDisplayNames, p)) {
        case (?entry) {
            if (entry.expiresAt > now) { return entry.name };
        };
        case null {};
    };
    // Fallback: short principal. The frontend renders "Player.<short>"
    // for unnamed principals; we mirror that here.
    let full = Principal.toText(p);
    let short = if (Text.size(full) > 8) {
        Text.fromIter(Iter.take(full.chars(), 8))
    } else { full };
    "Player." # short;
};
```

If the `Iter` import is missing, add `import Iter "mo:base/Iter";` at the top of main.mo (search for existing imports to confirm whether it's already there).

- [ ] **Step 2: Update `#renameSpell` to return rename detail**

The current handler at line 2211 returns the same `{ ppDeltaCaster; affectedTarget; affectedCount; shieldDeflected }` shape. We need it to additionally return the rename detail. Two paths:

Path A (cleaner): extend the success-effect return type to include `renameDetail : ?{ oldName : Text; newName : Text }`. Update every return site to set it to `null` except the rename-spell pool-pick site.

Path B (smaller): use a separate `?{ oldName : Text; newName : Text }` `var` declared at the top of `castShenanigan`, mutated only by the rename-spell handler via a closure or by changing the handler signature. More awkward.

Go with Path A. At line 2152:

```motoko
) : async { ppDeltaCaster : Int; affectedTarget : ?Principal; affectedCount : Nat; shieldDeflected : Bool; renameDetail : ?{ oldName : Text; newName : Text } } {
```

Update every `return { ... }` in `applySuccessEffect` to add `renameDetail = null` — same find-and-add pattern as Task 3 Step 1. Use grep to enumerate: `grep -n "shieldDeflected = " shenanigans/main.mo`.

- [ ] **Step 3: Update the `#renameSpell` handler to populate rename detail**

Current code at line 2211-2246. The handler creates a `pendingRenames` slot and waits for the caster to pick (or auto-commits with pool-pick after 5min). For v1 of this feature, the immediate-pool-pick path runs only after Phase 2 (Cease & Desist redesign). For now, in this task, populate `renameDetail` on the existing **auto-commit-of-prior-pending-slot** path (line 2225-2230) — when a caster casts again before committing, the prior slot auto-commits to a pool-pick name. That's a pool-pick rename we should capture in `renameDetail`.

Replace lines 2223-2233:

```motoko
switch (principalMap.get(pendingRenames, caster)) {
    case (?prior) {
        if (Time.now() < prior.expiresAt) {
            customDisplayNames := principalMap.put(customDisplayNames, prior.target, {
                name = pickRenameName();
                expiresAt = nowTs + renameDurationNs;
            });
        };
    };
    case null {};
};
```

With:

```motoko
switch (principalMap.get(pendingRenames, caster)) {
    case (?prior) {
        if (Time.now() < prior.expiresAt) {
            // Auto-committing the prior slot — this is a pool-pick rename.
            // The caster ran out of time on the previous cast; pick from pool.
            // (We do NOT capture renameDetail for this *previous* cast here
            // because the chat item for that cast was already appended with
            // null renameDetail. The user-visible rename still happens; the
            // feed just doesn't carry the detail. A future iteration could
            // emit a follow-up #renameCommitted chat item.)
            customDisplayNames := principalMap.put(customDisplayNames, prior.target, {
                name = pickRenameName();
                expiresAt = nowTs + renameDurationNs;
            });
        };
    };
    case null {};
};
```

The actual `renameDetail` population happens in the new immediate-pool-pick path landing in Phase 2, Task 9. This task just plumbs the return type.

- [ ] **Step 4: Build the canister**

Run: `dfx build shenanigans 2>&1 | tail -20`
Expected: build succeeds.

- [ ] **Step 5: Plumb `renameDetail` from `applySuccessEffect` into the record + chat item**

In `castShenanigan` around line 2063 (the `detail` switch result) and line 2078 (the record construction), `detail` now contains a `renameDetail` field. Update the type binding at line 2063:

```motoko
let detail : { ppDeltaCaster : Int; affectedTarget : ?Principal; affectedCount : Nat; shieldDeflected : Bool; renameDetail : ?{ oldName : Text; newName : Text } } = switch (outcome) {
```

For the `#fail` arm (line 2070):

```motoko
case (#fail) {
    { ppDeltaCaster = 0; affectedTarget = null; affectedCount = 0; shieldDeflected = false; renameDetail = null };
};
```

Then in the record construction (line 2078), replace `renameDetail = null;` (from Task 3 Step 4) with:

```motoko
renameDetail = detail.renameDetail;
```

And in the chat item construction (line 2091), replace `renameDetail = null;` with:

```motoko
renameDetail = detail.renameDetail;
```

- [ ] **Step 6: Build and commit**

Run: `dfx build shenanigans 2>&1 | tail -20`
Expected: build succeeds.

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): plumb rename-detail through outcome capture

Extend the applySuccessEffect return type with optional renameDetail
{ oldName, newName }, so the rename-spell success handler can later
populate it for pool-pick casts (wired in Phase 2). For now every
existing handler returns renameDetail = null and the field is plumbed
end-to-end into ShenaniganRecord and the #spellCast chat item."
```

### Task 5: Regenerate frontend declarations

**Files:**
- Modify: `frontend/src/declarations/shenanigans/shenanigans.did`
- Modify: `frontend/src/declarations/shenanigans/shenanigans.did.d.ts`
- Modify: `frontend/src/declarations/shenanigans/shenanigans.did.js`
- Modify: `frontend/src/declarations/shenanigans/index.ts` (may not need changes)

- [ ] **Step 1: Run dfx generate against the local replica**

Make sure local dfx is running first if not already: `dfx start --background 2>&1 | tail -5` (or skip if already running).

Run: `dfx generate shenanigans 2>&1 | tail -10`
Expected: regenerates `.did`, `.did.d.ts`, and `.did.js` under the dfx-managed output dir (typically `.dfx/local/canisters/shenanigans/` or similar — confirm with `find .dfx -name "shenanigans.did" -type f` if unclear).

- [ ] **Step 2: Copy the regenerated declarations into the frontend tree**

```bash
cp .dfx/local/canisters/shenanigans/shenanigans.did frontend/src/declarations/shenanigans/
cp .dfx/local/canisters/shenanigans/shenanigans.did.d.ts frontend/src/declarations/shenanigans/
cp .dfx/local/canisters/shenanigans/shenanigans.did.js frontend/src/declarations/shenanigans/
```

If those paths don't match, find them: `find . -path ./node_modules -prune -o -name "shenanigans.did*" -print | grep -v node_modules`.

- [ ] **Step 3: Verify the frontend type-checks**

Run: `cd frontend && npm run build 2>&1 | tail -30`
Expected: build succeeds. The new optional fields on `ShenaniganRecord` and the `#spellCast` variant should be visible in `shenanigans.did.d.ts`.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/declarations/shenanigans/
git commit -m "chore(declarations): regenerate shenanigans declarations for outcome detail"
```

### Task 6: Render PP delta and affected count in `LiveFeedRow`

**Files:**
- Modify: `frontend/src/components/Shenanigans/LiveFeedPanel.tsx:6-49`

- [ ] **Step 1: Add a PP-formatting helper next to `variantKey`**

Insert above the existing `variantKey` declaration (line 12):

```typescript
// Convert PP units (1 PP = 100_000_000 units) to a display string with up
// to 2 decimals when needed. Used by the live feed to render the ppDelta
// captured on each cast record.
function formatPp(units: bigint | number): string {
  const n = typeof units === 'bigint' ? Number(units) : units;
  const pp = n / 100_000_000;
  if (Number.isInteger(pp)) return pp.toLocaleString();
  return pp.toLocaleString(undefined, { maximumFractionDigits: 2 });
}
```

- [ ] **Step 2: Render outcome detail in `LiveFeedRow`**

Replace the existing `LiveFeedRow` body (lines 15-49) with:

```tsx
function LiveFeedRow({ record, spellName, spellIcon }: LiveFeedRowProps) {
  const casterName = useDisplayName(record.user);
  const isCasterGolden = useIsGolden(record.user);
  const target = record.target[0] ?? null;
  const targetName = useDisplayName(target);
  const isTargetGolden = useIsGolden(target);
  const outcomeKey = variantKey(record.outcome);

  // Forward-only detail fields. Old records have empty arrays here and
  // render in the terse format below the spell name. New records show
  // the rich detail line.
  const shieldDeflected = record.shieldDeflected?.[0] ?? false;
  const ppDelta = record.ppDelta?.[0] ?? null;
  const affected = record.affectedCount?.[0] ?? null;
  const renameDetail = record.renameDetail?.[0] ?? null;

  const outcomeLabel =
    shieldDeflected ? 'DEFLECTED' : outcomeKey.toUpperCase();
  const outcomeColor =
    shieldDeflected ? 'mc-text-muted' :
    outcomeKey === 'success' ? 'mc-text-green' :
    outcomeKey === 'fail' ? 'mc-text-danger' :
    'mc-text-purple';

  let detailLine: React.ReactNode = null;
  if (renameDetail) {
    detailLine = (
      <div className="mc-text-muted truncate">
        renamed <span className="mc-text-primary">{renameDetail.oldName}</span>
        {' → '}
        <span className="mc-text-primary">{renameDetail.newName}</span>
      </div>
    );
  } else if (shieldDeflected) {
    detailLine = <div className="mc-text-muted">shield blocked the effect</div>;
  } else if (ppDelta !== null && ppDelta !== 0n && ppDelta !== 0) {
    const ppNum = typeof ppDelta === 'bigint' ? ppDelta : BigInt(ppDelta);
    const sign = ppNum > 0n ? '+' : '';
    const sourceText = affected !== null && Number(affected) > 1
      ? ` across ${Number(affected)} players`
      : '';
    detailLine = (
      <div className="mc-text-muted">
        <span className={ppNum > 0n ? 'mc-text-green' : 'mc-text-danger'}>
          {sign}{formatPp(ppNum)} PP
        </span>
        {sourceText}
      </div>
    );
  }

  return (
    <div className="mc-card p-2 text-xs space-y-1">
      <div className="flex items-center justify-between gap-2">
        {isCasterGolden ? (
          <GoldenName name={casterName || 'Anon'} isGolden={true} className="font-bold truncate" />
        ) : (
          <span className="font-bold mc-text-primary truncate">{casterName || 'Anon'}</span>
        )}
        <span className={`font-bold flex-shrink-0 ${outcomeColor}`}>{outcomeLabel}</span>
      </div>
      <div className="mc-text-dim flex items-center gap-1 min-w-0">
        <span className="flex-shrink-0">{spellIcon}</span>
        <span className="truncate">{spellName}</span>
        {target ? (
          isTargetGolden ? (
            <span className="mc-text-muted truncate"> → <GoldenName name={targetName} isGolden={true} /></span>
          ) : (
            <span className="mc-text-muted truncate"> → {targetName}</span>
          )
        ) : null}
      </div>
      {detailLine}
    </div>
  );
}
```

- [ ] **Step 3: Build the frontend**

Run: `cd frontend && npm run build 2>&1 | tail -20`
Expected: build succeeds.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/Shenanigans/LiveFeedPanel.tsx
git commit -m "feat(feed): render PP delta, shield-deflect, and rename detail in live feed

The Live Feed row now renders the optional outcome detail captured on
ShenaniganRecord. Old records without detail continue to render in the
existing terse format (caster, spell name, target, outcome tag). New
records show:
  - rename detail: 'renamed X → Y'
  - shield-deflected: 'DEFLECTED' tag + 'shield blocked the effect'
  - PP delta: '+47 PP' or '-312 PP across 8 players' (signed, colored)"
```

### Task 7: Render the same detail in `SpellRow` (trollbox)

**Files:**
- Modify: `frontend/src/components/trollbox/rows/SpellRow.tsx:26-69`

- [ ] **Step 1: Update SpellRow to render the new detail**

Replace the entire body of `SpellRow` (everything from line 26 onward):

```tsx
function formatPp(units: bigint | number): string {
  const n = typeof units === 'bigint' ? Number(units) : units;
  const pp = n / 100_000_000;
  if (Number.isInteger(pp)) return pp.toLocaleString();
  return pp.toLocaleString(undefined, { maximumFractionDigits: 2 });
}

export default function SpellRow({ item }: Props) {
  if (!('spellCast' in item.kind)) return null;
  const cast = item.kind.spellCast;
  const userName = useDisplayName(cast.caster);
  const target = cast.target?.[0] ?? null;
  const targetName = useDisplayName(target);
  const isCasterGolden = useIsGolden(cast.caster);
  const isTargetGolden = useIsGolden(target);
  const { data: configs = [] } = useGetShenaniganConfigs();

  if (item.deleted) return <div className="px-3 py-1 text-zinc-500 italic text-xs">[removed by Management]</div>;

  const shieldDeflected = cast.shieldDeflected?.[0] ?? false;
  const ppDelta = cast.ppDelta?.[0] ?? null;
  const affected = cast.affectedCount?.[0] ?? null;
  const renameDetail = cast.renameDetail?.[0] ?? null;

  const outcomeText =
    shieldDeflected ? 'deflected' :
    'success' in cast.outcome ? 'landed clean' :
    'backfire' in cast.outcome ? 'backfired' : 'fizzled';
  const outcomeColor =
    shieldDeflected ? 'text-zinc-400' :
    'success' in cast.outcome ? 'text-emerald-300' :
    'backfire' in cast.outcome ? 'text-red-400' : 'text-zinc-400';
  const variantId = SHEN_VARIANT_ORDER.indexOf(variantKey(cast.shenaniganType) as typeof SHEN_VARIANT_ORDER[number]);
  const spellName = configs.find(c => Number(c.id) === variantId)?.name ?? 'a spell';

  let detailSuffix: React.ReactNode = null;
  if (renameDetail) {
    detailSuffix = (
      <span className="text-zinc-400">
        {' — '}
        <span className="text-zinc-200">{renameDetail.oldName}</span>
        {' → '}
        <span className="text-zinc-200">{renameDetail.newName}</span>
      </span>
    );
  } else if (ppDelta !== null && ppDelta !== 0n && ppDelta !== 0) {
    const ppNum = typeof ppDelta === 'bigint' ? ppDelta : BigInt(ppDelta);
    const sign = ppNum > 0n ? '+' : '';
    const acrossText = affected !== null && Number(affected) > 1
      ? ` across ${Number(affected)}`
      : '';
    detailSuffix = (
      <span className={`${ppNum > 0n ? 'text-emerald-300' : 'text-red-400'}`}>
        {' '}({sign}{formatPp(ppNum)} PP{acrossText})
      </span>
    );
  }

  return (
    <div className="px-3 py-1 text-xs">
      <span className={`${outcomeColor} font-medium`}>
        ✨{' '}
        {isCasterGolden ? (
          <GoldenName name={userName} isGolden={true} className="font-medium" />
        ) : (
          userName
        )}
      </span>
      <span className="text-zinc-400"> cast </span>
      <span className="text-zinc-200 font-medium">{spellName}</span>
      {target ? (
        <>
          <span className="text-zinc-400"> on </span>
          {isTargetGolden ? (
            <GoldenName name={targetName} isGolden={true} className="font-medium" />
          ) : (
            <span className="text-zinc-200 font-medium">{targetName}</span>
          )}
        </>
      ) : null}
      <span className="text-zinc-400"> — </span>
      <span className={outcomeColor}>{outcomeText}</span>
      <span className="text-zinc-400">.</span>
      {detailSuffix}
    </div>
  );
}
```

- [ ] **Step 2: Build the frontend**

Run: `cd frontend && npm run build 2>&1 | tail -20`
Expected: build succeeds.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/components/trollbox/rows/SpellRow.tsx
git commit -m "feat(trollbox): render outcome detail in spell-cast rows

Trollbox spell rows now show PP-delta, rename-from-to, and shield-deflect
suffix when the cast record has the optional detail fields populated."
```

### Task 8: Verify Phase 1 against the local dev server

**Files:** (no edits — manual verification)

- [ ] **Step 1: Start the local replica + frontend dev server**

Run: `dfx canister install shenanigans --mode reinstall --network local 2>&1 | tail -5` to reset the canister state with the new schema, then start the dev server.

Actually, before reinstall, check whether the user wants reinstall — `--mode reinstall` wipes state. If keeping state, use `dfx deploy shenanigans` (regular upgrade). For this verification round, reinstall is fine since the user said forward-only and there's no local data of value.

```bash
dfx canister install shenanigans --mode reinstall --network local
cd frontend && npm run dev &
```

- [ ] **Step 2: Cast a spell that should populate the new detail**

Open the dev UI. Cast Money Trickster on another player. Watch the Live Feed and Trollbox:

Expected — Live Feed row shows something like:
```
You                            SUCCESS
✨ MEV Attack → Anon
+12 PP
```

Expected — Trollbox row:
```
✨ You cast MEV Attack on Anon — landed clean. (+12 PP)
```

- [ ] **Step 3: Cast an AOE spell (Contagion)**

Expected — Live Feed row:
```
You                            SUCCESS
✨ Contagion
+47 PP across 6 players
```

(Numbers vary depending on number of other holders and effect roll.)

- [ ] **Step 4: Cast Cease & Desist (target with a known starting name)**

For this iteration, the modal still appears (pool-pick happens via the auto-commit path of the prior slot; Phase 2 makes pool-pick the default). For this verification, cast Cease & Desist twice in a row to trigger the prior-slot auto-commit. Watch the feed for the **second** cast.

(Even with the auto-commit codepath, the first cast's chat item went out with `renameDetail = null` — the rename detail isn't captured for the auto-commit path in v1. So we won't see the rename detail in the feed yet. That lands in Phase 2 Task 9 when pool-pick is the default.)

- [ ] **Step 5: Cast a hostile spell on a shielded target (Poison Pill on yourself first, then Money Trickster on yourself from another principal)**

Expected — Live Feed:
```
Attacker                       DEFLECTED
✨ MEV Attack → You
shield blocked the effect
```

- [ ] **Step 6: Report Phase 1 complete**

No commit on this step — verification only.

---

## Phase 2 — Cease & Desist Pool-Default + Pay-to-Name Premium

Invert the defaults: the standard cast picks a name from the pool instantly and applies it. A new premium toggle costs +400 PP and unlocks the existing pending-rename modal flow. Forward-only — no migration of existing pending slots; they remain on the legacy flow until they expire naturally.

### Task 9: Add `premiumRename` parameter to `castShenanigan`

**Files:**
- Modify: `shenanigans/main.mo:1953` (`castShenanigan` signature)
- Modify: `shenanigans/main.mo:2211-2246` (`#renameSpell` handler)

- [ ] **Step 1: Update castShenanigan signature**

Find the function declaration around line 1953:

```motoko
public shared ({ caller }) func castShenanigan(shenaniganType : ShenaniganType, target : ?Principal) : async ShenaniganOutcomeDetail {
```

Change to:

```motoko
public shared ({ caller }) func castShenanigan(shenaniganType : ShenaniganType, target : ?Principal, premiumRename : Bool) : async ShenaniganOutcomeDetail {
```

(Single new bool param. Default-on-old-callers will fail compilation client-side, which is what we want — Task 10 updates the frontend.)

- [ ] **Step 2: Add the premium-rename surcharge to the cost**

Define a constant near the top of `main.mo` (search for `let PP_UNIT_SCALE` or similar PP-related constants for a neighbor):

```motoko
/// Surcharge in whole PP added to the rolled outcome cost when the caster
/// opts to pick the rename name themselves rather than accepting a pool pick.
let PREMIUM_RENAME_SURCHARGE_PP : Nat = 400;
```

Then in `castShenanigan`, after the cost is rolled but before the burn (search for `actualBurnedUnits` and trace upward to where the outcome cost is computed — typically right after `determineOutcomeWithMod`), add:

```motoko
// Apply premium-rename surcharge if applicable.
let costAfterSurcharge : Float = if (shenaniganType == #renameSpell and premiumRename) {
    rolledCost + Float.fromInt(PREMIUM_RENAME_SURCHARGE_PP)
} else { rolledCost };
```

Then use `costAfterSurcharge` in place of `rolledCost` everywhere downstream in this function. (Find every reference to the locally-bound cost variable and replace.)

If the cost variable is named differently (e.g. `nominalCost` or `costToBurn`), update accordingly — the verification step is the `dfx build` after.

- [ ] **Step 3: Build to validate**

Run: `dfx build shenanigans 2>&1 | tail -20`
Expected: build succeeds.

- [ ] **Step 4: Don't commit yet — Task 10 lands the full premium flow**

### Task 10: Branch `#renameSpell` between immediate-pool-pick and pending-slot

**Files:**
- Modify: `shenanigans/main.mo:2211-2246` (`#renameSpell` handler in `applySuccessEffect`)

- [ ] **Step 1: Plumb `premiumRename` flag into `applySuccessEffect`**

The signature at line 2144 needs a new parameter. Find:

```motoko
func applySuccessEffect(
    shenaniganType : ShenaniganType,
    config : ShenaniganConfig,
    caster : Principal,
    target : ?Principal,
    _casterBal : Nat,
    targetBal : Nat,
    castId : Nat,
) : async { ... } {
```

Update to:

```motoko
func applySuccessEffect(
    shenaniganType : ShenaniganType,
    config : ShenaniganConfig,
    caster : Principal,
    target : ?Principal,
    _casterBal : Nat,
    targetBal : Nat,
    castId : Nat,
    premiumRename : Bool,
) : async { ... } {
```

Then find every call site (only one expected — in `castShenanigan` around line 2055-2065). Update to pass `premiumRename`.

- [ ] **Step 2: Replace the `#renameSpell` handler body**

Replace the existing handler (lines 2211-2246):

```motoko
case (#renameSpell) {
    switch (target) {
        case (null) {
            return { ppDeltaCaster = 0; affectedTarget = null; affectedCount = 0; shieldDeflected = false; renameDetail = null };
        };
        case (?t) {
            let renameDurationNs : Int = config.duration * 3_600_000_000_000;
            // Snapshot the target's name BEFORE we mutate customDisplayNames.
            let oldName = effectiveDisplayName(t);

            if (not premiumRename) {
                // POOL-PICK FAST PATH (new default).
                // Auto-commit any prior pending slot the caster has open, then
                // skip slot creation entirely and apply the rename immediately.
                switch (principalMap.get(pendingRenames, caster)) {
                    case (?prior) {
                        if (Time.now() < prior.expiresAt) {
                            customDisplayNames := principalMap.put(customDisplayNames, prior.target, {
                                name = pickRenameName();
                                expiresAt = nowTs + renameDurationNs;
                            });
                        };
                    };
                    case null {};
                };
                let pooledName = pickRenameName();
                customDisplayNames := principalMap.put(customDisplayNames, t, {
                    name = pooledName;
                    expiresAt = nowTs + renameDurationNs;
                });
                return {
                    ppDeltaCaster = 0;
                    affectedTarget = ?t;
                    affectedCount = 1;
                    shieldDeflected = false;
                    renameDetail = ?{ oldName; newName = pooledName };
                };
            };

            // PREMIUM PATH (legacy slot flow).
            // Auto-commit prior pending slot if any, then stash a fresh slot.
            switch (principalMap.get(pendingRenames, caster)) {
                case (?prior) {
                    if (Time.now() < prior.expiresAt) {
                        customDisplayNames := principalMap.put(customDisplayNames, prior.target, {
                            name = pickRenameName();
                            expiresAt = nowTs + renameDurationNs;
                        });
                    };
                };
                case null {};
            };
            let fiveMinNs : Int = 300_000_000_000;
            pendingRenames := principalMap.put(pendingRenames, caster, {
                target = t;
                expiresAt = nowTs + fiveMinNs;
            });
            // v1: premium renames don't populate renameDetail (caster's
            // private flex — picked later via setPendingRenameName).
            return {
                ppDeltaCaster = 0;
                affectedTarget = ?t;
                affectedCount = 1;
                shieldDeflected = false;
                renameDetail = null;
            };
        };
    };
};
```

- [ ] **Step 3: Build the canister**

Run: `dfx build shenanigans 2>&1 | tail -20`
Expected: build succeeds. Any reference to the old function signature is now fixed.

- [ ] **Step 4: Commit**

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): invert Cease & Desist defaults to pool-pick + premium toggle

Standard Cease & Desist casts now immediately pick a name from the
rename pool and apply it — no pending-rename modal. The feed captures
both old and new names on these casts.

A new boolean param 'premiumRename' on castShenanigan switches to the
legacy pending-slot flow at the cost of a +400 PP surcharge. The caster
gets to pick the exact name via the existing rename modal.

v1: only pool-pick (non-premium) renames populate renameDetail. Premium
renames leave renameDetail null in the chat item (caster's private flex)."
```

### Task 11: Add the premium toggle to the cast confirmation modal

**Files:**
- Modify: `frontend/src/components/Shenanigans.tsx:401-452` (cast confirmation handler)
- Modify: `frontend/src/components/Shenanigans.tsx` — wherever the cast confirmation modal JSX is (search for `confirmOpen` or the modal that triggers `handleConfirmCast`).
- Modify: `frontend/src/hooks/useQueries.ts:713` (`useCastShenanigan`)

- [ ] **Step 1: Update `useCastShenanigan` to accept the new flag**

Find `useCastShenanigan` around line 713 of `useQueries.ts`. Its current `mutateAsync` signature accepts `{ shenaniganType, target }`. Extend to `{ shenaniganType, target, premiumRename }` with a default of `false`:

```typescript
export function useCastShenanigan() {
  const actor = useShenaniganActor();
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ shenaniganType, target, premiumRename = false }: {
      shenaniganType: ShenaniganType;
      target?: [] | [Principal];
      premiumRename?: boolean;
    }) => {
      if (!actor) throw new Error('Actor not ready');
      return actor.castShenanigan(shenaniganType, target ?? [], premiumRename);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['shenanigans'] });
      queryClient.invalidateQueries({ queryKey: ['ponziPoints'] });
    },
  });
}
```

(The exact existing mutation body may differ — preserve the existing onSuccess wiring, just add the new param.)

- [ ] **Step 2: Add a premium-toggle state in the Shenanigans component**

Find the existing state declarations near the top of `Shenanigans()` around line 227+ (grep for `useState`). Add:

```typescript
const [premiumRenameToggle, setPremiumRenameToggle] = useState(false);
```

- [ ] **Step 3: Reset the toggle when the modal opens/closes**

Find where `setConfirmOpen` is called. When the modal closes, reset the toggle:

```typescript
// On modal close, reset the premium toggle so a fresh cast starts clean.
useEffect(() => {
  if (!confirmOpen) setPremiumRenameToggle(false);
}, [confirmOpen]);
```

- [ ] **Step 4: Add the toggle UI inside the confirmation modal**

Find the confirmation modal JSX (search for `confirmOpen &&` or for the spell name + "Cast" button in `Shenanigans.tsx`). Conditional on the selected spell being Cease & Desist (`selectedShenanigan?.id === 2`), render:

```tsx
{selectedShenanigan?.id === 2 && (
  <label className="flex items-start gap-2 mt-3 text-xs cursor-pointer">
    <input
      type="checkbox"
      checked={premiumRenameToggle}
      onChange={(e) => setPremiumRenameToggle(e.target.checked)}
      className="mt-0.5"
    />
    <span>
      <span className="font-medium mc-text-primary">Pick the name yourself</span>
      <span className="mc-text-muted"> (+400 PP)</span>
      <span className="block mc-text-muted">
        {premiumRenameToggle
          ? 'On success, a modal opens to type the exact name.'
          : 'On success, a name is pulled from the pool instantly.'}
      </span>
    </span>
  </label>
)}
```

- [ ] **Step 5: Update the cost preview**

If the modal shows a cost preview, find where `selectedShenanigan.costSuccess` is rendered. Adjust the preview to add 400 when the toggle is on and the spell is Cease & Desist. Search for the cost preview text in the modal — common patterns: `{selectedShenanigan.costSuccess} PP` or `cost.toString()`.

Example wrapping:

```tsx
const previewCost = selectedShenanigan.id === 2 && premiumRenameToggle
  ? selectedShenanigan.costSuccess + 400
  : selectedShenanigan.costSuccess;
```

- [ ] **Step 6: Pass `premiumRename` to the mutation in `handleConfirmCast`**

Around line 406 in `Shenanigans.tsx`, find:

```typescript
const detail = await castShenanigan.mutateAsync({ shenaniganType: selectedShenanigan.type, target: selectedTarget });
```

Change to:

```typescript
const detail = await castShenanigan.mutateAsync({
  shenaniganType: selectedShenanigan.type,
  target: selectedTarget,
  premiumRename: selectedShenanigan.id === 2 && premiumRenameToggle,
});
```

- [ ] **Step 7: Update the post-success branch**

Around line 415, the existing logic opens the rename modal only on `renameSpell` success. Update to ONLY open the modal on premium-rename success:

```typescript
const isRenameSuccess = outcome === 'success' && selectedShenanigan.id === 2 /* renameSpell */;
const isPremiumRename = isRenameSuccess && premiumRenameToggle;
```

Then change the existing condition that opens the rename modal:

```typescript
if (isPremiumRename && targetPrincipalText) {
  // Premium path — caster picks the exact name.
  setRenamePrompt({ targetPrincipal: targetPrincipalText });
} else if (isWhitelistedSuccess) {
  setWhitelistedFanfareOpen(true);
} else {
  // Includes pool-pick rename — show the outcome toast with the new name
  // already applied. (The toast doesn't display the new name in v1; the
  // feed and the target's displayed name are the affirmation.)
  ...existing toast code...
}
```

- [ ] **Step 8: Build the frontend**

Run: `cd frontend && npm run build 2>&1 | tail -20`
Expected: build succeeds.

- [ ] **Step 9: Commit**

```bash
git add frontend/src/components/Shenanigans.tsx frontend/src/hooks/useQueries.ts
git commit -m "feat(shenanigans): pool-default + premium pay-to-name toggle in Cease & Desist UI

The cast confirmation modal shows a 'Pick the name yourself (+400 PP)'
checkbox when Cease & Desist is selected. Off (default): instant pool-pick,
no follow-up modal, the feed shows 'renamed X → Y'. On: the existing
pending-rename modal opens after a successful cast and the caster picks
the exact name, at +400 PP surcharge."
```

### Task 12: Verify Phase 2 against the local dev server

- [ ] **Step 1: Reinstall the canister + restart the frontend**

```bash
dfx canister install shenanigans --mode reinstall --network local
cd frontend && npm run dev
```

- [ ] **Step 2: Cast Cease & Desist on a known target with the toggle OFF**

Expected: no modal opens after success. The target's display name changes immediately. The Live Feed shows:

```
You                            SUCCESS
✨ Cease & Desist → TargetOriginalName
renamed TargetOriginalName → SomePooledName
```

- [ ] **Step 3: Cast Cease & Desist on a known target with the toggle ON**

Expected: the modal opens after success exactly like the legacy flow. Type a name, lock it in. The Live Feed shows the cast row WITHOUT a rename detail line:

```
You                            SUCCESS
✨ Cease & Desist → TargetOriginalName
```

The target's name updates to the custom one.

- [ ] **Step 4: Verify the cost preview shows +400 only when toggle is on**

Open the modal for Cease & Desist. Default state: cost preview shows the rolled cost (e.g. 10 PP). Toggle on: cost preview shows 410 PP.

- [ ] **Step 5: Report Phase 2 complete — no commit**

---

## Phase 3 — Tender Offer Spell

A whale-only acquisition spell. Cost 500/100/300, odds 35/50/15. Success transfers the target's entire PP balance to caster and locks the target out of casting any spell for 24h. Failure: caster paid the premium, target unscathed. Backfire: target receives 3× the cost as poison-pill compensation; caster is locked out of casting Tender Offer specifically for 7 days. Pre-cast gate: target balance must be ≤ 50% of caster's balance.

### Task 13: Add `#tenderOffer` to the ShenaniganType variant

**Files:**
- Modify: `shenanigans/main.mo:41-53` (`ShenaniganType` variant)

- [ ] **Step 1: Add the variant**

Replace lines 41-53:

```motoko
public type ShenaniganType = {
    #moneyTrickster;
    #aoeSkim;
    #renameSpell;
    #mintTaxSiphon;
    #downlineHeist;
    #magicMirror;
    #ppBoosterAura;
    #purseCutter;
    #whaleRebalance;
    #downlineBoost;
    #goldenName;
    #tenderOffer;
};
```

- [ ] **Step 2: Add new state maps for the lockouts**

Search `main.mo` for the existing state declarations (typically `var cooldowns = ...` or similar — look around lines 320-360 where `shenaniganConfigs` is declared at line 341). Add:

```motoko
/// Principal -> nanosecond-precision deadline. While `now < deadline` the
/// principal cannot cast *any* spell (acquired-by-tender-offer lockout).
/// Set to now + 24h on every successful Tender Offer against the principal.
var acquiredLockUntil = principalMap.empty<Int>();

/// Principal -> nanosecond-precision deadline. While `now < deadline` the
/// principal cannot cast *Tender Offer specifically* (post-backfire
/// reputational cooldown). Set to now + 7d after every Tender Offer
/// backfire by the caster.
var tenderOfferBackfireLockUntil = principalMap.empty<Int>();
```

- [ ] **Step 3: Verify these maps will persist across upgrades**

In a persistent actor, top-level `var` declarations of stable types persist by default. `PrincipalMap<Int>` should be stable. Run `dfx build shenanigans 2>&1 | tail -20` — expected: build succeeds.

- [ ] **Step 4: Don't commit yet — Tasks 14-16 together**

### Task 14: Add the default config entry for Tender Offer

**Files:**
- Modify: `shenanigans/main.mo:1376-1392` (`defaultConfigs` array)
- Modify: post_upgrade or related seeding logic (search for `defaultConfigs`)

- [ ] **Step 1: Add the entry**

Append to the `defaultConfigs` array at line 1387 (after the `Whitelisted` entry):

```motoko
{ id = 11; name = "Tender Offer"; description = "Make a tender offer for a smaller player's entire position. They get taken private. Their cap table integrates into yours."; backfireDescription = ?"The target gets 3x your cost as poison-pill compensation, and you can't cast Tender Offer for 7 days."; costSuccess = 500.0; costFailure = 100.0; costBackfire = 300.0; successOdds = 35; failureOdds = 50; backfireOdds = 15; duration = 0; cooldown = 0; effectValues = [50.0]; castLimit = 0; backgroundColor = "#fff0ea" },
```

The `effectValues = [50.0]` encodes the 50% target-balance gate (target must be ≤ 50% of caster balance). The handler reads this value.

- [ ] **Step 2: Add a one-shot seed for the new config on upgrade**

If the canister has been deployed before, the `defaultConfigs` loop only runs on a fresh `shenaniganConfigs` map. New IDs (11+) won't appear. Add a one-shot seed: after `shenaniganConfigs := natMap.put(...)` in the init loop, also add an upgrade-time check.

Find the canister's `post_upgrade` hook (search for `system func post_upgrade` or simply `post_upgrade` in `main.mo`). If none exists, add one near the bottom of the actor body:

```motoko
system func postupgrade() {
    // One-shot seed: ensure new spell configs are present after upgrade.
    // Idempotent — re-adds only if the id is missing.
    let newConfigs : [ShenaniganConfig] = [
        // Tender Offer (id=11) — added 2026-05-27
        { id = 11; name = "Tender Offer"; description = "Make a tender offer for a smaller player's entire position. They get taken private. Their cap table integrates into yours."; backfireDescription = ?"The target gets 3x your cost as poison-pill compensation, and you can't cast Tender Offer for 7 days."; costSuccess = 500.0; costFailure = 100.0; costBackfire = 300.0; successOdds = 35; failureOdds = 50; backfireOdds = 15; duration = 0; cooldown = 0; effectValues = [50.0]; castLimit = 0; backgroundColor = "#fff0ea" },
        // Phase 4 will append Stimulus Check (id=12) and Bear Raid (id=13).
    ];
    for (cfg in newConfigs.vals()) {
        switch (natMap.get(shenaniganConfigs, cfg.id)) {
            case (?_existing) { /* leave admin-tuned config in place */ };
            case null {
                shenaniganConfigs := natMap.put(shenaniganConfigs, cfg.id, cfg);
            };
        };
    };
};
```

If a `postupgrade` already exists, merge the new seeding logic into it.

- [ ] **Step 3: Build to validate**

Run: `dfx build shenanigans 2>&1 | tail -20`
Expected: build succeeds.

### Task 15: Implement Tender Offer success/failure/backfire handlers

**Files:**
- Modify: `shenanigans/main.mo:2144+` (`applySuccessEffect`)
- Modify: `shenanigans/main.mo:2438+` (`applyBackfireEffect`)

- [ ] **Step 1: Add the success handler arm**

Append to the `switch (shenaniganType)` in `applySuccessEffect` (find a good place near the end of the existing arms — after `case (#goldenName)`):

```motoko
case (#tenderOffer) {
    switch (target) {
        case (null) {
            return { ppDeltaCaster = 0; affectedTarget = null; affectedCount = 0; shieldDeflected = false; renameDetail = null };
        };
        case (?t) {
            // Pre-cast gate ALREADY enforced at the top of castShenanigan
            // (see Task 16). Reaching here means the target is acquirable.
            if (consumeShieldIfActive(t)) {
                return { ppDeltaCaster = 0; affectedTarget = ?t; affectedCount = 0; shieldDeflected = true; renameDetail = null };
            };
            // Transfer the target's entire balance to the caster.
            let amount = targetBal;
            switch (await chipTransfer(t, caster, amount, memo)) {
                case (#Ok(_)) {
                    // Acquired-lockout: target can't cast any spell for 24h.
                    let oneDay : Int = 24 * 3600 * 1_000_000_000;
                    acquiredLockUntil := principalMap.put(acquiredLockUntil, t, nowTs + oneDay);
                    return { ppDeltaCaster = amount; affectedTarget = ?t; affectedCount = 1; shieldDeflected = false; renameDetail = null };
                };
                case (#Err(_)) {
                    return { ppDeltaCaster = 0; affectedTarget = ?t; affectedCount = 0; shieldDeflected = false; renameDetail = null };
                };
            };
        };
    };
};
```

- [ ] **Step 2: Add the backfire handler arm**

Find `applyBackfireEffect` at line 2438. It has its own `switch (shenaniganType)` and per-spell arms. Append:

```motoko
case (#tenderOffer) {
    switch (target) {
        case (null) {};
        case (?t) {
            // Backfire: caster pays target 3x the spell cost as poison-pill
            // compensation, AND caster is locked out of casting Tender Offer
            // for 7 days.
            let compensationPp : Nat = Int.abs(Float.toInt(config.costBackfire * 3.0));
            let compensationUnits = ppToUnits(compensationPp);
            let _ = await chipTransfer(caster, t, compensationUnits, memo);
            let sevenDays : Int = 7 * 24 * 3600 * 1_000_000_000;
            tenderOfferBackfireLockUntil := principalMap.put(tenderOfferBackfireLockUntil, caster, nowTs + sevenDays);
        };
    };
};
```

(If `nowTs` isn't bound at this point in `applyBackfireEffect`, declare it locally: `let nowTs = Time.now();`.)

- [ ] **Step 3: Build to validate**

Run: `dfx build shenanigans 2>&1 | tail -20`
Expected: build succeeds.

### Task 16: Enforce Tender Offer pre-cast gates

**Files:**
- Modify: `shenanigans/main.mo:1953+` (`castShenanigan`)

- [ ] **Step 1: Add the acquired-lockout check at the top of `castShenanigan`**

Right after the principal check and before the cost roll (search for the existing checks like `assert(...)` or balance lookup near the top of the function body), add:

```motoko
// Acquired-lockout: a recently-acquired target cannot cast ANY spell
// until the 24h post-acquisition integration period ends.
switch (principalMap.get(acquiredLockUntil, caller)) {
    case (?deadline) {
        if (Time.now() < deadline) {
            throw Error.reject("You are locked out of casting (recently acquired by tender offer). Try again later.");
        };
    };
    case null {};
};
```

(If `Error` isn't imported, add `import Error "mo:base/Error";` at the top.)

- [ ] **Step 2: Add the Tender-Offer-specific backfire-lockout check**

Right after the acquired-lockout check:

```motoko
// Tender Offer post-backfire lockout: 7d cooldown on Tender Offer
// specifically (other spells unaffected).
if (shenaniganType == #tenderOffer) {
    switch (principalMap.get(tenderOfferBackfireLockUntil, caller)) {
        case (?deadline) {
            if (Time.now() < deadline) {
                throw Error.reject("Tender Offer is locked out (recent backfire). Try a different spell.");
            };
        };
        case null {};
    };
};
```

- [ ] **Step 3: Add the 50% target-balance gate for Tender Offer**

After both lockout checks, conditional on Tender Offer:

```motoko
// Tender Offer requires target balance <= 50% of caster's. Whales only.
if (shenaniganType == #tenderOffer) {
    let casterBal = await ppLedger.icrc1_balance_of({ owner = caller; subaccount = null });
    let halfCasterBal = casterBal / 2;
    switch (target) {
        case (null) {
            throw Error.reject("Tender Offer requires a target.");
        };
        case (?t) {
            let targetBalCheck = await ppLedger.icrc1_balance_of({ owner = t; subaccount = null });
            if (targetBalCheck > halfCasterBal) {
                throw Error.reject("Target's PP balance must be at most 50% of yours for Tender Offer.");
            };
        };
    };
};
```

(Cross-reference: the existing balance-check pattern in `castShenanigan` may already exist — if so, lift the existing `casterBal`/`targetBal` variables and use them rather than re-querying.)

- [ ] **Step 4: Build and commit Phase 3 backend changes**

Run: `dfx build shenanigans 2>&1 | tail -20`
Expected: build succeeds.

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): add Tender Offer spell

New ShenaniganType #tenderOffer (id=11), cost 500/100/300, odds 35/50/15.

- Success: transfers target's entire PP balance to caster + locks target
  out of casting any spell for 24h ('acquired' status).
- Failure: caster's premium is burned; target unscathed.
- Backfire: target receives 3x backfire cost as poison-pill compensation
  + caster locked out of casting Tender Offer for 7 days.
- Pre-cast gate: target balance must be <= 50% of caster's balance.

Two new state maps (acquiredLockUntil, tenderOfferBackfireLockUntil)
gate both ends. The acquired-lockout blocks any-spell casting; the
backfire-lockout blocks Tender Offer specifically."
```

### Task 17: Wire Tender Offer into the frontend

**Files:**
- Modify: `frontend/src/declarations/shenanigans/` (regenerate)
- Modify: `frontend/src/components/Shenanigans.tsx:25-87` (spell registry arrays)
- Modify: `frontend/src/components/trollbox/rows/SpellRow.tsx:10-13` (`SHEN_VARIANT_ORDER`)

- [ ] **Step 1: Regenerate declarations**

```bash
dfx generate shenanigans
cp .dfx/local/canisters/shenanigans/shenanigans.did frontend/src/declarations/shenanigans/
cp .dfx/local/canisters/shenanigans/shenanigans.did.d.ts frontend/src/declarations/shenanigans/
cp .dfx/local/canisters/shenanigans/shenanigans.did.js frontend/src/declarations/shenanigans/
```

- [ ] **Step 2: Extend SHEN_VARIANT_ORDER in SpellRow**

Modify `frontend/src/components/trollbox/rows/SpellRow.tsx:10-13`:

```typescript
const SHEN_VARIANT_ORDER = [
  'moneyTrickster', 'aoeSkim', 'renameSpell', 'mintTaxSiphon', 'downlineHeist',
  'magicMirror', 'ppBoosterAura', 'purseCutter', 'whaleRebalance', 'downlineBoost', 'goldenName',
  'tenderOffer',
] as const;
```

- [ ] **Step 3: Extend the Shenanigans.tsx spell registry**

In `frontend/src/components/Shenanigans.tsx`, find:
- The `TARGETED_SPELL_IDS` set at line 25 — add `11`:
  ```typescript
  const TARGETED_SPELL_IDS = new Set([0, 2, 3, 4, 7, 11]); // moneyTrickster, renameSpell, mintTaxSiphon, downlineHeist, purseCutter, tenderOffer
  ```
- The variant array around line 64 — append `ShenaniganType.tenderOffer`:
  ```typescript
  ShenaniganType.moneyTrickster, ShenaniganType.aoeSkim, ShenaniganType.renameSpell,
  ShenaniganType.mintTaxSiphon, ShenaniganType.downlineHeist, ShenaniganType.magicMirror,
  ShenaniganType.ppBoosterAura, ShenaniganType.purseCutter, ShenaniganType.whaleRebalance,
  ShenaniganType.downlineBoost, ShenaniganType.goldenName, ShenaniganType.tenderOffer,
  ```
- The `offenseTypes` array at line 87 — append `11`:
  ```typescript
  const offenseTypes = [0, 1, 3, 4, 7, 8, 11]; // moneyTrickster, aoeSkim, mintTaxSiphon, downlineHeist, purseCutter, whaleRebalance, tenderOffer
  ```
- Any other id-keyed lookup (search `Shenanigans.tsx` for the digit `10` in array contexts — if it appears in additional registries, ensure `11` is added consistently).

- [ ] **Step 4: Build the frontend**

Run: `cd frontend && npm run build 2>&1 | tail -20`
Expected: build succeeds.

- [ ] **Step 5: Commit**

```bash
git add frontend/
git commit -m "feat(frontend): wire Tender Offer into spell registry + declarations

Extend SHEN_VARIANT_ORDER, TARGETED_SPELL_IDS, offenseTypes, and the
spell variant list with Tender Offer (id=11). Regenerate declarations
to include the new variant."
```

### Task 18: Verify Phase 3 against the local dev server

- [ ] **Step 1: Reinstall and start the dev server**

```bash
dfx canister install shenanigans --mode reinstall --network local
cd frontend && npm run dev
```

- [ ] **Step 2: Confirm Tender Offer appears in the spell grid**

Open the UI. Tender Offer should appear as a spell card with cost 500/100/300, odds 35/50/15.

- [ ] **Step 3: Try to cast on a target that's NOT ≤50% of your balance**

Expected: spell rejected with "Target's PP balance must be at most 50% of yours for Tender Offer."

- [ ] **Step 4: Cast on a small target (success path)**

Expected:
- Target's balance transfers entirely to you.
- Target appears in `acquiredLockUntil`.
- Live Feed shows: `You — SUCCESS — Tender Offer → Target — +<targetBal> PP`.

- [ ] **Step 5: With another principal, try to cast any spell from the acquired target**

Expected: rejected with "You are locked out of casting (recently acquired by tender offer)."

- [ ] **Step 6: Force a backfire (manually flip the odds via the admin panel temporarily, OR cast repeatedly until backfire rolls)**

Expected:
- Caster's balance drops by 3× the backfire cost (transferred to target).
- Caster appears in `tenderOfferBackfireLockUntil`.
- Subsequent Tender Offer cast attempts fail with "Tender Offer is locked out (recent backfire)."

- [ ] **Step 7: Report Phase 3 complete — no commit**

---

## Phase 4 — Stimulus Check + Bear Raid Twin

A symmetric hero/villain pair. Identical numbers (cost 100/30/50, odds 55/35/10, 24h cooldown). Stimulus Check (success): caster mints +100 PP for self, every other holder gets +40-50 PP (rolled per-victim). Bear Raid (success): caster +100 from drain, every other holder -40-50 PP, excess burned. Bear Raid backfire: inverse — caster -100 PP, others +40-50 (accidental Stimulus Check). Successful Bear Raid sets the caster as "Most Wanted" for 24h (any subsequent spell against them has +20pp success rate).

### Task 19: Add variants and config entries

**Files:**
- Modify: `shenanigans/main.mo:41-53` (`ShenaniganType` variant)
- Modify: `shenanigans/main.mo:1376-1392` (default configs)
- Modify: `shenanigans/main.mo` (post_upgrade seed from Task 14)

- [ ] **Step 1: Add the variants**

Append to `ShenaniganType`:

```motoko
public type ShenaniganType = {
    ...
    #tenderOffer;
    #stimulusCheck;
    #bearRaid;
};
```

- [ ] **Step 2: Append the configs to `defaultConfigs`**

After the Tender Offer entry (id=11):

```motoko
{ id = 12; name = "Stimulus Check"; description = "Pull strings at the Fed — everyone gets a check. You get a bigger one for proposing it."; backfireDescription = ?"The bill didn't pass. You ate the lobbying budget — burn 200 PP."; costSuccess = 100.0; costFailure = 30.0; costBackfire = 50.0; successOdds = 55; failureOdds = 35; backfireOdds = 10; duration = 0; cooldown = 24; effectValues = [100.0, 40.0, 50.0, 200.0]; castLimit = 0; backgroundColor = "#e6ffe6" },
{ id = 13; name = "Bear Raid"; description = "Coordinated short. You profit on the spread; everyone else takes a haircut."; backfireDescription = ?"You misread the cycle — burn 100 PP and everyone else gets paid 40-50 PP."; costSuccess = 100.0; costFailure = 30.0; costBackfire = 50.0; successOdds = 55; failureOdds = 35; backfireOdds = 10; duration = 0; cooldown = 24; effectValues = [100.0, 40.0, 50.0, 100.0]; backgroundColor = "#ffe6f0"; castLimit = 0 },
```

`effectValues` schema for both:
- `[0]` = caster gain on success (100 PP)
- `[1]` = per-victim payout min (40 PP)
- `[2]` = per-victim payout max (50 PP)
- `[3]` = caster loss on backfire (200 PP for Stimulus, 100 PP for Bear Raid)

- [ ] **Step 3: Update the `postupgrade` seed**

In the `postupgrade` hook added in Task 14, extend `newConfigs` to include both new configs (Stimulus + Bear Raid). Copy/paste the same entries.

- [ ] **Step 4: Add the Most Wanted state map**

Near the other top-level state maps:

```motoko
/// Principal -> nanosecond-precision deadline. While `now < deadline`,
/// any spell cast against this principal gets +20pp success-rate modifier.
/// Set on every successful Bear Raid by the caster (24h window).
var mostWantedUntil = principalMap.empty<Int>();
```

- [ ] **Step 5: Build to validate**

Run: `dfx build shenanigans 2>&1 | tail -20`
Expected: build succeeds.

### Task 20: Add the `programmaticMint` helper

**Files:**
- Modify: `shenanigans/main.mo` (helpers section)

- [ ] **Step 1: Survey existing mint paths**

Search `main.mo` for how mints currently happen (`chipMint`, `mintFor`, etc.). The cascade observer at the top of the file polls pp_ledger and applies mint multipliers — that path is not what we want. We want a direct mint that doesn't go through cascade-observer accounting.

Search: `grep -n "chipMint\|icrc1_transfer\|mint" shenanigans/main.mo | head -20`.

- [ ] **Step 2: Define `programmaticMint`**

If pp_ledger exposes a mint endpoint callable from the shenanigans canister (it should — search pp_ledger's interface), add a thin wrapper:

```motoko
/// Mint PP directly to a recipient, bypassing the cascade observer.
/// Used by spells that need a one-shot windfall (Stimulus Check) or a
/// one-shot symmetric refund (Bear Raid backfire).
/// Returns the actual minted amount on success, or 0 on failure.
func programmaticMint(recipient : Principal, amountUnits : Nat, memo : Text) : async Nat {
    // Adjust the call signature to match pp_ledger's actual mint endpoint.
    // Common patterns: icrc1_transfer from the minting account, or a
    // canister-specific mintFor function.
    switch (await ppLedger.chipMint(recipient, amountUnits, memo)) {
        case (#Ok(_)) { amountUnits };
        case (#Err(_)) { 0 };
    };
};
```

If pp_ledger doesn't expose such an endpoint, that's a blocker — investigate before continuing. Look in `pp_ledger/main.mo` for any `mint`, `chipMint`, `add_balance`, or similar.

- [ ] **Step 3: Build to validate**

Run: `dfx build shenanigans 2>&1 | tail -20`
Expected: build succeeds. If not, the pp_ledger interface needs a new mint endpoint — surface this to the user before proceeding.

### Task 21: Implement Stimulus Check handler

**Files:**
- Modify: `shenanigans/main.mo` (in `applySuccessEffect`)
- Modify: `shenanigans/main.mo` (in `applyBackfireEffect`)

- [ ] **Step 1: Add the success handler**

Append to `applySuccessEffect`:

```motoko
case (#stimulusCheck) {
    let casterGain : Nat = ppToUnits(effectNatOr(config.effectValues, 0, 100));
    let perVictimMin : Nat = effectNatOr(config.effectValues, 1, 40);
    let perVictimMax : Nat = effectNatOr(config.effectValues, 2, 50);

    // Mint to caster first.
    let casterMinted = await programmaticMint(caster, casterGain, memo);

    // Iterate all known holders (the existing AoE pattern in #aoeSkim uses
    // entries of ppBurnedPerPlayer as a holder enumeration. Use the same
    // source — it covers anyone who has ever held PP).
    var totalOthers : Nat = 0;
    var othersCount : Nat = 0;
    for ((holder, _) in principalMap.entries(ppBurnedPerPlayer)) {
        if (not Principal.equal(holder, caster)) {
            let perVictim = rollPct(perVictimMin, perVictimMax);
            let perVictimUnits = ppToUnits(perVictim);
            let minted = await programmaticMint(holder, perVictimUnits, memo);
            if (minted > 0) {
                totalOthers += minted;
                othersCount += 1;
            };
        };
    };

    return { ppDeltaCaster = casterMinted; affectedTarget = null; affectedCount = othersCount; shieldDeflected = false; renameDetail = null };
};
```

(`rollPct` should already exist — see the existing Money Trickster handler at line 2172. If not, search for the helper that returns a random number in a range.)

- [ ] **Step 2: Add the backfire handler**

Append to `applyBackfireEffect`:

```motoko
case (#stimulusCheck) {
    // Backfire: just burn casterLoss PP from the caster.
    // (The standard cost burn at the top of castShenanigan already
    // handles costBackfire = 50 PP. The additional 200 PP penalty is
    // applied here.)
    let casterLoss : Nat = ppToUnits(effectNatOr(config.effectValues, 3, 200));
    let _ = await chipBurn(caster, casterLoss, memo);
};
```

(Search for `chipBurn` to confirm the helper signature — there's an existing burn primitive used by other spells.)

- [ ] **Step 3: Build to validate**

Run: `dfx build shenanigans 2>&1 | tail -20`
Expected: build succeeds.

### Task 22: Implement Bear Raid handler

**Files:**
- Modify: `shenanigans/main.mo` (in `applySuccessEffect` and `applyBackfireEffect`)

- [ ] **Step 1: Add the success handler**

Append to `applySuccessEffect`:

```motoko
case (#bearRaid) {
    let casterGain : Nat = ppToUnits(effectNatOr(config.effectValues, 0, 100));
    let perVictimMin : Nat = effectNatOr(config.effectValues, 1, 40);
    let perVictimMax : Nat = effectNatOr(config.effectValues, 2, 50);

    var drained : Nat = 0;
    var victims : Nat = 0;
    for ((holder, _) in principalMap.entries(ppBurnedPerPlayer)) {
        if (not Principal.equal(holder, caster)) {
            if (not consumeShieldIfActive(holder)) {
                let perVictim = rollPct(perVictimMin, perVictimMax);
                let perVictimUnits = ppToUnits(perVictim);
                switch (await chipTransfer(holder, caster, perVictimUnits, memo)) {
                    case (#Ok(_)) {
                        drained += perVictimUnits;
                        victims += 1;
                    };
                    case (#Err(_)) { };
                };
            };
        };
    };

    // Caster keeps `casterGain`; burn the excess so the spell is net-neutral
    // beyond the caster's cap (mirrors the spec — "drain from victims; caster
    // keeps 100, excess burns").
    if (drained > casterGain) {
        let excess = drained - casterGain;
        let _ = await chipBurn(caster, excess, memo);
    };

    // Set Most Wanted status on the caster for 24h.
    let oneDay : Int = 24 * 3600 * 1_000_000_000;
    mostWantedUntil := principalMap.put(mostWantedUntil, caster, nowTs + oneDay);

    return { ppDeltaCaster = casterGain; affectedTarget = null; affectedCount = victims; shieldDeflected = false; renameDetail = null };
};
```

- [ ] **Step 2: Add the backfire handler**

Append to `applyBackfireEffect`:

```motoko
case (#bearRaid) {
    // Karmic inversion — backfire becomes an accidental Stimulus Check.
    // Caster takes a hit, every other holder gets paid.
    let casterLoss : Nat = ppToUnits(effectNatOr(config.effectValues, 3, 100));
    let perVictimMin : Nat = effectNatOr(config.effectValues, 1, 40);
    let perVictimMax : Nat = effectNatOr(config.effectValues, 2, 50);

    let _ = await chipBurn(caster, casterLoss, memo);
    for ((holder, _) in principalMap.entries(ppBurnedPerPlayer)) {
        if (not Principal.equal(holder, caster)) {
            let perVictim = rollPct(perVictimMin, perVictimMax);
            let perVictimUnits = ppToUnits(perVictim);
            let _ = await programmaticMint(holder, perVictimUnits, memo);
        };
    };
};
```

- [ ] **Step 3: Wire Most Wanted into success-rate modifier**

Find the existing `determineOutcomeWithMod` function around line 1888. It takes a `modPct : Int` parameter that adjusts success probability. The caller computes that mod from various sources (existing modifiers like Yield Boost cascade modifiers).

In `castShenanigan`, just before calling `determineOutcomeWithMod`, add Most Wanted to the modifier:

```motoko
// Most Wanted bonus: if the target was recently a successful Bear Raider,
// every spell against them gets +20pp success-rate modifier.
var mostWantedBonus : Int = 0;
switch (target) {
    case (?t) {
        switch (principalMap.get(mostWantedUntil, t)) {
            case (?deadline) {
                if (Time.now() < deadline) { mostWantedBonus := 20 };
            };
            case null {};
        };
    };
    case null {};
};
```

Then add `mostWantedBonus` to the modifier passed to `determineOutcomeWithMod`. (The exact call site needs to be located — search for the existing `determineOutcomeWithMod(config, ...)` call in `castShenanigan`.)

- [ ] **Step 4: Build and commit Phase 4 backend changes**

Run: `dfx build shenanigans 2>&1 | tail -20`
Expected: build succeeds.

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): add Stimulus Check + Bear Raid social pair

Twin spells with identical numbers (cost 100/30/50, odds 55/35/10).

- Stimulus Check (#stimulusCheck, id=12): inflationary windfall. Caster
  mints +100 PP for self, every other holder mints +40-50 PP. Backfire
  burns 200 PP from caster.
- Bear Raid (#bearRaid, id=13): deflationary drain. Caster collects 100 PP
  from victims; excess (above the 100 cap) burns. Backfire is karmic
  inversion — caster burns 100 PP, every other holder mints 40-50 PP.

Successful Bear Raid sets the caster to 'Most Wanted' for 24h. Every
spell cast against a Most Wanted target gets +20pp success rate.

programmaticMint helper bypasses the cascade observer for one-shot mints."
```

### Task 23: Wire Stimulus + Bear Raid into the frontend

**Files:**
- Modify: `frontend/src/declarations/shenanigans/` (regenerate)
- Modify: `frontend/src/components/Shenanigans.tsx:25-87`
- Modify: `frontend/src/components/trollbox/rows/SpellRow.tsx:10-13`

- [ ] **Step 1: Regenerate declarations**

```bash
dfx generate shenanigans
cp .dfx/local/canisters/shenanigans/shenanigans.did frontend/src/declarations/shenanigans/
cp .dfx/local/canisters/shenanigans/shenanigans.did.d.ts frontend/src/declarations/shenanigans/
cp .dfx/local/canisters/shenanigans/shenanigans.did.js frontend/src/declarations/shenanigans/
```

- [ ] **Step 2: Extend SHEN_VARIANT_ORDER**

Update `SpellRow.tsx`:

```typescript
const SHEN_VARIANT_ORDER = [
  'moneyTrickster', 'aoeSkim', 'renameSpell', 'mintTaxSiphon', 'downlineHeist',
  'magicMirror', 'ppBoosterAura', 'purseCutter', 'whaleRebalance', 'downlineBoost', 'goldenName',
  'tenderOffer', 'stimulusCheck', 'bearRaid',
] as const;
```

- [ ] **Step 3: Extend Shenanigans.tsx spell registry**

- The variant array around line 64 — append `ShenaniganType.stimulusCheck, ShenaniganType.bearRaid`.
- `offenseTypes` at line 87 — append `13` (Bear Raid is offense; Stimulus Check is friendly so no entry).
- `TARGETED_SPELL_IDS` at line 25 — no changes (neither has an explicit target).

- [ ] **Step 4: Build the frontend**

Run: `cd frontend && npm run build 2>&1 | tail -20`
Expected: build succeeds.

- [ ] **Step 5: Commit**

```bash
git add frontend/
git commit -m "feat(frontend): wire Stimulus Check + Bear Raid into spell registry

Extend SHEN_VARIANT_ORDER and the spell variant list with stimulusCheck
(id=12) and bearRaid (id=13). Bear Raid joins offenseTypes; Stimulus is
friendly so it stays out. Regenerate declarations to include both new
variants."
```

### Task 24: Verify Phase 4 against the local dev server

- [ ] **Step 1: Reinstall and start the dev server**

```bash
dfx canister install shenanigans --mode reinstall --network local
cd frontend && npm run dev
```

- [ ] **Step 2: Set up at least 3 test principals with PP balances**

Use `dfx identity new test1 / test2 / test3`, give each some PP via mint (manual setup or via the admin panel).

- [ ] **Step 3: Cast Stimulus Check from test1**

Expected:
- test1 balance increases by 100 PP.
- test2 and test3 balances increase by 40-50 PP each.
- Live Feed shows: `test1 — SUCCESS — Stimulus Check — +100 PP across 2 players`.

- [ ] **Step 4: Cast Bear Raid from test1**

Expected:
- test1 balance increases by 100 PP.
- test2 and test3 balances decrease by 40-50 PP each (or shielded).
- Excess above 100 burns.
- Live Feed shows: `test1 — SUCCESS — Bear Raid — +100 PP across 2 players`.
- test1 is now Most Wanted for 24h.

- [ ] **Step 5: Cast any spell on test1 (Most Wanted) from test2**

Expected: success rate appears higher than usual (the +20pp modifier in effect). Hard to verify directly without instrumenting — visually check the cooldown / outcome distribution by casting repeatedly.

- [ ] **Step 6: Force a Bear Raid backfire**

Expected:
- test1 balance decreases by 100 PP.
- test2 and test3 balances increase by 40-50 PP each (inversion).
- Live Feed shows: `test1 — BACKFIRE — Bear Raid — ...`.

- [ ] **Step 7: Report Phase 4 complete — no commit**

**Follow-up (out of scope for v1):** Hero/Villain leaderboards (Top Heroes / Most Wanted This Week), auto-trollbox broadcasts on successful casts, and the public broadcast banner. These layer on top of the spell mechanics and can be added later without backend changes — just extra frontend queries against existing data.

---

## Phase 5 — "This Round" Leaderboard + Cleanup

Two small follow-ups: enable the disabled "This Round" leaderboard filter (requires a new per-round burn tally and query endpoint), and delete the two dead leaderboard hooks identified in `shenanigans_known_issues`.

### Task 25: Add per-round burn tally state

**Files:**
- Modify: `shenanigans/main.mo` (state maps)

- [ ] **Step 1: Find the existing `currentRoundId` source**

Search: `grep -n "currentRoundId\|roundId" shenanigans/main.mo | head -10`.

The shenanigans canister likely polls ponzi_math to find the current round. Confirm the source (a `var currentRoundId` or a `getCurrentRound()` call against ponzi_math).

- [ ] **Step 2: Add a per-round burn tally**

Near the existing `ppBurnedPerPlayer` declaration:

```motoko
/// Per-round burn tally — Map<roundId, Map<player, totalBurnedUnits>>.
/// Updated alongside ppBurnedPerPlayer on every successful cast. Used by
/// the "This Round" leaderboard filter.
var ppBurnedPerPlayerPerRound = natMap.empty<PrincipalMap.PrincipalMap<Nat>>();
```

(The exact type depends on the existing map types — match the patterns in use. `natMap.empty<...>` for a per-round outer map, `principalMap.empty<Nat>()` for the inner.)

- [ ] **Step 3: Update the burn tracker**

Find where `ppBurnedPerPlayer` is updated (search `ppBurnedPerPlayer := principalMap.put`). Right alongside, add an update to `ppBurnedPerPlayerPerRound`:

```motoko
// Existing lifetime tally
ppBurnedPerPlayer := principalMap.put(ppBurnedPerPlayer, caller, currentTotal + burnedUnits);

// Per-round tally for the "This Round" leaderboard.
let currentRound : Nat = currentRoundId; // or however the round id is read
let roundMap : PrincipalMap.PrincipalMap<Nat> = switch (natMap.get(ppBurnedPerPlayerPerRound, currentRound)) {
    case (?m) { m };
    case null { principalMap.empty<Nat>() };
};
let priorRoundTotal : Nat = switch (principalMap.get(roundMap, caller)) {
    case (?n) { n };
    case null { 0 };
};
let newRoundMap = principalMap.put(roundMap, caller, priorRoundTotal + burnedUnits);
ppBurnedPerPlayerPerRound := natMap.put(ppBurnedPerPlayerPerRound, currentRound, newRoundMap);
```

- [ ] **Step 4: Build to validate**

Run: `dfx build shenanigans 2>&1 | tail -20`
Expected: build succeeds.

### Task 26: Add the `getRoundBurnedLeaderboard` query

**Files:**
- Modify: `shenanigans/main.mo`

- [ ] **Step 1: Add the public query**

Near the existing `getShenaniganStats` query (around line 3367):

```motoko
/// Returns burn totals for a specific round, sorted descending. Pass
/// `null` to get the current round. Returns up to `limit` entries (use
/// 50 if no limit needed).
public query func getRoundBurnedLeaderboard(roundId : ?Nat, limit : Nat) : async [(Principal, Nat)] {
    let target = switch (roundId) {
        case (?id) { id };
        case null { currentRoundId };
    };
    switch (natMap.get(ppBurnedPerPlayerPerRound, target)) {
        case (null) { [] };
        case (?roundMap) {
            let entries = Iter.toArray(principalMap.entries(roundMap));
            let sorted = Array.sort<(Principal, Nat)>(entries, func(a, b) {
                if (b.1 > a.1) { #greater }
                else if (b.1 < a.1) { #less }
                else { #equal }
            });
            let capped = if (sorted.size() > limit) {
                Array.subArray(sorted, 0, limit)
            } else { sorted };
            capped
        };
    };
};
```

- [ ] **Step 2: Build and commit backend changes**

Run: `dfx build shenanigans 2>&1 | tail -20`
Expected: build succeeds.

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): per-round burn tally + getRoundBurnedLeaderboard query

Add ppBurnedPerPlayerPerRound state map (keyed by currentRoundId, then
by player principal). Updated on every successful spell cast alongside
the existing lifetime ppBurnedPerPlayer tracker.

Expose getRoundBurnedLeaderboard(roundId, limit) query for the 'This
Round' leaderboard filter. Pass null for roundId to get the current
round; limit caps result size."
```

### Task 27: Wire "This Round" toggle into HallOfFame

**Files:**
- Modify: `frontend/src/declarations/shenanigans/` (regenerate)
- Modify: `frontend/src/hooks/useQueries.ts`
- Modify: `frontend/src/components/HallOfFame.tsx:118-123` (the disabled toggle)
- Modify: `frontend/src/components/hall-of-fame/HallOfFameRail.tsx` (if it has its own toggle)

- [ ] **Step 1: Regenerate declarations**

```bash
dfx generate shenanigans
cp .dfx/local/canisters/shenanigans/shenanigans.did frontend/src/declarations/shenanigans/
cp .dfx/local/canisters/shenanigans/shenanigans.did.d.ts frontend/src/declarations/shenanigans/
cp .dfx/local/canisters/shenanigans/shenanigans.did.js frontend/src/declarations/shenanigans/
```

- [ ] **Step 2: Add `useGetRoundBurnedLeaderboard` hook**

Add to `useQueries.ts`:

```typescript
export function useGetRoundBurnedLeaderboard(roundId?: number, limit = 50) {
  const actor = useReadShenaniganActor();
  return useQuery({
    queryKey: ['shenanigans', 'roundBurnedLeaderboard', roundId, limit],
    queryFn: async () => {
      if (!actor) return [];
      const arg = roundId !== undefined ? [BigInt(roundId)] : [];
      return actor.getRoundBurnedLeaderboard(arg, BigInt(limit));
    },
    enabled: !!actor,
    staleTime: 15_000,
  });
}
```

- [ ] **Step 3: Enable the "This Round" toggle**

Find the disabled toggle in `HallOfFame.tsx:118-123`. Remove the `disabled` attribute, the `title="Coming after next round reset"`, and `cursor-not-allowed opacity-50` classes. Add an `onClick` handler that updates a local `filter: 'all-time' | 'this-round'` state.

Replace with:

```tsx
<button
  type="button"
  onClick={() => setFilter('this-round')}
  className={`px-3 py-1 text-xs rounded ${
    filter === 'this-round' ? 'mc-bg-purple mc-text-primary' : 'mc-text-muted'
  }`}
>
  This Round
</button>
```

Add the state above the JSX:

```typescript
const [filter, setFilter] = useState<'all-time' | 'this-round'>('all-time');
const burnersAllTime = useGetBurners(); // existing
const burnersThisRound = useGetRoundBurnedLeaderboard();
const burners = filter === 'all-time' ? burnersAllTime : burnersThisRound;
```

(Adjust to match the existing data-flow patterns — the hook names may differ slightly. The point is: when filter is "this-round", swap the data source.)

- [ ] **Step 4: Apply the same change to `HallOfFameRail.tsx` if it has its own toggle**

Check `frontend/src/components/hall-of-fame/HallOfFameRail.tsx` for a similar "This Round" toggle. If present, mirror the change.

- [ ] **Step 5: Build the frontend**

Run: `cd frontend && npm run build 2>&1 | tail -20`
Expected: build succeeds.

- [ ] **Step 6: Commit**

```bash
git add frontend/
git commit -m "feat(frontend): enable This Round leaderboard filter

Wire the previously-disabled This Round toggle in HallOfFame.tsx (and
the sticky HoF rail) to a new useGetRoundBurnedLeaderboard hook that
calls the new getRoundBurnedLeaderboard query."
```

### Task 28: Delete dead leaderboard hooks

**Files:**
- Modify: `frontend/src/hooks/useQueries.ts`

- [ ] **Step 1: Find and delete `useGetTopPonziPointsHolders`**

Search: `grep -n "useGetTopPonziPointsHolders\|useGetHallOfFame" frontend/src/hooks/useQueries.ts`.

Expected lines around 1232 (`useGetTopPonziPointsHolders`) and 1264 (`useGetHallOfFame`). Delete both function definitions including any preceding comment/docstring.

- [ ] **Step 2: Search the codebase for any importers**

Run: `grep -rn "useGetTopPonziPointsHolders\|useGetHallOfFame" /Users/robertripley/coding/musicalchairs/frontend/src/`.

Expected: zero results outside the deleted file. If anything else imports them, surface it before deleting — the memory entry said dead but worth verifying.

- [ ] **Step 3: Build the frontend**

Run: `cd frontend && npm run build 2>&1 | tail -20`
Expected: build succeeds.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/hooks/useQueries.ts
git commit -m "chore(frontend): delete dead leaderboard hooks

useGetTopPonziPointsHolders and useGetHallOfFame haven't had a caller
since the 2026-05-21 leaderboard refactor. Per shenanigans_known_issues."
```

### Task 29: Phase 5 verification

- [ ] **Step 1: Reinstall and start the dev server**

```bash
dfx canister install shenanigans --mode reinstall --network local
cd frontend && npm run dev
```

- [ ] **Step 2: Cast a couple of spells with each of 2 different principals to seed the round-burn tally**

- [ ] **Step 3: Open the Hall of Fame**

Expected: "This Round" toggle is enabled. Click it. The leaderboard switches from lifetime to current-round burn totals.

- [ ] **Step 4: Verify the tally matches expected**

The "This Round" total for each player should equal the sum of PP they've burned via spell casts in this round.

- [ ] **Step 5: Report Phase 5 complete — no commit**

---

## Final Self-Review Checklist

- [ ] **Spec coverage:** All four scope items shipped (feed robustness, Cease & Desist redesign, Tender Offer + Stimulus + Bear Raid, This Round + cleanup). Insurance / MLM ranks / bounties / Dokapon / debt are OUT (confirmed at plan start).
- [ ] **Phase independence:** Each phase ends with a verification step and at least one commit. Phases can be paused between for review.
- [ ] **Type consistency:** `renameDetail` shape `{ oldName : Text; newName : Text }` is identical in ShenaniganRecord and `#spellCast`. `shieldDeflected` is `?Bool` in both. The applySuccessEffect return type carries the same fields and is updated across all return sites.
- [ ] **Forward-only:** Every schema change uses optional fields. No state migration. Old records render in the existing terse format because the new fields are null.
- [ ] **Deployment:** No mainnet deploy step. Verification is local-build + local-canister + dev-server. User controls mainnet rollout separately.

---

## Notes for the Implementer

- **The repo has no automated test suite.** Each "verify" step is a manual click-through on the local dev server. If a step's expected output doesn't match, surface to the user before plowing ahead — the spec may need adjustment based on a discovered constraint.
- **Run `graphify update .` after each phase commit** so the knowledge graph stays current. Cheap, AST-only.
- **The user-facing copy for spells matters.** When in doubt about spell descriptions/blurbs, stay close to the existing voice (VC/MLM jargon, satirical-but-not-mean) — pull patterns from the 11 existing entries at `shenanigans/main.mo:1376-1387`. Don't invent casino-framed copy (per `feedback_voice_jargon` memory).
- **Pre-cast gates throw errors.** Failed gates raise via `Error.reject(...)`, which surfaces in the frontend as a rejected promise. Existing error-message rendering (`prettifyCanisterError` in `frontend/src/lib/errorMessages.ts`) should handle them — verify and add cases if needed.
- **`acquiredLockUntil` blocks ALL spell casting from the locked principal.** This is by design (24h "post-acquisition integration period"). Tender Offer success should feel weighty — the target gets a day off the game.
