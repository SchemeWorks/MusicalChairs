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

No M14 approval is implied by this record or by the fix. The graph worker and the local Lookout metadata are untouched.
