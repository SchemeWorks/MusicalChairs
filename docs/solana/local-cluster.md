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

## Tools (owned temp dir, no global install)

| tool | where | pin |
| --- | --- | --- |
| `solana-test-validator`, `solana` 3.1.0 (`src:bd8c57e7`) | `/private/tmp/mc-solana-runtime-tools-20261013/agave-macos/solana-release/bin/` | official `solana-release-aarch64-apple-darwin.tar.bz2` of agave v3.1.0-beta.0, sha in `latest-build.json` (`toolchain.archives`) |
| program ELF | `…/sbf-output/musical_chairs_program.so` | `latest-build.json` artifact (`8f97e83d…` after M10) |
| Squads vault stand-in | `solana/tests/financial-runtime/standin/squads-vault-standin/artifacts/squads_vault_standin.so` | harness fixture (`SQDS4ep…`) |
| ORAO VRF | `solana/probes/runtime/fixtures/orao-callback-mainnet-finalized.so` at `VRFCBePmGTpZ234BhbzNNzmyg39Rgdd6VgdfhHwKypU` | probes provenance |
| bootstrap | `local_cluster` bin of the harness crate (`cargo build --locked --bin local_cluster`) | commit `73fd37d` |

## Recipe

```bash
# 1. fixtures: the two canonical multisig accounts + the validator argument list
local_cluster fixtures /private/tmp/mc-solana-runtime-tools-20261013/local-cluster/fixtures
# 2. validator (keep running): paste the printed args, filling the three <*_SO> paths, e.g.
solana-test-validator --reset --quiet --rpc-port 8899 --faucet-port 9900 --ledger <dir> --limit-ledger-size 10000 \
  --upgradeable-program 7tj9biW3KRJ7EEWmVUGigHiouCTXhV2dzcyvwma7Cyu7 <PROGRAM_SO> nVdpWArd89aMWgicLSvLQZGo7kXf8soRZrJfzuZycz9 \
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
P vault `ATE3hTDdzc8KjFJfRzs7QCyaaez7ydM9HzrVaxgtSxS`, ProgramData `9fbEej8qbMTZbWBgN19AME3WA6kRSWE3JmHM1hfX8Lup`,
pool `AYEAZnLGHzjV95vMtRu4StknBBRtmZUGLwK297Na1jqK`, ORAO client `7b1ebeG8qgEtCB3ufPHcZLiUeFJ6cYMS9BeiTnSBJ6HT`,
RngAuthority `6n6uusHJtZXSDSZec1Vt8T53hgLS6vp3iULH5ew1omm5`. Cluster domain `musical-chairs-localnet-dev!!!!!`
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
