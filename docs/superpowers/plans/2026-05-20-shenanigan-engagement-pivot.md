# Shenanigan Engagement Pivot Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reframe shenanigans from an economic optimization game into a **slot-machine engagement loop**: cheap admission per cast, success is the rate-limited reward, failures/backfires are "keep pulling the lever" moments. The public spectacle IS the payoff.

**Architecture:**
- Per-spell cost is dramatically tuned down to **5–20 PP** (chip-clipping admission, not economic stake). All three outcome costs default to the same value — the cost-split machinery from PR #77 stays in place as an admin power-tool but is no longer load-bearing.
- **Per-spell-per-player cooldown is finally enforced** — but only successful casts trigger it. Failures and backfires let you cast again immediately. This is the slot-machine.
- Doc copy that was lying ("Zero Floor — No player goes below 0 PP", "2-min global cooldown, 3-min per-target cooldown") gets updated to match what's actually enforced.
- VC Royalties stat tile (which becomes near-useless at low costs) gets removed.
- The shenanigans canister's V4 migration attachment (a deploy time-bomb flagged in the Phase-1 review) gets removed *first*, as its own PR, so the rest can deploy safely.

**Tech Stack:** Motoko (canister), TypeScript + React (frontend), dfx for declarations.

**Pre-work assumption:** PR #77 (per-outcome costs) has shipped to mainnet (it has). The cost-split type stays; this plan just retunes the values and adds the cooldown gate.

---

## PR Split

This is two PRs to keep the risky cleanup separate from the engagement work:

- **PR A (urgent, ~10 min):** Remove the V4 migration attachment — Task 0 only. Deploy immediately.
- **PR B (the actual feature, ~3 hrs):** Tasks 1–8. The slot-machine pivot.

---

## File Structure

**PR A modifies:**
- `shenanigans/main.mo:32-37` — remove `(with migration = Migration.runV4)` + the comment block + the now-unused `import Migration`

**PR B modifies:**
- `shenanigans/main.mo` — add cooldown state + enforcement, retune default costs
- `frontend/src/declarations/shenanigans/*` — regenerated for new query endpoints
- `frontend/src/components/Shenanigans.tsx` — surface cooldown state on spell cards, kill "Zero Floor" / cooldown-text lies, drop VC Royalties tile
- `frontend/src/components/ShenanigansAdminPanel.tsx` — admin tune costs

**Test plan:** No automated tests in repo. Manual verification against local replica plus a smoke pass on mainnet after deploy.

---

# PR A — Remove V4 migration attachment

## Task 0: V4 cleanup

**Files:**
- Modify: `shenanigans/main.mo:20-37` (the import + actor declaration block)

- [ ] **Step 1: Remove the `Migration` import and the `(with migration = ...)` attachment**

Read [shenanigans/main.mo:20-37](shenanigans/main.mo:20) and replace with:

```motoko
import PpLedger "PpLedger";
import Reginald "Reginald";
import Subaccount "Subaccount";
import Icrc21 "icrc21";

// TODO(2026-05-11): Rename "chips" terminology in this canister — depositChips,
// claimCashOut, chip subaccount, CashOutEntry, etc. — to non-casino verbiage
// (e.g. credits, PP units, tokens). Deferred from the ponzi_math extraction
// migration to keep that scope tight. See
// docs/superpowers/specs/2026-05-11-ponzi-math-extraction-design.md.

persistent actor Self {
```

`migration.mo` itself stays — V2/V3/V4 are kept as legacy reference, matching the existing pattern.

- [ ] **Step 2: Build clean**

Run: `dfx build shenanigans --check`
Expected: clean.

- [ ] **Step 3: Commit + push + open PR + merge + deploy**

```bash
git checkout -b chore/drop-v4-migration-attachment
git add shenanigans/main.mo
git commit -m "chore(shenanigans): drop V4 migration attachment now that it has run

V4 ran successfully against the deployed mainnet canister during the
per-outcome-costs upgrade. Leaving the attachment wired means the next
upgrade would re-run V4 against the new three-cost shape and trap on
the type mismatch (V4's input expects the OLD single-cost shape).
Removing the attachment now so subsequent upgrades are safe."
git push -u origin chore/drop-v4-migration-attachment
gh pr create --title "chore(shenanigans): drop V4 migration attachment" --body "Eliminates the upgrade time-bomb flagged in the Phase-1 review."
gh pr merge --squash --delete-branch
git checkout main && git pull
echo "yes" | dfx deploy shenanigans --network ic
```

The deploy will succeed cleanly because there's no schema change — Motoko's compatible-upgrade path runs (no migration needed, no data movement).

---

# PR B — The slot-machine pivot

## Task 1: Frontend — kill brittle CustomEvent shuttle, trust React Query

**Why:** The admin panel currently dispatches a `shenaniganUpdated` CustomEvent on save and Shenanigans.tsx has a listener that shuttles named fields into local state. The listener overwrites unlisted fields with `undefined` and adds fragility every time the schema grows. React Query invalidation already happens correctly in the mutation hooks — same-tab updates propagate through the query, and DocsPage gets fresh data the same way. Removing the event eliminates the propagation bug *and* makes Task 5's cooldown field "just work" without manual wiring.

**Files:**
- Modify: `frontend/src/components/ShenanigansAdminPanel.tsx` — drop both `window.dispatchEvent` calls
- Modify: `frontend/src/components/Shenanigans.tsx` — drop the `shenaniganUpdated` listener

- [ ] **Step 1: Remove the CustomEvent dispatch in `handleSaveShenanigan` (~line 177)**

Delete this block entirely:

```tsx
      window.dispatchEvent(new CustomEvent('shenaniganUpdated', {
        detail: {
          id: selectedShenanigan.id, name: selectedShenanigan.name, icon: shenaniganIcons[selectedShenanigan.id],
          description: selectedShenanigan.description,
          costSuccess: selectedShenanigan.costSuccess,
          costFailure: selectedShenanigan.costFailure,
          costBackfire: selectedShenanigan.costBackfire,
          successOdds: selectedShenanigan.successOdds, failOdds: selectedShenanigan.failureOdds,
          backfireOdds: selectedShenanigan.backfireOdds, effectValues: selectedShenanigan.effectValues.join(', '),
        }
      }));
```

- [ ] **Step 2: Remove the CustomEvent dispatch in `handleSaveAllChanges` (~line 224)**

Delete the `shenanigans.forEach(...)` block that fires events.

- [ ] **Step 3: Remove the listener in `Shenanigans.tsx` (~line 190)**

Delete the entire useEffect block:

```tsx
  // Listen for admin panel live updates
  useEffect(() => {
    const handler = (event: CustomEvent) => { /* ... */ };
    window.addEventListener('shenaniganUpdated', handler as EventListener);
    return () => window.removeEventListener('shenaniganUpdated', handler as EventListener);
  }, []);
```

- [ ] **Step 4: TypeScript check**

```bash
cd frontend && npx tsc --noEmit
```
Expected: clean.

- [ ] **Step 5: Commit**

```bash
git checkout -b feat/shenanigan-slot-machine
git add frontend/src/components/Shenanigans.tsx frontend/src/components/ShenanigansAdminPanel.tsx
git commit -m "fix(shenanigans-ui): drop brittle CustomEvent shuttle, trust React Query invalidation

The CustomEvent only shipped a hardcoded subset of fields, so any
schema growth would overwrite live fields with undefined. React Query
invalidation already propagates same-tab updates correctly via the
mutation hooks. Removing the event eliminates the propagation bug and
keeps DocsPage + spell cards aligned with admin changes automatically."
```

---

## Task 2: Backend — cooldown enforcement (success-only)

**Files:**
- Modify: `shenanigans/main.mo` near `var shenanigans = ...` declarations (~line 305) and inside `castShenanigan` (~line 1750)
- Modify: `shenanigans/main.mo` admin endpoints (~line 3329) to expose query for the frontend

**Mechanic:** On a `#success` outcome, set `cooldownExpiresAt[(caller, spellId)] = Time.now() + config.cooldown * 3600 * 10^9`. On any cast attempt, if the cooldown for `(caller, spellId)` is in the future, trap. `#fail` and `#backfire` outcomes do NOT set cooldown — they let the player keep trying.

Per-spell-per-player. Not per-target (keeps it simple — if the spammability concern surfaces later, add per-target as a follow-up).

- [ ] **Step 1: Add the cooldown stable state**

Place near the other `var shenaniganX = ...` declarations (~line 305):

```motoko
    // Per-(player, spell) success cooldown expiry timestamps (ns since
    // Unix epoch). Populated when a cast lands #success; consulted by
    // castShenanigan as a pre-cast gate. Failures and backfires DO NOT
    // populate this map — the design is "keep pulling the lever until
    // success, then you're locked out for cooldown hours." Entries are
    // lazily pruned in castShenanigan after expiry — no background sweep.
    var spellCooldowns = principalMap.empty<[(Nat, Int)]>();
```

(Storing `[(spellId, expiresAt)]` as an array per player keeps the map shape simple. 11 spells max per player — array scans are trivially cheap.)

- [ ] **Step 2: Add cooldown read/write helpers**

Place near `getConfigForType` (~line 2340):

```motoko
    /// Return the cooldown expiry timestamp for (player, spell), or 0 if
    /// no cooldown is active (no entry or expired). 0 is sentinel because
    /// any real cooldown timestamp is in the future relative to Time.now().
    func getCooldownExpiry(player : Principal, spellId : Nat) : Int {
        let entries = switch (principalMap.get(spellCooldowns, player)) {
            case (null) { return 0 };
            case (?xs) { xs };
        };
        for ((id, expires) in entries.vals()) {
            if (id == spellId) { return expires };
        };
        0;
    };

    /// Set (or replace) the cooldown expiry for (player, spell). Prunes
    /// any other expired entries for this player while we're traversing.
    func setCooldownExpiry(player : Principal, spellId : Nat, expiresAt : Int) {
        let now = Time.now();
        let prior = switch (principalMap.get(spellCooldowns, player)) {
            case (null) { [] };
            case (?xs) { xs };
        };
        let buf = Buffer.Buffer<(Nat, Int)>(prior.size() + 1);
        var replaced = false;
        for ((id, expires) in prior.vals()) {
            if (id == spellId) {
                buf.add((id, expiresAt));
                replaced := true;
            } else if (expires > now) {
                // Keep live cooldowns for other spells; drop expired ones.
                buf.add((id, expires));
            };
        };
        if (not replaced) { buf.add((spellId, expiresAt)) };
        spellCooldowns := principalMap.put(spellCooldowns, player, Buffer.toArray(buf));
    };
```

- [ ] **Step 3: Add the pre-cast gate**

In `castShenanigan` (~line 1750), AFTER the config lookup and BEFORE the balance check, insert:

```motoko
        // Cooldown gate: a successful cast locks the player out of this
        // spell for config.cooldown hours. Failures and backfires don't
        // lock. cooldown == 0 means no lockout ever.
        let cooldownExpiry = getCooldownExpiry(caller, config.id);
        if (cooldownExpiry > Time.now()) {
            let secondsLeft = (cooldownExpiry - Time.now()) / 1_000_000_000;
            Debug.trap("On cooldown — try again in " # Int.toText(secondsLeft) # "s");
        };
```

- [ ] **Step 4: Set the cooldown on success**

In `castShenanigan`, find the `if (outcome == #success or outcome == #backfire)` block (~line 1872) and ADD a sibling block immediately above it:

```motoko
        if (outcome == #success and config.cooldown > 0) {
            let cooldownNs : Int = Int.abs(config.cooldown) * 3600 * 1_000_000_000;
            setCooldownExpiry(caller, config.id, Time.now() + cooldownNs);
        };
```

- [ ] **Step 5: Expose cooldown query for the frontend**

Place near `getRecentShenanigans` (~line 3002):

```motoko
    /// Per-(player, spell) cooldown query for the spell card UI. Returns
    /// the expiry timestamp (ns since epoch) for each spell on cooldown
    /// for the player. Spells not on cooldown are omitted. Frontend
    /// computes "X minutes left" client-side.
    public query func getSpellCooldowns(player : Principal) : async [(Nat, Int)] {
        let now = Time.now();
        switch (principalMap.get(spellCooldowns, player)) {
            case (null) { [] };
            case (?xs) { Array.filter<(Nat, Int)>(xs, func((_, expires)) = expires > now) };
        };
    };
```

- [ ] **Step 6: Build clean**

Run: `dfx build shenanigans --check`
Expected: clean.

---

## Task 3: Backend — retune default costs

**Files:**
- Modify: `shenanigans/main.mo` `initializeDefaultShenanigans` (~line 1245)

Costs go from "economically meaningful" to "admission fee". All three outcome costs equal (the per-outcome split stays in the code as an admin power-tool but defaults to uniform).

**Cost + cooldown recommendations (starting values):**

| ID | Spell | Cost (all 3 outcomes) | Cooldown on success | Rationale |
|---|---|---|---|---|
| 0 | MEV Attack | 10 PP | 2h | The "common" attack — easy to cast often |
| 1 | Contagion | 20 PP | 12h | AOE, hits every player — big public moment |
| 2 | Cease & Desist | 10 PP | 24h | Rename lasts 7d — cap to daily |
| 3 | Trailing Commission | 15 PP | 24h | Persistent skim, 7d effect — daily |
| 4 | Crossline Poach | 15 PP | 8h | Targeted social grief |
| 5 | Poison Pill | 5 PP | 6h | Defensive — should be more available |
| 6 | Yield Boost | 10 PP | 24h | Self-buff, daily |
| 7 | Bridge Exploit | 15 PP | 8h | High-variance aggro |
| 8 | Wealth Tax | 20 PP | 12h | Hits top 3 — big public event |
| 9 | Override Bonus | 10 PP | 24h | Self-buff, daily |
| 10 | Whitelisted | 5 PP | 24h | Pure cosmetic |

**Starting values, not source-backed.** Test plan: deploy to mainnet, watch a week of casts in the trollbox. Pass criteria: avg PP burned per player per active day is "noticeable but trivial" — informally <5% of typical balance. Adjust direction:
- If players cast rarely (low engagement): lower costs further (round to 5 PP across the board)
- If players spam-cast aggressively until balance bottoms out: raise costs by 50%
- If a particular spell never gets cast: lower its cooldown OR raise visibility, not cost

- [ ] **Step 1: Rewrite default configs with new numbers**

In `initializeDefaultShenanigans` (~line 1245), replace the `defaultConfigs` array with the values from the table above:

```motoko
        let defaultConfigs : [ShenaniganConfig] = [
            { id = 0; name = "MEV Attack"; description = "Sandwich-attacks the target for 2\u{2013}8% of their Ponzi Points (max 250 PP)."; costSuccess = 10.0; costFailure = 10.0; costBackfire = 10.0; successOdds = 60; failureOdds = 25; backfireOdds = 15; duration = 0; cooldown = 2; effectValues = [2.0, 8.0, 250.0]; castLimit = 0; backgroundColor = "#fff9e6" },
            { id = 1; name = "Contagion"; description = "Losses get socialized \u{2014} every player surrenders 1\u{2013}3% (max 60 PP each)."; costSuccess = 20.0; costFailure = 20.0; costBackfire = 20.0; successOdds = 40; failureOdds = 40; backfireOdds = 20; duration = 0; cooldown = 12; effectValues = [1.0, 3.0, 60.0]; castLimit = 1; backgroundColor = "#e6f7ff" },
            { id = 2; name = "Cease & Desist"; description = "Target is forced to change their display name for 7 days."; costSuccess = 10.0; costFailure = 10.0; costBackfire = 10.0; successOdds = 90; failureOdds = 5; backfireOdds = 5; duration = 168; cooldown = 24; effectValues = [7.0]; castLimit = 0; backgroundColor = "#ffe6f7" },
            { id = 3; name = "Trailing Commission"; description = "Skims 5% of target's new PP for 7 days (max 1000 PP)."; costSuccess = 15.0; costFailure = 15.0; costBackfire = 15.0; successOdds = 70; failureOdds = 20; backfireOdds = 10; duration = 168; cooldown = 24; effectValues = [5.0, 1000.0]; castLimit = 0; backgroundColor = "#f3e6ff" },
            { id = 4; name = "Crossline Poach"; description = "Poach one member from target's downline (favors L3)."; costSuccess = 15.0; costFailure = 15.0; costBackfire = 15.0; successOdds = 30; failureOdds = 60; backfireOdds = 10; duration = 0; cooldown = 8; effectValues = []; castLimit = 1; backgroundColor = "#e6fff2" },
            { id = 5; name = "Poison Pill"; description = "Defensive measure \u{2014} blocks one hostile shenanigan."; costSuccess = 5.0; costFailure = 5.0; costBackfire = 5.0; successOdds = 100; failureOdds = 0; backfireOdds = 0; duration = 0; cooldown = 6; effectValues = []; castLimit = 2; backgroundColor = "#fff4e6" },
            { id = 6; name = "Yield Boost"; description = "Earn +5\u{2013}15% additional PP for the rest of the round."; costSuccess = 10.0; costFailure = 10.0; costBackfire = 10.0; successOdds = 100; failureOdds = 0; backfireOdds = 0; duration = 0; cooldown = 24; effectValues = [5.0, 15.0]; castLimit = 1; backgroundColor = "#e6f2ff" },
            { id = 7; name = "Bridge Exploit"; description = "Target loses 25\u{2013}50% of their PP (max 800 PP)."; costSuccess = 15.0; costFailure = 15.0; costBackfire = 15.0; successOdds = 20; failureOdds = 50; backfireOdds = 30; duration = 0; cooldown = 8; effectValues = [25.0, 50.0, 800.0]; castLimit = 0; backgroundColor = "#ffe6e6" },
            { id = 8; name = "Wealth Tax"; description = "A socialist mayor takes office \u{2014} 20% from the top 3 PP holders (max 300 PP/whale)."; costSuccess = 20.0; costFailure = 20.0; costBackfire = 20.0; successOdds = 50; failureOdds = 30; backfireOdds = 20; duration = 0; cooldown = 12; effectValues = [20.0, 300.0]; castLimit = 0; backgroundColor = "#f0e6ff" },
            { id = 9; name = "Override Bonus"; description = "Your downline kicks up 1.3x PP for the rest of the round."; costSuccess = 10.0; costFailure = 10.0; costBackfire = 10.0; successOdds = 100; failureOdds = 0; backfireOdds = 0; duration = 0; cooldown = 24; effectValues = [1.3]; castLimit = 1; backgroundColor = "#e6fffa" },
            { id = 10; name = "Whitelisted"; description = "Gold name on the leaderboard (24h or 7d) \u{2014} the only clout that matters."; costSuccess = 5.0; costFailure = 5.0; costBackfire = 5.0; successOdds = 100; failureOdds = 0; backfireOdds = 0; duration = 24; cooldown = 24; effectValues = [24.0, 168.0]; castLimit = 1; backgroundColor = "#fff0e6" },
        ];
```

(The new defaults only seed on a fresh install. Mainnet already has the old high values stored — Task 8 manually retunes via the admin panel after deploy.)

- [ ] **Step 2: Build clean**

Run: `dfx build shenanigans --check`
Expected: clean.

- [ ] **Step 3: Commit Tasks 2 + 3 together (backend cooldown + retuned defaults)**

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): enforce success-only cooldown + retune costs to admission tier"
```

---

## Task 4: Regenerate declarations

**Files:**
- Modify (generated): `frontend/src/declarations/shenanigans/*`

- [ ] **Step 1: Regenerate**

```bash
dfx generate shenanigans
cp src/declarations/shenanigans/shenanigans.did frontend/src/declarations/shenanigans/shenanigans.did
cp src/declarations/shenanigans/shenanigans.did.js frontend/src/declarations/shenanigans/shenanigans.did.js
cp src/declarations/shenanigans/shenanigans.did.d.ts frontend/src/declarations/shenanigans/shenanigans.did.d.ts
```

- [ ] **Step 2: Confirm `getSpellCooldowns` is present**

```bash
grep -n "getSpellCooldowns" frontend/src/declarations/shenanigans/shenanigans.did.d.ts
```
Expected: match.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/declarations/shenanigans/
git commit -m "chore(shenanigans): regenerate declarations for cooldown query"
```

---

## Task 5: Frontend — cooldown UI + copy fixes

**Files:**
- Modify: `frontend/src/components/Shenanigans.tsx`
- Modify: `frontend/src/hooks/useQueries.ts` (add `useGetSpellCooldowns` hook)

- [ ] **Step 1: Add the cooldown query hook**

In `frontend/src/hooks/useQueries.ts`, after the existing `useGetShenaniganConfigs`:

```tsx
export function useGetSpellCooldowns(principal: Principal | undefined) {
  const actor = useReadShenaniganActor();
  return useQuery({
    queryKey: ['spellCooldowns', principal?.toText()],
    queryFn: async () => {
      if (!actor || !principal) return [] as [bigint, bigint][];
      return await actor.getSpellCooldowns(principal);
    },
    enabled: !!actor && !!principal,
    refetchInterval: 30_000,  // refresh while user watches
  });
}
```

- [ ] **Step 2: Wire it into Shenanigans.tsx, surface "On cooldown" on cards**

Around the existing query block (~line 132):

```tsx
const { data: cooldownsRaw } = useGetSpellCooldowns(callerPrincipal);
const cooldownMap = useMemo(() => {
  const m = new Map<number, number>();
  (cooldownsRaw ?? []).forEach(([id, expiresNs]) => {
    m.set(Number(id), Number(expiresNs) / 1_000_000); // → ms
  });
  return m;
}, [cooldownsRaw]);
```

In each card's disabled-check and button label, factor in cooldown:

```tsx
const cooldownExpiresMs = cooldownMap.get(trick.id) ?? 0;
const now = Date.now();
const onCooldown = cooldownExpiresMs > now;
const minutesLeft = onCooldown ? Math.ceil((cooldownExpiresMs - now) / 60_000) : 0;
const isDisabled = castShenanigan.isPending || userPoints < trick.costSuccess || animatingTrick === trickKey || onCooldown;
```

Button label:

```tsx
{animatingTrick === trickKey ? (
  <><span className="inline-block animate-spin mr-2">🎲</span>Casting…</>
) : onCooldown ? `Cooldown — ${minutesLeft}m`
: userPoints < trick.costSuccess ? `Need ${trick.costSuccess} PP`
: `Cast (${trick.costSuccess} PP)`}
```

(Apply the same pattern to the compact-list view's button further down the file.)

- [ ] **Step 3: Kill the "Zero Floor" and cooldown-text lies**

Find the static doc section around line 505. Replace the three "Zero Floor / Cooldowns / No Refunds" `<div>` blocks with two truthful lines:

```tsx
              <div className="flex items-start gap-2">
                <Zap className="h-3 w-3 mc-text-purple mt-0.5 flex-shrink-0" />
                <span><strong className="mc-text-primary">Cooldowns</strong> — Successful casts lock that spell for hours. Failures and backfires? Try again.</span>
              </div>
              <div className="flex items-start gap-2">
                <AlertTriangle className="h-3 w-3 mc-text-gold mt-0.5 flex-shrink-0" />
                <span><strong className="mc-text-primary">No Refunds</strong> — Every cast burns PP, win or lose.</span>
              </div>
```

(Drops the "Zero Floor" line entirely. The system never had real cooldown enforcement before; now it does.)

- [ ] **Step 4: Drop the VC Royalties stat tile**

In the stats grid (around line 528), remove the fourth tile and switch the grid to 3 cols:

```tsx
            <div className="grid grid-cols-3 gap-3 mb-6">
              {[
                { label: 'PP Spent', value: stats?.totalSpent?.toLocaleString() || '0', color: 'mc-text-cyan' },
                { label: 'Total Cast', value: stats?.totalCast?.toString() || '0', color: 'mc-text-green' },
                { label: 'Outcomes', value: `${stats?.goodOutcomes || 0}/${stats?.badOutcomes || 0}/${stats?.backfires || 0}`, sub: 'good/bad/backfire', color: 'mc-text-purple' },
              ].map(s => (
```

(The backend `dealerCut` field stays in the `ShenaniganStats` type — the comment there already calls it "purely informational." Just stop displaying it.)

- [ ] **Step 5: Invalidate cooldowns after a cast**

In the `handleConfirmCast` success handler, after `setOutcomeToast(...)`, invalidate the cooldown query so the card updates immediately:

```tsx
queryClient.invalidateQueries({ queryKey: ['spellCooldowns'] });
```

(Bring `useQueryClient` into scope at the top of the component if it isn't already.)

- [ ] **Step 6: TypeScript check**

```bash
cd frontend && npx tsc --noEmit
```
Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add frontend/src/components/Shenanigans.tsx frontend/src/hooks/useQueries.ts
git commit -m "feat(shenanigans-ui): surface cooldown state on cards + fix lying copy + drop dealerCut tile"
```

---

## Task 6: Frontend — admin panel sanity

**Files:**
- Modify: `frontend/src/components/ShenanigansAdminPanel.tsx`

- [ ] **Step 1: Update the cooldown input hint to reflect the new mechanic**

Find the existing cooldown input (~line 438):

```tsx
                  <AdminInput label="Cooldown (hours)" type="number" value={selectedShenanigan.cooldown}
                    onChange={v => updateField('cooldown', Math.max(0, parseInt(v) || 0))}
                    min="0"
                    hint="Successful casts lock this spell for the player. 0 = no cooldown." />
```

- [ ] **Step 2: TypeScript check**

```bash
cd frontend && npx tsc --noEmit
```
Expected: clean.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/components/ShenanigansAdminPanel.tsx
git commit -m "feat(shenanigans-admin): clarify cooldown hint matches new enforcement"
```

---

## Task 7: Manual verification (local)

**No automated tests** — run scenarios on a local replica.

- [ ] **Step 1: Local deploy**

```bash
dfx start --background --clean
dfx deploy
```

- [ ] **Step 2: Cast Magic Mirror (id=5, success=100%) twice**

Cast 1 → success. Card flips to "Cooldown — 360m" (6h).
Cast 2 → trap with "On cooldown — try again in ~21000s".
Expected: card disabled, button text shows cooldown.

- [ ] **Step 3: Force a fail/backfire chain (use a low-success spell)**

Cast Bridge Exploit repeatedly. Until success, no cooldown applies — keep going. As soon as success lands → 8h cooldown.
Expected: 5–10 attempts allowed back-to-back; success locks you out.

- [ ] **Step 4: Cooldown isolation per-spell**

Cast Magic Mirror → cooldown set. Cast MEV Attack → unaffected, casts normally.
Expected: cooldowns are per-spell, not global.

- [ ] **Step 5: Cost economics sanity**

Confirm cast cost matches the new defaults (5–20 PP). A chain of 10 backfires burns at most 200 PP.

- [ ] **Step 6: Frontend cooldown countdown ticks down**

Wait 30s, refresh — minutesLeft drops. After cooldown expires, cast button re-enables.

---

## Task 8: Deploy + retune existing mainnet configs

**Context:** Fresh-install seed defaults are baked into the canister, but the deployed mainnet canister keeps its existing config map across upgrades (preserved by design — admin tuning shouldn't be wiped). So after deploy the canister code is new, but the old high costs are still stored. Admin retunes via the panel.

- [ ] **Step 1: Open PR**

```bash
git push -u origin feat/shenanigan-slot-machine
gh pr create --title "feat(shenanigans): slot-machine engagement loop" --body "$(cat <<'EOF'
## Summary

Reframes shenanigans from an EV-optimization game into a slot-machine engagement loop.

- **Cooldown is finally enforced** — per-spell, per-player, success-only. Failures and backfires let you keep pulling the lever; success locks you out for `config.cooldown` hours.
- **Cost defaults dropped to admission-tier** (5–20 PP across the board) for fresh installs. Mainnet keeps its existing tuned values — admin retunes via the panel after deploy.
- "Zero Floor" and the lying cooldown copy in the player UI are removed.
- VC Royalties stat tile is dropped (becomes meaningless at low costs).

## What this isn't

- No schema change → no migration needed.
- The per-outcome cost split from PR #77 stays in the code. Admin retains the power to tune asymmetrically; defaults make all three equal.
- Debt / negative balance feature is parked. With admission-tier costs, the zero-out clamp from Phase 1 is sufficient.

## Test plan

See plan doc — [`docs/superpowers/plans/2026-05-20-shenanigan-engagement-pivot.md`](docs/superpowers/plans/2026-05-20-shenanigan-engagement-pivot.md).

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 2: Merge + deploy**

```bash
gh pr merge --squash --delete-branch
git checkout main && git pull
echo "yes" | dfx deploy shenanigans --network ic
npx vite build && echo "yes" | dfx deploy frontend --network ic
```

- [ ] **Step 3: Retune mainnet costs via admin panel**

Open the admin panel on mainnet. For each of the 11 spells, set costSuccess/costFailure/costBackfire and cooldown to the values in the table above. Save each one.

(Can also be done via `dfx canister call --network ic shenanigans saveAllShenaniganConfigs '(...)'` as a single batch update — but the panel is the more reviewable path.)

- [ ] **Step 4: Smoke test on mainnet**

Cast Magic Mirror once from a test account. Confirm success + cooldown applied. Refresh the spell card, confirm "Cooldown — 360m" shows.

---

# Design rationale (for reference)

## 5-Component Filter (the slot-machine design)

| Component | Pre-pivot state | Post-pivot state |
|---|---|---|
| **Clarity** | Players see odds + cost, can predict EV roughly | Same odds visible; cost is small so EV math doesn't dominate; cooldown is the only new mental model |
| **Motivation** | "Steal PP from rival" — but math rarely positive → motivation collapses | "Land a public spell — the trollbox sees it, target gets renamed/dunked-on" — motivation is social, not economic |
| **Response** | Cast button → animate → outcome toast (good) | Same, plus immediate cooldown lockout (slot-machine release of dopamine + lockout) |
| **Satisfaction** | Toast + small PP gain — both modest | Toast + lockout = "I got my fun, see you tomorrow" — emotional closure |
| **Fit** | Casino/MLM theming + casino mechanic ✓ | Casino/MLM theming + actual slot-machine mechanic ✓ ✓ |

## Risks & abuse cases

- **Spam-cast cheap aggro to drain a target via repeated 2-8% transfers.** Cooldown on success blocks this — once you hit, you're out for hours per spell. Without cooldown on fail/backfire, an unlucky streak does let a player make MANY attempts cheaply (10 attempts × 10 PP = 100 PP). Mitigated by the small chip-transfer caps already in place (e.g., max 250 PP per MEV Attack — even on the rare success, target loss is bounded).
- **Player has 4 PP, tries to cast 5-PP Poison Pill, traps.** Acceptable — trap message is clear ("Insufficient chips").
- **Cooldown stable map grows unboundedly.** Bounded by player count × 11 spells max. Pruning of expired entries happens on write in `setCooldownExpiry`. Stale entries for inactive players persist but cost nothing.
- **Frontend cooldown drift / stale data.** 30s refetch interval is a reasonable middle ground. If a player races a cast against the expiry, the backend gate is authoritative — they get a trap message and learn.

## Playtest scenarios

1. **New player test:** Spell card shows "Cast (10 PP)". They cast. It fails. They cast again — no penalty. Eventually it succeeds → "Cooldown — 2h". They infer the mechanic without explanation.
2. **Stress test:** Spam-click the cast button until success. Verify lockout fires immediately. Refresh page, lockout persists.
3. **Abuse test:** Try to cast a different spell during cooldown — works (per-spell isolation).
4. **Readability test:** Trollbox shows the cast. Other players see the outcome. The public moment is the payoff.

## Tuning priority (if it doesn't feel right after a week)

In order:
1. **If engagement is too low** (people cast rarely) → drop costs further to 5 PP across the board
2. **If failure-chasing burns through balances too fast** → raise costs by 50% OR shorten the longest cooldowns
3. **If a specific spell is never used** → check name/description salience first; lower cooldown second; lower cost last
4. **If a specific spell is overused** → raise cooldown, NOT cost
