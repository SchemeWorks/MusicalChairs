# Musical Chairs 🎰

A satirical, transparent Ponzi-themed gambling game on the Internet Computer Protocol (ICP).

**⚠️ THIS IS A GAMBLING GAME! Only play with money you can afford to lose! ⚠️**

## Overview

Musical Chairs is deliberately designed to be transparent about its mechanics - openly declaring "It's a Ponzi!" The goal is to lampoon real-world crypto and gambling UX patterns through exaggeration and explicit disclosure.

## Features

- 🎮 **Game Plans**: 21-day Simple (11% daily), 15-day Compounding (12% daily), 30-day Compounding (9% daily)
- 💰 **Musical Chairs Wallet**: In-app ICP wallet system
- 🏠 **House System**: Upstream & Downstream dealers with fee distribution
- 🎲 **Shenanigans**: 11 different items affecting Ponzi Points
- 🏆 **Hall of Fame**: Top Ponzi Points holders and burners leaderboards
- 🤝 **MLM Referrals**: Three-level referral system for Ponzi Points

## Tech Stack

- **Backend**: Motoko on Internet Computer
- **Frontend**: React + TypeScript + Vite
- **Styling**: Tailwind CSS + shadcn/ui
- **Auth**: Internet Identity
- **Token**: ICRC-2 Ponzi Points token (awsqm-4qaaa-aaaau-aclja-cai)

## Prerequisites

- [dfx](https://internetcomputer.org/docs/current/developer-docs/setup/install) (version 0.15.0 or later)
- [Node.js](https://nodejs.org/) (version 18 or later)
- npm or yarn

## Local Development

### 1. Install Dependencies

```bash
npm install
```

### 2. Start Local Replica

```bash
dfx start --background --clean
```

### 3. Deploy Canisters Locally

```bash
dfx deploy
```

### 4. Start Frontend Dev Server

```bash
npm run dev
```

The app will be available at http://localhost:5173

## Mainnet Deployment

### 1. Configure Network

Update `.env` file:
```
DFX_NETWORK=ic
```

### 2. Deploy to Mainnet

```bash
dfx deploy --network ic
```

### 3. Update Environment Variables

After deployment, update the `.env` file with your canister IDs:
```
CANISTER_ID_BACKEND=<your-backend-canister-id>
```

### 4. Build and Deploy Frontend

```bash
npm run build
dfx deploy frontend --network ic
```

### 5. Configure Ponzi Points Token Minter

**IMPORTANT**: After deploying the backend, you must update the Ponzi Points token canister's minter to be your backend canister:

```bash
# Get your backend canister ID
dfx canister id backend --network ic

# Update the minter (requires admin access to the token canister)
dfx canister call awsqm-4qaaa-aaaau-aclja-cai update_minter "(principal \"<your-backend-canister-id>\")" --network ic
```

## Project Structure

```
musicalchairs/
├── backend/
│   ├── main.mo              # Main backend canister
│   ├── migration.mo         # State migration utilities
│   └── authorization/
│       └── access-control.mo # Admin access control
├── frontend/
│   ├── src/
│   │   ├── components/      # React components
│   │   ├── hooks/           # React Query hooks
│   │   ├── lib/             # Utility functions
│   │   └── declarations/    # Generated Candid types
│   └── index.html
├── dfx.json                  # DFX configuration
├── package.json              # NPM dependencies
└── vite.config.ts           # Vite configuration
```

## Important Notes

- The app starts users with 500 dummy ICP for testing (testMode)
- Disable testMode before mainnet launch
- The Ponzi Points token is already deployed at `awsqm-4qaaa-aaaau-aclja-cai`
- All fees and mechanics are transparently disclosed in the UI

## License

This project is for educational and entertainment purposes only.
