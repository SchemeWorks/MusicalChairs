---
name: icp-solana-motoko
description: >-
  Field guide to the seam between an Internet Computer (ICP) canister backend and
  Solana — the gotchas neither the IC chain-fusion docs nor the generic Solana
  dev guides cover: calling the SOL RPC canister (Multi*Result consensus
  variants, candid decode-traps, broadcast-then-trap), webhook-free deposit
  detection (timer + cursor + intent TTL), threshold-Ed25519 withdrawals, the
  browser-side boundary (public-RPC 403s, wallet-handoff sends, the CSP that
  blocks Solana fetches), SIWS sign-in, and the devnet→mainnet cutover.
  IMPORTANT: for MOTOKO canisters only — does NOT cover Rust. Use it whenever
  building, debugging, or planning any Solana integration on ICP in Motoko —
  deposits, withdrawals, SOL RPC calls, SIWS auth, chain fusion, or a
  devnet→mainnet cutover — even if the specific gotcha isn't named. Reach for it
  especially when a Solana RPC call traps on decode, SOL deposits don't
  auto-credit, a one-click deposit spins forever, or a browser fetch to a Solana
  endpoint 403s.
---

# ICP ↔ Solana Integration — Field Guide (Motoko)

> **Scope: Motoko canisters only. This skill does NOT cover Rust canisters.**
> The mental model is language-agnostic and still useful for Rust, but every code
> pattern is Motoko, and the candid-decode and inline-migration gotchas are
> described in Motoko terms.

The official **IC chain-fusion docs** cover the IC half (threshold Ed25519, the
SOL RPC canister, HTTPS-outcall economics). The generic **Solana dev guides**
cover the Solana half (`@solana/kit` / `@solana/web3.js`, wallet-standard).
**Every expensive bug lives at the seam between them — and most of it is the
BROWSER-side seam, which neither set of docs covers and which only bites on
mainnet.** This skill is that seam, hardened by a real devnet → mainnet-beta
cutover.

## The four-actor model (read first — most frontend bugs are a confusion here)

| Actor | Does | Never |
|-------|------|-------|
| **User's wallet (Phantom)** | SIGNS + BROADCASTS the user's txs (`signAndSendTransaction`) | — |
| **RPC provider** (domain-locked Helius/QuickNode) | Browser-side READS (`getBalance`) + the **recent blockhash** the wallet needs | submit the user's tx (wallet's job) |
| **Your canister** (`*_sol`) | Server-side: deposit detection, threshold-Ed25519 withdrawals, sweeps; pays cycles to call the SOL RPC canister | — |
| **DFINITY SOL RPC canister** | the canister's gateway to Solana (consensus over providers) | be called **from the browser** (needs cycles + update call) |
| **Public RPC** (`api.mainnet-beta…`) | nothing you should depend on | be called from the browser — it **403s** |

Crystallized rule: **wallet for signing + broadcasting; an RPC provider for reads
and the blockhash; your canister for server-side chain ops.** Two things the
browser physically cannot do: call the public mainnet RPC (403) or call the SOL
RPC canister (ingress carries no cycles). And: **there are no webhooks** — every
"did the money land?" is a PULL (a timer in your canister polling the SOL RPC
canister).

## The seam at a glance

Each row is a trap we paid for. `Pt` maps to the Part / section number in
**`references/playbook.md`** — read that section for the full explanation + code.

| # | The trap | Do this instead | Pt |
|---|----------|-----------------|----|
| 1 | SOL RPC returns 3-arg calls + `#Consistent`/`#Inconsistent` `Multi*Result` | pull the LIVE candid before hand-typing the actor | 1 |
| 2 | a `sendTransaction` response decode traps AFTER the tx broadcast → nonce desync | exact types; mutate nonce/state only after a good decode; decode-fail = "maybe sent" | 1 |
| 3 | `Nat`/`Int` for a `nat64`/`int64` field traps decode (even on untouched arms) | mirror exact widths (lamports `Nat64`, offsets `Int`) | 1 |
| 4 | `getLatestBlockhash` never reaches multi-provider consensus (per-slot value) | admin/override path: fetch one fresh blockhash, pass it straight in | 1 |
| 5 | deposit detection — no webhooks exist | timer + per-address cursor + generous TTL (hrs) + at-least-once; reverse to oldest-first | 2 |
| 6 | frontend waiting on the chain | intent pattern: prepare → user sends → timer reconciles → poll your canister | 2 |
| 7 | a new deposit TYPE silently never auto-detects | wire it into BOTH the poke gate AND the timer scan-set; match the other path's accounting | 2 |
| 8 | detection isn't instant and can stall | ship `adminCreditManualDeposit` + an admin getter for open intents | 2 |
| 9 | signing / withdrawals | threshold Ed25519 + durable nonce; pay out to the caller's OWN validated address | 3 |
| 10 | `IC0406`: credits succeed but sweeps fail | retry (transient sol-rpc) vs cycles (freeze threshold); size CycleOps for sol-rpc burn | 3 |
| 11 | devnet→mainnet flip is 3 changes; missing the frontend pair fails SILENTLY | flip canister arg + `SOLANA_RPC_ENDPOINT` + CSP together; kill stale devnet banners | 4 |
| 12 | browser can't use the public mainnet RPC (403) or the SOL RPC canister | use a real RPC provider for browser-side Solana | 4 |
| 13 | app-submitted sends 403 on mainnet | wallet `signAndSendTransaction` — but YOU still fetch+set the blockhash; no `confirmTransaction` (wss) | 4 |
| 14 | browser reads (`getBalance`) 403 on the public RPC | domain-locked provider key (safe in bundle); mind the Vite `envDir`/`define` traps | 4 |
| 15 | CSP `connect-src` blocks Solana by default | add host(s) in `.ic-assets.json`; verify the LIVE header by curl | 4 |
| 16 | one-click flakes (rate-limit) | always keep a manual-deposit fallback (Phantom uses its own RPC) | 4 |
| 17 | SIWS message + hardware wallets | pure-ASCII byte-exact message; Ledger can't sign it (`0x6a81`) → catch + steer to a hot wallet | 4 |
| 18 | a SOL session needs more than a flipped deposit card | denomination-branch EVERY page (stats/ledger/lists/copy), not just the widget | 4 |
| 19 | frontend declarations go stale | hand-copy `.did`/`.did.d.ts`/`.did.js` after candid changes (not `index.ts`); alias SOL types | 4 |
| 20 | a new variant case silently drifts an old migration → M0170 | enumerate the FULL current variant set in each `runVN` | 5 |
| 21 | upgrading a canister with a live timer/observer | stop → upgrade(keep) → start; unwire migration; verify FE deploy by live bundle hash | 5 |
| 22 | devnet→mainnet is a REINSTALL, not an upgrade — and skips `postupgrade` | redo timer-arm/seed/cursor-prime MANUALLY; use UPGRADE for feature adds | 5 |
| 23 | agent-supplied money addresses get blocked | surface treasury/pool address char-by-char before any send | 5 |
| 24 | the cutover is irreversible | runbook it (preflight → gate → reinstall → seed → bootstrap → observer reset → FE flip → smoke test) | 5 |
| 25 | a live frontend can be ahead of `origin/main` | sort the true mainline before a PR; keep secrets in gitignored `.env.local` | 5 |

## Recommended build order

Sequencing this way proves one risky thing per milestone — but note the
browser-side traps (Part 4) mostly stay invisible until M3, because devnet's
public RPC is permissive:

| Milestone | Scope | Proves |
|-----------|-------|--------|
| **M1** | Read-only on **devnet**: derive pool address, read balances, bootstrap nonce | The SOL-RPC seam (1–4) + the blockhash escape hatch |
| **M2** | Deposits + crediting/minting on **devnet**: intent + timer + observer; browser send via wallet, reads via provider | Detection (5–8) and the browser boundary (11–19) — wire it right now, even though the 403s won't bite until mainnet |
| **M3** | **Mainnet cutover** — a REINSTALL, irreversible; runbook it | Cutover discipline (20–25) |

## Pre-flight checklist

```
CANISTER ↔ SOL-RPC
[ ] Actor types from LIVE candid; exact int widths (Nat64/Int, never bare Nat)
[ ] sendTransaction tolerates #Inconsistent (first #Ok wins); reads strict
[ ] getLatestBlockhash has an admin/override path (per-slot value can't reach consensus)
[ ] state/nonce mutated only after good decode; decode-fail = "maybe sent"

DEPOSITS
[ ] timer (~60s) + per-address cursor + generous TTL (hours) + at-least-once credit
[ ] results reversed to oldest-first; cursor advances only after credit/skip
[ ] EACH deposit type wired into BOTH the poke gate AND the timer scan-set
[ ] adminCreditManualDeposit backstop + admin getter for open intents

SIGNING
[ ] threshold Ed25519; durable nonce bootstrapped via the blockhash escape hatch
[ ] withdrawals take the caller's own validated address
[ ] CycleOps threshold sized for sol-rpc burn (not idle baseline)

BROWSER (the part that bites on mainnet)
[ ] cluster flip = canister arg + frontend SOLANA_RPC_ENDPOINT + CSP host, together
[ ] sends via wallet signAndSendTransaction; app sets the blockhash itself; no app submit
[ ] no confirmTransaction (wss blocked); no public mainnet RPC from browser; no sol-rpc canister from browser
[ ] reads/blockhash via a DOMAIN-LOCKED provider key (Allowed Domains = your origins)
[ ] Vite env: right envDir/define pattern; key embedded in the actual dist; .env.local gitignored
[ ] live CSP verified by curl after deploy
[ ] manual-deposit fallback present
[ ] SIWS message pure ASCII + under length; Ledger error caught and steered to hot wallet
[ ] every page denomination-branched, not just the deposit card
[ ] declarations hand-synced after candid changes (not index.ts)

DEPLOY / CUTOVER
[ ] migration enumerates full current variant set; unwired after
[ ] timer/observer canisters: stop → upgrade(keep) → start; operator-gated
[ ] devnet→mainnet = REINSTALL (wipes state) and redo postupgrade work MANUALLY
[ ] money addresses verified character-by-character before any send
[ ] observer cursors reset after reinstall, before first deposit
[ ] frontend deploy verified by live bundle hash; repo mainline sorted before PR
```

## Where to go next

- **`references/playbook.md`** — the full field guide: the four-actor model, then
  one section per row above (numbered to match the `Pt`/`#` columns), with Motoko
  and TypeScript code. Read the section matching whatever trap you're hitting; read
  the whole thing once if you're starting an integration from scratch.
