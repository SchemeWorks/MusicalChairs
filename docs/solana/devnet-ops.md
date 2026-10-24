# Devnet operator tool (M14-TOOL, packet RG-4)

`devnet_ops` (`solana/tests/financial-runtime/src/devnet_ops.rs`, binary `src/bin/devnet_ops.rs`) carries out
the Block A ceremony of the [M14 approval packet](../coordination/native-solana-m14-approval-packet-20261023.md)
(A2 Squads, A3 deploy and authority transfer, A4 initialize and ORAO register via G, A6/A8 read-backs).
**Local preparation only:** nothing here has touched a public cluster. Every write is a dry run unless `--execute` is
given, and `--execute` is refused on anything but a loopback validator, or devnet with the **packet pin** (below).

**The packet pin is not an approval.** `--packet-sha256` + `--packet-commit` only pin WHICH packet text was reviewed (an
artifact-identity check). The authority to run anything on devnet is **Rob's explicit approval, given in chat**; no flag, hash or
file stands in for it, and a hash that verifies proves nothing about whether anyone approved.

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
| where `--execute` works | `--cluster local` (default): RPC host exactly `localhost`, `127.0.0.1` or `[::1]`. `--cluster devnet`: https URL, an RPC host on the allowlist (`api.devnet.solana.com`, or one host named with `--allow-rpc-host`), `--pending-file`, and the **packet pin** `--packet-sha256 <sha256> --packet-commit <40-hex commit>`: the SHA-256 of `git show <commit>:docs/coordination/native-solana-m14-approval-packet-20261023.md` (the frozen, committed blob) must equal `--packet-sha256`, and the working file must equal that blob byte for byte. The packet path is fixed: `--packet` is refused on every cluster. The old flag `--i-have-approval` is REMOVED (rejected with an explanation): it only ever pinned the text, and its name claimed more. Any other cluster name (mainnet-beta, testnet) is refused. Read-only commands take no `--execute` |
| receipts | every executed output, flow receipt included, records `packet: {packet_path, packet_commit, packet_sha256}` (`null` on a loopback cluster); flow receipts are also stamped with the cluster (below) |
| double-send safety | before a transaction is sent its signature, blockhash, `lastValidBlockHeight`, genesis hash, command and packet pin are written to stderr and to `--pending-file` (default `<wallet-dir>/pending.jsonl` for `flow`; REQUIRED on devnet), durably: one `write` of the whole line, then `fsync` (`sync_data`); a failed write stops the send. An ambiguous send error (timeout, transport) is polled with `getSignatureStatuses` until confirmed, failed or the blockhash is invalid; nothing is rebuilt while it may still land (`--pending-wait-seconds`, default 90). Every run resolves unresolved pending records of this cluster FIRST, and the file is read **fail-closed** (next section). After a confirmed send, reads carry `minContextSlot` = the transaction's slot, and a one-transaction step stops after one confirmed send instead of looping on a read that may be stale |
| genesis hash | checked before any command: devnet needs `EtWTRABZ...`; a loopback RPC that reports mainnet, testnet or devnet genesis (a tunnel) is refused |
| packet size | every transaction is measured unsigned; over 1,232 bytes is refused before any signature exists. `vault_transaction_create` is always proposer = creator = rent payer = fee payer (one signature) |
| simulate before send | `simulateTransaction` on the signed transaction before every send; an error refuses the send (client-API simulation has sigVerify off; the cluster verifies on send) |
| time lock | `execute` reads the proposal's `Approved{timestamp}` and the cluster Clock and refuses before `timestamp + time_lock` (Squads 6021) |
| pinned bytes | `deploy`/`verify-bytes` hash the ELF first; only `8f97e83d...` (`PINNED_PROGRAM_ELF_SHA256`, kept equal to `latest-build.json` by a test); `--expected-sha256` overrides on `local` only |
| vote on what was shown | `approve`/`execute` rebuild the inner instruction from the flags and refuse unless the on-chain vault transaction stores exactly that message; `propose` refuses if the program's upgrade authority is not the G vault, the action is already done, or an identical live proposal exists (`--expect-tx-index N` is required to execute a propose) |
| spend caps | `deploy` refuses a peak above 15 SOL or an underfunded deployer; on devnet `--max-spend-lamports` cannot exceed 15 SOL and `--max-data-len` (ProgramData padding is permanent rent) needs `--max-data-len-justification <text>`, which is recorded. `fund-vault` runs the full G profile check, refuses if the vault already holds the rent it needs, and caps the vault's TOTAL (not one transfer) at 0.05 SOL |
| keys and files | nothing is generated; only the files named on the command line sign; `~/.config/solana/id.json` is refused by path for every file argument (`--elf`, `--program-so`, `--receipts`, `--pending-file`, key flags); files the tool appends to (`--receipts`, `--pending-file`) must be `.jsonl` and are refused if they look like a keypair; the program keypair must be the compiled-in id (`--allow-program-id-mismatch` on `local` only, for tests) |

**The write-ahead file (`--pending-file`) is the tool's memory of what it sent. Use the SAME file for every command against a cluster.**
The tool has no other record: a command run with a different (or no) `--pending-file` cannot see an unresolved send, a confirmed
transfer, or the slot a transaction landed in, so it could rebuild a transaction that may still land, send a funding transfer a second
time, and read from a load-balanced node that has not yet seen the last transaction. (`flow` defaults to `<wallet-dir>/pending.jsonl`; give
`fund-vault`, `deploy`, `propose`, `approve` and `execute` the same explicit path, and keep it with the receipts.) What the file does:

* **Fail closed.** Only a file that does not exist is empty. A read error, a line that is not a JSON record this tool wrote (a torn last line
  after a crash included), or a record missing its signature, blockhash or genesis hash makes EVERY command that was given the file refuse,
  naming the line number. Inspect the file; do not delete lines. A resolution record that could not be appended leaves the write-ahead record
  open, and the next run resolves it again from the chain.
* **Slot seed.** At startup each run hands the highest slot among the confirmed records of this cluster to the RPC client, so even the first
  read of a new run carries `minContextSlot` (the read-only commands too). A confirmed send's slot is polled for (`--slot-polls`, default 20
  polls, 500 ms apart), then falls back to the cluster slot read after the confirmation (recorded as `slot_source:
  get_slot_after_confirmation`); if neither works the step ends with an error that says the transaction CONFIRMED and names it, and no
  post-state read is made.
* **Once-only transfers.** The plain funding transfers (`fund-vault`; the flow steps `fund_operations` and the `*_fund_series_a` steps) are not
  idempotent. If the file already holds a CONFIRMED record for the same step on this genesis, the tool refuses and prints the signature,
  whatever the RPC node says about balances or counters.
* **Reads are typed.** The post-send `getAccountInfo` / `getProgramAccounts` configs are typed values checked before use (the commitment sits
  at the top level of the config; a dropped commitment would silently read at `finalized`).

**Residual risk (F9).** The genesis hash is reported by whatever endpoint `--rpc` names; the tool checks it, but an endpoint that
lies can report devnet's. The `--execute` host allowlist narrows that to the public devnet endpoint or one host you name, which
catches a typo or a look-alike URL, not a malicious operator-chosen provider. Use `https://api.devnet.solana.com` or a provider you
trust and have named explicitly.

**Solana notes.** Deploy = buffer account (ELF written in ~1,000-byte transactions) + ProgramData (45-byte header, the ELF,
zero padding to `max_data_len`, which is rent-paid and permanent) + the 36-byte Program account; "deployed equals reviewed" is
`data[45..45+elf_len] == ELF` and the rest zero (`verify-bytes`, packet A8). SetAuthority to the G vault is unchecked because a
PDA cannot co-sign (closest analogues: a proxy admin that is a Safe with a timelock; SNS-controlled controllers). The Squads time
lock runs on the cluster's Clock sysvar, so it is read from the chain, never from this machine.

## Commands

Global: `--rpc`, `--cluster`, `--execute`, `--packet-sha256`, `--packet-commit`, `--allow-rpc-host`, `--pending-file`, `--pending-wait-seconds`, `--slot-polls`, `--commitment confirmed|finalized`, `--simulate`.
A key flag takes a keypair JSON file, a file holding one base58 pubkey, or a bare pubkey (planning, read-backs and the pubkey-only
roles). Members files list exactly three pubkeys, one per line.

| command | needs | does |
| --- | --- | --- |
| `plan` | `--elf --deployer --program-keypair --buffer-keypair --payer --g-create-key --p-create-key --g-members --p-members --sponsor --management-recipient` | read-only: ordered phases A2, A3, A4-prep, propose, approve, execute with accounts, signers, fee payer, packet bytes, rent estimates |
| `squads-create` | `--payer --g-create-key --p-create-key` (keypairs) `--g-members --p-members` | real `multisig_create_v2`: G 2-of-3, 172,800 s; P 2-of-3, 0 s; skips (and verifies) an existing identical multisig, refuses a different one |
| `deploy` | `--elf --deployer --program-keypair --buffer-keypair` (keypairs) `--g-create-key --g-members` | resumable: create buffer, idempotent chunked writes, buffer byte check, `DeployWithMaxDataLen` (max_data_len = elf_len unless `--max-data-len`), A8 check BEFORE the authority moves, then SetAuthority to the G vault (only if G is in place) and read-back |
| `verify-bytes` | `--elf` (`--program-id`, `--expect-authority g-vault\|none\|<key>`, `--g-create-key`) | read-only A8: header, `elf_len`, `data.len()`, slot, authority, ELF equality, zero padding |
| `fund-vault` | `--payer --g-create-key --g-members --lamports` | system transfer so the G vault can pay the ORAO Client rent; checks the full G profile and the vault's balance first, reads the balance back |
| `propose initialize\|register` | `--member` (keypair) `--g-create-key --g-members`; `initialize` also `--p-create-key --sponsor --management-recipient` | `vault_transaction_create` + `proposal_create`; prints index, message sha256 and parameters |
| `approve initialize\|register` | as propose plus `--tx-index` | `proposal_approve` by one member |
| `execute initialize\|register` | as approve; `initialize` needs `--sponsor` as a keypair (it co-signs the outer transaction) | `vault_transaction_execute` after the lock |
| `readback` | `--g-create-key --p-create-key --g-members --p-members` (`--sections squads,program,initialize,orao`, `--sponsor`, `--management-recipient`, `--elf`) | packet A6: profiles, vault index 0, `Admission.guardian_vault == Pool.guardian_vault == P vault`, ORAO `Client.owner == G vault`, `Client.state == RngAuthority`, Client account owner == ORAO program |
| `validator-fixtures <out_dir>` | optional `--program-so --g-create-key` | offline: writes the validator account files and prints the exact local validator command |
| `flow init\|status\|next` | `--wallet-dir` (see [Flow runner](#flow-runner-the-m14-plan)) | runs the 39-step M14 flow plan against a cluster, one interlocked step at a time |

`initialize` parameters default to the packet (devnet cluster domain `musical-chairs-devnet-domain-v1!`, request fee 300,000,
headroom 2,500 bps, `--public-series-a-enabled true`, the devnet ORAO tuple); the flags `--cluster-domain`,
`--provider-request-fee-lamports`, `--provider-fee-headroom-bps`, `--public-series-a-enabled` override them and must be identical at
propose, approve and execute. Ceremony order: `squads-create` -> `deploy` -> `fund-vault` -> `propose` (initialize, register) ->
2 x `approve` each -> wait 172,800 s on the cluster clock -> `execute initialize` -> `execute register` -> `readback` -> `verify-bytes`.
Devnet runs need `--cluster devnet --rpc https://api.devnet.solana.com --pending-file <path.jsonl> --packet-sha256 <sha256> --packet-commit <commit>` AND Rob's explicit approval; the pin is taken from the frozen commit of the reviewed packet (`git show <commit>:<packet path> | shasum -a 256`), after the packet text is committed, and proves only which text was reviewed.
A fresh program id (packet 5a) changes the compiled-in id and the pinned ELF hash: rebuild, re-pin, then use the tool.

## Flow runner (the M14 plan)

`devnet_ops flow` executes the [M14 flow plan](m14-flow-plan.md) (39 steps, Seq-B, 41.46 days) against a cluster. The plan is
DATA in the library (`src/m14_plan.rs`: who acts, which instruction, the amount, the offset from genesis, the expected outcome),
shared with the clock-warped LiteSVM rehearsal `tests/m14_flow_plan.rs`, so the rehearsed plan and the run plan cannot drift.

```bash
devnet_ops flow init   --wallet-dir W                                  # offline: record the wallets' PUBLIC keys in W/wallets.json
devnet_ops flow status --wallet-dir W --rpc URL [--cluster devnet]     # read-only: done / next / earliest time / both identities
devnet_ops flow next   --step ID --wallet-dir W --rpc URL [--execute] [--local-oracle] [--wait-seconds N] [--receipts FILE]
```

* **Wallets.** `W` holds one Solana-CLI keypair file per plan actor, created by the operator (the tool generates nothing):
  `S` (sponsor), `K` (keeper), `A1`, `A2`, `P1`..`P5`, `W` (the plain wallet), plus `ORACLE.json` for a loopback synthetic oracle.
  `flow init` writes `W/wallets.json` (`{"schema":1,"plan":...,"program_id":...,"wallets":{"A1":"<pubkey>",...}}`, public keys only) and
  refuses to overwrite a `wallets.json` that records different keys. `flow next` loads the actor's file only after its pubkey equals the
  recorded one (the promotion "Winner" resolves to P4 or P5 from `CloseRecord(1)`); any mismatch is refused before anything is signed.
* **Genesis and "earliest time".** Plan offsets count from the G approvals. `initialize` runs no earlier than 172,800 s later and stamps
  `ConfigVersion#1.activated_at`, so genesis = that stamp minus 172,800 s and step 1 is due at the stamp. `earliest = genesis + offset`, judged
  against the CLUSTER clock (Clock sysvar). Real runs drift later than the offsets; the program's own gates (maturity, the 600 s promotion
  deadline, strict insolvency) still decide, and the runner pre-checks them with a plain-language refusal. `--genesis-unix N` overrides
  genesis on a loopback cluster only (a local validator cannot wait two days).
* **What is done.** Every step has a state predicate judged from the cluster (a counter reached, an account exists, a draw state, `finalized`,
  ...). A step is also done when any later step is. A receipt line counts as evidence ONLY for the one step that leaves no on-chain trace
  (the refused-by-design expiry), only if the receipt is stamped for THIS cluster and says `refused_as_expected` (any other result, a failed-check one included, leaves the step to be simulated again); every other step is judged from the chain alone. `flow
  status` prints each step's `done`, its evidence, its earliest time, and `next` with `ready` and `seconds_until_earliest`.
* **`flow next`** refuses: a step that is not THE next one; a step before its earliest time; an actor wallet that does not match
  `wallets.json`; books that do not balance before the step; an economics oracle that disagrees with the plan (a payout the plan calls solvent
  that would be insolvent, or the reverse); a promotion deadline already passed for request or capture. Then it builds exactly that step's
  transaction(s) with the harness builders (see below), measures (1,232 B), simulates, and sends only under the standard interlocks (dry run by
  default; `--execute` on loopback, or devnet with the packet pin, `--pending-file` and the devnet genesis hash; never mainnet). Multi-transaction
  steps (allocation ticket phases, candidate pages, the candidate build) rebuild each next transaction from the chain after the previous one, so a
  step is resumable; a one-transaction step (every other step) stops after ONE confirmed send, so a read that is still pre-send can never cause a
  second send of a deposit or a funding. A dry run prints the first transaction only.
* **Receipts.** Each executed step appends one JSON line to `--receipts` (default `W/flow-receipts.jsonl`): signatures and slots, the account
  read-backs its `expect` names (Pool, Ops, Ticket, the position / Backers / CloseRecord it touched), the post-condition checks (the facts the
  rehearsal asserts from events, restated as state differences: pot, management, registry, tolls, the actor's lamport equation) and
  `conservation` (the vault identity `vault - rent == pot + seed + management + pending_toll + funded_claims + excess`, the OpsCash identity, and
  `Ops.locked_remaining_total == sum of the CloseRecord partitions`, all computed from RPC). A failing check or identity records
  `result: "done_with_failed_checks"` (not `done`) and exits 4: STOP. The plan then refuses every further `flow next` until the receipt has been
  examined and the step named with `--accept-failed-checks <step>[,<step>]`. Each receipt is stamped with the cluster it ran against (genesis hash,
  program id, `ConfigVersion#1` activation stamp) and with the packet pin; `flow status` counts receipts of another cluster as `foreign_receipts`,
  and `flow next` refuses a log that holds any (use a fresh `--receipts` file for a new cluster or a `--reset` ledger).
* **How steps are built.** The harness builders read state from a LiteSVM. The runner copies every program account (`getProgramAccounts`) and the
  cluster's Rent sysvar and clock into a MIRROR `World` (`FinancialRuntime::mirror`, no program loaded, nothing executed), points its fee payer at the
  actor, and lets `World` / `close.rs` produce the instruction. The provider tuple of `request_round_draw` comes from the ConfigVersion (the pinned
  treasury), not the rehearsal's synthetic one.
* **The oracle (packet A7).** `fulfil_*` never fulfils on devnet. `flow next --step fulfil_1 --execute --wait-seconds 600` polls the request
  account until the real ORAO oracle fulfils it (no transaction is sent; without a fulfilment it exits 4 with `"result":"pending"`), then records
  request -> fulfilment latency from the request account's own history (`getSignaturesForAddress`): slots and seconds from block times, and the margin
  to the promotion deadline. If the oracle fulfils between invocations, `flow next --step fulfil_1` observes it and writes the receipt. Only on a
  loopback cluster, `--local-oracle` signs the fulfilment with `W/ORACLE.json` (the synthetic authority the local ORAO network is configured with).
* **`expire_at_deadline`** is meant to be REFUSED (`DeadlineNotReached`, expiry on the deadline). The runner builds, signs and simulates it, requires
  exactly that error in the logs, records `refused_as_expected` and sends nothing (a known-failing transaction would only burn a fee; the simulation
  is the evidence). It must run while the cluster clock is at or before the deadline; after that the step counts as passed.
* **Local use.** On a loopback validator the clock is real time, so only the first steps (setup and round 1 deposits, offsets up to 173,160 s)
  are runnable in one sitting, with `--genesis-unix <now - 172800>`; the whole plan runs in LiteSVM (`tests/m14_flow_tool.rs`).

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
cargo +1.92.0 test --locked --test m14_flow_tool -- --test-threads=1 --nocapture          # flow runner gates and refusals; 1 test ignored
MC_ORAO_PROFILE=devnet cargo +1.92.0 test --locked --test m14_flow_tool -- --ignored --test-threads=1 --nocapture   # the whole plan through `flow next`
cargo +1.92.0 test --locked --test m14_flow_plan                                           # plan shape + arithmetic (the rehearsal itself is --ignored)
MC_LOCAL_RPC=http://127.0.0.1:8899 cargo +1.92.0 test --locked --test devnet_ops_local -- --ignored --test-threads=1 --nocapture \
  local_validator_deploy_authority_squads_and_proposal_lifecycle     # fresh --reset validator per run
cargo +1.92.0 test --locked --test devnet_ops_local -- --ignored deploy_in_litesvm   # probe: LiteSVM loader-v3 deploy support unverified
```

## Limits (not achievable locally)

The real 1.4 MB deploy (about 1,400 write transactions, rate limits, faucet size) and devnet fees/latency; the 48-hour time lock on a
live clock; the devnet oracle's liveness; RPC `sigVerify` in the pre-send simulation (the client API fixes it off).
