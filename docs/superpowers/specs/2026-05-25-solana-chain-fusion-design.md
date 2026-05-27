# Musical Chairs — Solana via Chain Fusion — Design Spec

**Date:** 2026-05-27 (initial draft 2026-05-25, iterated over 2 days).
**Status:** Design approved — implementation plan to follow.
**Supersedes:** [2026-05-23-solana-port-design.md](2026-05-23-solana-port-design.md) (native Anchor program approach, killed by [the 2026-05-23 premortem](2026-05-23-solana-port-premortem.md)).

## Why this exists

The 2026-05-23 spec proposed a native Solana Anchor program with an IC indexer canister, two fully isolated games, and no shared Shenanigans. The premortem run against that spec identified nine failure modes — most likely was **operator burnout** (dual Rust+Motoko stack), most dangerous was **MEV exploitation** of slot-hash RNG, and the deepest hidden assumption was *"this is a port — same product, same audience, different plumbing"* when in fact it was a new product in a new market under a familiar name with no shared social layer.

This spec pivots to **chain fusion** (everything stays Motoko on IC; threshold Ed25519 signs Solana txs from a canister; users sign into the existing app with their Solana wallet) instead of a parallel Anchor program. The pivot directly mitigates 6 of the 9 premortem failure modes — see the mitigation table at the end.

## Goal

Make Musical Chairs playable with native SOL as the deposit asset, without forking the social game. A user with a Solana wallet (Phantom / Solflare / Backpack) signs into the existing app, gets a deterministic IC principal derived from their Solana pubkey, deposits SOL into a separate `ponzi_math_sol` canister, and plays in **the same Shenanigans game** as Internet Identity users — same chat, same PP economy, same spells, same referral chain, same leaderboards.

The Ponzi side is separate (two pots, two sets of game records, two house cuts). The Shenanigans side is one game.

## Non-goals

- **Native Solana program.** No Anchor, no Rust on-chain code, no devnet program deploys. All logic lives on IC.
- **State migration.** `ponzi_math_sol` starts at zero. Existing II users keep playing `ponzi_math` unchanged.
- **SPL tokens.** Native SOL only. No USDC, no stablecoins, no other SPL assets.
- **Separate PP-SOL token.** One Ponzi Points ledger serves both pots.
- **Identity unification across wallets.** A user with both Internet Identity and a Solana wallet is treated as two different users (two principals, two profiles, two referral codes). Account linking is V2 if at all.
- **Cross-pot leaderboard scoring tied to denomination.** Leaderboards rank by PP, which is unified — so the two audiences compete fairly on the social side.
- **Chain-tribalism badges in chat.** A user is just a user. Denomination is shown on individual events (`#signup`, `#roundResult`) only, not as an identity marker.
- **Blackholing `ponzi_math_sol` in the near term.** Same upgrade-controller story we have on `ponzi_math` — admin can patch bugs until the canister is stable enough to renounce.

## Architecture summary

```
                       ┌─────────────────┐
   sign in via SIWS ──▶│  siws_provider  │── issues delegation ──┐
                       └─────────────────┘                       │
                                                                 ▼
   sign in via II ───────────────────────────────────────────────┤
                                                                 │
                                                                 ▼
                       ┌─────────────────┐         ┌──────────────────────┐
                       │   backend       │         │   shenanigans        │
                       │ (profiles,      │         │ (chat, PP econ,      │
                       │  access ctrl)   │         │  spells, referrals)  │
                       └─────────────────┘         └──────────────────────┘
                                                            │
                                  observer polls            │ polls both
                          ┌─────────────────────────────────┴──────────────────────────┐
                          ▼                                                            ▼
                  ┌──────────────────┐                                       ┌────────────────────┐
                  │  ponzi_math      │                                       │  ponzi_math_sol    │ ◀── NEW
                  │  (ICP pot)       │                                       │  (SOL pot)         │
                  └──────────────────┘                                       └─────┬──────────────┘
                          │                                                        │
                          ▼                                                        ▼
                  ┌──────────────────┐                          ┌─────────────────────────────────────┐
                  │  ICP ledger      │                          │  sol-rpc canister                   │
                  │  (ryjl3-tyaaa-)  │                          │  tghme-zyaaa-aaaar-qarca-cai        │
                  └──────────────────┘                          │                                     │
                                                                │  + management canister aaaaa-aa     │
                                                                │  (sign_with_schnorr, Ed25519)       │
                                                                └─────────────────────────────────────┘
```

Three canisters added or modified, no native Solana code:

- **NEW:** `siws_provider` — Sign-In with Solana, issues IC delegation chains.
- **NEW:** `ponzi_math_sol` — forked from `ponzi_math`, swaps ICP ledger calls for Solana RPC + threshold Ed25519 signing.
- **MODIFIED:** `shenanigans` — observer learns to poll a second source (`ponzi_math_sol`), with a second cursor and a second set of mint rates.

## What ports vs what doesn't

| `ponzi_math` feature | `ponzi_math_sol` port? | Notes |
|---|---|---|
| 3 plan types (`simple21Day`, `compounding15Day`, `compounding30Day`) | Yes | Same rules, same maturities |
| Carried interest tiers (12%/7.5%/3%) | Yes | Same percentages |
| Compounding ROI curves (1.12^15, 1.09^30) | Yes | Same `Float.pow` math, no fixed-point needed |
| Front-end load (4% cover charge) | Yes | Destination = SOL house treasury (separate from ICP) |
| Insolvency partial-payout scaling | Yes | Same logic |
| Round-based seed reserve carry | Yes | 50% of toll → next round |
| Backer system (Series A self-funded) | Yes | Same 24% bonus entitlement |
| Series B promotion at round reset | Yes | `raw_rand` from `aaaaa-aa` — NOT slot-hash. No MEV exposure. |
| Oldest Series A 35% bonus | Yes | Iteration works fine in Motoko, unlike Solana programs |
| Per-user rate limit (3 deposits/hour) | Yes | Same |
| General ledger (all events on-chain) | Yes | Same Motoko pattern |
| Admin god-view queries | Yes | Same |
| Concurrency locks | Yes | Same Motoko pattern |
| ICRC-1 transfer to user | Replaced | t-Ed25519-signed Solana SystemProgram transfer |
| ICRC-2 approve → transfer_from on deposit | Replaced | User signs SOL transfer in their wallet → per-user derived deposit address |
| Shenanigans/PP cover-charge sweep | Same | Cover charge still goes to admin treasury (SOL side has its own treasury address) |
| Test admin escape hatches | Yes | Same |
| ICRC-21 / ICRC-28 / ICRC-10 | Yes | Standard IC consent / wallet flow for any IC-side calls. SOL transfers are signed in Phantom directly. |

## Component 1: `siws_provider` canister

**Job:** Turn a Solana wallet signature into an IC `DelegationIdentity` with a deterministic principal.

**Language:** Motoko (or Rust if we use Kristofer Lund's `ic-siws` library directly — its reference impl is Rust).

**Library:** [ic-siws](https://github.com/kristoferlund/ic-siws). Analogous to `ic-siwe` (Ethereum), `ic-siwb` (Bitcoin). Provides the SIWS message format, Ed25519 verification, and delegation chain construction.

### Flow

1. Frontend asks user's wallet for their Solana pubkey via Wallet Standard.
2. Frontend calls `siws_provider.siws_prepare_login(pubkey)` → returns a SIWS message (statement, domain, nonce, expiry).
3. Frontend asks the wallet to sign the message (Phantom's `signMessage`).
4. Frontend calls `siws_provider.siws_login(signed_message, pubkey, session_pubkey)` → canister verifies Ed25519 signature, returns delegation chain.
5. Frontend constructs a `DelegationIdentity` from the chain + session key.
6. All subsequent IC calls (to `backend`, `shenanigans`, `ponzi_math_sol`) use that identity → caller principal is deterministic from `(solana_pubkey, siws_provider_canister_id)`.

### Principal derivation

```
principal = SHA224(siws_provider_canister_id || solana_pubkey) || 0x02
```

Standard self-authenticating principal derivation. Same Solana wallet → same principal forever, as long as the `siws_provider` canister ID stays the same. (Implication: never re-deploy `siws_provider` to a new ID without a migration story. Same constraint Internet Identity has.)

### State

```motoko
persistent actor SiwsProvider {
    // Pending SIWS challenges (5-minute TTL).
    // Key: solana_pubkey (base58). Value: nonce, expires_at.
    var pendingChallenges = OrderedMap.Make<Text>(Text.compare).empty<Challenge>();

    // Active delegations (cleared when expired).
    // Key: (solana_pubkey, session_pubkey). Value: delegation chain.
    var activeDelegations = ...;

    type Challenge = {
        nonce : Text;
        statement : Text;
        domain : Text;
        expiresAt : Int;  // ns
    };
}
```

### Public interface (matches ic-siws conventions)

```motoko
public shared func siws_prepare_login(pubkey : Text) : async { #ok : SiwsMessage; #err : Text };
public shared func siws_login(message : Text, signature : Blob, pubkey : Text, session_pubkey : Blob) : async { #ok : Delegation; #err : Text };
public query func siws_get_delegation(pubkey : Text, session_pubkey : Blob, expiration : Nat64) : async { #ok : SignedDelegation; #err : Text };
```

### Deployment

- Deployed once per environment (mainnet, devnet-equivalent local).
- Canister ID frozen into frontend config. **Never change this without a migration plan.**
- Cycles cost: trivial. Mostly Ed25519 verification (cheap).

## Component 2: `ponzi_math_sol` canister

**Job:** SOL-denominated mirror of `ponzi_math`. Same economic rules, different asset.

**Language:** Motoko. **Start as a literal copy of `ponzi_math/main.mo`**, then swap the ledger interface.

### Diff from `ponzi_math`

| `ponzi_math/main.mo` | `ponzi_math_sol/main.mo` |
|---|---|
| `transient let icpLedger : Ledger.LedgerActor = actor(Ledger.ICP_LEDGER_CANISTER_ID);` | `transient let solRpc : SolRpc.RpcActor = actor(SolRpc.SOL_RPC_CANISTER_ID);` |
| Initial `mintConfig` ICP-denominated amounts | (no PP minting — same as `ponzi_math`. The observer mints, not us.) |
| `await icpLedger.icrc1_transfer(...)` on payouts | `await sendSolWithdrawal(to, lamports)` — see Component 3 |
| `await icpLedger.icrc2_transfer_from(...)` on deposits | (no transfer_from. Deposit flow is asynchronous: user sends SOL to their per-user deposit address; a deposit-detection timer credits the game record.) — see Component 3 |
| `amount : Float` (ICP) | `amount : Float` (SOL) — same shape, different denomination; UI distinguishes |
| `backerPpPerIcp`, `simple21DayPpPerIcp`, etc. (in `MintConfig` on `shenanigans`) | parallel `backerPpPerSol`, `simple21DayPpPerSol`, etc. — see Shenanigans changes below |
| `gameId` namespace shared with shenanigans observer | distinct namespace — eventId prefix `game-sol-N` instead of `game-N` |

Everything else — `GameRecord`, `BackerKey`, the carried interest tier schedule, the compounding ROI math, the round reset / Series B logic, the cover charge accrual, the admin god-view queries, the general ledger — copies verbatim.

### Deposit flow

The biggest divergence from `ponzi_math`. ICP-side uses `icrc2_approve` then `icrc2_transfer_from`. SOL has no approve pattern. We use **per-user deposit addresses** instead:

1. User calls `ponzi_math_sol.getOrCreateDepositAddress()` from the frontend (caller identity = their SIWS-derived principal).
2. Canister derives a Solana address via threshold Ed25519:
   - `pubkey = sign_with_schnorr.public_key(derivation_path = [caller_principal_bytes], algorithm = Ed25519)`
   - Address = base58(pubkey)
3. Canister stores `(principal, solana_address)` mapping.
4. Frontend shows the user their deposit address + QR code.
5. User sends SOL to that address from Phantom.
6. A **deposit-detection timer** on `ponzi_math_sol` polls each known deposit address via `sol-rpc.getSignaturesForAddress` every N seconds:
   - For each new signature past the per-address `lastSeenSignature` cursor, fetch the transaction with `sol-rpc.getTransaction`.
   - Confirm it's an inbound SOL transfer (not an outbound one, not a token transfer).
   - Credit the user's open `Game` (if they have one) or create a new one based on whatever pending intent they recorded via `prepareSolDeposit(plan)`.
7. On successful credit, advance `lastSeenSignature` for that address.
8. **Sweep:** after credit, canister immediately constructs a SystemProgram transfer of the deposited lamports from the user's per-user deposit address to the canister's **pool address** (a single t-Ed25519-derived address with a fixed derivation path like `["pool"]`), signs it with `sign_with_schnorr`, and broadcasts via `sol-rpc.sendTransaction`. The per-user address is a deposit alias only — actual pot lamports live on the pool address. Same trust model as `ponzi_math` (deposited ICP enters the canister-controlled pool).

### Pool address & sweeping

The canister controls **two classes** of Solana addresses, both t-Ed25519-derived:

1. **Pool address** — singleton, derivation path `["pool"]`. Holds all pot lamports. Funds all withdrawals. The "balance" of `ponzi_math_sol`'s pot equals `getBalance(pool_address)`.
2. **Per-user deposit addresses** — one per user, derivation path `[user_principal_bytes]`. Used for **inbound deposits only**. Every successful detection is followed by a sweep to the pool address; per-user addresses should hover at near-zero balance.

The pool address also funds the durable nonce account (~0.002 SOL rent-exempt) and pays Solana network fees on every outbound tx. The operator pre-funds the pool address at deploy time (Series A seed deposit goes here).

**Why not just have users send directly to the pool address?** Same address for everyone means no way to attribute deposits without forcing a memo (which the user vetoed). Per-user addresses + sweep is the cleanest UX: user sees their own QR code, canister knows who sent what.

**Pending intent.** Because the deposit is asynchronous (user-initiated transfer, detected later), the user has to tell the canister *which plan* they're depositing into before sending the SOL. Frontend flow:

1. User picks plan (simple21 / comp15 / comp30) and amount in UI.
2. Frontend calls `ponzi_math_sol.prepareSolDeposit({ plan, expectedAmountLamports })` → returns deposit address + an `intentId`.
3. UI shows: "Send exactly X SOL to address Y. We'll detect it within ~30 seconds."
4. User sends from Phantom.
5. Detection timer matches the incoming transfer to the intent (by amount, within a tolerance window) and credits the appropriate plan.
6. If the user sends a wrong amount or the intent expires (10 min TTL), the SOL still gets credited but as a "manual review" entry the admin can resolve.

This is uglier than ICRC-2 approve, but it's the price of native Solana with no smart-contract escrow on the SOL side. Mitigated by the wallet's "this transaction sends exactly X SOL" preview.

### Withdrawal flow

1. User calls `ponzi_math_sol.withdrawEarnings(gameId)` from the frontend.
2. Canister computes the payout (same math as `ponzi_math`).
3. Canister builds a Solana tx with two instructions:
   - `advance_nonce_account` on the canister's durable nonce account (signer: pool address).
   - SystemProgram::transfer **from the pool address** to `user.depositAddress` (or a user-specified destination).
4. Canister signs both instructions with `sign_with_schnorr` using the `["pool"]` derivation path (algorithm = Ed25519).
5. Canister broadcasts via `sol-rpc.sendTransaction`.
6. Canister updates `Game.totalWithdrawn` only after the tx confirms (returned by `sendTransaction` if it succeeds, or polled via `sol-rpc.getSignatureStatuses` for finality).

### Durable nonce bootstrap

`sol-rpc.getLatestBlockhash` doesn't reach IC consensus reliably (per sol-rpc docs — fast-changing response), so all routine outbound txs use **durable nonces** instead. The nonce account itself has to be created once with a chicken-and-egg bootstrap.

**One-time admin-callable `bootstrap()` function on `ponzi_math_sol`:**

1. Derive pool address (`sign_with_schnorr.public_key` with derivation path `["pool"]`). Deterministic — no Solana tx, just an IC call.
2. Verify pool address has been funded by the operator. Required minimum: `~3M lamports` (≈ 0.003 SOL: 2× nonce-account rent-exemption ~1.44M lamports each, with a buffer). The operator's Series A seed deposit goes here too — same address, just adds to it.
3. Derive nonce account address (derivation path `["nonce"]`).
4. Build a Solana tx with two instructions:
   - SystemProgram::createAccount — funder = pool address, new account = nonce account, lamports = rent-exempt minimum, space = 80 bytes, owner = SystemProgram.
   - SystemProgram::initializeNonceAccount — nonce account = the new account, authority = pool address.
5. Sign with both `["pool"]` and `["nonce"]` derivation paths via `sign_with_schnorr`.
6. Fetch a recent blockhash via `sol-rpc.getLatestBlockhash` for this *one* bootstrap tx, retrying on failure since the endpoint is consensus-flaky (try up to 5 times with fresh blockhashes).
7. Broadcast via `sol-rpc.sendTransaction`.
8. After confirmation, fetch the initial nonce value via `sol-rpc.getAccountInfo(nonce_account_address)`, store in `lastNonceValue`.
9. Mark `bootstrapped := true`. Function is idempotent — calling it again after success returns OK without doing anything.

**After bootstrap:** every routine outbound tx prepends `advance_nonce_account` as its first instruction. The canister stores the nonce value advanced after each tx so it always knows the current nonce locally. If consistency is ever lost (e.g. a tx fails after signing but before broadcast), recovery is `sol-rpc.getAccountInfo(nonce_account_address)` to resync.

**Operator flow at deploy:** fund the pool address with at least 0.003 SOL plus your Series A seed → call `bootstrap()` from the admin UI → done. Pool address is shown in the admin UI; QR code provided.

### State

Adds to the `ponzi_math` state:

```motoko
// Per-user derived Solana deposit address.
var depositAddresses = principalMap.empty<Text>();  // principal → base58 pubkey
var addressToPrincipal = textMap.empty<Principal>();  // base58 pubkey → principal (reverse lookup)

// Deposit detection cursors.
var lastSeenSignature = textMap.empty<Text>();  // base58 pubkey → last sig processed

// Pending deposit intents.
type DepositIntent = {
    principal : Principal;
    plan : GamePlan;
    expectedAmountLamports : Nat64;
    expiresAt : Int;
    fulfilled : Bool;
};
var pendingIntents = natMap.empty<DepositIntent>();
var nextIntentId : Nat = 0;

// Canister-controlled addresses (t-Ed25519 derived).
var poolAddress : ?Text = null;          // derivation path ["pool"]; holds the pot
var nonceAccountAddress : ?Text = null;  // durable nonce account
var lastNonceValue : ?Text = null;       // current nonce; advances on every outbound tx
```

### Cycle economics

Per-poll outcall costs to `sol-rpc`:
- `getSignaturesForAddress` per known deposit address: ~50M cycles each.
- `getTransaction` per new signature: ~100M cycles each.
- `sendTransaction` per withdrawal: ~100M cycles.

Estimated steady-state (100 active users, 10 deposits/day, 5 withdrawals/day, 60-second poll interval):
- Per-poll: 100 addresses × 50M = 5B cycles (signature checks)
- Per-day: ~7.2T cycles signature polling + ~1.5T cycles on tx fetches + 0.5T on sends ≈ 9T cycles/day ≈ **$11/day**.

Mitigations:
- Skip polling for addresses with no recent activity (TTL-based skip: don't poll if no deposit intent and no signature in last 24h).
- Batch via `sol-rpc`'s generic `jsonRequest` if Helius supports JSON-RPC batching (Triton does; check sol-rpc passthrough).
- Increase poll interval to 5min for cold addresses, keep 30s for addresses with active intents.

Add CycleOps controller (`cpbhu-5iaaa-aaaad-aalta-cai`) at deploy.

## Component 3: chain fusion plumbing (modules inside `ponzi_math_sol`)

No separate adapter canister. Two Motoko modules inside `ponzi_math_sol`:

### `SolRpc.mo`

Candid actor type for `tghme-zyaaa-aaaar-qarca-cai`. No Motoko client library exists (Rust has `sol_rpc_client`); we hand-write the actor type based on the [sol-rpc-canister candid](https://github.com/dfinity/sol-rpc-canister/blob/main/canister/sol_rpc_canister.did).

```motoko
module SolRpc {
    public let SOL_RPC_CANISTER_ID : Text = "tghme-zyaaa-aaaar-qarca-cai";

    public type RpcActor = actor {
        getBalance : shared (Text, ?RpcConfig) -> async RpcResult<Nat64>;
        getSignaturesForAddress : shared (Text, ?GetSignaturesConfig, ?RpcConfig) -> async RpcResult<[ConfirmedSignature]>;
        getTransaction : shared (Text, ?GetTransactionConfig, ?RpcConfig) -> async RpcResult<?Transaction>;
        getAccountInfo : shared (Text, ?GetAccountInfoConfig, ?RpcConfig) -> async RpcResult<?AccountInfo>;
        sendTransaction : shared (Blob, ?SendTransactionConfig, ?RpcConfig) -> async RpcResult<Text>;
        getSignatureStatuses : shared ([Text], ?RpcConfig) -> async RpcResult<[?SignatureStatus]>;
        // ... etc, per sol-rpc-canister.did
    };

    // Types omitted for brevity — mirror the .did exactly.
}
```

### `SolSigner.mo`

Wraps the management canister's `sign_with_schnorr` for Ed25519 signing.

```motoko
module SolSigner {
    transient let ic : actor {
        schnorr_public_key : shared {
            canister_id : ?Principal;
            derivation_path : [Blob];
            key_id : { algorithm : { #ed25519 }; name : Text };
        } -> async { public_key : Blob; chain_code : Blob };

        sign_with_schnorr : shared {
            message : Blob;
            derivation_path : [Blob];
            key_id : { algorithm : { #ed25519 }; name : Text };
        } -> async { signature : Blob };
    } = actor "aaaaa-aa";

    public let KEY_ID = { algorithm = #ed25519; name = "key_1" };  // production key; "dfx_test_key" for local

    // Derive a Solana address for a user.
    public func deriveAddress(userPrincipal : Principal) : async Text {
        let { public_key } = await ic.schnorr_public_key({
            canister_id = null;
            derivation_path = [Principal.toBlob(userPrincipal)];
            key_id = KEY_ID;
        });
        // Base58-encode public_key (32 bytes for Ed25519).
        Base58.encode(public_key);
    };

    // Sign a Solana tx message hash with the canister's own key.
    public func signCanisterTx(message : Blob) : async Blob {
        let { signature } = await ic.sign_with_schnorr({
            message;
            derivation_path = [];  // canister's own key (no per-user derivation)
            key_id = KEY_ID;
        });
        signature;
    };
}
```

### Solana transaction construction

We build a Solana transaction message in Motoko (it's just a byte blob with a defined layout: header, account keys, recent blockhash / nonce, instructions). System Program transfer instruction is straightforward:
- `program_id = SystemProgram::ID` (well-known constant).
- Instruction data = `[2, 0, 0, 0, lamports_le_bytes...]` (the `transfer` discriminator + amount).

Write a `SolTx.mo` module that builds these bytes. No external dependencies needed — just byte-level construction.

## Shenanigans changes

The observer learns to poll a **second source**. The whole change is contained in `shenanigans/main.mo`.

### MintConfig additions

```motoko
public type MintConfig = {
    // ... existing fields ...
    simple21DayPpPerIcp : Nat;
    compounding15DayPpPerIcp : Nat;
    compounding30DayPpPerIcp : Nat;
    backerPpPerIcp : Nat;

    // NEW: SOL-side rates.
    simple21DayPpPerSol : Nat;
    compounding15DayPpPerSol : Nat;
    compounding30DayPpPerSol : Nat;
    backerPpPerSol : Nat;

    // ... existing fields ...
};
```

**Initial values for SOL rates** (matching the 30× ratio the user picked, which at current ICP rates 200/400/600 implies $75-equivalent SOL):

- `simple21DayPpPerSol = 6_000`
- `compounding15DayPpPerSol = 12_000`
- `compounding30DayPpPerSol = 18_000`
- `backerPpPerSol` = match the live `backerPpPerIcp` × 30 at deploy time

Admin-tunable via existing config update endpoint, same as the ICP rates.

### Second canister principal

```motoko
var ponziMathPrincipal : ?Principal = null;       // existing
var ponziMathSolPrincipal : ?Principal = null;    // NEW
```

With matching setter:

```motoko
public shared ({ caller }) func setPonziMathSolPrincipal(p : Principal) : async () {
    if (not (AccessControl.hasPermission(accessControlState, caller, #admin))) {
        Debug.trap("Unauthorized: admin only");
    };
    ponziMathSolPrincipal := ?p;
};
```

### Second observer cursor

```motoko
var gameIdCursor : Nat = 0;        // ICP-side cursor (existing)
var solGameIdCursor : Nat = 0;     // NEW: SOL-side cursor

var backerSeen = principalMap.empty<BackerSeen>();        // ICP-side (existing)
var solBackerSeen = principalMap.empty<BackerSeen>();     // NEW: SOL-side
```

### Observer tick changes

```motoko
func observerTick() : async () {
    if (observerRunning) return;
    if (not bootstrapped) return;
    observerRunning := true;
    try {
        await processNewGames(ponziMathPrincipal, #icp);       // existing
        await processNewGames(ponziMathSolPrincipal, #sol);    // NEW
        await processBackerDeltas(ponziMathPrincipal, #icp);   // existing
        await processBackerDeltas(ponziMathSolPrincipal, #sol);// NEW
    } catch (e) { Debug.print("Observer tick error: " # Error.message(e)); };
    observerRunning := false;
};
```

`processNewGames` and `processBackerDeltas` get a second parameter `Denomination = { #icp; #sol }` that selects which cursor, which rate fields, and which eventId prefix to use:
- Event IDs become `game-icp-N` and `game-sol-N` (was `game-N`) — distinct namespaces so ledger-level dedup via `created_at_time + memo` works.
- Rate lookups: `match denomination { #icp -> simple21DayPpPerIcp; #sol -> simple21DayPpPerSol }`.

**`#signup` chat items** include the denomination so the chat shows "Bob joined with 1.5 SOL on the Simple 21-Day Plan" vs "Alice joined with 5 ICP on the Compounding 30-Day Plan." Denomination is a property of the *event*, not of the *user*.

### No PP ledger changes

Confirmed by code review: shenanigans is the sole PP minter. No multi-minter shim, no PP-ledger refactor, no backend changes for minting. The 2026-05-25 outline initially proposed a Component 5 to refactor PP minting; that component is deleted as unnecessary.

## Frontend changes

### Wallet adapter

Add Solana Wallet Standard adapter. Use [`@solana/wallet-adapter`](https://github.com/anza-xyz/wallet-adapter) for the React integration. Phantom, Solflare, Backpack auto-detect.

### Sign-in flow

Add a "Sign in with Solana" button on the landing page next to "Sign in with Internet Identity." Flow:

```typescript
async function signInWithSolana() {
  const wallet = await selectSolanaWallet();
  const pubkey = wallet.publicKey;
  const { message } = await siwsProvider.siws_prepare_login(pubkey);
  const signature = await wallet.signMessage(new TextEncoder().encode(message));
  const sessionKey = Ed25519KeyIdentity.generate();
  const delegation = await siwsProvider.siws_login(message, signature, pubkey, sessionKey.getPublicKey().toDer());
  const identity = DelegationIdentity.fromDelegation(sessionKey, delegation);
  // Use `identity` for all subsequent IC calls.
  setActiveIdentity(identity);
}
```

After sign-in, the user has a deterministic IC principal. All subsequent UI works exactly the same as for II users — same actors, same canister calls, same Shenanigans UI.

### New widgets

- **`BuySOLFlyout` / `BuySOLWidget`** mirroring the existing `BuyPPWidget` / `BuyPPFlyout`. Calls `ponzi_math_sol.prepareSolDeposit` instead of the ICP path.
- **Deposit address QR code** for the per-user derived Solana address. User scans with Phantom mobile or pastes into desktop Phantom.
- **Withdrawal target picker**: defaults to the user's deposit address, allows overriding to a different Solana address.

### Existing components

Most components don't change. The IC actor pattern is identical for II-derived and SIWS-derived principals. A few specific touches:

- **Header chip ("Charles")**: shows the user's display name from `getUserProfile` regardless of auth source. Behind-the-chip wallet dropdown shows either II principal or Solana pubkey (base58, truncated) depending on auth source.
- **Amount formatting**: existing `formatICP` helper handles 8-decimal ICP. Add `formatSOL` for 9-decimal lamports. Game record carries denomination → UI picks the formatter.
- **No chain badge in chat** (per design decision). Chat items show denomination on the *amount* where relevant (`"deposited 1.5 SOL"`), not as a user-identity tag.

## PP mint rates table

Anchored at deploy time. Admin-tunable afterwards via existing config update endpoint.

| Plan | PP per ICP (live) | PP per SOL (initial) | Ratio |
|---|---|---|---|
| `simple21Day` | 200 | 6,000 | 30× |
| `compounding15Day` | 400 | 12,000 | 30× |
| `compounding30Day` | 600 | 18,000 | 30× |
| `backer` | (live value) | (live × 30) | 30× |

If the live `backerPpPerIcp` ≠ 800 at deploy time, the `backerPpPerSol` value at deploy gets set to `live × 30`. Both stay tunable after.

The 30× ratio implies a deploy-time SOL/ICP price ratio of 30 (i.e., SOL at $75 if ICP is $2.50). Admin should retune both rates if the ratio drifts materially.

## Milestones

### M0: SIWS sign-in works end-to-end

**Scope:** Components 1 + frontend SIWS button only.

- Deploy `siws_provider` to local + mainnet.
- Add Solana Wallet Standard adapter to frontend.
- Add "Sign in with Solana" button.
- User signs in with Phantom → gets a `DelegationIdentity` → app loads with their deterministic IC principal.
- **No game interaction.** Per user decision, the signup gift stays gated behind a real deposit. SIWS-only users in M0 are "ghosts" — they have a principal and a session, but can't post in chat (no PP), can't see themselves announced. They can call `getCallerUserProfile` (returns null), navigate the UI, and confirm sign-in works.

**Demoable to:** the operator only. Not a public-facing milestone.

**Out of scope for M0:** any `ponzi_math_sol` work, any shenanigans changes, any SOL deposit/withdrawal flows.

### M1: `ponzi_math_sol` on devnet

**Scope:** Component 2 + Component 3 (chain fusion plumbing). Devnet only.

- Fork `ponzi_math/main.mo` → `ponzi_math_sol/main.mo`. Mechanical clone.
- Replace ICP ledger calls with `SolRpc.mo` / `SolSigner.mo` modules.
- Implement per-user deposit address derivation via t-Ed25519 (`dfx_test_key` for local, real `key_1` for mainnet).
- Implement deposit detection timer.
- Implement durable nonce setup and withdrawal signing.
- Deploy `ponzi_math_sol` to mainnet, but configured to talk to Solana **devnet** RPC.
- Test deposit flow end-to-end with devnet SOL.
- Test withdrawal flow.
- **No shenanigans wiring yet.** PP isn't minting for SOL games yet.

**Risks at M1:** durable nonce setup is the trickiest piece. Budget 1-2 days for debugging the first successful t-Ed25519-signed devnet tx.

### M2: Wire shenanigans observer to poll `ponzi_math_sol`

**Scope:** Shenanigans changes.

- Extend `MintConfig` with SOL rate fields.
- Add `ponziMathSolPrincipal` setter and `solGameIdCursor` / `solBackerSeen` state.
- Refactor `processNewGames` / `processBackerDeltas` to accept a `Denomination` parameter.
- Add `#signup` and `#roundResult` chat item denomination tagging.
- Test on devnet: SOL deposit → observer mints PP → chat announces with `1.5 SOL` in the message.

**Devnet end-to-end loop works.** A Phantom user can sign in, deposit devnet SOL, see PP minted, post in chat.

### M3: Mainnet rollout

**Scope:** flip `ponzi_math_sol` from devnet to Solana mainnet, fund the pot.

- Update `SolRpc` config to point at mainnet (Helius mainnet via sol-rpc canister).
- Operator seeds the SOL pot as Series A backer.
- Update marketing surfaces (landing page, social).
- Phantom users can now deposit real SOL.

## Premortem mitigation table

How chain fusion handles the 9 failure modes from the [2026-05-23 premortem](2026-05-23-solana-port-premortem.md):

| # | Failure mode (May 23 spec) | Chain fusion (this spec) handles it by... |
|---|---|---|
| 1 | Math parity drift (Float vs fixed-point) | **Eliminated.** Same Motoko `Float.pow`, same `roundToEightDecimals`, same runtime. No fixed-point translation. SOL amounts are still `Float`, just denominated differently. |
| 2 | Audience mismatch (degen vs satire) | **Partially mitigated.** SOL users are in the same Shenanigans chat as ICP users from day one. The in-group that the satirical frame depends on is shared, not bifurcated. Still requires audience pilot before mainnet (see Risks). |
| 3 | MEV bots + Series B slot-hash exploitation | **Eliminated.** Series B selection uses `raw_rand` from `aaaaa-aa`, same as `ponzi_math`. No slot hashes, no MEV. |
| 4 | Indexer infrastructure failure | **Eliminated.** No indexer. `ponzi_math_sol` IS the source of truth. The `sol-rpc` canister is just a query helper, used in line with each user action, not as a continuous mirror. |
| 5 | Frontend adapter abstraction leaks | **Eliminated.** Same actor pattern, same Candid types, same auth flow shape. The only divergence is the deposit widget (`BuySOLFlyout` vs `BuyPPFlyout`) and amount formatting. |
| 6 | Phantom UX friction kills retention | **Partially mitigated.** Most user activity is in Shenanigans (chat, spells, reactions) which doesn't trigger Phantom popups — those calls use the IC DelegationIdentity. Only SOL deposits trigger Phantom popups (1 popup per deposit). Withdrawals are zero-popup (canister signs via t-Ed25519). Vastly better than per-action popup count of the native Anchor approach. |
| 7 | Audit unavailable or refused | **Mostly eliminated.** No Solana program to audit. The Motoko code reuses `ponzi_math`'s already-stable logic. The only new attack surface is `siws_provider` (well-known library: ic-siws) and the chain fusion plumbing inside `ponzi_math_sol` (signature verification handled by IC management canister). Reasonable to ship without a paid external audit; use the `audit-icp-cdp` skill for an internal pass. |
| 8 | Maintainer burnout (dual stack) | **Eliminated.** Single Motoko stack. Same Mops, same dfx, same deploy flow, same upgrade story. Adding `ponzi_math_sol` is one more Motoko canister, not a new language and toolchain. |
| 9 | Cold-start liquidity failure | **Mitigated.** Operator seeds Series A. Cross-pot referrals: ICP users can refer SOL users and vice versa, so the existing ICP community pulls SOL users in via referral links instead of requiring SOL users to self-bootstrap. Shenanigans social layer is shared, so SOL deposits show up in the same chat ICP users are already reading. Still requires deliberate launch playbook — see Risks. |

## Risks

### Inherited from premortem (chain fusion doesn't fully fix)

- **Audience mismatch — sophisticate cohort.** The premortem worried about Jito-adjacent quant bots reverse-engineering the tier schedule. The bot wallet still has an IC principal here, and can still extract value via optimal-timing exits. Mitigation: monitor for the "underwater sockpuppet cluster" early warning sign from #3 of the premortem. The same activity pattern would be visible.
- **Cold-start liquidity.** Operator-as-Series-A handles bootstrap, but doesn't guarantee growth. Need a real launch playbook (cross-promo posts, ICP community referrals, etc.). Premortem revised plan item #8 still applies.

### Unique to chain fusion

- **Per-user deposit address race.** If a user sends SOL before calling `prepareSolDeposit`, the deposit lands without an intent and needs admin review. Mitigation: UI flow always prepares the intent first; admin escape hatch for manual credit; intent TTL is generous (10 min).
- **`sol-rpc` canister single point of failure.** All SOL traffic flows through `tghme-zyaaa-aaaar-qarca-cai`. If it's down or degraded, `ponzi_math_sol` is offline. Mitigation: DFINITY-operated, production-grade, multi-provider behind the scenes; track its uptime independently. If sustained outage, deposit detection backs up but never loses money — the SOL is in user-controlled-derived-but-canister-signing addresses, recoverable on resumption.
- **Durable nonce account corruption.** If the nonce account gets out of sync (we sign a tx with a stale nonce), withdrawals fail until reset. Mitigation: defensive check on every withdrawal — if `sendTransaction` fails with "nonce" error, fetch the current nonce via `getAccountInfo` and retry.
- **Threshold Ed25519 key migration.** If DFINITY rotates `key_1`, all derived addresses change. Mitigation: standard IC threshold-signing concern, not chain-fusion-specific. Migration story would be: re-derive, sweep, transition.
- **Audience confusion with shared Shenanigans.** Chat will mix SOL-denominated and ICP-denominated events. Could be jarring at first. Mitigation: denomination labels on events; A/B if confusion surfaces in user testing.
- **PP rate parity over time.** The 30× ratio anchors to a deploy-time SOL/ICP price ratio. If prices drift hard, one side gets undervalued PP relative to the other. Mitigation: admin retunes via existing config endpoint; consider a quarterly review cadence.

## Open implementation details

### Premortem-required pre-launch gates

The premortem's revised plan listed 5 gates. With chain fusion, three become moot, two still apply:

| Gate (from premortem) | Status in chain fusion approach |
|---|---|
| 1. Audience pilot complete | **Still applies.** Run a small SIWS-only sign-in pilot during M0 to gauge "is this a scam" sentiment vs satirical engagement. |
| 2. Parity test suite green | **Moot.** Same Motoko code; no parity to test. |
| 3. ≥2 reputable auditors quoted | **Moot.** No Solana program. Internal audit-icp-cdp pass on `ponzi_math_sol` + `siws_provider` substitutes. |
| 4. Indexer viral-spike test | **Moot.** No indexer. The `sol-rpc` canister is DFINITY-operated and handles its own scaling. |
| 5. Operator self-assessment yes | **Still applies.** Operator confirms they want to maintain a new Motoko canister and the SOL pot. Should be easy "yes" since it's the same stack, but write it down. |

### Other open items

1. **SIWS provider canister — resolved.** Deploy the upstream Rust `ic-siws` canister as-is. Operator confirmed Rust-alongside-Motoko is fine (already true via `pp_ledger` and `internet-identity`). Build tooling: dfx handles Rust canister builds out of the box; just need the `wasm32-unknown-unknown` rustup target installed.
2. **Mainnet Solana RPC provider behind sol-rpc canister.** `sol-rpc` multiplexes Helius / Triton / QuickNode internally. Confirm which provider is default and the SLA.
3. **Deposit address QR code design.** Phantom mobile reads standard Solana URL format `solana:<address>?amount=<sol>` — easy. Make sure the QR generator supports the `amount` parameter.
4. **Cover charge SOL destination — resolved.** Mirrors the existing ICP cover charge flow:
   - `ponzi_math_sol` accrues cover charge in state field `coverChargeAccrualLamports : Nat64`, incremented on every deposit detection by 4% of the deposited amount. SOL itself sits on the pool address (commingled with pot, same as ICP cover charge commingles with the pot balance on the ICP canister).
   - `solTreasuryAddress : ?Text` config field on `ponzi_math_sol`, admin-tunable. Set to the operator's personal Phantom address at deploy.
   - New admin function `payManagementSol() : async Result<Text, Text>` builds a tx: `advance_nonce_account` + SystemProgram::transfer (from pool, to `solTreasuryAddress`, amount = `coverChargeAccrualLamports`). Signs with `["pool"]`. Broadcasts. On confirmation, resets `coverChargeAccrualLamports := 0`. Returns the tx signature.
   - Admin Wallet widget in the frontend gets a second card mirroring the ICP cover charge card: shows `coverChargeAccrualLamports` from a query, button to sweep, displays the Solana tx signature on success.
5. **Test admin escape hatches.** `ponzi_math_sol` inherits all of `ponzi_math`'s test admin hatches but parameterized for SOL. Ensure the principals authorized to call them are the operator's.
6. **Frontend bundling.** Adding `@solana/wallet-adapter` plus dependencies could add ~200KB gzipped to the frontend. Code-split the SIWS path so II-only users don't pay the cost.

## Out of scope (explicit)

- Native Solana program / Anchor / Rust on-chain code.
- IC indexer canister (no need; the canister IS the source of truth).
- Cross-chain bridging or unifying the two pots.
- Multi-token support on Solana (SPL tokens, USDC, etc.).
- Account linking (II + Solana wallet → same principal).
- Migrating existing II users to Solana.
- Mobile-app Phantom integration (web only for V1).
- Closing the IC version or in any way deprecating `ponzi_math`.

## Appendix: relevant skills

When implementing, load these:

- **`motoko`** — language pitfalls, persistent actor patterns
- **`stable-memory`** — upgrade safety for the new state
- **`https-outcalls`** — sol-rpc canister is essentially an outcall-driven service; understand transform functions, max_response_bytes
- **`cycles-management`** — CycleOps setup for `ponzi_math_sol` and `siws_provider`
- **`canister-security`** — IC-side hardening: anonymous principal rejection, reentrancy, caller guards
- **`audit-icp-cdp`** — pre-mainnet self-audit of `ponzi_math_sol` (the new canister handling money)
- **`migrating-motoko`** — `ponzi_math_sol` state will evolve; use inline migration patterns
- **`solana-dev`** — reference for the Solana side of the wire (transaction format, durable nonces, etc.); not for writing Solana programs

## Appendix: principal canister IDs

| Canister | ID |
|---|---|
| `sol-rpc` (DFINITY-operated) | `tghme-zyaaa-aaaar-qarca-cai` |
| Management canister (`sign_with_schnorr`) | `aaaaa-aa` |
| Internet Identity | `rdmx6-jaaaa-aaaaa-aaadq-cai` |
| Existing `backend` | see `canister_ids.json` |
| Existing `ponzi_math` | see `canister_ids.json` |
| Existing `shenanigans` | see `canister_ids.json` |
| NEW: `siws_provider` | TBD on first mainnet deploy |
| NEW: `ponzi_math_sol` | TBD on first mainnet deploy |
