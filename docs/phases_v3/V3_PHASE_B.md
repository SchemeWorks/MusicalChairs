## Phase B — v2 Spec Compliance

*Resolves: B-33, B-34, B-35, B-36, B-37, B-38, B-39*

These items are specific sub-items from the v2 plan itself that were neither implemented to spec nor tracked for future work. Found by cross-referencing every v2 plan sub-item against the actual codebase.

### #8 — Status bar P/L glow effect (B-33)

**File:** `frontend/src/components/GameStatusBar.tsx`

**Problem:** The v2 plan specified `mc-glow-green` on the P/L stat when positive. `GameTracking.tsx` correctly uses `mc-glow-green` on its P/L hero number (line 306), but `GameStatusBar.tsx` only uses `mc-text-green` with no glow. The same data point (net P/L) is styled differently in the two places it appears.

**Fix:** Change line 31 of GameStatusBar.tsx from:
```
className={`mc-status-bar-value ${isUp ? 'mc-text-green' : 'mc-text-danger'}`}
```
to:
```
className={`mc-status-bar-value ${isUp ? 'mc-text-green mc-glow-green' : 'mc-text-danger'}`}
```

**Effort:** 2 min

### #9 — Header tab base font size spec compliance (B-34)

**File:** `frontend/src/index.css`

**Problem:** The v2 plan specified `13px` for header tab font size (Phase 2, line 107). The implementation uses `11px` (line 503 of index.css). The v3 Phase 1.6 proposes shrinking further to `10px` at narrow breakpoints — moving in the opposite direction.

**Fix:** This requires a design decision: the v2 spec said 13px, the implementation chose 11px (likely because 13px caused overflow at narrower widths), and the v3 plan proposes 10px. The honest resolution is:
1. Try `12px` as a compromise between spec (13px) and current (11px)
2. At `@media (max-width: 1024px)`, drop to `11px`
3. At `@media (max-width: 900px)`, drop to `10px` or switch to icon-only
4. Verify at 769px, 900px, 1024px, 1200px breakpoints

This supersedes the approach in v3 Phase 1.6. Do both items together.

**Effort:** 30 min (including visual testing)

### #10 — Podium avatar/initial circles (B-35)

**File:** `frontend/src/components/HallOfFame.tsx`

**Problem:** The v2 plan specified "Avatar/initial circle" on each podium block. The implementation uses medal icons (Crown, Medal) inside colored circles. The code comment on line 42 says "Avatar + name" but the content is a medal icon, not the player's initial.

**Fix:** Replace the medal icon in the circle with the player's first letter initial. Move the medal/crown to a smaller badge overlapping the circle:
```tsx
<div className={`w-10 h-10 rounded-full ${m.bg} border ${m.border} flex items-center justify-center mb-1.5 relative`} style={{ boxShadow: m.glow }}>
  <span className={`font-display text-sm ${m.text}`}>{entry.name.charAt(0).toUpperCase()}</span>
  <div className="absolute -top-1 -right-1 w-4 h-4 flex items-center justify-center">
    {m.icon}
  </div>
</div>
```

This shows user identity (initial) as primary and rank identity (medal) as a badge — matching the v2 plan's intent while keeping the rank medals.

**Effort:** 15 min

### #11 — QR code download button (B-36)

**File:** `frontend/src/components/ReferralSection.tsx`

**Problem:** The v2 plan specified a downloadable QR ("wrap in a canvas and provide a 'Download QR' button"). The v3 Phase 7.1 adds QR display but not download.

**Fix:** Use `qrcode.react`'s `QRCodeCanvas` (not `QRCodeSVG`) and add a download handler:
```tsx
import { QRCodeCanvas } from 'qrcode.react';

const qrRef = useRef<HTMLCanvasElement>(null);

const downloadQR = () => {
  const canvas = qrRef.current;
  if (!canvas) return;
  const url = canvas.toDataURL('image/png');
  const link = document.createElement('a');
  link.href = url;
  link.download = 'musical-chairs-referral-qr.png';
  link.click();
};

<QRCodeCanvas ref={qrRef} value={referralLink} size={160} bgColor="#0a0812" fgColor="#ffffff" level="M" />
<button onClick={downloadQR} className="mc-btn-secondary text-xs mt-2 flex items-center gap-1.5 mx-auto">
  <Download className="h-3.5 w-3.5" /> Download QR
</button>
```

This replaces the `QRCodeSVG` in v3 Phase 7.1 with `QRCodeCanvas` to enable download.

**Effort:** 15 min

### #12 — "Last Payout" stat on splash ribbon (B-37)

**File:** `frontend/src/App.tsx`

**Problem:** The v2 plan specified "Last Payout: 2.4 ICP" as one of three splash ribbon stats. Both the current implementation and v3 Phase 8.1 use "Live on ICP" instead. "Last Payout" is more compelling social proof — it tells visitors someone recently got paid.

**Fix (depends on data availability):**
1. **If public stats include last payout data:** Show `Last Payout: {formatICP(lastPayout)} ICP` as the third ribbon stat
2. **If no public endpoint for this:** Show it when live data becomes available (after v3 Phase 8.1 adds public stats). Add "Last Payout" to the proposed `getPublicStats()` return type
3. **Static fallback:** If truly blocked, note it as a backend dependency and keep "Live on ICP"

The key point: when the public stats endpoint is eventually added (Phase 8.1), include `lastPayout` in the return value. Don't settle for "Live on ICP" if the data can be made available.

**Effort:** 10 min (frontend), depends on backend for data

### #13 — Celebration timer correction (B-38)

**File:** `frontend/src/components/ProfileSetup.tsx`

**Problem:** The v2 plan said "After 3 seconds, auto-navigate." The implementation uses `4000`. The v3 Phase 1.3 fixes the empty callback and adds a proceed button but doesn't correct the timer.

**Fix:** When implementing v3 Phase 1.3, also change `4000` to `3000` on line 33:
```
}, 3000);
```

Trivial fix that should be bundled with Phase 1.3's other celebration fixes.

**Effort:** 1 min

### #14 — House Ledger accordion — first section expanded by default (B-39)

**File:** `frontend/src/components/HouseDashboard.tsx`

**Problem:** The v2 plan said "Default: all collapsed (the first one optionally expanded)." The implementation starts with `openSection = null` (all closed). The common progressive disclosure pattern is first-section-open so users see content immediately rather than a wall of closed headers.

**Fix:** Change the initial state:
```tsx
const [openSection, setOpenSection] = useState<string | null>(sections[0]?.title || null);
```

Or hardcode to the first section title if the array is static:
```tsx
const [openSection, setOpenSection] = useState<string | null>('What Is This?');
```

This way new users see the first section's content and understand the accordion pattern. Returning users who've read it can collapse it.

**Effort:** 5 min

