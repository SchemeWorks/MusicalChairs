# M14 operator tool: Codex F3 unknown-outcome finding and adjudication (2026-10-24)

**Source:** Codex's bounded independent source review of frozen commit `81841a47dd282143e53277a922b1a7bdf51474e6`. Rob pasted it into the coordinator's chat. Codex did not duplicate any build or test.

**Codex confirmed present:**
- the N1 serialization guard;
- the durable journal append;
- the refusal to repeat a confirmed transfer;
- the fail-closed receipt reader.

## Finding (blocks public `--execute`)

There is still one F3 unknown-outcome path:

1. `RpcCluster::signature_outcome` (`solana/tests/financial-runtime/src/devnet_ops.rs` around line 1200) uses `get_signature_statuses`. By default that searches only the node's recent status cache ([Solana RPC docs](https://solana.com/docs/rpc/http/getsignaturestatuses)).
2. `wait_outcome` (around lines 2197–2213) runs once `blockhash_valid` is false. Its final lookup maps ANY result that is neither confirmed nor failed to `Resolution::Expired`. That includes an RPC error.
3. `resolve_pending_all` and the in-process ambiguous-send path then record `expired_never_landed` and allow a rebuild.

Concrete case:
- A transfer lands.
- The process dies before the resolution record is written.
- The operator resumes after the status cache has evicted the signature, or while the status RPC is failing.

An expired blockhash proves only that the transaction cannot land *now*. It does not prove that it never landed earlier. Because the once-only guard counts only confirmed records, `FundOperations`, `FundSeriesA` or `fund-vault` could be sent twice.

## Adjudication: ACCEPTED (coordinator confirmed in source)

The coordinator read the cited code at `e52d567`, which is unchanged from `81841a4` for these lines. It confirmed all of the following:
- The final lookup's `_ => Resolution::Expired` arm covers both `Ok(SigOutcome::Unknown)` and `Err(_)`.
- `signature_slot` also uses the recent-only lookup.
- Both callers write `expired_never_landed`.

The finding is valid and blocks any public execution request until it is fixed.

## Follow-up card M14-F3-HOLD (narrow, as Codex scoped it)

- **History lookup.** Positive confirmation or failure uses the historical lookup, `get_signature_statuses_with_history` (searchTransactionHistory = true).
- **Missing or error is never absence.** A missing status or an error is never treated as proof of absence. An expired-but-unresolved record stays HELD as UNKNOWN, with its original signature, and every `--execute` command refuses. Each run re-queries history.
- **Recovery restores the guard.** If a later history lookup finds the transaction confirmed, the record resolves as confirmed, which restores the once-only guard.
- **No rebuild and no clearing by the tool.** A held record clears only when a later history lookup *positively* finds the transaction confirmed or failed. The tool has no command that clears an unknown record.
- **Codex scope clarification (same day): release deferred.** The `pending release` path first drafted for this card is out of scope and was removed from the candidate. It is not part of the required fix, and Block A readiness does not need it. A successful history miss, an expired blockhash and an operator-written reason still cannot prove non-landing, and neither a CLI flag nor a reason creates Rob's authority to risk duplicate funds. If a held record never resolves, it is an incident: stop, report the signature and step, and gather information (recipient balance history, an explorer). Any retry then needs Rob's separate, exact approval of the signature, effect and retry, plus a review. It is never routine operator or coordinator discretion, and it is outside the ordinary Block A authority.
- **Focused regression.** The test models a transaction that already landed, a lost response or process, an expired blockhash, and a missing or error status. It asserts zero additional sends and no never-landed claim. A recovered historical confirmation must restore the once-only guard.
- **Budget wording.** Codex also asked for this. The 25 SOL total is a process budget that the coordinator keeps; `devnet_ops` does not track it. The packet now says so. Software enforces only the per-command ceilings: deploy at most 15 SOL on devnet, and a `fund-vault` total of at most 0.05 SOL.

## Resolution (fixed locally in the commit that adds this section; Codex confirmation requested)

All line numbers are in `solana/tests/financial-runtime/src/devnet_ops.rs`.

- **History lookups.** `RpcCluster::signature_outcome` (around 1213) and `signature_slot` (around 1251) call `get_signature_statuses_with_history`. `SigOutcome::Unknown` now means only "the lookup succeeded and found no status". A new `BelowCommitment` variant covers a status found below the client commitment.
- **Expired is gone.** `Resolution::Expired` and `expired_never_landed` are removed from the crate, the tests and the runbook. After expiry, `wait_outcome` (around 2254) closes a record only on a FOUND confirmed or failed status. A missing status, a below-commitment status or a lookup error gives `Resolution::Held`.
- **Held records stay open.** `mark_held` appends a non-closing `held_unknown` line. `read_pending` accepts it without closing or confirming the record.
- **Execute is refused.** `resolve_pending_all` refuses every `--execute` with an INCIDENT message: no clear command; any retry needs Rob's separate exact approval. Read-only and dry-run commands still run and print a warning listing the open and held records.
- **Ambiguous sends.** The in-process ambiguous-send path ends HELD with a `fail` and no rebuild.
- **Extra guard.** `refuse_repeat_of_confirmed` also refuses while an open record exists for the same non-idempotent step.
- **No release command.** There is no release or clear subcommand.

**Tests (coordinator-run):**
- `devnet_ops_local` 33/33 and `m14_flow_tool` 14/14. New and renamed tests:
  - `a_landed_transfer_with_no_resolution_a_dead_blockhash_and_a_missing_or_failing_status_is_held_and_never_sent_twice` (fund-vault);
  - `a_landed_funding_with_no_resolution_a_dead_blockhash_and_a_missing_or_failing_status_is_held_and_never_sent_twice` (FundOperations);
  - `a_held_record_that_history_later_shows_failed_resolves_failed_on_chain`;
  - `the_rpc_cluster_searches_transaction_history_and_tells_a_missing_status_from_a_found_one` (a loopback JSON-RPC server checks that every lookup sends `searchTransactionHistory: true`);
  - `an_ambiguous_send_whose_status_cannot_be_found_after_the_blockhash_dies_ends_held_and_is_never_rebuilt`;
  - two renamed tests that previously asserted `expired_never_landed` and now assert HELD with zero sends.
- The full 39-step plan through the runner: PASS.
- `m14_flow_plan`: 2/2.
- The real-node lifecycle on a fresh local validator: PASS in 25.43 s, 34/34 write-ahead records resolved, 0 held.
- check and clippy `-D warnings` are clean, and `Cargo.lock` is unchanged.

Summary: `docs/solana/evidence/2026-10-23-m14-preflight/m14-f3-hold-test-summary.txt`.

No M14 approval is implied by this record or by the fix. The graph worker and the local Lookout metadata are untouched.
