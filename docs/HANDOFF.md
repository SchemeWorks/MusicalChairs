# Musical Chairs - Project Handoff Document

**Last Updated**: April 18, 2026
**Project Location**: `/Users/robertripley/coding/musicalchairs`

---

## ⚠️ Architectural Note (April 2026) — Internal Wallet Killed

The in-canister "internal wallet" has been removed. ICP no longer sits in a backend-managed balance between actions. Instead:

- Users approve an ICRC-2 allowance on their wallet.
- The backend pulls exactly the amount needed (`icrc2_transfer_from`) when the user opens a position, funds a backer slot, etc.
- Payouts are pushed directly back to the user's wallet via ICRC-1 `transfer`.
- The backend methods `depositICP`, `withdrawICP`, `transferInternal`, `getWalletBalance`, `getWalletBalanceICP`, `addDownstreamDealer`, and `addHouseMoney` are **deleted**.
- `AddHouseMoney.tsx` / `HouseMoneyToast.tsx` were renamed to `AddBackerMoney.tsx` / `BackerMoneyToast.tsx` to match the backer/VC terminology.
- New saga pattern: snapshot → mutate → `await` ledger → compensate-on-failure. See the three reference sagas `withdrawEarnings`, `settleCompoundingGame`, `claimDealerRepayment` in `backend/main.mo`.

Any section below that references internal-wallet methods, deposit/withdraw/send tabs, or "held by the backend canister" reflects the pre-April-2026 architecture and should be read historically.

---

## 🎯 Project Overview

**Musical Chairs** is a satirical, transparent Ponzi-themed gambling dApp on the Internet Computer Protocol (ICP). The project combines neon casino aesthetics with glassmorphism UI design, openly mocking dark UX patterns while maintaining complete transparency about being a Ponzi scheme.

### Core Concept
- Deliberately transparent about Ponzi mechanics
- Casino/vaporwave aesthetic with neon pink/cyan theme
- Full game logic with multiple game plans (21-day Simple, 15-day Compounding, 30-day Compounding)
- Dealer system with upstream/downstream fee distribution
- "Shenanigans" feature system (11 different items affecting Ponzi Points)
- MLM-style three-level referral system

### Tech Stack
- **Backend**: Motoko on ICP
- **Frontend**: React + TypeScript + Vite
- **Styling**: Tailwind CSS + shadcn/ui with custom casino theme
- **Authentication**: Internet Identity 2.0, Plug Wallet, OISY Wallet
- **Token**: ICRC-2 Ponzi Points token (mainnet deployed)

---

## 🚀 Current Deployment Status

### Identity & Principal Change (Recent)
- **Moved to new identity**: All canisters now controlled by new principal `5tksj-gr72b-oo3d6-dhtnc-eo5r5-fdi2v-wl5tz-j3y5e-y6yvq-awdca-nae`
- **CycleOps Balance Checker**: Added as controller to all canisters: `cpbhu-5iaaa-aaaad-aalta-cai`

### Mainnet Canister IDs (New)
| Canister | ID | Status |
|----------|-----|---------|
| **backend** | `5zxxg-tyaaa-aaaac-qeckq-cai` | ✅ Deployed |
| **frontend** | `5qu42-fqaaa-aaaac-qecla-cai` | ✅ Deployed |
| **pp_ledger** | `5xv2o-iiaaa-aaaac-qeclq-cai` | ✅ Ponzi Points token (1M PP minted) |
| **pp_assets** | `4236a-haaaa-aaaac-qecma-cai` | ✅ Logo assets |

### Legacy Canister IDs (Old - For Reference)
| Canister | Old ID | Notes |
|----------|---------|-------|
| backend | `uxrrr-q7777-77774-qaaaq-cai` | Old local testing ID |
| pp_ledger (old repo) | `awsqm-4qaaa-aaaau-aclja-cai` | From ponzipoints repo before merge |

### Repository Merge
- **ponzipoints** repo merged into **musicalchairs** repo
- Combined both projects into single codebase
- All token logic and assets now unified

---

## 📁 Project Structure

```
musicalchairs/
├── backend/
│   ├── main.mo                    # Main backend canister (game logic, wallet system)
│   ├── ledger.mo                  # ICP ledger ICRC-1/ICRC-2 integration
│   ├── migration.mo               # State migration utilities
│   └── authorization/
│       └── access-control.mo      # Admin access control
├── frontend/
│   ├── src/
│   │   ├── App.tsx                       # Main app: splash page, header, auth, routing
│   │   ├── index.css                     # All custom CSS (mc-* design system, animations)
│   │   ├── components/
│   │   │   ├── Dashboard.tsx             # Tab-based main UI (5 tabs)
│   │   │   ├── GameTracking.tsx          # Profit Center tab — active games
│   │   │   ├── GamePlans.tsx             # Invest tab — plan selection
│   │   │   ├── GameStatusBar.tsx         # Live game status bar
│   │   │   ├── HouseDashboard.tsx        # Seed Round tab
│   │   │   ├── ReferralSection.tsx       # MLM tab — referral system
│   │   │   ├── Shenanigans.tsx           # Shenanigans tab — cosmetic items
│   │   │   ├── PonziPointsDashboard.tsx  # PP display in Shenanigans
│   │   │   ├── HallOfFame.tsx            # Leaderboards
│   │   │   ├── WalletConnectModal.tsx    # Multi-wallet selection UI
│   │   │   ├── WalletDropdown.tsx        # Buy In/Cash Out/Wire interface
│   │   │   ├── LoginButton.tsx           # Wallet connection button
│   │   │   ├── LogoutButton.tsx          # Logout button
│   │   │   ├── ProfileSetup.tsx          # New user onboarding
│   │   │   ├── OnboardingTour.tsx        # 5-step guided tooltip tour
│   │   │   ├── GameDocs.tsx              # In-app docs overlay (9 sections)
│   │   │   ├── MobileSheet.tsx           # Bottom-sheet for mobile nav
│   │   │   ├── HouseMoneyToast.tsx       # Deposit success toast
│   │   │   ├── AddHouseMoney.tsx         # House money addition UI
│   │   │   ├── ConfettiCanvas.tsx        # Confetti celebration effect
│   │   │   ├── ErrorBoundary.tsx         # Error handling with personality
│   │   │   ├── LoadingSpinner.tsx        # Loading indicator
│   │   │   └── ShenanigansAdminPanel.tsx # Admin panel for shenanigan config
│   │   ├── hooks/
│   │   │   ├── useWallet.tsx             # Multi-wallet context (II, Plug, OISY)
│   │   │   ├── useLedger.ts              # ICRC ledger operations
│   │   │   ├── useQueries.ts             # React Query hooks + mutations
│   │   │   ├── useLiveEarnings.ts        # Live portfolio earnings ticker
│   │   │   └── useInternetIdentity.tsx   # Internet Identity hook
│   │   ├── lib/
│   │   │   ├── formatICP.ts              # ICP formatting utilities
│   │   │   └── charles.ts               # Charles persona utilities
│   │   └── declarations/                 # Generated Candid types
│   │       └── backend/
│   │           ├── backend.did.d.ts      # Backend type definitions
│   │           └── index.ts              # Re-exports
│   └── index.html
├── docs/
│   ├── HANDOFF.md                         # This document
│   ├── DEPLOYMENT_STATUS.md               # Technical deployment details
│   ├── TASK_LIST_V3.md                    # Master task list (63 items, all completed)
│   └── phases_v3/                         # Phase plans and reports (A-L)
│       ├── V3_PHASE_{A..L}.md             # Phase plans
│       └── V3_PHASE_{A..L}_REPORT.md      # Phase completion reports
├── dfx.json                               # DFX canister configuration
├── canister_ids.json                      # Network-specific canister IDs
├── package.json                           # NPM dependencies
├── vite.config.ts                         # Vite bundler config (root: 'frontend')
├── tailwind.config.js                     # Tailwind + shadcn/ui theme
├── postcss.config.js                      # PostCSS/Tailwind pipeline
└── .npmrc                                 # Fixes NODE_ENV issues from Claude Desktop
```

---

## 🔧 Development Workflow

### Prerequisites
- [dfx](https://internetcomputer.org/docs/current/developer-docs/setup/install) (v0.15.0+)
- [Node.js](https://nodejs.org/) (v18+)
- npm

### Local Development Commands

```bash
# Navigate to project
cd /Users/robertripley/coding/musicalchairs

# Start local replica (if not running)
dfx start --background --clean

# Deploy backend locally
dfx deploy backend

# Regenerate type declarations after backend changes
dfx generate backend

# Start frontend dev server (runs on http://localhost:5173)
npm run dev

# Set canister principal (do once after local deploy)
dfx canister call backend setCanisterPrincipal '(principal "uxrrr-q7777-77774-qaaaq-cai")'

# Toggle test mode
dfx canister call backend setTestMode '(true)'   # Enable test mode (fake 500 ICP)
dfx canister call backend setTestMode '(false)'  # Disable for real ICP
dfx canister call backend isTestMode             # Check current status
```

### Mainnet Deployment Commands

```bash
# IMPORTANT: Must use CharlesPonzi identity for frontend deploys
dfx identity use CharlesPonzi

# Build frontend (DO NOT use `npm run build` — tsc fails on 44 pre-existing errors)
npx vite build

# Deploy frontend to mainnet (DO NOT use `dfx deploy` — workspace errors)
dfx canister install frontend --network ic --mode upgrade

# If permission error: re-grant Prepare/Commit
CALLER=$(dfx identity get-principal)
dfx canister call frontend grant_permission '(record { to_principal = principal "'$CALLER'"; permission = variant { Prepare } })' --network ic
dfx canister call frontend grant_permission '(record { to_principal = principal "'$CALLER'"; permission = variant { Commit } })' --network ic

# Check canister status
dfx canister status --network ic --all
```

---

## 🎨 Key Features Implemented

### ✅ Multi-Wallet Support
- **Internet Identity 2.0**: Native ICP authentication
- **Plug Wallet**: Browser extension integration
- **OISY Wallet**: II-based wallet using `@dfinity/oisy-wallet-signer`
- Session restoration from localStorage
- Clean wallet switching UX
- Official brand assets for all three wallets

### ✅ ICP Ledger Integration
- **ICRC-1 transfers**: Standard token transfers
- **ICRC-2 approve/transfer_from**: Pull-based deposit flow
- `useLedger.ts` hook with:
  - `getBalance()` - Query ICP balance from external wallet
  - `transfer()` - Send ICP from external wallet
  - `approve()` - Approve backend for deposits
  - `approveForDeposit()` - Helper for deposit flow

### ✅ Backend Wallet System
- `depositICP()` - ICRC-2 pull funds from user wallet to backend
- `withdrawICP()` - ICRC-1 send funds from backend to user wallet
- `transferInternal()` - Internal wallet-to-wallet transfers
- `getWalletBalance()` / `getWalletBalanceICP()` - Balance queries
- `isTestMode()` / `setTestMode()` - Test mode toggle for development

### ✅ Frontend Wallet UI
- **WalletDropdown.tsx**: Complete deposit/withdraw/send interface
  - Deposit tab: ICRC-2 approve + backend pull flow
  - Withdraw tab: Backend sends to user's external wallet
  - Send tab: Internal transfers to other players
  - Test mode detection with warning banners
  - External wallet balance display
  
### ✅ Ponzi Points Token
- **Mainnet deployed**: `5xv2o-iiaaa-aaaac-qeclq-cai`
- ICRC-1 compliant token
- 1,000,000 PP initial supply
- Backend canister authorized as minter
- Logo assets deployed at `4236a-haaaa-aaaac-qecma-cai`

### ✅ Game Mechanics (Backend)
- Three game plans with different return rates
- Dealer system with upstream/downstream fee structure
- 11 "Shenanigans" items affecting Ponzi Points
- MLM-style three-level referral system
- Hall of Fame leaderboards
- Full internal wallet accounting

---

## 🚧 Priority TODO List

### ✅ COMPLETED (from prior TODO)
- ~~Update Hardcoded Canister IDs~~ — done
- ~~Domain & Hosting~~ — musicalchairs.fun live, pointing to frontend canister
- ~~UI/UX Polish~~ — full 63-task UX overhaul completed (Phases A-L)
- ~~Documentation~~ — in-app GameDocs with 9 sections, onboarding tour

### 🔥 HIGH PRIORITY

1. **Fix 44 Pre-existing TypeScript Errors**
   - These don't block Vite builds (we skip tsc) but should be cleaned up
   - Mostly in `backend.did.d.ts` type re-exports and wallet hooks
   - Trend: started at 49, now at 44 (5 fixed incidentally during UX work)

2. **Test Multi-Wallet Flow on Mainnet**
   - [ ] Test Internet Identity connection
   - [ ] Test Plug wallet connection
   - [ ] Test OISY wallet connection
   - [ ] Verify wallet switching and session restoration

3. **Verify Real ICP Deposit/Withdraw Flow**
   - [ ] Ensure testMode is disabled on mainnet backend
   - [ ] Test deposit: approve → depositICP
   - [ ] Test withdrawal: withdrawICP → verify ICP received
   - [ ] Test internal transfers between players

### 📋 MEDIUM PRIORITY

4. **Complete Game Feature Testing**
   - [ ] Test all three game plans end-to-end
   - [ ] Verify dealer/backer fee distribution
   - [ ] Test all 11 Shenanigans items
   - [ ] Verify referral system (3-level MLM)
   - [ ] Test Hall of Fame leaderboards

5. **Security & Access Control**
   - [ ] Review admin access control system
   - [ ] Audit wallet transaction flows
   - [ ] Test edge cases (insufficient balance, concurrent withdrawals)

6. **Trollbox Implementation**
   - Currently a "Coming Soon" teaser in Shenanigans tab
   - Was explicitly deferred during Phase L (#62)

### 🎯 LOWER PRIORITY

7. **Set Up GitHub Organization**
   - [ ] Create GitHub org
   - [ ] Transfer repository to org
   - [ ] Set up branch protection

8. **Mobile Testing**
   - All components built mobile-first but need real-device testing
   - MobileSheet bottom-sheet for nav, responsive grids, touch targets

---

## 🔐 Security & Access Notes

### Current Controllers
All canisters have two controllers:
1. New principal: `5tksj-gr72b-oo3d6-dhtnc-eo5r5-fdi2v-wl5tz-j3y5e-y6yvq-awdca-nae`
2. CycleOps: `cpbhu-5iaaa-aaaad-aalta-cai` (for monitoring/cycle management)

### Test Mode
- When `testMode = true`: Users get 500 fake ICP on first login
- When `testMode = false`: Real ICRC-1/ICRC-2 ledger calls execute
- **Production**: testMode must be `false` on mainnet

### Ledger Interaction Patterns

**Deposit (ICRC-2 Pull Pattern)**:
1. User calls `ledger.approve()` in their wallet → authorizes backend to pull funds
2. User confirms wallet popup
3. Frontend calls `actor.depositICP(amount)`
4. Backend calls ICRC-2 `transfer_from()` to pull funds
5. Backend credits internal wallet balance

**Withdraw (ICRC-1 Push Pattern)**:
1. Frontend calls `actor.withdrawICP(amount)`
2. Backend calls ICRC-1 `transfer()` to send to user
3. Backend debits internal wallet balance

---

## 🐛 Known Issues & Quirks

### TypeScript Build Warnings
- Some TypeScript errors appear in `npm run build` output
- Files still compile and work correctly
- Issue likely in type re-exports from `backend.did.d.ts`

### Environment Variables
- `.npmrc` required to override `NODE_ENV=production` from Claude Desktop
- Without this, npm install fails in development

### Wallet Quirks
- **Plug Wallet**: Requires browser extension installed
- **OISY Wallet**: Requires Internet Identity authentication first
- **Internet Identity**: Must be on HTTPS or localhost for security

---

## 🔑 Key Files Reference

| File | Purpose | Notes |
|------|---------|-------|
| `backend/main.mo` | Main backend canister | Game logic, wallet system, admin functions |
| `backend/ledger.mo` | ICRC ledger types | ICRC-1/ICRC-2 interfaces and helper functions |
| `frontend/src/hooks/useWallet.tsx` | Multi-wallet context | II, Plug, OISY support with session restore |
| `frontend/src/hooks/useLedger.ts` | Ledger operations | balance, transfer, approve, approveForDeposit |
| `frontend/src/hooks/useQueries.ts` | React Query hooks | Backend queries + deposit/withdraw mutations |
| `frontend/src/components/WalletDropdown.tsx` | Wallet UI | Deposit/Withdraw/Send tabs |
| `frontend/src/components/WalletConnectModal.tsx` | Wallet picker | Modal for selecting wallet type |
| `dfx.json` | DFX config | Canister definitions and build settings |
| `canister_ids.json` | Network IDs | Mainnet and local canister IDs |
| `vite.config.ts` | Vite config | Frontend build with ICP environment vars |
| `tailwind.config.js` | Tailwind theme | Casino aesthetic with neon pink/cyan |

---

## 💡 Development Tips

### Common Error: "Cannot find module"
If you see module resolution errors after `dfx generate`:
```bash
# Rebuild declarations
dfx generate backend
# Clear node modules cache
rm -rf node_modules/.vite
# Restart dev server
npm run dev
```

### Debugging Wallet Issues
```bash
# Check current identity
dfx identity whoami

# Check principal
dfx identity get-principal

# List all identities
dfx identity list

# Switch identity
dfx identity use <identity-name>
```

### Checking Canister Cycles
```bash
# Check cycles balance
dfx canister status --network ic backend
dfx canister status --network ic frontend
```

### Quick Mainnet Deploy
```bash
# Must be CharlesPonzi identity
dfx identity use CharlesPonzi
# Build and upgrade frontend in one command
npx vite build && dfx canister install frontend --network ic --mode upgrade
```

---

## 🎭 Project Philosophy

**Transparency First**: This isn't a scam pretending to be legitimate - it's a Ponzi scheme that's completely upfront about being a Ponzi scheme. The entire UI lampoons dark UX patterns through exaggeration and explicit disclosure.

**Casino Aesthetic**: Neon pink/cyan vaporwave theme with glassmorphism. Think Las Vegas meets 1980s arcade meets satirical gambling commentary.

**Educational Satire**: By being completely transparent about the mechanics, the project serves as commentary on opaque DeFi protocols that hide their Ponzi-like mechanics behind complexity and jargon.

---

## 📞 Quick Reference

### Important Principals & IDs
- **New Identity Principal**: `5tksj-gr72b-oo3d6-dhtnc-eo5r5-fdi2v-wl5tz-j3y5e-y6yvq-awdca-nae`
- **CycleOps Controller**: `cpbhu-5iaaa-aaaad-aalta-cai`
- **Backend Canister**: `5zxxg-tyaaa-aaaac-qeckq-cai`
- **Frontend Canister**: `5qu42-fqaaa-aaaac-qecla-cai`
- **PP Token**: `5xv2o-iiaaa-aaaac-qeclq-cai`
- **PP Assets**: `4236a-haaaa-aaaac-qecma-cai`

### External Links
- **Domain**: https://musicalchairs.fun/ (live, pointing to frontend canister)
- **ICP Dashboard**: https://dashboard.internetcomputer.org/
- **Candid UI**: https://a4gq6-oaaaa-aaaab-qaa4q-cai.raw.icp0.io/?id=<canister-id>

---

---

## 🎨 UX Overhaul (Branch: claude/dreamy-swanson)

### Completed Phases

**Phase 1**: Admin gating, wallet dropdown, icon fix
**Phase 2**: Navigation restructure (7 tabs → 5: Profit Center, Invest, Seed Round, MLM, Shenanigans)
**Phase 3**: dealer→backer rename, live earnings ticker, GameTracking rewrite, Charles quotes empty state
**Phase 4**: GamePlans (Invest tab) — success toast, satirical mode copy, daily earnings, empty wallet state
**Phase 5**: ProfileSetup personality, ReferralSection MLM quotes, HallOfFame Diamond Tier + PP disclaimer
**Phase 6**: WalletConnectModal entrance, WalletDropdown casino tabs, Shenanigans outcome flavor, ErrorBoundary + App.tsx personality
**Phase 7**: ShenanigansAdminPanel Charles deepening — personality, form hints, snarky copy
**Phase 8**: HouseMoneyToast Charles quotes, index.css cleanup (65 lines of dead/duplicate CSS removed)

### Copy/Text Pending User Review

The following text was written by Claude using best judgment based on established tone patterns. All need user review/approval before shipping:

#### Phase 5 — ProfileSetup
- **Charles welcome quote**: "I'm glad you're here. Truly. Let me show you something special."
- **Input subtext**: "Every great partnership starts with a name."
- **Placeholder**: "What should we call you?"
- **Warning**: "Real ICP. Real risk. Only play with what you can afford to lose."

#### Phase 5 — HallOfFame
- **Empty state Charles quote**: "Every empire starts with a first transaction."
- **PP disclaimer copy**: "Ponzi Points are the in-game fun currency. Shenanigans are cosmetic chaos you cast using PP. They don't affect the actual game math — just the madness. Burn those Ponzi Points for glory!"
- **PP disclaimer warning**: "All effects are limited to Ponzi Points and cosmetics only — never touching ICP, pot size, backer selection, payout math, or round structure."
- **Footer CTA**: "Earn Ponzi Points by depositing and referring friends. Spend them on Shenanigans to reach Diamond Tier."

#### Phase 6 — WalletConnectModal
- **Header**: "Step Right In" (was "Connect Wallet")
- **II description**: "The house standard. Clean, native, no questions asked."
- **Plug description**: "For those who like to keep their keys close."
- **OISY description**: "Multi-chain. For the diversified degen."
- **Footer warning**: "By connecting, you agree that this is a gambling game and that you're fine with that."

#### Phase 6 — WalletDropdown
- **Tab labels**: "Buy In" / "Cash Out" / "Wire" (was Deposit/Withdraw/Send)
- **Test mode banner**: "Test Mode — Playing with Monopoly money"
- **Test deposit**: "Monopoly money mode. You start with 500 ICP to burn through."
- **Test withdraw**: "Can't cash out Monopoly money. Use 'Wire' for internal transfers."
- **Deposit success**: "You're in. Good luck."
- **Withdraw success**: "Cashed out. Smart move — or was it?"
- **Wire success**: "Wired. The money's gone."
- **Button labels**: "Approve & Buy In", "Confirm Buy In", "Buying In...", "Cash Out to Wallet", "Cashing Out...", "Wire ICP", "Wiring..."

#### Phase 6 — Shenanigans Outcome Flavor
- **Success pool** (5): "The house smiles upon you." / "Clean hit. Charles would be proud." / "Flawless execution. You're a natural." / "They never saw it coming." / "That's how it's done in this business."
- **Fail pool** (5): "The universe said no." / "Not your day. It happens to everyone. Mostly to you." / "Swing and a miss. The PP is still gone, though." / "Nothing happened. Except you're poorer now." / "Better luck next time. Or not. Who knows."
- **Backfire pool** (5): "Oh no. It hit you instead." / "Karma works fast around here." / "You played yourself. Literally." / "That's what they call a learning experience." / "Charles is laughing somewhere."
- **Dismiss button**: "Noted"

#### Phase 6 — ErrorBoundary
- **Error quips** (4): "The house always wins. Except right now." / "Even Ponzis have bad days." / "Charles is looking into it. He's not, but it sounds reassuring." / "Something broke. Probably not the math. Probably."
- **Button**: "Spin Again" (was "Try Again")

#### Phase 6 — App.tsx
- **Loading state**: "Counting your chips..." (was "Loading your profile...")
- **Main error**: Title "The Table Flipped", subtitle "The house always wins, but the website doesn't always cooperate."
- **Profile error**: Title "Onboarding Hit a Snag", subtitle "Try logging out and back in. Charles apologizes for nothing."
- **Admin error**: Title "Charles's Office Is on Fire", subtitle "The back office crashed. The front office is fine. Probably."
- **Dashboard error**: Title "The Dashboard Took a Hit", subtitle "Your money's still there. Probably. Refresh and find out."

#### Phase 7 — ShenanigansAdminPanel
- **Header subtitle**: "Pull the strings. Tweak the odds. The house edge is whatever you say it is." (was "Pull the strings. Tweak the odds. Charles sees all.")
- **Instructions heading**: "How This Works" (was "Charles's Instructions")
- **Instructions footer**: "Remember: you break it, you fix it. The players will notice."
- **Minter heading**: "Post-Deploy Ritual" (was "Token Canister Minter Config")
- **Minter copy**: "re-authorize the backend to mint PP. Forget this and shenanigans stop working"
- **Selector subtext**: "Pick your poison" (was "Select to edit")
- **Editor subtext**: "Tweak everything. Nobody's watching." (was "Edit all parameters below")
- **Empty state**: "Pick a shenanigan from the list." + "With great power comes great responsibility. Just kidding. Go nuts."
- **Cost hint**: "Higher cost = fewer casts = less chaos"
- **Duration hint**: "0 = instant. Otherwise, how long the effect lingers."
- **Cooldown hint**: "How long before they can cast again. 0 = spam city."
- **Effect Values hint**: "Comma-separated. What the shenanigan actually does."
- **Cast Limit hint**: "0 = unlimited. Set a cap or let anarchy reign."

#### Phase 8 — HouseMoneyToast
- **Header**: "You're In" (was "Deposit Successful!")
- **Body**: "{amount} ICP added to the house · {PP} PP earned" (condensed from verbose original)
- **Charles quotes** (5, random): "The house always appreciates a generous patron." / "Smart money. Or at least, money." / "Every dollar helps. Mostly me." / "You just made the pot a little heavier. Charles approves." / "That's the spirit. Keep it coming."
- **Button**: "Nice" (was "Nice!")

#### Phase 8 — index.css Cleanup
- Removed dead CSS: `mc-counter-flash`, `mc-btn-primary.pulse`, `mc-btn-refresh`, `mc-rail-divider`, `mc-display`, `mc-glow-pink`
- Removed duplicate rules: `.mc-card:hover` (identical duplicate), `.mc-plan-simple:hover`, `.mc-plan-compound:hover` (merged enhanced versions into originals)
- Removed stale comment ("More sheet removed")
- 1,002 → 933 lines

**Phase I (9)**: Invest tab polish — ROI badge animation, plan card hover lift, empty-state "no active positions" card, deposit CTA on empty wallet
**Phase J (10)**: Mobile/responsive — MobileSheet bottom-nav, header tab scroll, touch targets ≥44px, safe-area insets, viewport meta tag
**Phase K (11)**: Onboarding & docs — OnboardingTour.tsx (5-step localStorage-gated tooltip tour), GameDocs.tsx (full-page overlay with 9 collapsible accordion sections covering all game mechanics)
**Phase L (12)**: Final polish — duplicate refresh button audit (none found), Trollbox "Coming Soon" teaser in Shenanigans tab, information density audit (all 5 tabs balanced, no changes needed)

### Post-Phase Work (Feb 17, 2026)

**Merged to main**: All 63 tasks committed, PR #1 created and merged. 54 files changed, 7,195 insertions, 392 deletions.

**Splash page fixes** (post-merge, deployed directly):
1. **Invisible content bug**: `useScrollAnimate` hook refs were null on mount because splash page renders conditionally. Fixed by adding `enabled` parameter gated on `splashVisible = !isInitializing && !identity` + 100ms layout-settling delay.
2. **First card invisible**: CSS animation conflict — `.mc-card-hook` overrode `.mc-splash-cards > *`'s `mc-card-enter` animation. Fixed by combining both animations.
3. **Tagline animation**: Replaced typewriter with JS-driven spring-physics drop animation (`useSpringDrop` hook in App.tsx). Uses damped harmonic oscillator `A · e^(-ζt) · cos(ωt)` for perfectly smooth 60/120fps motion. Drops from above, bounces elastically, settles at -3° rotation.
4. **Header tagline**: Restored diagonal slant (removed erroneous `transform: none` override).

### Build & Deploy Status
- **TS errors**: 44 (all pre-existing, trending down — started at 49, fixed 5 incidentally)
- **New errors introduced**: 0 across all phases
- **Build command**: `npx vite build` (NOT `npm run build` — tsc fails)
- **Deploy command**: `dfx canister install frontend --network ic --mode upgrade`
- **Identity for deploys**: CharlesPonzi (`6pwpo-d5iaw-mfjrn-owfb3-v4oz6-72woh-pc5t2-cwn73-zrzeq-4bjeh-tqe`)
- **Live URL**: https://musicalchairs.fun/

### Key Technical Patterns

**Worktree workflow**: Development happens in `.claude/worktrees/dreamy-swanson/`. Files are copied to the main repo at project root for building/deploying.

**Vite config**: `vite.config.ts` at project root with `root: 'frontend'` and `@` alias → `frontend/src`. Must run Vite from project root.

**mc-* design system**: Custom CSS in `index.css` with design tokens, card hierarchy (mc-card, mc-card-elevated), accent variants (mc-accent-green/gold/danger/purple), glow effects, and scroll-triggered animations.

**TabType**: `'profitCenter' | 'invest' | 'seedRound' | 'mlm' | 'shenanigans'`

**localStorage keys**: `mc_tour_completed`, `mc_last_seen_referral_earnings`

**Last Sync**: Feb 17, 2026. All 63 UX tasks complete. Splash page fixes deployed.