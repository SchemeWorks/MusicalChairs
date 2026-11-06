# Negative controls for the watchdog keeper (IMPL-49e, Codex review R5)

**What these are.** Each control is a *valid, type-correct* module with the same public API as the real one and exactly one deliberate wrong behaviour, run through the *same assertions* as the real suite. A control is satisfied when the wrong variant is **distinguished**: the run ends in the suite's own `check(s) FAILED` trap with exactly the FAIL lines recorded below. A missing module, a type error or a fixture failure is never evidence here (the earlier `M0009 import error` runs recorded as RED in the M13 receipt are withdrawn as evidence; see "What this replaces").

**What these are not.** They were written **after** the implementation. They are negative controls, **not** historical failing-test-first development. They show that the three plan properties are *checkable* (a correct implementation is distinguishable from a wrong but valid one); they do not show that the watchdog was developed test-first.

## Boundary (stated plainly)

These controls exercise the **pure decision modules only**: `Policy` (stale-sign decision, timer lifecycle decision), `Planner` (step builders, with `Tx.compile`) and the generated `AuthorityTable`. They prove **nothing** about:

- the actor's `await` paths (SOL RPC calls, journaling around the send, reply decode traps);
- the timer at runtime (`Timer.recurringTimer`, `system func postupgrade`, whether the timer is really re-armed after a real upgrade);
- threshold Ed25519 signing (`schnorr_public_key` / `sign_with_schnorr`) or its latency.

Those stay **unproven** until exercised on a replica (`../replica/native-sol-keeper.md`, a plan that has not been run) and on devnet (M14). **Local threshold Ed25519 may be unavailable on the dfx replica** (the ICP budget doc warns that local threshold signing must not be assumed); if it is, the signing path stays covered only by the pure tests and the fee-payer-only checks, and the boundary stays exactly as written here. There is no public signing endpoint and no alternate executor: nothing in these files adds one.

## How it works

- `Wrong*.mo`: the wrong module. For `Policy` and `Planner` it is an *alias module*: every public name of the real module is re-exported (`public let f = Real.f`, `public type T = Real.T`) and only the deliberately wrong functions are replaced, so the only possible reason for a different result is the marked `BUG` comment. `WrongAuthorityTablePayerIsAuthority.mo` is a copy of the generated table with four values flipped.
- `Wrong*.test.mo`: a textual copy of `../policy.test.mo` or `../authority.test.mo` in which **only** the import paths (`../X` becomes `../../X`) and the subject import (`import Policy "WrongPolicy...";`) change, plus the file name in the final trap/print strings. The real suites are untouched.
- `negative.py check` proves both claims: every generated file is current, each negative test equals its real suite after undoing the mechanical edits (identical assertions), and each wrong module still exposes the full public API of the real one.
- `expected_fail_lines.txt`: the exact FAIL lines each control must end with (recorded by `negative.py record`, reviewed below). `negative.py run` compares both compilers against it.

These files live in `test/negative/`, outside the `test/*.test.mo` glob, because they are *expected to exit non-zero*.

## Commands (repo root; zsh, pinned rig)

```sh
source <scratchpad>/moc-rig/env.sh                                # MOC, MOC_SOURCES (moc 1.5.1, base 0.16.0)
export MOC MOC_SOURCES
python3 native_sol_keeper/test/negative/negative.py check          # generated files current, assertions identical, API complete
python3 native_sol_keeper/test/negative/negative.py run            # 8 real suites GREEN + 9 controls FAIL as recorded, both compilers
# one control by hand (the real suite for comparison first):
$MOC -r ${=MOC_SOURCES} native_sol_keeper/test/policy.test.mo                                   # PASS  ...  policy.test.mo: ALL PASS
$MOC -r ${=MOC_SOURCES} native_sol_keeper/test/negative/WrongPolicyEarlySign.test.mo            # FAIL  ...  exit code 1
~/.cache/dfinity/versions/0.29.2/moc -r ${=MOC_SOURCES} native_sol_keeper/test/negative/WrongPolicyEarlySign.test.mo   # same under moc 0.16.2
python3 native_sol_keeper/test/negative/negative.py gen            # regenerate (after editing negative.py); then `record` only if a FAIL set changes on purpose
```

Each negative run prints the suite's PASS lines for the checks the wrong variant still satisfies, the `FAIL:` lines it does not, and ends with `prim:<line>.<col>: execution error, explicit trap: <Name>.test.mo (negative control of <suite>.test.mo): N check(s) FAILED` and exit code 1. `negative.py run` additionally requires: exit code non-zero, no `ALL PASS`, at least one PASS, no compile/type/import error, and FAIL lines equal to the record.

## The controls

Real counterpart for 1 and 2 (and the shared checks): `policy.test.mo` ends `policy.test.mo: ALL PASS` (54 checks). For 3: `authority.test.mo` ends `authority.test.mo: ALL PASS` (86 checks).

### `WrongPolicyEarlySign.test.mo`

- Property guarded: **1. stale-sign timing** - plan test name `== signs_only_after_stale_progress_threshold ==` (plus the shared policy assertions).
- Wrong behaviour (valid module `WrongPolicyEarlySign.mo`): `decide` signs as soon as the 60 s alert threshold is reached, never returns Blocked (ignores an unresolved unknown journal entry, an unconfigured keeper, invalid thresholds) and has no signing cool-down. This is the combined variant asked for in R5; the next three isolate each bug.
- Command: `$MOC -r ${=MOC_SOURCES} native_sol_keeper/test/negative/WrongPolicyEarlySign.test.mo` (same under `~/.cache/dfinity/versions/0.29.2/moc`).
- Recorded result (moc 1.5.1 and 0.16.2 identical): PASS=44, FAIL=10, ending `WrongPolicyEarlySign.test.mo (negative control of policy.test.mo): 10 check(s) FAILED`, exit code 1.
- Recorded FAIL lines (exact):

```
FAIL: 60 s -> Alert
FAIL: 61 s -> Alert
FAIL: 299 s -> Alert (never Sign before the threshold)
FAIL: 300 s with an unresolved unknown entry must be Blocked
FAIL: an unknown entry does not hide the Alert below the threshold
FAIL: not configured must be Blocked
FAIL: not configured is Blocked even with fresh progress
FAIL: signAfterNs = 0 is an invalid threshold, never an immediate Sign
FAIL: signAfterNs below alertAfterNs is invalid -> Blocked
FAIL: signed 10 s ago -> Alert, not another Sign
```

### `WrongPolicySignsAtAlert.test.mo`

- Property guarded: **1. stale-sign timing** - plan test name `== signs_only_after_stale_progress_threshold ==`.
- Wrong behaviour (valid module `WrongPolicySignsAtAlert.mo`): the real `decide` except that the sign threshold equals the alert threshold (no Alert band; 60 s signs).
- Command: `$MOC -r ${=MOC_SOURCES} native_sol_keeper/test/negative/WrongPolicySignsAtAlert.test.mo` (same under `~/.cache/dfinity/versions/0.29.2/moc`).
- Recorded result (moc 1.5.1 and 0.16.2 identical): PASS=50, FAIL=4, ending `WrongPolicySignsAtAlert.test.mo (negative control of policy.test.mo): 4 check(s) FAILED`, exit code 1.
- Recorded FAIL lines (exact):

```
FAIL: 60 s -> Alert
FAIL: 61 s -> Alert
FAIL: 299 s -> Alert (never Sign before the threshold)
FAIL: an unknown entry does not hide the Alert below the threshold
```

### `WrongPolicyIgnoresUnknown.test.mo`

- Property guarded: **1. stale-sign timing (unknown outcome)** - plan test name `== signs_only_after_stale_progress_threshold ==`.
- Wrong behaviour (valid module `WrongPolicyIgnoresUnknown.mo`): the real `decide` minus the unresolved-unknown-journal-entry check: it signs at 300 s while an earlier transaction's outcome is unknown.
- Command: `$MOC -r ${=MOC_SOURCES} native_sol_keeper/test/negative/WrongPolicyIgnoresUnknown.test.mo` (same under `~/.cache/dfinity/versions/0.29.2/moc`).
- Recorded result (moc 1.5.1 and 0.16.2 identical): PASS=53, FAIL=1, ending `WrongPolicyIgnoresUnknown.test.mo (negative control of policy.test.mo): 1 check(s) FAILED`, exit code 1.
- Recorded FAIL lines (exact):

```
FAIL: 300 s with an unresolved unknown entry must be Blocked
```

### `WrongPolicyIgnoresUnconfigured.test.mo`

- Property guarded: **1. stale-sign timing (configured)** - plan test name `== signs_only_after_stale_progress_threshold ==`.
- Wrong behaviour (valid module `WrongPolicyIgnoresUnconfigured.mo`): the real `decide` minus the configured check: an unconfigured keeper (no fee address) still decides to act.
- Command: `$MOC -r ${=MOC_SOURCES} native_sol_keeper/test/negative/WrongPolicyIgnoresUnconfigured.test.mo` (same under `~/.cache/dfinity/versions/0.29.2/moc`).
- Recorded result (moc 1.5.1 and 0.16.2 identical): PASS=52, FAIL=2, ending `WrongPolicyIgnoresUnconfigured.test.mo (negative control of policy.test.mo): 2 check(s) FAILED`, exit code 1.
- Recorded FAIL lines (exact):

```
FAIL: not configured must be Blocked
FAIL: not configured is Blocked even with fresh progress
```

### `WrongPolicyNeverRearm.test.mo`

- Property guarded: **2. timer re-arm after upgrade** - plan test name `== rearms_timer_after_upgrade ==`.
- Wrong behaviour (valid module `WrongPolicyNeverRearm.mo`): `rearmAfterUpgrade` is always false and `timerStep(#PostUpgrade)` is a no-op: the decision never re-arms.
- Command: `$MOC -r ${=MOC_SOURCES} native_sol_keeper/test/negative/WrongPolicyNeverRearm.test.mo` (same under `~/.cache/dfinity/versions/0.29.2/moc`).
- Recorded result (moc 1.5.1 and 0.16.2 identical): PASS=51, FAIL=3, ending `WrongPolicyNeverRearm.test.mo (negative control of policy.test.mo): 3 check(s) FAILED`, exit code 1.
- Recorded FAIL lines (exact):

```
FAIL: armed + configured -> postupgrade re-arms
FAIL: postupgrade re-arms because the flag was set
FAIL: disarm cancels the timer and clears the flag
```

### `WrongPolicyRearmWhenDisarmed.test.mo`

- Property guarded: **2. timer re-arm after upgrade** - plan test name `== rearms_timer_after_upgrade ==`.
- Wrong behaviour (valid module `WrongPolicyRearmWhenDisarmed.mo`): `rearmAfterUpgrade` ignores the stable `armed` flag and `timerStep(#PostUpgrade)` starts the timer whenever the keeper is configured: a disarmed keeper is resurrected by an upgrade.
- Command: `$MOC -r ${=MOC_SOURCES} native_sol_keeper/test/negative/WrongPolicyRearmWhenDisarmed.test.mo` (same under `~/.cache/dfinity/versions/0.29.2/moc`).
- Recorded result (moc 1.5.1 and 0.16.2 identical): PASS=52, FAIL=2, ending `WrongPolicyRearmWhenDisarmed.test.mo (negative control of policy.test.mo): 2 check(s) FAILED`, exit code 1.
- Recorded FAIL lines (exact):

```
FAIL: disarmed stays disarmed after an upgrade
FAIL: after a disarm an upgrade does not resurrect the timer
```

### `WrongPlannerFeeInAuthoritySlot.test.mo`

- Property guarded: **3. prohibited authority signing** - plan test name `== fee_address_has_no_program_authority ==`.
- Wrong behaviour (valid module `WrongPlannerFeeInAuthoritySlot.mo`): the step builders for `seal_weights` and `finalize_round` place the fee key, as a signing writable account, in their first account slot (the slot the program treats as state/authority).
- Command: `$MOC -r ${=MOC_SOURCES} native_sol_keeper/test/negative/WrongPlannerFeeInAuthoritySlot.test.mo` (same under `~/.cache/dfinity/versions/0.29.2/moc`).
- Recorded result (moc 1.5.1 and 0.16.2 identical): PASS=80, FAIL=6, ending `WrongPlannerFeeInAuthoritySlot.test.mo (negative control of authority.test.mo): 6 check(s) FAILED`, exit code 1.
- Recorded FAIL lines (exact):

```
FAIL: seal_weights: account flags match the IDL positions
FAIL: seal_weights: no instruction account is a signer and none is the fee address
FAIL: seal_weights: no instruction references the fee address (it pays the network fee only)
FAIL: finalize_round: account flags match the IDL positions
FAIL: finalize_round: no instruction account is a signer and none is the fee address
FAIL: finalize_round: no instruction references the fee address (it pays the network fee only)
```

### `WrongPlannerExtraSigner.test.mo`

- Property guarded: **3. prohibited authority signing** - plan test name `== fee_address_has_no_program_authority ==`.
- Wrong behaviour (valid module `WrongPlannerExtraSigner.mo`): the step builders for `commit_ticket` and `expire_promotion` append an extra signer account.
- Command: `$MOC -r ${=MOC_SOURCES} native_sol_keeper/test/negative/WrongPlannerExtraSigner.test.mo` (same under `~/.cache/dfinity/versions/0.29.2/moc`).
- Recorded result (moc 1.5.1 and 0.16.2 identical): PASS=82, FAIL=4, ending `WrongPlannerExtraSigner.test.mo (negative control of authority.test.mo): 4 check(s) FAILED`, exit code 1.
- Recorded FAIL lines (exact):

```
FAIL: commit_ticket: no instruction account is a signer and none is the fee address
FAIL: commit_ticket: the fee address is message account 0 and the only signer
FAIL: expire_promotion: no instruction account is a signer and none is the fee address
FAIL: expire_promotion: the fee address is message account 0 and the only signer
```

### `WrongAuthorityTablePayerIsAuthority.test.mo`

- Property guarded: **3. authority table** - plan test name `== fee_address_has_no_program_authority ==` (table half).
- Wrong behaviour (valid module `WrongAuthorityTablePayerIsAuthority.mo`): a copy of the generated `AuthorityTable` in which the four `payer` accounts are marked `isAuthority = true`.
- Command: `$MOC -r ${=MOC_SOURCES} native_sol_keeper/test/negative/WrongAuthorityTablePayerIsAuthority.test.mo` (same under `~/.cache/dfinity/versions/0.29.2/moc`).
- Recorded result (moc 1.5.1 and 0.16.2 identical): PASS=85, FAIL=1, ending `WrongAuthorityTablePayerIsAuthority.test.mo (negative control of authority.test.mo): 1 check(s) FAILED`, exit code 1.
- Recorded FAIL lines (exact):

```
FAIL: no payer / fee_payer account is marked authority
```

## Review of the recorded FAIL sets

- **Stale-sign timing.** `WrongPolicyEarlySign` fails the 60 s, 61 s and 299 s Alert cases and both Blocked cases (300 s with an unresolved unknown entry, not configured); it still passes `300 s -> Sign` because a wrong early signer also signs at 300 s, which is why the 300 s case alone cannot catch it. The single-bug variants fail exactly their own cases: signing at the alert threshold fails the Alert band (60/61/299 s), ignoring unknown entries fails only the 300 s-with-unknown Blocked case, ignoring `configured` fails only the two not-configured cases.
- **Timer re-arm.** A decision that never re-arms fails `armed + configured -> postupgrade re-arms` and `postupgrade re-arms because the flag was set` (and, as a consequence, `disarm cancels the timer`, since the lifecycle model never had a live timer to cancel). A decision that re-arms regardless of `armed` fails `disarmed stays disarmed after an upgrade` and `after a disarm an upgrade does not resurrect the timer`. Each variant leaves the other direction's checks green, so the two bugs are told apart.
- **Authority.** Placing the fee key in an authority slot fails the table-alignment check (`account flags match the IDL positions`), the no-signer check and the `no instruction references the fee address` check (the fee address is only ever allowed to be message account 0). An extra signer fails the no-signer check and the `only signer` check. Marking `payer` as authority in the table fails `no payer / fee_payer account is marked authority`. The `is message account 0` / `appears once` checks stay green for the fee-in-slot variant because the compiler merges the fee key into account 0; the instruction-reference check is the one that catches it (unit-pinned in `tx.test.mo` and `gate.test.mo`).

## Recorded run (`negative.py run`, both compilers)

```
##### Motoko compiler 1.5.1
real suites (must be GREEN):
  policy     PASS=54  FAIL=0  policy.test.mo: ALL PASS
  journal    PASS=47  FAIL=0  journal.test.mo: ALL PASS
  authority  PASS=86  FAIL=0  authority.test.mo: ALL PASS
  gate       PASS=28  FAIL=0  gate.test.mo: ALL PASS
  pda        PASS=23  FAIL=0  pda.test.mo: ALL PASS
  accounts   PASS=42  FAIL=0  accounts.test.mo: ALL PASS
  tx         PASS=34  FAIL=0  tx.test.mo: ALL PASS
  planner    PASS=78  FAIL=0  planner.test.mo: ALL PASS
negative controls (must END IN A 'FAILED' TRAP with exactly the recorded FAIL lines):
  WrongPolicyEarlySign                   PASS=44  FAIL=10 trap: WrongPolicyEarlySign.test.mo (negative control of policy.test.mo): 10 check(s) FAILED
  WrongPolicySignsAtAlert                PASS=50  FAIL=4  trap: WrongPolicySignsAtAlert.test.mo (negative control of policy.test.mo): 4 check(s) FAILED
  WrongPolicyIgnoresUnknown              PASS=53  FAIL=1  trap: WrongPolicyIgnoresUnknown.test.mo (negative control of policy.test.mo): 1 check(s) FAILED
  WrongPolicyIgnoresUnconfigured         PASS=52  FAIL=2  trap: WrongPolicyIgnoresUnconfigured.test.mo (negative control of policy.test.mo): 2 check(s) FAILED
  WrongPolicyNeverRearm                  PASS=51  FAIL=3  trap: WrongPolicyNeverRearm.test.mo (negative control of policy.test.mo): 3 check(s) FAILED
  WrongPolicyRearmWhenDisarmed           PASS=52  FAIL=2  trap: WrongPolicyRearmWhenDisarmed.test.mo (negative control of policy.test.mo): 2 check(s) FAILED
  WrongPlannerFeeInAuthoritySlot         PASS=80  FAIL=6  trap: WrongPlannerFeeInAuthoritySlot.test.mo (negative control of authority.test.mo): 6 check(s) FAILED
  WrongPlannerExtraSigner                PASS=82  FAIL=4  trap: WrongPlannerExtraSigner.test.mo (negative control of authority.test.mo): 4 check(s) FAILED
  WrongAuthorityTablePayerIsAuthority    PASS=85  FAIL=1  trap: WrongAuthorityTablePayerIsAuthority.test.mo (negative control of authority.test.mo): 1 check(s) FAILED
##### Motoko compiler 0.16.2
real suites (must be GREEN):
  policy     PASS=54  FAIL=0  policy.test.mo: ALL PASS
  journal    PASS=47  FAIL=0  journal.test.mo: ALL PASS
  authority  PASS=86  FAIL=0  authority.test.mo: ALL PASS
  gate       PASS=28  FAIL=0  gate.test.mo: ALL PASS
  pda        PASS=23  FAIL=0  pda.test.mo: ALL PASS
  accounts   PASS=42  FAIL=0  accounts.test.mo: ALL PASS
  tx         PASS=34  FAIL=0  tx.test.mo: ALL PASS
  planner    PASS=78  FAIL=0  planner.test.mo: ALL PASS
negative controls (must END IN A 'FAILED' TRAP with exactly the recorded FAIL lines):
  WrongPolicyEarlySign                   PASS=44  FAIL=10 trap: WrongPolicyEarlySign.test.mo (negative control of policy.test.mo): 10 check(s) FAILED
  WrongPolicySignsAtAlert                PASS=50  FAIL=4  trap: WrongPolicySignsAtAlert.test.mo (negative control of policy.test.mo): 4 check(s) FAILED
  WrongPolicyIgnoresUnknown              PASS=53  FAIL=1  trap: WrongPolicyIgnoresUnknown.test.mo (negative control of policy.test.mo): 1 check(s) FAILED
  WrongPolicyIgnoresUnconfigured         PASS=52  FAIL=2  trap: WrongPolicyIgnoresUnconfigured.test.mo (negative control of policy.test.mo): 2 check(s) FAILED
  WrongPolicyNeverRearm                  PASS=51  FAIL=3  trap: WrongPolicyNeverRearm.test.mo (negative control of policy.test.mo): 3 check(s) FAILED
  WrongPolicyRearmWhenDisarmed           PASS=52  FAIL=2  trap: WrongPolicyRearmWhenDisarmed.test.mo (negative control of policy.test.mo): 2 check(s) FAILED
  WrongPlannerFeeInAuthoritySlot         PASS=80  FAIL=6  trap: WrongPlannerFeeInAuthoritySlot.test.mo (negative control of authority.test.mo): 6 check(s) FAILED
  WrongPlannerExtraSigner                PASS=82  FAIL=4  trap: WrongPlannerExtraSigner.test.mo (negative control of authority.test.mo): 4 check(s) FAILED
  WrongAuthorityTablePayerIsAuthority    PASS=85  FAIL=1  trap: WrongAuthorityTablePayerIsAuthority.test.mo (negative control of authority.test.mo): 1 check(s) FAILED
RESULT: OK
```

## What this replaces

The M13 receipt (`docs/solana/evidence/2026-10-14-m13-keepers.json`) recorded `M0009` missing-module import errors as the RED stage for the watchdog. Codex review R5 rejects compile/fixture failures as RED (go-ahead line 18). That wording is withdrawn: the missing-module runs show only that the modules did not yet exist. The evidence for the three plan properties is this directory: assertion-level failures of valid wrong variants, next to the PASS of the real modules, labelled as later negative controls.
