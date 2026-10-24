# Devnet operator tool (M14-TOOL, packet RG-4)

`devnet_ops` (`solana/tests/financial-runtime/src/devnet_ops.rs`, binary `src/bin/devnet_ops.rs`) carries out
the Block A ceremony of the [M14 approval packet](../coordination/native-solana-m14-approval-packet-20261023.md)
(A2 Squads, A3 deploy and authority transfer, A4 initialize and ORAO register via G, A6/A8 read-backs).
**Local preparation only:** nothing here has touched a public cluster. Every write is a dry run unless `--execute` is
given, and `--execute` is refused on anything but a loopback validator, or devnet with the approved packet's hash.

## Build and run

```bash
export CARGO_HOME=$HOME/.cache/mc-solana-tools/cargo-home CARGO_TARGET_DIR=$HOME/.cache/mc-solana-tools/harness-target
cd solana/tests/financial-runtime
cargo +1.92.0 build --locked --bin devnet_ops          # -> $CARGO_TARGET_DIR/debug/devnet_ops
devnet_ops <command> [action] --rpc <url> [--cluster local|devnet] [--execute] [flags]
```

Output is a JSON receipt on stdout (progress on stderr). Exit codes: `0` ok, `1` failure, `2` usage, `3` refused by an
interlock, `4` a verification or read-back check did not hold (receipt still printed).

## Safety interlocks

| interlock | behaviour |
| --- | --- |
| dry run | no `--execute` = nothing signed or sent; `--simulate` additionally simulates the independent steps (unsigned, sigVerify off) |
| where `--execute` works | `--cluster local` (default): RPC host exactly `localhost`, `127.0.0.1` or `[::1]`. `--cluster devnet`: https URL and `--i-have-approval <sha256>` equal to the SHA-256 of the packet file on disk (`--packet` overrides the path), so editing the packet after approval invalidates it. Any other cluster name (mainnet-beta, testnet) is refused. Read-only commands take no `--execute` |
| genesis hash | checked before any command: devnet needs `EtWTRABZ...`; a loopback RPC that reports mainnet, testnet or devnet genesis (a tunnel) is refused |
| packet size | every transaction is measured unsigned; over 1,232 bytes is refused before any signature exists. `vault_transaction_create` is always proposer = creator = rent payer = fee payer (one signature) |
| simulate before send | `simulateTransaction` on the signed transaction before every send; an error refuses the send (client-API simulation has sigVerify off; the cluster verifies on send) |
| time lock | `execute` reads the proposal's `Approved{timestamp}` and the cluster Clock and refuses before `timestamp + time_lock` (Squads 6021) |
| pinned bytes | `deploy`/`verify-bytes` hash the ELF first; only `8f97e83d...` (`PINNED_PROGRAM_ELF_SHA256`, kept equal to `latest-build.json` by a test); `--expected-sha256` overrides on `local` only |
| vote on what was shown | `approve`/`execute` rebuild the inner instruction from the flags and refuse unless the on-chain vault transaction stores exactly that message; `propose` refuses if the program's upgrade authority is not the G vault, the action is already done, or an identical live proposal exists (`--expect-tx-index N` is required to execute a propose) |
| spend caps | `deploy` refuses a peak above 15 SOL (`--max-spend-lamports`) or an underfunded deployer; `fund-vault` is capped at 0.05 SOL |
| keys | nothing is generated; only the files named on the command line sign; `~/.config/solana/id.json` is refused by path; the program keypair must be the compiled-in id (`--allow-program-id-mismatch` on `local` only, for tests) |

**Solana notes.** Deploy = buffer account (ELF written in ~1,000-byte transactions) + ProgramData (45-byte header, the ELF,
zero padding to `max_data_len`, which is rent-paid and permanent) + the 36-byte Program account; "deployed equals reviewed" is
`data[45..45+elf_len] == ELF` and the rest zero (`verify-bytes`, packet A8). SetAuthority to the G vault is unchecked because a
PDA cannot co-sign (closest analogues: a proxy admin that is a Safe with a timelock; SNS-controlled controllers). The Squads time
lock runs on the cluster's Clock sysvar, so it is read from the chain, never from this machine.

## Commands

Global: `--rpc`, `--cluster`, `--execute`, `--i-have-approval`, `--packet`, `--commitment confirmed|finalized`, `--simulate`.
A key flag takes a keypair JSON file, a file holding one base58 pubkey, or a bare pubkey (planning, read-backs and the pubkey-only
roles). Members files list exactly three pubkeys, one per line.

| command | needs | does |
| --- | --- | --- |
| `plan` | `--elf --deployer --program-keypair --buffer-keypair --payer --g-create-key --p-create-key --g-members --p-members --sponsor --management-recipient` | read-only: ordered phases A2, A3, A4-prep, propose, approve, execute with accounts, signers, fee payer, packet bytes, rent estimates |
| `squads-create` | `--payer --g-create-key --p-create-key` (keypairs) `--g-members --p-members` | real `multisig_create_v2`: G 2-of-3, 172,800 s; P 2-of-3, 0 s; skips (and verifies) an existing identical multisig, refuses a different one |
| `deploy` | `--elf --deployer --program-keypair --buffer-keypair` (keypairs) `--g-create-key --g-members` | resumable: create buffer, idempotent chunked writes, buffer byte check, `DeployWithMaxDataLen` (max_data_len = elf_len unless `--max-data-len`), A8 check BEFORE the authority moves, then SetAuthority to the G vault (only if G is in place) and read-back |
| `verify-bytes` | `--elf` (`--program-id`, `--expect-authority g-vault\|none\|<key>`, `--g-create-key`) | read-only A8: header, `elf_len`, `data.len()`, slot, authority, ELF equality, zero padding |
| `fund-vault` | `--payer --g-create-key --lamports` | system transfer so the G vault can pay the ORAO Client rent |
| `propose initialize\|register` | `--member` (keypair) `--g-create-key --g-members`; `initialize` also `--p-create-key --sponsor --management-recipient` | `vault_transaction_create` + `proposal_create`; prints index, message sha256 and parameters |
| `approve initialize\|register` | as propose plus `--tx-index` | `proposal_approve` by one member |
| `execute initialize\|register` | as approve; `initialize` needs `--sponsor` as a keypair (it co-signs the outer transaction) | `vault_transaction_execute` after the lock |
| `readback` | `--g-create-key --p-create-key --g-members --p-members` (`--sections squads,program,initialize,orao`, `--sponsor`, `--management-recipient`, `--elf`) | packet A6: profiles, vault index 0, `Admission.guardian_vault == Pool.guardian_vault == P vault`, ORAO `Client.owner == G vault`, `Client.state == RngAuthority`, Client account owner == ORAO program |
| `validator-fixtures <out_dir>` | optional `--program-so --g-create-key` | offline: writes the validator account files and prints the exact local validator command |

`initialize` parameters default to the packet (devnet cluster domain `musical-chairs-devnet-domain-v1!`, request fee 300,000,
headroom 2,500 bps, `--public-series-a-enabled true`, the devnet ORAO tuple); the flags `--cluster-domain`,
`--provider-request-fee-lamports`, `--provider-fee-headroom-bps`, `--public-series-a-enabled` override them and must be identical at
propose, approve and execute. Ceremony order: `squads-create` -> `deploy` -> `fund-vault` -> `propose` (initialize, register) ->
2 x `approve` each -> wait 172,800 s on the cluster clock -> `execute initialize` -> `execute register` -> `readback` -> `verify-bytes`.
Devnet runs need `--cluster devnet --rpc https://api.devnet.solana.com --i-have-approval $(shasum -a 256 <packet> | cut -d' ' -f1)`.
A fresh program id (packet 5a) changes the compiled-in id and the pinned ELF hash: rebuild, re-pin, then use the tool.

## Local rehearsal

**Agave 3.1.0-beta.0 (pinned in `latest-build.json`; the scratch copy is gone).** Download only, no global install; the archive hash is
checked before anything is extracted (official anza release, 77,248,568 bytes):

```bash
D=$HOME/.cache/mc-solana-tools/agave && mkdir -p $D && cd $D
curl -fL -o solana-release-aarch64-apple-darwin.tar.bz2 \
  https://github.com/anza-xyz/agave/releases/download/v3.1.0-beta.0/solana-release-aarch64-apple-darwin.tar.bz2
echo "de9be527da14757854c47defffffeb0d17d1b7b6c0a4175f46f531aca262bad9  solana-release-aarch64-apple-darwin.tar.bz2" | shasum -a 256 -c -
tar xjf solana-release-aarch64-apple-darwin.tar.bz2            # -> $D/solana-release/bin/solana-test-validator
$D/solana-release/bin/solana-test-validator --version         # expect 3.1.0 (src:bd8c57e7)
```

**Validator** (real Squads and devnet ORAO binaries from `solana/fixtures/devnet/`; `FIX` = that directory):

```bash
OUT=$HOME/.cache/mc-solana-tools/devnet-ops-local; FIX=<repo>/solana/fixtures/devnet
devnet_ops validator-fixtures $OUT      # writes squads_program_config.json, orao_network_state.json, orao_treasury.json
$HOME/.cache/mc-solana-tools/agave/solana-release/bin/solana-test-validator --reset --quiet --rpc-port 8899 --faucet-port 9900 \
  --ledger $OUT/ledger --limit-ledger-size 10000 \
  --bpf-program SQDS4ep65T869zMMBKyuUq6aD6EgTu8psMjkvj52pCf $FIX/squads-devnet.so \
  --bpf-program VRFCBePmGTpZ234BhbzNNzmyg39Rgdd6VgdfhHwKypU $FIX/orao-devnet.so \
  --account BSTq9w3kZwNwpBXJEvTZz2G9ZTNyKBvoSeXMvwb4cNZr $OUT/squads_program_config.json \
  --account 3e483QYyYQqu8eesT6ZjfuBiWF9PN5JXKeB8TRmn3hUr $OUT/orao_network_state.json \
  --account 9ZTHWWZDpB36UFe1vszf2KEpt83vwi27jDqtHQ7NSXyR $OUT/orao_treasury.json
```

To also rehearse propose/approve/execute on the validator, preload the pinned program with the G vault as authority: run
`devnet_ops validator-fixtures $OUT --program-so ~/.cache/mc-solana-artifacts/8f97e83db9dbc3112b1c18b5a35dfd5cfe6ae86bcb1a0cc93da78f7bb18dc826/musical_chairs_program.so --g-create-key <g-create keypair file>`
and start the validator with the command it prints (it appends `--upgradeable-program 7tj9... <so> <G vault>`), then export `MC_LOCAL_PROGRAM_PRELOADED=1`.
A validator clock cannot be warped, so the 48-hour execute is exercised in LiteSVM; on the validator the tool must refuse it (time lock).

**Tests** (coordinator):

```bash
cd solana/tests/financial-runtime
cargo +1.92.0 test --locked --test devnet_ops_local -- --test-threads=1 --nocapture      # LiteSVM + gates; 2 tests ignored
MC_LOCAL_RPC=http://127.0.0.1:8899 cargo +1.92.0 test --locked --test devnet_ops_local -- --ignored --test-threads=1 --nocapture \
  local_validator_deploy_authority_squads_and_proposal_lifecycle     # fresh --reset validator per run
cargo +1.92.0 test --locked --test devnet_ops_local -- --ignored deploy_in_litesvm   # probe: LiteSVM loader-v3 deploy support unverified
```

## Limits (not achievable locally)

The real 1.4 MB deploy (about 1,400 write transactions, rate limits, faucet size) and devnet fees/latency; the 48-hour time lock on a
live clock; the devnet oracle's liveness; RPC `sigVerify` in the pre-send simulation (the client API fixes it off).
