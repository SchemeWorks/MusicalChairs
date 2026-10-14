# Native Solana M11 handoff — frontend native adapter behind the existing SOL panels (2026-10-14)

Branch `codex/native-solana-production-core`; cards IMPL-46a (encoding fixture), 46b (adapter), 46c (panels + dev-only local-cluster mode), 46d (local cluster, live test, browser verification). Receipt: [`evidence/2026-10-14-m11-frontend.json`](../solana/evidence/2026-10-14-m11-frontend.json). Local cluster recipe: [`docs/solana/local-cluster.md`](../solana/local-cluster.md).

## Commits (oldest first)

| commit | what |
| --- | --- |
| `54f3ff8` | ci: explicit CARGO_HOME for the IDL check; docs: M10 disclosures (Codex delta review); test(harness): instruction-encoding fixture (IMPL-46a) |
| `73fd37d` | tooling(solana): local-cluster bootstrap binary for the M11 browser verification (IMPL-46d) |
| `2faf46d` | docs(evidence): M10 Linux CI evidence and Codex acceptance recorded; CARGO_HOME finding corrected; local-cluster recipe |
| `12e63b8` | tooling(solana): local_cluster status subcommand (decoded Pool / Ops / Ticket summary of the running local cluster) |
| `9632a54` | test(frontend): live local-cluster adapter test (IMPL-46d, gated on MC_LOCAL_RPC) |
| `9703b7c` | feat(frontend): native Solana adapter — IDL-driven Borsh codecs, PDAs, readers, 36 instruction builders, status ladder, NativeClient, React Query hooks (IMPL-46b) |
| `daa9f12` | test(frontend)+tooling(solana): closing-world fixture export and the live closing-path test on the local cluster (IMPL-46d) |
| `1d56874` | test(frontend): live local-cluster tests wait for 'confirmed' by name and for real finality on the money-moving transactions; kanban IMPL-46b/46c/46d statuses |
| `e7e3cfe` | fix(frontend): native adapter — cluster identity, pre-sign simulation, batch cap with page-edge alignment and one-row fallback, signed fee-payer binding (IMPL-46b, Codex R1–R4) |
| `417948d` | fix(frontend): native adapter — retry after a cluster switch fails closed; read-only reconcile on the origin cluster (IMPL-46b, Codex selected-cluster case) |
| `1f93263` | feat(frontend): native Solana adapter behind existing SOL panels; gross projections; correct split text; dev-only local-cluster wallet (IMPL-46c) |

## What M11 delivered

1. **Instruction-encoding fixture** (`solana/fixtures/ix-encodings.json`, 39 samples over all 36 instructions) generated and pinned from the Rust side, discriminators cross-checked against the committed IDL.
2. **Adapter** `frontend/src/solana/native/` + `hooks/useNativeSolana.ts`: hand-written Borsh codecs (no new dependency — no new dependency: hand-written Borsh codecs from the IDL; @coral-xyz/anchor not added; no @rollup/rollup-darwin-*; package.json/package-lock.json untouched), every fixture sample byte-exact, account decoders reproducing every Rust SPACE, PDA derivations checked against the IDL seed lists, builders for all 36 instructions incl. keeper remaining-account layouts and the opt-in inline payout, status ladder without wss, `NativeClient` with the wallet/session pubkey guard and unknown-outcome reconciliation (signature + nonce kept, `retry()` re-polls and re-broadcasts identical bytes), `advancePhase`, selectors that never merge legacy and native positions. Adapter tests: {'files': 16, 'tests': 150}.
3. **Panels** (IMPL-46c, layout preserved): `SolInvestPanel` (native open for Simple/Compound15/Compound30, gross projection, Front-End Load line, status ladder with signature + nonce, "Check status" re-polls, "Try again" only after failed/expired), `GameTracking` (native cards with owed / Carried Interest or Jackpot Fee tier / net; Withdraw, Settle at maturity, Close when owed is 0; minimum receive = quote −1 %; Advanced "Max work contribution (SOL)" default 0; allocation run and round-closing "resume processing" one transaction per click; closing payouts provision the CloseRecord first), `SolBackerPanel` / `HouseDashboard` (native Series A funding gated on `publicSeriesAEnabled`, entitlement 124 %, combined A+B claim, 35/45/20 copy, legacy Chain-Fusion section per DR-22), `GameStatusBar` (AUM vs Legacy AUM, "n native · m legacy"), `WalletDropdown` (cluster chip from the endpoint host; dev-only "Local cluster wallet (dev)" chip with burner or imported secret, loopback-guarded, compiled out of production builds — verified by building), `DocsPage` labels. Rail selection by `useNativeAvailability` (Pool read on the verified cluster: native first with legacy sections; `unavailable` → legacy only, which is what the default mainnet endpoint yields until a mainnet domain exists). Panels use the cluster-scoped readers and query keys; `describeNativeError` maps the simulation / cluster / fee-payer error types. 20 new test files (402 tests total), `NativePanels.test.tsx` renders with react-dom/server (no jsdom added).

4. **Live proof against the real program** on a local `solana-test-validator` (solana-test-validator 3.1.0 (src:bd8c57e7) from the official macOS arm64 agave v3.1.0-beta.0 release (sha de9be527…), owned temp dir; program ELF 8f97e83d… as an upgradeable program with the G vault authority; Squads stand-in; captured ORAO ELF; genesis via local_cluster): flows test {'files': 1, 'tests': 2} in 27.97s (transform 108ms, setup 0ms, import 173ms, tests 27.70s, environment 0ms) — open 1 SOL Simple (pot +960,000,000, management +40,000,000); dust payout → Carried Interest seeds, no ticket; Series A funding 0.05 SOL by a second wallet → entitlement 62,000,000, registry_len 1; ticket-bearing payout → advancePhase until IDLE, pending_toll 0; claim_backer → funded_claims 0; closing test {'files': 1, 'tests': 1} in 33.04s (transform 107ms, setup 0ms, import 170ms, tests 32.66s, environment 0ms) — provision CloseRecord; insolvent withdraw_simple → pool.currentCloseId, pot 0, owner paid; candidate pages both sets; build_candidates until sealed; request_round_draw → local oracle fulfils → capture (DRAW_STATE.Captured); apply_promotion; finalize_round → round 2, close finalized (finality awaited for the payout and the finalize); participant page round 2 → open_position in round 2 (export-closing-fixture world (1-SOL Simple position opened at wall time − 10 days, pot 0.1 SOL, ops 5 SOL, localnet domain), corrected adapter (R1–R4 + cluster-switch guard)).
5. **Browser verification** (built-in browser, dev-only local-cluster wallet): Built-in browser (Claude desktop pane, 1024×768) against the Vite dev server (`npx vite --port 5180`, `.env.local`: `VITE_SOLANA_RPC_URL=http://127.0.0.1:8899`, `VITE_NATIVE_LOCAL_CLUSTER=1`) and a fresh closing-fixture cluster (`local_cluster export-closing-fixture`; owner secret imported through the dev-only "Local cluster wallet (dev)" chip — a synthetic fixture keypair, loopback only). Run 1 (screens/run1/01–24):

1. Landing shows the LOCALNET chip; "Import a wallet secret" accepts the exported owner key; the burner adapter airdrops 5 SOL; the header shows balance 5.0000, 1 native position, AUM 0.1000 SOL. The Profit Center lists the fixture position: 1.0000 SOL deposited, live earnings 1.1007 SOL, Carried Interest 7.5 % (day 10 of 21), quote "0.1000 SOL gross, −0.0075 Carried Interest, you receive 0.0925 SOL. This payout closes the round." (pot-capped).
2. Deposit (Invest → Starter Package, 0.5 SOL): Front-End Load (4 %) −0.0200 SOL, deposit into AUM 0.4800 SOL, daily earnings 0.0550 SOL/day (= 11 % of GROSS; the legacy figure would be 0.0528), expected 2.31×. "Deposit from local wallet" → Confirmed (signature 2yugCuz7…, nonce 9235486594…) → Finalized ~13 s later; 2 native positions, AUM 0.58 SOL.
3. Solvent payout on the new position (Withdraw → confirm sheet: owed, Carried Interest 12 %, net, minimum receive = quote −1 %): Confirmed (hkh9sAps…); no allocation run (empty registry → the dust Carried Interest seeded).
4. Seed Round (Backer) tab: 0.2 SOL "Back with local wallet" → Confirmed; entitlement 0.2480 SOL (124 %); 1 open Backer; split copy 35 % senior / 45 % pro rata Series A / 20 % equal Series B; legacy Chain-Fusion Backers in their own section.
5. Ticket-bearing payout on the new position → "Allocation run in progress — resume processing: step 1 of 6 (Discover). One transaction per click."; other payouts blocked with "Resume processing first"; six clicks (Discover → Senior → Seal → Allocate → Prune → Commit), each "The allocation step landed", run finished.
6. Backer tab: "0.00000252 SOL ready to claim" → Claim Repayment → Confirmed (2R23bhJy…); claimable 0.0000.
7. Closing payout on the fixture position: confirm sheet "Owed 1.1011 SOL, paid from AUM 0.7799 SOL, Carried Interest (7.5 %) −0.0585, you receive 0.7214 SOL. This payout closes the round … starts the Emergency Equity Conversion … First this sends a small transaction to prepare the round's close record" → Confirm and close the round → CloseRecord provisioned then the payout landed; header AUM 0.0000; "Round closing — resume processing", promotion window 9:51.
8. Resume processing, one click each: closing allocation run steps 1–6 → "Provisioning candidate page 1 of 1 for the preferred set" → fallback page → build candidates → request randomness ("The randomness request landed") → within seconds "The provider has answered. Capturing the randomness." (local oracle) → Capture randomness → "Apply Emergency Equity Conversion" → "The randomized selection is settled. Finalizing carries the seed into AUM and opens the next round." → Finalize round → "The finalization landed"; no open native positions; "Closed native positions (2)"; AUM 0.0293 SOL (the seed carry). On-chain after: round_id 2, registry_len 2 (the converted owner as Series B + the Series A Backer), next_close_id 2, pot 29,253,572, funded_claims 29,247,229.
9. "Open a Position in the new round" → Invest → 0.3 SOL → "A required account is not provisioned yet. Try again shortly. Nothing was signed." — the pre-sign simulation refused `open_position` because round 2's participant page 0 is not provisioned and the UI has no provisioning step for it (finding F1, assigned to IMPL-46c together with F2: the one-step "Finalize and open" box produced no status or error; F3: the "open in the new round" CTA appears after a non-closing commit). The headless closing test provisions that page explicitly and reaches round 2 with a new position.

Observed protections: every rejection reached the UI without a wallet prompt (simulation first); signature and nonce stay visible with "Check status" re-polling the same signature; the cluster chip reads "localnet"; legacy and native figures never summed.

Run 2 (after the IMPL-46c fixes; screens/run2/01–13), fresh closing-fixture cluster, session restored from sessionStorage (the restore path does not top up — the coordinator airdropped 5 SOL through the local faucet): deposit 0.5 SOL → Confirmed; Series A 0.2 SOL → Confirmed (entitlement 0.2480); ticket-bearing payout → six "Resume processing" clicks → "The allocation run finished. Payouts are available again." with no call to action (F3 fixed); claim → Confirmed; closing payout (CloseRecord prepared, then the payout; balance 10.01 SOL) → closing run (six clicks) → candidate pages (preferred, fallback) → build ("The candidates are sealed. Requesting randomness from the provider.") → request → "The provider has answered. Capturing the randomness." → capture → "Apply Emergency Equity Conversion" → "Finalize round" → "The round is finalized. The next round is open." with the CTA (only now) → Invest → 0.3 SOL → "Preparing this round's participant page landed" (F1 fixed: the UI provisions round 2's page 0 caller-funded, waits for it, then sends the open) → "Confirmed: your deposit landed" → Profit Center shows the round-2 position (0.3000 SOL, 12 % Carried Interest, day 0 of 21), "Closed native positions (2)", AUM 0.3173 SOL (0.0293 seed carry + 0.288). The one-step "Finalize and open" box is gone (F2: impossible on-chain, two-step path kept). Every step one wallet-free local signature; every state change visible with its signature.


Screenshots:
- `run1/01-landing-local-wallet-chip.jpg`
- `run1/02-import-wallet-secret.jpg`
- `run1/03-connected-profit-center-fixture-position.jpg`
- `run1/04-closing-payout-quote-day10.jpg`
- `run1/05-invest-plans.jpg`
- `run1/06-deposit-gross-projection.jpg`
- `run1/07-deposit-confirmed-signature-nonce.jpg`
- `run1/08-two-native-positions.jpg`
- `run1/09-withdraw-confirm-sheet.jpg`
- `run1/10-payout-confirmed.jpg`
- `run1/11-series-a-funded-entitlement-124.jpg`
- `run1/12-allocation-run-step1.jpg`
- `run1/13-allocation-run-step3.jpg`
- `run1/14-allocation-run-committed.jpg`
- `run1/15-backer-claimable.jpg`
- `run1/16-claim-confirmed.jpg`
- `run1/17-closing-confirm-sheet-eec.jpg`
- `run1/18-round-closing-resume.jpg`
- `run1/19-closing-run-committed-candidate-pages.jpg`
- `run1/20-randomness-answered.jpg`
- `run1/21-capture-landed-apply-eec.jpg`
- `run1/22-promotion-settled-finalize.jpg`
- `run1/23-finalized-open-new-round.jpg`
- `run1/24-open-round2-page-not-provisioned-nothing-signed.jpg`
- `run2/01-session-restored-fixture-position.jpg`
- `run2/02-deposit-confirmed.jpg`
- `run2/03-series-a-funded.jpg`
- `run2/04-allocation-run-finished-no-cta-F3.jpg`
- `run2/05-claim-confirmed.jpg`
- `run2/06-closing-confirm-sheet.jpg`
- `run2/07-round-closing-resume.jpg`
- `run2/08-capture-landed-apply-eec.jpg`
- `run2/09-finalized-next-round-open-cta.jpg`
- `run2/10-round2-open-page-preparation-F1.jpg`
- `run2/11-round2-page-prepared-deposit-confirmed.jpg`
- `run2/12-profit-center-round2.jpg`
- `run2/13-round2-position-card.jpg`

`npm test` final: {'files': 32, 'tests': 434} (adapter alone after IMPL-46b: {'files': 16, 'tests': 150}); `npm run build`: ok. On-chain state after browser run 2:

```
slot 776
pool: round_id 2 active_config 1 participant_count 1 registry_len 2 snapshot_pages 1 next_close_id 2 next_ticket 3 active_ticket 0
      pot 317250747 seed 0 management 72000000 pending_toll 0 funded_claims 29248757 excess 0  (vault lamports 419453024)
ops:  free 4985223200 locked 0 unallocated_excess 0  (cash lamports 4986114080)
ticket: id 2 phase 6
```

## Keeper / integration notes carried from M10

Ticket completion is the keeper's first priority (any toll ≥ 5 lamports starts a run); batches aligned with participant AND candidate page edges with the one-row fallback; preflight `credit_is_deliverable` on every user-chosen destination (claim_backer has no on-chain preflight, M9-F1); G staging preflight (effective round within the intended horizon, version == active + 1, terms validated off-chain).

## Caveats

- local validator only; no public network, live wallet, deployment or activation
- the closing / promotion path needs an insolvent payout (days of real accrual at 11 %/day) and is not driven on the live validator — it stays proven in LiteSVM (M7/M8) with clock warps
- the plan's Agave 2.3.13 validator pin is superseded by the M0-pinned 3.1.0-beta.0 toolchain (recorded in latest-build.json)
- hook tests are typecheck-only (no jsdom / testing-library added)
- The frontend's legacy ponzi_math_sol flows (deposit intents, withdrawals, Backer claims through the ICP canister) are untouched and listed separately from native positions; nothing in the local-cluster mode signs or sends to an ICP canister.

## Proof level

Adapter and panels: vitest + tsc + `npm run build`; adapter against the real SBF program: the gated live test on the pinned local validator (this machine); browser flows: screenshots on the same cluster. No public network, live wallet, deployment, activation or merge.

## Staffing

Coordinator: fixture, local-cluster tooling, live test, browser verification, receipts, graph. Two Sonnet frontend workers (46b adapter, 46c panels), coordinator-verified.

## Next

M12 ICP bridge canister and PP exactly-once (plan L678–689), M13 keepers and monitoring (code only). Stop before M14/M15 public-network actions.
