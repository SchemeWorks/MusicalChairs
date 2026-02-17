# Phase F — Wallet & Referral Enrichment — REPORT

*Resolves: A-2, A-1, B-16, B-22, B-8, B-10*

**TS errors:** 44 total (all pre-existing) · 0 new

---

## #33 — Money flow diagram in wallet (A-2)

**Problem:** No visual explaining wallet → game balance → position → earnings flow.

**Solution:** Added a compact flow diagram inside the wallet dropdown, below the balance and PP section. Uses a numeric step indicator to highlight the user's current phase:
- `step 0` (Wallet) — user has no game balance
- `step 1` (Game Balance) — user has deposited but we don't have position data in this component

The diagram shows: `Wallet → Game Balance → Position → Earnings` with the current step highlighted in the appropriate accent color (primary, green, gold).

**Files:** `WalletDropdown.tsx`

---

## #34 — QR code for referral link (A-1)

**Problem:** QR code was planned but never implemented.

**Finding:** QRCodeCanvas was already imported and rendered in ReferralSection.tsx from a prior implementation. The `qrcode.react` package is installed, QRCodeCanvas renders the referral link at 160px with download functionality.

**Enhancement:** Added `mc-card` wrapper around the QR code section with padding, and added the caption "Scan to join your pyramid" below the QR image per the spec.

**Files:** `ReferralSection.tsx`

---

## #35 — Referral stats context (B-16)

**Problem:** Stats show raw numbers with no context about what they mean or what to aim for.

**Solution:** Added contextual text below each stat card:
- **Direct Referrals:** Dynamic milestone progress — "Share your link to get started" (0), "X more for Networker badge" (<5), "X more for Pyramid Architect" (<10), "X more for MLM Legend" (<25), "Top recruiter energy" (25+)
- **Level 2:** "Your referrals' referrals"
- **Level 3:** "Three levels deep"
- **Referral PP:** Context on spending power — "Earn PP from referral deposits" (0), "Keep growing your network" (<100), "Enough for N shenanigan casts" (100+)

Refactored stats grid into an IIFE that extracts values once for cleaner template logic.

**Files:** `ReferralSection.tsx`

---

## #36 — "Spend your PP" bridge CTA (B-22)

**Problem:** No cross-linking between earning PP (MLM tab) and spending PP (Shenanigans tab).

**Solution:**
1. Added `onTabChange?: (tab: TabType) => void` prop to `ReferralSection`
2. Dashboard passes `handleTabChange` to ReferralSection
3. After the stats grid, if user has ≥100 total PP and `onTabChange` is available, render a button:
   `[Dice5 icon] Spend your PP on Shenanigans →`
4. Button calls `onTabChange('shenanigans')` on click
5. Uses `mc-btn-secondary` styling with purple dice icon
6. **Also added the same CTA in GameTracking (Profit Center)** — after the House info card, shows "Spend your N PP on Shenanigans →" with the user's actual PP count. Added `onTabChange` prop to `GameTrackingProps`, `useGetPonziPoints` query, and `Dice5` import. Dashboard passes `handleTabChange` to both `profitCenter` and `default` cases.

**Files:** `ReferralSection.tsx`, `Dashboard.tsx`, `GameTracking.tsx`

---

## #37 — Referral activity feed (B-8)

**Problem:** No feed showing recent referral activity.

**Status: BLOCKED** — Backend provides only aggregate tier points (`getReferralTierPoints`), not individual referral events. No query exists for referral activity data.

**Solution:** Added placeholder section (visible only when user has referrals):
- "Recent Activity" heading with mc-label
- "Referral activity feed coming soon" in muted italic text

This can be wired up when the backend adds a referral events endpoint.

**Files:** `ReferralSection.tsx`

---

## #38 — Network visualization (B-10)

**Problem:** No visual tree/graph of the user's referral network.

**Reality:** Backend only provides aggregate point totals per tier (`level1Points`, `level2Points`, `level3Points`), not individual referral node data (names, principals). A true interactive tree is impossible without individual node data.

**Solution:** Built a simplified 3-tier pyramid visualization using pure CSS:
- **Top:** "You" node in a gold-bordered circle
- **Level 1:** Green-accented pill showing L1 PP total + "Direct referrals" label
- **Level 2:** Cyan-accented pill showing L2 PP total + "Your referrals' referrals" label
- **Level 3:** Purple-accented pill showing L3 PP total + "Three levels deep" label
- Connector lines between tiers using `w-px` dividers
- Horizontal bars widen at each level to create visual pyramid shape
- Replaced the old flat "How the Pyramid Works" text with this visual

Full interactive tree visualization deferred pending backend support for individual referral node data.

**Files:** `ReferralSection.tsx`

---

## Files Modified

| File | Tasks |
|------|-------|
| `frontend/src/components/WalletDropdown.tsx` | #33 |
| `frontend/src/components/ReferralSection.tsx` | #34, #35, #36, #37, #38 |
| `frontend/src/components/GameTracking.tsx` | #36 |
| `frontend/src/components/Dashboard.tsx` | #36 |

## Spec Coverage Audit

| ID | Item | Status |
|----|------|--------|
| A-2 | Money flow diagram in wallet | ✅ Implemented |
| A-1 | QR code for referral link | ✅ Already existed, enhanced with card wrapper + caption |
| B-16 | Referral stats context | ✅ Implemented with milestone progress + spending context |
| B-22 | Spend PP bridge CTA | ✅ Implemented with tab navigation |
| B-8 | Referral activity feed | ⚠️ Blocked — backend placeholder added |
| B-10 | Network visualization | ✅ Simplified pyramid tier viz (full tree deferred — no node data) |
