# M14 operator tool and flow runner: independent read-only review (2026-10-23)

**Target:** commit `da3de8ce06eab06c90e6d962e015e57993214c64`.

**Reviewer:** a fresh Sonnet agent using the repository's `musicalchairs-protocol-security-reviewer` profile. It is distinct from the tool's author and from the coordinator's runs. The review was read-only: no network, no keypair reads, no builds or tests, no git writes. It also read the vendored `solana-rpc-client-2.3.1` source for `send_and_confirm_transaction`.

**Verdict: PASS with findings.** Nothing lets the tool target mainnet, read an implicit wallet, generate a key, or skip the ELF pin or the G-profile gate. Before the first devnet `--execute`, fix F1 (approval wording and binding), F2 and F3 (the only double-send paths) and F4 (receipt "done" semantics). The rest are low severity.

## Items

1. **Authority binding: PASS on mechanics; finding F1.** `--i-have-approval` equals the sha256 of the packet file bytes. That is an artifact-identity check: it proves which text, not that Rob approved it. The code, help text, runbook and packet wording overstate it. `--packet` accepts any file. The hash is of a mutable working file. Executed receipts do not record the packet path or hash.
2. **Refusal ordering: PASS.** On every network path the order is: gate (cluster name, URL shape), then the identity token, then `getGenesisHash` and the genesis check, then key loads, signing and sending. Only `Ctx::send_signed` sends, and it requires the token. Mainnet is refused both by cluster name and by genesis hash, including loopback tunnels. Two offline exceptions read key files without a genesis check (`flow init`, and `validator-fixtures --g-create-key`); neither can send.
3. **No default wallet: PASS, narrow guard (F7).** `refuse_default_wallet` runs on the given path and its canonicalized form before any key read. No key flag has a default, and the tool generates no keys. The guard does not cover `--packet`, `--elf`, `--program-so` or `--receipts`.
4. **Ceilings: finding F5.** These are per-call defaults, not enforced totals: `--max-spend-lamports` can raise the 15 SOL deploy cap, `fund-vault` is capped per call only, and nothing tracks the 25 SOL total. Flow amounts are constants in the plan.
5. **Size and simulation gating: PASS.** Every transaction is measured unsigned (≤ 1,232 bytes) before any signature exists and simulated before send. `vault_transaction_create` and execute are always member-paid (proposer = fee payer).
6. **Ownership: PASS.** SetAuthority to the G vault happens only after the ELF report is OK, the current authority is the deployer, and G passes the full profile check; the result is read back. Approve and execute refuse unless the stored vault message equals the rebuilt instruction. Execute enforces the time lock on the cluster clock. ORAO and Pool ownership are read back.
7. **Unknown outcome and resume: findings F2, F3, F8.** The tool sends once and never re-sends original bytes. A transport error returns `Failed` with no signature, and there is no write-ahead record. Most steps are idempotent by on-chain state, but `fund-vault`, `FundOperations` and `FundSeriesA` are not.
8. **Step, time and wallet guards: PASS (receipt issue under F4).** Only the next step is accepted. The earliest time is checked against the cluster clock. The wallet pubkey is matched before signing. `--local-oracle` and `--genesis-unix` are loopback only, and the oracle is never fulfilled on devnet.
9. **Receipt truthfulness and conservation: PASS on the coordinator's fee fix; findings F4 and F6.** Read-backs and the three identities are computed from RPC state. Receipts mark any step done if its id appears, with no cluster scoping. `result:"done"` is recorded even when `all_ok` is false. `fund-vault` and `approve` hard-code `all_ok: true`.

## Findings

- **F1 (Medium):** identity-only flag documented as approval; `--packet` override; mutable file; not recorded in receipts.
- **F2 (Medium):** a stale post-send read on a load-balanced RPC can re-send a non-idempotent step (FundOperations, FundSeriesA) inside one `flow next`.
- **F3 (Medium):** no write-ahead signature or unknown-outcome polling; a rerun rebuilds while the first transaction may still land; `fund-vault` has no state guard.
- **F4 (Low-Medium):** receipt-derived "done" is neither scoped nor cluster-bound; a failed-check step does not block the next one.
- **F5 (Low):** ceilings are defaults, not totals.
- **F6 (Low):** hard-coded `all_ok`; no post-deploy deployer balance recorded.
- **F7 (Low):** the default-wallet guard does not cover the other file arguments.
- **F8 (Low):** the duplicate-live-proposal guard sees only the newest index.
- **F9 (Informational):** the genesis hash is self-reported by whatever endpoint is named. This is residual risk.

The fixes are assigned to card M14-TOOL-FIX (the tool's author), with a regression test for each of F1–F4. The genuine human approval boundary is unchanged: Rob's explicit approval is the authority, and the hash flag is not.
