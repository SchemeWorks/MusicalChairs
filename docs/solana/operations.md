# Native SOL operations runbook (plan M13)

Status: code and documentation only. **No keeper, worker, canister, key or transaction has been started,
deployed or funded**, and nothing here authorizes doing so (M14 is the authorization gate). Everything below
describes how the two keepers are designed to be run, which numbers are measured and which are planning
assumptions, and what a human does when processing stalls.

Related: [threat model](threat-model.md) (keeper rows), [round state machine](round-state-machine.md),
[measurements](measurements-2026-10.md), [ICP budget](../coordination/native-solana-icp-budget.md),
watchdog code `native_sol_keeper/`, worker code `solana/keepers/worker/` (IMPL-48a).

## 0. No credentials in this document (rule)

This file, its commits, tickets and chat threads must never contain a private key, seed phrase, keypair file
content or path to one, API key, token, password, or an RPC URL that embeds a key. Addresses (program id,
PDAs, the watchdog fee address) are public and may be written down. RPC sources with embedded keys are set
only by an ICP controller through `setConfig`; `getConfig` never returns them. The commands below use
placeholders (`$RPC_URL`, `$FEE_PAYER_KEYPAIR`) that the operator fills from their own secret store at run
time. If a credential is ever pasted anywhere, treat it as compromised and rotate it before anything else.

## 1. Roles

| role | what it is | what it may do | what it can never do |
| --- | --- | --- | --- |
| **External primary worker** | `solana/keepers/worker`, a small Rust CLI run by the sponsor on a normal host | polls on-chain markers every 30-60 s and sends the next permissionless step: ticket phases, candidate pages, draw request/capture, apply/expire, finalize; keeps a journal and a metrics file | hold any authority: its key is a plain ed25519 fee payer that holds only operations SOL |
| **ICP watchdog** | `native_sol_keeper` canister | monitors the same markers through the SOL RPC canister; after 300 s without progress signs ONE step at a time with a threshold Ed25519 key; exposes `getStatus` for an external monitor | sign arbitrary bytes (no public method takes any), appear in an instruction as anything but the fee payer, perform provisioning / candidate / draw steps |
| **Any wallet** | public recovery | run the same permissionless instructions (the web app's "Resume processing" panel, the worker CLI with a throwaway key, or hand-built instructions from the IDL) | nothing special is required: there is no keeper lease, no privileged key path and no original-keeper dependency |
| **ICP controllers** | the principals that control the watchdog canister | `setConfig`, `arm` / `disarm`, `tickNow`, upgrades | change economic state: the canister can only pay fees and choose when to act |
| **Sponsor / operator** | people | fund the two fee addresses with operations SOL, top up cycles, run the worker host, watch the alerts | spend game money |
| **G / P** | Squads 2-of-3 governance (48 h delay) / guardian (no delay) | G: configuration within the pins; P: pause admission only | move pot, seed, claims or locked partitions |

There is no leader election. Two keepers sending the same step is harmless by construction: every phase
instruction carries `expected_ticket_id` and `expected_cursor`, so the second one fails fee-only. That is also
why the watchdog waits the full 300 s before acting: it is a backup, not a second primary.

## 2. What the two thresholds mean

- **Progress** is a change of the on-chain progress marker: round id, allocation ticket id/phase/cursor, and
  the close stage (cash committed, candidates sealed, draw state, promotion outcome, reserve settled,
  candidate cursor). Money fields are not part of it. An idle pool (no open ticket, no close in progress)
  has nothing to progress and never alerts.
- **60 s without progress while work is pending = ALERT.** A signal to look, not to act.
- **300 s without progress = ESCALATE.** The backup may now act and a human should be looking.
- They are monitoring thresholds, **not completion guarantees**: no wall-clock bound on processing is
  promised by either keeper or by the program. The one hard on-chain time is separate: the promotion deadline,
  `close_started_at + 600` Solana Clock seconds, pinned once. Missing it never erases a cash allocation; it
  only lets anyone expire the randomized promotion (`PromotionSkipped(Deadline)`) so the round can finalize.
- **Watchdog specifics** (defaults from `configTemplate`): the recurring timer fires every 15 s; it polls
  every 30 s while work is pending and every 300 s when idle, so its stale clock starts at its first
  observation and can lag the true start by up to the idle interval (shorten `idlePollNs` if that
  matters; each poll costs cycles). Its own landed steps do not reset the stale clock (otherwise it would
  wait another 300 s after every step it lands); progress it did not cause does, and ends the rescue. In a
  rescue it sends at most one transaction at a time and about one per minute.
- **Worker specifics:** the same thresholds, in seconds, from `metrics.rs`; it never acts on its own alert
  (it is the primary: it acts every pass), the alert is for the monitor.
- **Commitment differs by keeper.** The watchdog reads and plans from `finalized` state only (a read lags the leader by roughly 13 s or more, and it never plans from state that could still be rolled back). The worker reads at `confirmed` and judges blockhash expiry by finalized block height (`solana/keepers/worker/src/rpc.rs` at the time of writing). The two can therefore see a marker move up to a quarter of a minute apart; that is expected and harmless.

## 3. Escalation runbook

Do these in order and stop as soon as the marker moves again. Nothing here requires a privileged key.

**Step 1 - check the keeper journals.**
- Watchdog: `dfx canister call native_sol_keeper getStatus '()' --network ic` and `getJournal '(10)'`.
  Read `decision`, `blockedReason`, `lastOutcome`, `lastError`, `journalTail`. Common causes:
  `unresolved unknown journal entry` (a send errored; it resolves by a status read or by blockhash expiry,
  within about two minutes), `hourly signature budget spent`, `fee address balance ... below the floor`, `unsupported step: ...`
  (the stall is in a step only the primary or a wallet can run: see step 3), `cluster domain mismatch`
  (misconfiguration: stop and fix the config), `position failed 3 times: operator action needed`.
- Worker: open the `--metrics-json` file and the `--journal` file. `unresolved_journal_entries > 0` or
  `last_action` ending in an error points to the same causes.

**Step 2 - check the validator / RPC.** From any machine, with a provider URL from your own secret store:

```bash
for m in getHealth getVersion; do
  curl -s "$RPC_URL" -H 'content-type: application/json' -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"$m\"}"; echo
done
curl -s "$RPC_URL" -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"getSlot","params":[{"commitment":"finalized"}]}'
```

Run the `getSlot` read against at least two independent providers: a finalized slot that does not advance,
or providers that disagree by more than a minute, is an RPC or cluster problem, not a keeper problem. On
the ICP side, `getStatus.lastError` shows `getAccountInfo: providers disagree ...` when the SOL RPC canister
sees inconsistent answers. Cluster-level trouble: switch the keeper to another provider set (worker
`--rpc`, watchdog `setConfig`) and keep watching; do not pause for it (step 4).

**Step 3 - run the public recovery instructions from any wallet.** The program has no privileged keeper, so
any funded wallet can send the next step. Use the web app's "Resume processing" panel, or the worker with a
throwaway fee payer (`once`, first with `--dry-run`), or build the instruction from the IDL. The next step is
determined by on-chain state alone:

| on-chain state (pool / ticket / close record) | next instruction | watchdog can send it? |
| --- | --- | --- |
| allocation ticket open: phase Discover / Senior / Seal / Allocate / Prune / Commit | `discover_rows`, `apply_senior`, `seal_weights`, `allocate_rows`, `prune_rows`, `commit_ticket` (batches of at most 8 rows within one page, one row after a failure) | yes |
| close in progress, cash not committed | wait: the closing ticket above commits the cash | n/a |
| candidate pages missing | `provision_page` (payer pays rent, or locked-close funding) | no (needs a payer) |
| candidates not sealed | `build_candidates` (batches inside participant AND candidate page edges) | no |
| sealed, draw not requested | `request_round_draw` (payer; ORAO accounts) | no |
| draw requested | `capture_round_draw` once ORAO has fulfilled (`observe_request_fulfilled` after) | no |
| draw captured | `apply_promotion` (payer; applies even after the deadline) | no |
| deadline passed and draw not captured | `expire_promotion` | yes (after `deadlineMarginSecs`) |
| promotion terminal, reserve settled | `finalize_round` | yes |
| after finalize | optional cleanup: `close_shared_account`, `deliver_owner_refund`, `reconcile_ops_cash*` | no |

If a wallet and a keeper race, the loser fails fee-only. Never "retry harder": check `getStatus` /
the worker journal first, because a transaction with an unknown outcome must be read before it is replaced.

**Step 4 - pause admission via P only for these conditions.** The guardian pause (`set_pause`, Squads P, no
delay) stops NEW deposits and Series A funding only. Exits (payouts, claims, position close, ticket progress)
keep working while paused, so a pause cannot help a stalled keeper and must not be used for one. Pause when:

1. a conservation break is observed on finalized state (`vault - rent != pot + seed + management + pending_toll
   + funded_claims + excess`, or the OpsCash partition identity);
2. an exploit of an admission or accounting path is suspected or reported, until triaged;
3. the oracle provider announces a compromise or fulfilment anomalies appear that affect new closes;
4. a client defect would mis-submit deposits (wrong cluster, program id or config) until it is fixed;
5. a cluster-level incident (halt, fork or rollback, providers disagreeing for a sustained period) makes new
   deposits impossible to observe or confirm reliably.

Do **not** pause for: a keeper stall, slow or failing RPC, low fee balance or cycles, a single failed
transaction, an ICP incident, or compromise of a keeper fee key (those keys hold no authority). This list is a
proposed operating policy for the owner's acceptance; the spec only fixes what P can do, not when to use it.

**Step 5 - after recovery.** Confirm the marker advances and `level` returns to `#Ok` / `none`; write down the
cause, the first-alert time and the step that fixed it. A watchdog that signed leaves its entries in
`getJournal` (bounded to the last `journalMax`).

## 4. Metrics and JSON fields

### 4.1 Watchdog: `getStatus` (query, no cycles spent by the caller)

| field | meaning |
| --- | --- |
| `level` | `#Ok`, `#Alert` (stale >= `alertAfterNs`, or armed but its timer is silent), `#Escalate` (stale >= `signAfterNs`) |
| `staleForNs` | time since the marker last moved for a reason other than the watchdog's own landed step; 0 when nothing is pending |
| `workPending`, `marker` | whether a ticket or close is waiting, and the text marker (`r<round>|t<ticket>.<phase>.<cursor>|k<close>.<flags>.<cursor>`) |
| `lastSignature`, `journalTail` | the newest transaction id; the last 10 entries (`signature, step, position, rows, blockhash, lastValidBlockHeight, status, at, updatedAt, note`; no raw bytes). Status: `#Submitted #Unknown #Confirmed #Finalized #Failed #Expired` |
| `decision`, `blockedReason`, `lastOutcome`, `lastError` | last tick: `#Idle #Alert #Blocked #Sign` with the reason; one line about the last transaction attempt; last read error |
| `armed`, `configured`, `feeAddress` | stable arm flag; config present; the stored fee address |
| `lastTickAt`, `lastTimerAt`, `timerAgeNs` | last poll that ran; last timer firing; age of the timer (silent while armed means dead) |
| `signaturesLastHour` | signatures in the trailing hour (against `maxSignaturesPerHour`) |
| `cyclesBalance`, `nowNs` | canister cycles; the query time |

### 4.2 Worker: `--metrics-json` (field names as committed in `solana/keepers/worker/src/metrics.rs`, a93d4d0)

`now`, `last_progress_at`, `last_progress_marker`, `seconds_since_progress`, `alert_level` (`none` | `alert` |
`escalate`), `pending_work`, `unresolved_journal_entries`, `last_action`, `dry_run`. The journal is JSON lines
(`signature, blockhash, last_valid_block_height, step, bytes_sha256, status, at`).

### 4.3 What the external monitor should page on

The watchdog has no outbound HTTP by design, so a monitor must poll it. Page on: watchdog `level != #Ok`;
worker `alert_level != none`; `armed = false` or `feeAddress = null` while the program is live;
`timerAgeNs` above 60 s; `cyclesBalance` below the top-up line (section 5); either fee address below its
funding line (read with `getBalance` from any Solana RPC: the watchdog only reads it when about to sign);
`journalTail` showing `#Failed` / `#Expired` / `#Unknown` older than 5 minutes.

## 5. Fee-address funding floor and hourly budget

Fee addresses pay **network fees only**. Game money, rent for pages and the oracle fee are paid by program
accounts (OpsCash) or by the payer of the specific instruction, never by the watchdog's fee address, because
the watchdog sends no payer instruction. Fund a fee address with ordinary SOL from an operator wallet; this is
unrelated to `fund_operations` (which funds OpsCash). Amounts are replenishable operating money, not a
guarantee.

| quantity | value | status |
| --- | --- | --- |
| base fee | 5,000 lamports per signature (one signature per keeper transaction) | planning assumption, not a quote |
| priority fee | 0 by default (`computeUnitPriceMicroLamports = 0`: no ComputeBudget instructions); if set, `ceil(limit x price / 1e6)` with limit 200,000 (prune 300,000) | opt-in |
| watchdog funding floor | `minFeeBalanceLamports` = 10,000,000 (0.01 SOL): it will not sign unless the balance covers the floor PLUS that transaction's fee | default |
| watchdog per-transaction ceiling | `maxFeeLamportsPerTx` = 10,000 | default |
| watchdog hourly budget | `maxSignaturesPerHour` = 60 | default |
| worst-case watchdog burn | 60 x 10,000 = 600,000 lamports per hour (0.0006 SOL/h, 0.0144 SOL/day) | arithmetic on the defaults |
| suggested watchdog funding | floor + about 3 days of worst-case burn = about 0.06 SOL; the address spends nothing while the primary is healthy | recommendation |
| worker fee spend (pilot) | about 42,200 keeper transactions a month core = 0.211 SOL/month at 5,000 lamports; about 0.2126 SOL with provisioning and cleanup; priority fees extra | measured count (`measurements-2026-10.md`), assumed fee |
| refill line | refill a fee address when it holds less than 3 days of its expected spend plus the floor | recommendation |

ICP cycles: each threshold signature costs 26,153,846,153 cycles at the production key; the hourly budget caps
the signing cost at 60 x that, about 1.57 trillion cycles an hour worst case, plus the SOL RPC calls
(`rpcCycles` per call, default 10 billion as an upper bound, unspent cycles are refunded) and idle polling.
Defaults for `rpcCycles` and `signCycles` are placeholders to be calibrated with the `*CyclesCost` queries
during M14. A top-up line of at least 24 hours of worst-case signing (about 38 trillion cycles) is a reasonable
starting point until real burn is measured; a cycles monitor such as CycleOps can poll `cyclesBalance`.

## 6. What is NOT automated

- Funding either fee address, topping up cycles, rotating or replacing a fee key (a new `feeKeyName` or a
  worker key means a new address to fund; the watchdog re-derives and `arm` needs the new address).
- In the watchdog: `provision_page`, `provision_close_record`, `build_candidates`, `request_round_draw`,
  `capture_round_draw`, `apply_promotion` (all `#Blocked "unsupported step: ..."`), plus cleanup,
  `deliver_owner_refund`, `observe_request_fulfilled` and `reconcile_ops_cash*`. The primary or a wallet runs them.
- Standby election, leases, automatic failover between providers, idle backoff beyond the two fixed intervals.
- Alert delivery (the watchdog has no outbound HTTP; a monitor must poll), paging, on-call.
- Pausing (P), configuration staging and activation (G), provider returns, upgrades of the program or of the
  canisters, governance preflights.
- Any decision about a transaction with an unknown outcome beyond the journal rules: the watchdog never
  replaces it before its status is read or its blockhash has expired.
- Calibration of cycles and fee numbers on a real cluster (M14).

## 7. Bring-up and change control (not performed)

Gated by M14. Order when authorized: build and review the exact wasm; create the canister under controllers
the owner names; `setConfig` (RPC sources set by a controller, never echoed); `getFeeAddress`; fund it; run the
replica checks in `native_sol_keeper/test/replica/native-sol-keeper.md` against the stub; devnet acceptance;
only then `arm`. Upgrades follow the snapshot-protected recipe: stop, snapshot, upgrade, start, then confirm `armed`, `feeAddress`, `journalTail` and `lastTimerAt` (the timer is
re-armed by `postupgrade` from the stable `armed` flag; timers themselves are not persisted).

## 8. Solana notes for readers coming from ICP / EVM

- **Fee payer vs authority.** On ICP the caller principal is both who pays cycles and who is authorized; on EVM
  `msg.sender` pays gas and is the actor. On Solana the transaction's first signer pays the network fee, and
  authority is a separate thing: an instruction lists the signers or PDAs it requires. The keeper instructions
  list none, so the watchdog's key is only ever message account 0. This is what `fee_address_has_no_program_authority`
  proves from the IDL.
- **Derived addresses (closest to EVM CREATE2).** Page and close-record addresses are derived from seeds and the
  program id. Motoko has no Ed25519 verification, so `native_sol_keeper/Pda.mo` decides "is this hash a valid
  curve point?" with big-integer arithmetic and is pinned against `@solana/web3.js` vectors.
- **Recent blockhash instead of a nonce.** A Solana transaction names a recent blockhash and is valid for 150
  blocks. After `lastValidBlockHeight` it can never land, which gives a clean proof that an unknown-outcome
  transaction is dead; until then the only safe move is to re-send the identical bytes.
- **Finalized observations.** The watchdog acts only on finalized state, so its view lags the leader by roughly
  a quarter of a minute; it never plans a step from state that could still be rolled back.
