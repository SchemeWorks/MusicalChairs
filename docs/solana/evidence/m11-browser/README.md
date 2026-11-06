# M11 browser evidence — native Solana panels on a local cluster

Durable copies of the built-in-browser captures behind the M11 handoff
(`docs/coordination/native-solana-m11-handoff-20261014.md`) and receipt
(`docs/solana/evidence/2026-10-14-m11-frontend.json`). Root: this directory. Every image is a
JPEG of the Claude desktop browser pane (1024×768 viewport, 800×600 capture) against the Vite dev
server (`npx vite --port 5180`, local-cluster mode) and a `solana-test-validator` 3.1.0 started
from `local_cluster export-closing-fixture` (program ELF 8f97e83d…, Squads stand-in, captured
ORAO ELF). Hashes: `MANIFEST.sha256` (`shasum -a 256 -c MANIFEST.sha256` from this directory).

**Omitted on purpose:** `run1/02-import-wallet-secret.jpg` (the wallet-secret import screen;
sha256 `88825c567c53cab6cdfaabb42c8b5a952bbaa2d10e88fa8f4a20973f94131f19`, kept out of the
repository because it shows key pixels of the synthetic fixture owner).

## Scope (read before drawing conclusions)

- Local validator only. No public network, live wallet, funds, deployment, activation or merge.
- The closing world is a **synthetic fixture**: the 1-SOL position was opened in LiteSVM under a
  Clock sysvar set to wall time − 10 days, the pot was set to 0.1 SOL and operations funded, and
  the 20 accounts were exported as validator genesis accounts. Everything after genesis (payout,
  closing run, candidate pages, randomness request, capture, promotion, finalize, round-2 open) is
  real program execution on the validator. The randomness was fulfilled by the **local oracle**
  (`local_cluster oracle`, the synthetic fulfil authority), so these runs prove the program's
  draw pipeline end to end on the real ORAO executable but are **not** a live ORAO-network proof.
- Local-cluster mode is dev-only (`import.meta.env.DEV && VITE_NATIVE_LOCAL_CLUSTER=1`, compiled
  out of production builds). The `loading` and `unavailable` captures in run 3 come from the same
  rail components production uses (`railOf` → `NativeRailVerifying` / `LegacyRailBanner`), with
  one dev-only difference: in local mode SolInvestPanel and SolBackerPanel render a "native program
  isn't available" card where production renders the legacy form under the banner. The production
  legacy form branch is proven by `frontend/src/components/LegacyRail.test.tsx` (static renders of
  every panel in loading / unavailable / native / local-mode), not by a browser capture: a non-local
  SIWS session would have to be registered against the mainnet backend canister, which is a
  public-network action outside this scope.
- Run 3's `unknown` outcome was produced with a dev-only loopback proxy
  (`127.0.0.1:8898 → 8899`) that forwarded `sendTransaction` to the validator once and answered the
  browser with HTTP 502 while returning null signature statuses, then was switched to transparent.
  The proxy log shows exactly one forwarded `sendTransaction`; the status card resolved on the
  original signature with no second send.

## Pairs (plan M11 step 3: "screenshots per major screen against the current baseline")

| Screen | Baseline / before | Native / after |
| --- | --- | --- |
| Profit Center rail | run3/12 legacy rail (`Legacy (Chain Fusion)` banner, legacy-only tally caption, no native UI) | run3/05–06, run1/03 native tally + position card; run3/14 verifying state (no Withdraw) |
| Seed Round rail | run3/17 legacy banner + local card + `Legacy (Chain Fusion) backers` | run3/10–11 native Series A form + native section + labeled legacy section; run3/13 verifying (no form) |
| Invest rail | run3/16 unavailable card (local mode) | run3/02 gross projection + Front-End Load; run3/15 verifying (no form) |
| Deposit outcome | run3/07 `Outcome unknown` (Check status + guarded resend) | run3/08 checking (read-only) → run3/09 finalized, same signature/nonce, no resend |
| Round-2 open (F1) | run1/24 "A required account is not provisioned yet. Nothing was signed." | run2/10 page preparation landed → run2/11 deposit confirmed → run2/12–13 round-2 position |
| Finalize CTA (F2/F3) | run1/23 one-step "Finalize and open" box; run1/14 CTA after a non-closing commit | run2/09 two-step finalize with the CTA only after finalization; run2/04 no CTA after a non-closing run |
| Closing path | run1/17 confirm sheet (CloseRecord preparation) → run1/18–22 resume → capture → promotion | run2/06–08 same path after the fixes |

## run1 — first full pass (24 captures, 23 published)

01 landing with the LOCALNET chip · 03 fixture position (1.0000 SOL, day 10, pot-capped closing
quote) · 04 closing payout quote · 05 plans · 06 deposit 0.5 SOL gross projection (Front-End Load
−0.0200, 0.0550 SOL/day on GROSS) · 07 deposit confirmed (signature + nonce) · 08 two native
positions · 09 withdraw confirm sheet · 10 payout confirmed · 11 Series A funded, entitlement 124 %
· 12–14 allocation run steps 1/3/committed · 15 backer claimable · 16 claim confirmed · 17 closing
confirm sheet (Emergency Equity Conversion) · 18 round closing — resume · 19 closing run committed
+ candidate pages · 20 randomness answered (local oracle) · 21 capture landed → apply promotion ·
22 promotion settled → finalize · 23 finalized, open-new-round CTA · 24 round-2 open refused
(page not provisioned; nothing signed) = finding F1.

## run2 — after the IMPL-46c fixes (13 captures)

01 session restored · 02 deposit confirmed · 03 Series A funded · 04 allocation run finished, no
CTA (F3) · 05 claim confirmed · 06 closing confirm sheet · 07 round closing — resume · 08 capture
landed → apply promotion · 09 finalized, next-round CTA · 10 round-2 page preparation (F1) · 11
page prepared, deposit confirmed · 12 Profit Center round 2 · 13 round-2 position card.

## run3 — Codex P1/P2 re-check on 89f4660 + copy fix (17 captures)

Fresh fixture cluster, burner wallet (no secret import). 01 burner connected, no native
positions, tally caption names the legacy section · 02 0.5 SOL gross projection · 03 confirmed,
"Sent on localnet (127.0.0.1:8899)" · 04 finalized, same signature/nonce · 05–06 Profit Center
native tally + position card · 07 **P2** outcome unknown with "Check status" and "Resend the same
transaction" · 08 checking (read-only reconcile on the origin) · 09 resolved Finalized on the same
signature, no resend · 10 Seed Round native form · 11 native section + `Legacy (Chain Fusion)
backers` · 12 **P1** Profit Center legacy rail banner (native unavailable) · 13 Seed Round verifying
(no form) · 14 Profit Center verifying ("legacy (Chain Fusion) figures until it finishes", no
Withdraw) · 15 Invest verifying (no deposit form) · 16 Invest unavailable (local-mode card) · 17
Seed Round legacy banner + local card + legacy backers.
