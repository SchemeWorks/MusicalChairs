## Phase G — Splash Page Enhancements

*Resolves: A-6, B-1, B-2, B-3, B-14*

### #39 — Live data on splash ribbon (A-6)

**File:** `frontend/src/App.tsx`, possibly `frontend/src/hooks/useQueries.ts`

**Problem:** Backend requires auth for `useGetGameStats`. The ribbon shows static copy instead of live numbers.

**Fix options (in order of preference):**
1. **Add a public (no-auth) query endpoint** to the backend: `getPublicStats()` returning pot size and player count. This is the correct fix but requires backend work.
2. **Call the existing public queries** mentioned in the v1 task list: `getPlatformStats()`, `getActiveGameCount()`, `getAvailableBalance()`. Check if these actually exist and work without auth.
3. **If no public endpoint exists:** Keep the static copy but explicitly note this as a backend dependency in the report. Don't fake numbers.

If live data becomes available:
```
const { data: publicStats } = useQuery({
  queryKey: ['publicStats'],
  queryFn: () => backendActor.getPublicStats(),
  enabled: !isAuthenticated, // only on splash
  refetchInterval: 30000,
});
```

Update the ribbon to show: `Pot: {formatICP(publicStats.pot)} ICP | {publicStats.activePlayers} Players | Live on ICP`

**Effort:** 1-2 hours (frontend), unknown for backend

### #40 — Typewriter effect on tagline (B-1)

**File:** `frontend/src/App.tsx`, `frontend/src/index.css`

**Problem:** The "It's a Ponzi!" tagline appears instantly. The original report wanted it to type out letter by letter.

**Fix:** CSS-only typewriter:
```css
.mc-typewriter {
  overflow: hidden;
  white-space: nowrap;
  border-right: 2px solid var(--mc-gold);
  width: 0;
  animation: mc-typewriter 1.2s steps(14) 0.8s forwards, mc-blink-caret 0.6s step-end 3;
}
@keyframes mc-typewriter {
  to { width: 100%; }
}
@keyframes mc-blink-caret {
  50% { border-color: transparent; }
}
```

Apply to the tagline. The delay (0.8s) allows the logo to appear first. `steps(14)` matches the character count of "It's a Ponzi!". After animation completes, set `border-right: transparent` (animation-fill-mode handles this).

The caret blinks 3 times then stops. Clean, lightweight, no JS.

**Effort:** 20 min

### #41 — Animated background on splash (B-2)

**File:** `frontend/src/index.css` (or new lightweight component)

**Problem:** The splash page is static. The original report wanted particles or gradient shifts.

**Fix (CSS-only, no library):** A slow-moving gradient background:
```css
.mc-splash-bg {
  position: fixed;
  inset: 0;
  z-index: -1;
  background: radial-gradient(ellipse at 20% 50%, rgba(168, 85, 247, 0.06) 0%, transparent 50%),
              radial-gradient(ellipse at 80% 50%, rgba(57, 255, 20, 0.04) 0%, transparent 50%);
  animation: mc-bg-drift 20s ease-in-out infinite alternate;
}
@keyframes mc-bg-drift {
  0% { background-position: 0% 50%; }
  100% { background-position: 100% 50%; }
}
```

Add `<div className="mc-splash-bg" />` inside the splash section. Respects `prefers-reduced-motion` (Phase 13.2).

Subtle, performant, no dependencies.

**Effort:** 15 min

### #42 — Docs teaser / "How It Works" section (B-3)

**File:** `frontend/src/App.tsx`

**Problem:** No expandable section on the splash explaining the game mechanics. The entire app assumes you know what a Ponzi scheme game is.

**Fix:** Below the info cards and before the stats ribbon, add an expandable "How It Works" section:
```
const [showHowItWorks, setShowHowItWorks] = useState(false);

<div className="mt-6">
  <button
    onClick={() => setShowHowItWorks(!showHowItWorks)}
    className="flex items-center gap-2 mx-auto text-xs mc-text-dim hover:mc-text-primary transition-colors"
  >
    <ChevronDown className={`h-4 w-4 transition-transform ${showHowItWorks ? 'rotate-180' : ''}`} />
    How does it work?
  </button>
  {showHowItWorks && (
    <div className="mt-4 grid grid-cols-1 md:grid-cols-2 gap-3 text-left mc-stagger">
      <div className="mc-card p-4">
        <h4 className="font-display text-sm mc-text-green mb-2">Deposit ICP</h4>
        <p className="text-xs mc-text-dim">Choose a plan. Simple earns 11%/day for 21 days. Compounding earns more but locks your money.</p>
      </div>
      <div className="mc-card p-4">
        <h4 className="font-display text-sm mc-text-gold mb-2">Earn Daily</h4>
        <p className="text-xs mc-text-dim">Your position earns interest from the pot. Withdraw anytime — earlier exits pay a higher toll.</p>
      </div>
      <div className="mc-card p-4">
        <h4 className="font-display text-sm mc-text-purple mb-2">Cast Shenanigans</h4>
        <p className="text-xs mc-text-dim">Earn Ponzi Points. Spend them on cosmetic chaos — rename other players, skim their earnings, boost your referrals.</p>
      </div>
      <div className="mc-card p-4">
        <h4 className="font-display text-sm mc-text-danger mb-2">The Catch</h4>
        <p className="text-xs mc-text-dim">When the pot empties, the game resets. If you're still in — total loss. That's the Ponzi part.</p>
      </div>
    </div>
  )}
</div>
```

**Effort:** 30 min

### #43 — Scroll-triggered animations (B-14)

**File:** `frontend/src/App.tsx`, `frontend/src/index.css`

**Problem:** Page-load animations fire before below-the-fold elements are visible. On mobile, cards animate into empty air.

**Fix:** Create a small `useScrollAnimate` hook or use IntersectionObserver directly:
```ts
function useScrollAnimate(ref: RefObject<HTMLElement>, className = 'mc-scroll-visible') {
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const observer = new IntersectionObserver(
      ([entry]) => {
        if (entry.isIntersecting) {
          el.classList.add(className);
          observer.unobserve(el);
        }
      },
      { threshold: 0.1 }
    );
    observer.observe(el);
    return () => observer.disconnect();
  }, []);
}
```

CSS:
```css
.mc-scroll-animate {
  opacity: 0;
  transform: translateY(20px);
  transition: opacity 0.6s ease-out, transform 0.6s ease-out;
}
.mc-scroll-animate.mc-scroll-visible {
  opacity: 1;
  transform: translateY(0);
}
```

Apply `mc-scroll-animate` to the info cards, stats ribbon, and "How It Works" section. The hero (logo, tagline) keeps the page-load animation since it's always above the fold.

**Effort:** 45 min

