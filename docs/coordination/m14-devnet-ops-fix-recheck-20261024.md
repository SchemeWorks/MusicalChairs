# M14 operator tool: bounded re-check of the F1–F9 fixes (2026-10-24)

**Target:** commit `d2dffc9f1b86fa12f2e8876d65be100e3626f2f4` (the fixes for the review of `da3de8c`, recorded in `m14-devnet-ops-independent-review-20261023.md`).

**Reviewer:** a fresh Sonnet agent with the repository's `musicalchairs-protocol-security-reviewer` profile. It is not the author of the tool or of the fixes, and it is a different instance from the first reviewer. The re-check was read-only. It made no builds, test runs, network calls, keypair reads or git writes. It read the fix diff `0ae0f95..d2dffc9`, the touched regions, and vendored `solana-rpc-client(-types) 2.3.1` and `serde` source.

**Verdict: PASS with residual findings.** No double-send path remains inside one `flow next`, and nothing regressed. Specifically:
- refusal order: gate, then host and packet pin, then the pending file, then genesis, then pending resolution, then key loads;
- mainnet refused by name and by genesis;
- no default wallet;
- one send choke point (`Ctx::send_signed`).

## Item results

| Item | Result | Evidence (in `solana/tests/financial-runtime/`) |
|---|---|---|
| F1 identity vs approval | CLOSED | Pin hashes `git show <40-hex commit>:<constant path>`, so neither option injection nor traversal is possible. The working file must equal the blob. `--i-have-approval` is a usage error and `--packet` is refused. The pin and the host allowlist are enforced in `execute_token_in` before genesis or any send. Receipts record the pin. Docs, help, errors, runbook and packet all say "not approval". |
| F2 stale reads / double send | PARTIAL | Single-send break for single-transaction acts: CLOSED. The `minContextSlot` plumbing has defects (N1, N2). |
| F3 write-ahead / unknown outcome | PARTIAL | Record written before send, failed write stops the send, required on devnet, polling on ambiguity, `resolve_pending_all` before every subcommand, `fund-vault` guards before send: CLOSED. Durability gap: N3. |
| F4 receipts | CLOSED | Stamp of genesis, program id and ConfigVersion#1 activation. Foreign or unstamped receipts are refused. Only the traceless step can be done from a receipt. `done_with_failed_checks` blocks the next step. Info: `has()` accepts any result string for the traceless step. |
| F5–F9 | CLOSED / residual documented | 15 SOL devnet deploy ceiling refused, `max_data_len` justification, computed `all_ok`, post-deploy balance, file guards, 32-index proposal scan, F9 residual in the runbook. The 25 SOL program total is still untracked (Info). |
| Regressions | none | The new tests are not tautological. The pin tests use a real throwaway git repo. The write-ahead tests assert on `pending.jsonl`. The stale-read test would have double-sent on the old code. Coverage gap: the test cluster ignores `minContextSlot`. |

## Residual findings

- **N1 (Medium; independently confirmed by the coordinator from the vendored types).** `RpcCluster::config_json` nests `{"commitment":{"commitment":"confirmed"}}`. `RpcAccountInfoConfig.commitment` is `#[serde(flatten)] Option<CommitmentConfig>`, so the nested object fails silently to `None` and post-send account reads run at *finalized*. This is consistent with the real-node lifecycle slowing from 31.38 s to 120.13 s. It fails closed: a stalled finalization past the 30 s catch-up cap loses the step's receipt, but nothing is double-sent and no funds are lost. Fix: build the typed configs directly, plus a serialization test.
- **N2 (Low).** `min_slot` is silently not advanced when the signature's slot can't be learned, and it restarts at 0 on every CLI run. A re-run of a non-idempotent step (FundOperations, FundSeriesA, `fund-vault` up to its 0.05 SOL cap) against a lagging load-balanced node could therefore send twice. Fix:
  - poll for the slot, then fall back to the slot read after confirmation, and fail closed if both fail;
  - seed `min_slot` from confirmed write-ahead records;
  - refuse non-idempotent acts that already have a confirmed record.
- **N3 (Low).** There is no `fsync`, and the line is written in two `write(2)` calls. Unparsable lines and non-NotFound read errors are silently skipped, and the pending-file path is free-form per command. Fix:
  - single write plus `sync_data`;
  - fail closed on parse and read errors;
  - the runbook requires the same `--pending-file` for every command.
- **N4 (Info).** Write-ahead records do not carry the packet pin. Fix: add it.
- **Info.**
  - Stale wording: the packet's "is being renamed", the M14-TOOL and M14-RUN kanban rows, and the `devnet_ops_local.rs` comments.
  - `flow init` and `validator-fixtures` silently ignore the removed flags (offline, so no impact).
  - `has()` should require `refused_as_expected`.

## Not verified read-only

- Nothing was built or run.
- The serde flatten behaviour was read from source. The coordinator confirmed the type shape: `CommitmentConfig { commitment: CommitmentLevel }` flattened, with no `#[serde(flatten)]`-tolerant nesting.
- Live load-balancer staleness, devnet finalization timing and macOS fsync semantics were not exercised.

## Disposition

N1–N4 and the info items went to card M14-TOOL-FIX2 and are fixed.

Fixes:
- **N1.** The read config carries `commitment` at the top level. The config is serialized back and refused if any requested key was dropped, because the typed RPC config lives in a crate that is not a direct dependency and adding it would change `Cargo.lock`.
- **N2.** The tool polls for the slot, falls back to a slot read after confirmation, and fails loudly if neither works. `min_slot` is seeded from confirmed write-ahead records. `fund-vault`, FundOperations and FundSeriesA are refused once a confirmed record exists.
- **N3.** Each record is written once and then `sync_data`. Unreadable files and unparsable lines refuse, with the line number. The runbook sets the same-`--pending-file` rule.
- **N4.** The packet pin is in every write-ahead record.
- **Info.** `traceless_done` requires `refused_as_expected`. The offline subcommands refuse the removed flags. The stale wording is fixed.
- **Coordinator addition.** The flow receipt reader fails closed on unreadable or unparsable lines, with a test. The worker had flagged it as the same fail-open pattern.

Tests:
- `devnet_ops_local`: 30 passed (2 ignored by design).
- `m14_flow_tool`: 12 passed (1 ignored).
- Full plan through the runner: PASS.
- `m14_flow_plan`: 2 passed.
- Real-node lifecycle: PASS in **24.25 s**, down from 120.13 s, which confirms N1 empirically. 34/34 write-ahead records were resolved.
- check and clippy `-D warnings` are clean, and `Cargo.lock` is unchanged.

The worker temporarily reverted each fix and saw the new tests fail. These fixes were verified by the coordinator, not by a third independent review. The genuine human approval boundary is unchanged: Rob's explicit approval in chat is the authority, and the packet pin is an identity check only. No public-cluster action has been taken.
