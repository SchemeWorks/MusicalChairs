# M14 F11: G time-lock reduce and restore protocol, plus the M14-F11-TOOL contract (2026-10-27)

**Status.** PREPARATION ONLY.

**Revision note (2026-10-27).** Gideon's bounded review of candidate `5aa64e1` raised ONE blocking correction, **F11-R1**. `execute` must enforce the no-other-live-G-transaction rule and the exact latest index itself, before any signing or send. A manual `status` check before execute is not an enforced guard. R4 and S3 below now say so. The receipt is in `m14-f11-tool-delta-20261027.md`.

**Scope.** F11 is part of the approved Block A flow list:
- packet section 6.5, line 490: "G reduces its own time lock; document; restore";
- D12, line 561: "proposed after A6, executed at day 4, restored at once";
- section 6.4, line 473;
- RG-8, line 370.

It is not optional; F12 is the optional item.

**Authority.** Gideon's 2026-10-27 adjudication authorizes offline preparation only. **Before ANY public config proposal or send, Silas stops for Gideon's review of:**
- the source;
- the exact pins and tests;
- the reduce and restore preconditions in this document.

The frozen packet is not edited.

## Why this is safe for the main flow, and what it costs

- **RG-8 (program side).** `require_governance` demands G's `time_lock == 172,800` exactly (`controls.rs:626-631`, `validate_profile` at `controls.rs:1403`). While G is reduced, the program refuses EVERY G governance instruction: `stage_config_version`, `rotate_recipient`, `rotate_guardian` and `withdraw_provider_return`.
- **The main flow does not depend on G.** None of the 39 Seq-B steps is a G governance instruction, and the flow runner never reads G's profile (`src/devnet_ops/flow.rs`). The ORAO Client check compares `Client.owner` with the G vault ADDRESS, which does not change.
- **The weakened window.** While the lock is 0 s, any two G voters could execute a vault transaction (a program upgrade included) without delay. The voters are coordinator-held devnet test keys. The restore runs at once, in the same sitting.
- **Squads side.** Executing a config transaction makes every earlier unexecuted G transaction stale (`stale_transaction_index = index`). That is why F11 comes after `initialize` and `register` executed (both done) and refuses while any G transaction is live.

## Fixed values

| item | value |
| --- | --- |
| G multisig, vault (index 0) | `5JxzvXvQpYpwQMTQb1Pxch9ypQrBmDbgWf8KqPECzuEu`, `EjBoCzPngyaPUA6q33XaXRVnbUdEEbShZ2yudeAD5dgB` |
| profile kept throughout | threshold 2; the same 3 members, each with permission 7; config authority = default (autonomous); rent collector none; same create key and bump |
| normal lock `T_N` | 172,800 s |
| reduced lock `T_R` | **0 s**. Packet 6.4: "restoring is immediate at the reduced delay". This is also P's profile value. |
| G state read 2026-10-27 (finalized slot 515,754,287) | transaction_index 2 (both executed), stale_transaction_index 0, time_lock 172,800 |
| expected indexes | reduce **#3**, restore **#4** (each confirmed by a dry run, then `--expect-tx-index`) |
| actors | G voter 1 is proposer = creator = rent payer = fee payer and executor; G voters 1 and 2 approve |
| cost (Squads bucket, from voter balances) | 2 config transactions + 2 proposals (rent kept, no rent collector) + about 10 fees, at most about 0.01 SOL (each dry run prints the exact rent; voter 1 holds about 0.177 SOL) |

## Protocol

Every write is dry-run first. It is sent only with `--execute` under the standard interlocks:
- devnet with the packet pin `ae8a6a0e…` at `aeab470`;
- the shared pending file;
- the genesis check;
- size of at most 1,232 bytes, simulation before send, and write-ahead before send.

ANY refusal, failed check, simulation error or held-unknown result is a STOP: no retry, no clearing, no rebuild.

**R0. Preconditions** (read-only, at the seeded `minContextSlot`):
- devnet genesis;
- the pending file has 0 open or held records;
- the G profile is exact with `time_lock == 172,800`;
- the program's upgrade authority is the G vault;
- the Pool is initialized, `ConfigVersion#1` is active, and the ORAO Client is registered (owner G vault, state RngAuthority);
- NO live G transaction: every index in `(stale_transaction_index, transaction_index]` has a terminal proposal (Executed, Rejected or Cancelled), and none is a transaction without a proposal;
- the member key is a G member with Initiate permission;
- the baseline consequence probe (R5) does NOT report `WrongGovernanceProfile`.

**R1. Propose reduce.**
- **Command:** `g-timelock propose reduce --execute --expect-tx-index 3`.
- **Transactions:** `config_transaction_create { actions: [SetTimeLock { new_time_lock: 0 }], memo: None }`, then `proposal_create` (Active).
- **Checks after:** the decoded config transaction #3 stores EXACTLY `[SetTimeLock{0}]` (discriminator, multisig, index, creator = proposer); the proposal is Active; `transaction_index == 3`.

**R2. Approve reduce, twice** (voter 1, then voter 2).
- **Before each:** re-decode, and require that the stored actions are exactly `[SetTimeLock{0}]`, the proposal is Active, `3 > stale_transaction_index`, the current `time_lock == 172,800` and the member has not yet approved.
- **After the second approval:** the proposal is `Approved{t_R}`.

**R3. Wait.** `execute` refuses until `cluster clock ≥ t_R + 172,800`, the CURRENT lock, read from the multisig. There is no shortened clock or warp.

**R4. Execute reduce.**
- **Command:** `g-timelock execute reduce --tx-index 3` (voter 1).
- **Enforced guard (F11-R1, before any signing, signed simulation or send, in dry runs too):**
  - The executing index must equal G's LATEST `transaction_index` (3).
  - A full bounded scan of `(stale_transaction_index, transaction_index]` must find every OTHER index terminal: Executed, Rejected or Cancelled.
  - Any of these refuses: a live proposal, an orphan transaction without a proposal (identical configs included), a missing or undecodable account, a read error, or a range above the scan cap.
  - The only exclusion is this exact `[SetTimeLock{0}]` config with its Approved proposal.
  - Why: a transaction created during the 48 h wait would otherwise be silently staled, and an already-approved stale vault transaction can still execute on the real binary.
- **Transaction:** `config_transaction_execute`, with the optional rent payer and system program passed as None. `SetTimeLock` does not reallocate.
- **Checks after:**
  - the proposal is Executed;
  - `time_lock == 0`;
  - threshold, members and permissions, config authority, rent collector, create key and vault are unchanged;
  - `stale_transaction_index == 3` and `transaction_index == 3`;
  - finality is recorded.

**R5. Document the reduced state** (read-only).
- **`g-timelock status`:** the profile and phase.
- **`readback --sections squads`:** expect exactly ONE failed check, `G.time_lock_seconds` (0 vs 172,800). P is unchanged.
- **`g-timelock probe` (the RG-8 consequence):**
  - What it does: simulates, and never sends, `rotate_recipient(kind 0, new_recipient = the current Pool.management_recipient)`, which is a no-op. The G vault is a signer flag in an unsigned message, under `sigVerify: false`.
  - Expected result: the program error `WrongGovernanceProfile`.
  - Why it cannot be sent: no key can sign for the vault PDA.
  - Baseline: run at R0 and again at S4, it must NOT report `WrongGovernanceProfile`.

**S0. Restore preconditions** (immediately after R5, same sitting):
- the G profile is exact except `time_lock == 0`;
- reduce #3 is Executed;
- no live G transaction above stale index 3;
- the pending file has 0 open or held records.

**S1. Propose restore.** `g-timelock propose restore --execute --expect-tx-index 4`: `[SetTimeLock { new_time_lock: 172,800 }]`. The same checks as R1 apply.

**S2. Approve restore, twice.** The same rules as R2, with the current `time_lock == 0`.

**S3. Execute restore.**
- **Command:** `g-timelock execute restore --tx-index 4`, allowed at once because `now ≥ t_S + 0`.
- **Enforced guard (F11-R1):** the same as R4. Index 4 must be the latest, and every other index above stale index 3 must be terminal, all before any signing, simulation or send.
- **Checks after:** `time_lock == 172,800`; everything else unchanged; `stale_transaction_index == 4`.

**S4. Final read-back.**
- the full `readback` is 44/44 again;
- the probe no longer reports `WrongGovernanceProfile`;
- `g-timelock status` phase = restored;
- spends go in the SOL ledger.

**If the restore cannot complete.**
- **State:** G stays at 0 s. The program refuses all G governance (RG-8), and the lock offers no delay.
- **Remedy:** only the same guarded restore path.
- **A definite on-chain rejection that moved nothing:** the restore may be re-proposed.
- **A stale or rejected restore proposal:** a new restore proposal at 0 s, so there is no wait.
- **A held-unknown result:** STOP. Any retry needs Rob's exact approval of the signature, the effect and the retry, plus a review.

**Timing.** Propose the reduce only after Gideon accepts this tool, then execute it no earlier than 48 h after its second approval (packet: "day 4"). Restore in the same sitting. Do not run R4 to S4 inside the main flow's step 10 window (2026-11-03 13:11:35Z to 15:37:47Z). The flow does not need G, but the window must not have two operations competing for the operator.

## Receipt contract

Each `g-timelock` command prints one JSON receipt on stdout. Executed runs also carry the packet pin and the cluster stamp. The fields:
- `command`, `mode`, `action` (reduce or restore), `transaction_index`;
- the config transaction and proposal addresses;
- `stored_actions`, decoded;
- `g_profile_before` and `g_profile_after`: threshold, time lock, members and permissions, config authority, rent collector, create key, transaction and stale indexes;
- `approved_at`, `time_lock_releases_at`, `cluster_clock`;
- `steps`: label, signature, slot, simulation units or error;
- `checks` and `all_ok`.

`status` and `probe` are read-only and say so. The probe records `simulated_only: true`, the instruction, the program error code and name, and `expected`.

The run's evidence goes in `docs/solana/evidence/2026-1N-NN-devnet-governance.json` (packet line 524). It holds:
- R0 to S4 in order, with every signature, slot and finality;
- the three profiles (normal, reduced, restored);
- the three probe results;
- the timing;
- the read-backs before, during and after;
- the spend.

## M14-F11-TOOL card (offline implementation contract)

**Ownership.**
- **One Sonnet worker:** implementation and compile checks only (`cargo check`, `clippy`).
- **Silas:** brief, review, and the ONLY test runner and build executor for verification and the binary hash.
- **Gideon:** bounded delta review before any devnet use.

**Files.**
- **NEW:**
  - `solana/tests/financial-runtime/src/devnet_ops/timelock.rs`: all F11 logic, its Squads config-transaction builders and decoders;
  - `solana/tests/financial-runtime/tests/g_timelock.rs`.
- **Minimal wiring in `src/devnet_ops.rs`:** `mod timelock;`, ONE dispatch arm `"g-timelock"`, a help-text line, and `pub(crate)` on reused helpers. Existing commands keep their behavior.
- **`docs/solana/devnet-ops.md`:** one new section.
- **Untouched:** the program crate, IDL, ELF, `Cargo.toml` and `Cargo.lock` (no new crates), the packet, fixtures, `squads_real.rs`, `flow.rs`, `fund.rs`, the board and evidence.

**Interface.**
- `g-timelock status | probe` (read-only).
- `g-timelock propose | approve | execute reduce | restore` (writes; dry run by default).
- Flags: `--g-create-key --g-members --member` (keypair, writes only), `--tx-index` (approve and execute), `--expect-tx-index` (required for `propose --execute`). Plus the standard devnet flags.

**Refused by design (parse or usage error):**
- any other action, and any value or time-lock flag (0 and 172,800 are code constants);
- threshold, member, spending-limit or rent-collector changes;
- the P multisig, ephemeral signers, a memo;
- approving or executing a config transaction whose stored actions are not exactly the expected single `SetTimeLock`.

**Reused guards.** The tool reuses, unchanged:
- the Gate (devnet, packet pin, host allowlist, required pending file);
- the genesis check and `resolve_pending_all`, including held-unknown handling with history lookups and no clear command;
- `Ctx::run_step` (size, simulate, write-ahead before send, F3);
- the `minContextSlot` seed and the key-path guards (no default wallet).

Propose mirrors `cmd_propose`:
- it scans recent indexes for an identical live config transaction and refuses one;
- it resumes the proposal only when the config transaction exists without a proposal;
- `--expect-tx-index` is required.

**Tests**, LiteSVM with the REAL Squads v4 devnet binary and the production ELF, through `run_with_cluster`:
- **Happy path:** reduce (propose, approve twice); execute refused before the lock with zero sends; warp; execute; probe = `WrongGovernanceProfile`; a real G vault transaction carrying the no-op `rotate_recipient` fails `WrongGovernanceProfile`; restore at once; profile restored; probe clean; Squads read-back all true.
- **Refusals:** a stored action that differs (`ChangeThreshold`, `SetTimeLock{1}`); a live vault proposal; restore while at 172,800; reduce while at 0; an `--expect-tx-index` mismatch; a non-member or missing permission; an unknown action or an extra flag.
- **F3:** an ambiguous execute that landed ends held with zero further sends; a later `--execute` refuses; the history lookup resolves it confirmed; nothing is rebuilt.
- **Encoding:** both instructions match the committed Squads IDL (`assert_matches_idl`); the Borsh layout of `ConfigAction::SetTimeLock`; a config-transaction decode round-trip.
- **Stale:** the reduce stales an earlier unexecuted vault transaction, recorded as the real binary's behavior.

**Acceptance.**
- `check` and `clippy -D warnings` are clean, and `Cargo.lock` is unchanged.
- The new suite passes, run by Silas.
- The existing suites stay green: `devnet_ops_local`, `fund_wallets`, `m14_flow_tool`, `m14_flow_plan`.
- The new operator binary's sha256 is recorded.
- Gideon's bounded delta acceptance comes BEFORE any public F11 transaction. The accepted program, ELF, IDL, lock and packet stay byte-identical.
