# Musical Chairs - Project Handoff Document

**Last Updated**: February 2026  
**Project Location**: `/Users/robertripley/coding/musicalchairs`

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
│   │   ├── components/
│   │   │   ├── WalletConnectModal.tsx    # Multi-wallet selection UI
│   │   │   ├── WalletDropdown.tsx        # Deposit/Withdraw/Send interface
│   │   │   └── LoginButton.tsx           # Wallet connection button
│   │   ├── hooks/
│   │   │   ├── useWallet.tsx             # Multi-wallet context (II, Plug, OISY)
│   │   │   ├── useLedger.ts              # ICRC ledger operations
│   │   │   ├── useQueries.ts             # React Query hooks
│   │   │   └── useInternetIdentity.tsx   # Legacy compatibility wrapper
│   │   ├── lib/                          # Utility functions
│   │   └── declarations/                 # Generated Candid types
│   │       ├── backend/
│   │       │   ├── backend.did.d.ts      # Backend type definitions
│   │       │   └── index.ts              # Re-exports
│   └── index.html
├── docs/
│   ├── HANDOFF.md                         # This document
│   └── DEPLOYMENT_STATUS.md               # Technical deployment details
├── dfx.json                               # DFX canister configuration
├── canister_ids.json                      # Network-specific canister IDs
├── package.json                           # NPM dependencies
├── vite.config.ts                         # Vite bundler config
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
# Build frontend
npx vite build

# Deploy to mainnet
dfx deploy --network ic

# Or upgrade existing canister
dfx canister install frontend --network ic --mode upgrade

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

### 🔥 HIGH PRIORITY (Do First)

1. **Update Hardcoded Canister IDs**
   - [ ] Search codebase for old canister IDs and replace with new ones:
     - Old backend: `uxrrr-q7777-77774-qaaaq-cai` → New: `5zxxg-tyaaa-aaaac-qeckq-cai`
     - Old PP ledger: `awsqm-4qaaa-aaaau-aclja-cai` → New: `5xv2o-iiaaa-aaaac-qeclq-cai`
   - Files likely affected: `*.ts`, `*.tsx`, `*.mo`, `.env`, `canister_ids.json`
   - Command to find: `grep -rn "uxrrr-q7777\|awsqm-4qaaa" --include="*.ts" --include="*.tsx" --include="*.mo" .`

2. **Configure Git User Info**
   - [ ] Set git config with new identity:
     ```bash
     git config user.name "Your Name"
     git config user.email "your@email.com"
     ```

3. **Fix Frontend Build TypeScript Errors**
   - [ ] Address TypeScript errors in build output (seen in recent deployment)
   - [ ] Check `backend.did.d.ts` and type re-exports
   - [ ] Verify all wallet type exports are correct

4. **Set Up GitHub Organization**
   - [ ] Create GitHub org (options: fun/funny name vs "MusicalChairsDOTfun")
   - [ ] Transfer repository to org
   - [ ] Set up team access and branch protection

### 📋 MEDIUM PRIORITY

5. **Test Multi-Wallet Flow on Mainnet**
   - [ ] Test Internet Identity connection
   - [ ] Test Plug wallet connection
   - [ ] Test OISY wallet connection
   - [ ] Verify wallet switching works correctly
   - [ ] Test session restoration after page refresh

6. **Verify Real ICP Deposit/Withdraw Flow**
   - [ ] Ensure testMode is disabled on mainnet backend
   - [ ] Test deposit: approve → depositICP
   - [ ] Test withdrawal: withdrawICP → verify ICP received in wallet
   - [ ] Test internal transfers between players

### 🎯 LOWER PRIORITY (Future Enhancements)

7. **Domain & Hosting**
   - [ ] Configure musicalchairs.fun domain to point to frontend canister
   - [ ] Set up custom domain in IC settings
   - [ ] Test domain resolution

8. **Complete Game Features**
   - [ ] Test all three game plans end-to-end
   - [ ] Verify dealer fee distribution
   - [ ] Test all 11 Shenanigans items
   - [ ] Verify referral system (3-level MLM)
   - [ ] Test Hall of Fame leaderboards

9. **Security & Access Control**
   - [ ] Review admin access control system
   - [ ] Audit wallet transaction flows
   - [ ] Test edge cases (insufficient balance, concurrent withdrawals)
   - [ ] Review ICRC-2 approval amounts and expiration

10. **UI/UX Polish**
    - [ ] Review casino aesthetic consistency
    - [ ] Test mobile responsiveness
    - [ ] Add loading states for blockchain operations
    - [ ] Improve error messages for failed transactions

11. **Documentation**
    - [ ] Write user guide (how to play)
    - [ ] Create dealer onboarding docs
    - [ ] Document Shenanigans mechanics
    - [ ] Add API documentation for backend functions

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
- **Domain**: musicalchairs.fun (purchased, needs DNS configuration)
- **ICP Dashboard**: https://dashboard.internetcomputer.org/
- **Candid UI**: https://a4gq6-oaaaa-aaaab-qaa4q-cai.raw.icp0.io/?id=<canister-id>

---

**Last Sync**: This handoff doc created from chat sessions on Feb 5, 2026. Check `DEPLOYMENT_STATUS.md` for latest technical deployment details.