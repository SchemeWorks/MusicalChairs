# Ledger Hardware-Wallet Sign-In for Musical Chairs — Design Spec

- **Date:** 2026-06-02
- **Status:** Design only. **Recommendation: DEFER the build for the soft launch.** No code, no canister changes, no deploys are proposed by this document. The app and its canisters are LIVE on IC mainnet.
- **Scope:** Let **Ledger hardware wallets** sign in via Solana. Software ("hot") wallet sign-in already works and MUST remain unchanged.
- **Skills referenced:** `solana-dev`, `wallet-integration`, `internet-identity` (see Appendix A for how each informed this design).

---

## 0. TL;DR / Recommendation

1. **The root-cause holds and is still broken upstream as of 2026.** Phantom does not forward off-chain message signing to a connected Ledger; the wallet-standard `solana:signIn` reduces to the *same* broken path; no Phantom/Ledger/firmware version combination fixes it. The cheap escape hatch ("require min versions + use `signIn`") is **confirmed unavailable**. See §2.1.
2. **Real Ledger support requires a brand-new auth canister.** Our `siws_provider` is an immutable, version-pinned external wasm we cannot extend; replacing it would change every existing user's principal and break live positions. A new canister puts Ledger users in their own principal namespace — which is safe, because zero Ledger users can sign in today. See §3.
3. **A transaction-based sign-in is the right primary direction** (Ledger *can* sign transactions). Two viable proof mechanisms exist — off-chain verification of a signed-but-unsubmitted transaction (**A**, zero SOL) vs. broadcast-and-read-back via the existing `sol-rpc` canister (**B**, ~$0.001 fee). **A feasibility spike must pick between them** because A hinges on an unconfirmed dependency (Phantom exposing `signTransaction` for Ledger). See §4 and §8.
4. **Honest recommendation: defer.** For a soft launch, keep the graceful steer already shipped (hot wallet / Internet Identity / Plug). A Ledger-only Solana user can already get in via **Internet Identity** (passkey or hardware FIDO key) with zero engineering. The cost of real Ledger support — a new auth canister, canister-signature delegation, a mainnet deploy under the live-data caution, a frontend branch, and a security review — is high relative to the soft-launch payoff. Revisit when there is evidence of real Ledger demand. This document de-risks that future build. See §11.

---

## 1. Background & problem

Sign-in uses **SIWS (Sign-In With Solana)** backed by a standard `ic-siws` provider canister. The flow lives in `frontend/src/lib/siwsSigner.ts` → `connectSiws()`:

1. `siws_prepare_login(pubkey)` → a SIWS message struct (canister-issued nonce).
2. `buildSiwsMessageText(msg)` → canonical EIP-4361-style text.
3. `signMessage(messageBytes)` — the wallet signs the **off-chain message**. **← Ledger fails here.**
4. `siws_login(bs58(signature), pubkey, sessionPubKey, nonce)` → canister verifies the Ed25519 signature over the message text, returns login details.
5. `siws_get_delegation(...)` → a `SignedDelegation`; the frontend builds a `DelegationIdentity` and persists it.

A Ledger (via Phantom) fails at step 3 with Phantom's `ledgerUnknownSignError` → Ledger APDU `0x6a81`. Already confirmed by the team:

- Our SIWS message is spec-clean: pure ASCII, ~300 bytes (well under Ledger's 1212-byte restricted-ASCII limit). `domain="musicalchairs.fun"`, `statement="Sign in with your Solana wallet to play Musical Chairs."`.
- Blind signing was already enabled on the device.
- The **same Ledger signs transactions fine** — real SOL deposits via Phantom `signAndSendTransaction` work. Only off-chain **message** signing is broken.

A graceful fallback already ships: `frontend/src/components/WalletConnectModal.tsx` → `friendlyConnectError()` matches `/ledger|0x6a81/i` and tells Ledger users to use a hot wallet / Internet Identity / Plug. **This spec decides whether and how to make Ledger sign-in real.**

---

## 2. Current-state validation

### 2.1 Is it still broken upstream? — YES (confirmed, 2026)

The blocker is **Phantom's Ledger message-signing layer**, not our message. Verified against current primary sources:

| Question | Finding | Source |
|---|---|---|
| Does Phantom forward off-chain message signing to a Ledger? | **No.** Phantom maintainer reproduced the failure: "We have some work to do in order to support SIWS and Ledger. But I don't have an ETA." Status: acknowledged, unresolved, no ETA. | Phantom org discussion [#139](https://github.com/orgs/phantom/discussions/139) |
| Is SIWS-over-Ledger broken across wallets generally? | **Yes.** Phantom **and** Solflare **and** Jupiter all fail: "SIWS relies on off-chain message signing; … the message never reaches the hardware wallet, so authentication always fails." | supabase/auth [#2277](https://github.com/supabase/auth/issues/2277), opened 2025-12-01 |
| Does `solana:signIn` (wallet-standard) avoid the broken path? | **No.** `signIn` is not a distinct Ledger APDU — it constructs an off-chain message and reduces to the **same** path that fails on Ledger. So "require min versions + use `signIn`" is **not** a fix. | supabase/auth #2277; Phantom SIWS dev doc |
| Did the Ledger Solana *app* ever add message signing? | **Yes, at the device level** (INS-07 "SIGN SOLANA OFF-CHAIN MESSAGE", app ≥ 1.8.0 for spec-correct behavior). But Phantom is the gating layer and **does not send the instruction**. Even Anza's reference `LedgerWalletAdapter` on `main` exposes **no** `signMessage` (message-signing was merged to an example branch, not the shipping adapter). | LedgerHQ/app-solana `doc/api.md`; anza-xyz/wallet-adapter [PR #1094](https://github.com/anza-xyz/wallet-adapter/pull/1094) + `packages/wallets/ledger/src/adapter.ts` |
| Does enabling blind signing help? | **No.** Blind signing governs *transactions*; off-chain message signing is a separate instruction. Users in #139 enabled blind signing and SIWS still failed. | Phantom discussion #139 |
| Any other wallet a Ledger user could use for SIWS today? | **None confirmed.** Solflare/Jupiter fail; Backpack is unverified (no evidence it works). Ledger Live signs off-chain messages but is not a SIWS browser wallet. | supabase/auth #2277 |

**Conclusion:** A custom path is the only way to give Ledger users real Solana sign-in. There is no min-version upgrade that fixes this in the Phantom + Ledger combination.

> Note on `0x6a81`: in older Ledger Solana builds this is the generic `INS_NOT_SUPPORTED`. The modern app-solana `doc/api.md` overloads `6A81` to mean "Invalid off-chain message header" and `6A82` "Invalid off-chain message format." Either way it is consistent with the request never being satisfied by the device, which is what Phantom surfaces as `ledgerUnknownSignError`.

### 2.2 In-repo stack audit

**`siws_provider` is an immutable black box.** From `dfx.json`:

```jsonc
"siws_provider": {
  "type": "custom",
  "candid": "https://github.com/kristoferlund/ic-siws/releases/download/v0.1.0/ic_siws_provider.did",
  "wasm":   "https://github.com/kristoferlund/ic-siws/releases/download/v0.1.0/ic_siws_provider.wasm.gz",
  "init_arg": "(record { domain = \"musicalchairs.fun\"; uri = \"https://musicalchairs.fun\"; salt = \"musical-chairs-siws-v1\"; chain_id = opt \"mainnet\"; scheme = opt \"https\"; statement = opt \"Sign in with your Solana wallet to play Musical Chairs.\"; sign_in_expires_in = opt 300000000000; session_expires_in = opt 2592000000000000; targets = null; runtime_features = null })"
}
```

- Mainnet canister id: `tcm26-yqaaa-aaaac-qg2lq-cai` (`canister_ids.json`).
- `sign_in_expires_in = 300000000000` ns = **5 minutes**; `session_expires_in = 2592000000000000` ns = **30 days** (the IC delegation maximum — see `internet-identity` skill).
- **No source is vendored.** There is no `.rs`/`.mo` for this canister in the repo; only the downloaded `.wasm.gz` + candid + the `ic-siws-js` v0.1.0 client. We can configure it via init args but **cannot extend its verification logic**.
- The candid surface (from `frontend/src/declarations/siws_provider/`):
  - `siws_prepare_login : (Address) -> (PrepareLoginResponse)` → `SiwsMessage { domain; address; statement; uri; version; chain_id; nonce; issued_at; expiration_time }`
  - `siws_login : (SiwsSignature, Address, SessionKey, Nonce) -> (LoginResponse)` → `LoginDetails { expiration; user_canister_pubkey }`
  - `siws_get_delegation : (Address, SessionKey, Timestamp) -> (GetDelegationResponse) query` → `SignedDelegation { delegation { pubkey; expiration; targets }; signature }`
  - plus `get_address`, `get_caller_address`, `get_principal`.

**The crux finding: there is no Ed25519 *verification* anywhere in our Motoko stack.**

- `ponzi_math_sol/SolSigner.mo` only **signs**, via the IC management canister's `sign_with_schnorr` (`#ed25519`) at a derivation path — i.e. the canister signs as *its own* threshold Solana key. The management canister offers `schnorr_public_key` + `sign_with_schnorr` but **no verify endpoint**. There is no `mo:ed25519` import, no bundled curve25519, nothing that verifies a *user's* signature.
- `ponzi_math_sol/SolTx.mo` **builds and serializes** Solana transaction messages (`compile`, `serializeMessage`, `assembleTransaction`, `compactU16`, instruction builders for transfer/nonce/createAccount). It has **no decoder/deserializer** and **no Memo-program instruction**.
- `ponzi_math_sol/Base58.mo` provides `encode`/`decode`.
- `ponzi_math_sol/SolRpc.mo` integrates the `sol-rpc` canister (account reads incl. `parseNonceFromAccountData`, transaction send, signature/blockTime checks) with the `Multi*Result` consensus handling.
- A battle-tested **deposit-intent TTL state machine** already exists in `ponzi_math_sol/main.mo`: `DepositIntent { id; principal; …; createdAt; expiresAt; fulfilled }`, `pendingIntents`, monotonic `nextIntentId`, `INTENT_TTL_NS`, periodic detection. A sign-in **nonce** scheme can model directly on this.

> By contrast, `ic-siws` performs Ed25519 verification trivially because it is **Rust** (`ed25519-dalek` compiled to wasm). This is the single most important input to the "build path" decision (§6).

---

## 3. Why a new auth canister is unavoidable

To sign a user in, the system must (a) **prove** the user controls a Solana key, and (b) **mint an IC delegation** so the browser holds a `DelegationIdentity` and a stable principal.

- `siws_provider` cannot do (a) for Ledger: `siws_login` requires a valid Ed25519 signature over the SIWS *text*, which a Ledger cannot produce. It is immutable, so we cannot add a transaction-verifying method.
- We cannot point Ledger users at a *replacement* of `siws_provider` either: the IC principal is **self-authenticating from a canister-signature public key that embeds the issuing canister's id**. A different canister yields a **different principal for the same Solana address**, so swapping `siws_provider` would change every existing user's identity and orphan live positions.

**Therefore real Ledger support = a new, separate auth canister** that proves ownership via a Ledger-signable transaction and issues its own delegation. Consequences:

- **Ledger users form their own principal cohort.** This is **safe today** because nobody can sign in with a Ledger right now — there are no existing Ledger principals to migrate.
- **Portability edge case (document, don't block):** if a user later imports the *same* Ledger seed into a hot wallet and signs in via SIWS, they get a *different* principal than their Ledger login. This is an unusual recovery scenario; we surface it in copy rather than engineer around it.
- **Login key ≠ deposit/withdraw key.** Deposits credit the principal that holds an open intent (the user sends SOL from any wallet to a per-principal address); withdrawals take a target address parameter. So the Ledger *login* key does not constrain how the user funds or cashes out. This usefully decouples auth from money movement.

The delegation-issuing model is the correct one here. Per the `wallet-integration` skill's own decision test, the ICRC-signer popup model is explicitly **not** a session/delegation system (ICRC-46 unsupported) and is wrong for "high-frequency interactions / games / invisible writes" — i.e. exactly Musical Chairs. We replicate SIWS's *delegation* behavior, not the signer model.

---

## 4. Design: transaction-based sign-in

**Core idea:** the user authorizes a Solana **transaction** (which a Ledger can sign) whose contents bind a one-time, canister-issued **nonce** + the **domain** + (server-side) the **session key**. The canister treats a valid signature on that transaction as proof of key ownership and issues the delegation SIWS would have.

### 4.1 Shared flow

```mermaid
sequenceDiagram
    participant FE as Frontend (useWallet)
    participant Ph as Phantom + Ledger
    participant Auth as New auth canister
    participant Sol as Solana / sol-rpc

    FE->>FE: generate Ed25519 session key (as today)
    FE->>Auth: prepare_ledger_login(solanaPubkey, sessionPubkey)
    Auth->>Auth: mint nonce; store {nonce → (solanaPubkey, sessionPubkey, expiresAt)}
    Auth-->>FE: nonce (+ canonical payload to sign)
    FE->>Ph: sign a tx containing a Memo(nonce + domain)
    Ph->>Ph: Ledger displays + approves a TRANSACTION (works)
    Ph-->>FE: signature (Variant A) or tx signature after broadcast (Variant B)
    Note over FE,Sol: Variant B only: tx is broadcast & confirmed
    FE->>Auth: complete_ledger_login(nonce, proof)
    Auth->>Auth: verify proof (see A vs B), check nonce live + unused + domain
    opt Variant B
        Auth->>Sol: read confirmed tx, check fee-payer == solanaPubkey & memo == nonce
    end
    Auth->>Auth: consume nonce; mint SignedDelegation for the bound sessionPubkey
    Auth-->>FE: SignedDelegation
    FE->>FE: build DelegationIdentity, persist (same path as SIWS)
```

The session key is **bound server-side at prepare time** (the `nonce → sessionPubkey` map), so the delegation can only ever be issued for the session key the user committed to before signing.

### 4.2 Variant A — sign-only, verify off-chain (zero SOL)

- The canister returns, at prepare time, the **exact serialized transaction message** it expects (it dictates every field, including a Memo instruction carrying `nonce|domain`; the "recent blockhash" slot can be a server-chosen value since the tx is never submitted).
- The user signs with `signTransaction` (**sign, do not send**). The frontend returns only the 64-byte signature.
- The canister verifies `ed25519_verify(solanaPubkey, serializedMessage, signature)` over the bytes it already holds. No decoding, no RPC, no SOL.
- **Requires in-canister Ed25519 verification** (Rust trivial; Motoko is a gap — see §6).
- **Hinges on an UNCONFIRMED dependency:** that Phantom exposes `signTransaction` (sign-without-send) for a **Ledger**, and will sign a transaction with a non-canonical blockhash without rejecting it. Today's deposit flow uses `signAndSendTransaction`; sign-only for Ledger is unverified. **The spike must confirm this (§8).**

### 4.3 Variant B — broadcast + read-back via `sol-rpc` (~$0.001, confirmed-working)

- The frontend builds a tiny transaction with a single **Memo** instruction carrying `nonce|domain`, fee-paid by the user's Ledger key, and submits it with `signAndSendTransaction` — **the same APDU path that already works for deposits.**
- After confirmation, `complete_ledger_login(nonce, txSignature)` has the canister read the confirmed transaction via the existing `sol-rpc` integration and check: (1) confirmed/finalized; (2) fee-payer (first signer) == the claimed `solanaPubkey`; (3) a Memo instruction whose data == the issued `nonce` (+ domain); (4) within nonce TTL.
- **No in-canister Ed25519 verification needed** — Solana's validators already verified the signature; the canister only reads and checks fields.
- Costs one transaction fee (~5000 lamports ≈ $0.001). Reuses the deposit stack end-to-end. May require **adding a `getTransaction` read path** to `SolRpc.mo` if the parsed-instruction read isn't already present — a bounded, known addition.

### 4.4 A vs. B comparison

| Dimension | A — sign-only / off-chain verify | B — broadcast / read-back |
|---|---|---|
| SOL cost to log in | **None** | ~5000 lamports (~$0.001) |
| Ledger signing path | `signTransaction` (sign-only) — **unconfirmed for Ledger** | `signAndSendTransaction` — **confirmed working (deposits)** |
| In-canister Ed25519 verify | **Required** (Rust easy / Motoko gap) | **Not required** |
| RPC dependency at login | None | `sol-rpc` read (already integrated) |
| Reuse of existing code | `SolTx.serializeMessage` + new verify | `SolTx` (+ Memo ix) + `SolRpc` read path |
| Latency | Fast (no confirmation wait) | Solana confirmation wait (~seconds) |
| Primary risk | Phantom may not expose sign-only for Ledger | trivial fee + confirmation UX |

**Both need:** a Memo-instruction builder added to `SolTx.mo` (Memo program `MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr`), the nonce state machine, and the delegation-issuance machinery (§6). **The spike (§8) selects A or B.**

---

## 5. Security analysis

The bar: at least as strong as the existing SIWS path, with no weakening of the hot-wallet flow.

- **One-time nonce + short expiry.** Nonce is canister-minted, single-use (consumed on success), and expires on a short TTL — mirror `sign_in_expires_in` (5 min) and reuse the proven `DepositIntent`/`INTENT_TTL_NS` pattern. Replaying a transaction reuses its (already-consumed) nonce → rejected.
- **Domain binding.** The signed payload (Memo data in both variants; also the serialized message in A) includes `musicalchairs.fun`. A signature produced for another dApp cannot be replayed here, and ours cannot be replayed elsewhere.
- **Session-key binding — tighter than stock SIWS.** Stock `ic-siws` does *not* put the session key in the signed SIWS text (confirmed: `buildSiwsMessageText` has no session field); the session key is supplied separately at `siws_login`. Our flow commits the session key **at prepare time** (`nonce → sessionPubkey`), so the delegation can only be minted for the key the user committed to before signing — removing any "swap the session key at completion" ambiguity.
- **Replay / fixation protection.** Nonce single-use covers transaction replay. In B, optionally also record consumed tx signatures (defense in depth). Reject anonymous callers on the canister methods (per `canister-security` patterns).
- **Proof integrity.**
  - A: the canister verifies the Ed25519 signature over bytes **it** generated, so a malicious frontend cannot substitute a different message.
  - B: the canister trusts only an **on-chain-confirmed** transaction read via `sol-rpc` consensus, and checks the fee-payer pubkey itself — the frontend cannot forge confirmation.
- **No change to the existing SIWS path.** The hot-wallet flow (`siws_provider`, `siwsSigner.connectSiws`) is untouched; the new canister and the new frontend branch are purely additive.
- **Delegation hygiene.** Cap delegation lifetime at the IC max (30 days) or shorter; consider setting `targets` to scope the delegation to the game canisters (today `siws_provider` leaves `targets = null`). The `internet-identity` skill's expiry guidance applies.

Open security question for the spike/threat-model: in Variant A, can Phantom be made to sign a transaction whose blockhash is server-chosen rather than a live cluster blockhash, and does any Phantom-side simulation leak or mutate the bytes? If Phantom requires a real recent blockhash even for sign-only, the canister must obtain/track one (another `sol-rpc` read) and the "zero RPC" advantage of A shrinks.

---

## 6. Build paths (decided by the spike)

The hard part of **both** variants is **minting an IC delegation** (canister signatures over a delegation, the self-authenticating principal, certified data). Ed25519 verification (needed only by A) is the other crypto-sensitive piece.

- **Path 1 — fork/adapt `ic-siws` (Rust).** Reuse its Ed25519 verification, canister-signature delegation issuance, and principal derivation; replace only the proof step (verify a Solana transaction instead of the SIWS text). **Least security-critical code to write and audit.** This is the recommended leaning. It is a *new* canister (own id, own salt e.g. `musical-chairs-ledger-v1`), independent of the pinned `siws_provider`.
- **Path 2 — net-new Motoko canister.** Keeps the stack uniform with the Motoko backend, but requires implementing **both** Ed25519 verification (for A) **and** canister-signature delegation issuance in Motoko — substantially more security-critical code (the `certified-variables` skill covers the delegation machinery, but it is non-trivial and unproven here).

If the spike picks Variant B, Path 1's Ed25519 advantage no longer matters (B needs no in-canister verify), and the choice reduces to "reuse ic-siws's delegation issuance (Rust)" vs. "build canister-signature delegation in Motoko." Reuse still favors Rust. **The spike confirms feasibility and makes the final call.**

---

## 7. Frontend integration

The delegation/session plumbing is **method-agnostic** and fully reusable. Touch points (verified):

- **`frontend/src/hooks/useWallet.tsx`** — `connectSiws()` (lines ~376–427) finds an installed adapter, calls `adapter.connect()`, then wraps `adapter.signMessage`. **The Ledger branch belongs here**, after `adapter.connect()` (≈ line 404): detect Ledger (test-sign and catch `0x6a81`, or inspect adapter/account source) **or** offer an explicit "Ledger" option in the modal, then drive the new auth canister instead of the message-signing callback. Add a `WalletType` value (e.g. `'siws-ledger'`) alongside the existing `'none' | 'internet-identity' | 'plug' | 'oisy' | 'siws'`, and persist it under the existing `musical-chairs-wallet-type` key so `restoreSiwsSession()` resurrects it.
- **`frontend/src/lib/siwsSigner.ts`** — reuse `buildDelegationChain(...)`, `DelegationIdentity.fromDelegation(...)`, and the three localStorage keys (`musical-chairs-siws-delegation`, `-session-key`, `-pubkey`). Add a parallel `connectLedger(...)` that talks to the new canister; **do not modify** the existing `connectSiws` message path.
- **`frontend/src/components/WalletConnectModal.tsx`** — the `friendlyConnectError()` regex (`/ledger|0x6a81/i`) becomes a fallback for genuine failures; the modal gains a working Ledger affordance instead of only steering away. Until built, the existing steer copy stays.
- **New canister declarations** — generate and **hand-copy** the `.did`/`.did.d.ts`/`.did.js` into `frontend/src/declarations/<new_canister>/` (not `index.ts`), per the repo's declarations-sync rule.
- **CSP** — Variant B's `sol-rpc`/RPC reads are canister-side, so no new browser `connect-src` is needed beyond what the existing one-click deposit already allows. Variant A is fully canister-side too. Confirm against `frontend/public/.ic-assets.json` during the build.
- **Stack note (`solana-dev`):** the repo uses the legacy `@solana/wallet-adapter-*` packages (Phantom + Solflare explicit adapters; Wallet-Standard auto-discovery deferred). `solana-dev` recommends framework-kit / Wallet-Standard-first. Aligning is orthogonal to auth and out of scope here; the Ledger branch can live in the current adapter model.

---

## 8. Feasibility spike plan (the gating next step)

A small, **isolated** spike — **no mainnet changes, no deploys** — that resolves the two open decisions before any build is greenlit. Requires a physical Ledger + Phantom.

1. **Decide A vs B — the pivotal test.** With a Ledger connected in Phantom, can the dApp obtain a signature via **`signTransaction` (sign-without-send)** for a transaction carrying a Memo instruction? Test with a server-chosen blockhash and with a live blockhash.
   - **Works →** Variant A is viable (zero-SOL login).
   - **Fails / sign-only not exposed for Ledger →** Variant B (broadcast + read-back). B is already de-risked by the working deposit path.
2. **Confirm Ed25519-verify feasibility (only if A).** Stand up a throwaway Rust canister using `ed25519-dalek` (or evaluate a Motoko ed25519 verify) and verify a Phantom-Ledger transaction signature over a reconstructed `SolTx.serializeMessage` payload. Confirms Path 1 vs Path 2.
3. **Delegation-issuance reuse check.** Confirm an `ic-siws` fork can mint a delegation whose principal is stable and self-authenticating from the new canister, and that the frontend's existing `buildDelegationChain`/`DelegationIdentity` accept it unchanged.
4. **`sol-rpc` read path (only if B).** Confirm `SolRpc.mo` can fetch a confirmed transaction with enough structure to read the fee-payer and Memo data, or scope the addition.
5. **Memo builder.** Add `memoIx(text)` to a local copy of `SolTx.mo` and round-trip a serialized message.

Spike exit criteria: a one-page memo stating (a) A or B, (b) Rust-fork or Motoko, (c) any newly discovered blockers, and (d) a revised effort estimate. Only then consider promoting to an implementation plan.

---

## 9. Risks & open questions

- **R1 — Variant A's core dependency is unconfirmed.** Phantom may not expose `signTransaction` for Ledger, or may reject a non-canonical blockhash. Mitigation: the spike tests this first; B is the fallback.
- **R2 — Motoko Ed25519 verification is a gap.** No verify primitive exists in our Motoko stack and the management canister offers none. Mitigation: prefer the Rust fork (Path 1), or choose Variant B (no verify needed).
- **R3 — Canister-signature delegation is non-trivial to build fresh.** Mitigation: reuse `ic-siws`'s Rust implementation rather than reimplement in Motoko.
- **R4 — Live-canister deploy risk.** Any new mainnet canister + the frontend redeploy must follow the repo's deploy-safety discipline (explicit permission; never redeploy stateful canisters casually). The new auth canister is stateless-ish (nonces + delegation seed) but still a mainnet artifact.
- **R5 — Principal-cohort & portability.** Ledger users live in a separate principal namespace; same-seed-in-hot-wallet yields a different principal. Mitigation: document in copy; acceptable because there are no existing Ledger users.
- **R6 — Upstream could fix it later.** If Phantom ships Ledger message signing, plain SIWS would work and our custom path becomes legacy (and those users would be on a different principal). Mitigation: defer until demand is real; keep the custom path optional and clearly separate.
- **Open question:** is Solana-native sign-in *functionally* required for a Ledger user, or only a brand/UX affordance? Positions key off the IC principal; deposits/withdrawals don't require the login key to match the funding/withdrawal wallet. If it is purely UX, Internet Identity already covers Ledger users functionally (§10), strengthening "defer."

---

## 10. Alternatives compared (honestly)

| Alternative | Verdict |
|---|---|
| **Status quo — graceful steer to hot wallet / II / Plug** | ✅ **Recommended for the soft launch.** Already shipped; zero added risk. Ledger users are redirected, not blocked. |
| **Internet Identity as the Ledger user's path** | ✅ **Zero-engineering functional substitute.** II authenticates with passkeys *and* hardware FIDO security keys (`internet-identity` skill). A Ledger-holding user can sign in and play today; they just don't get a Solana-native identity. This is the backbone of the "defer" recommendation. |
| **Require min Phantom/Ledger versions + use `solana:signIn`** | ❌ **Confirmed non-viable.** `signIn` reduces to the same broken off-chain path on Ledger; no version combination fixes the Phantom+Ledger gap (§2.1). |
| **Switch to Solflare / Backpack for Ledger** | ❌ Solflare/Jupiter confirmed to fail the same way; Backpack unverified. Not a path. |
| **Another `ic-siws` fork that "handles hardware wallets"** | ❌ None known. The gap is upstream in Phantom's wallet→device layer, not in `ic-siws`; no provider fork can make Phantom forward the off-chain message. |
| **Adopt the ICRC-signer (Oisy) model for Ledger** | ❌ Wrong tool. Per `wallet-integration`, it is a per-action popup model, **not** a delegation/session system, and explicitly unsuited to games/high-frequency/invisible writes. |
| **Build transaction-based sign-in now** | ⚠️ Viable and correct in shape, but premature for a soft launch given cost vs. demand (§11). Build after the spike, if demand appears. |

---

## 11. Recommendation — DEFER (with the build de-risked)

**Keep the graceful steer for the soft launch. Do not build Ledger sign-in yet.**

Cost/benefit:

- **Benefit at soft launch is small.** Ledger-only Solana users who refuse a hot wallet are likely a minority, and they already have a working way in (**Internet Identity**, including hardware FIDO keys). Positions, deposits, and withdrawals do not require the *login* key to be the Solana key, so II is a functional substitute, not just a consolation.
- **Cost is real:** a new mainnet auth canister, canister-signature delegation, a frontend branch + declarations sync, a security review, and a deploy under the live-data caution — plus, for the cheapest variant, an unconfirmed Phantom dependency that needs hardware to test.
- **The expensive escape hatch is closed:** "just require newer versions" does not work (§2.1), so there is no quick win to grab now.

**When demand appears, the path is clear:** run the §8 spike (decides Variant A vs B and Rust vs Motoko), then write an implementation plan from §4–§7. The recommended starting hypothesis is **fork `ic-siws` (Rust)**, with **Variant A (sign-only, zero-SOL)** if the spike confirms Phantom exposes sign-only for Ledger, else **Variant B (broadcast + read-back)** as the de-risked fallback.

---

## 12. References

- Phantom org discussion #139 — SIWS with Phantom + Ledger (maintainer: "no ETA"): https://github.com/orgs/phantom/discussions/139
- supabase/auth #2277 — Solana signIn fails on Ledger (Phantom/Solflare/Jupiter), opened 2025-12-01: https://github.com/supabase/auth/issues/2277
- anza-xyz/wallet-adapter PR #1094 — Ledger message signing requires app ≥ 1.8.0; merged to example branch: https://github.com/anza-xyz/wallet-adapter/pull/1094
- anza-xyz/wallet-adapter — `packages/wallets/ledger/src/adapter.ts` (no `signMessage` on `main`)
- LedgerHQ/app-solana — `doc/api.md` (INS-07 off-chain message; 6A81/6A82 OCMS errors): https://github.com/LedgerHQ/app-solana
- Phantom SIWS developer doc (`signIn` since extension 23.11): https://phantom.com/learn/developers/sign-in-with-solana
- Anza off-chain message signing spec: https://docs.solana.com/cli/sign-offchain-message
- `kristoferlund/ic-siws` v0.1.0 (the pinned provider): https://github.com/kristoferlund/ic-siws
- In-repo: `frontend/src/lib/siwsSigner.ts`, `frontend/src/hooks/useWallet.tsx`, `frontend/src/components/WalletConnectModal.tsx`, `frontend/src/declarations/siws_provider/`, `dfx.json`, `canister_ids.json`, `ponzi_math_sol/{SolSigner,SolTx,Base58,SolRpc}.mo`, `ponzi_math_sol/main.mo` (deposit-intent state machine).

---

## Appendix A — How the referenced skills informed this design

- **`solana-dev`** — confirmed the framework-kit / Wallet-Standard-first direction and the transaction-signing primitives (fee payer, recent blockhash, signers); grounds the §7 note that the repo's legacy adapter model is fine for the Ledger branch and the §4 transaction-message reasoning (what a Solana signature actually covers).
- **`internet-identity`** — the delegation model (passkeys *and* hardware FIDO), 30-day max delegation expiry (matching the repo's `session_expires_in`), the anonymous-principal pitfall, and the per-app principal model. Basis for §10's "II is a functional substitute for Ledger users" and §5's delegation-hygiene guidance.
- **`wallet-integration`** — established that the ICRC-signer model is a per-action popup system, **not** a delegation/session system (ICRC-46 unsupported), and is explicitly unsuited to games/high-frequency/invisible writes. Basis for §3's "replicate SIWS's delegation, not the signer model" and §10's rejection of the ICRC-signer alternative.

## Appendix B — Out of scope (explicitly NOT proposed here)

- Any change to `siws_provider`, the hot-wallet SIWS flow, or existing user principals.
- Any mainnet deploy, canister install/upgrade, or frontend redeploy.
- Renaming internal identifiers (`exitToll`, `coverCharge`, etc.) — unrelated.
- Building the feature now. This document recommends **defer**; §8 is a spike, not an implementation.
