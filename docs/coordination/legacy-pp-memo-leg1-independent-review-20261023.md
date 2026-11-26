# LEG-1 independent review: legacy PP-ledger memo fix (2026-10-23)

**Target:** branch `fix/pp-ledger-memo-limit`, commit `a2487934651cd811088eb5179af87446fd000d6d` (based on `origin/main` f96b33f, whose `shenanigans/` equals ee9a402). Rehearsal receipt `shenanigans/test/replica/memo/receipt-2026-10-23.json`, sha256 `551f5c32403e46f221269beebdfd048c965ca197af57413372f04047a5e6f0ee`.

**Reviewer:** a fresh Sonnet agent (the repository's `musicalchairs-shenanigans-pp-reviewer` profile), distinct from the LEG-1 implementer and from the coordinator's reruns. The review was read-only. It made no edits, builds, test runs, deploys, canister calls or git writes. It ran only `shasum`, `strings` and isolated `python3 -I` computations on committed files and the gitignored wasm artifacts.

**Verdict: PASS with findings.** No blocking or high-severity issue. The fix is memo-only. This is not Codex acceptance and not deployment approval.

## Item results

1. **Short-memo byte identity: PASS.** `LedgerMemo.mo:30–33` returns `Text.encodeUtf8(text)` whenever the encoded size is at most 32 bytes. `Blob.size()` counts bytes, so 32 bytes stays raw and 33 or more is hashed. Unit tests cover 26 short ids, the 32/33-byte boundary, 16×é (32 bytes, kept) and 17×é (34 bytes, hashed). The receipt's blocks show `admin-mint`, `game-N` and `cascade-residual-game-N` memos byte-identical before and after the fix.
2. **SHA-256 correctness: PASS.** `Sha256.mo:34–119` was read against FIPS 180-4: padding, big-endian length, schedule, rounds, wrapping arithmetic and output. All 64 round constants and 8 initial values were regenerated from first principles and match. A line-for-line Python transliteration matches `hashlib` for every length 0–299 and for 1,000. Every test vector equals `hashlib`. On-ledger memos in receipt blocks 113, 114, 117–120, 123 and 124 equal `sha256` of their event id.
3. **All ledger call sites: PASS.** The ledger actor exposes only `icrc1_transfer` and `icrc2_transfer_from` as mutating calls; there is no `icrc2_approve`. All seven mutating calls in `main.mo` build their memo with `LedgerMemo.ledgerMemo`: `icrc2_transfer_from` at 1752, and `icrc1_transfer` at 1877, 2019, 2071, 2092, 2227 and 2261. No `encodeUtf8` remains, and no other file under `shenanigans/` makes a ledger call.
4. **Unchanged semantics: PASS.** The diff is +8/−8: the import plus seven memo expressions. Unchanged: amounts, recipients and subaccounts, `fee`, `created_at_time`, `#Duplicate` handling, the `mintedEventIds` keys (event-id text), retry and await ordering, and siphon and cascade behaviour.
5. **Stable signature and source pins: PASS, with a provenance caveat (F-4).** The `motoko:stable-types` section (12,459 bytes), `candid:service`, `candid:args`, the EOP marker and the compiler (moc 0.16.2) are byte-identical between the pre-fix and fixed wasm artifacts. The artifact hashes match the receipt (pre-fix `76a33c09…`, fixed `6e848496…`).
6. **Meaningful rehearsal: PASS.** Receipt sha256 confirmed. All 29 checks are computed from dfx replies, canister status, logs or decoded ledger blocks. The sequence is:
   - Phase 1 reproduces the IC0503 memo traps on the pre-fix wasm: signup gift 70 bytes, backers 79 and 80 bytes.
   - Phase 2 is an in-place `install --mode upgrade --wasm-memory-persistence keep` of the same canister id, with state, Candid and balances preserved.
   - Phase 3 commits a new signup gift, a new backer mint and their long-id cascade legs with 32-byte memos. Short memos stay unchanged, no memo exceeds 32 bytes, the old failures are untouched, and a second tick mints nothing.
   - Not exercised on the replica: the chip-deposit, cash-out, spell, backfire and karma sites, and a ranked cascade-A leg. These have static and unit coverage only.
7. **No recovery, compensation, backfill or re-credit code or state: PASS.** `LedgerMemo.mo` and `Sha256.mo` are stateless. The stub canister is a test fixture in its own dfx project.
8. **Legacy residuals (disclosed, not changed).**
   - **Lost gift retried on the next game: better.** The claim is written only after `#Ok` (`main.mo:1489–1496`), so the next qualifying game now commits the gift. A player who never plays again stays unpaid.
   - **Lost backer mints never re-presented: unchanged** for backers already given up (`backerSeen := backer.amount` at 1669). **Better** for backers still inside the 10-attempt window.
   - **`created_at_time` stamped per call, so ledger dedup never fires: unchanged.** The comment at `main.mo:1387–1389` that claims ledger-level dedup is stale and pre-existing.
   - **`mintedEventIds` check (2878) → await (2917) → write (2923): code unchanged, reach slightly wider.** Gift, backer and long-id cascade mints can now commit, so they inherit the same admin-versus-observer race and commit-then-trap window that `game-N` mints already had. The only guard is the transient `observerRunning` flag.

## Findings

- **F-1 (medium, decision-relevant, not a defect).** Deploying the fix alone causes payouts. The unchanged observer will mint each lost signup gift (500 PP: 450 net plus 50 cascade) on that player's next qualifying game, and it will commit any in-flight backer retry on the next tick. Amounts use the current `mintConfig`, referral chain and activity state, and backers who mint become "active", which changes cascade routing. Any separate recovery (LEG-2) must reuse the ORIGINAL event ids through `mintWithEffects`, so that its dedup turns a later natural retry into a no-op. A recovery under a different id would double-pay anyone who plays again. The sequencing between deploy and recovery belongs to whoever authorizes the deploy.
- **F-2 (low, doc nit): fixed** in a follow-up text-only commit on the fix branch. The kickup memo threshold is 9 digits, not 8: `kickup-residual-chipdep-` is 24 bytes.
- **F-3 (low/info).** `memo-static.test.sh` inspects only `shenanigans/main.mo`. A ledger call added in another module, or through an actor not named `ppLedger`, would not be counted, and the script is not wired into CI. It catches every bypass form inside `main.mo`.
- **F-4 (low/info).** The receipt records wasm hashes but no commit or source pin. The fixed wasm was most plausibly built from the committed sources (mtimes are consistent and the tree was clean at a2487934). That the pre-fix wasm came from main@ee9a402 cannot be verified read-only, though its symbols and stable types are consistent with that. Before any deploy, rebuild with `--network ic` from a2487934 and record that hash.
- **F-5 (info): fixed** in the same follow-up text commit. The rehearsal script's "refuses any other network" comment now states exactly what it checks.

## Not verifiable read-only

- Running the unit test, the static guard and `moc --stable-compatible` against the live signature. The coordinator ran these separately: 145/145, PASS with self-test, and OK with a structurally identical signature.
- The live canister's lineage. It is inferred, and the served bytes are unverified.
- Mainnet behaviour.

`ponzi_math_sol` uses `memo = null` and is unaffected.
