# Local cluster for browser verification (M11, IMPL-46d)

Everything here runs on the developer machine against a `solana-test-validator`. No public
network, live wallet, deployment or activation is involved; the Squads multisigs are the
harness fixtures signed through the vault stand-in, ORAO is the captured mainnet ELF with a
synthetic fulfil authority, and the program is the locally built ELF pinned in
`docs/solana/evidence/latest-build.json`.

**Solana note (vs ICP):** on ICP a local replica (`dfx start`) hosts the canister with the same
code path as mainnet. Here the *validator* is the runtime (loader, rent, compute budget, sysvars)
and the program is loaded at its declared address with `--upgradeable-program`, so the
`ProgramData.upgrade_authority == G vault` check that genesis performs holds exactly as it will
on a cluster. LiteSVM (the harness) skips some runtime rules (rent-state checks on data-less
accounts); the validator applies all of them, which is why the browser flows run here.

## Tools (cached pinned copies, no global install)

The original M11 copies lived in `/private/tmp/mc-solana-runtime-tools-20261013/`, which no longer exists. The same pinned
tools are cached under `~/.cache/mc-solana-tools/` (re-checked 2026-10-30 for DEMO-LOCAL).

| tool | where | pin |
| --- | --- | --- |
| `solana-test-validator`, `solana` 3.1.0 (`src:bd8c57e7`) | `~/.cache/mc-solana-tools/agave/solana-release/bin/` | official `solana-release-aarch64-apple-darwin.tar.bz2` of agave v3.1.0-beta.0, sha in `latest-build.json` (`toolchain.archives`) |
| program ELF | `solana/programs/musical_chairs/artifacts/musical_chairs_program.so` (gitignored) | `latest-build.json` artifact (`0c88d4e7…` since the M14-5A re-pin; id `7aSvuR19…`) |
| Squads vault stand-in | `solana/tests/financial-runtime/standin/squads-vault-standin/artifacts/squads_vault_standin.so` | `latest-build.json` `standin_artifact_sha256` (`eb78e77f…`) |
| ORAO VRF | `solana/probes/runtime/fixtures/orao-callback-mainnet-finalized.so` at `VRFCBePmGTpZ234BhbzNNzmyg39Rgdd6VgdfhHwKypU` | probes provenance (`6d180b1e…`) |
| bootstrap | `local_cluster` bin of the harness crate, `~/.cache/mc-solana-tools/harness-target/debug/local_cluster` | `23f690ae…`, built from the harness source committed at `1765c2f` (unchanged since) |

## One-command demo (DEMO-LOCAL, 2026-10-30)

`scripts/demo-local.sh` wraps the closing-world recipe below for a click-through. It uses the pinned files above and refuses
to start, without rebuilding anything, if any hash differs. It also refuses if a port it needs is busy or less than 3 GiB is free.
It never stops a process it did not start.

```bash
scripts/demo-local.sh            # start: practice game -> validator -> local oracle -> website; Ctrl-C stops and cleans up
scripts/demo-local.sh owner-key  # copy the practice owner's key to the clipboard (for the wallet import box)
scripts/demo-local.sh status     # read-only Pool / Ops / Ticket summary
scripts/demo-local.sh stop       # clean up a run that was killed without Ctrl-C
```

In the Claude desktop app the same launcher is the `demo-local` entry of `.claude/launch.json`. Each run owns one folder,
`~/.cache/mc-demo-local/run-<UTC time>/`, holding the capped ledger (`--limit-ledger-size 10000`), the practice keys, the logs
and the process ids. Stopping removes that folder and nothing else.

**What to click** (website at `http://localhost:5180`):

1. **As a new player.**
   1. Choose "Connect new burner wallet". The local faucet funds it with 5 test SOL.
   2. Go to Invest, pick The Starter Package and enter an amount. The projection shows the Front-End Load and the daily rate. Deposit.
   3. In the Profit Center the position card shows live earnings and the Carried Interest tier. Withdraw to see the confirmation sheet and the payout.
   4. In the Seed Round, fund the Series A. Withdraw once more in the Profit Center. That starts a six-step allocation run, one click per step, with "Resume processing".
   5. Back in the Seed Round, "Claim Repayment" pays the backer's share.
2. **As the ten-day-old player.**
   1. Log out of the burner.
   2. Run `scripts/demo-local.sh owner-key`, open "Import a wallet secret", paste and choose "Import and connect".
   3. The 1 SOL position from ten days ago shows "This payout closes the round". Withdraw and choose "Confirm and close the round".
   4. Keep pressing the next button: the six allocation steps, the candidate pages and the candidate list, "Request randomized selection" (the local oracle answers in seconds), "Capture randomness", "Apply Emergency Equity Conversion", then "Finalize round".
   5. "Open a position in the new round" makes a round-2 deposit.

Browser smoke of exactly this path, 2026-10-30: `docs/solana/evidence/demo-local-20261030/` (9 captures plus a hash manifest).

**What it is not.** The practice game is SYNTHETIC: the position is backdated ten days and the pot is set to 0.1 SOL, so
the closing path is reachable today. The draw is answered by the local oracle's synthetic key, not the ORAO network. It
proves nothing about devnet timing, oracle liveness or the accepted 39-step flow.

**Limits found on 2026-10-30:**
- The pinned validator 3.1.0 has no RPC or faucet bind option, so ports 8899 and 9900 listen on all interfaces.
  `--bind-address 127.0.0.1` keeps gossip and TPU on loopback, and the chain holds only throwaway practice state.
- The existing legacy (Chain Fusion) panels and the Shenanigans panels make anonymous, read-only queries to the live
  Internet Computer. They make no update calls, signatures or payments, and all native game traffic goes to the local validator.
- Both are logged in `docs/coordination/DEMO_DEFERRED_WORK.md`.

## Recipe

```bash
# 1. fixtures: the two canonical multisig accounts + the validator argument list
local_cluster fixtures /private/tmp/mc-solana-runtime-tools-20261013/local-cluster/fixtures
# 2. validator (keep running): paste the printed args, filling the three <*_SO> paths, e.g.
solana-test-validator --reset --quiet --rpc-port 8899 --faucet-port 9900 --ledger <dir> --limit-ledger-size 10000 \
  --upgradeable-program 7aSvuR19bh9WBHoPC5ceLNvP6txxFkowTJ2FSi9fnf3o <PROGRAM_SO> nVdpWArd89aMWgicLSvLQZGo7kXf8soRZrJfzuZycz9 \
  --bpf-program SQDS4ep65T869zMMBKyuUq6aD6EgTu8psMjkvj52pCf <STANDIN_SO> \
  --bpf-program VRFCBePmGTpZ234BhbzNNzmyg39Rgdd6VgdfhHwKypU <ORAO_SO> \
  --account 5ASstTEwct76TEqMsKw4c9tZcKckyJK7j5doXeY1UPDn <fixtures>/multisig_g.json \
  --account HfbGhr2YTRhaU8BNLHoPb17P3AGRfGVzV2xnYQQ89QR5 <fixtures>/multisig_p.json
# 3. genesis (once per --reset): initialize via the G vault, ORAO initialize/configure/register, client +
#    operations funding, participant/registry/snapshot page 0
local_cluster genesis http://127.0.0.1:8899 <keys_dir>
# 4. oracle (keep running): fulfils pending ORAO requests with the synthetic authority (funds its own fee payer from the local faucet)
local_cluster oracle http://127.0.0.1:8899 <keys_dir>
# 5. read-only summary of Pool / Ops / Ticket
local_cluster status http://127.0.0.1:8899
```

Genesis facts (deterministic from the fixture seeds): G multisig `5ASstTEwct76TEqMsKw4c9tZcKckyJK7j5doXeY1UPDn`,
G vault `nVdpWArd89aMWgicLSvLQZGo7kXf8soRZrJfzuZycz9`, P multisig `HfbGhr2YTRhaU8BNLHoPb17P3AGRfGVzV2xnYQQ89QR5`,
P vault `ATE3hTDdzc8KjFJfRzs7QCyaaez7ydM9HzrVaxgtSxS`, ProgramData `9DDAjGQwc6cGaiyg5aubCHa11cdG383cBsR2hkToczSG`,
pool `Au9fiQqP7nNVZBTv2ZUPadFoxLnU75Q3VSczKbwwJsyf`, ORAO client `9k91kwTA4DFF4S3wVTvCirdLAimVKGsm9vDJVad1tdo3`,
RngAuthority `5dKzB8Vw6FYZCLYdHFp7m6maxnJmvWQ1rEBrhMEpUV22`. Cluster domain `musical-chairs-localnet-dev!!!!!`
(dev only; mainnet/devnet domains are the production constants). `public_series_a_enabled = true` so the
Series A panel can be exercised; the payer is sponsor and management recipient (dev only).

## Closing-world fixture (the path real time cannot reach)

An insolvent payout needs owed > pot, i.e. days of accrual at 11 %/day. `local_cluster
export-closing-fixture <out> <keys>` builds that world in LiteSVM with the production ELF — the
harness `World::genesis_with_provider()`, the real ORAO initialize / configure / register (G vault
through the stand-in), one 1-SOL Simple position opened under a Clock sysvar set to **wall time minus
ten days** (the harness' backdated-clock probe), the pot reduced to 0.1 SOL with `set_pot_fixture`
and operations funded with `set_ops_free` (both labelled synthetic fixture writes that keep both
cash registers conserved) — and exports its 20 program / ORAO / Squads / system accounts as validator
`--account` genesis files (`validator-args.txt`, `manifest.txt`), plus the owner secret at
`<keys>/owner.json`. The validator then starts from those accounts with no `genesis` step; the owner's
first payout is insolvent *today* and closes the round. Exercised end to end by
`frontend/src/solana/native/localClusterClosing.test.ts` (provision CloseRecord → insolvent
`withdraw_simple` → candidate pages → `build_candidates` until sealed → `request_round_draw` → local
oracle fulfils → `capture_round_draw` → `apply_promotion` → `finalize_round` → participant page →
`open_position` in round 2) in ~19 s. This is synthetic fixture setup on the tested ELF and economics,
not a deployment rehearsal; the position's history is fabricated (opened ten days before genesis),
everything after genesis is real program execution.

## Frontend against the local cluster

`VITE_SOLANA_RPC_URL=http://127.0.0.1:8899` in the gitignored `.env.local`; the native adapter's
dev-only local-cluster mode (IMPL-46c) uses the wallet-adapter burner wallet and skips the SIWS
canister sign-in so no ICP mainnet canister is touched; production builds compile that mode out.

## Limits

Local only: the validator's clock is real time (no warp); the closing path is reached through the
exported closing-world fixture above, maturity-gated compounding settlements are not driven here (LiteSVM); the fulfil authority is synthetic; fees are the validator defaults; nothing here is a
cluster measurement or a deployment rehearsal (M14).
