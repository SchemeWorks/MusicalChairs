# Native Solana — M9 measurement and inline feasibility handoff (2026-10-14)

Branch `codex/native-solana-production-core`, continuing from the [M8 handoff](native-solana-m8-handoff-20261014.md) (`8b688cc`, G1 runtime proof `931b6d5`). Local work plus GitHub-hosted CI for the production SBF build; nothing deployed, signed with live keys, funded, registered, activated or merged.

## Commits (oldest first)

| Commit | Subject |
| --- | --- |
| `56df0b9` | chore(program): clippy clean under -D warnings; committed build pins for the production SBF CI job |
| `ebd9884` | docs(solana): measured compute, packet, rent and pilot budget; ci: locked production SBF job (IMPL-39a / 39c part 1) |
| `9f3da9a` | feat(program): opt-in inline allocation for registries of at most eight Backers (IMPL-39b); Codex M9 corrections |
| `3c3003c` | ci: link the pinned platform tools into the SBF SDK's dependencies directory (Linux run 2 failed at the build step without it) |
| `37185ea` | test(harness): matured Compound15/30 inline settlement equals the chunked ticket; ci: run cargo from the crate directories (the committed `.cargo/config.toml` check-cfg applies only from the crate cwd — Linux run 3 failed at clippy without it); docs: Codex M9 corrections (budget retained rent incl. the permanent CloseRecord, receipt inline estimate labelled historical, M9-F1 accepted/documented) |
| this commit | docs(evidence): Linux production-SBF CI evidence (run 38605447811, green), M9 graph fragment + refresh receipt, this handoff; kanban IMPL-39c Done |
| (this commit) | docs(evidence): Linux production-SBF CI evidence, M9 graph fragment and handoff (IMPL-39c) |

## What M9 delivered

- **Measurement matrix (IMPL-39a)**: `src/measure.rs`, 6 families / 24 headline rows / 6,680 observations, 9/9 on first execution. Every chunk transaction at N = 64/100/256/1,024 × rows 1/4/8 is within the legacy packet, 64 locks and compute (worst prune 57,410 CU); the one-row fallback is executed at every N. Churn 20 cycles; dust claims 1..=10 lamports (LiteSVM accepts fresh-wallet credits the cluster would refuse — documented, M9-F1); full close path M = 100 (required reserve 94,633,200 lamports at local rent; 20 keeper core transactions per close); keeper count at b = 8: 42,200 core vs the 42,170 scenario. Document: `docs/solana/measurements-2026-10.md`; budget "Measured" section in `native-solana-icp-budget.md` (SIMD-0437 local step and the 696-lamport column; permanent CloseRecord retained rent 5,463,600 lamports per close added once, outside the reserve and the held-before-cleanup total).
- **Inline step (IMPL-39b)**: opt-in, non-closing only: a payout that supplies `[Backer slot 0..registry_len−1, RegistryPage 0, SnapshotPage 0]` (all writable) runs Discover→Commit in-instruction when `registry_len <= INLINE_THRESHOLD` (8); no remaining accounts → the chunked ticket starts exactly as before; `registry_len > 8` with accounts → `InvalidState`. 262 unit tests (stepwise equality for every N 1..8 over 24 seeds × 6 tolls, the M6 goldens, the Book oracle; RED with the driver stubbed), clippy clean, IDL unchanged. LiteSVM `tests/inline_alloc.rs`: inline == chunked byte for byte for N = 1..8; real cost:

| N | packet B | keys | CU |
| --- | --- | --- | --- |
| 1 | 827 | 19 | 82,483 |
| 2 | 860 | 20 | 91,915 |
| 3 | 893 | 21 | 101,551 |
| 4 | 926 | 22 | 109,636 |
| 5 | 959 | 23 | 118,510 |
| 6 | 992 | 24 | 127,835 |
| 7 | 1,025 | 25 | 137,517 |
| 8 | 1,058 | 26 | 147,005 |

  N = 8 fits the legacy packet with 174 B to spare and stays under the 200k default compute (the §5 envelope estimate of 528k was conservative by design). N = 9 is refused; wrong layouts are refused fee-only before any cash moves.
- **DR-31 decision output**: chunk default **8 rows**, one-row fallback proven at every N; inline threshold **8** (measured, not estimated); keeper transaction format legacy/v0 fits everywhere (DR-28). Nothing is returned to Rob as a redesign: the inline step is the plan's own step 2(b), bounded by the measurement.
- **Production SBF CI (IMPL-39c)**: `production-sbf` job in `.github/workflows/native-solana-model.yml` — pinned Linux toolchain archives by SHA-256 from the official anza-xyz releases (platform-tools-linux-x86_64 v1.48 `a9d3157f…`, agave v3.1.0-beta.0 `4f1d4db5…`) into a job-local directory, `build-sbf.sh` + `build-standin-sbf.sh` with the `MC_*` variables (job-local `MC_STANDIN_TARGET_DIR`), source-set and lock asserted against `docs/solana/evidence/latest-build.json` with Linux's own artifact hash recorded, the LiteSVM harness on the fresh artifact, clippy `-D warnings` on both crates. First run (ebd9884) failed visibly at the unpinned-hash step as designed; the first pinned run is recorded below.

## Linux CI evidence (GitHub-hosted ubuntu runner)

{
 "recorded_at_utc": "2026-10-14T17:55:54.914788+00:00",
 "run_id": "38605447811",
 "url": "https://github.com/SchemeWorks/MusicalChairs/actions/runs/38605447811",
 "runner": "ubuntu-latest (GitHub-hosted)",
 "linux_artifact_sha256": "b824ec0bffc02fe9547c8337a5fbacfb3b1c462e32a9a9a3bf70e0579f76961d",
 "linux_source_set_sha256": "4d4258073424de99b64afd7d315ede7a57ce231927cae2120c10f3667d3abfd0",
 "linux_lock_sha256": "77f859f9a7ad4e2bfdd836b66b9353bf79b49041e69760d3adc9b93245a2601f",
 "standin_artifact_sha256": "955441aa488074c17af35e1cf191a564977f5a810da91a3622927ee8de0ee7ca",
 "cargo_build_sbf_version": "cargo-build-sbf 3.1.0-beta.0",
 "platform_rustc": "rustc 1.84.1-dev",
 "test_results": [
  "ok. 175 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 823.25s",
  "ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s",
  "ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 10.22s",
  "ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 46.34s",
  "ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 7.65s",
  "ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 303.74s",
  "ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.23s",
  "ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s"
 ],
 "steps": {
  "Set up job": "success",
  "Run actions/checkout@v4": "success",
  "Run dtolnay/rust-toolchain@1.92.0": "success",
  "Read the pinned archive hashes": "success",
  "Restore the pinned toolchain archives": "success",
  "Download and verify the pinned toolchain archives": "success",
  "Unpack the toolchain into the job-local directory": "success",
  "Build the program and the vault stand-in with the pinned tools (provenance guard on)": "success",
  "Assert the source set and lock against the committed pins; record the artifact hash": "success",
  "Run the LiteSVM harness on the fresh artifact": "success",
  "Clippy (deny warnings)": "success",
  "Post Restore the pinned toolchain archives": "success",
  "Post Run actions/checkout@v4": "success",
  "Complete job": "success"
 },
 "clippy": "success",
 "artifact_vs_macos_pin": "Artifact differs from the local macOS build (Linux b824ec0b… vs pinned d77b23bf…): expected for a different host/path; source set and lock asserted equal to the pins",
 "note": "Linux bytes are not required to equal the macOS bytes; the source set and lock are asserted equal to docs/solana/evidence/latest-build.json",
 "harness": "success",
 "tests_passed_total": 195,
 "macos_artifact_sha256_pinned": "d77b23bffddccdc74d7003ec5e5d4948d6b99a9d0b0d90dcb47bc0702eba0958",
 "artifact_bytes_equal_macos": false,
 "proof_level": "GitHub-hosted Linux runner executed the pinned production SBF build, the whole LiteSVM harness and clippy; Linux ELF hash recorded on its own; source set and lock asserted equal to the committed pins; cross-host bytes are not required to be equal"
}

Linux bytes are not required to equal the macOS bytes (same source set and lock; cross-path/host builds differ in layout, M7 receipts); the job asserts the source set and lock and records its own artifact hash.

## Findings for Codex

1. **M9-F1 — DECIDED by Codex (M9 review of 9f3da9a): accept and document; no new deliverability mechanism, no cash-progress gate.** `claim_backer` credits with `validate_destination` only; when the cluster rent rule refuses a dust credit to a data-less destination the whole transaction fails, program state is unchanged and the credit stays claimable to a fundable destination — the same accepted limitation as page cleanup (M8-F1). Source-backed cluster rule (Agave `transition_allowed`, pinned against `credit_is_deliverable` in `tests/transfer_constraints.rs`) vs local execution (LiteSVM 0.7.1 skips rent-state checks for data-less accounts, so the dust claims in the measurements §3 succeed locally). Both M10 personas independently reached the same verdict (SEC-7 / ECO-4).
2. **Codex M9 review items (ebd9884) resolved in 9f3da9a**: permanent CloseRecord retained rent accounted once in the measurements document, budget and receipt; job-local stand-in target in CI; Linux archives pinned from official sources and the SDK path corrected to `bin/platform-tools-sdk/sbf`.
3. **Codex M9 corrections of 9f3da9a / 3c3003c resolved in 37185ea** (Codex checkpoint: independent focused review of 37185ea PASS): (a) the second inline entry point — matured Compound15 / Compound30 `settle_compounding` with the registry's accounts appended equals the chunked ticket at N = 1, 4, 8 on the same ELF `d77b23bf…` (PayoutPaid gross / Jackpot toll / net identical, toll == gross × jackpot_bps / 10,000 with the frozen 900 / 1,300 pins, Compound15 402,620,918 as the positions suite pins; TicketCommitted carries the whole toll; every Backer, page and ticket byte-identical; 82,013 … 146,553 CU, 827 … 1,058 B; malformed layouts WrongAccount / DuplicateWritableAccount fee-only with the position still open and a successful well-formed retry); pot fixture 100 SOL because a 10-SOL pot routes Compound30 (gross 12.27 SOL) to the closing path (`PageNotProvisioned` without close pages — the intended barrier); (b) budget line names the permanent CloseRecord funded OUTSIDE the refundable close reserve; (c) receipt `limits` labels the inline envelope estimate HISTORICAL and the measured N = 1..8 rows (both entry points) actual local SBF evidence.

## Caveats a reviewer should know

1. Every number is LiteSVM at the local default rent fixture and the legacy transaction format; the 696-lamport column is the conditional terminal SIMD-0437 step, not a cluster quote.
2. LiteSVM does not check rent state for data-less accounts (dust claims to fresh wallets succeed locally).
3. The envelope family's N = 16/17 rows are rejected transactions (`InvalidState`); their packet sizes are those of the refused transaction.
4. The fuzz and measurement suites add about three minutes to `cargo test`, CI included.
5. The CI job downloads ~730 MB of pinned archives per cold cache; `actions/cache` keys them by hash.

## Proof level

GREEN on real lamports in LiteSVM for the measurement matrix and for the inline step (byte-identical to the chunked ticket, real packet/CU recorded); the production SBF build, provenance guard and the whole harness run on a GitHub-hosted Linux runner with pinned tools (evidence above); no deployment or live action.

## Staffing

Coordinator (Claude Fable 5.1): first execution and one page-edge fix of IMPL-39a, program clippy cleanup, harness clippy cleanup, CI job, Linux pins, inline LiteSVM rows, envelope adaptation, receipts, graph. Sonnet workers: IMPL-39a (measure suite), IMPL-39b (inline step, isolated worktree). Codex: bounded M9 review with three corrections.

## Solana notes

- A compute budget is per transaction (200k CU default, raisable to 1.4M with a compute-budget instruction): the inline payout at eight Backers stays under the default, so no extra instruction is needed; a chunk phase never exceeds ~60k.
- The legacy packet (1,232 B) binds before compute does: eight Backers plus two pages reach 1,058 B; sixteen reach 1,388 B. Address-lookup tables (v0 transactions) could extend this but are not needed for the pilot (DR-28).
- Rent is a refundable deposit and the only truly retained items per close are the permanent CloseRecord, the fulfilled request record, the consumed oracle fee and a new B account; ICP has no analogue because cycles are consumed, not deposited.

## Next

M10 (IMPL-40): targeted security review with the review personas (signer/owner/PDA/discriminator substitution, duplicate mutable aliases, reinit, overflow, replay, double claims, barrier bypass, destination substitution, fee-payer authority, config races, late/duplicate fulfilment, expiry-vs-capture race, sponsor withdrawal vs lock, cleanup predicates, CU exhaustion, heap in loops), fixes failing-test-first; then M11 keeper contract (incl. the page-edge batching rule from M8-F2) and M12/M13.
