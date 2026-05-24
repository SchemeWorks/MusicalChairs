# Admins Exempt from Hall of Fame Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Filter `CHARLES_PRINCIPALS` out of the Hall of Fame burner list and replace the "Your Rank" banner with a "House Status" variant when an admin views the page.

**Architecture:** Frontend-only display filter. The hook layer (`useGetTopPonziPointsBurners`) filters and re-ranks before any consumer sees the data. The Hall of Fame component branches the "Your Rank" banner based on `isCharles(principal)`.

**Tech Stack:** Existing — React + TypeScript + Tailwind, existing `isCharles` / `CharlesIcon` helpers from `frontend/src/lib/charles.tsx`.

**Spec:** [docs/superpowers/specs/2026-05-24-admins-exempt-from-leaderboard.md](../specs/2026-05-24-admins-exempt-from-leaderboard.md)

**Testing posture:** No test runner in repo — verification is `npm run build` plus manual visual check via deployed/dev app.

**Working directory:** All paths relative to repo root `/Users/robertripley/coding/musicalchairs`.

---

## File Map

**Modified only:**
- `frontend/src/hooks/useQueries.ts` — import `isCharles`, add filter in `useGetTopPonziPointsBurners`
- `frontend/src/components/HallOfFame.tsx` — import `CharlesIcon`, branch the "Your Rank" banner on `isCharles(principal)`

**Not touched:**
- `frontend/src/lib/charles.tsx` (already exports everything we need)
- Backend (no deploy)
- `PodiumCard` / `Podium` / `LeaderboardRow` (downstream of the filtered hook, no changes needed)

---

## Task 1: Filter admins from the burner hook

**Files:**
- Modify: `frontend/src/hooks/useQueries.ts`

- [ ] **Step 1: Add `isCharles` import**

Find the existing import line for `isCharles` (it's already imported at the top of the file — verify with `grep -n "isCharles" frontend/src/hooks/useQueries.ts`). It's at line 20:

```tsx
import { isCharles } from '../lib/charles';
```

If for some reason it's missing, add it. Otherwise no change.

- [ ] **Step 2: Add the filter in `useGetTopPonziPointsBurners`**

Find the existing hook (around line 1239):

```tsx
export function useGetTopPonziPointsBurners() {
  const actor = useReadShenaniganActor();
  return useQuery({
    queryKey: ['topPpBurners'],
    queryFn: async () => {
      const burners = await actor.getTopPpBurners(50n);
      // No `name` field: display names are resolved per-row via useDisplayName
      // at render time (so golden-name spells take precedence over saved profile
      // names and update in real time).
      return burners.map(([principal, unitsBig], index) => ({
        rank: index + 1,
        ponziPointsBurned: Number(unitsBig / 100_000_000n),
        principal: principal.toString(),
      }));
    },
    refetchInterval: 30000,
  });
}
```

Replace with:

```tsx
export function useGetTopPonziPointsBurners() {
  const actor = useReadShenaniganActor();
  return useQuery({
    queryKey: ['topPpBurners'],
    queryFn: async () => {
      const burners = await actor.getTopPpBurners(50n);
      // Admins (Charles principals) are filtered out of the public ranking —
      // the house never plays its own table. Filter BEFORE rank assignment so
      // the remaining list is contiguous (1..N with no gaps where admins
      // would've been). No `name` field: display names are resolved per-row
      // via useDisplayName at render time (so golden-name spells take
      // precedence over saved profile names and update in real time).
      return burners
        .filter(([principal]) => !isCharles(principal.toString()))
        .map(([principal, unitsBig], index) => ({
          rank: index + 1,
          ponziPointsBurned: Number(unitsBig / 100_000_000n),
          principal: principal.toString(),
        }));
    },
    refetchInterval: 30000,
  });
}
```

- [ ] **Step 3: Verify build**

Run: `npm run build`
Expected: clean pass.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/hooks/useQueries.ts
git commit -m "$(cat <<'EOF'
feat(hof): filter Charles principals out of the burner leaderboard

The house never plays its own table. Filters at the hook layer before
rank assignment so the remaining ranking is contiguous (1..N with no
gaps where admins would have been). Frontend-only display filter —
backend state and PP economics are unchanged.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: "House Status" banner variant in HallOfFame

**Files:**
- Modify: `frontend/src/components/HallOfFame.tsx`

- [ ] **Step 1: Add the `CharlesIcon` + `isCharles` imports**

Find the existing imports near the top. Add:

```tsx
import { CharlesIcon, isCharles } from '../lib/charles';
```

(The existing file currently imports nothing from `../lib/charles` — this is a brand-new import line.)

- [ ] **Step 2: Branch the "Your Rank" banner on `isCharles(principal)`**

Find the existing "Your Rank" banner. Currently:

```tsx
      {/* Your Rank banner */}
      <div className="mc-card p-4 flex items-center justify-between">
        <div className="flex items-center gap-3">
          <Target className="h-5 w-5 mc-text-cyan" />
          <div>
            <span className="text-xs mc-label">Your Rank (Diamond Tier)</span>
            <div className="font-bold mc-text-primary text-sm">
              {userBurnerRank !== undefined && userBurnerRank >= 0 ? (
                <span className={userBurnerRank < 3 ? 'mc-text-gold mc-glow-gold' : ''}>
                  #{userBurnerRank + 1} of {burnersData?.length || 0} burners
                </span>
              ) : (
                <span className="mc-text-muted">Unranked — burn PP to climb</span>
              )}
            </div>
          </div>
        </div>
        <div className="text-right">
          <div className="text-lg font-bold mc-text-purple mc-glow-purple">{userPoints.toLocaleString()}</div>
          <div className="text-xs mc-text-muted">PP</div>
        </div>
      </div>
```

Replace with:

```tsx
      {/* Your Rank banner — "House Status" variant for admins */}
      {principal && isCharles(principal) ? (
        <div className="mc-card p-4 flex items-center justify-between">
          <div className="flex items-center gap-3">
            <CharlesIcon className="h-5 w-5 mc-text-gold" />
            <div>
              <span className="text-xs mc-label">House Status</span>
              <div className="font-bold mc-text-primary text-sm">Not ranked</div>
              <div className="text-xs mc-text-muted italic">The house never plays its own table.</div>
            </div>
          </div>
          <div className="text-right">
            <div className="text-lg font-bold mc-text-purple mc-glow-purple">{userPoints.toLocaleString()}</div>
            <div className="text-xs mc-text-muted">PP</div>
          </div>
        </div>
      ) : (
        <div className="mc-card p-4 flex items-center justify-between">
          <div className="flex items-center gap-3">
            <Target className="h-5 w-5 mc-text-cyan" />
            <div>
              <span className="text-xs mc-label">Your Rank (Diamond Tier)</span>
              <div className="font-bold mc-text-primary text-sm">
                {userBurnerRank !== undefined && userBurnerRank >= 0 ? (
                  <span className={userBurnerRank < 3 ? 'mc-text-gold mc-glow-gold' : ''}>
                    #{userBurnerRank + 1} of {burnersData?.length || 0} burners
                  </span>
                ) : (
                  <span className="mc-text-muted">Unranked — burn PP to climb</span>
                )}
              </div>
            </div>
          </div>
          <div className="text-right">
            <div className="text-lg font-bold mc-text-purple mc-glow-purple">{userPoints.toLocaleString()}</div>
            <div className="text-xs mc-text-muted">PP</div>
          </div>
        </div>
      )}
```

(The `principal` is already destructured from `useWallet()` earlier in the function. Verify it's in scope before the banner. If not, that's a separate bug — but the existing `userPrincipal = principal || ''` line implies it is.)

- [ ] **Step 3: Verify build**

Run: `npm run build`
Expected: clean pass. CharlesIcon is a React component (default export pattern in charles.tsx — it's also exported as a named export per the existing grep — verify by reading the export line in `frontend/src/lib/charles.tsx` if the import fails).

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/HallOfFame.tsx
git commit -m "$(cat <<'EOF'
feat(hof): show 'House Status' banner for Charles principals

Replaces the 'Your Rank (Diamond Tier)' banner with a Charles-icon
'House Status — Not ranked. The house never plays its own table.'
variant when an admin views the page. Non-admin behavior unchanged.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: Deploy + visual sweep

**Files:** None modified — verification + deploy.

- [ ] **Step 1: Build**

Run: `npm run build`
Expected: clean pass.

- [ ] **Step 2: Deploy frontend only**

The user's deploy-safety rule: NEVER deploy the backend without explicit permission. This change is frontend-only.

Run:
```bash
dfx deploy --network ic frontend
```

Expected: canister `5qu42-fqaaa-aaaac-qecla-cai` upgrades; no backend canisters touched.

- [ ] **Step 3: Visual verification on live site**

Open `https://musicalchairs.fun` as Charles. Navigate to Shenanigans → Hall of Fame.

Verify:
- "Your Rank" banner is replaced with "House Status — Not ranked — The house never plays its own table." Icon is CharlesIcon (silhouette), gold colored.
- Charles does NOT appear on the podium or in the leaderboard list.
- The "burner count" placeholder line (if shown) reflects the post-filter count (e.g. was "3 burners", now "2 burners").
- Other burners' rankings are contiguous (no gap where Charles would have been).

If logged out OR signed in as a non-Charles principal, "Your Rank" banner shows the standard variant and the leaderboard is the same minus Charles.

---

## Self-Review Notes

**Spec coverage:**

| Spec section | Task |
|--------------|------|
| §1 Filter at the hook layer | Task 1 |
| §2 "House Status" banner variant | Task 2 |
| §3 Empty-list placeholder (no code change needed) | n/a — verified inline |
| §4 Edge cases (logged-out guard) | Task 2 Step 2 (`principal && isCharles(principal)`) |
| §5 Files touched | All tasks |
| §6 Testing | Task 3 |

**Placeholder scan:** No "TBD", no narrative-only steps. All commit messages, code blocks, and commands are complete.

**Type consistency:** `isCharles` takes `string`; `principal` from `useWallet()` is `string | null`. The `principal && isCharles(principal)` guard narrows correctly.
