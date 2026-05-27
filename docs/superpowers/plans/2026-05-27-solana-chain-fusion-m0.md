# Solana Chain Fusion — Milestone M0 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship "Sign in with Solana" end-to-end. A user with Phantom (or any wallet-standard Solana wallet) clicks a button on the landing page, signs a SIWS message, and ends up authenticated to Musical Chairs with a deterministic IC principal derived from their Solana pubkey. No money flows yet — this is auth plumbing only.

**Architecture:** Add a 4th wallet type (`'siws'`) to the existing `useWallet` multi-wallet abstraction. Deploy DFINITY-blessed `ic_siws_provider` Rust canister alongside the existing Motoko canisters (no language compatibility issues — codebase already mixes Rust via `pp_ledger` and `internet-identity`). Use `@solana/wallet-adapter` for wallet detection/signing, `ic-siws-js` for the SIWS flow, derive a `DelegationIdentity` from the returned chain.

**Tech Stack:** Rust (canister via `ic_siws_provider` crate), TypeScript/React (frontend extension), `@solana/wallet-adapter-base`, `@solana/wallet-adapter-react`, `ic-siws-js`, existing `@dfinity/identity` for DelegationIdentity construction.

**Spec reference:** [`docs/superpowers/specs/2026-05-25-solana-chain-fusion-design.md`](../specs/2026-05-25-solana-chain-fusion-design.md) — Component 1 + frontend SIWS section.

**Out of scope for M0:** Anything touching SOL deposits, `ponzi_math_sol`, shenanigans observer changes, mainnet rollout. M0 is demoable to the operator only; SIWS-only users without ICP deposits are "ghosts" (no PP, no chat posting) by design.

**Done when:** Operator can open the app in a fresh browser, click "Sign in with Solana," approve in Phantom, and see their deterministic IC principal in the wallet dropdown. Same Phantom wallet always produces the same principal across sessions. Existing II/Plug/Oisy login flows still work unchanged.

---

## File Structure

**New files:**
- `siws_provider/Cargo.toml` — declares the `ic_siws_provider` crate as a deployable canister
- `siws_provider/src/lib.rs` — main canister entry point (thin re-export of ic_siws_provider)
- `siws_provider/siws_provider.did` — Candid interface (vendored from upstream or generated)
- `frontend/src/lib/siwsSigner.ts` — wraps `ic-siws-js` to expose a connect/restore/disconnect API matching the existing oisySigner pattern
- `frontend/src/lib/siwsProvider.ts` — re-exports the canister actor for SIWS calls

**Modified files:**
- `dfx.json` — register `siws_provider` canister
- `package.json` — add Solana wallet adapter + ic-siws-js deps
- `frontend/src/hooks/useWallet.tsx` — extend `WalletType` union, add `connectSiws()` and `restoreSiwsSession()`, dispatch in `connect`/`initializeWallet`/`disconnect`
- `frontend/src/components/WalletConnectModal.tsx` — add Solana wallet entry to the modal
- `frontend/src/components/WalletDropdown.tsx` — show Solana pubkey (base58 truncated) for `walletType === 'siws'` users
- `frontend/.env.local` (and prod `.env`) — add `VITE_SIWS_PROVIDER_CANISTER_ID`

**Out of repo (configuration / one-time setup):**
- Operator's machine: `rustup target add wasm32-unknown-unknown` (for building Rust canisters)
- Operator's machine: install `wasm-opt` if not present (dfx may need it for size-optimization)

---

## Task 1: Verify Rust toolchain readiness

**Files:** none — environment check only.

- [ ] **Step 1: Check rustc + cargo are installed**

Run: `rustc --version && cargo --version`
Expected: both report a version (e.g. `rustc 1.79.0` or later). If not installed, run `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh` and re-run.

- [ ] **Step 2: Add the wasm32-unknown-unknown target**

Run: `rustup target add wasm32-unknown-unknown`
Expected: "info: component 'rust-std' for target 'wasm32-unknown-unknown' is up to date" (or installs if missing).

- [ ] **Step 3: Confirm dfx version is recent**

Run: `dfx --version`
Expected: dfx 0.20+ (any version that supports Rust canister builds — should already be the case since the project deploys Motoko canisters via dfx).

- [ ] **Step 4: No commit — environment check only.**

---

## Task 2: Scaffold the siws_provider Rust canister

**Files:**
- Create: `siws_provider/Cargo.toml`
- Create: `siws_provider/src/lib.rs`

- [ ] **Step 1: Look up the latest version of `ic_siws_provider` on crates.io**

Run: `curl -s https://crates.io/api/v1/crates/ic_siws_provider | jq -r '.crate.max_stable_version'`
Expected: a semver string like `0.1.0` or `0.2.x`. Record this version — you'll pin it in Cargo.toml.

If `jq` not installed, visit https://crates.io/crates/ic_siws_provider in a browser and read the latest stable version.

- [ ] **Step 2: Create the Cargo manifest**

File: `siws_provider/Cargo.toml`

```toml
[package]
name = "siws_provider"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]
path = "src/lib.rs"

[dependencies]
ic_siws_provider = "<VERSION_FROM_STEP_1>"
ic-cdk = "0.16"
candid = "0.10"
```

Replace `<VERSION_FROM_STEP_1>` with the version recorded in Step 1.

- [ ] **Step 3: Create the canister entry point**

File: `siws_provider/src/lib.rs`

```rust
// Musical Chairs SIWS provider.
//
// Re-exports the upstream `ic_siws_provider` canister wholesale. The whole
// canister is just configuration — domain/statement/scheme — supplied at
// init time. The actual SIWS protocol implementation lives in the
// ic-siws crate, which is well-tested.
//
// If we ever need to fork behavior, we'd inline the provider's source
// here and modify; today we use it stock.

pub use ic_siws_provider::*;
```

- [ ] **Step 4: Verify it builds against the wasm target**

Run: `cd siws_provider && cargo build --release --target wasm32-unknown-unknown && cd ..`
Expected: a `target/wasm32-unknown-unknown/release/siws_provider.wasm` file is produced. Compilation succeeds with no errors. Warnings are OK.

- [ ] **Step 5: Commit**

```bash
git add siws_provider/Cargo.toml siws_provider/src/lib.rs
git commit -m "feat(siws): scaffold ic_siws_provider Rust canister

Adds the Rust crate that will be deployed as siws_provider canister.
Re-exports ic_siws_provider stock; configuration via init args in dfx.json."
```

---

## Task 3: Vendor the Candid interface for siws_provider

**Files:**
- Create: `siws_provider/siws_provider.did`

- [ ] **Step 1: Fetch the upstream .did file**

Run: `curl -fsSL https://raw.githubusercontent.com/kristoferlund/ic-siws/main/packages/ic_siws_provider/ic_siws_provider.did -o siws_provider/siws_provider.did`
Expected: file is created with non-zero size. Open it and verify it contains type definitions like `LoginDetails`, `Delegation`, `PrepareLoginOkResponse`, and methods like `siws_prepare_login`, `siws_login`, `siws_get_delegation`.

If the URL 404s (upstream restructured), find the .did file by browsing https://github.com/kristoferlund/ic-siws/tree/main/packages/ic_siws_provider and copy its content into `siws_provider/siws_provider.did` manually.

- [ ] **Step 2: Verify the .did is valid Candid**

Run: `dfx --version >/dev/null && echo "dfx available"` then `(cd siws_provider && dfx generate --help >/dev/null 2>&1 || true)`
Expected: dfx is available. (Full candid validation happens when dfx.json is updated in Task 4.)

- [ ] **Step 3: Commit**

```bash
git add siws_provider/siws_provider.did
git commit -m "feat(siws): vendor upstream Candid interface

Pinned at the version compatible with ic_siws_provider crate.
Re-sync if we bump the crate version in Cargo.toml."
```

---

## Task 4: Register siws_provider in dfx.json

**Files:**
- Modify: `dfx.json`

- [ ] **Step 1: Read current dfx.json**

Run: `cat dfx.json`
Note the existing canister entries (`backend`, `ponzi_math`, `shenanigans`, `frontend`, `internet-identity`, `pp_ledger`, `pp_assets`). The new entry will sit alongside them.

- [ ] **Step 2: Add siws_provider canister entry**

Modify `dfx.json` — inside the `"canisters"` object, add a new entry. Insert it after the `"shenanigans"` entry (keeping logical grouping):

```json
    "siws_provider": {
      "type": "rust",
      "package": "siws_provider",
      "candid": "siws_provider/siws_provider.did",
      "init_arg": "(record { domain = \"musicalchairs.fun\"; uri = \"https://musicalchairs.fun\"; salt = \"musical-chairs-siws-v1\"; chain_id = opt \"mainnet\"; scheme = opt \"https\"; statement = opt \"Sign in with your Solana wallet to play Musical Chairs.\"; sign_in_expires_in = opt 300000000000; session_expires_in = opt 2592000000000000; targets = null; runtime_features = null })"
    }
```

The `init_arg` parameters:
- `domain` — must match the URL your frontend is served from. Use `"musicalchairs.fun"` for mainnet; for local dev, override at deploy time (Task 8).
- `uri` — full origin including scheme.
- `salt` — any string. Used to make derived principals canister-specific. Don't change after first deploy or all existing principals break.
- `chain_id` — `"mainnet"` for Solana mainnet (Phantom signs against this).
- `sign_in_expires_in` — 5 minutes in nanoseconds (`5 * 60 * 1_000_000_000`).
- `session_expires_in` — 30 days in nanoseconds (`30 * 24 * 60 * 60 * 1_000_000_000`).

- [ ] **Step 3: Validate dfx.json parses and builds plan**

Run: `dfx canister --network=local create siws_provider 2>&1 | head -20`
Expected: "Creating canister siws_provider..." then either "Created canister siws_provider with id …" OR "Cannot find canister id. Local network may not be running." Either is acceptable — both prove dfx parsed the new entry.

If you get a parse error on `init_arg`, the issue is JSON-escaped Candid: double-check that every internal double-quote inside `init_arg` is escaped as `\"`.

- [ ] **Step 4: Commit**

```bash
git add dfx.json
git commit -m "build(siws): register siws_provider canister in dfx.json

Type: rust, init_arg pins domain/salt/expiry for the musicalchairs.fun deploy.
Local dev will override domain at deploy time."
```

---

## Task 5: Build and deploy siws_provider locally; smoke-test the Candid surface

**Files:** none — uses existing files.

- [ ] **Step 1: Start the local dfx network**

Run: `dfx start --clean --background`
Expected: "Replica API running on 127.0.0.1:4943" (or similar). Background mode lets the rest of the steps execute in the same shell.

- [ ] **Step 2: Deploy siws_provider locally with localhost domain override**

Run:
```bash
dfx deploy siws_provider --network=local --argument '(record { domain = "localhost"; uri = "http://localhost:5173"; salt = "musical-chairs-siws-v1-local"; chain_id = opt "mainnet"; scheme = opt "http"; statement = opt "Sign in with your Solana wallet to play Musical Chairs (local dev)."; sign_in_expires_in = opt 300000000000; session_expires_in = opt 2592000000000000; targets = null; runtime_features = null })'
```
Expected: "Building canister 'siws_provider'..." → "Installing code for canister siws_provider..." → "Deployed canisters." with a canister ID printed.

If the build takes >2 minutes on first run, that's normal — Rust crates are cached afterwards.

- [ ] **Step 3: Smoke-test siws_prepare_login**

Use a hardcoded valid base58 Solana pubkey for the test (any well-known mainnet address works; we're not actually signing — just probing the interface):

Run:
```bash
dfx canister call siws_provider siws_prepare_login '("vCXgcZTphfWtukVHo5jjHsJBfTL6tWeAUcRoSAcd6PJ")'
```
Expected: a result containing a record with a `message` field (the SIWS challenge string) and an `address` field. Output looks like `(variant { Ok = record { ... } })`.

If it returns `(variant { Err = ... })`, read the error — most likely the address format is wrong. Try a different valid base58 string.

- [ ] **Step 4: Record the local canister ID for the frontend**

Run: `dfx canister id siws_provider --network=local`
Expected: a principal like `bd3sg-teaaa-aaaaa-qaaba-cai`. Copy this value.

- [ ] **Step 5: Stop local dfx**

Run: `dfx stop`

- [ ] **Step 6: No commit yet.** Frontend wiring happens in subsequent tasks.

---

## Task 6: Install Solana frontend dependencies

**Files:**
- Modify: `package.json` (via npm CLI; don't hand-edit)

- [ ] **Step 1: Install the wallet adapter and SIWS client libs**

Run from repo root:
```bash
npm install --save \
  @solana/wallet-adapter-base \
  @solana/wallet-adapter-react \
  @solana/wallet-adapter-wallets \
  @solana/web3.js \
  ic-siws-js \
  bs58
```
Expected: packages install without errors. `npm WARN` lines about peer deps may appear and are usually OK.

`bs58` is for base58 encoding/decoding pubkeys (Solana's standard format). `@solana/web3.js` is needed transitively by the wallet adapter for `PublicKey` types.

- [ ] **Step 2: Verify the lockfile updated cleanly**

Run: `git diff package.json package-lock.json | head -30`
Expected: the diff shows added entries for all six packages. No deletions of existing packages.

- [ ] **Step 3: Sanity check — bundle still builds**

Run: `npm run build`
Expected: Vite builds to `frontend/dist` with no errors. Warnings about chunk size are OK and expected (Solana libs are big — we'll codesplit later if needed; not a M0 blocker).

- [ ] **Step 4: Commit**

```bash
git add package.json package-lock.json
git commit -m "build(deps): add Solana wallet adapter and ic-siws-js

Adds @solana/wallet-adapter-{base,react,wallets}, web3.js, ic-siws-js,
and bs58. Required for SIWS sign-in. Bundle size grows ~200KB gzipped;
codesplit deferred to later."
```

---

## Task 7: Generate TypeScript bindings for siws_provider

**Files:**
- Create: `frontend/src/declarations/siws_provider/*` (generated)

- [ ] **Step 1: Start local dfx (if not running)**

Run: `dfx start --clean --background`
Expected: replica starts. If a prior local deployment hasn't been re-done since `--clean`, redeploy siws_provider per Task 5 Step 2 first.

- [ ] **Step 2: Generate Candid TypeScript bindings**

Run: `dfx generate siws_provider`
Expected: files are created under `frontend/src/declarations/siws_provider/` (or `src/declarations/siws_provider/` depending on existing project layout; check `dfx.json`'s output paths). Files include `siws_provider.did.d.ts`, `siws_provider.did.js`, and `index.js`.

If `dfx generate` fails with "canister not deployed," redeploy first per Task 5 Step 2.

- [ ] **Step 3: Inspect the generated types**

Run: `head -40 frontend/src/declarations/siws_provider/siws_provider.did.d.ts`
Expected: TypeScript declarations for the same methods you tested in Task 5 Step 3 (`siws_prepare_login`, `siws_login`, `siws_get_delegation`) and their argument/return types.

- [ ] **Step 4: Add generated declarations to git**

```bash
git add frontend/src/declarations/siws_provider
git commit -m "feat(siws): generate TypeScript bindings for siws_provider"
```

If your `.gitignore` excludes generated declarations, that's fine — the build step (`npm run generate`) regenerates them. Skip the commit in that case.

---

## Task 8: Create the SIWS signer adapter

**Files:**
- Create: `frontend/src/lib/siwsSigner.ts`

This module mirrors the pattern of `frontend/src/lib/oisySigner.ts` (which the codebase already has for OISY) so it slots cleanly into `useWallet.tsx`.

- [ ] **Step 1: Read the existing oisySigner.ts for reference**

Run: `head -100 frontend/src/lib/oisySigner.ts 2>/dev/null || echo "file not found — pattern reference unavailable, adapt freely"`
Expected: you see a module exporting `oisySigner.getAccounts()`, `restoreOisySession()`, `clearOisySigner()`. Mirror this shape.

- [ ] **Step 2: Write the SIWS signer module**

File: `frontend/src/lib/siwsSigner.ts`

```typescript
/**
 * Sign-In with Solana adapter.
 *
 * Bridges @solana/wallet-adapter (for Phantom/Solflare/Backpack signing)
 * with ic-siws-js (for the IC-side challenge/delegation flow).
 *
 * Result: a DelegationIdentity rooted in a principal deterministically
 * derived from (solana_pubkey, siws_provider_canister_id). Same wallet
 * always yields the same principal.
 *
 * Mirrors the shape of oisySigner.ts so useWallet can dispatch to it
 * the same way.
 */

import { DelegationChain, DelegationIdentity, Ed25519KeyIdentity } from '@dfinity/identity';
import { Identity } from '@dfinity/agent';
import { Principal } from '@dfinity/principal';
import bs58 from 'bs58';
import { SiwsIdentityProvider } from 'ic-siws-js';

import { canisterId as SIWS_PROVIDER_CANISTER_ID } from '../declarations/siws_provider';

// LocalStorage keys for session persistence.
const STORAGE_KEY_DELEGATION = 'musical-chairs-siws-delegation';
const STORAGE_KEY_SESSION_KEY = 'musical-chairs-siws-session-key';
const STORAGE_KEY_PUBKEY = 'musical-chairs-siws-pubkey';

export interface SiwsConnection {
  identity: Identity;
  principal: string;
  solanaPubkey: string; // base58
}

/**
 * Initiate SIWS sign-in. Pops the user's Solana wallet for a message
 * signature, returns a usable Identity bound to a deterministic principal.
 *
 * @param signMessage callback that delegates the actual message signing
 *   to the user's Solana wallet (provided by @solana/wallet-adapter)
 * @param publicKey the wallet's public key (base58 Solana address)
 */
export async function connectSiws(
  signMessage: (msg: Uint8Array) => Promise<Uint8Array>,
  publicKey: string,
): Promise<SiwsConnection> {
  const siwsProvider = new SiwsIdentityProvider({
    canisterId: SIWS_PROVIDER_CANISTER_ID,
    publicKey,
    signMessage,
  });

  // ic-siws-js handles: prepare_login → user-signs → login → DelegationChain.
  const identity = await siwsProvider.login();

  const principal = identity.getPrincipal().toString();

  // Persist for session restoration. We store the delegation + session
  // key separately because DelegationIdentity is reconstructed from parts.
  const sessionKey = (siwsProvider as any).sessionKey as Ed25519KeyIdentity;
  const delegation = (siwsProvider as any).delegationChain as DelegationChain;

  localStorage.setItem(
    STORAGE_KEY_SESSION_KEY,
    JSON.stringify(sessionKey.toJSON()),
  );
  localStorage.setItem(
    STORAGE_KEY_DELEGATION,
    JSON.stringify(delegation.toJSON()),
  );
  localStorage.setItem(STORAGE_KEY_PUBKEY, publicKey);

  return {
    identity,
    principal,
    solanaPubkey: publicKey,
  };
}

/**
 * Restore a previously-saved SIWS session. Returns null if no session
 * exists or the delegation has expired.
 */
export async function restoreSiwsSession(): Promise<SiwsConnection | null> {
  const sessionKeyJson = localStorage.getItem(STORAGE_KEY_SESSION_KEY);
  const delegationJson = localStorage.getItem(STORAGE_KEY_DELEGATION);
  const pubkey = localStorage.getItem(STORAGE_KEY_PUBKEY);

  if (!sessionKeyJson || !delegationJson || !pubkey) {
    return null;
  }

  try {
    const sessionKey = Ed25519KeyIdentity.fromJSON(sessionKeyJson);
    const delegationChain = DelegationChain.fromJSON(delegationJson);

    // Check expiry on the topmost delegation.
    const now = BigInt(Date.now()) * 1_000_000n;
    const topDelegation = delegationChain.delegations[0];
    if (topDelegation.delegation.expiration < now) {
      clearSiwsSession();
      return null;
    }

    const identity = DelegationIdentity.fromDelegation(sessionKey, delegationChain);
    return {
      identity,
      principal: identity.getPrincipal().toString(),
      solanaPubkey: pubkey,
    };
  } catch (err) {
    console.error('Failed to restore SIWS session:', err);
    clearSiwsSession();
    return null;
  }
}

/**
 * Clear all SIWS session state. Safe to call when no session exists.
 */
export function clearSiwsSession(): void {
  localStorage.removeItem(STORAGE_KEY_DELEGATION);
  localStorage.removeItem(STORAGE_KEY_SESSION_KEY);
  localStorage.removeItem(STORAGE_KEY_PUBKEY);
}

/**
 * Helper for the wallet dropdown UI: truncate a base58 pubkey to
 * "A1b2…Z9y8" format.
 */
export function truncateSolanaPubkey(pubkey: string): string {
  if (pubkey.length <= 12) return pubkey;
  return `${pubkey.slice(0, 4)}…${pubkey.slice(-4)}`;
}
```

- [ ] **Step 3: Verify TypeScript compiles**

Run: `npm run build 2>&1 | tail -20`
Expected: build succeeds OR fails with errors specifically inside `siwsSigner.ts`. If it fails:
- "Cannot find module 'ic-siws-js'" → version mismatch between the npm package shape we expected and reality. Read the actual ic-siws-js README/types under `node_modules/ic-siws-js/` and adjust the import in `siwsSigner.ts`.
- "Cannot find module '../declarations/siws_provider'" → Task 7 didn't generate declarations correctly; revisit.

If `(siwsProvider as any).sessionKey` doesn't resolve at runtime, ic-siws-js uses a different internal property name. Open `node_modules/ic-siws-js/dist/index.d.ts` and adjust the property accesses.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/lib/siwsSigner.ts
git commit -m "feat(siws): add frontend SIWS signer adapter

Mirrors oisySigner.ts shape: connectSiws / restoreSiwsSession /
clearSiwsSession. Bridges @solana/wallet-adapter signing into
ic-siws-js challenge/delegation flow. Returns a DelegationIdentity
rooted in a deterministic principal."
```

---

## Task 9: Extend useWallet to support the 'siws' wallet type

**Files:**
- Modify: `frontend/src/hooks/useWallet.tsx`

This is the keystone change — once useWallet knows about SIWS, the rest of the app inherits it for free.

- [ ] **Step 1: Update the WalletType union**

In `frontend/src/hooks/useWallet.tsx`, find the `WalletType` declaration (line 10):

```typescript
export type WalletType = 'none' | 'internet-identity' | 'plug' | 'oisy';
```

Change to:

```typescript
export type WalletType = 'none' | 'internet-identity' | 'plug' | 'oisy' | 'siws';
```

- [ ] **Step 2: Add restore branch to `initializeWallet`**

In `initializeWallet` (around line 153), find the `else if (savedWalletType === 'oisy')` block. After it, add:

```typescript
      } else if (savedWalletType === 'siws') {
        const { restoreSiwsSession } = await import('../lib/siwsSigner');
        const connection = await restoreSiwsSession();
        if (connection) {
          setIdentity(connection.identity);
          setPrincipal(connection.principal);
          setWalletType('siws');
        } else {
          localStorage.removeItem('musical-chairs-wallet-type');
        }
```

This keeps the dynamic-import pattern (siws code only loads if the user has previously connected via SIWS).

- [ ] **Step 3: Add the siws case to the connect switch**

In `connect` (around line 231), find the switch statement. After the `case 'oisy':` block, add:

```typescript
        case 'siws':
          await connectSiws();
          break;
```

- [ ] **Step 4: Add the connectSiws function**

After `connectOisy` (around line 344, end of the function), add:

```typescript
  const connectSiws = async () => {
    const { connectSiws: doConnect } = await import('../lib/siwsSigner');
    const walletAdapterMod = await import('@solana/wallet-adapter-base');
    const walletsMod = await import('@solana/wallet-adapter-wallets');

    // Detect available wallets via Wallet Standard.
    const wallets = [
      new walletsMod.PhantomWalletAdapter(),
      new walletsMod.SolflareWalletAdapter(),
    ];

    // Pick the first wallet that's available (installed in the browser).
    let adapter = wallets.find(w => w.readyState === walletAdapterMod.WalletReadyState.Installed);

    if (!adapter) {
      throw new Error('No Solana wallet detected. Install Phantom or Solflare to continue.');
    }

    // Connect (this triggers the wallet's connect popup).
    await adapter.connect();

    if (!adapter.publicKey) {
      throw new Error('Wallet did not return a public key.');
    }

    const pubkeyBase58 = adapter.publicKey.toBase58();

    // Wrap adapter.signMessage so SIWS adapter can call it generically.
    const signMessage = async (msg: Uint8Array): Promise<Uint8Array> => {
      if (!adapter!.signMessage) {
        throw new Error('Selected wallet does not support signMessage.');
      }
      return adapter!.signMessage(msg);
    };

    const connection = await doConnect(signMessage, pubkeyBase58);

    setIdentity(connection.identity);
    setPrincipal(connection.principal);
    setWalletType('siws');
  };
```

- [ ] **Step 5: Add disconnect handling for siws**

In `disconnect` (around line 350), find the if/else chain. Replace the existing `else if (walletType === 'oisy')` line with a chain that handles siws too:

```typescript
      if (walletType === 'plug' && window.ic?.plug) {
        await window.ic.plug.disconnect();
      } else if (walletType === 'oisy') {
        const { clearOisySigner } = await import('../lib/oisySigner');
        clearOisySigner();
      } else if (walletType === 'siws') {
        const { clearSiwsSession } = await import('../lib/siwsSigner');
        clearSiwsSession();
      } else if (authClient) {
        await authClient.logout();
      }
```

- [ ] **Step 6: Update isConnected to account for siws**

In the `isConnected` computation (around line 143):

```typescript
  const isConnected = walletType !== 'none' && (!!identity || walletType === 'oisy');
```

Change to:

```typescript
  const isConnected = walletType !== 'none' && (!!identity || walletType === 'oisy' || walletType === 'siws');
```

(SIWS does provide an identity, so the `!!identity` branch should already cover it — but the explicit clause makes the intent obvious and resilient if identity construction is ever deferred.)

- [ ] **Step 7: Type-check the changes**

Run: `npm run build 2>&1 | tail -30`
Expected: compiles cleanly. If you see "exhaustiveness check" errors about WalletType, you have other switch statements elsewhere — fix each.

- [ ] **Step 8: Commit**

```bash
git add frontend/src/hooks/useWallet.tsx
git commit -m "feat(siws): wire 'siws' wallet type into useWallet

Extends WalletType union, adds connectSiws (Phantom/Solflare via wallet
adapter → ic-siws-js), restores sessions from localStorage, handles
disconnect. All other components inherit SIWS support via the existing
useWallet abstraction — no further wiring needed for read-only flows."
```

---

## Task 10: Add Solana entry to WalletConnectModal

**Files:**
- Modify: `frontend/src/components/WalletConnectModal.tsx`

- [ ] **Step 1: Read the modal's existing structure**

Run: `grep -n "internet-identity\|plug\|oisy\|comingSoon\|WalletEntry" frontend/src/components/WalletConnectModal.tsx | head -20`
Note how existing wallet entries are rendered (likely a mapped array of `{ type, label, icon, ... }` objects).

- [ ] **Step 2: Locate the wallet array or JSX list**

Open `frontend/src/components/WalletConnectModal.tsx`. Search for the section that lists wallets. If it's a JSX literal with `<WalletEntry type="internet-identity" .../>` per entry, you'll add a new entry directly. If it's a `wallets = [{...}, {...}]` array, you'll add a new object.

- [ ] **Step 3: Add the SIWS wallet entry**

Pattern depends on existing shape. If the file uses an array of wallet metadata:

```typescript
const wallets = [
  // ... existing entries ...
  {
    type: 'siws' as const,
    label: 'Solana (Phantom)',
    icon: '◎', // or import a Solana logo asset
    description: 'Sign in with your Solana wallet',
    comingSoon: false,
  },
];
```

If the file uses inline JSX entries:

```tsx
<WalletEntry
  type="siws"
  label="Solana (Phantom)"
  icon="◎"
  description="Sign in with your Solana wallet"
  onClick={() => handleConnect('siws')}
  disabled={false}
/>
```

Place the new entry directly below the OISY entry to match the spec's "added alongside existing options" intent.

- [ ] **Step 4: Verify the click handler routes correctly**

The modal's existing `handleConnect = async (walletType: WalletType)` (line 35) already calls `connect(walletType)` from useWallet. Since `'siws'` is now a valid WalletType (Task 9), no handler changes needed — adding the entry is sufficient.

- [ ] **Step 5: Type-check**

Run: `npm run build 2>&1 | tail -20`
Expected: clean compile.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/components/WalletConnectModal.tsx
git commit -m "feat(siws): add 'Sign in with Solana' entry to wallet modal

User can now pick Solana alongside II / Plug / OISY. Click triggers
connectSiws() via the existing useWallet.connect dispatch."
```

---

## Task 11: Update WalletDropdown to show Solana pubkey for SIWS users

**Files:**
- Modify: `frontend/src/components/WalletDropdown.tsx`

The existing dropdown shows IC Principal text. For SIWS users, we also want to show their Solana pubkey so they know which wallet is connected.

- [ ] **Step 1: Read existing dropdown structure**

Run: `grep -n "walletType\|principal" frontend/src/components/WalletDropdown.tsx | head -20`
Note where `walletType` is consumed and where the principal display lives.

- [ ] **Step 2: Surface the Solana pubkey when walletType === 'siws'**

At the top of `WalletDropdown.tsx`, add:

```typescript
import { truncateSolanaPubkey } from '../lib/siwsSigner';
```

Find the JSX that renders the principal. Add a sibling block (or replace, depending on intent):

```tsx
{walletType === 'siws' && (
  <div className="text-xs text-zinc-400 mt-1">
    Solana: {truncateSolanaPubkey(localStorage.getItem('musical-chairs-siws-pubkey') || '—')}
  </div>
)}
```

Read the pubkey from localStorage rather than threading it through state — it's set in `siwsSigner.ts` and stable for the session.

- [ ] **Step 3: Type-check**

Run: `npm run build 2>&1 | tail -20`
Expected: clean compile.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/WalletDropdown.tsx
git commit -m "feat(siws): show Solana pubkey in wallet dropdown for SIWS users

Truncated base58 pubkey appears alongside the IC principal so users can
verify which Solana wallet is connected."
```

---

## Task 12: Local end-to-end smoke test with a real Solana wallet

**Files:** none — manual validation.

This step requires Phantom (or another wallet-standard Solana wallet) installed in the operator's browser.

- [ ] **Step 1: Start local dfx + frontend dev server**

In one shell: `dfx start --clean --background`
Deploy all canisters: `dfx deploy --network=local` (this deploys siws_provider with its localhost init args; revisit Task 5 Step 2 for the exact `--argument` if dfx complains about missing init).

In another shell: `npm run dev` (from repo root)
Expected: Vite serves at `http://localhost:5173`.

- [ ] **Step 2: Open the app in a browser with Phantom installed**

Visit `http://localhost:5173`. Open the Wallet Connect modal. You should see a "Solana (Phantom)" entry.

- [ ] **Step 3: Sign in with Phantom**

Click "Solana (Phantom)". Phantom should pop up requesting permission to connect, then a second popup requesting a signature on the SIWS challenge message.

Approve both.

Expected outcome:
- Phantom popups close.
- The app's "Connected" state updates.
- WalletDropdown shows your IC principal (54-char base32 with `-`s) and your truncated Solana pubkey.

- [ ] **Step 4: Refresh the page and verify session restoration**

Hit Cmd+R / F5. The app should restore the SIWS session without re-popping Phantom — the saved delegation is still valid. Wallet dropdown still shows the same IC principal.

- [ ] **Step 5: Verify deterministic principal**

Note the IC principal shown. Click the wallet dropdown → disconnect. Sign in again with the same Phantom wallet. The IC principal should be **identical** to the previous one.

If it changes, the SIWS provider's `salt` is being re-randomized somewhere — go back to Task 4 Step 2 and confirm `salt` is a stable literal.

- [ ] **Step 6: Verify existing II login still works**

Disconnect SIWS. Open Wallet Connect modal again. Click "Internet Identity". Standard II flow should work unchanged.

- [ ] **Step 7: Stop services**

Run: `dfx stop`
Stop the `npm run dev` process (Ctrl+C in that shell).

- [ ] **Step 8: No commit — manual validation only.**

If any step failed, debug before proceeding. The most common failure modes:
- Phantom doesn't appear in the wallet list: Phantom not installed or not enabled for `localhost`.
- "No accounts" / "signMessage not available": wallet-adapter API version mismatch — read the actual `node_modules/@solana/wallet-adapter-base/lib/index.d.ts` for current method signatures.
- Delegation expires immediately on restoration: check the `session_expires_in` init arg on siws_provider; should be 30-day nanoseconds.

---

## Task 13: Deploy siws_provider to mainnet

**Files:** none — deploy only.

- [ ] **Step 1: Confirm you have an admin identity for canister creation**

Run: `dfx identity whoami`
Expected: an identity name (e.g. `default` or your named identity). It must have cycles or controller rights to create new mainnet canisters.

If you typically use `--wallet=<wallet_canister_id>` for mainnet deploys (the existing Musical Chairs canisters were probably created this way), check `canister_ids.json` and use the same wallet identity.

- [ ] **Step 2: Create the siws_provider canister on mainnet**

Run: `dfx canister --network=ic create siws_provider`
Expected: "Created canister siws_provider with id <CANISTER_ID>." Record the canister ID.

This creates the empty canister; deploy fills it in the next step.

- [ ] **Step 3: Deploy with mainnet init args**

Run:
```bash
dfx deploy siws_provider --network=ic --argument '(record { domain = "musicalchairs.fun"; uri = "https://musicalchairs.fun"; salt = "musical-chairs-siws-v1"; chain_id = opt "mainnet"; scheme = opt "https"; statement = opt "Sign in with your Solana wallet to play Musical Chairs."; sign_in_expires_in = opt 300000000000; session_expires_in = opt 2592000000000000; targets = null; runtime_features = null })'
```
Expected: "Installing code for canister siws_provider..." then "Deployed canisters."

- [ ] **Step 4: Confirm the canister is healthy on mainnet**

Run:
```bash
dfx canister call --network=ic siws_provider siws_prepare_login '("vCXgcZTphfWtukVHo5jjHsJBfTL6tWeAUcRoSAcd6PJ")'
```
Expected: `(variant { Ok = record { ... } })` containing a SIWS challenge message scoped to `"musicalchairs.fun"`.

- [ ] **Step 5: Update canister_ids.json**

Run: `dfx canister id --network=ic siws_provider` and add the returned ID to `canister_ids.json` under a `"siws_provider"` key paired with `"ic": "<CANISTER_ID>"`. (Check git diff against the existing file pattern for the exact JSON shape.)

- [ ] **Step 6: Commit canister_ids update**

```bash
git add canister_ids.json
git commit -m "ops(siws): record mainnet canister ID for siws_provider"
```

---

## Task 14: Mainnet end-to-end validation

**Files:** none — manual validation.

- [ ] **Step 1: Build and deploy the frontend with mainnet config**

The frontend reads canister IDs from `frontend/src/declarations/siws_provider/index.js`, which dfx generates with both local and mainnet IDs. Regenerate after Task 13:

Run: `dfx generate siws_provider`

Then build and deploy the frontend canister (or your existing deploy pipeline — check `scripts/` for an existing deploy script):

Run: `dfx deploy frontend --network=ic`

- [ ] **Step 2: Visit musicalchairs.fun in a fresh browser session**

Open an incognito window. Visit https://musicalchairs.fun (or the canister's `<canister-id>.icp0.io` URL if custom domain isn't wired up yet for SIWS).

- [ ] **Step 3: Sign in with Solana**

Same flow as Task 12 Step 3. Phantom connects, signs, app authenticates. Wallet dropdown shows your IC principal + truncated Solana pubkey.

- [ ] **Step 4: Verify principal stability across sessions**

Disconnect, reconnect. Same IC principal both times.

- [ ] **Step 5: Verify ghost-mode behavior is correct**

Visit Shenanigans. You should see chat but not be able to post (no PP). This is the intended ghost mode per the spec. Confirms M0's "no signup gift untether" decision is in effect.

- [ ] **Step 6: Verify existing II users see no change**

Open another fresh browser session. Log in via II. Everything should work exactly as before M0.

- [ ] **Step 7: No commit — manual validation only.**

---

## Definition of Done for M0

All of the following must be true before declaring M0 shipped:

1. ✅ `siws_provider` canister deployed to mainnet at a recorded canister ID.
2. ✅ Frontend deployed with SIWS button visible alongside II/Plug/OISY in the wallet modal.
3. ✅ End-to-end Phantom sign-in works on mainnet (Task 14 Steps 1-4).
4. ✅ Same Phantom wallet always yields the same IC principal on mainnet.
5. ✅ Session restoration works (refresh page → no re-pop of Phantom).
6. ✅ Existing II/Plug/OISY login flows unchanged.
7. ✅ SIWS-only users without ICP deposits are in ghost mode (read-only, no PP, no chat posting).
8. ✅ No bundle size regression >250KB gzipped on the main chunk.

After M0 ships, return to spec and write M1 plan (ponzi_math_sol on devnet).

---

## Notes and gotchas to keep in mind during execution

- **`ic-siws-js` API surface may differ from our `siwsSigner.ts` assumption.** The library is younger than `ic-siwe-js`. If `(siwsProvider as any).sessionKey` doesn't exist, look at the actual exported types in `node_modules/ic-siws-js/dist/index.d.ts` and adjust the property accesses. The public `login()` method definitely exists; the internal session-key access is the volatile bit.
- **Phantom on localhost requires Phantom's developer settings to allow localhost.** If the wallet doesn't appear during Task 12, check Phantom → Settings → Developer Mode → allow `localhost`.
- **The `salt` value in dfx.json's init_arg is forever-locked.** Once mainnet siws_provider has minted any principals, changing salt re-derives everyone's principals. Treat it as immutable post-deploy.
- **Bundle codesplit is deferred.** Solana wallet adapter adds ~200KB gzipped. M0 accepts this; if it becomes a perf issue, codesplit the SIWS path so II-only users don't load the Solana libs.
- **`siws_provider`'s `domain` init_arg must match the URL the user is signing from.** Phantom embeds the domain into its signature confirmation popup. Mismatch = SIWS verification fails. Local dev needs domain=`localhost`; production needs `musicalchairs.fun`.

## Skills to load during execution

- `superpowers:subagent-driven-development` (or `executing-plans`) — the execution engine for this plan
- `superpowers:test-driven-development` — pattern guidance for any unit tests you add
- `superpowers:verification-before-completion` — required before claiming each task is done
- `motoko` and `rust-best-practices` are not heavily needed here; M0 is mostly TypeScript + a thin Rust shim. Re-load them for M1 when `ponzi_math_sol` gets written.
