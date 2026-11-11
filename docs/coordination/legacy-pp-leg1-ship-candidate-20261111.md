# LEG-1-SHIP local deploy candidate: evidence and Gideon's adjudication (2026-11-11)

This is local candidate preparation for the legacy Shenanigans PP-ledger memo fix (LEG-1). It is not deployment approval. Nothing was deployed, installed, minted, recovered, funded or activated.

## Adjudication (Gideon, 2026-11-11): ACCEPTED for local deploy-candidate readiness only

- **Accepted.** The exact original, unmodified candidate pinned below: `candidate/shenanigans-53136a74.wasm`, sha256 `53136a74eee136a2da6e21c7b787a7810b5989d3525384eb077e83621e575ffb`, in the sealed folder whose `SHA256SUMS` hashes to `4639fd2b…`.
- **His independent bounded review verified** the complete manifest and candidate bytes, the compiled-source baseline and the reviewed memo-only delta, every section across the candidate builds, the live interface and compiler metadata, and formal stable compatibility in both directions with the recorded compiler.
- **Raw cross-path byte reproducibility remains NOT ACHIEVED.** Only the compiler-generated names in the embedded stable signature differ; every other section is raw-identical. Normalized equivalence is reported separately and does not substitute for raw reproducibility.
- **What it closes.** The LEG-1-REV provenance gap (the deploy candidate must be rebuilt and pinned first; F-4), for this preserved file only.
- **What it does not do.** It does not approve arbitrary rebuilds or claim proven live source lineage. It authorizes no deployment, minting, recovery, new funding, production activation or other execution.
- **Status.** Local preparation is ACCEPTED. Live delivery is BLOCKED on Rob's separate upgrade decision. All other existing holds and deposit maturity requirements remain.

### Deployment recommendation for Rob (not authorization)

- **Order.** Gideon judges the exact preserved memo-only candidate suitable for a separately approved, snapshot-protected upgrade before the native Shenanigans upgrade (M14-B3). The later native candidate must then be reassessed against the changed baseline. This reverses the approval packet's original MI-4 suggestion (B3 on the current live baseline, LEG-1 rebased afterwards). The decision stays Rob's.
- **Disclosure.** Once live, ordinary signup-gift retries and in-flight backer retries can start paying at the current configuration, referral chain and activity state (LEG-1-REV F-1). Recovery of missed rewards (LEG-2) remains separate and must preserve original-event duplicate prevention by reusing the original event ids.

### Wording correction

Earlier wording here, and the sealed folder's `README.txt` ("a rebuild yields a different raw hash"), implied that any fresh rebuild must differ. That is corrected: only the tested different-path builds differed, the same-path baseline control matched, and the cause (build-path dependence) remains a hypothesis. The sealed folder and its manifest are preserved unchanged.

## Scope and authority

- **Resumed** on 2026-11-11 by the instruction in Silas's session. It covers the existing LEG-1 local candidate preparation only: reproducible local builds, source-baseline and stable-compatibility evidence, and the necessary read-only checks. It used only the included subscription route, with no paid or API fallback.
- **Gideon's clarification during the sitting:**
  - Report raw byte-for-byte rebuild reproducibility as **not achieved**, and label normalized equivalence separately.
  - Preserve and pin the original, unmodified candidate.
  - Return the section-difference evidence and the formal stable-compatibility results for his adjudication, without waiving the raw-hash difference.
  - No further builds or full audit.
- **Unchanged.** The memo-only scope; the LEG-1-REV findings (F-1 payout sequencing; F-3 and F-4 informational; F-2 and F-5 fixed at 44be00b); LEG-2 recovery stays separate and unowned.
- **Still required.** Gideon's candidate acceptance (given 2026-11-11, above), then Rob's separate mainnet-upgrade approval (not given). The snapshot-protected upgrade (stop, snapshot, upgrade, start, state diff, restore on mismatch) is a separate step after both.

## Pinned candidate

| Item | Value |
| --- | --- |
| Artifact | `~/.cache/mc-legacy-artifacts/leg1-candidate-20261111/candidate/shenanigans-53136a74.wasm` (the original, unmodified dfx output of build A; folder made read-only) |
| sha256 | `53136a74eee136a2da6e21c7b787a7810b5989d3525384eb077e83621e575ffb` (1,053,976 bytes) |
| Source | `fix/pp-ledger-memo-limit` at `a2487934651cd811088eb5179af87446fd000d6d`, the LEG-1-REV reviewed commit, built from a clean detached worktree |
| Toolchain | dfx 0.29.2 with its bundled moc 0.16.2 (`nhr40q5b-9hlpnhd0-9pz2rqqi-z1giglvc`) and bundled `base`, which is also the live `motoko:compiler` value |
| Command | `dfx build --network ic shenanigans` (no gzip; `dfx.json` build args empty; no mops) |
| Companion files | `candidate/shenanigans.did` sha256 `bdad2ea6…0ac6`; `candidate/shenanigans.most` sha256 `28e482b8…9603` (equal to the wasm's embedded stable-types section) |
| Folder manifest | `SHA256SUMS` (60 files) sha256 `4639fd2bc3dc449b447b6d9f0fa0e6e1f54849682f61844be05b910e34d4229e` |

An upgrade must install this exact file. Its on-chain module hash would then be `0x53136a74…` (no gzip). A rebuild is not a substitute for this file: the tested builds at different paths gave different raw hashes, as the next section shows.

## Raw byte-for-byte rebuild reproducibility: NOT ACHIEVED

Five builds were made with the same command and toolchain, in four fresh detached worktrees; base run 2 reused run 1's worktree. The hotfix worktree was untouched.

| Build | Commit | Worktree path | Raw wasm sha256 |
| --- | --- | --- | --- |
| A (pinned) | a2487934 | `…/scratchpad/leg1b/candA` | `53136a74eee136a2da6e21c7b787a7810b5989d3525384eb077e83621e575ffb` |
| B | a2487934 | `…/scratchpad/leg1b/candB` | `3afe1e18fe27a7921aabef62e7a19f4d8ec0927046bb0b888755c6c17409f45e` |
| tip | 44be00b (same compiled inputs as a2487934) | `…/scratchpad/leg1b/tip` | `f9ae488310acc562226a98a82602d0a66f8803169d667c759747f7ca3aed5a40` |
| base, run 1 | f96b33f (fix base) | `…/scratchpad/leg1b/base` | `9da2e63d0a9e8a77fbdd8f4adb31139f366b6fdc0ee9e0b259f01295789c20b9` |
| base, run 2 | f96b33f | same path as run 1 | `9da2e63d…0b9` (raw-identical to run 1) |

- Three builds of the candidate's compiled source gave three different raw hashes.
- The 2026-10-23 base build of f96b33f, made at another path, hashed `13bddcc4…`. The live module is `7fa41360…`.

### Section-difference evidence (`sections-compare.txt`)

- Across A, B and the tip, **every wasm section is raw-identical except one**: `custom:icp:private motoko:stable-types`.
  - Its length is 12,465, 12,461 and 12,456 bytes respectively.
  - It differs in the compiler-generated type-name suffixes, such as `BackerSeen__278293399`.
- The code section (id 10, 837,600 bytes) and the type, import, function, table, memory, global, export, start, element, data-count and data sections are identical across the three. So are `custom:name`, `candid:service`, `candid:args`, `motoko:compiler`, the `enhanced-orthogonal-persistence` marker and the `dfx` metadata.
- **Base vs candidate** differs in sections id 3, 7, 8, 9 and 10 and in `custom:name`, as expected for added code.
  - Function names present only in the candidate: `ledgerMemo`, `sha256`, `bigint_to_word32_wrap`, `fromArrayMut`, `alloc_1699`.
  - The other name differences are eight anonymous functions, each renamed exactly one source line later.
- **Live metadata vs the pinned candidate, raw bytes:**
  - `candid:service` (16,920 B), `candid:args` and `motoko:compiler` are identical.
  - `motoko:stable-types` is not raw-equal: 12,459 B live vs 12,465 B candidate, because of the suffixes.
- **Observation, not tested further.** The two base builds at the same path were raw-identical, while builds at different paths differed. This suggests the suffix numbering depends on the build location, but no test isolates that. Per Gideon, no further builds were run, so it remains a hypothesis.
- **Consequence.** In the tested builds, rebuilding at a different path did not reproduce `53136a74…`. Whether a rebuild at the original path would is untested; the same-path baseline control matched, and the cause remains a hypothesis. The pin is the preserved file itself, and Gideon's acceptance covers only that file.

## Normalized equivalence (a separate result, not raw reproducibility)

The normalization strips `__<digits>` suffixes from the `motoko:stable-types` section only. Every other section is hashed raw, in order. The method is in `tools/wnorm.py`, and the results are in `normalized-equivalence.txt`.

- A, B and the tip share the normalized digest `6c15c7741bfe3363e1162201c41db0074244eee218214ab3bff11eab4a40e688`. The base's normalized digest is `b1d37ced…b9cf`.
- The normalized stable signature is byte-identical, at 11,066 bytes, across the live canister, the base and all three candidate builds.

## Formal stable-compatibility results (`stable-compat.txt`)

Using `moc 0.16.2 --stable-compatible <pre> <post>`:

| Pre | Post | Result |
| --- | --- | --- |
| live `motoko:stable-types` (fetched 2026-11-11) | candidate `shenanigans.most` | exit 0, no output |
| candidate `shenanigans.most` | live | exit 0, no output |
| live | base `shenanigans.most` | exit 0, no output |

The candidate's `candid:service` is byte-identical to the live interface, so the public interface is unchanged.

## Source baseline (`tools/closure.py`)

The compiled closure of `shenanigans/main.mo` covers every relative import, recursively. It includes `observatory/types.mo`, which sits outside `shenanigans/`, and the built-in `mo:base` package.

Git blob ids per commit (12-character prefixes):

| File | ee9a402 (inferred live lineage) | f96b33f (fix base) | a2487934 (candidate) | 44be00b (tip) | origin/main 7030b0f |
| --- | --- | --- | --- | --- | --- |
| `shenanigans/main.mo` | 34e68ff96629 | 34e68ff96629 | c424ea356b57 | c424ea356b57 | 34e68ff96629 |
| `shenanigans/LedgerMemo.mo` | — | — | 3e36f89ee315 | 3e36f89ee315 | — |
| `shenanigans/Sha256.mo` | — | — | 138df975cd97 | 138df975cd97 | — |
| `shenanigans/PpLedger.mo`, `Reginald.mo`, `Subaccount.mo`, `icrc21.mo`, `observatory/types.mo` | identical in all five commits | | | | |
| `dfx.json` | a48403ae0ac0 | a48403ae0ac0 | a48403ae0ac0 | a48403ae0ac0 | a48403ae0ac0 |
| `canister_ids.json` | 99237553da1b | 4f06044dc7c9 | 4f06044dc7c9 | 4f06044dc7c9 | 4f06044dc7c9 |

- **ee9a402 → f96b33f.** Touches only `canister_ids.json` (it adds the observatory id), `cycle-manager.targets.json`, a doc and `observatory/main.mo`. None of them is in the compiled closure, and Shenanigans has no `canister:` imports.
- **f96b33f → a2487934.** `main.mo` +8/−8, the two new modules, and tests and fixtures only. Nothing outside `shenanigans/` changes.
- **a2487934 → 44be00b.** Changes only `memo-rehearsal.sh` and `recovery-evidence.md`, which are not compiled.
- **origin/main** is 3 commits past ee9a402, and none touches the closure.
- **Live lineage stays inferred.** ee9a402 as the live source rests on interface and stable-signature equality (both re-confirmed today). Live code bytes cannot be fetched, so served-byte equality remains unverifiable.
- **Base library.** `mo:base` comes from the dfx 0.29.2 bundle. That the live build used the same bundle is inferred from the identical compiler, interface and signature, not proven.

## Read-only live checks (2026-11-11T16:19:54Z)

No update calls were made, nothing was written, and no state changed.

- `dfx canister info --network ic --identity anonymous j56tm-oaaaa-aaaac-qf34q-cai`: module hash `0x7fa413608bdb4d1412ff21e16c92421b6949c0bf3861738919e8a649874067bc`, unchanged since 2026-10-23. Controllers are `6pwpo-…-tqe` and `cpbhu-…-cai` (CycleOps), also unchanged.
- `dfx canister metadata` (anonymous): `candid:service` (sha256 `da8db88a…`, 16,920 B) and `candid:args`. An anonymous read of `motoko:stable-types` is refused with 403 (controller-only), as expected.
- `dfx canister metadata --identity CharlesPonzi` (controller read, still read-only, the same method as 2026-10-23): `motoko:stable-types` (sha256 `bee973f8…`, 12,459 B) and `motoko:compiler` = `0.16.2`.

## Local tests at the candidate commit (worktree candA)

- `moc -r --package base … shenanigans/test/ledger-memo.test.mo`: **145/145** passed.
- `memo-static.test.sh`: **PASS**, with 7 ledger calls, all through `LedgerMemo.ledgerMemo`. `--self-test`: **PASS**.
- **Build warnings.** Only the two pre-existing M0155 warnings (main.mo 3438 and 3920), the same as the base build.

## Open items

1. **Raw reproducibility not achieved: adjudicated.** Gideon accepted the preserved file on 2026-11-11 for local readiness only (see Adjudication). Raw cross-path reproducibility stays NOT ACHIEVED.
2. **F-1 payout sequencing (from LEG-1-REV).** Deploying the fix alone pays each lost signup gift on that player's next qualifying game, and commits in-flight backer retries. Any LEG-2 recovery must reuse the original event ids.
3. **Upgrade mechanics, for the later approved step only.** Install the pinned file as an in-place upgrade with `--wasm-memory-persistence keep` (the canister carries the enhanced-orthogonal-persistence marker), with the snapshot recipe. Order relative to M14-B3 stays Rob's decision.

## Not done

No deploy, install, upgrade, mint, recovery, compensation, funding, production activation, ledger write, or change to the fix branch or its review. No full audit, and no builds after Gideon's clarification. The four throwaway build worktrees are kept in place, under the session scratchpad at `leg1b/`. The pinned files do not depend on them.
