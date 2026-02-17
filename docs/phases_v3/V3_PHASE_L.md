## Phase L — Final Cleanup

*Resolves: B-6, B-13, B-18*

### #61 — Check for duplicate refresh buttons (B-6)

**File:** `frontend/src/components/GameTracking.tsx`

**Problem:** v1 said "Duplicate refresh buttons may still exist." Nobody ever checked.

**Fix:** Search GameTracking.tsx for all refresh/reload buttons. If there are two, remove the duplicate. Keep only one refresh button at the top-right of the section.

This is a 5-minute check-and-fix.

**Effort:** 5-10 min

### #62 — Trollbox — explicit deferral (B-13)

**Status:** EXPLICITLY DEFERRED

The trollbox is a major feature requiring a new Motoko canister, websocket or polling infrastructure, rate limiting, moderation tools, and a full frontend component. It was the last phase in the v1 task list for good reason.

**Not doing it in v3 because:** It's estimated at 11-16 hours of work, requires backend changes, and is a feature addition rather than a UX fix. The v3 plan focuses on closing gaps in the existing UI, not adding new features.

**What to do instead:** Add a placeholder "Trollbox — Coming Soon" teaser somewhere visible (footer or header) to acknowledge it's planned. This is better than silently dropping it for a third time.

### #63 — Information density audit (B-18)

**Problem:** Some tabs feel packed, others feel sparse.

**Fix:** After all other phases are complete, do a visual review of every tab and assess:
- **Dense pages** (Profit Center, House Ledger): are they now using progressive disclosure, collapsible sections, and visual hierarchy to manage density?
- **Sparse pages** (MLM, ProfileSetup): have the enrichments from other phases (activity feed, milestones, atmospheric visuals) filled in the empty space?

If any page still feels empty after all enrichments, add contextual CTAs or "did you know" info cards to pad it. If any page still feels dense, add more collapsible sections.

This is an assessment pass, not a coding phase. Time it after everything else is done.

**Effort:** 30-60 min

