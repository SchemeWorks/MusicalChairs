# Musical Chairs — UX Design Philosophy & Handoff Notes

*For any agent continuing UX/frontend work on this project.*

---

## Core Design Principles

### 1. Show Less, Do More, Feel Better
Every screen should answer one question, present one action, and create one feeling. If a component is showing three things at equal weight, it's showing nothing. Prioritize ruthlessly.

### 2. Experience-First, Not Component-First
Don't build a "card component" and put data in it. Ask: "What does the player need to feel right now?" and build backward from that emotion. The withdrawal flow is the best example of this done right — countdown timer, toll info, dramatic confirmation, confetti. That's an *experience*, not a CRUD form.

### 3. The Casino Floor Is Never Silent
Ambient activity (trollbox messages, system events, live feed entries) should be visible at all times. A dead-looking app is a dead app. Social proof, other players' actions, and live stats create the feeling that the room is alive and things are happening whether you participate or not.

### 4. Satire Is the Brand, Not the Apology
The self-aware Ponzi humor isn't a gimmick — it's the entire identity. Every piece of copy should lean into the absurdity. Never sound corporate. Never sound sincere about returns. The gambling warnings are *funnier* because they're honest. "*(Returns not guaranteed)*" right below "Earn a guaranteed 12% return*" is perfection. Protect this voice at all costs.

### 5. Urgency Creates Engagement
Live pot stats, countdowns to toll reductions, maturity progress bars, "X players active" — these create FOMO and return visits. Static numbers don't. Every data point should imply time sensitivity.

---

## Visual Language Rules

### Color Hierarchy
- **Neon Green (#39FF14):** Primary CTA, success, money-in actions, positive P/L
- **Purple (#A855F7):** Interactive elements, PP-related, secondary actions, hover states
- **Gold (#FFD700):** Premium status, rankings, exit tolls, warnings-that-are-also-features
- **Pink (#FF2D78):** Brand accent, logo gradient, danger-that's-exciting
- **Cyan (#00F0FF):** Info, secondary data, level indicators
- **Red (#FF4444):** True danger, errors, gambling warnings (distinct from pink — red means STOP, pink means THRILL)

### Typography
- **Bungee:** ALL section headers, titles, hero text. Nothing else. Never for body text.
- **Fredoka One:** Taglines and snarky subtitles only. Used sparingly — it's the wink, not the voice.
- **Space Mono:** Everything else. Body, buttons, inputs, labels, data. This is the workhorse. Always bold (700) for buttons, regular for body.

### Spacing
- Don't use arbitrary pixel values. Use the CSS custom properties: `--space-xs` through `--space-2xl`.
- Cards get `--space-lg` (24px) padding.
- Section gaps get `--space-xl` (32px) or the Tailwind `space-y-6`.
- Never less than `--space-sm` (8px) between interactive elements.

### Animation
- **Purposeful only.** Every animation must answer: "What information does this convey?"
- Good: confetti on withdrawal (reward signal), counter-flash on refresh (data updated), spin on loading (progress).
- Bad: ambient pulse on static elements, hover glow on non-interactive items, perpetual bounce on decoration.
- All transitions: 0.2-0.3s ease. Nothing slower unless it's a hero entrance.

### Glass & Glow
- `backdrop-filter: blur()` is the signature — use on header, modals, elevated cards.
- Box-shadow glow on hover only, not at rest (except CTAs with the pulse class).
- The `::before` shimmer border on `mc-card-elevated` is expensive — use only for primary section containers, not nested cards.

---

## Layout Rules

### Desktop
- Header tab navigation (5 tabs inline in the top bar — replaces the original left rail sidebar).
- Status bar below header showing live P/L, pot balance, and game stats.
- Main content area: `max-w-4xl` for single-column, `max-w-7xl` for grid layouts.
- Tagline ("It's a Ponzi!") shown only on splash page; hidden when logged in to save header space.

### Mobile
- Bottom tab bar, 5 items max (NO "More" overflow).
- Single-column layout for all content.
- Bottom sheets instead of dropdowns for wallet/modals. All bottom sheets must include working drag-to-dismiss (touch events, not decorative-only drag handles).
- Status bar in compact mode.

### Responsive Breakpoints
- `769px+`: Header tabs visible, bottom tabs hidden.
- Below `769px`: Bottom tabs visible, header tabs hidden. All grids collapse to single column.
- `769px–1024px`: Header tab font reduces to 11px, tighter padding.
- `769px–900px`: Header tab font reduces to 10px, compact padding.

---

## Component Naming Conventions

All custom CSS classes use `mc-` prefix (Musical Chairs):
- `mc-card`, `mc-card-elevated`, `mc-card-select` — card variants
- `mc-btn-primary`, `mc-btn-secondary`, `mc-btn-danger`, `mc-btn-pill` — button variants
- `mc-text-*` — text color utilities (green, purple, gold, cyan, pink, danger, primary, dim, muted)
- `mc-bg-*` — background color utilities (green, purple, gold, danger)
- `mc-glow-*` — text-shadow glow utilities
- `mc-accent-*` — top border accent utilities
- `mc-active-*` — card selection state variants (green, purple, gold) — used on `mc-card-select`
- `mc-status-*` — inline status indicators
- `mc-rank-*` — leaderboard rank styles

**Rule:** Use `mc-*` tokens instead of Tailwind color classes (e.g., `mc-text-green` not `text-green-400`, `mc-bg-purple` not `bg-purple-500`). For alpha/opacity variants, use `bg-[var(--mc-purple)]/20` syntax.

React components use PascalCase, one component per file, in `frontend/src/components/`.

---

## Wallet-Aware UX

Critical context: Internet Identity creates a unique principal per application. This means II users need explicit deposit/withdraw functionality to move ICP in and out of the game. Plug and OISY wallet users do NOT — their wallets are general-purpose and interact with the canister directly.

**Rule:** Any wallet-related UI must check `walletType` and conditionally render:
- `internet-identity`: Show deposit/withdraw, show internal vs external balance, show funding instructions.
- `plug` / `oisy`: Show game balance only. Deposit/withdraw buttons are irrelevant and confusing.

---

## Trollbox Integration Notes

**Status: Planned but not yet implemented.** The trollbox is deferred until core game features are stable. When implemented, it should be core infrastructure that other components feed into:
- **GameTracking.tsx**: On deposit/withdrawal, emit a system message to the trollbox canister.
- **Shenanigans.tsx**: On cast result, emit "[Player] cast [Shenanigan] — [Outcome]!" to trollbox.
- **Trollbox.tsx**: New component, polls canister every 3-5s, renders message feed + input.
- System messages are visually distinct (muted color, italic, no username highlight).
- User messages show rank-colored usernames.

The original layout assumed trollbox would be a fixed right panel (~300px) on desktop and a floating bubble on mobile. When implemented, the layout may need to accommodate this — but do NOT pre-allocate space for it in the current layout.

---

## Key Decisions Made

1. **Emojis replaced with lucide-react icons** — see `docs/EMOJI_MAPPING_REFERENCE.md` for revert reference.
2. **Wallet connect modal uses createPortal** to render above all z-index layers.
3. **Wallet SVG logos** stored in `frontend/public/` (ii-logo.svg, plug-logo.svg, oisy-logo.svg).
4. **Logo is clickable** — resets to dashboard, scrolls to top.
5. **Admin panel has back button** — no longer a trap.
6. **Tab navigation is in App.tsx (header) and Dashboard.tsx (mobile bottom bar)** — no React Router. Tab state is managed via `useState`.
7. **Sidebar removed in v2** — replaced with inline header tabs (desktop) and bottom tab bar (mobile).
8. **Tagline hidden when logged in** — saves ~100px of header space for tab navigation.

---

## What NOT to Change

- The satirical tone and copy. It's perfect. Don't corporate it up.
- The dark felt background system. It's atmospheric and unique.
- The confetti on successful transactions. It's the best micro-interaction in the app.
- The withdrawal dialog with live toll countdown. Genuinely thoughtful.
- The shenanigan card aura/glow effects. Best visual design in the codebase.
- The gambling warnings. They're legally smart and brand-reinforcing.

---

*This document should be updated as major design decisions are made.*
