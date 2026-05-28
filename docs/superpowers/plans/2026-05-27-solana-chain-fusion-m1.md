# Solana Chain Fusion — Milestone M1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship `ponzi_math_sol` — a SOL-denominated mirror of `ponzi_math` that uses chain fusion (threshold Ed25519 + DFINITY's `sol-rpc` canister) instead of an ICP ledger. The canister deploys to IC mainnet but is wired to talk to Solana **devnet** RPC; no real SOL flows. Same economic rules, same math, same backer system, same ledger structure — only the asset side of the wire is replaced.

**Architecture:** Fork `ponzi_math/main.mo` to `ponzi_math_sol/main.mo`. Keep every line of economic logic verbatim (carried-interest tiers, compounding ROI, Series A/B backer math, exit-toll distribution, round-reset, general ledger). Replace ICP-specific bits in four places only:
- **Deposit:** synchronous `icrc2_transfer_from` → asynchronous "intent + detect" via per-user t-Ed25519 deposit addresses and a polling timer that watches Solana signatures via the sol-rpc canister.
- **Withdrawal:** `icrc1_transfer` → SystemProgram::transfer instruction signed in-canister via `sign_with_schnorr` (algorithm=Ed25519) and broadcast via `sol-rpc.sendTransaction`.
- **Outbound blockhash:** `getLatestBlockhash` doesn't reach IC consensus reliably; use a **durable nonce account** (bootstrapped once) and prepend `advance_nonce_account` to every outbound tx.
- **Cover-charge sweep:** `sweepCoverCharges` (to backend principal) → `payManagementSol` (to operator's personal Phantom address).

Four new Motoko modules live inside `ponzi_math_sol/`: `Base58.mo`, `SolRpc.mo`, `SolSigner.mo`, `SolTx.mo`. No external dependencies (no native Solana code, no third-party Motoko Solana libraries).

**Tech Stack:** Motoko (canister). Threshold Ed25519 via management canister `aaaaa-aa` (`sign_with_schnorr`/`schnorr_public_key` with `algorithm = #ed25519`). DFINITY-operated `sol-rpc` canister at `tghme-zyaaa-aaaar-qarca-cai` (multi-provider Solana RPC behind consensus). Solana devnet as the target chain.

**Spec reference:** [`docs/superpowers/specs/2026-05-25-solana-chain-fusion-design.md`](../specs/2026-05-25-solana-chain-fusion-design.md) — Component 2 + Component 3 + the "Durable nonce bootstrap" subsection + the M1 milestone description.

**Out of scope for M1:**
- Shenanigans observer changes (M2): `ponzi_math_sol` just needs to expose the same query surface (`getAllGames`, `getBackerPositions`, `getCurrentRoundId`) that the observer already calls on `ponzi_math`.
- Mainnet rollout of the SOL pot (M3): we deploy to IC mainnet but configured against Solana **devnet**.
- PP minting changes: shenanigans is the sole PP minter; `ponzi_math_sol` does not touch the PP ledger.
- Public Series A backer flow (`addBackerMoney`): deferred. The operator's seed is recorded once via an admin hatch (`adminRegisterSeriesABacker`); a public deposit flow can be added later if needed.
- Native Solana programs (Anchor/Rust on-chain): explicitly killed by the 2026-05-23 premortem.

**Done when:** Operator can call `bootstrap()` once on the deployed mainnet `ponzi_math_sol` canister (configured for Solana devnet), see the durable nonce account created on Solana, run an end-to-end flow (`prepareSolDeposit` → user sends devnet SOL → detection timer credits a game record → operator triggers a withdrawal → SOL appears at the destination devnet address), and every step is reflected in the on-canister general ledger.

---

## Critical fork hygiene

`ponzi_math_sol/main.mo` is a **literal fork** of `ponzi_math/main.mo`. The premortem flagged "math parity drift" as a real risk — translating from one fixed-point world to another invites mistakes. We are NOT translating: SOL amounts stay `Float`, just denominated differently. Same `Float.pow`, same `roundToEightDecimals`, same lock primitives, same `recordLedger`, same `distributeExitToll`, same `selectPromotionCandidate`, same `triggerGameReset`, same admin guards.

**Allowed diffs from `ponzi_math/main.mo`:**
1. `actor class PonziMath` → `actor class PonziMathSol`.
2. `import Ledger "ledger"` → import the new `SolRpc`, `SolSigner`, `SolTx`, `Base58` modules.
3. `transient let icpLedger : Ledger.LedgerActor = ...` → `transient let solRpc : SolRpc.RpcActor = actor(SolRpc.SOL_RPC_CANISTER_ID);` plus management-canister actor for `sign_with_schnorr`.
4. The synchronous `createGame` is **removed**; replaced by `prepareSolDeposit` (records intent) and the deposit-detection timer (creates the game when SOL arrives).
5. Every `await icpLedger.icrc1_transfer(...)` payout is replaced by `await sendSolPayout(toAddress, lamports)` which builds + signs + broadcasts a SystemProgram::transfer from the pool address.
6. `sweepCoverCharges` → `payManagementSol` (sends to `solTreasuryAddress` instead of `BACKEND_PRINCIPAL`).
7. `addBackerMoney` is **removed** for M1 (deferred); `adminRegisterSeriesABacker` is the operator's one-shot seed-recording hatch.
8. New state for deposit addresses, intents, nonce, and pool/treasury addresses (additions, not replacements).
9. `getCanisterICPBalance` → `getCanisterSolBalance` (queries sol-rpc for the pool address's lamports).

**Forbidden diffs:**
- Re-deriving the math.
- "Improving" rounding, lock acquisition, or ledger event shapes.
- Renaming `exitToll`/`carriedInterest` identifiers (project policy — see CLAUDE.md).
- Renaming `coverCharge` identifiers (project policy — see CLAUDE.md). On the SOL side the parallel state field is `coverChargeAccrualLamports` and the sweep is `payManagementSol`, matching the spec.
- Touching `ponzi_math/main.mo` itself.

---

## File Structure

**New files** (all under `ponzi_math_sol/`):
- `ponzi_math_sol/main.mo` — forked actor; holds game records, backer positions, ledger, locks, plus new SOL chain-fusion plumbing.
- `ponzi_math_sol/Base58.mo` — base58 encoder for Solana addresses (32-byte Ed25519 pubkeys ↔ Solana address strings).
- `ponzi_math_sol/SolRpc.mo` — Candid actor type for `tghme-zyaaa-aaaar-qarca-cai`; constants for canister ID and devnet/mainnet provider tags.
- `ponzi_math_sol/SolSigner.mo` — wraps `aaaaa-aa.sign_with_schnorr` and `schnorr_public_key` for Ed25519; helpers for deriving addresses by derivation path and signing message blobs.
- `ponzi_math_sol/SolTx.mo` — pure byte-level construction of Solana transaction messages (SystemProgram::transfer, advance_nonce_account, createAccount, initializeNonceAccount); compact-u16 encoding; assembling full signed transactions.
- `ponzi_math_sol/icrc21.mo` — verbatim copy of `ponzi_math/icrc21.mo` (consent messages, trusted origins, standards table).
- `ponzi_math_sol/scripts/smoke-base58.sh` — shell helper for invoking the Base58 self-test query after deploy.
- `ponzi_math_sol/scripts/smoke-soltx.sh` — shell helper for invoking the SolTx self-test query after deploy.
- `ponzi_math_sol/scripts/bootstrap-devnet.sh` — operator runbook: pre-fund pool, call `bootstrap()`, verify nonce account on-chain.
- `ponzi_math_sol/scripts/e2e-devnet.sh` — operator runbook: prepareSolDeposit → external SOL transfer → wait for detection → withdrawEarnings → verify on devnet explorer.

**Modified files:**
- `dfx.json` — register `ponzi_math_sol` canister (type=motoko).
- `canister_ids.json` — record mainnet canister ID after Task 22.

**Out of repo (one-time setup):**
- Operator funds the pool address on **Solana devnet** with ~0.05 SOL before bootstrap (rent-exempt minimum + Series A seed + tx fee buffer).
- Phantom wallet configured to "devnet" for the operator's smoke tests.

---

## Task 1: Verify environment and read prerequisite spec sections

**Files:** none — environment check only.

- [ ] **Step 1: Confirm dfx is installed and recent**

Run: `dfx --version`
Expected: 0.20+ — same version that successfully deployed siws_provider in M0. If older, the threshold-Schnorr management-canister methods may not be exposed.

- [ ] **Step 2: Confirm mainnet identity is `CharlesPonzi`**

Run: `dfx identity whoami`
Expected: `CharlesPonzi`. If something else, the operator's principal won't match the admin allowlist baked into `ponzi_math_sol`.

- [ ] **Step 3: Confirm cycles wallet is funded**

Run: `dfx cycles balance --network=ic`
Expected: at least 4 TC. Creating a new mainnet canister + a few hundred test calls will consume <1 TC.

- [ ] **Step 4: Confirm sol-rpc canister responds on mainnet**

Run: `dfx canister --network=ic call tghme-zyaaa-aaaar-qarca-cai getSlot '(opt record { providers = vec {} : vec opt record { url : text; headers : opt vec record { name : text; value : text } } } : opt record { providers : vec opt record { url : text; headers : opt vec record { name : text; value : text } } })' 2>&1 | head -10`
Expected: either a valid `Ok` record with a slot number, or an `Err` with a specific RPC error. If you get "Cannot find canister" the canister ID is wrong.

If the call signature doesn't match (sol-rpc updates its candid sometimes), it's fine — Step 4 is a liveness check, not a strict-shape check. As long as the call reaches the canister, we're good.

- [ ] **Step 5: Read the spec sections that govern M1**

Open `docs/superpowers/specs/2026-05-25-solana-chain-fusion-design.md` and re-read:
- "Component 2: ponzi_math_sol canister" (every subsection)
- "Component 3: chain fusion plumbing"
- "Durable nonce bootstrap"
- The Risks section
- The "M1" milestone block at the bottom

If anything contradicts this plan, stop and surface the conflict before proceeding.

- [ ] **Step 6: No commit — environment check only.**

---

## Task 2: Create `ponzi_math_sol/` directory and register the canister in dfx.json

**Files:**
- Create: `ponzi_math_sol/main.mo` (empty placeholder for now)
- Modify: `dfx.json`

- [ ] **Step 1: Create the directory and placeholder main.mo**

Run:
```bash
mkdir -p ponzi_math_sol/scripts
```

Create `ponzi_math_sol/main.mo` with this minimal placeholder so dfx can parse it during the subsequent task. Task 8 will overwrite this with the full forked actor.

```motoko
import Principal "mo:base/Principal";

persistent actor class PonziMathSol(initArgs : {
    backendPrincipal : Principal;
    testAdmin : Principal;
}) = Self {
    transient let _BACKEND_PRINCIPAL : Principal = initArgs.backendPrincipal;
    transient let _TEST_ADMIN : Principal = initArgs.testAdmin;

    public query func ping() : async Text {
        "ponzi_math_sol skeleton";
    };
};
```

- [ ] **Step 2: Read the current dfx.json to confirm structure**

Run: `cat dfx.json`
Note the `"ponzi_math"` entry shape — the new entry will mirror it.

- [ ] **Step 3: Add `ponzi_math_sol` to dfx.json**

Modify `dfx.json`. Inside the `"canisters"` object, immediately after the `"ponzi_math"` entry, add:

```json
    "ponzi_math_sol": {
      "main": "ponzi_math_sol/main.mo",
      "type": "motoko"
    },
```

The block (with the existing `ponzi_math` entry shown for placement reference) should look like:

```json
    "ponzi_math": {
      "main": "ponzi_math/main.mo",
      "type": "motoko"
    },
    "ponzi_math_sol": {
      "main": "ponzi_math_sol/main.mo",
      "type": "motoko"
    },
    "shenanigans": {
```

- [ ] **Step 4: Validate dfx parses the new entry**

Run: `dfx canister --network=local create ponzi_math_sol 2>&1 | head -10`
Expected: either "Created canister ponzi_math_sol with id …" or "Local network may not be running. Run dfx start." Either confirms dfx accepted the new entry.

- [ ] **Step 5: Commit**

```bash
git add dfx.json ponzi_math_sol/main.mo
git commit -m "build(ponzi_math_sol): scaffold canister entry and placeholder actor

Registers ponzi_math_sol in dfx.json (type=motoko) mirroring the
existing ponzi_math entry. Placeholder main.mo lets dfx parse the
project until Task 8 lands the forked actor with full state + types."
```

---

## Task 3: Copy `icrc21.mo` verbatim from `ponzi_math/`

**Files:**
- Create: `ponzi_math_sol/icrc21.mo`

This module is reused as-is — ICRC-21 consent messages and ICRC-28 trusted origins don't change between the two canisters.

- [ ] **Step 1: Copy the file**

Run:
```bash
cp ponzi_math/icrc21.mo ponzi_math_sol/icrc21.mo
```

- [ ] **Step 2: Verify the copy is byte-identical**

Run: `diff ponzi_math/icrc21.mo ponzi_math_sol/icrc21.mo`
Expected: empty output (files identical).

- [ ] **Step 3: Commit**

```bash
git add ponzi_math_sol/icrc21.mo
git commit -m "feat(ponzi_math_sol): vendor icrc21.mo from ponzi_math

Verbatim copy — same ICRC-21 consent messages and ICRC-28 trusted
origins are valid for both ponzi_math and ponzi_math_sol."
```

---

## Task 4: Implement `Base58.mo` (encode + decode) with self-test query

**Files:**
- Create: `ponzi_math_sol/Base58.mo`

Solana addresses are 32-byte Ed25519 pubkeys rendered as base58. We need both directions: encode (for surfacing addresses to the frontend / QR codes) and decode (for accepting destination addresses on withdrawals).

- [ ] **Step 1: Create Base58.mo**

File: `ponzi_math_sol/Base58.mo`

```motoko
import Array "mo:base/Array";
import Blob "mo:base/Blob";
import Char "mo:base/Char";
import Iter "mo:base/Iter";
import Nat "mo:base/Nat";
import Nat8 "mo:base/Nat8";
import Nat32 "mo:base/Nat32";
import Text "mo:base/Text";

module {
    // Solana / Bitcoin base58 alphabet (no 0, O, I, l).
    private let ALPHABET : [Char] = [
        '1','2','3','4','5','6','7','8','9',
        'A','B','C','D','E','F','G','H','J','K','L','M','N','P','Q','R','S','T','U','V','W','X','Y','Z',
        'a','b','c','d','e','f','g','h','i','j','k','m','n','o','p','q','r','s','t','u','v','w','x','y','z'
    ];

    // Reverse lookup table indexed by ASCII codepoint. -1 means invalid char.
    // Build at module load.
    private let DECODE_TABLE : [Int] = buildDecodeTable();

    private func buildDecodeTable() : [Int] {
        let t = Array.init<Int>(128, -1);
        var i : Nat = 0;
        while (i < ALPHABET.size()) {
            let cp = Nat32.toNat(Char.toNat32(ALPHABET[i]));
            t[cp] := i;
            i += 1;
        };
        Array.freeze(t);
    };

    /// Encode raw bytes as a base58 string.
    public func encode(bytes : Blob) : Text {
        let arr = Blob.toArray(bytes);
        if (arr.size() == 0) { return "" };

        // Count leading zero bytes — each becomes a leading '1'.
        var leadingZeros : Nat = 0;
        var idx : Nat = 0;
        while (idx < arr.size() and arr[idx] == (0 : Nat8)) {
            leadingZeros += 1;
            idx += 1;
        };

        // Convert the input bytes to a single big-int via base-256 accumulation.
        var num : Nat = 0;
        for (b in arr.vals()) {
            num := num * 256 + Nat8.toNat(b);
        };

        // Divmod 58 down to zero, building the digits in reverse.
        var digits : [var Char] = [var];
        if (num == 0 and leadingZeros == 0) {
            // Empty input was handled above; pure all-zero input produces only
            // leading '1's via the loop below.
        };
        var reverseChars = "";
        while (num > 0) {
            let rem = num % 58;
            num := num / 58;
            reverseChars := Text.fromChar(ALPHABET[rem]) # reverseChars;
        };

        // Prepend one '1' for each leading zero byte.
        var prefix = "";
        var i : Nat = 0;
        while (i < leadingZeros) {
            prefix := prefix # "1";
            i += 1;
        };
        prefix # reverseChars;
    };

    /// Decode a base58 string back to raw bytes.
    /// Returns null on invalid characters.
    public func decode(s : Text) : ?Blob {
        if (s.size() == 0) { return ?Blob.fromArray([]) };

        // Count leading '1's — each becomes a leading zero byte.
        var leadingOnes : Nat = 0;
        let chars = Iter.toArray(Text.toIter(s));
        var i : Nat = 0;
        while (i < chars.size() and chars[i] == '1') {
            leadingOnes += 1;
            i += 1;
        };

        // Accumulate the remaining digits into a big int.
        var num : Nat = 0;
        while (i < chars.size()) {
            let cp = Nat32.toNat(Char.toNat32(chars[i]));
            if (cp >= 128) { return null };
            let v = DECODE_TABLE[cp];
            if (v < 0) { return null };
            num := num * 58 + Nat.fromInt(v);
            i += 1;
        };

        // Convert the big int back to base-256 bytes (most-significant first).
        var revBytes : [var Nat8] = [var];
        var buf = num;
        var byteList : [Nat8] = [];
        if (buf == 0) {
            byteList := [];
        } else {
            // Use a growing array via Buffer; rebuild via Array.
            let tmp = Array.init<Nat8>(64, 0); // 32 input bytes max → 64 output is safe
            var bi : Nat = 0;
            while (buf > 0) {
                tmp[bi] := Nat8.fromNat(buf % 256);
                buf := buf / 256;
                bi += 1;
            };
            // Reverse into the final array.
            let final = Array.init<Nat8>(bi, 0);
            var k : Nat = 0;
            while (k < bi) {
                final[k] := tmp[bi - 1 - k];
                k += 1;
            };
            byteList := Array.freeze(final);
        };

        // Prepend the leading-zero bytes.
        let total = leadingOnes + byteList.size();
        let out = Array.init<Nat8>(total, 0);
        var oi : Nat = leadingOnes;
        for (b in byteList.vals()) {
            out[oi] := b;
            oi += 1;
        };
        ?Blob.fromArray(Array.freeze(out));
    };

    /// Convenience: known well-formed Solana pubkey length check.
    /// Solana pubkeys are 32 bytes → base58 length 32-44 chars.
    public func isPlausibleSolanaAddress(s : Text) : Bool {
        if (s.size() < 32 or s.size() > 44) { return false };
        switch (decode(s)) {
            case (null) { false };
            case (?b) { Blob.toArray(b).size() == 32 };
        };
    };
};
```

- [ ] **Step 2: Type-check the module by deploying locally**

We can't unit-test Motoko in isolation without `mops` test infra, but if it compiles via `dfx build`, the syntax is sound. Task 21 deploys and runs self-tests via canister queries.

For now, just check that the file parses:

Run: `dfx build ponzi_math_sol --network=local 2>&1 | tail -20`
Expected: "Building canisters..." → "Generating ..." (or "Local network may not be running" if dfx isn't up — that's fine, we're checking syntax).

If you see Motoko compile errors, fix them inline. The most likely error is "field … not found" if the Motoko version doesn't have something we assumed; consult `mo:base/Array`, `mo:base/Char`, `mo:base/Nat32` docs.

- [ ] **Step 3: Commit**

```bash
git add ponzi_math_sol/Base58.mo
git commit -m "feat(ponzi_math_sol): add Base58 encode/decode module

Pure-Motoko base58 codec for Solana pubkey ↔ address-string
conversion. Uses big-int divmod over Nat (no Float). Encodes leading
zero bytes as leading '1's; decode rejects characters outside the
Solana/Bitcoin alphabet (no 0/O/I/l). Self-test query lives on the
actor (Task 8) and runs in Task 21."
```

---

## Task 5: Implement `SolRpc.mo` actor type

**Files:**
- Create: `ponzi_math_sol/SolRpc.mo`

Type-only module: declares the Candid shape of the sol-rpc canister so we can call it. The shape mirrors `sol-rpc-canister.did` v1.3.0 — we only declare the methods we actually need.

- [ ] **Step 1: Create SolRpc.mo**

File: `ponzi_math_sol/SolRpc.mo`

```motoko
import Blob "mo:base/Blob";
import Nat64 "mo:base/Nat64";
import Principal "mo:base/Principal";

module {
    // DFINITY-operated sol-rpc canister (mainnet).
    // Same canister ID is used whether we're talking to Solana devnet or
    // mainnet — the difference is conveyed via the `RpcConfig` passed on
    // every call (see `commitmentLevel` / `responseConsensus` / network
    // selection per the sol-rpc candid).
    public let SOL_RPC_CANISTER_ID : Text = "tghme-zyaaa-aaaar-qarca-cai";

    // ====================================================================
    // Common types — these mirror the sol-rpc candid shapes we depend on.
    // The full candid covers many more variants; we declare only what
    // ponzi_math_sol actually invokes. If a method we add later needs a
    // type that isn't here, extend this module in the same task.
    // ====================================================================

    public type Provider = {
        #devnet;
        #mainnet;
    };

    public type RpcConfig = {
        provider : ?Provider;
        // Other fields (responseConsensus, requestCostMultiplier, etc.) are
        // omitted; defaults are fine for M1.
    };

    public type RpcError = {
        #ProviderError : Text;
        #HttpOutcallError : Text;
        #JsonRpcError : { code : Int; message : Text };
        #ConsensusError : Text;
        #ValidationError : Text;
    };

    public type RpcResult<T> = { #Ok : T; #Err : RpcError };

    // getBalance returns lamports as Nat64.
    public type Lamports = Nat64;

    // ConfirmedSignature returned by getSignaturesForAddress.
    public type ConfirmedSignature = {
        signature : Text; // base58
        slot : Nat64;
        err : ?Text;
        memo : ?Text;
        blockTime : ?Int;
        confirmationStatus : ?Text; // "processed" | "confirmed" | "finalized"
    };

    public type GetSignaturesConfig = {
        limit : ?Nat;          // max 1000; default 1000
        before : ?Text;        // start before this signature
        until : ?Text;         // stop at this signature
        commitment : ?Text;    // "processed" | "confirmed" | "finalized"
    };

    public type GetTransactionConfig = {
        commitment : ?Text;
        maxSupportedTransactionVersion : ?Nat64; // 0 covers the common cases
        encoding : ?Text; // "json" | "jsonParsed" | "base64"
    };

    // Simplified Transaction shape: we only need the postTokenBalances /
    // accountKeys / lamports deltas to detect inbound transfers.
    public type ParsedTransaction = {
        slot : Nat64;
        blockTime : ?Int;
        meta : ?TransactionMeta;
        transaction : ?TransactionDetail;
    };

    public type TransactionMeta = {
        err : ?Text;
        fee : Nat64;
        preBalances : [Nat64];
        postBalances : [Nat64];
    };

    public type TransactionDetail = {
        message : ?TransactionMessage;
        signatures : [Text];
    };

    public type TransactionMessage = {
        accountKeys : [Text]; // base58 pubkeys in canonical account order
        recentBlockhash : Text;
    };

    public type GetAccountInfoConfig = {
        commitment : ?Text;
        encoding : ?Text;
    };

    public type AccountInfo = {
        lamports : Nat64;
        owner : Text;
        executable : Bool;
        rentEpoch : Nat64;
        data : Blob;
    };

    public type SendTransactionConfig = {
        skipPreflight : ?Bool;
        preflightCommitment : ?Text;
        maxRetries : ?Nat64;
        encoding : ?Text; // "base58" | "base64"
    };

    public type SignatureStatus = {
        slot : Nat64;
        confirmations : ?Nat64;
        err : ?Text;
        confirmationStatus : ?Text;
    };

    // ====================================================================
    // Actor type
    // ====================================================================

    public type RpcActor = actor {
        getBalance : shared (Text, ?RpcConfig) -> async RpcResult<Lamports>;
        getSignaturesForAddress : shared (Text, ?GetSignaturesConfig, ?RpcConfig) -> async RpcResult<[ConfirmedSignature]>;
        getTransaction : shared (Text, ?GetTransactionConfig, ?RpcConfig) -> async RpcResult<?ParsedTransaction>;
        getAccountInfo : shared (Text, ?GetAccountInfoConfig, ?RpcConfig) -> async RpcResult<?AccountInfo>;
        sendTransaction : shared (Blob, ?SendTransactionConfig, ?RpcConfig) -> async RpcResult<Text>;
        getSignatureStatuses : shared ([Text], ?RpcConfig) -> async RpcResult<[?SignatureStatus]>;
        getLatestBlockhash : shared (?RpcConfig) -> async RpcResult<{ blockhash : Text; lastValidBlockHeight : Nat64 }>;
    };
};
```

> **Note:** The exact sol-rpc candid shape may have minor differences from this module (field ordering, optional vs required, additional variants). The module compiles against the IC's structural subtyping — Motoko only checks the methods you actually call. If a call fails at runtime with a type error, fetch the live candid via `dfx canister --network=ic metadata tghme-zyaaa-aaaar-qarca-cai candid:service > /tmp/sol-rpc.did` and adjust the affected type. Track such adjustments in the commit message for traceability.

- [ ] **Step 2: Verify it parses**

Run: `dfx build ponzi_math_sol --network=local 2>&1 | tail -20`
Expected: clean build (or "Local network may not be running"). No type errors.

- [ ] **Step 3: Commit**

```bash
git add ponzi_math_sol/SolRpc.mo
git commit -m "feat(ponzi_math_sol): add SolRpc actor type module

Type-only declarations for the methods we call on the DFINITY-operated
sol-rpc canister (tghme-zyaaa-aaaar-qarca-cai): getBalance,
getSignaturesForAddress, getTransaction, getAccountInfo,
sendTransaction, getSignatureStatuses, getLatestBlockhash. Network
selection (devnet vs mainnet) is conveyed per-call via RpcConfig.

If the live candid drifts from what's declared here, fetch it via
'dfx canister metadata' and update this module — Motoko's structural
typing only checks methods actually invoked, so partial-coverage is
safe."
```

---

## Task 6: Implement `SolTx.mo` (transaction byte construction)

**Files:**
- Create: `ponzi_math_sol/SolTx.mo`

Solana transactions are deterministic byte layouts. This module assembles them in pure Motoko.

**Reference layout** (Solana docs):
- A signed transaction = `compact-array<Signature[64 bytes]>` + Message.
- Message = Header(3 bytes) + `compact-array<Pubkey[32 bytes]>` (account keys) + recentBlockhash(32 bytes) + `compact-array<CompiledInstruction>`.
- Header = (num_required_signatures, num_readonly_signed_accounts, num_readonly_unsigned_accounts) each 1 byte.
- CompiledInstruction = program_id_index(1 byte) + `compact-array<u8>`(account indices) + `compact-array<u8>`(data).
- `compact-array<T>` = compact-u16 length + raw bytes/items.
- compact-u16 = variable-length 1–3 bytes (high bit set on continuation).

**SystemProgram::transfer** instruction:
- program_id = System Program (`11111111111111111111111111111111`, 32 zero bytes).
- accounts: [from (signer, writable), to (writable)].
- data = `[2, 0, 0, 0, lamports_u64_le_bytes(8 bytes)]`. Discriminator 2 = Transfer.

**System::advance_nonce_account** instruction:
- accounts: [nonce_account (writable), RecentBlockhashes sysvar `SysvarRecentB1ockHashes11111111111111111111` (read-only — and yes, lowercase "L" in the address), nonce_authority (signer)].
- data = `[4, 0, 0, 0]`. Discriminator 4 = AdvanceNonceAccount.

**System::createAccount** instruction:
- accounts: [funding_account (signer, writable), new_account (signer, writable)].
- data = `[0, 0, 0, 0, lamports_u64_le, space_u64_le, owner_pubkey(32 bytes)]`. Discriminator 0 = CreateAccount.

**System::initializeNonceAccount** instruction:
- accounts: [nonce_account (writable), RecentBlockhashes sysvar (read-only), Rent sysvar (read-only) `SysvarRent111111111111111111111111111111111`].
- data = `[6, 0, 0, 0, authority_pubkey(32 bytes)]`. Discriminator 6 = InitializeNonceAccount.

- [ ] **Step 1: Create SolTx.mo**

File: `ponzi_math_sol/SolTx.mo`

```motoko
import Array "mo:base/Array";
import Blob "mo:base/Blob";
import Buffer "mo:base/Buffer";
import Nat "mo:base/Nat";
import Nat8 "mo:base/Nat8";
import Nat64 "mo:base/Nat64";

import Base58 "Base58";

module {

    // ====================================================================
    // Well-known program / sysvar addresses (base58 strings).
    // ====================================================================

    public let SYSTEM_PROGRAM_ID : Text = "11111111111111111111111111111111";
    // NOTE the lowercase "l" — Solana's sysvar IDs are intentionally
    // typo-resistant base58 strings, NOT human-readable.
    public let SYSVAR_RECENT_BLOCKHASHES : Text = "SysvarRecentB1ockHashes11111111111111111111";
    public let SYSVAR_RENT : Text = "SysvarRent111111111111111111111111111111111";

    // 80 bytes for a nonce account body.
    public let NONCE_ACCOUNT_SPACE : Nat64 = 80;

    // ====================================================================
    // compact-u16 (Solana's variable-length length prefix)
    // ====================================================================

    /// Encode a Nat as compact-u16 bytes. Solana's compact-u16:
    ///   if n < 0x80 → 1 byte
    ///   if n < 0x4000 → 2 bytes (low 7 bits + 0x80; next 7 bits)
    ///   else → 3 bytes (low 7 + 0x80; next 7 + 0x80; high 2 bits)
    public func compactU16(n : Nat) : [Nat8] {
        if (n < 0x80) {
            [Nat8.fromNat(n)];
        } else if (n < 0x4000) {
            [
                Nat8.fromNat((n & 0x7F) | 0x80),
                Nat8.fromNat((n >> 7) & 0x7F),
            ];
        } else {
            [
                Nat8.fromNat((n & 0x7F) | 0x80),
                Nat8.fromNat(((n >> 7) & 0x7F) | 0x80),
                Nat8.fromNat((n >> 14) & 0x03),
            ];
        };
    };

    // ====================================================================
    // Little-endian Nat64 → 8 bytes.
    // ====================================================================

    public func u64Le(n : Nat64) : [Nat8] {
        let v = Nat64.toNat(n);
        [
            Nat8.fromNat(v & 0xFF),
            Nat8.fromNat((v >> 8) & 0xFF),
            Nat8.fromNat((v >> 16) & 0xFF),
            Nat8.fromNat((v >> 24) & 0xFF),
            Nat8.fromNat((v >> 32) & 0xFF),
            Nat8.fromNat((v >> 40) & 0xFF),
            Nat8.fromNat((v >> 48) & 0xFF),
            Nat8.fromNat((v >> 56) & 0xFF),
        ];
    };

    // ====================================================================
    // Instruction + Message types
    // ====================================================================

    /// A logical instruction, prior to compilation against an account-key
    /// table. `signer` and `writable` flags govern header bookkeeping.
    public type AccountMeta = {
        pubkey : Text;     // base58
        isSigner : Bool;
        isWritable : Bool;
    };

    public type Instruction = {
        programId : Text;  // base58
        accounts : [AccountMeta];
        data : [Nat8];
    };

    /// A compiled, serializable message — what gets signed.
    public type CompiledMessage = {
        header : (Nat8, Nat8, Nat8); // (numSigs, numReadonlySigned, numReadonlyUnsigned)
        accountKeys : [Text];        // canonical order: signer-writable, signer-readonly, unsigned-writable, unsigned-readonly
        recentBlockhash : Text;      // base58, 32 bytes
        instructions : [CompiledInstruction];
    };

    public type CompiledInstruction = {
        programIdIndex : Nat8;
        accounts : [Nat8];   // indices into CompiledMessage.accountKeys
        data : [Nat8];
    };

    // ====================================================================
    // Pre-built instruction constructors
    // ====================================================================

    /// System::transfer(from → to, lamports).
    public func transferIx(from : Text, to : Text, lamports : Nat64) : Instruction {
        let header : [Nat8] = [2, 0, 0, 0]; // Transfer discriminator (u32 LE)
        let data = Array.append<Nat8>(header, u64Le(lamports));
        {
            programId = SYSTEM_PROGRAM_ID;
            accounts = [
                { pubkey = from; isSigner = true; isWritable = true },
                { pubkey = to;   isSigner = false; isWritable = true },
            ];
            data;
        };
    };

    /// System::advance_nonce_account(nonceAccount, authority).
    public func advanceNonceIx(nonceAccount : Text, authority : Text) : Instruction {
        let data : [Nat8] = [4, 0, 0, 0]; // AdvanceNonceAccount discriminator
        {
            programId = SYSTEM_PROGRAM_ID;
            accounts = [
                { pubkey = nonceAccount;            isSigner = false; isWritable = true },
                { pubkey = SYSVAR_RECENT_BLOCKHASHES; isSigner = false; isWritable = false },
                { pubkey = authority;               isSigner = true; isWritable = false },
            ];
            data;
        };
    };

    /// System::createAccount(funder → new, lamports, space, owner).
    public func createAccountIx(
        funder : Text,
        newAccount : Text,
        lamports : Nat64,
        space : Nat64,
        owner : Text,
    ) : Instruction {
        // data = [0,0,0,0] + lamports_u64_le + space_u64_le + owner_pubkey(32 bytes)
        let header : [Nat8] = [0, 0, 0, 0];
        let lamportsBytes = u64Le(lamports);
        let spaceBytes = u64Le(space);
        let ownerBytes = switch (Base58.decode(owner)) {
            case (?b) { Blob.toArray(b) };
            case (null) { [] };
        };
        let body = Array.append<Nat8>(Array.append<Nat8>(lamportsBytes, spaceBytes), ownerBytes);
        let data = Array.append<Nat8>(header, body);
        {
            programId = SYSTEM_PROGRAM_ID;
            accounts = [
                { pubkey = funder;     isSigner = true; isWritable = true },
                { pubkey = newAccount; isSigner = true; isWritable = true },
            ];
            data;
        };
    };

    /// System::initialize_nonce_account(nonce, authority).
    public func initializeNonceIx(nonceAccount : Text, authority : Text) : Instruction {
        let header : [Nat8] = [6, 0, 0, 0]; // InitializeNonceAccount discriminator
        let authBytes = switch (Base58.decode(authority)) {
            case (?b) { Blob.toArray(b) };
            case (null) { [] };
        };
        let data = Array.append<Nat8>(header, authBytes);
        {
            programId = SYSTEM_PROGRAM_ID;
            accounts = [
                { pubkey = nonceAccount;             isSigner = false; isWritable = true },
                { pubkey = SYSVAR_RECENT_BLOCKHASHES; isSigner = false; isWritable = false },
                { pubkey = SYSVAR_RENT;              isSigner = false; isWritable = false },
            ];
            data;
        };
    };

    // ====================================================================
    // Compilation — collect distinct accounts, order them, build header,
    // emit CompiledMessage.
    // ====================================================================

    /// Compile a list of logical instructions into a CompiledMessage.
    /// `feePayer` MUST appear as a signer in at least one instruction.
    /// Account ordering follows Solana's canonical rule:
    ///   1. signer + writable (feePayer first)
    ///   2. signer + readonly
    ///   3. non-signer + writable
    ///   4. non-signer + readonly
    public func compile(
        feePayer : Text,
        recentBlockhash : Text,
        ixs : [Instruction],
    ) : CompiledMessage {
        // Build a deduped account list, tracking signer/writable flags.
        // The dedup logic: for any pubkey appearing in multiple metas,
        // take the OR of signer flags and the OR of writable flags.
        let pubkeyBuf = Buffer.Buffer<Text>(16);
        let signerBuf = Buffer.Buffer<Bool>(16);
        let writableBuf = Buffer.Buffer<Bool>(16);

        // Seed with feePayer as signer+writable (always).
        pubkeyBuf.add(feePayer);
        signerBuf.add(true);
        writableBuf.add(true);

        // Walk every instruction, plus the programId (programs are always
        // readonly non-signers — but they MUST appear in accountKeys).
        for (ix in ixs.vals()) {
            // Add the program id if not already present.
            ensureAccount(pubkeyBuf, signerBuf, writableBuf, ix.programId, false, false);
            for (m in ix.accounts.vals()) {
                ensureAccount(pubkeyBuf, signerBuf, writableBuf, m.pubkey, m.isSigner, m.isWritable);
            };
        };

        // Sort into canonical order: signer-writable, signer-readonly,
        // non-signer-writable, non-signer-readonly. feePayer stays first.
        let keys = Buffer.toArray(pubkeyBuf);
        let isSigner = Buffer.toArray(signerBuf);
        let isWritable = Buffer.toArray(writableBuf);

        let n = keys.size();
        let categories = Array.init<Nat>(n, 0);
        var i : Nat = 0;
        while (i < n) {
            categories[i] := categorize(isSigner[i], isWritable[i]);
            i += 1;
        };

        // Stable sort with feePayer (index 0) always first.
        let permuted = stableSortIndices(n, func(a : Nat, b : Nat) : { #less; #equal; #greater } {
            if (a == 0) { #less }      // feePayer first
            else if (b == 0) { #greater }
            else if (categories[a] < categories[b]) { #less }
            else if (categories[a] > categories[b]) { #greater }
            else { #equal };
        });

        let sortedKeys = Array.tabulate<Text>(n, func(idx) { keys[permuted[idx]] });
        let sortedSigner = Array.tabulate<Bool>(n, func(idx) { isSigner[permuted[idx]] });
        let sortedWritable = Array.tabulate<Bool>(n, func(idx) { isWritable[permuted[idx]] });

        // Count header buckets.
        var numSigs : Nat8 = 0;
        var numROSigned : Nat8 = 0;
        var numROUnsigned : Nat8 = 0;
        i := 0;
        while (i < n) {
            if (sortedSigner[i]) {
                numSigs += 1;
                if (not sortedWritable[i]) { numROSigned += 1 };
            } else if (not sortedWritable[i]) {
                numROUnsigned += 1;
            };
            i += 1;
        };

        // Compile each instruction against the sorted account table.
        let compiledIxs = Array.tabulate<CompiledInstruction>(ixs.size(), func(ixIdx) {
            let ix = ixs[ixIdx];
            let pIdx = indexOf(sortedKeys, ix.programId);
            let accountIndices = Array.tabulate<Nat8>(ix.accounts.size(), func(j) {
                Nat8.fromNat(indexOf(sortedKeys, ix.accounts[j].pubkey));
            });
            { programIdIndex = Nat8.fromNat(pIdx); accounts = accountIndices; data = ix.data };
        });

        {
            header = (numSigs, numROSigned, numROUnsigned);
            accountKeys = sortedKeys;
            recentBlockhash;
            instructions = compiledIxs;
        };
    };

    private func ensureAccount(
        keys : Buffer.Buffer<Text>,
        signers : Buffer.Buffer<Bool>,
        writables : Buffer.Buffer<Bool>,
        pubkey : Text,
        signer : Bool,
        writable : Bool,
    ) {
        let n = keys.size();
        var i : Nat = 0;
        while (i < n) {
            if (keys.get(i) == pubkey) {
                if (signer) { signers.put(i, true) };
                if (writable) { writables.put(i, true) };
                return;
            };
            i += 1;
        };
        keys.add(pubkey);
        signers.add(signer);
        writables.add(writable);
    };

    private func categorize(signer : Bool, writable : Bool) : Nat {
        if (signer and writable) { 0 }
        else if (signer) { 1 }
        else if (writable) { 2 }
        else { 3 };
    };

    private func indexOf(arr : [Text], target : Text) : Nat {
        var i : Nat = 0;
        while (i < arr.size()) {
            if (arr[i] == target) { return i };
            i += 1;
        };
        // Should never be reached for a well-compiled message.
        0;
    };

    // Stable sort permutation: returns indices [0..n) reordered so that
    // for i<j, cmp(indices[i], indices[j]) ≠ #greater. O(n²) is fine —
    // a Solana message has at most ~10 accounts.
    private func stableSortIndices(n : Nat, cmp : (Nat, Nat) -> { #less; #equal; #greater }) : [Nat] {
        let arr = Array.init<Nat>(n, 0);
        var i : Nat = 0;
        while (i < n) { arr[i] := i; i += 1; };
        // Insertion sort (stable).
        var k : Nat = 1;
        while (k < n) {
            let cur = arr[k];
            var j : Nat = k;
            label inner while (j > 0) {
                if (cmp(arr[j - 1], cur) == #greater) {
                    arr[j] := arr[j - 1];
                    j -= 1;
                } else { break inner };
            };
            arr[j] := cur;
            k += 1;
        };
        Array.freeze(arr);
    };

    // ====================================================================
    // Serialization — CompiledMessage → bytes (this is what gets signed).
    // ====================================================================

    public func serializeMessage(msg : CompiledMessage) : Blob {
        let buf = Buffer.Buffer<Nat8>(256);

        // Header.
        let (h1, h2, h3) = msg.header;
        buf.add(h1);
        buf.add(h2);
        buf.add(h3);

        // Account keys.
        for (b in compactU16(msg.accountKeys.size()).vals()) { buf.add(b) };
        for (key in msg.accountKeys.vals()) {
            switch (Base58.decode(key)) {
                case (?blob) {
                    for (b in Blob.toArray(blob).vals()) { buf.add(b) };
                };
                case (null) {
                    // Invalid base58 — emit 32 zero bytes so the message
                    // length is still right and the caller's signing step
                    // fails loudly rather than silently.
                    var z : Nat = 0;
                    while (z < 32) { buf.add(0); z += 1 };
                };
            };
        };

        // Recent blockhash (32 bytes).
        switch (Base58.decode(msg.recentBlockhash)) {
            case (?blob) {
                for (b in Blob.toArray(blob).vals()) { buf.add(b) };
            };
            case (null) {
                var z : Nat = 0;
                while (z < 32) { buf.add(0); z += 1 };
            };
        };

        // Instructions.
        for (b in compactU16(msg.instructions.size()).vals()) { buf.add(b) };
        for (ix in msg.instructions.vals()) {
            buf.add(ix.programIdIndex);
            for (b in compactU16(ix.accounts.size()).vals()) { buf.add(b) };
            for (a in ix.accounts.vals()) { buf.add(a) };
            for (b in compactU16(ix.data.size()).vals()) { buf.add(b) };
            for (b in ix.data.vals()) { buf.add(b) };
        };

        Blob.fromArray(Buffer.toArray(buf));
    };

    /// Combine a serialized message with a list of signatures (one per
    /// required signer, in the same order as accountKeys' signer prefix).
    /// Returns the full wire-format transaction bytes for sendTransaction.
    public func assembleTransaction(messageBytes : Blob, signatures : [Blob]) : Blob {
        let buf = Buffer.Buffer<Nat8>(messageBytes.size() + signatures.size() * 64 + 8);
        for (b in compactU16(signatures.size()).vals()) { buf.add(b) };
        for (sig in signatures.vals()) {
            for (b in Blob.toArray(sig).vals()) { buf.add(b) };
        };
        for (b in Blob.toArray(messageBytes).vals()) { buf.add(b) };
        Blob.fromArray(Buffer.toArray(buf));
    };
};
```

- [ ] **Step 2: Verify it parses**

Run: `dfx build ponzi_math_sol --network=local 2>&1 | tail -30`
Expected: no errors. If Motoko complains about `Array.init` mutation (e.g. "expected non-mut got mut") fix the specific line — Motoko occasionally requires explicit `Array.thaw`/`Array.freeze` pairs.

- [ ] **Step 3: Commit**

```bash
git add ponzi_math_sol/SolTx.mo
git commit -m "feat(ponzi_math_sol): add SolTx byte-construction module

Pure-Motoko builder for Solana transaction message bytes:
- compact-u16 length prefix encoding (1-3 bytes)
- little-endian u64 lamport amounts
- SystemProgram transfer, createAccount, initializeNonceAccount,
  advance_nonce_account instruction constructors
- Compilation (instructions + feePayer + recentBlockhash) into the
  canonical account-key ordering Solana expects
- Serialization to the byte-blob that sign_with_schnorr signs
- Assembly of message + signatures into a wire-format transaction

Self-test query lives on the actor (Task 8) and runs in Task 21."
```

---

## Task 7: Implement `SolSigner.mo` (management canister wrapper)

**Files:**
- Create: `ponzi_math_sol/SolSigner.mo`

This module wraps the IC management canister's `schnorr_public_key` and `sign_with_schnorr` methods. We use **Ed25519** as the Schnorr variant.

**Key naming:**
- Local dev: `"dfx_test_key"`. Works with `dfx start` if you've enabled the test key in your local replica config. For M1 we don't actually sign locally — Tasks 21–24 do all signing on mainnet — so the local key name is effectively unused.
- Mainnet: `"key_1"`. This is the production threshold key.

**Cycle attachment:** Each `sign_with_schnorr` call on mainnet costs ~26.15 B cycles (subject to change; check the [IC docs](https://internetcomputer.org/docs/current/references/t-sigs-how-it-works/) for current pricing). For M1 we hard-code the attach amount on a per-call basis using `(with cycles = ...)` syntax. Be conservative — if the call traps for cycle shortage, the canister state mutations are rolled back but the user-experience is bad.

- [ ] **Step 1: Create SolSigner.mo**

File: `ponzi_math_sol/SolSigner.mo`

```motoko
import Blob "mo:base/Blob";
import Buffer "mo:base/Buffer";
import Cycles "mo:base/ExperimentalCycles";
import Principal "mo:base/Principal";

import Base58 "Base58";

module {

    // ====================================================================
    // Management-canister actor — Schnorr (Ed25519) signing.
    // ====================================================================

    public type SchnorrAlgorithm = { #ed25519; #bip340secp256k1 };

    public type KeyId = {
        algorithm : SchnorrAlgorithm;
        name : Text;
    };

    public type PublicKeyArgs = {
        canister_id : ?Principal;
        derivation_path : [Blob];
        key_id : KeyId;
    };

    public type PublicKeyResult = {
        public_key : Blob;
        chain_code : Blob;
    };

    public type SignArgs = {
        message : Blob;
        derivation_path : [Blob];
        key_id : KeyId;
    };

    public type SignResult = {
        signature : Blob;
    };

    transient let ic : actor {
        schnorr_public_key : shared PublicKeyArgs -> async PublicKeyResult;
        sign_with_schnorr : shared SignArgs -> async SignResult;
    } = actor "aaaaa-aa";

    // ====================================================================
    // Well-known key ids.
    // Mainnet uses "key_1" (production threshold key). Local dev uses
    // "dfx_test_key" — only works against a local replica configured with
    // the test key, and we don't actually exercise local signing in M1.
    // ====================================================================

    public let KEY_ID_MAINNET : KeyId = { algorithm = #ed25519; name = "key_1" };
    public let KEY_ID_LOCAL : KeyId = { algorithm = #ed25519; name = "dfx_test_key" };

    /// Approximate cycle cost of one sign_with_schnorr call on mainnet
    /// (Q1 2026 pricing ≈ 26 G cycles). 30 G is a defensive buffer.
    public let SIGN_CYCLES : Nat = 30_000_000_000;

    // ====================================================================
    // Public helpers — pubkey derivation and signing.
    // ====================================================================

    /// Derive a Solana address by base58-encoding the threshold Ed25519
    /// pubkey for the given derivation path. canister_id = null means
    /// "use the calling canister's id" (this canister).
    public func deriveAddress(keyId : KeyId, derivationPath : [Blob]) : async Text {
        let res = await ic.schnorr_public_key({
            canister_id = null;
            derivation_path = derivationPath;
            key_id = keyId;
        });
        // Ed25519 pubkey is 32 bytes; Solana addresses are exactly those
        // 32 bytes base58-encoded.
        Base58.encode(res.public_key);
    };

    /// Sign a raw message blob with the threshold key at the given
    /// derivation path. Returns the 64-byte signature.
    public func sign(keyId : KeyId, derivationPath : [Blob], message : Blob) : async Blob {
        Cycles.add<system>(SIGN_CYCLES);
        let res = await ic.sign_with_schnorr({
            message;
            derivation_path = derivationPath;
            key_id = keyId;
        });
        res.signature;
    };

    /// Convenience: sign one message with multiple derivation paths
    /// (e.g., pool + nonce-account for bootstrap). Returns signatures in
    /// the same order as `derivationPaths`.
    public func signMulti(keyId : KeyId, derivationPaths : [[Blob]], message : Blob) : async [Blob] {
        let sigs = Buffer.Buffer<Blob>(derivationPaths.size());
        for (path in derivationPaths.vals()) {
            let sig = await sign(keyId, path, message);
            sigs.add(sig);
        };
        Buffer.toArray(sigs);
    };
};
```

- [ ] **Step 2: Verify it parses**

Run: `dfx build ponzi_math_sol --network=local 2>&1 | tail -20`
Expected: clean build. `unused-import` warnings on `Array` or `Principal` are fine if any appear; ignore them.

- [ ] **Step 3: Commit**

```bash
git add ponzi_math_sol/SolSigner.mo
git commit -m "feat(ponzi_math_sol): add SolSigner threshold-Ed25519 wrapper

Wraps the IC management canister's schnorr_public_key /
sign_with_schnorr (algorithm=ed25519) for pubkey derivation and
message signing. Cycles are attached on each sign call; mainnet key
'key_1' is the production identity. deriveAddress base58-encodes the
returned 32-byte Ed25519 pubkey as a Solana address."
```

---

## Task 8: Fork `ponzi_math/main.mo` into `ponzi_math_sol/main.mo` (mechanical copy + ICP-call removal)

**Files:**
- Replace: `ponzi_math_sol/main.mo` (from Task 2's placeholder)

This is the largest single task. We perform a **literal copy** of `ponzi_math/main.mo` into `ponzi_math_sol/main.mo`, then:

1. Rename the actor class.
2. Replace ICP-ledger imports with the new SolRpc/SolSigner/SolTx/Base58 imports.
3. Delete `createGame`, `addBackerMoney`, and `sweepCoverCharges` (they'll be replaced in later tasks).
4. Stub out the inside of `withdrawEarnings`, `settleCompoundingGame`, `claimBackerRepayment` so the file still compiles — the actual SOL-payout body lands in Tasks 15–18.
5. Add the new state fields needed for chain fusion.
6. Add stub public methods (`getPoolAddress`, `bootstrap`, `prepareSolDeposit`, etc.) that all `Debug.trap` for now and get implemented in later tasks.
7. Keep ALL of: `calculateExitToll`, `calculateEarnings`, `calculateCompounded{15,30}DayEarnings`, `distributeExitToll`, `creditBackerRepayment`, `selectPromotionCandidate`, `applySeriesBPromotion`, `promoteAndReset`, `triggerGameReset`, `computeActivePlanSnapshot`, the general ledger, the locks, the admin allowlist, every existing query.

- [ ] **Step 1: Mechanical copy**

Run:
```bash
cp ponzi_math/main.mo ponzi_math_sol/main.mo
```

This overwrites the Task 2 placeholder. We now have an exact byte-for-byte clone in the new directory.

- [ ] **Step 2: Verify the copy**

Run: `diff ponzi_math/main.mo ponzi_math_sol/main.mo | head -5`
Expected: empty (files identical).

- [ ] **Step 3: Apply diff #1 — rename actor and swap imports**

Open `ponzi_math_sol/main.mo` and make these targeted edits:

Replace:
```motoko
import Ledger "ledger";
import Icrc21 "icrc21";
```
with:
```motoko
import Icrc21 "icrc21";

import Base58 "Base58";
import SolRpc "SolRpc";
import SolSigner "SolSigner";
import SolTx "SolTx";
```

Replace:
```motoko
persistent actor class PonziMath(initArgs : {
    backendPrincipal : Principal;
    testAdmin : Principal;
}) = Self {
    transient let BACKEND_PRINCIPAL : Principal = initArgs.backendPrincipal;
    transient let TEST_ADMIN : Principal = initArgs.testAdmin;
    transient let icpLedger : Ledger.LedgerActor = actor(Ledger.ICP_LEDGER_CANISTER_ID);
    transient let ic : actor { raw_rand : () -> async Blob } = actor "aaaaa-aa";
```
with:
```motoko
persistent actor class PonziMathSol(initArgs : {
    backendPrincipal : Principal;
    testAdmin : Principal;
    solTreasuryAddress : Text;
    solRpcProvider : SolRpc.Provider;
    keyId : SolSigner.KeyId;
}) = Self {
    transient let BACKEND_PRINCIPAL : Principal = initArgs.backendPrincipal;
    transient let TEST_ADMIN : Principal = initArgs.testAdmin;
    transient let solRpc : SolRpc.RpcActor = actor(SolRpc.SOL_RPC_CANISTER_ID);
    transient let ic : actor { raw_rand : () -> async Blob } = actor "aaaaa-aa";

    // SOL-side config — captured at init, then mirrored to mutable state
    // below so admin can update it post-deploy.
    transient let _INIT_TREASURY : Text = initArgs.solTreasuryAddress;
    transient let _INIT_RPC_PROVIDER : SolRpc.Provider = initArgs.solRpcProvider;
    transient let _INIT_KEY_ID : SolSigner.KeyId = initArgs.keyId;
```

- [ ] **Step 4: Apply diff #2 — add new persistent state for chain fusion**

Find the state block (around line 197-225 of the original; should be around the same place in the copy). After the line:
```motoko
    var currentRoundId : Nat = 1;
```
INSERT (before the transient block):

```motoko
    // ============== Chain fusion / SOL state ==============

    // Admin-tunable Solana RPC + signing config.
    var solRpcProvider : SolRpc.Provider = initArgs.solRpcProvider;
    var keyId : SolSigner.KeyId = initArgs.keyId;
    var solTreasuryAddress : Text = initArgs.solTreasuryAddress;

    // Pool address — singleton, derivation path ["pool"]. Holds all pot lamports.
    var poolAddress : ?Text = null;

    // Nonce account — singleton, derivation path ["nonce"]. Durable nonce
    // for outbound txs.
    var nonceAccountAddress : ?Text = null;
    var lastNonceValue : ?Text = null;
    var bootstrapped : Bool = false;

    // Per-user deposit addresses (caller principal → base58 pubkey).
    // `principalMapNat` already exists in the original state block — it's
    // a principal-keyed OrderedMap whose value type is per-empty<>(). We
    // reuse it for `depositAddresses`. The new `textMap` alias serves
    // address-keyed maps (reverse lookup + signature cursor).
    transient let textMap = OrderedMap.Make<Text>(Text.compare);
    var depositAddresses = principalMapNat.empty<Text>();
    var addressToPrincipal = textMap.empty<Principal>();

    // Deposit-detection cursors per address.
    var lastSeenSignature = textMap.empty<Text>();

    // Deposit intents — caller commits to a plan + amount before sending SOL.
    public type DepositIntent = {
        id : Nat;
        principal : Principal;
        plan : GamePlan;
        expectedAmountLamports : Nat64;
        createdAt : Int;
        expiresAt : Int;
        fulfilled : Bool;
    };
    var pendingIntents = natMap.empty<DepositIntent>();
    var nextIntentId : Nat = 0;

    // Cover charge accrual in lamports. Lives on the pool address until
    // payManagementSol sweeps it.
    var coverChargeAccrualLamports : Nat64 = 0;

    // Min deposit gate — 0.05 SOL (50_000_000 lamports). Mirrors
    // ponzi_math's 0.1 ICP gate at deploy-time prices.
    transient let MIN_DEPOSIT_LAMPORTS : Nat64 = 50_000_000;

    // Intent TTL — 10 minutes.
    transient let INTENT_TTL_NS : Int = 10 * 60 * 1_000_000_000;
```

> **Note:** if `OrderedMap.Make<Text>(Text.compare)` is already declared at the top of the original state block as `textMap`, reuse that and skip the duplicate. The original file declared `natMap`, `principalMapNat`, `intMap`, `backerKeyMap` — none of those are `Text`-keyed, so we DO need a new `textMap` alias. Pick whichever name is unused. Verify by grepping the file for existing `OrderedMap.Make` calls before adding the new aliases.

- [ ] **Step 5: Apply diff #3 — delete `createGame`, `addBackerMoney`, `sweepCoverCharges`**

These three functions are entirely replaced. Delete them from `ponzi_math_sol/main.mo`:
- `createGame` (lines ~729-849 in the original) — replaced by `prepareSolDeposit` + the detection timer (Tasks 11, 12-14).
- `addBackerMoney` (lines ~855-925) — replaced by admin hatch `adminRegisterSeriesABacker` (Task 20).
- `sweepCoverCharges` (lines ~1255-1304) — replaced by `payManagementSol` (Task 19).

Also delete the import-only line:
```motoko
    transient let COVER_CHARGE_RATE : Float = 0.04;
```
…which lives just above `createGame`. We keep cover-charge math but it moves into the detection callback. Re-declare it in Task 13.

After deletion, the file will be smaller but still compile because nothing else references those names — `createGame` was a public method, so the IDL is implicitly smaller too. Frontend callers that used to reach for `createGame` will instead use `prepareSolDeposit` (added Task 11).

- [ ] **Step 6: Apply diff #4 — stub `withdrawEarnings`, `settleCompoundingGame`, `claimBackerRepayment` bodies**

For these three functions, the math + lock + ledger bookkeeping stays the same. Only the ICP-ledger payout call changes. For now (Task 8) we want the file to compile, so we replace the `await icpLedger.icrc1_transfer({...})` block in each one with:

```motoko
                        // ICP-side payout was here. SOL-side payout lands in
                        // Task 15-17; for now we trap so the function is wired
                        // but unusable until then.
                        Debug.trap("withdrawEarnings: SOL payout not yet wired");
```

Apply this in all three functions at the spot where `icrc1_transfer` was called. The math and state mutations BEFORE that line stay; the "set state, then call ledger, then on err revert state" pattern means the trap is safe (state rollback happens automatically).

Also delete every reference to `Ledger.ICP_TRANSFER_FEE` and `Ledger.TransferError` / `Ledger.TransferFromError` — those types no longer exist for us. The functions `transferFromErrorMessage` and `transferErrorMessage` lose their callers, so delete them too.

- [ ] **Step 7: Apply diff #5 — replace `getCanisterICPBalance` with `getCanisterSolBalance`**

Find:
```motoko
    public shared ({ caller }) func getCanisterICPBalance() : async Nat {
        requireAdmin(caller);
        let selfPrincipal = Principal.fromActor(Self);
        try {
            await icpLedger.icrc1_balance_of({ owner = selfPrincipal; subaccount = null });
        } catch (_) { 0 };
    };
```
Replace with:
```motoko
    public shared ({ caller }) func getCanisterSolBalance() : async Nat64 {
        requireAdmin(caller);
        switch (poolAddress) {
            case (null) { 0 };
            case (?addr) {
                let res = await solRpc.getBalance(addr, ?{ provider = ?solRpcProvider });
                switch (res) {
                    case (#Ok(lamports)) { lamports };
                    case (#Err(_)) { 0 };
                };
            };
        };
    };
```

- [ ] **Step 8: Apply diff #6 — stub public methods for later tasks**

Add these stubbed public methods anywhere convenient (the bottom of the actor body, before the closing brace, is fine). They get bodies in the named tasks:

```motoko
    // ====================================================================
    // STUBS — bodies land in later tasks. Each function traps for now so
    // the IDL is present and the canister upgrades smoothly later.
    // ====================================================================

    public shared ({ caller }) func bootstrap() : async { #Ok : Text; #Err : Text } {
        requireAdmin(caller);
        Debug.trap("bootstrap(): not yet implemented (Task 12)");
    };

    public query func getPoolAddress() : async ?Text { poolAddress };
    public query func getNonceAccountAddress() : async ?Text { nonceAccountAddress };
    public query func isBootstrapped() : async Bool { bootstrapped };

    public shared ({ caller }) func getOrCreateDepositAddress() : async { #Ok : Text; #Err : Text } {
        requireAuthenticated(caller);
        Debug.trap("getOrCreateDepositAddress: not yet implemented (Task 10)");
    };

    public shared ({ caller }) func prepareSolDeposit(args : { plan : GamePlan; expectedAmountLamports : Nat64 }) : async { #Ok : { intentId : Nat; depositAddress : Text }; #Err : Text } {
        requireAuthenticated(caller);
        let _ = args;
        Debug.trap("prepareSolDeposit: not yet implemented (Task 11)");
    };

    public shared ({ caller }) func runDepositDetection() : async { #Ok : Nat; #Err : Text } {
        requireAdmin(caller);
        Debug.trap("runDepositDetection: not yet implemented (Task 12-14)");
    };

    public shared ({ caller }) func payManagementSol() : async { #Ok : Text; #Err : Text } {
        let _ = caller;
        Debug.trap("payManagementSol: not yet implemented (Task 19)");
    };

    public shared ({ caller }) func adminRegisterSeriesABacker(owner : Principal, amount : Float) : async { #Ok; #Err : Text } {
        let _ = caller;
        let _ = owner;
        let _ = amount;
        Debug.trap("adminRegisterSeriesABacker: not yet implemented (Task 20)");
    };

    // ====================================================================
    // Self-test queries (used by Task 21 to smoke-test pure modules
    // without needing devnet round-trips).
    // ====================================================================

    public query func selfTestBase58() : async Bool {
        // 32 zero bytes → "11111111111111111111111111111111" (System Program ID).
        let zeros = Blob.fromArray(Array.tabulate<Nat8>(32, func(_) { 0 }));
        let encoded = Base58.encode(zeros);
        encoded == SolTx.SYSTEM_PROGRAM_ID;
    };

    public query func selfTestSolTx() : async {
        compactU16_42 : [Nat8];
        compactU16_128 : [Nat8];
        compactU16_300 : [Nat8];
        u64Le_1 : [Nat8];
        u64Le_1B : [Nat8];
    } {
        {
            compactU16_42 = SolTx.compactU16(42);          // expect [42]
            compactU16_128 = SolTx.compactU16(128);        // expect [0x80, 0x01]
            compactU16_300 = SolTx.compactU16(300);        // expect [0xAC, 0x02]
            u64Le_1 = SolTx.u64Le(1);                       // expect [1, 0, 0, 0, 0, 0, 0, 0]
            u64Le_1B = SolTx.u64Le(1_000_000_000);          // expect [0,0xCA,0x9A,0x3B,0,0,0,0]
        };
    };
```

- [ ] **Step 9: Build and resolve any remaining errors**

Run: `dfx build ponzi_math_sol --network=local 2>&1 | tail -50`
Expected: clean build. Common errors after the fork:
- `Ledger` still imported somewhere → grep and delete.
- `transferErrorMessage` / `transferFromErrorMessage` still referenced → those references vanished with the function bodies, but if anything in the test-admin block (~line 1700+) still references them, replace with `Debug.trap("ICP error path no longer reachable")` inside the relevant catch arm and surface in Task 17/20.
- `import Result "mo:base/Result"` in `ledger.mo` is no longer imported by main; that's fine, no action needed.
- `Array` not imported but `Array.tabulate` used in selfTestBase58 → add `import Array "mo:base/Array";` at the top.

Fix each error inline; don't paper over. The file should compile before committing.

- [ ] **Step 10: Commit**

```bash
git add ponzi_math_sol/main.mo
git commit -m "feat(ponzi_math_sol): fork from ponzi_math, strip ICP-ledger paths

Mechanical copy of ponzi_math/main.mo with:
- Actor class renamed PonziMath → PonziMathSol
- Ledger.mo import replaced by Base58/SolRpc/SolSigner/SolTx imports
- ICP ledger actor field replaced by solRpc + sign-with-schnorr config
- createGame, addBackerMoney, sweepCoverCharges deleted (replaced
  later by intent-based deposit, admin backer registration, and
  payManagementSol respectively)
- withdrawEarnings, settleCompoundingGame, claimBackerRepayment
  payout bodies stubbed with Debug.trap (Tasks 15-18 wire them up)
- getCanisterICPBalance → getCanisterSolBalance via sol-rpc
- New state for pool/nonce addresses, deposit addresses, intents,
  cover-charge accrual in lamports
- Stub public methods + self-test queries for the upcoming wiring

Math, locks, ledger, admin guards, backer system, round-reset all
copied verbatim per the spec's 'literal fork' mandate."
```

---

## Task 9: Implement pool-address derivation and surface it via query

**Files:**
- Modify: `ponzi_math_sol/main.mo`

The pool address is the canister's primary Solana identity. Derivation path = `["pool"]`. We derive on first call and cache.

- [ ] **Step 1: Add an internal helper that derives + caches the pool address**

In `ponzi_math_sol/main.mo`, locate the block of internal helpers (near `creditBackerRepayment` or similar). Add:

```motoko
    func derivationPathPool() : [Blob] {
        [Text.encodeUtf8("pool")];
    };

    func derivationPathNonce() : [Blob] {
        [Text.encodeUtf8("nonce")];
    };

    func derivationPathForPrincipal(p : Principal) : [Blob] {
        [Principal.toBlob(p)];
    };

    func ensurePoolAddress() : async Text {
        switch (poolAddress) {
            case (?addr) { addr };
            case (null) {
                let addr = await SolSigner.deriveAddress(keyId, derivationPathPool());
                poolAddress := ?addr;
                addr;
            };
        };
    };
```

- [ ] **Step 2: Add a public admin-callable derivePoolAddress() that forces derivation**

After the stub `getPoolAddress` query (Task 8 Step 8), add:

```motoko
    /// Admin-callable: derive the pool address via threshold-Schnorr and
    /// cache it. Idempotent — subsequent calls just return the cached
    /// value. Must be called once before bootstrap() so the operator can
    /// fund the pool.
    public shared ({ caller }) func adminDerivePoolAddress() : async { #Ok : Text; #Err : Text } {
        requireAdmin(caller);
        try {
            let addr = await ensurePoolAddress();
            #Ok(addr);
        } catch (e) {
            #Err("Failed to derive pool address: " # Error.message(e));
        };
    };
```

- [ ] **Step 3: Verify compile**

Run: `dfx build ponzi_math_sol --network=local 2>&1 | tail -20`
Expected: clean build.

- [ ] **Step 4: Commit**

```bash
git add ponzi_math_sol/main.mo
git commit -m "feat(ponzi_math_sol): derive and surface pool address

adminDerivePoolAddress() pulls the canister's primary Solana identity
from threshold Ed25519 (derivation path ['pool']) and caches it.
getPoolAddress() query returns the cached value.

Operator flow: deploy → adminDerivePoolAddress() → fund the printed
address with ~0.05 SOL on devnet → bootstrap()."
```

---

## Task 10: Implement per-user deposit address derivation

**Files:**
- Modify: `ponzi_math_sol/main.mo`

When a user first calls into `ponzi_math_sol`, derive a per-user Solana address keyed on their IC principal. Store both directions of the mapping (`principal → address` and `address → principal`) so the detection timer can look up the owner of an incoming SOL transfer.

- [ ] **Step 1: Replace the Task 8 stub of `getOrCreateDepositAddress`**

In `ponzi_math_sol/main.mo`, find:

```motoko
    public shared ({ caller }) func getOrCreateDepositAddress() : async { #Ok : Text; #Err : Text } {
        requireAuthenticated(caller);
        Debug.trap("getOrCreateDepositAddress: not yet implemented (Task 10)");
    };
```

Replace with:

```motoko
    public shared ({ caller }) func getOrCreateDepositAddress() : async { #Ok : Text; #Err : Text } {
        requireAuthenticated(caller);
        switch (principalMapNat.get(depositAddresses, caller)) {
            case (?addr) { #Ok(addr) };
            case (null) {
                acquireCallerLock(caller);
                try {
                    let addr = await SolSigner.deriveAddress(keyId, derivationPathForPrincipal(caller));
                    depositAddresses := principalMapNat.put(depositAddresses, caller, addr);
                    addressToPrincipal := textMap.put(addressToPrincipal, addr, caller);
                    #Ok(addr);
                } catch (e) {
                    #Err("Failed to derive deposit address: " # Error.message(e));
                } finally {
                    releaseCallerLock(caller);
                };
            };
        };
    };
```

- [ ] **Step 2: Add a public read of the caller's deposit address (no derivation)**

After `getOrCreateDepositAddress`, add:

```motoko
    public query ({ caller }) func getMyDepositAddress() : async ?Text {
        principalMapNat.get(depositAddresses, caller);
    };

    public query func getDepositAddressFor(p : Principal) : async ?Text {
        principalMapNat.get(depositAddresses, p);
    };
```

- [ ] **Step 3: Verify compile**

Run: `dfx build ponzi_math_sol --network=local 2>&1 | tail -20`
Expected: clean build.

- [ ] **Step 4: Commit**

```bash
git add ponzi_math_sol/main.mo
git commit -m "feat(ponzi_math_sol): derive per-user deposit address

getOrCreateDepositAddress() derives via threshold Ed25519 (derivation
path = [caller_principal_bytes]) on first call and caches in
depositAddresses + addressToPrincipal (for reverse lookup by the
detection timer).

Caller-lock-guarded to prevent two concurrent first-time calls from
double-deriving. Subsequent reads use the cached value via
getMyDepositAddress."
```

---

## Task 11: Implement `prepareSolDeposit` (intent recording)

**Files:**
- Modify: `ponzi_math_sol/main.mo`

The user commits to a plan + amount before sending SOL. The intent has a 10-minute TTL; if not fulfilled in time and the SOL still arrives, it's an admin-review case.

- [ ] **Step 1: Replace the Task 8 stub of `prepareSolDeposit`**

Find:

```motoko
    public shared ({ caller }) func prepareSolDeposit(args : { plan : GamePlan; expectedAmountLamports : Nat64 }) : async { #Ok : { intentId : Nat; depositAddress : Text }; #Err : Text } {
        requireAuthenticated(caller);
        let _ = args;
        Debug.trap("prepareSolDeposit: not yet implemented (Task 11)");
    };
```

Replace with:

```motoko
    public shared ({ caller }) func prepareSolDeposit(args : {
        plan : GamePlan;
        expectedAmountLamports : Nat64;
    }) : async { #Ok : { intentId : Nat; depositAddress : Text }; #Err : Text } {
        requireAuthenticated(caller);
        if (args.expectedAmountLamports < MIN_DEPOSIT_LAMPORTS) {
            return #Err("Minimum deposit is 0.05 SOL (50,000,000 lamports)");
        };
        if (not bootstrapped) {
            return #Err("Canister not bootstrapped yet — operator must run bootstrap() first");
        };

        acquireCallerLock(caller);
        try {
            // Per-user rate limit, identical to ponzi_math's 3-deposits-per-hour
            // gate. Sourced from the existing depositTimestamps map.
            let currentTime = Time.now();
            let oneHourAgo = currentTime - 3_600_000_000_000;
            switch (principalMapNat.get(depositTimestamps, caller)) {
                case (null) {};
                case (?timestamps) {
                    let filtered = List.filter<Int>(
                        timestamps,
                        func(t) { t > oneHourAgo },
                    );
                    if (List.size(filtered) >= 3) {
                        return #Err("You can only open 3 positions per hour");
                    };
                };
            };

            // Ensure the user has a deposit address.
            let depositAddr = switch (principalMapNat.get(depositAddresses, caller)) {
                case (?a) { a };
                case (null) {
                    // Derive inline. Same logic as getOrCreateDepositAddress
                    // but without recursive lock acquisition.
                    let addr = await SolSigner.deriveAddress(keyId, derivationPathForPrincipal(caller));
                    depositAddresses := principalMapNat.put(depositAddresses, caller, addr);
                    addressToPrincipal := textMap.put(addressToPrincipal, addr, caller);
                    addr;
                };
            };

            let intent : DepositIntent = {
                id = nextIntentId;
                principal = caller;
                plan = args.plan;
                expectedAmountLamports = args.expectedAmountLamports;
                createdAt = currentTime;
                expiresAt = currentTime + INTENT_TTL_NS;
                fulfilled = false;
            };
            pendingIntents := natMap.put(pendingIntents, nextIntentId, intent);
            let intentId = nextIntentId;
            nextIntentId += 1;

            #Ok({ intentId; depositAddress = depositAddr });
        } finally {
            releaseCallerLock(caller);
        };
    };
```

- [ ] **Step 2: Add a query for caller's pending intents**

After `prepareSolDeposit`, add:

```motoko
    public query ({ caller }) func getMyPendingIntents() : async [DepositIntent] {
        var out = List.nil<DepositIntent>();
        for (intent in natMap.vals(pendingIntents)) {
            if (intent.principal == caller and not intent.fulfilled) {
                out := List.push(intent, out);
            };
        };
        List.toArray(out);
    };

    public query ({ caller }) func adminGetAllIntents() : async [DepositIntent] {
        requireAdmin(caller);
        Iter.toArray(natMap.vals(pendingIntents));
    };
```

- [ ] **Step 3: Verify compile**

Run: `dfx build ponzi_math_sol --network=local 2>&1 | tail -20`
Expected: clean build.

- [ ] **Step 4: Commit**

```bash
git add ponzi_math_sol/main.mo
git commit -m "feat(ponzi_math_sol): prepareSolDeposit + intent management

User commits to (plan, expectedAmountLamports) before sending SOL.
Records a DepositIntent (10-minute TTL) and returns the deposit
address + intent ID. Inherits ponzi_math's 3-deposits-per-hour gate
and 0.1 ICP → 0.05 SOL minimum.

Refuses if bootstrap() hasn't run yet (so SOL can't accumulate on
addresses the canister can't yet act on)."
```

---

## Task 12: Implement `bootstrap()` — durable nonce account setup

**Files:**
- Modify: `ponzi_math_sol/main.mo`

Bootstrap is the one place where we DO use `getLatestBlockhash` (because there's no nonce yet). We retry up to 5 times to ride out the consensus flakiness.

After bootstrap, every routine outbound tx prepends `advance_nonce_account` instead of relying on a fresh blockhash.

- [ ] **Step 1: Add bootstrap helper functions above the `bootstrap` stub**

```motoko
    // ====================================================================
    // Bootstrap helpers
    // ====================================================================

    func ensureNonceAccountAddress() : async Text {
        switch (nonceAccountAddress) {
            case (?addr) { addr };
            case (null) {
                let addr = await SolSigner.deriveAddress(keyId, derivationPathNonce());
                nonceAccountAddress := ?addr;
                addr;
            };
        };
    };

    /// Fetch a recent blockhash, retrying on consensus failures. Used
    /// ONLY by bootstrap — all other outbound txs use the durable nonce.
    func fetchRecentBlockhashWithRetry(attempts : Nat) : async ?Text {
        var i : Nat = 0;
        while (i < attempts) {
            let res = await solRpc.getLatestBlockhash(?{ provider = ?solRpcProvider });
            switch (res) {
                case (#Ok({ blockhash; lastValidBlockHeight = _ })) { return ?blockhash };
                case (#Err(_)) { i += 1 };
            };
        };
        null;
    };

    /// Parse 32 bytes of nonce account body as a base58 blockhash.
    /// Solana nonce-account layout (System program account state):
    ///   bytes 0..4 — version (u32 LE)
    ///   bytes 4..8 — state (u32 LE; 1 = Initialized)
    ///   bytes 8..40 — authority pubkey (32 bytes)
    ///   bytes 40..72 — nonce value (32 bytes — what we want)
    ///   bytes 72..80 — fee_calculator.lamports_per_signature (u64 LE)
    func parseNonceFromAccountData(data : Blob) : ?Text {
        let arr = Blob.toArray(data);
        if (arr.size() < 72) { return null };
        let nonceBytes = Array.tabulate<Nat8>(32, func(i) { arr[40 + i] });
        ?Base58.encode(Blob.fromArray(nonceBytes));
    };
```

- [ ] **Step 2: Replace the `bootstrap` stub with the real implementation**

Find:

```motoko
    public shared ({ caller }) func bootstrap() : async { #Ok : Text; #Err : Text } {
        requireAdmin(caller);
        Debug.trap("bootstrap(): not yet implemented (Task 12)");
    };
```

Replace with:

```motoko
    public shared ({ caller }) func bootstrap() : async { #Ok : Text; #Err : Text } {
        requireAdmin(caller);
        if (bootstrapped) { return #Ok("already-bootstrapped") };

        acquireGlobalLock();
        try {
            // 1. Derive pool + nonce addresses.
            let pool = await ensurePoolAddress();
            let nonce = await ensureNonceAccountAddress();

            // 2. Confirm pool funded (~0.003 SOL = 3M lamports minimum).
            let balanceRes = await solRpc.getBalance(pool, ?{ provider = ?solRpcProvider });
            let balance = switch (balanceRes) {
                case (#Ok(b)) { b };
                case (#Err(e)) {
                    return #Err("getBalance(pool) failed: " # rpcErrorText(e));
                };
            };
            if (balance < 3_000_000 : Nat64) {
                return #Err("Pool address " # pool # " has only " # Nat64.toText(balance) # " lamports; needs ≥3,000,000 (≈0.003 SOL). Fund and retry.");
            };

            // 3. Build the bootstrap tx: createAccount(pool → nonce, 1.5M lamports, 80 bytes, SystemProgram) + initializeNonceAccount(nonce, pool).
            let createIx = SolTx.createAccountIx(
                pool,
                nonce,
                1_500_000 : Nat64,   // rent-exempt minimum for 80 bytes is ~1.44M; round up.
                SolTx.NONCE_ACCOUNT_SPACE,
                SolTx.SYSTEM_PROGRAM_ID,
            );
            let initIx = SolTx.initializeNonceIx(nonce, pool);

            // 4. Fetch a recent blockhash (with retry).
            let blockhash = switch (await fetchRecentBlockhashWithRetry(5)) {
                case (?h) { h };
                case (null) { return #Err("getLatestBlockhash failed after 5 retries") };
            };

            // 5. Compile + serialize the message.
            let compiled = SolTx.compile(pool, blockhash, [createIx, initIx]);
            let msgBytes = SolTx.serializeMessage(compiled);

            // 6. Sign with pool + nonce derivation paths. The bootstrap tx
            //    is unique in that BOTH the funder (pool) and the new
            //    account (nonce) must sign.
            let sigs = await SolSigner.signMulti(
                keyId,
                [derivationPathPool(), derivationPathNonce()],
                msgBytes,
            );

            // 7. Assemble + broadcast.
            let txBytes = SolTx.assembleTransaction(msgBytes, sigs);
            let sendRes = await solRpc.sendTransaction(
                txBytes,
                ?{
                    skipPreflight = ?false;
                    preflightCommitment = ?"confirmed";
                    maxRetries = ?(3 : Nat64);
                    encoding = ?"base64";
                },
                ?{ provider = ?solRpcProvider },
            );
            let txSig = switch (sendRes) {
                case (#Ok(s)) { s };
                case (#Err(e)) { return #Err("sendTransaction failed: " # rpcErrorText(e)) };
            };

            // 8. Fetch nonce account state to read the initial nonce value.
            //    Try a few times — confirmation may lag the send.
            var attempts : Nat = 0;
            var initialNonce : ?Text = null;
            while (attempts < 10 and initialNonce == null) {
                let acctRes = await solRpc.getAccountInfo(nonce, ?{ commitment = ?"confirmed"; encoding = ?"base64" }, ?{ provider = ?solRpcProvider });
                switch (acctRes) {
                    case (#Ok(?account)) {
                        initialNonce := parseNonceFromAccountData(account.data);
                    };
                    case (_) {};
                };
                attempts += 1;
            };
            switch (initialNonce) {
                case (?n) {
                    lastNonceValue := ?n;
                    bootstrapped := true;
                    #Ok("bootstrapped; nonce-account=" # nonce # " initial-nonce=" # n # " tx=" # txSig);
                };
                case (null) {
                    #Err("createAccount+initializeNonceAccount broadcast as tx " # txSig # ", but getAccountInfo could not parse the nonce body after 10 retries. Inspect on devnet explorer and re-run bootstrap.");
                };
            };
        } finally {
            releaseGlobalLock();
        };
    };

    /// Convenience: render an RpcError for #Err returns.
    func rpcErrorText(e : SolRpc.RpcError) : Text {
        switch (e) {
            case (#ProviderError(m)) { "ProviderError: " # m };
            case (#HttpOutcallError(m)) { "HttpOutcallError: " # m };
            case (#JsonRpcError({ code; message })) { "JsonRpcError(" # Int.toText(code) # "): " # message };
            case (#ConsensusError(m)) { "ConsensusError: " # m };
            case (#ValidationError(m)) { "ValidationError: " # m };
        };
    };
```

- [ ] **Step 3: Verify compile**

Run: `dfx build ponzi_math_sol --network=local 2>&1 | tail -30`
Expected: clean build. Possible issues:
- `Nat64` comparisons (`<` between literal and Nat64) may need explicit casts. If so, change `balance < 3_000_000 : Nat64` to `balance < (3_000_000 : Nat64)`.
- `Cycles.add` is called inside `SolSigner.sign`; the bootstrap tx will burn ~60B cycles (2 sign calls). Cycles balance check is informational; the call will trap on insufficient cycles if it can't pay.

- [ ] **Step 4: Commit**

```bash
git add ponzi_math_sol/main.mo
git commit -m "feat(ponzi_math_sol): implement bootstrap() durable nonce setup

One-shot admin call that:
1. Derives pool + nonce account addresses
2. Verifies pool funded (≥0.003 SOL)
3. Builds createAccount + initializeNonceAccount tx
4. Fetches a recent blockhash (5-retry tolerance for consensus flake)
5. Signs with both pool and nonce derivation paths
6. Broadcasts via sol-rpc.sendTransaction
7. Reads the initial nonce value from the new account's data
8. Marks bootstrapped=true, caches the nonce value

After bootstrap, every routine outbound tx prepends
advance_nonce_account and bumps lastNonceValue locally (Task 15).
Idempotent — second call returns 'already-bootstrapped' without
re-signing."
```

---

## Task 13: Implement deposit detection — polling signatures

**Files:**
- Modify: `ponzi_math_sol/main.mo`

The detection loop iterates over every known deposit address, polls `getSignaturesForAddress` for new signatures past the per-address cursor, and queues them for transaction-fetching (Task 14).

For M1 we run the loop on-demand via `runDepositDetection()` (admin-callable) rather than on a recurring timer. Timer integration can land in M2 alongside the shenanigans observer changes.

- [ ] **Step 1: Add detection helpers**

Add inside the actor body, near the other internal helpers:

```motoko
    // ====================================================================
    // Deposit detection
    // ====================================================================

    /// A single new-signature record discovered for a deposit address.
    type DetectedSignature = {
        address : Text;
        principal : Principal;
        signature : Text;
        slot : Nat64;
    };

    /// Scan a single deposit address for new inbound signatures.
    /// Returns the list of signatures observed past lastSeenSignature
    /// (chronologically ordered: oldest first). DOES NOT mutate
    /// lastSeenSignature; that happens after the credit step succeeds in
    /// Task 14.
    func scanAddress(address : Text, principal : Principal) : async [DetectedSignature] {
        let cursor = textMap.get(lastSeenSignature, address);
        // getSignaturesForAddress returns newest-first. We page until we
        // see the cursor (or exhaust). For M1 we do one page (up to 100
        // signatures) — devnet test volume is well under that.
        let res = await solRpc.getSignaturesForAddress(
            address,
            ?{
                limit = ?100;
                before = null;
                until = cursor;
                commitment = ?"confirmed";
            },
            ?{ provider = ?solRpcProvider },
        );
        switch (res) {
            case (#Err(_)) { [] };
            case (#Ok(sigs)) {
                // Reverse so we process oldest-first.
                let buf = Buffer.Buffer<DetectedSignature>(sigs.size());
                var i : Nat = sigs.size();
                while (i > 0) {
                    i -= 1;
                    let s = sigs[i];
                    if (s.err == null) {
                        buf.add({
                            address;
                            principal;
                            signature = s.signature;
                            slot = s.slot;
                        });
                    };
                };
                Buffer.toArray(buf);
            };
        };
    };
```

- [ ] **Step 2: Add `Buffer` import at the top if not already present**

If the original `ponzi_math/main.mo` doesn't import `Buffer`, add:
```motoko
import Buffer "mo:base/Buffer";
```

(Verify by grepping the existing imports first; only add if missing.)

- [ ] **Step 3: Verify compile**

Run: `dfx build ponzi_math_sol --network=local 2>&1 | tail -20`
Expected: clean build.

- [ ] **Step 4: Commit**

```bash
git add ponzi_math_sol/main.mo
git commit -m "feat(ponzi_math_sol): scaffold deposit-detection signature scanner

scanAddress polls sol-rpc.getSignaturesForAddress for new signatures
past the per-address cursor. Returns DetectedSignature records
oldest-first so the Task 14 credit step processes them in arrival
order. Cursor mutation is deferred until after successful credit
(Task 14) to preserve at-least-once detection semantics."
```

---

## Task 14: Implement deposit credit + sweep tx

**Files:**
- Modify: `ponzi_math_sol/main.mo`

For each `DetectedSignature`:
1. Fetch the full transaction via `getTransaction`.
2. Compute the inbound lamports landed on the deposit address (`postBalances[idx] - preBalances[idx]`).
3. Match the amount against any open intent for that principal (within ±1% tolerance). If matched, create a `GameRecord`. If unmatched, flag for admin review.
4. Build a sweep tx (advance_nonce + transfer entire balance from deposit address → pool address), sign with the per-user derivation path, broadcast.
5. Advance `lastSeenSignature` only after sweep + ledger mutations succeed.

- [ ] **Step 1: Add credit + sweep helper**

Inside the actor body, after `scanAddress`:

```motoko
    /// Default cycle attach for outbound sol-rpc calls. Holds enough for
    /// getTransaction + sendTransaction round-trips; sign_with_schnorr
    /// adds its own attach inside SolSigner.sign.
    transient let COVER_CHARGE_RATE_LAMPORTS_BPS : Nat64 = 400; // 4% = 400 / 10_000

    func bpsApply(amount : Nat64, bps : Nat64) : Nat64 {
        amount * bps / 10_000;
    };

    /// Convert lamports → SOL Float (9 decimals). Matches the canister-
    /// wide Float convention used by Game.amount, platformStats, etc.
    func lamportsToSol(lamports : Nat64) : Float {
        Float.fromInt(Nat64.toNat(lamports)) / 1_000_000_000.0;
    };

    /// Build + sign + broadcast a sweep tx from `fromAddress` (per-user
    /// deposit address) to the pool address for `lamports` lamports.
    /// Bumps lastNonceValue on success.
    func sweepToPool(fromAddress : Text, fromDerivationPath : [Blob], lamports : Nat64) : async { #Ok : Text; #Err : Text } {
        let pool = switch (poolAddress) {
            case (null) { return #Err("Pool address not derived") };
            case (?p) { p };
        };
        let nonceAddr = switch (nonceAccountAddress) {
            case (null) { return #Err("Nonce account not initialized") };
            case (?n) { n };
        };
        let nonceVal = switch (lastNonceValue) {
            case (null) { return #Err("Nonce value cache empty — call adminRefreshNonce") };
            case (?n) { n };
        };

        // Sweep tx leaves a tiny dust on the per-user address for the
        // network fee (~5000 lamports). The credit accounting accepts
        // this loss as a fixed overhead — it's accrued against the
        // pot/cover-charge split, not the player.
        let sweepLamports : Nat64 = if (lamports > 5_000) { lamports - 5_000 } else {
            return #Err("Detected amount below network-fee floor");
        };

        let nonceIx = SolTx.advanceNonceIx(nonceAddr, pool);
        let transferIx = SolTx.transferIx(fromAddress, pool, sweepLamports);
        // Compile with the per-user address as feePayer so the sweep
        // tx's tx fee is paid by the per-user dust. (Pool address pays
        // nothing on a sweep — keeps pot intact.)
        let compiled = SolTx.compile(fromAddress, nonceVal, [nonceIx, transferIx]);
        let msgBytes = SolTx.serializeMessage(compiled);
        // Two signers: per-user address (feePayer + transfer source) AND
        // pool address (nonce authority on advance_nonce_account).
        let sigs = await SolSigner.signMulti(keyId, [fromDerivationPath, derivationPathPool()], msgBytes);
        let txBytes = SolTx.assembleTransaction(msgBytes, sigs);

        let sendRes = await solRpc.sendTransaction(
            txBytes,
            ?{ skipPreflight = ?false; preflightCommitment = ?"confirmed"; maxRetries = ?(3 : Nat64); encoding = ?"base64" },
            ?{ provider = ?solRpcProvider },
        );
        switch (sendRes) {
            case (#Err(e)) { #Err("sendTransaction failed: " # rpcErrorText(e)) };
            case (#Ok(txSig)) {
                // After successful broadcast, refresh the nonce by reading
                // account info. Future txs use the new value.
                let acctRes = await solRpc.getAccountInfo(nonceAddr, ?{ commitment = ?"confirmed"; encoding = ?"base64" }, ?{ provider = ?solRpcProvider });
                switch (acctRes) {
                    case (#Ok(?account)) {
                        switch (parseNonceFromAccountData(account.data)) {
                            case (?n) { lastNonceValue := ?n };
                            case (null) {};
                        };
                    };
                    case (_) {};
                };
                #Ok(txSig);
            };
        };
    };

    /// For a single DetectedSignature, fetch the tx, compute the inbound
    /// lamports, match against an open intent, credit the game, and
    /// sweep to the pool. Returns Ok on full success.
    func creditDeposit(sig : DetectedSignature) : async { #Ok : Nat; #Err : Text } {
        // 1. Fetch transaction details.
        let txRes = await solRpc.getTransaction(
            sig.signature,
            ?{
                commitment = ?"confirmed";
                maxSupportedTransactionVersion = ?(0 : Nat64);
                encoding = ?"json";
            },
            ?{ provider = ?solRpcProvider },
        );
        let tx = switch (txRes) {
            case (#Err(e)) { return #Err("getTransaction failed: " # rpcErrorText(e)) };
            case (#Ok(null)) { return #Err("Transaction not found / not confirmed yet") };
            case (#Ok(?t)) { t };
        };

        let meta = switch (tx.meta) {
            case (null) { return #Err("Transaction meta missing") };
            case (?m) { m };
        };
        if (meta.err != null) { return #Err("Transaction failed on-chain") };

        let message = switch (tx.transaction) {
            case (null) { return #Err("Transaction body missing") };
            case (?b) {
                switch (b.message) {
                    case (null) { return #Err("Message missing from transaction body") };
                    case (?m) { m };
                };
            };
        };

        // 2. Locate the deposit address inside accountKeys and compute the
        //    inbound delta.
        var addrIdx : ?Nat = null;
        var i : Nat = 0;
        while (i < message.accountKeys.size()) {
            if (message.accountKeys[i] == sig.address) { addrIdx := ?i };
            i += 1;
        };
        let idx = switch (addrIdx) {
            case (null) { return #Err("Deposit address not in transaction account keys (filter bug or false-positive)") };
            case (?n) { n };
        };
        if (idx >= meta.preBalances.size() or idx >= meta.postBalances.size()) {
            return #Err("Pre/post balances missing for deposit address");
        };
        let preBal = meta.preBalances[idx];
        let postBal = meta.postBalances[idx];
        if (postBal <= preBal) {
            // Outbound tx (probably a prior sweep we initiated). Advance
            // the cursor without crediting.
            return #Ok(0);
        };
        let inboundLamports : Nat64 = postBal - preBal;

        // 3. Find an open intent for this principal that matches the amount
        //    within ±5% tolerance.
        var matched : ?DepositIntent = null;
        for (intent in natMap.vals(pendingIntents)) {
            if (intent.principal == sig.principal and not intent.fulfilled) {
                let expected = intent.expectedAmountLamports;
                let tol = bpsApply(expected, 500); // 5%
                let lo : Nat64 = if (expected > tol) { expected - tol } else { 0 };
                let hi : Nat64 = expected + tol;
                if (inboundLamports >= lo and inboundLamports <= hi and Time.now() <= intent.expiresAt) {
                    matched := ?intent;
                };
            };
        };

        let intent = switch (matched) {
            case (null) {
                // Admin-review case. Don't trap — just log and advance the
                // cursor so the operator can later resolve via an admin
                // hatch.
                Debug.print("Unmatched deposit on " # sig.address # ": " # Nat64.toText(inboundLamports) # " lamports");
                return #Ok(0);
            };
            case (?intent) { intent };
        };

        // 4. Compute cover charge + net.
        let coverChargeLamports = bpsApply(inboundLamports, COVER_CHARGE_RATE_LAMPORTS_BPS);
        let netLamports = inboundLamports - coverChargeLamports;
        let depositSol = lamportsToSol(inboundLamports);
        let coverChargeSol = lamportsToSol(coverChargeLamports);
        let netSol = lamportsToSol(netLamports);

        // 5. Create the GameRecord, mark intent fulfilled, update stats,
        //    record ledger events.
        let gameId = nextGameId;
        nextGameId += 1;
        coverChargeAccrualLamports += coverChargeLamports;

        if (coverChargeLamports > 0) {
            recordLedger(#coverChargeAccrued({
                gameId;
                player = intent.principal;
                amountE8s = Nat64.toNat(coverChargeLamports); // Reusing the existing field as "lamports" here.
            }));
        };

        let newGame : GameRecord = {
            id = gameId;
            player = intent.principal;
            plan = intent.plan;
            amount = depositSol;
            startTime = Time.now();
            isCompounding = switch (intent.plan) {
                case (#simple21Day) { false };
                case (_) { true };
            };
            isActive = true;
            lastUpdateTime = Time.now();
            accumulatedEarnings = 0.0;
            totalWithdrawn = 0.0;
        };
        gameRecords := natMap.put(gameRecords, gameId, newGame);
        platformStats := {
            platformStats with
            totalDeposits = platformStats.totalDeposits + depositSol;
            activeGames = platformStats.activeGames + 1;
            potBalance = platformStats.potBalance + netSol;
        };
        recordLedger(#deposit({
            player = intent.principal;
            gameId;
            gross = depositSol;
            coverCharge = coverChargeSol;
            netToPot = netSol;
            plan = intent.plan;
            isCompounding = newGame.isCompounding;
        }));

        // Record the per-user deposit timestamp (rate-limit bookkeeping).
        let now = Time.now();
        let oneHourAgo = now - 3_600_000_000_000;
        switch (principalMapNat.get(depositTimestamps, intent.principal)) {
            case (null) {
                depositTimestamps := principalMapNat.put(depositTimestamps, intent.principal, List.push(now, List.nil()));
            };
            case (?ts) {
                let filtered = List.filter<Int>(ts, func(t) { t > oneHourAgo });
                depositTimestamps := principalMapNat.put(depositTimestamps, intent.principal, List.push(now, filtered));
            };
        };

        // Mark intent fulfilled.
        pendingIntents := natMap.put(pendingIntents, intent.id, { intent with fulfilled = true });

        // 6. Sweep to pool.
        switch (await sweepToPool(sig.address, derivationPathForPrincipal(intent.principal), inboundLamports)) {
            case (#Err(e)) {
                // The pot already credits this deposit. Sweep failure
                // leaves SOL on the per-user address; admin retry later.
                Debug.print("Sweep failed for " # sig.signature # ": " # e);
            };
            case (#Ok(_)) {};
        };

        // 7. Advance the per-address cursor.
        lastSeenSignature := textMap.put(lastSeenSignature, sig.address, sig.signature);

        #Ok(gameId);
    };
```

- [ ] **Step 2: Replace the Task 8 stub of `runDepositDetection`**

Find:

```motoko
    public shared ({ caller }) func runDepositDetection() : async { #Ok : Nat; #Err : Text } {
        requireAdmin(caller);
        Debug.trap("runDepositDetection: not yet implemented (Task 12-14)");
    };
```

Replace with:

```motoko
    /// Admin-callable detection sweep. Iterates every known deposit
    /// address, fetches new signatures, credits matching intents, sweeps
    /// to pool. Returns the number of new credits made (zero is normal).
    public shared ({ caller }) func runDepositDetection() : async { #Ok : Nat; #Err : Text } {
        requireAdmin(caller);
        if (not bootstrapped) { return #Err("Not bootstrapped") };

        var credits : Nat = 0;
        for ((address, principal) in textMap.entries(addressToPrincipal)) {
            let sigs = await scanAddress(address, principal);
            for (sig in sigs.vals()) {
                switch (await creditDeposit(sig)) {
                    case (#Ok(gid)) { if (gid > 0) { credits += 1 } };
                    case (#Err(e)) {
                        Debug.print("creditDeposit error for " # sig.signature # ": " # e);
                    };
                };
            };
        };
        #Ok(credits);
    };

    /// Admin: refresh the cached nonce by reading account info. Used to
    /// recover from a desync (e.g., outbound tx broadcast but nonce read
    /// failed). Idempotent.
    public shared ({ caller }) func adminRefreshNonce() : async { #Ok : Text; #Err : Text } {
        requireAdmin(caller);
        let nonceAddr = switch (nonceAccountAddress) {
            case (null) { return #Err("Nonce account not initialized") };
            case (?n) { n };
        };
        let res = await solRpc.getAccountInfo(nonceAddr, ?{ commitment = ?"confirmed"; encoding = ?"base64" }, ?{ provider = ?solRpcProvider });
        switch (res) {
            case (#Err(e)) { #Err("getAccountInfo: " # rpcErrorText(e)) };
            case (#Ok(null)) { #Err("Nonce account not found on-chain") };
            case (#Ok(?account)) {
                switch (parseNonceFromAccountData(account.data)) {
                    case (?n) { lastNonceValue := ?n; #Ok(n) };
                    case (null) { #Err("Could not parse nonce from account data") };
                };
            };
        };
    };
```

- [ ] **Step 3: Verify compile**

Run: `dfx build ponzi_math_sol --network=local 2>&1 | tail -30`
Expected: clean build.

- [ ] **Step 4: Commit**

```bash
git add ponzi_math_sol/main.mo
git commit -m "feat(ponzi_math_sol): deposit credit + sweep to pool

runDepositDetection() (admin-callable for M1; timer integration is
M2) iterates every known deposit address and:
1. Polls sol-rpc.getSignaturesForAddress for new signatures
2. Fetches each via getTransaction and computes inbound delta
3. Matches against the user's open DepositIntent (±5% amount, TTL)
4. Creates a GameRecord, accrues cover charge, records ledger events
5. Sweeps the deposit address → pool via advance_nonce + transfer
6. Advances the per-address signature cursor

Unmatched / TTL-expired deposits are logged for admin review rather
than trapped — operator resolves manually."
```

---

## Task 15: Implement SOL payout helper (replaces icrc1_transfer)

**Files:**
- Modify: `ponzi_math_sol/main.mo`

A single `sendSolPayout(toAddress, lamports)` helper used by withdrawals, settlements, and backer-repayment claims. Builds advance_nonce + transfer, signs with pool, broadcasts.

- [ ] **Step 1: Add the helper above the withdrawEarnings function**

```motoko
    // ====================================================================
    // SOL payouts (used by withdraw, settle, claimRepayment, payManagement)
    // ====================================================================

    /// Build and broadcast a SOL transfer FROM the pool TO `toAddress`
    /// for `lamports` lamports. Returns the tx signature on success.
    /// Bumps lastNonceValue on success.
    func sendSolPayout(toAddress : Text, lamports : Nat64) : async { #Ok : Text; #Err : Text } {
        let pool = switch (poolAddress) {
            case (null) { return #Err("Pool address not derived") };
            case (?p) { p };
        };
        let nonceAddr = switch (nonceAccountAddress) {
            case (null) { return #Err("Nonce account not initialized") };
            case (?n) { n };
        };
        let nonceVal = switch (lastNonceValue) {
            case (null) { return #Err("Nonce value cache empty — call adminRefreshNonce") };
            case (?n) { n };
        };
        if (not Base58.isPlausibleSolanaAddress(toAddress)) {
            return #Err("Destination is not a valid Solana address: " # toAddress);
        };

        let advanceIx = SolTx.advanceNonceIx(nonceAddr, pool);
        let transferIx = SolTx.transferIx(pool, toAddress, lamports);
        let compiled = SolTx.compile(pool, nonceVal, [advanceIx, transferIx]);
        let msgBytes = SolTx.serializeMessage(compiled);

        // Pool is the only signer (both feePayer and nonce authority).
        let sigs = await SolSigner.signMulti(keyId, [derivationPathPool()], msgBytes);
        let txBytes = SolTx.assembleTransaction(msgBytes, sigs);

        let sendRes = await solRpc.sendTransaction(
            txBytes,
            ?{ skipPreflight = ?false; preflightCommitment = ?"confirmed"; maxRetries = ?(3 : Nat64); encoding = ?"base64" },
            ?{ provider = ?solRpcProvider },
        );
        switch (sendRes) {
            case (#Err(e)) { #Err("sendTransaction failed: " # rpcErrorText(e)) };
            case (#Ok(txSig)) {
                // Refresh nonce.
                let acctRes = await solRpc.getAccountInfo(nonceAddr, ?{ commitment = ?"confirmed"; encoding = ?"base64" }, ?{ provider = ?solRpcProvider });
                switch (acctRes) {
                    case (#Ok(?account)) {
                        switch (parseNonceFromAccountData(account.data)) {
                            case (?n) { lastNonceValue := ?n };
                            case (null) {};
                        };
                    };
                    case (_) {};
                };
                #Ok(txSig);
            };
        };
    };

    /// Convert SOL (Float) → lamports (Nat64) using ponzi_math's
    /// roundToEightDecimals → multiply by 10^9 path. We round to 9
    /// decimals here (matching SOL's natural precision).
    func solToLamports(sol : Float) : Nat64 {
        let lam = sol * 1_000_000_000.0;
        if (lam < 0.0) { return 0 };
        Nat64.fromNat(Int.abs(Float.toInt(lam)));
    };
```

- [ ] **Step 2: Verify compile**

Run: `dfx build ponzi_math_sol --network=local 2>&1 | tail -20`
Expected: clean build.

- [ ] **Step 3: Commit**

```bash
git add ponzi_math_sol/main.mo
git commit -m "feat(ponzi_math_sol): sendSolPayout helper for outbound transfers

Replaces icrc1_transfer for every pool→user path. Builds the standard
advance_nonce + transfer tx, signs with the pool derivation path,
broadcasts via sol-rpc.sendTransaction, refreshes the local nonce
cache from account info on success.

Destination validation rejects unparseable base58 / wrong-length
addresses pre-flight."
```

---

## Task 16: Wire SOL payout into `withdrawEarnings`

**Files:**
- Modify: `ponzi_math_sol/main.mo`

Replace the Task 8 stub `Debug.trap` inside `withdrawEarnings` with a call to `sendSolPayout`. The math, locks, ledger events all stay verbatim.

- [ ] **Step 1: Find and edit `withdrawEarnings`**

In `ponzi_math_sol/main.mo`, locate `withdrawEarnings`. It contains a block that used to call `icrc1_transfer`; Task 8 stubbed that to `Debug.trap("withdrawEarnings: SOL payout not yet wired")`. Replace that trap with the SOL payout.

The relevant block originally (in ponzi_math) was:

```motoko
                    let netEarningsE8s = Int.abs(Float.toInt(actualNetEarnings * 100_000_000.0));
                    if (netEarningsE8s > Ledger.ICP_TRANSFER_FEE) {
                        let transferAmount : Nat = netEarningsE8s - Ledger.ICP_TRANSFER_FEE;
                        let transferResult = try {
                            await icpLedger.icrc1_transfer({ ... });
                        } catch (e) {
                            // revert state
                        };
                        switch (transferResult) {
                            case (#Err(err)) { /* revert */ };
                            case (#Ok(_)) {};
                        };
                    };
```

After Task 8 it became:
```motoko
                    Debug.trap("withdrawEarnings: SOL payout not yet wired");
```

Replace that with:

```motoko
                    let netLamports = solToLamports(actualNetEarnings);
                    let solFee : Nat64 = 5_000; // Solana network fee floor
                    if (netLamports > solFee) {
                        let payoutLamports : Nat64 = netLamports - solFee;
                        let destination = switch (principalMapNat.get(depositAddresses, caller)) {
                            case (?addr) { addr };
                            case (null) {
                                gameRecords := natMap.put(gameRecords, gameId, originalGame);
                                platformStats := originalStats;
                                backerRepayments := originalRepayments;
                                roundSeedReserve := originalSeedReserve;
                                return #Err("Caller has no deposit address; cannot pay out. Call getOrCreateDepositAddress first.");
                            };
                        };
                        switch (await sendSolPayout(destination, payoutLamports)) {
                            case (#Err(e)) {
                                gameRecords := natMap.put(gameRecords, gameId, originalGame);
                                platformStats := originalStats;
                                backerRepayments := originalRepayments;
                                roundSeedReserve := originalSeedReserve;
                                return #Err("SOL payout failed: " # e);
                            };
                            case (#Ok(_txSig)) {};
                        };
                    };
```

- [ ] **Step 2: Verify compile**

Run: `dfx build ponzi_math_sol --network=local 2>&1 | tail -20`
Expected: clean build.

- [ ] **Step 3: Commit**

```bash
git add ponzi_math_sol/main.mo
git commit -m "feat(ponzi_math_sol): wire SOL payout into withdrawEarnings

Replaces the Task 8 stub with sendSolPayout. Destination defaults to
the caller's deposit address (always available since they had to
prepareSolDeposit to open the position). Network-fee floor uses
Solana's standard 5_000 lamports per signature. On RPC error, the
existing rollback path restores all state in lock-step with the
ponzi_math behavior."
```

---

## Task 17: Wire SOL payout into `settleCompoundingGame`

**Files:**
- Modify: `ponzi_math_sol/main.mo`

Identical pattern to Task 16, applied to the compounding settlement function.

- [ ] **Step 1: Find and edit `settleCompoundingGame`**

Locate the stub inside `settleCompoundingGame`:
```motoko
                    Debug.trap("settleCompoundingGame: SOL payout not yet wired");
```
(Or wherever Task 8 left the trap — there should be exactly one per the three payout functions.)

Replace with the same payout block as Task 16 Step 1, except `actualNetEarnings` is the variable name in the compounding settlement scope (same name, no rename needed).

```motoko
                    let netLamports = solToLamports(actualNetEarnings);
                    let solFee : Nat64 = 5_000;
                    if (netLamports > solFee) {
                        let payoutLamports : Nat64 = netLamports - solFee;
                        let destination = switch (principalMapNat.get(depositAddresses, caller)) {
                            case (?addr) { addr };
                            case (null) {
                                gameRecords := natMap.put(gameRecords, gameId, originalGame);
                                platformStats := originalStats;
                                backerRepayments := originalRepayments;
                                roundSeedReserve := originalSeedReserve;
                                return #Err("Caller has no deposit address; cannot pay out. Call getOrCreateDepositAddress first.");
                            };
                        };
                        switch (await sendSolPayout(destination, payoutLamports)) {
                            case (#Err(e)) {
                                gameRecords := natMap.put(gameRecords, gameId, originalGame);
                                platformStats := originalStats;
                                backerRepayments := originalRepayments;
                                roundSeedReserve := originalSeedReserve;
                                return #Err("SOL payout failed: " # e);
                            };
                            case (#Ok(_txSig)) {};
                        };
                    };
```

- [ ] **Step 2: Verify compile**

Run: `dfx build ponzi_math_sol --network=local 2>&1 | tail -20`
Expected: clean build.

- [ ] **Step 3: Commit**

```bash
git add ponzi_math_sol/main.mo
git commit -m "feat(ponzi_math_sol): wire SOL payout into settleCompoundingGame

Same payout pattern as withdrawEarnings (Task 16) — pays to the
caller's deposit address, uses 5000-lamport fee floor, rolls back
state on RPC error."
```

---

## Task 18: Wire SOL payout into `claimBackerRepayment`

**Files:**
- Modify: `ponzi_math_sol/main.mo`

Final of the three payout sites. Same pattern.

- [ ] **Step 1: Find and edit `claimBackerRepayment`**

Locate the stub:
```motoko
                Debug.trap("claimBackerRepayment: SOL payout not yet wired");
```

Replace with:

```motoko
            let netLamports = solToLamports(balance);
            let solFee : Nat64 = 5_000;
            if (netLamports <= solFee) {
                backerRepayments := backerKeyMap.put(backerRepayments, (caller, #seriesA), aBalance);
                backerRepayments := backerKeyMap.put(backerRepayments, (caller, #seriesB), bBalance);
                return #Err("Claimable balance is below the Solana network fee; wait until your balance grows past 5,000 lamports");
            };
            let payoutLamports : Nat64 = netLamports - solFee;
            let destination = switch (principalMapNat.get(depositAddresses, caller)) {
                case (?addr) { addr };
                case (null) {
                    backerRepayments := backerKeyMap.put(backerRepayments, (caller, #seriesA), aBalance);
                    backerRepayments := backerKeyMap.put(backerRepayments, (caller, #seriesB), bBalance);
                    return #Err("Caller has no deposit address; call getOrCreateDepositAddress first.");
                };
            };
            switch (await sendSolPayout(destination, payoutLamports)) {
                case (#Err(e)) {
                    backerRepayments := backerKeyMap.put(backerRepayments, (caller, #seriesA), aBalance);
                    backerRepayments := backerKeyMap.put(backerRepayments, (caller, #seriesB), bBalance);
                    return #Err("SOL payout failed: " # e);
                };
                case (#Ok(_txSig)) {};
            };
```

The surrounding bookkeeping (zeroing the repayment balances, recording `#backerRepaymentClaim`) stays as in `ponzi_math` — those lines should already be present, untouched.

- [ ] **Step 2: Verify compile**

Run: `dfx build ponzi_math_sol --network=local 2>&1 | tail -20`
Expected: clean build.

- [ ] **Step 3: Commit**

```bash
git add ponzi_math_sol/main.mo
git commit -m "feat(ponzi_math_sol): wire SOL payout into claimBackerRepayment

Same pattern as withdrawEarnings/settleCompoundingGame. On RPC error
restores both Series A and Series B repayment balances in lockstep
with the existing ponzi_math rollback path."
```

---

## Task 19: Implement `payManagementSol` (cover-charge sweep to operator's Phantom)

**Files:**
- Modify: `ponzi_math_sol/main.mo`

Replaces ponzi_math's `sweepCoverCharges` (which sent to `BACKEND_PRINCIPAL`). Destination is the admin-tunable `solTreasuryAddress`, defaulting to the operator's personal Phantom address per the spec.

- [ ] **Step 1: Replace the Task 8 stub of `payManagementSol`**

Find:
```motoko
    public shared ({ caller }) func payManagementSol() : async { #Ok : Text; #Err : Text } {
        let _ = caller;
        Debug.trap("payManagementSol: not yet implemented (Task 19)");
    };
```

Replace with:

```motoko
    public shared ({ caller }) func payManagementSol() : async { #Ok : Text; #Err : Text } {
        requireAdmin(caller);
        if (coverChargeAccrualLamports == 0) { return #Err("Nothing to sweep") };
        if (not Base58.isPlausibleSolanaAddress(solTreasuryAddress)) {
            return #Err("solTreasuryAddress is not a valid Solana address: " # solTreasuryAddress);
        };

        acquireGlobalLock();
        try {
            let amount = coverChargeAccrualLamports;
            let solFee : Nat64 = 5_000;
            if (amount <= solFee) {
                return #Err("Accumulated balance below transfer fee");
            };
            let payout : Nat64 = amount - solFee;

            // Zero internal accrual BEFORE the outbound call — same
            // pattern as sweepCoverCharges.
            coverChargeAccrualLamports := 0;

            switch (await sendSolPayout(solTreasuryAddress, payout)) {
                case (#Err(e)) {
                    coverChargeAccrualLamports := amount;
                    #Err("SOL payout failed: " # e);
                };
                case (#Ok(txSig)) {
                    recordLedger(#coverChargeSwept({
                        amountE8s = Nat64.toNat(amount); // Reusing the field as lamports.
                        toBackend = BACKEND_PRINCIPAL; // For audit only; the actual destination is solTreasuryAddress (in the ledger note).
                        blockIndex = 0;                 // No block index — SOL tx; signature lives in the ledger as a separate note.
                    }));
                    #Ok(txSig);
                };
            };
        } finally {
            releaseGlobalLock();
        };
    };

    /// Admin: update solTreasuryAddress (the destination of payManagementSol).
    public shared ({ caller }) func adminSetSolTreasuryAddress(addr : Text) : async { #Ok; #Err : Text } {
        requireAdmin(caller);
        if (not Base58.isPlausibleSolanaAddress(addr)) {
            return #Err("Not a valid Solana address: " # addr);
        };
        solTreasuryAddress := addr;
        #Ok;
    };

    public query func getSolTreasuryAddress() : async Text { solTreasuryAddress };
    public query func getCoverChargeAccrualLamports() : async Nat64 { coverChargeAccrualLamports };
```

- [ ] **Step 2: Verify compile**

Run: `dfx build ponzi_math_sol --network=local 2>&1 | tail -20`
Expected: clean build.

- [ ] **Step 3: Commit**

```bash
git add ponzi_math_sol/main.mo
git commit -m "feat(ponzi_math_sol): payManagementSol cover-charge sweep

Admin-callable sweep of coverChargeAccrualLamports from the pool
address to solTreasuryAddress (set at deploy to the operator's
personal Phantom address per spec).

Includes adminSetSolTreasuryAddress for retargeting and a getter for
the destination + accrual balance — surfaces feed the admin wallet
widget once the frontend lands."
```

---

## Task 20: Admin hatches — register operator backer, manually credit deposits, force resync

**Files:**
- Modify: `ponzi_math_sol/main.mo`

Three small admin hatches the operator needs for M1:
1. `adminRegisterSeriesABacker(owner, amount)` — record the operator's Series A seed without an on-chain deposit flow.
2. `adminCreditManualDeposit(principal, plan, lamports)` — accept an unmatched/expired deposit and create a game record.
3. `adminClearAllBackerPositions` is already in the ponzi_math fork; verify it still compiles. Same for `adminMergeBackerPosition`, `adminForceReset`, `createBackdatedGame`.

We also remove the original `adminSweepUntracked` (which relied on `icpLedger.icrc1_balance_of`) — replace with a SOL-aware version.

- [ ] **Step 1: Replace the Task 8 stub of `adminRegisterSeriesABacker`**

Find:
```motoko
    public shared ({ caller }) func adminRegisterSeriesABacker(owner : Principal, amount : Float) : async { #Ok; #Err : Text } {
        let _ = caller;
        let _ = owner;
        let _ = amount;
        Debug.trap("adminRegisterSeriesABacker: not yet implemented (Task 20)");
    };
```

Replace with:

```motoko
    /// Admin: record a Series A backer position for `owner` of `amount`
    /// SOL. Use ONCE at deploy to register the operator's pre-deposited
    /// pool seed. Mirrors ponzi_math.addBackerMoney's bookkeeping but
    /// skips the synchronous transfer-from (the SOL is already on the
    /// pool address, deposited out-of-band by the operator).
    public shared ({ caller }) func adminRegisterSeriesABacker(owner : Principal, amount : Float) : async { #Ok; #Err : Text } {
        requireAdmin(caller);
        validateAmount(amount);
        if (amount < 0.05) { return #Err("Minimum is 0.05 SOL") };

        acquireGlobalLock();
        try {
            let entitlement = amount * 1.24;
            switch (backerKeyMap.get(backerPositions, (owner, #seriesA))) {
                case (null) {
                    let pos : BackerPosition = {
                        owner;
                        amount;
                        entitlement;
                        startTime = Time.now();
                        isActive = true;
                        backerType = #seriesA;
                        firstDepositDate = ?Time.now();
                    };
                    backerPositions := backerKeyMap.put(backerPositions, (owner, #seriesA), pos);
                };
                case (?existing) {
                    let updated : BackerPosition = {
                        existing with
                        amount = existing.amount + amount;
                        entitlement = existing.entitlement + entitlement;
                    };
                    backerPositions := backerKeyMap.put(backerPositions, (owner, #seriesA), updated);
                };
            };
            platformStats := { platformStats with potBalance = platformStats.potBalance + amount };
            recordLedger(#backerDeposit({ backer = owner; amount; entitlement }));
            #Ok;
        } finally {
            releaseGlobalLock();
        };
    };
```

- [ ] **Step 2: Add `adminCreditManualDeposit`**

After `adminRegisterSeriesABacker`:

```motoko
    /// Admin: manually credit an unmatched / TTL-expired SOL deposit.
    /// `lamports` is the gross detected amount; cover charge is
    /// computed at the standard 4% rate. Used to clear admin-review
    /// entries flagged by creditDeposit when no intent matched.
    public shared ({ caller }) func adminCreditManualDeposit(
        player : Principal,
        plan : GamePlan,
        lamports : Nat64,
    ) : async { #Ok : Nat; #Err : Text } {
        requireAdmin(caller);
        if (lamports < MIN_DEPOSIT_LAMPORTS) { return #Err("Below minimum deposit") };

        acquireGlobalLock();
        try {
            let coverChargeLamports = bpsApply(lamports, COVER_CHARGE_RATE_LAMPORTS_BPS);
            let netLamports = lamports - coverChargeLamports;
            let depositSol = lamportsToSol(lamports);
            let coverChargeSol = lamportsToSol(coverChargeLamports);
            let netSol = lamportsToSol(netLamports);

            let gameId = nextGameId;
            nextGameId += 1;
            coverChargeAccrualLamports += coverChargeLamports;

            let isCompounding = switch (plan) { case (#simple21Day) { false }; case (_) { true } };
            let game : GameRecord = {
                id = gameId;
                player;
                plan;
                amount = depositSol;
                startTime = Time.now();
                isCompounding;
                isActive = true;
                lastUpdateTime = Time.now();
                accumulatedEarnings = 0.0;
                totalWithdrawn = 0.0;
            };
            gameRecords := natMap.put(gameRecords, gameId, game);
            platformStats := {
                platformStats with
                totalDeposits = platformStats.totalDeposits + depositSol;
                activeGames = platformStats.activeGames + 1;
                potBalance = platformStats.potBalance + netSol;
            };
            recordLedger(#deposit({
                player;
                gameId;
                gross = depositSol;
                coverCharge = coverChargeSol;
                netToPot = netSol;
                plan;
                isCompounding;
            }));
            recordLedger(#backdatedGameCreated({
                admin = caller;
                player;
                gameId;
                startTime = Time.now();
                amount = depositSol;
            }));

            #Ok(gameId);
        } finally {
            releaseGlobalLock();
        };
    };
```

- [ ] **Step 3: Strip / replace the ICP-flavored test hatches**

`adminSweepUntracked` in the original calls `icpLedger.icrc1_balance_of` — that no longer exists. Either:
- Delete it (and surface the loss in a code comment for completeness), OR
- Reimplement with `sol-rpc.getBalance(pool)`.

Reimplement, since the spec lists test admin hatches as ported:

Find `adminSweepUntracked` in the fork and replace its body entirely with:

```motoko
    /// Admin: compute the difference between the pool address's actual
    /// on-chain balance and the sum of internal accounting (pot +
    /// roundSeedReserve + repayments + coverChargeAccrual). If positive
    /// (untracked dust), send it to the testAdmin's deposit address.
    /// No-op otherwise.
    public shared ({ caller }) func adminSweepUntracked() : async { #Ok : Text; #Err : Text } {
        if (caller != TEST_ADMIN) { return #Err("Unauthorized: testAdmin only") };
        acquireGlobalLock();
        try {
            let pool = switch (poolAddress) {
                case (null) { return #Err("Pool not derived") };
                case (?p) { p };
            };
            let balRes = await solRpc.getBalance(pool, ?{ provider = ?solRpcProvider });
            let actualLamports = switch (balRes) {
                case (#Ok(b)) { b };
                case (#Err(e)) { return #Err("getBalance: " # rpcErrorText(e)) };
            };

            var repaymentSum : Float = 0.0;
            for ((_, amount) in backerKeyMap.entries(backerRepayments)) {
                repaymentSum += amount;
            };
            let internalFloat = platformStats.potBalance + roundSeedReserve + repaymentSum;
            let internalLamports : Nat64 = solToLamports(internalFloat) + coverChargeAccrualLamports;

            if (actualLamports <= internalLamports) {
                return #Err("No untracked balance (actual=" # Nat64.toText(actualLamports) # ", internal=" # Nat64.toText(internalLamports) # ")");
            };
            let untracked : Nat64 = actualLamports - internalLamports;
            let solFee : Nat64 = 5_000;
            if (untracked <= solFee) {
                return #Err("Untracked balance below fee");
            };
            let payout : Nat64 = untracked - solFee;

            let destination = switch (principalMapNat.get(depositAddresses, caller)) {
                case (?addr) { addr };
                case (null) {
                    return #Err("testAdmin has no deposit address; call getOrCreateDepositAddress as testAdmin first");
                };
            };
            switch (await sendSolPayout(destination, payout)) {
                case (#Ok(txSig)) { #Ok(txSig) };
                case (#Err(e)) { #Err(e) };
            };
        } finally {
            releaseGlobalLock();
        };
    };
```

- [ ] **Step 4: Also remove (or stub-trap) `createBackdatedGame` and `adminMergeBackerPosition`**

`createBackdatedGame` was deeply tied to icrc2_transfer_from. For M1 we don't need it — it was a pre-blackhole test hatch for matured-position payouts. Delete it entirely.

`adminMergeBackerPosition` doesn't touch ICP; it stays as-is (no edits needed).

`adminClearAllBackerPositions` doesn't touch ICP; stays as-is.

`adminForceReset` doesn't touch ICP; stays as-is.

- [ ] **Step 5: Verify compile**

Run: `dfx build ponzi_math_sol --network=local 2>&1 | tail -30`
Expected: clean build. Likely remaining issues:
- If `createBackdatedGame` was referenced elsewhere (unlikely — it's a public method), remove those references.

- [ ] **Step 6: Commit**

```bash
git add ponzi_math_sol/main.mo
git commit -m "feat(ponzi_math_sol): admin hatches for backer seed + manual credit

- adminRegisterSeriesABacker(owner, amount): records the operator's
  Series A seed without an on-chain deposit (SOL already pre-funded
  on the pool address via the bootstrap flow)
- adminCreditManualDeposit(player, plan, lamports): clears
  admin-review entries from unmatched / TTL-expired SOL transfers
- adminSweepUntracked: SOL version of the dust-recovery hatch,
  using sol-rpc.getBalance(pool) for the actual balance read
- createBackdatedGame deleted (tied to icrc2_transfer_from; not
  needed for M1)

adminMergeBackerPosition, adminClearAllBackerPositions, and
adminForceReset stay untouched from the ponzi_math fork."
```

---

## Task 21: Local build + self-test of pure modules

**Files:** none — local verification only.

The pure modules (Base58, SolTx) can be smoke-tested locally without any threshold signing or sol-rpc calls.

- [ ] **Step 1: Start local replica + deploy ponzi_math_sol**

Run: `dfx start --clean --background`

Then deploy with the local key/devnet provider so the actor's init args are valid:

```bash
dfx deploy ponzi_math_sol --network=local --argument '(record { backendPrincipal = principal "5zxxg-tyaaa-aaaac-qeckq-cai"; testAdmin = principal "6pwpo-d5iaw-mfjrn-owfb3-v4oz6-72woh-pc5t2-cwn73-zrzeq-4bjeh-tqe"; solTreasuryAddress = "5EVdR6qcPuDqJb6W69fmcvTJjbEUGZqQtefEB8sK8QQ2"; solRpcProvider = variant { devnet }; keyId = record { algorithm = variant { ed25519 }; name = "dfx_test_key" } })'
```

Expected: "Building canisters..." → "Installing code for canister ponzi_math_sol..." → "Deployed canisters." with a local canister ID printed.

- [ ] **Step 2: Run the Base58 self-test**

Run: `dfx canister --network=local call ponzi_math_sol selfTestBase58`
Expected: `(true)` — confirms 32 zero bytes encode to the System Program ID (`11111111111111111111111111111111`).

If `(false)`, there's a bug in `Base58.encode`. Likely culprits:
- Wrong alphabet (verify '1234567...…xyz' is intact with no missing chars).
- Leading-zero handling off by one.
Fix and redeploy.

- [ ] **Step 3: Run the SolTx self-test**

Run: `dfx canister --network=local call ponzi_math_sol selfTestSolTx`

Expected record:
- `compactU16_42 = vec { 42 : nat8 }` — single byte.
- `compactU16_128 = vec { 128 : nat8; 1 : nat8 }` — 0x80, 0x01.
- `compactU16_300 = vec { 172 : nat8; 2 : nat8 }` — 0xAC, 0x02. (300 = 0b1_00101100; low 7 bits = 0x2C, high bit set → 0xAC; remaining = 2.)
- `u64Le_1 = vec { 1; 0; 0; 0; 0; 0; 0; 0 : nat8 }`.
- `u64Le_1B = vec { 0; 202; 154; 59; 0; 0; 0; 0 : nat8 }` — 1,000,000,000 in little-endian.

If anything mismatches, fix `SolTx.compactU16` or `u64Le` and redeploy.

- [ ] **Step 4: Stop dfx**

Run: `dfx stop`

- [ ] **Step 5: No commit — verification only.**

---

## Task 22: Deploy `ponzi_math_sol` to IC mainnet (configured for Solana devnet)

**Files:**
- Modify: `canister_ids.json`

- [ ] **Step 1: Create the mainnet canister**

Run: `dfx canister --network=ic create ponzi_math_sol`
Expected: "Created canister ponzi_math_sol with id <CANISTER_ID>." Note the ID.

- [ ] **Step 2: Deploy with mainnet init args**

Use the operator's mainnet principal (`6pwpo-d5iaw-mfjrn-owfb3-v4oz6-72woh-pc5t2-cwn73-zrzeq-4bjeh-tqe`) as both backend (we don't actually use it for ponzi_math_sol; this is just a placeholder for the init record's required field) and testAdmin. Or use the live backend canister ID (`5zxxg-tyaaa-aaaac-qeckq-cai`) for `backendPrincipal`.

```bash
dfx deploy ponzi_math_sol --network=ic --argument '(record { backendPrincipal = principal "5zxxg-tyaaa-aaaac-qeckq-cai"; testAdmin = principal "6pwpo-d5iaw-mfjrn-owfb3-v4oz6-72woh-pc5t2-cwn73-zrzeq-4bjeh-tqe"; solTreasuryAddress = "5EVdR6qcPuDqJb6W69fmcvTJjbEUGZqQtefEB8sK8QQ2"; solRpcProvider = variant { devnet }; keyId = record { algorithm = variant { ed25519 }; name = "key_1" } })'
```

Expected: "Installing code for canister ponzi_math_sol..." → "Deployed canisters."

- [ ] **Step 3: Add CycleOps as controller**

Run:
```bash
dfx canister --network=ic update-settings ponzi_math_sol --add-controller cpbhu-5iaaa-aaaad-aalta-cai
```
Expected: no output on success.

- [ ] **Step 4: Smoke-test the deploy**

Run:
```bash
dfx canister --network=ic call ponzi_math_sol selfTestBase58
dfx canister --network=ic call ponzi_math_sol selfTestSolTx
dfx canister --network=ic call ponzi_math_sol isBootstrapped
dfx canister --network=ic call ponzi_math_sol getSolTreasuryAddress
```
Expected:
- `selfTestBase58 → (true)`
- `selfTestSolTx → (record { ... })` with the expected byte values.
- `isBootstrapped → (false)` (Task 23 flips this).
- `getSolTreasuryAddress → ("5EVdR6qcPuDqJb6W69fmcvTJjbEUGZqQtefEB8sK8QQ2")`.

- [ ] **Step 5: Update canister_ids.json**

Run: `dfx canister id --network=ic ponzi_math_sol`
Take the returned ID and add to `canister_ids.json`:

```json
  "ponzi_math_sol": {
    "ic": "<the printed canister id>"
  },
```

Place it alphabetically after `"ponzi_math"`.

- [ ] **Step 6: Commit the canister-IDs change**

```bash
git add canister_ids.json
git commit -m "ops(ponzi_math_sol): record mainnet canister ID

Deployed to IC mainnet at <CANISTER_ID>, configured against Solana
devnet (variant { devnet } passed to sol-rpc on every call). Key
identity: key_1. CycleOps controller added.

Bootstrap (Task 23) pending."
```

---

## Task 23: Bootstrap the canister on mainnet — fund pool, derive nonce, initialize durable nonce

**Files:** none — operational task.

- [ ] **Step 1: Derive the pool address**

Run:
```bash
dfx canister --network=ic call ponzi_math_sol adminDerivePoolAddress
```
Expected: `(variant { Ok = "<base58 pubkey>" })`. Copy the address.

- [ ] **Step 2: Fund the pool address on Solana devnet**

In Phantom (switched to devnet) or via the Solana CLI:
```bash
solana airdrop 2 <POOL_ADDRESS> --url devnet
```

(2 SOL is plenty: 1.5M lamports for nonce-account rent + ~5000 lamports per tx fee + 0.5 SOL buffer for the operator's Series A seed and a few rounds of test deposits.)

Alternative: use https://faucet.solana.com or any devnet faucet.

- [ ] **Step 3: Verify the pool balance**

Run:
```bash
dfx canister --network=ic call ponzi_math_sol getCanisterSolBalance
```
Expected: a non-zero `Nat64` (something on the order of 2_000_000_000 = 2 SOL in lamports).

- [ ] **Step 4: Run bootstrap**

Run:
```bash
dfx canister --network=ic call ponzi_math_sol bootstrap
```
Expected: `(variant { Ok = "bootstrapped; nonce-account=...<NONCE_ADDR>... initial-nonce=...<NONCE_VALUE>... tx=<TX_SIG>" })`. This typically takes 10–30 seconds because of the multiple t-Schnorr signs + sol-rpc round-trips.

Failure modes and recovery:
- `getLatestBlockhash failed after 5 retries`: sol-rpc consensus flake. Wait a minute and retry — `bootstrapped := true` is set only on success, so retry is safe.
- `Pool address has only X lamports; needs ≥3,000,000`: send more SOL.
- `sendTransaction failed: ...`: check the explorer for the tx signature in the error. If the tx confirmed but `getAccountInfo` didn't read the nonce in time, the `bootstrapped := true` flag may not have been set. Call `adminRefreshNonce` to manually populate `lastNonceValue` (`adminRefreshNonce` will succeed once the account is on-chain), then call `bootstrap()` again — it'll see the existing nonce account, fail on createAccount (account already exists), but you can manually flip the flag via a one-off admin call. For M1 we accept this is a rough edge; document the recovery in `ponzi_math_sol/scripts/bootstrap-devnet.sh`.

- [ ] **Step 5: Verify post-bootstrap state**

```bash
dfx canister --network=ic call ponzi_math_sol isBootstrapped
dfx canister --network=ic call ponzi_math_sol getPoolAddress
dfx canister --network=ic call ponzi_math_sol getNonceAccountAddress
```
Expected:
- `isBootstrapped → (true)`.
- `getPoolAddress → (opt "<base58>")`.
- `getNonceAccountAddress → (opt "<base58>")`.

Cross-check on the Solana devnet explorer at https://explorer.solana.com/?cluster=devnet by searching for the pool address and the nonce account address. Both should exist; nonce account should show System Program ownership and an 80-byte data section.

- [ ] **Step 6: Register the operator's Series A seed**

The bootstrap consumed ~1.5M lamports for nonce rent + ~5000 for tx fee. The remaining pool balance is the operator's de-facto Series A seed. Record it:

```bash
# Replace 1.99 with whatever the actual remaining balance is in SOL.
dfx canister --network=ic call ponzi_math_sol adminRegisterSeriesABacker '(principal "6pwpo-d5iaw-mfjrn-owfb3-v4oz6-72woh-pc5t2-cwn73-zrzeq-4bjeh-tqe", 1.99 : float64)'
```
Expected: `(variant { Ok })`.

- [ ] **Step 7: No commit — operational only.**

Document the run in your operator notes; tasks 24 and beyond build on this state.

---

## Task 24: End-to-end devnet test — prepareSolDeposit → external send → detection → withdraw

**Files:** none — manual validation.

This is the M1 "works on devnet" milestone proof.

- [ ] **Step 1: Get a fresh deposit address as a normal user**

Operate as a user (any non-admin principal — use `dfx identity new tester1 && dfx identity use tester1`, then back to `dfx identity use CharlesPonzi` later):

```bash
dfx identity new tester1 || true
dfx identity use tester1
dfx canister --network=ic call ponzi_math_sol getOrCreateDepositAddress
```
Expected: `(variant { Ok = "<base58 deposit address>" })`. Copy this address.

- [ ] **Step 2: Prepare a deposit intent**

```bash
# 0.1 SOL = 100_000_000 lamports
dfx canister --network=ic call ponzi_math_sol prepareSolDeposit '(record { plan = variant { simple21Day }; expectedAmountLamports = 100_000_000 : nat64 })'
```
Expected: `(variant { Ok = record { intentId = <N>; depositAddress = "<same address>" } })`.

- [ ] **Step 3: Send devnet SOL to the deposit address**

Use Phantom (devnet) or the Solana CLI:

```bash
solana transfer <DEPOSIT_ADDRESS> 0.1 --url devnet --allow-unfunded-recipient
```

Wait for the tx to confirm on devnet (~5–10 seconds).

- [ ] **Step 4: Trigger detection**

Switch back to operator identity:
```bash
dfx identity use CharlesPonzi
dfx canister --network=ic call ponzi_math_sol runDepositDetection
```
Expected: `(variant { Ok = 1 })` — one new credit. If it returns `(variant { Ok = 0 })`, the tx isn't confirmed yet (wait 30s and retry) or didn't match the intent (verify the amount sent ≈ 0.1 SOL within 5%).

- [ ] **Step 5: Verify the game record + ledger**

```bash
dfx canister --network=ic call ponzi_math_sol getAllGames
```
Expected: a list containing a record with `player = principal "<tester1>"`, `plan = variant { simple21Day }`, `amount ≈ 0.1 : float64`, `isActive = true`.

```bash
dfx canister --network=ic call ponzi_math_sol getGeneralLedger
```
Expected: events including `#deposit` and `#coverChargeAccrued` for the new game.

- [ ] **Step 6: Verify the sweep landed on the pool**

```bash
dfx canister --network=ic call ponzi_math_sol getCanisterSolBalance
```
Expected: roughly previous balance + 0.1 SOL (minus 5000 lamport sweep fee).

The per-user deposit address balance, checked on the devnet explorer, should be ~0 (sweep emptied it).

- [ ] **Step 7: Wait, then withdraw**

If you want to verify a non-zero payout, wait a few hours (or modify the test to use createBackdatedGame… oh wait, we deleted that. Either patiently wait for earnings to accrue, or accept that for M1 we're primarily proving the wire works, not the payout numbers.)

For a minimal proof, just trigger a payout of accrued earnings on the simple plan after ≥1 day:

```bash
dfx identity use tester1
dfx canister --network=ic call ponzi_math_sol withdrawEarnings '(<GAME_ID> : nat)'
```

Expected: `(variant { Ok = <Float> })`. The tx signature isn't in the response — check the devnet explorer for the pool address's outbound tx of the indicated amount.

- [ ] **Step 8: Verify the pot decremented**

```bash
dfx canister --network=ic call ponzi_math_sol getPlatformStats
```
Expected: `totalWithdrawals` increased; `potBalance` decreased by the gross-earnings amount.

- [ ] **Step 9: Sweep cover charge to operator's Phantom**

```bash
dfx identity use CharlesPonzi
dfx canister --network=ic call ponzi_math_sol payManagementSol
```
Expected: `(variant { Ok = "<tx signature>" })`. Verify on devnet explorer that `5EVdR6qcPuDqJb6W69fmcvTJjbEUGZqQtefEB8sK8QQ2` received the lamports.

- [ ] **Step 10: No commit — manual validation only.**

If anything fails:
- Sweep tx fails to broadcast: nonce desync. Run `adminRefreshNonce`, then re-trigger.
- Detection finds 0 credits despite a confirmed SOL transfer: verify `lastSeenSignature` and `pendingIntents` via the admin queries; the intent may have expired (TTL is 10 min).
- Withdraw fails with "Caller has no deposit address": run `getOrCreateDepositAddress` first under the tester identity.

---

## Definition of Done for M1

All of the following must be true before declaring M1 shipped:

1. ✅ `ponzi_math_sol` canister deployed to IC mainnet at a recorded ID in `canister_ids.json`.
2. ✅ Canister configured for Solana **devnet** (init arg `solRpcProvider = variant { devnet }`, key id `key_1`).
3. ✅ CycleOps controller (`cpbhu-5iaaa-aaaad-aalta-cai`) added.
4. ✅ `selfTestBase58` returns `true`; `selfTestSolTx` returns the expected byte sequences.
5. ✅ `bootstrap()` ran successfully — `isBootstrapped` is `true`, `getPoolAddress` and `getNonceAccountAddress` both return populated values, and the nonce account is visible on the devnet explorer.
6. ✅ Operator's Series A seed recorded via `adminRegisterSeriesABacker`.
7. ✅ End-to-end devnet deposit flow works: `prepareSolDeposit` → external send → `runDepositDetection` credits a `GameRecord` → sweep to pool succeeds.
8. ✅ End-to-end devnet withdrawal works: `withdrawEarnings` produces a confirmed devnet tx visible on the explorer at the destination address.
9. ✅ `payManagementSol` successfully sweeps cover-charge accrual to `5EVdR6qcPuDqJb6W69fmcvTJjbEUGZqQtefEB8sK8QQ2`.
10. ✅ `ponzi_math/main.mo` is unmodified (verify with `git diff main:ponzi_math/main.mo HEAD:ponzi_math/main.mo`).
11. ✅ Existing canisters (`backend`, `ponzi_math`, `shenanigans`, `siws_provider`) are unaffected — verify by smoke-testing each via its usual entry point.

After M1 ships, return to the spec and write the M2 plan (shenanigans observer wiring to poll `ponzi_math_sol`).

---

## Notes and gotchas to keep in mind during execution

- **Math parity is sacred.** The single most likely M1 footgun is "improving" the carried-interest math or the compounding formulas during the fork. Don't. Same `Float.pow`, same `roundToEightDecimals`. The pre-fork commit hash for `ponzi_math/main.mo` is the canonical reference — if you reach for a refactor that would diff against it, stop and reconsider.
- **`sol-rpc` candid drift.** The hand-written `SolRpc.mo` actor type may diverge from the live candid as DFINITY ships sol-rpc updates. Motoko's structural typing only checks methods you actually call, so partial-coverage is safe — but if a call hits an `ic-cdk decode error: ...`, fetch the live candid via `dfx canister --network=ic metadata tghme-zyaaa-aaaar-qarca-cai candid:service > /tmp/sol-rpc.did` and re-derive the affected type.
- **Cycle attachment on signing.** `SolSigner.sign` attaches 30 G cycles per call (defensive buffer over the ~26 G real cost). Bootstrap signs twice; sweeps sign twice; payouts sign once. Plan for at least 5 G cycles per real-world tx, plus tx-fee SOL on the pool address. CycleOps top-up is wired post-deploy.
- **The "deposit before intent" race.** If a user sends SOL to their deposit address before calling `prepareSolDeposit`, detection logs the unmatched amount and admin clears via `adminCreditManualDeposit`. Document this in the operator runbook; don't try to autofix in M1.
- **`sweepCoverCharges` ledger event reuse.** `payManagementSol` records a `#coverChargeSwept` event with the existing fields — `toBackend = BACKEND_PRINCIPAL` (the live backend canister, just for the ledger audit) and `blockIndex = 0` (no Solana equivalent). The actual destination address and Solana tx signature are surfaced in the function's `Ok` return; if you want them in the ledger too, extend the event variant in a follow-up (don't muddy M1 with a schema change).
- **Phantom on devnet.** Bind Phantom to devnet via its in-app settings (Network → Solana → Devnet). All Phantom interactions in Task 24 expect devnet; mainnet Phantom will refuse to broadcast against the devnet RPC.
- **Solana fee accounting.** Every outbound tx burns ~5000 lamports of pool SOL on signature fees. Over time this is a small but real drag on the pot. The cover-charge accrual partly offsets this; operator should monitor.
- **Nonce account corruption.** The most dangerous post-bootstrap failure is a nonce desync — we sign with stale `lastNonceValue` and the tx fails with `BlockhashNotFound`. Recovery is `adminRefreshNonce` (reads on-chain, repopulates the cache) then retry. If the nonce account itself gets clobbered (somehow corrupted authority), recovery is more drastic — manual `createAccount` + `initializeNonceAccount` via the Solana CLI signed by a backed-up authority. Out of scope for M1 but flag the operator to keep notes.
- **CLAUDE.md naming policy.** Do NOT rename `exitToll`/`coverCharge` identifiers in `ponzi_math_sol/main.mo`. User-facing strings in any frontend wiring (M2+) use "Carried Interest" and "Front-End Load"; internal code keeps the old names.

## Skills to load during execution

- **`superpowers:subagent-driven-development`** or **`superpowers:executing-plans`** — the execution engine for this plan.
- **`superpowers:verification-before-completion`** — required before claiming each task is done.
- **`motoko`** — Motoko language pitfalls, persistent actor patterns, common errors (especially around `Array.init` mutability and `Float.pow` semantics).
- **`stable-memory`** — `ponzi_math_sol` adds new persistent state; check upgrade safety patterns.
- **`https-outcalls`** — sol-rpc passthroughs are technically inter-canister calls but conceptually mirror the outcalls cost model; understand cycle attachment.
- **`canister-security`** — anonymous-principal rejection, caller guards, reentrancy. `ponzi_math_sol` inherits these from the fork but a fresh read catches drift.
- **`audit-icp-cdp`** — defer to M3 (pre-mainnet) but mark it as a known follow-up. The `ponzi_math` audit findings already apply.
- **`migrating-motoko`** — when the deposit-intent or game-record schemas evolve in M2+, this is the upgrade-safe migration pattern.
- **`solana-dev`** — reference for the Solana side of the wire (transaction format, durable nonces, devnet faucets). Not for writing Solana programs.
