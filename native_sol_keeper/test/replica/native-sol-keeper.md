# native_sol_keeper - local replica check plan (PLAN ONLY, not executed)

Status: written in IMPL-48b. **Nothing here has been run.** The pure modules are covered by the
standalone tests in `native_sol_keeper/test/*.test.mo`; the actor glue (`main.mo`: timers, SOL RPC
calls, `sign_with_schnorr`, journaling around the awaits) can only be exercised on a replica. This
plan says exactly what to do, what to look at, and what stays unproven until M14.

Rules for whoever runs it:

- LOCAL replica only: every command carries `--network local`; there is no `--network ic` anywhere.
- The shared local network may be in use by another worker. Create the canister under its own name
  (`native_sol_keeper`), never reinstall or touch `pp_ledger`, `siws_provider` or the other canisters.
- No real keys, no funds, no mainnet or devnet endpoint. The "providers" in `setConfig` are `*.invalid`
  names that only a stub canister interprets, exactly as `native_sol_bridge/test/replica/` does.
- Do not paste any credential into a command line, config or note. The local threshold key id is a public
  string (`dfx_test_key`), not a secret.

## 0. Prerequisites

| need | note |
| --- | --- |
| dfx 0.29.x with a running shared local network | `dfx ping local` |
| a SOL RPC stub canister that answers the six methods the keeper calls | `getAccountInfo`, `getBalance`, `getSignatureStatuses`, `getSlot`, `getBlock`, `sendTransaction`, with the pinned types in `native_sol_keeper/Rpc.mo`. `native_sol_bridge/test/replica/sol_rpc_stub` answers two other methods, so a keeper stub is a small extension (scripted Pool / Ticket / CloseRecord bytes as base64, a fixed blockhash and height, a send log). Install it at `tghme-zyaaa-aaaar-qarca-cai` with `--specified-id`, as the bridge run does. |
| threshold Ed25519 on the local replica | UNKNOWN. The ICP budget doc warns that local threshold signing must not be assumed. If `schnorr_public_key` fails with `dfx_test_key`, steps 3-5 still hold for everything except the derived address; record that and use the pure tests for the signing path. |

## 1. Install and configure

```bash
dfx canister create native_sol_keeper --network local
dfx build native_sol_keeper --network local
dfx canister install native_sol_keeper --network local --mode install
dfx canister call native_sol_keeper configTemplate '()' --network local     # copy, edit, then:
dfx canister call native_sol_keeper setConfig '(record { ... })' --network local
dfx canister call native_sol_keeper getConfig '()' --network local          # rpcSources must NOT be echoed
```

Expect `setConfig` from a non-controller identity to answer `err "controllers only"`. Use
`clusterDomain` = the 32 bytes of the stub's Pool (`musical-chairs-localnet-dev!!!!!` for the local
cluster fixtures) and `feeKeyName = "dfx_test_key"`.

## 2. Fee address, derived once

```bash
dfx canister call native_sol_keeper getFeeAddress '()' --network local   # ok "<base58>"
dfx canister call native_sol_keeper getFeeAddress '()' --network local   # the SAME address (stored, not re-derived)
dfx canister call native_sol_keeper getStatus '()' --network local        # feeAddress = ?that address
```

Then `setConfig` again with a different `feeKeyName`: `getStatus.feeAddress` must become `null` (the old
address belongs to another key) and `arm` must answer `err "derive the fee address first"`.

## 3. Arm, observe, status

```bash
dfx canister call native_sol_keeper arm '()' --network local             # ok
sleep 40
dfx canister call native_sol_keeper getStatus '()' --network local
```

Expect `armed = true`, `lastTimerAt` advancing every 15 s (`timerAgeNs` below 30 s), `lastTickAt`
advancing at the configured poll cadence (`idlePollNs` while the stub Pool is idle), and for an idle stub
Pool `workPending = false`, `level = #Ok`, `decision = "#Idle"`.

## 4. THE UPGRADE CHECK (live version of `rearms_timer_after_upgrade`)

`Timer.recurringTimer` is NOT persisted across an upgrade of a persistent actor; the stable `armed` flag is.

```bash
# before
dfx canister call native_sol_keeper getStatus '()' --network local > before.txt
dfx canister call native_sol_keeper getConfig '()' --network local > config-before.txt

dfx canister stop native_sol_keeper --network local
dfx canister snapshot create native_sol_keeper --network local            # reversible backup
dfx canister install native_sol_keeper --network local --mode upgrade --wasm-memory-persistence keep
dfx canister start native_sol_keeper --network local
sleep 40

# after
dfx canister call native_sol_keeper getStatus '()' --network local > after.txt
dfx canister call native_sol_keeper getConfig '()' --network local > config-after.txt
diff config-before.txt config-after.txt
```

PASS when ALL hold:

1. `armed = true` in `after.txt` (stable flag survived).
2. `lastTimerAt` in `after.txt` is LATER than the moment `dfx canister start` returned, and `timerAgeNs`
   is below 30 s - the timer was re-armed by `postupgrade`. (Without it `lastTimerAt` stays frozen at the
   pre-upgrade value and `getStatus.level` escalates to `#Alert` once `timerAgeNs` exceeds 60 s, because
   an armed keeper that is silent is reported as dead.)
3. `feeAddress`, `journalTail`, `configured`, `signaturesLastHour` are unchanged; `getConfig` is identical.

Negative control: `disarm`, upgrade the same way, wait 40 s: `armed = false` and `lastTimerAt` does NOT
advance (a disarmed keeper must not be resurrected by an upgrade).

If anything differs, restore the snapshot (`dfx canister snapshot load`) before investigating.

## 5. Threshold walk with the stub (needs working local signing)

Script the stub's Pool + Ticket (phase Discover, cursor 0, captured_len 1, registry page 0 and snapshot
page 0 with one Backer) and keep the progress marker constant:

| elapsed with the marker unchanged | expected `getStatus.level` | expected `decision` |
| --- | --- | --- |
| 0-59 s | `#Ok` | `#Idle` |
| 60-299 s | `#Alert` | `#Alert` |
| 300 s | `#Escalate` | `#Sign` and `lastOutcome` = `submitted discover_rows|t1|c0 rows=1 <sig>` |

Then inspect the stub's send log: exactly ONE `sendTransaction`, a base64 legacy transaction whose message
parses with the fee address as account 0 and the only signer, one `discover_rows` instruction (plus no
ComputeBudget instruction while `computeUnitPriceMicroLamports = 0`), no other program. `getJournal(10)` shows
one entry (`#Submitted` after the stub acknowledged it).

Failure injection, one at a time (each must leave NO second signature):

- stub `sendTransaction` answers an HTTP-outcall error -> entry `#Unknown`, `decision = "#Blocked"`,
  `blockedReason = "unresolved unknown journal entry"`; a later tick re-broadcasts the IDENTICAL bytes
  (same signature in the stub's log) while the stub's height is within `lastValidBlockHeight`, and only
  after the stub's height passes it does the entry become `#Expired` and a new transaction (one-row
  fallback) become possible;
- stub answers a JSON-RPC error (simulation failure) -> entry `#Failed`; the next attempt at the same
  position is ONE row; the third failure blocks the position (`position failed 3 times`);
- fee address balance (stub `getBalance`) below `minFeeBalanceLamports` + fee -> `lastOutcome` starts with
  `blocked: fee address balance`, no signature spent;
- `maxSignaturesPerHour = 1` -> the second attempt inside the hour is `hourly signature budget spent`;
- marker keeps moving (stub advances the cursor every 30 s) -> never `#Sign`;
- the stub flips `Pool.cluster_domain` -> `blockedReason = "cluster domain mismatch..."`.

## 6. Interface check (no method takes bytes to sign)

```bash
moc --idl --package base <base@0.16.0/src> native_sol_keeper/main.mo -o /tmp/keeper.did   # scratch dir in practice
```

The service must list only: `arm`, `configTemplate`, `disarm`, `getConfig`, `getFeeAddress`, `getJournal`,
`getStatus`, `setConfig`, `tickNow`. The only `blob` among method inputs is `Config.clusterDomain` (a 32-byte
cluster label, never signed). `sign_with_schnorr` appears once in `main.mo` (`signMessage`).

## 7. Cycle accounting to record

`getStatus.cyclesBalance` before/after a run of N ticks and M signatures: per signature about
26.15 billion cycles at the production key (`docs/coordination/native-solana-icp-budget.md`), plus the
SOL RPC calls (`rpcCycles` is an upper bound per call; the unspent part is refunded). Calibrate
`rpcCycles` / `signCycles` with `*CyclesCost` queries on the real canister during M14 and write the
numbers into `docs/solana/operations.md`.

## What this plan does not prove

Behaviour against a real RPC provider, real consensus disagreement between providers, the real
`sign_with_schnorr` latency against the blockhash age (a finalized blockhash leaves roughly 40 s), the
Solana Clock versus ICP time skew at the 600 s deadline, and anything on devnet or mainnet. Those belong
to M14 (authorization gate).
