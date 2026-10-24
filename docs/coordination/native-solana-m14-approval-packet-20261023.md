# Native Solana M14 approval packet (devnet acceptance gate)

Date 2026-10-23. Branch `codex/native-solana-production-core`, head `2f031cc`; revision 1 of this packet was committed as `fcc74df` [C] (plan Milestone 14, lines 705-717 of [the implementation plan](../superpowers/plans/2026-10-13-native-solana-implementation.md)). Prepared by a Sonnet worker for the coordinator.

**This packet authorizes and executes nothing.** No key was read or created, no transaction, airdrop, deploy or `dfx` write was made. The only network contact was Solana devnet JSON-RPC reads and anonymous `dfx` info/query calls, all listed in the [appendix](#10-appendix-every-read-only-query-and-local-check). Evidence labels used below:

- **[R]** observed read-only by this packet (appendix id in brackets, e.g. [R3]).
- **[S]** read from source at the cited file and line.
- **[M]** measured earlier by the project (local LiteSVM, not a cluster quote).
- **[C]** reported by the coordinator, not re-verified here.
- **[E]** estimate or inference, labeled as such.

**Revision 2 (M14-PKT-R, 2026-10-23)** applies Codex's four corrections. (1) Keeper durations are stated in nanoseconds, the C2 arming window is exact with hard caps (4f), and B1/B2/B4 carry single hard approval ceilings (4d). (2) RG-2 and RG-13 cite the accepted criterion (ELF hash equality, source-set equality, the accepted receipts) and call for a rebuild only on a concrete changed-source concern; the durable ELF copy is recorded. (3) RG-4/5/6 point at the owned local cards M14-PRE-ORAO and M14-PRE-SQUADS, separating optional preflight hardening from the required real devnet acceptance, and Block A is not gated on any ICP item. (4) Missing inputs are cut to four decisions plus defaults (9), the program-key wording is corrected with a fresh program-id envelope (5a), and the six-week calendar is validated from source with acceptance-preserving sequencing (6.4).

## 0. Revision 4: status and the one recommended path (2026-10-23)

**Recommendation: one bounded, Solana-devnet-first path.** Approve **Block A only**: devnet, test keys, public-faucet SOL, at most 25 devnet SOL, using the mechanical defaults D1–D13. Run it as the committed Seq-B plan. Hold every ICP deployment and upgrade (B1–B4), every activation (C1, C2) and any real PP minting (C3) until they are approved separately.

**The smallest genuinely required authority decision:**
1. **Approve Block A on devnet.** In order:
   - Fresh devnet keys, generated and held by the coordinator for test use only.
   - Public-faucet devnet SOL, within the 25 SOL cap.
   - Deploy the pinned ELF with the operator tool, then move the upgrade authority to the G vault.
   - Real Squads G (2-of-3, 172,800 s) and P (2-of-3, 0 s).
   - `initialize` and ORAO `register` through G.
   - The 41.5-day Seq-B flow plan.
   - The deployed-byte proof and conservation reconciles.
2. **Say whether anyone holds the key for program id `7tj9…`.** If nobody does, the fresh devnet program-id envelope (5a) is used. Its source/IDL/PDA re-pinning and the normal local re-review happen BEFORE any devnet transaction, and need no further approval.

**Ready now (local evidence, no public action):**

| item | status | evidence |
| --- | --- | --- |
| Candidate pins | ELF `8f97e83d…` and source set `994a9ff4…` still equal the accepted pins; a durable ELF copy is verified | 5 (RG-2), `~/.cache/mc-solana-artifacts/` |
| RG-5 devnet ORAO binary | PASS, 10/10 close-suite flows on the exact devnet binary (LiteSVM) | `docs/solana/evidence/2026-10-23-m14-preflight/` |
| RG-6 real Squads v4 | PASS, 12/12; real Squads forwards the external sponsor signer, so `initialize` runs through a real vault transaction | same |
| RG-4 operator tool `devnet_ops` | PASS. LiteSVM, gate and pin tests: 18/18. Loopback-validator lifecycle with the real Squads and devnet ORAO binaries: real `multisig_create_v2`, real loader-v3 deploy, SetAuthority to the G vault, `verify-bytes`, idempotent re-run | `docs/solana/devnet-ops.md`; same evidence dir |
| RG-7 flow plan | PASS. The 39-step Seq-B plan reached all 15 flow codes in clock-warped LiteSVM on the devnet ORAO binary, with conservation checked after every step | `docs/solana/m14-flow-plan.md`; `m14-flow-report.json` |
| Agave validator (for local checks only) | re-provisioned to `~/.cache/mc-solana-tools/agave/`, sha256 `de9be527…` OK | evidence NOTES.md |

**Conditions carried into execution (enforced by the tool, not by hand):**
- **Packet size.** `vault_transaction_create` fits the 1,232-byte packet only when the proposer is also the fee payer: measured 1,140 bytes, with execute at 878 bytes. `devnet_ops` always builds it that way, refuses any transaction over 1,232 bytes before signing, simulates before every send, and refuses execute before `approved_at + time_lock`.
- **Real ownership constraints.** The program's upgrade authority ends as the G vault. ORAO `Client.owner` is the G vault and `Client.state` is the RngAuthority PDA. `Admission.guardian_vault == Pool.guardian_vault == P vault`. All of these are read back by `devnet_ops readback`.
- **Approval binding.** Devnet `--execute` requires `--i-have-approval <sha256 of this packet file>` and the devnet genesis hash. The tool never targets mainnet.

**Calendar corrected by the simulation.** First draw request at day **11.45**: day 10.7 is not reachable once Series A and candidates exist, and 11.36 is the earliest with a deposit of at most 1 SOL. Total duration is **41.46 days**. Test wallets need about **2.12 SOL**, sponsor operations 0.5 SOL, and the deploy peak stays at about 14.2 SOL, all within the 25 SOL cap.

**Remaining local work (no approval needed; does not block approving Block A):**
- **M14-RUN** (in progress): the `devnet_ops flow` runner that executes the committed plan step by step under the same interlocks. On devnet it waits for real ORAO fulfilment and records latency (A7).
- **Only if MI-1 resolves to the fresh id:** change `declare_id!`, rebuild with the pinned platform-tools, and re-pin and re-review per 5a.

**Held, each needing its own approval later:** B1 bridge, B2 keeper (unarmed), B3 live Shenanigans upgrade, B4 devnet frontend canister, C1 activation, C2 keeper arming, C3 real PP mints.

For B3, the separate legacy memo card's conditions apply. **F-1:** after any Shenanigans deploy containing the memo fix, the ordinary observer retries begin paying previously failed signup gifts and in-flight backer attempts at the current configuration. "No recovery code" does not mean "no payouts". **F-4:** the exact deploy candidate's build, module hash and stable signature must be pinned before any upgrade request. The old local rehearsal is not proof of deployed bytes. LEG-2 recovery stays separate.

## 1. Smallest combined approval request

Recommended: answer **A, B1, B2 as "yes, go when the coordinator reports the A0 items of section 5 closed"**, and hold **B3, B4, C1, C2, C3** until the evidence named in section 6 exists. **Block A is not gated on any ICP build, cycles calibration or B/C row.** Each row is one independent yes/no.

| id | yes / no | exactly what (and what it is not) | hard ceiling | depends on |
| --- | --- | --- | --- | --- |
| **A** | ____ | ONE Solana-devnet block A1-A9: fresh devnet keys and public-faucet SOL; deploy the hash-pinned ELF `8f97e83d...` (or the re-pinned ELF if the 5a fallback is taken); upgrade authority to G; two real Squads v4 multisigs (G 2-of-3 / 172,800 s, P 2-of-3 / 0 s); initialize and ORAO-register via G; test wallets; the plan flow list, deployed-byte proof, conservation reconciles. No ICP, no mainnet, no real funds | 25 devnet SOL total (deployer 15; any other transfer 2) | A0 items; MI-1, MI-2 |
| B1 | ____ | Create and install `native_sol_bridge` on the IC (new canister), devnet config, no relayer | **2 T cycles** total incl. creation | A6 green; MI-3 |
| B2 | ____ | Create and install `native_sol_keeper` (new canister), devnet config, **unarmed** | **3 T cycles** total incl. creation | A6 green; MI-3 |
| B3 | ____ | Upgrade the LIVE Shenanigans `j56tm-...` to the native branch (R6 behind an empty allowlist): stop / snapshot / upgrade / start / state diff. Highest risk | at most 0.1 T consumed; 1 T headroom over the freezing threshold | A green; B1; MI-3, MI-4 |
| B4 | ____ | Devnet frontend build (devnet RPC and CSP) on a NEW asset canister, never `5qu42-...` | **3 T cycles** total incl. creation | B1; MI-3 |
| C1 | ____ | `setRelayers`, `setNativeBridgeAllowlist`, one native policy version | none | B1, B3 |
| C2 | ____ | Arm the keeper watchdog ONCE: opens when close #2's closing payout confirms, closes at `finalize_round` or after 45 min; at most 6 signatures (`maxSignaturesPerHour = 6`, `maxFeeLamportsPerTx = 5_000`) | 1 T consumed in the window | B2; close #2 reached in A7 |
| C3 | ____ | PP path acceptance: one scripted SIWS login of a test keypair on the production `siws_provider`, then real PP mints for one `PositionOpened` and one `PromotionApplied` | at most 5,000 PP | B1, B3, C1; MI-4 |

Two answers are needed with the approvals because they decide whether A can start: **MI-1** who holds the key for program id `7tj9...`, if anyone (if nobody, or it will not be provided, approve the fresh devnet program-id envelope of 5a), and **MI-2** whether the acceptance-preserving schedule of about 41 days (6.4) is acceptable. Mechanical setup (keys, amounts, which ELF, RPC) follows the defaults in section 9.

## 2. Candidate pins

Everything in this table that is a hash was either re-computed read-only in this packet (local check id in brackets) or taken from the named receipt.

| item | value | how known |
| --- | --- | --- |
| Branch / head | `codex/native-solana-production-core` / `2f031cc` | [L1]; draft review PR #125 (kanban row PR-125) [C] |
| `source_set_sha256` | `994a9ff48fa2603318d4b28198f9f43d61418fa9566cf128662c32b68ac0d08f` (19 files, helper `solana/tests/financial-runtime/scripts/source-set-digest.sh`) | pinned in [latest-build.json](../solana/evidence/latest-build.json); working tree recomputed equal [L4] |
| `lock_sha256` | `77f859f9a7ad4e2bfdd836b66b9353bf79b49041e69760d3adc9b93245a2601f` (`solana/programs/musical_chairs/Cargo.lock`) | pinned; matches the file hash in the [L4] listing |
| **ELF to deploy** (reviewed macOS artifact) | sha256 `8f97e83db9dbc3112b1c18b5a35dfd5cfe6ae86bcb1a0cc93da78f7bb18dc826`, 1,397,160 bytes (`elf_len`), file mtime 2026-10-14 10:49 | on disk at `solana/programs/musical_chairs/artifacts/musical_chairs_program.so` (gitignored) and in the durable copy `~/.cache/mc-solana-artifacts/8f97e83db9dbc3112b1c18b5a35dfd5cfe6ae86bcb1a0cc93da78f7bb18dc826/musical_chairs_program.so` with `SHA256SUMS` and `PROVENANCE.txt`; both re-hashed now and `shasum -c` passes [L2, L14] |
| ELF build conditions | macOS arm64, platform-tools v1.48 (rustc 1.84.1-dev), cargo-build-sbf 3.1.0-beta.0, built at path `/Users/robertripley/.codex/worktrees/native-solana-model-probes/musicalchairs` | [latest-build.json](../solana/evidence/latest-build.json) |
| Acceptance criterion (what "the reviewed ELF" means) | Hash equality of the ELF to be deployed with the pinned sha256, source-set equality with the pinned `source_set_sha256`, and the accepted receipts (M0 same-path reproducibility, M10 Linux CI cross-check, Codex acceptance). The scratch toolchain and replica state having disappeared does not invalidate them. A rebuild is called for only by a concrete changed-source or changed-artifact concern (for example 5a) | plan M14/C5; RG-2 |
| Reproducibility caveat | Byte-identical only for a clean rebuild from the SAME path on the SAME platform tools (M0 receipt); another path or host gives different code bytes with the identical source set (M7 receipts). The cause is unverified. A byte search of the ELF finds only 12 `/Users/runner/work...` strings (the platform-tools std sources) and no worktree or cargo-home path [L7], so the dependence is not through embedded path strings | [C] for the receipts; [R] for the string check |
| CI Linux artifact (cross-check only) | sha256 `a398d6f4b5fba0fe78c76cc5bcbe79bbbbc725bfd25451eeaab14bb9a161f297` from run 38614117412 on `f132fd4` (ubuntu-latest, pinned Linux tools); source set and lock asserted equal to the pins; **bytes differ from the macOS ELF by host and path, as the pin file says**; 210 harness tests and clippy passed on it | [2026-10-14-m10-fixes.json](../solana/evidence/2026-10-14-m10-fixes.json) `linux_ci`. Latest exact-head CI named by Codex: run 38705116196 at `c7ccc6e`, four jobs green [second closure review](native-solana-r1-r5-second-closure-review-20261017.md). CI at `2f031cc` was not read here |
| Which one is deployed | Recommended: the macOS `8f97e83d...` (the artifact every receipt and local run used). The Linux ELF is a second valid build of the same source set but is not what was reviewed locally | recommendation [E]; default D5 |
| Committed IDL | sha256 `11840ae04eaebff5d75a1952e87577c280a79f5b4e42550319201cdf3e9bb597`, byte-identical copies at `solana/programs/musical_chairs/idl/musical_chairs.json` and `frontend/src/solana/native/idl/musical_chairs.json` | [L3]; CI step "Committed IDL matches a fresh generation" (`solana/scripts/check-idl.sh`) reported the same hash on run 38614117412. No on-chain IDL account is planned, so no IDL account hash |
| Vault stand-in (localnet ONLY, never deployed to devnet) | `eb78e77fc107c45d38d7e45bb365d632a8418909a3b0c79dbca26f7c20f109c8` (macOS); CI Linux `955441aa488074c17af35e1cf191a564977f5a810da91a3622927ee8de0ee7ca` | [latest-build.json](../solana/evidence/latest-build.json), [m10-fixes](../solana/evidence/2026-10-14-m10-fixes.json). Devnet uses the real Squads v4 program |
| Program id | `7tj9biW3KRJ7EEWmVUGigHiouCTXhV2dzcyvwma7Cyu7` (`declare_id!`, [S] `lib.rs:27`); a valid ed25519 public key [L9]; no keypair file was found by filename search [L11], which does not show that nobody holds one; the build wrapper deletes the random keypair it generates unread [S] `build-sbf.sh:47-59` | MI-1 (who holds it, if anyone); fallback 5a |
| Devnet cluster domain (32 bytes) | `musical-chairs-devnet-domain-v1!` = hex `6d75736963616c2d6368616972732d6465766e65742d646f6d61696e2d763121`; localnet is `musical-chairs-localnet-dev!!!!!` (do not reuse) | [S] `frontend/src/solana/native/cluster.ts:35-38`, `sampling.rs:106`; [L8] |
| Devnet genesis hash pin | `EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG` | [S] `cluster.ts:46`; to be checked live at A1 (`getGenesisHash` was not in this packet's allowed method list) |
| Toolchain archives | platform-tools-osx-aarch64 v1.48 `799e4cfcee38e227155c83fb2294f96f94a843d42e01c0359fb7128e2216d2dd` (422,197,780 B); agave v3.1.0-beta.0 macOS aarch64 `de9be527da14757854c47defffffeb0d17d1b7b6c0a4175f46f531aca262bad9` (77,248,568 B); Linux archives pinned for CI | [latest-build.json](../solana/evidence/latest-build.json), [m0-toolchain.json](../solana/evidence/2026-10-14-m0-toolchain.json) |

**Solana note (ICP comparison).** On ICP a canister's module hash is one number the replica reports (`dfx canister info`). On Solana the deployed bytes live in a `ProgramData` account behind a 45-byte loader-v3 header, and the deploy allocates `max_data_len`, so "deployed equals reviewed" is a three-part statement: header decodes, the `elf_len` bytes equal the ELF, the rest is zero padding. That is exactly the check in section 6 (A8).

## 3. Intended devnet targets and roles

### 3a. Program and PDAs (derived locally; no key needed)

If the fresh-id fallback (5a) is taken, every address in this table is re-derived; the ORAO and Squads program addresses do not change, the ORAO Client does. Derived by [L8] with `@solana/web3.js` from the repository's `node_modules`; four of these match [docs/solana/local-cluster.md](../solana/local-cluster.md) (Pool, ProgramData, ORAO client, RngAuthority), which validates the derivation. Devnet check [R6]: none exists today.

| account | address | seeds / note |
| --- | --- | --- |
| Program | `7tj9biW3KRJ7EEWmVUGigHiouCTXhV2dzcyvwma7Cyu7` | absent on devnet [R1] |
| ProgramData | `9fbEej8qbMTZbWBgN19AME3WA6kRSWE3JmHM1hfX8Lup` | PDA of the loader; absent [R6] |
| Pool | `AYEAZnLGHzjV95vMtRu4StknBBRtmZUGLwK297Na1jqK` | `[pool]`; absent [R6] |
| Admission | `EMmmtPRp62FrxeGNwm2NWqvQvZ1xdJmqHSczL3RRh43A` | `[admission]` |
| GameVault | `8Me6QeLPzYKzvSv1jsy6W8n6UYNHMN4MYe8jdUa3ZXwa` | `[game-vault]` |
| Ops | `3aNRyVcEMQxXWqAxciDv77gKWMv8d7PRmfePJdjcriiv` | `[ops]` |
| OpsCash | `4FSt2vmzZmniwMmT9LWBU9UvttLLkE1DPXNPsfKadqEc` | `[ops-cash]`, system-owned, data-less |
| Ticket | `E6Kv9gFiA87x1vuAvagweYD9hV1cvEpv4sp5PKCy4t3M` | `[ticket]` |
| RngAuthority | `6n6uusHJtZXSDSZec1Vt8T53hgLS6vp3iULH5ew1omm5` | `[rng-authority]` |
| ConfigVersion 1 | `FxieAQkL3Ur1PKh8Gs3WvJ92hXyRVymbqxgVczdZ5uFU` | `[config, 1u32 le]` |
| Event authority | `BuzaujH9dKaTPZGcLmRe3SA1DNvHJGN9GAGrGVAzwZd6` | `[__event_authority]`; the bridge `event_authority` config value |
| First pages | Registry 0 `24ZGCA7ebUqsmNg3fSizAiifL8jtidEJccdqVJMhqri6`; Snapshot 0 `7tHni27iz5gJSFU7pc9tGEDrHDiYyk5zoxtcrDavukF9`; Participants round 1 page 0 `82Tyc7ytGNWDkqR5KxSr3E6tNU1oW6vNjMRJipiCgcfk` | provisioned at genesis in the local flow ([S] `local_cluster.rs:252-254`) |

**Solana note.** A PDA (program derived address) is an address with no private key, derived from seeds and a program id, closest to an EVM CREATE2 address or an ICP canister id you can compute in advance. That is why these addresses are known before anything is deployed, while the Squads G and P addresses below are not (they depend on a key chosen at creation).

### 3b. Governance G and guardian P (Squads v4, real program on devnet)

Squads v4 `SQDS4ep65T869zMMBKyuUq6aD6EgTu8psMjkvj52pCf` is deployed on devnet, executable, loader-v3, ProgramData `Fy3YMJCvwbAXUgUM5b91ucUVA3jYzwWLHL3MwBqKsh8n` (1,774,845 bytes, authority `HM5y4mz3Bt9JY9mr1hkyhnvqxSH4H2u2451j7Hc2dtvK`, deployment slot 446,638,068) [R2, R3, R5]. Its `ProgramConfig` (`BSTq9w3kZwNwpBXJEvTZz2G9ZTNyKBvoSeXMvwb4cNZr`) shows `multisig_creation_fee = 0` and treasury `HM5y4mz3...` [R3].

| field | G (governance) | P (guardian) | source |
| --- | --- | --- | --- |
| multisig address | `PDA(["multisig","multisig",create_key_G], SQDS4...)` = unknown until the create key is generated at A1 (D2); not invented here | same with `create_key_P` | [S] `controls.rs:1406-1410` |
| vault (the signer the program sees) | `PDA(["multisig", G_multisig, "vault", [0]], SQDS4...)` | same for P | [S] `runtime.rs:1910-1915`; vault index 0 [S] `controls.rs:39` |
| `config_authority` | zero pubkey (autonomous) | zero | exact, [S] `controls.rs:1397-1401` |
| threshold / members | 2 of exactly 3 distinct non-zero members; each needs the Vote bit and no unknown bits; at least one member with Initiate and one with Execute | same | [S] `controls.rs:1402-1432` |
| `time_lock` | **172,800 s exactly** | **0 exactly** | [S] `controls.rs:32,1403`; `initialize` passes 0 for P at `controls.rs:338-340` |
| proposed test voters | 3 fresh devnet keys per multisig (D2); same keys for G and P is allowed but loses the independence the mainnet launch needs | same | decision |

Do **not** reuse the local-cluster fixture multisigs (`5ASstTEw...`, `HfbGhr2Y...`): their create keys are constants, not keys anyone can sign with, so they cannot be created on a real cluster.

**Solana note (ICP/EVM comparison).** A Squads vault is a PDA, so it can only "sign" when the Squads program executes an approved vault transaction. The closest ICP analogue is an SNS or multi-controller proposal that only runs after a delay; the closest EVM analogue is a Safe with a timelock module. The `time_lock` is therefore a wait of 48 hours between the second approval and execution. Both `initialize` and the ORAO registration are G actions, so **the real run cannot start before a 48 hour wait** (6.4).

### 3c. ORAO VRF on devnet (observed read-only)

| item | value | source |
| --- | --- | --- |
| ORAO Callback program | `VRFCBePmGTpZ234BhbzNNzmyg39Rgdd6VgdfhHwKypU` is deployed on devnet (executable, loader-v3), ProgramData `7mzfTTbQ4h7S9uNFdzbVUCs7srF6hiLa52y4eotkp1MD` (616,509 B), deployment slot 403,064,376, authority `9ZTHWWZDpB36UFe1vszf2KEpt83vwi27jDqtHQ7NSXyR` | [R2, R3] |
| NetworkState | `3e483QYyYQqu8eesT6ZjfuBiWF9PN5JXKeB8TRmn3hUr`, owner is the ORAO program, 245 B; `request_fee = 300,000`, `callback_deadline = 9,000` slots, treasury and authority both `9ZTHWWZD...`, **four** fulfil authorities (`2VYNr8yM...`, `A9iZdgYa...`, `9ZTHWWZD...`, `HSCqQaDH...`), lifetime `num_requests = 44,183`, `num_registered = 509`, `num_terminated = 0` | [R3] decoded locally by [L12] |
| Registered Client for us | `7b1ebeG8qgEtCB3ufPHcZLiUeFJ6cYMS9BeiTnSBJ6HT` (derived), absent today [R6]; after A5 it must read `Client.owner == G vault`, `Client.state == RngAuthority`, account owner == ORAO program | [S] spec / plan M14 readback line |
| Registration payer | the **G vault** (the ORAO `Register` payer must be the program upgrade authority, per the Codex review packet's R5 row) [S] `local_cluster.rs:236-241`; the vault needs a small SOL balance first (D13) | local genesis flow |
| Provider tuple for `initialize` | program `VRFCBeP...`, network state `3e483QYy...`, treasury `9ZTHWWZD...`, client `7b1ebeG8...`, `provider_request_fee_lamports = 300,000` (equals the live devnet fee), `provider_fee_headroom_bps = 2,500` (the local-genesis value; M9 reserve arithmetic used 2,000, so confirm) | [S] `local_cluster.rs:194-206`; [M] measurements section 4 |

**Finding RG-5 (material).** The devnet ORAO bytes are **not** the mainnet ELF the project tested against. Same length (616,464 bytes after the header) but sha256 `b7f30b89cda7d3f067fbb36f2e817a5e50c8f510f0f8f65f5b87d5a65526ccea` versus the captured mainnet fixture `6d180b1e91be7d5c541f75e2413146beb047d9beeb76ac8eb1d7c883200c6b8b`; 304,671 byte positions differ, the first at offset 24 [R5, L10]. Every local ORAO result (LiteSVM, local cluster, the 759 B / 147 B request sizes, the `Register` account order) was produced on the mainnet build. Compatibility of the devnet build is unproven until it is exercised locally with these exact bytes and then on devnet.

### 3d. Wallets and keys (none exists today; this packet created and accessed none)

By default (D1-D3 in section 9) the coordinator generates every devnet test key fresh at A1 and holds it for test use only, in a 0700 directory outside any git worktree, never `~/.config/solana/id.json`, never committed. Addresses enter the receipts only after creation, so none is invented here. The one key that is Rob's to answer for is the program-id key (MI-1).

| role | purpose | default |
| --- | --- | --- |
| Deployer D | fee payer and temporary upgrade authority until A3 completes | fresh devnet keypair (D1) |
| Squads create keys (2) | sign multisig creation; the secret is not needed afterwards | fresh keypairs, discarded after creation (D2) |
| G voters (3), P voters (3) | test voters; threshold 2 of 3 | six fresh keys, three distinct per multisig (D2) |
| Sponsor S | `initialize` signer and sponsor recipient; pays shared storage; funds operations | fresh devnet keypair (D3) |
| Management recipient | receives accrued Front-End Load | a different fresh keypair, so `withdraw_management` can be observed (D3) |
| Keeper fee payer K | worker CLI `mc-keeper` fee payer; holds fees only | fresh keypair; the ICP watchdog address is separate and only known after B2 |
| Players P1-P5, backers A1-A2, plain wallet W | test wallets; W submits the composed finalize/reopen transaction | fresh keypairs (D3) |
| Program-id key for `7tj9...` | signs the first deploy only | MI-1: Rob's, if it exists; otherwise the fresh-id fallback (5a) |
| Management / sponsor identities for mainnet | not part of M14 | M15 |

### 3e. ICP targets

| target | id / status | source |
| --- | --- | --- |
| `native_sol_bridge`, `native_sol_keeper` | in `dfx.json` but absent from `canister_ids.json`: never created | [S] `dfx.json`, `canister_ids.json`; [C] "never deployed" |
| Live Shenanigans | `j56tm-oaaaa-aaaac-qf34q-cai`, module `0x7fa413608bdb4d1412ff21e16c92421b6949c0bf3861738919e8a649874067bc`, controllers `6pwpo-...-tqe` (dfx identity `CharlesPonzi`) and CycleOps `cpbhu-5iaaa-aaaad-aalta-cai` | [R7]; matches [C] |
| SOL RPC canister | `tghme-zyaaa-aaaar-qarca-cai`, module `0x1733b1e5...3115` (unchanged since the 2026-10-13 capture in `Rpc.mo`), controllers `r7inp-6aaaa-aaaaa-aaabq-cai` and itself | [R8] |
| Production frontend | `5qu42-fqaaa-aaaac-qecla-cai`, module `0x423f20ee4e5daf8f76d6bb2b4a87440227f15b26cf874c132fd75d83e252c8f6`; **do not deploy the devnet build here** (RG-10) | [R9] |
| `siws_provider` | `tcm26-yqaaa-aaaac-qg2lq-cai`, module `0x9715ad271d09492b17b55b6087f9999a1438191ccd698372b909c90cecfb5f8e` | [R9] |
| PP ledger | `5xv2o-iiaaa-aaaac-qeclq-cai`, module `0x24e6b0b0...2bd1`, total supply 194,559,644,108,105 units (1,945,596.44 PP at 8 decimals), `max_memo_length` 32 [C] | [R9, R10] |

## 4. Funding sources and proposed hard budgets

### 4a. Pricing basis (read live)

Devnet rent today is **5,080 lamports x (bytes + 128)**, from 20 `getMinimumBalanceForRentExemption` quotes [R4]. The project's local figures use 6,960 per byte, so devnet is about 27 percent cheaper than the local measurements. Everything below uses the live devnet quotes; size inputs come from [measurements-2026-10.md](../solana/measurements-2026-10.md) and `state.rs`.

### 4b. SOL buckets (devnet SOL from the public faucet ONLY; no mainnet funds; never `~/.config/solana/id.json`)

| bucket | computation | cap |
| --- | --- | --- |
| **Program deploy (D)** | upload buffer R(1,397,197) = 7,098,411,000; ProgramData R(1,397,205) = 7,098,451,640; Program R(36) = 833,120. **Peak = 14,197,695,760 lamports (14.1977 SOL)** while both exist; **net retained = 7,099,284,760 (7.0993 SOL)** after the buffer's lamports are returned. Write transactions at about 1,000 B each: roughly 1,400-1,700 x 5,000 = 0.007-0.009 SOL [E]. `max_len` = `elf_len`, so zero allocation padding (any headroom doubles ProgramData rent; D5) | 15.0 SOL |
| Squads | two 231-byte multisigs R(231) = 1,823,720 each = 3,647,440 (creation fee 0 [R3]); proposals and vault-transaction accounts about 0.1 SOL for about 6 proposals [E]; six voter wallets x 0.05 SOL fees | 0.5 SOL |
| Sponsor S | initialize accounts: Pool 2,550,160 + Admission 904,240 + GameVault 695,960 + Ops 1,630,680 + Ticket 1,874,520 + RngAuthority 858,520 + ConfigVersion 2,057,400 + OpsCash R(0) 650,240 = **11,221,720**; first pages 2,326,640 + 2,280,920 + 3,337,560 = **7,945,120**; extra pages about 0.02 SOL; **operations funding 0.5 SOL** (covers two closes below); ORAO client funding 0.05 SOL (local genesis used 50,000,000); fees | 1.0 SOL |
| G vault | ORAO Client R(123) = 1,275,080 paid at registration | 0.02 SOL |
| Keeper fee payer K | at most 1,000 transactions x 5,000 | 0.1 SOL |
| Watchdog fee address (B2, C2) | floor 10,000,000 + 6 x 5,000 = 10,030,000 lamports (4f) | 0.02 SOL |
| Test wallets | deposits at most **6.0 SOL** gross in total; Series A at most 0.5 SOL; 8 wallets x 0.05 SOL for rent and fees (User 1,076,960 + UserRound 1,102,360 + Position 1,457,960 each, so about 5.1M per wallet with two positions) | 7.0 SOL |
| **Sum of buckets** | 15.0 + 0.5 + 1.0 + 0.02 + 0.1 + 0.02 + 7.0 | 23.64 SOL |
| **Total hard cap** | the 1.36 SOL slack covers retries only; it never lifts a per-action cap | **25 SOL** |

Close arithmetic behind the 0.5 SOL operations figure ([M] formula, devnet rents): required close reserve at 8 or fewer participants and no Backers = 2 x R(331) 4,663,440 + registry extension R(321) 2,280,920 + winner B R(147) 1,397,000 + pending ORAO request R(759) 4,505,960 + fee cap 375,000 = **13,222,320**; at 16 participants 17,885,760. Retained per close: CloseRecord R(657) 3,987,800 + fulfilled request 1,397,000 + fee 300,000 + a B account 1,397,000 = 7,081,800. Two closes lock about 0.027 SOL (most of it refundable) and retain about 0.014 SOL, so 0.5 SOL is roughly ten times the need.

### 4c. Per-action caps

| action class | cap |
| --- | --- |
| one faucet request | whatever the faucet allows; unverified here (the faucet was not queried) |
| one transfer between devnet keys | 2 SOL, except the deployer's own faucet funding (up to 15 SOL in total) |
| one program deployment (buffer + deploy) | peak 14.2 SOL, cap 15.0 SOL |
| one deposit | 1.0 SOL gross; Series A 0.25 SOL each |
| total gross deposits across all wallets | 6.0 SOL |
| priority fees | none (0) |
| any action that would exceed a bucket cap | stop, do not top up without a new approval |

Faucet feasibility is a real risk, not a detail: the deployer needs about 14.2 SOL at once and public devnet airdrops are small and rate-limited (limits not measured here). If the public faucet cannot supply the deployer's cap in a reasonable time, A stops. Another devnet faucet or a funded devnet account is a Rob decision (MI-5), never mainnet funds.

### 4d. ICP cycles: proposed hard approval ceilings

Supporting price facts read live [R11] (SOL RPC canister read costs on devnet defaults, in cycles): `getAccountInfo` 1,755,488,000; `getBlock` 1,754,712,800; `getSlot` 1,715,952,800; `getBalance` 1,733,619,200; `getSignatureStatuses` 1,745,655,200; `sendTransaction` 1,721,542,400; `getTransaction` 2,384,420,000 (default estimate), 12,244,311,200 at the 131,072 B estimate across the default providers, and **4,081,373,600** for a single provider at that estimate. Threshold Ed25519 signature: **26,153,846,153 cycles** at the production key `key_1` ([ICP budget](native-solana-icp-budget.md); not re-queried; the price of other key names is unverified). Canister creation is **assumed** at 0.5 T from the public price table and was not queried; if the real fee is higher the ceiling is too low and the step stops.

Each approval ceiling below is ONE number: the total cycles Rob authorizes to be spent or placed in that canister for the row, including creation. T = 10^12 cycles.

| row | **hard ceiling** | derivation kept as supporting evidence [E] |
| --- | --- | --- |
| B1 bridge | **2 T** | creation 0.5 T + C3 reads: two events x two single-provider reads x 4.08 B = 16.3 B (0.16 T even at 10 x retries); the code attaches 25 B per call (refunded), so at least 50 B must stay free; the rest of the ceiling is margin above the freezing threshold and storage |
| B2 keeper | **3 T** | creation 0.5 T + one C2 window at most 0.67 T computed in 4f (rounded up to 1 T) + setup calls (`getFeeAddress`, `setConfig`) at most 0.1 T + margin about 1.4 T. The canister is unarmed outside the window, so it runs no timer and polls nothing |
| B3 Shenanigans upgrade | **0.1 T consumed** | no creation; instruction cost of one upgrade of an existing canister; precondition: the balance (controller-only read) is at least 1 T above the freezing threshold |
| B4 frontend | **3 T** | creation 0.5 T + about 2 T working balance + margin; the upload of a few MB is negligible |
| **Sum of ceilings** | **8.1 T** | about 11 USD at the budget document's illustrative 1 T = 1 XDR = 1.366 USD; reprice at funding time |

Why the keeper is never armed open-ended (this replaces the earlier armed-for-weeks idea): at the default 300 s idle poll one `getAccountInfo` of Pool costs 1.755 B, 288 polls a day = 0.5055 T a day, about 21 T over 42 days. Only the bounded window of 4f is requested.

### 4e. What stays locked afterward

The 7.1 SOL ProgramData rent and the CloseRecord/page rents are recoverable only by closing the program or accounts under G (a 48 hour action) or by cleanup; devnet SOL has no value, so this is a faucet-throughput question, not a cost.

### 4f. Exact C2 arming window and hard caps

**Units.** Every duration field of `native_sol_keeper` is in **nanoseconds** [S] `native_sol_keeper/main.mo:82-86,222-230`, `Policy.mo:19-20`. The window uses these exact values: `alertAfterNs = 60_000_000_000`, `signAfterNs = 300_000_000_000`, `activePollNs = 30_000_000_000`, `idlePollNs = 300_000_000_000` (all the code defaults). For scale, 3,600 s would be `3_600_000_000_000` ns; it is not used.

**What the window demonstrates.** The watchdog's rescue of the close that follows a missed promotion: with the primary worker deliberately not running, the watchdog sends `expire_promotion` after the 600 s deadline plus margin, then `finalize_round`. These are two of the steps it supports; `build_candidates`, `request_round_draw` and `apply_promotion` it never sends [S] `operations.md` sections 3 and 6. The window rides inside flow F10 (close #2, the expiry path), so it costs no calendar time.

| rule | value |
| --- | --- |
| Number of windows | exactly **one**. A second window needs a new approval |
| Start rule | `arm` is called after the closing payout of close #2 has confirmed and before the 600 s deadline (`Pool.currentCloseId` set), with the fee address funded and the primary worker not running. The canister is disarmed at all other times |
| Stop rule (`disarm` at the FIRST of) | (a) `finalize_round` landed and `Pool.round_id` advanced; (b) 3 watchdog-signed transactions are journaled (2 are expected: expire and finalize); (c) **45 minutes (2,700 s)** after `arm`; (d) a `#Blocked` reason other than the expected `unsupported step: ...` (expected from the 300 s stale mark until the deadline passes), any `#Failed`, `#Expired` or `#Unknown` journal entry, a `cluster domain mismatch`, or the fee address below the floor |
| Expected duration | about 12 minutes: 600 s deadline + `deadlineMarginSecs = 15` + at most one 30 s poll + finalize |
| **Maximum duration** | **45 minutes** |
| `maxSignaturesPerHour` | **6** |
| **Total signature cap for the window** | **6**, by construction (a 45 minute window sits inside one rolling hour of the 6-per-hour gate [S] `main.mo:923`), with the operator rule of 3 in the stop rule |
| `maxFeeLamportsPerTx` | **5_000** (the minimum the canister accepts, one base signature fee [S] `main.mo:272`); `computeUnitPriceMicroLamports = 0`, so no priority fee |
| `minFeeBalanceLamports` | 10_000_000 (default); the fee address is funded with 0.02 SOL (floor plus 6 x 5,000 is 10,030,000) |
| **Cycles ceiling** | **1 T consumed in the window**; computed worst case 0.67 T [E]: 90 ticks x two reads (Pool and CloseRecord) x 1.755 B = 0.316 T; at most 90 signature-status reads x 1.746 B = 0.157 T; at most 6 signed transactions x (getSlot 1.716 + getBlock 1.755 + getBalance 1.734 + signature 26.154 + send 1.722 = 33.08 B) = 0.198 T |

## 5. Readiness gaps (actual)

Classes: **A0** = required before Block A starts; **OPT** = optional preflight hardening (evidence that can save a wasted 48 hour governance cycle; the REQUIRED M14 acceptance is the real devnet A4, A5 and A7); **B/C** = gates only the ICP rows. **Block A is not gated on any ICP build, cycle calibration or B-row item.** Nothing here needs Rob's approval to close except where the last column says so.

| id | class | gap | evidence | what closes it | approval needed |
| --- | --- | --- | --- | --- | --- |
| RG-1 | A0 (consequence of RG-4) | **Scratch toolchain gone.** `/private/tmp/mc-solana-runtime-tools-20261013` no longer exists; free disk is 20 GiB of 926 (98 percent used). This is not itself a reason to rebuild or rerun anything accepted (RG-2, RG-13) | [L5, L6] | Re-download the official archives (sha256 pinned in `latest-build.json`) to a durable path ONLY for genuinely new work: the devnet tooling (RG-4), the two optional compatibility cards (RG-5, RG-6) and, if taken, the rebuild of 5a. Free disk first | none (local-stage authority covers dependency fetches) |
| RG-2 | A0 (a check, not a rebuild) | **Accepted criterion for "the reviewed ELF".** The plan's M14/C5 evidence requires the deployed bytes to equal the reviewed ELF, identified by the sha256 pinned in `latest-build.json`, built from the pinned `source_set_sha256`; the accepted receipts (M0 same-path reproducibility, M10 Linux CI cross-check with source set and lock asserted, Codex acceptance) stay valid even though the scratch toolchain disappeared | [L2, L4, L14] | **Satisfied today:** the ELF hashes to `8f97e83d...` [L2]; the durable copy `~/.cache/mc-solana-artifacts/8f97e83db9dbc3112b1c18b5a35dfd5cfe6ae86bcb1a0cc93da78f7bb18dc826/musical_chairs_program.so` (+ `SHA256SUMS`, `PROVENANCE.txt`) passes `shasum -c` [L14]; the working-tree source set recomputes to `994a9ff4...` [L4]. A0 therefore needs only the same two hash comparisons repeated immediately before A3. A rebuild or rerun is called for ONLY on a concrete changed-source or changed-artifact concern, for example the program-id change of 5a, which changes source and so requires a rebuild plus the normal local re-review | none |
| RG-3 | A0 | **Program-id key (MI-1).** Whether anyone holds the keypair for `7tj9...` is unknown. A filename search found none on this machine [L11], which does NOT show that nobody holds one; the build wrapper discards the random keypair it generates [S] `build-sbf.sh:47-59`; the decision register lists "Program ID (deploy keypair)" as an operator input at deploy [S] `native-solana-decision-register-20261013.md:219` | [L11] | Rob answers MI-1. If a holder exists, the key signs the first deploy only (A3). If not, or it will not be provided, take the fresh devnet program-id envelope (5a) | Rob (MI-1); Codex for the re-pin |
| RG-4 | A0 | **Devnet tooling does not exist.** `local_cluster` targets a local validator with a Squads stand-in. Needed: loader-v3 deploy + set-authority + deployed-byte checker; real Squads v4 multisig create, vault transaction, proposal, approve, execute and config-transaction flows; ORAO register via a G proposal; flow runner; conservation reconciler; latency recorder. No Squads SDK is in `node_modules` [L13] | [C]; [L13] | Write and test it with the existing local harness (stand-in and captured ORAO, no devnet spend). Rehearsal against the real Squads and devnet ORAO binaries is the optional hardening of RG-5 and RG-6 | none |
| RG-5 | **OPT** (card **M14-PRE-ORAO**) | **Devnet ORAO build differs from the tested mainnet build** (3c). Also unknown: whether the devnet oracle fulfils our requests, and how fast | [R5, L10] | Card M14-PRE-ORAO: fetch the devnet ORAO program bytes read-only (slot, sha256, provenance) and run the existing provider/close flows (register, request, fulfil, capture, apply/expire) in LiteSVM with that exact binary through the harness ORAO-ELF override. Pass or fail per flow, with the exact binary hash. The required acceptance is the real devnet A5 and A7. **results (2026-10-23, coordinator-run, local LiteSVM only):** PASS. The devnet ORAO ELF (payload sha256 `b7f30b89…` = this packet's value; extracted ELF `0fbe2e1d…`, 612,272 bytes; fetched read-only at slot 514486473 and unchanged ~6,500 slots later) ran the SAME close-suite rows that pass on the mainnet binary: client register via the G-vault path, request_round_draw, fulfil, observe_request_fulfilled, capture_round_draw, apply_promotion, the expire path, provider return, provider-side rejections and the whole close suite — 10/10 passed (`MC_ORAO_PROFILE=devnet`, 83 s); the devnet NetworkState snapshot decodes (fee 300,000, four fulfil authorities). Default mainnet-profile regression unchanged (provider_setup row passes). Limits: fulfilment uses the harness's synthetic authority, so the real devnet oracle's liveness and latency remain the first A7 observation; LiteSVM's default rent schedule, not devnet's. Evidence: `docs/solana/evidence/2026-10-23-m14-preflight/` and `solana/fixtures/devnet/`. | none |
| RG-6 | **OPT** (card **M14-PRE-SQUADS**) | **Real Squads and the two-signer `initialize`.** `initialize` needs the sponsor keypair AND the G vault to sign [S] `controls.rs:296-298,1546-1548,1577`. The local stand-in forwards one inner instruction with the vault seeds while the sponsor signs the outer transaction. Whether the real Squads executor forwards a non-vault outer signature to the inner instruction is **not verified here** | [S] M3 handoff, "stand-in signer" | Card M14-PRE-SQUADS: real Squads v4 binary and `ProgramConfig` fetched read-only from devnet; G (2-of-3, 172,800 s) and P created; a vault transaction carrying `initialize` through proposal, approve, time-lock wait (clock warp), execute, recording exactly whether a non-vault outer signer is forwarded; then ORAO register via G. The required acceptance is the real devnet A4. A failure here is evidence that the same failure would occur on devnet (it would cost a wasted 48 h cycle); proceeding anyway is a Codex/Rob decision (for example sponsor = G vault). **results (2026-10-23, coordinator-run, local LiteSVM only):** PASS — **real Squads v4 forwards the non-vault sponsor signer.** With the devnet Squads v4 ELF (`1b4e69fd…`, 980,160 bytes; ProgramConfig and on-chain IDL v2.1.0 fetched read-only), a vault transaction carrying `initialize` with the sponsor as an extra inner signer went vault_transaction_create → proposal_create → approve ×2 → execute; execute before the time lock was refused with 6021 (TimeLockNotReleased) and after a 172,800 s clock warp it succeeded and `initialize` ran with the sponsor signing the OUTER transaction (outcome record `initialize_sponsor_outer_signer.json`). 12/12 Squads tests passed: G/P created exactly as the stand-in fixtures describe, threshold 2 enforced, time lock enforced, wrong G/P profiles and vault index 1 rejected by the program, guardian pause through P with no time lock, and ORAO registration through a real G vault transaction against the devnet ORAO binary. Packet sizes: vault_transaction_create 1,140 / 1,232 bytes ONLY when the proposer is also the fee payer (a separate payer would be ≈1,235 bytes and need Squads' buffer path — so the devnet tooling must make the proposer the fee payer); execute 878 bytes; peak 145,355 CU. The G-vault-as-sponsor fallback also works but is not needed. Limit: LiteSVM, not the devnet cluster; the real A4 remains the required acceptance. | Codex decision only if it fails (it did not) |
| RG-7 | A0 | **Flow scenario with solvency arithmetic.** Position payouts require the position's round to be the current one [S] `positions.rs:843`; compounding terms are 15 and 30 days [S] `state.rs:384`; the amounts and order of the flow list must be designed and simulated (clock-warped LiteSVM) before real days are spent | 6.4 | Write the scenario, run it in LiteSVM with warps, record expected balances per step | none |
| RG-8 | A0 (a design rule) | **Time-lock reduction test is destructive to G.** `require_governance` demands `time_lock == 172,800` exactly [S] `controls.rs:626-631,1403`, so after G shortens its own delay the program stops accepting G's governance instructions until the delay is restored | [S] | Schedule it per 6.4 (after initialize and register have executed; restore immediately) | none |
| RG-9 | B/C | **ICP: bridge and keeper never deployed; Shenanigans path.** Native branch `shenanigans/` is main plus R6 and the R1 reservation [C]. Live bytes are unverified against any commit (a local build of main hashes `13bddcc4...`, the live module is `7fa41360...`) [C]. The stable-compatibility check against the live signature needs a controller (anonymous gets 403) [R12] | [R7, R12] | At B3 time: `moc --stable-compatible` live to the exact B3 build with the controller identity, record the build's wasm hash, capture a state baseline, rehearse the snapshot restore on the local replica | Rob (MI-3: use of the controller identity for the read) |
| RG-10 | B/C | **Frontend.** Current CSP `connect-src` lists mainnet Solana hosts only [S] `frontend/public/.ic-assets.json`; one constant `SOLANA_RPC_ENDPOINT` also drives the LEGACY SOL balance and deposit paths [S] `frontend/src/solana/rpc.ts:18-19`, so a devnet build on the production asset canister would repoint legacy flows to devnet. The SIWS provider is configured for domain and URI `musicalchairs.fun` [S] `dfx.json`, so an interactive wallet sign-in from another origin may be refused (not tested) | [S] | Deploy to a NEW asset canister (B4); C3's SIWS binding is scripted with a test keypair, as the M12 local run did; add `https://api.devnet.solana.com` to that build's CSP and set `VITE_NATIVE_BRIDGE_CANISTER_ID` after B1 | B4 |
| RG-11 | B/C | **Sequencing with the legacy memo card.** LEG-1 (separate branch off `main`) changes the same Shenanigans file; if it ships first the live signature and module hash change and B3 must be re-based; if B3 ships first LEG-1 must rebase onto the native branch. Not an M14 gate: the native PP legs use their own frozen 32-byte hashed memo [S] `NativeRewards.mo:68,116,521-529,848`, and the bridge never calls the legacy mint path | [legacy memo evidence](legacy-pp-ledger-memo-evidence-20261023.md) | State the order explicitly at B3 | Rob (MI-4) |
| RG-12 | B/C | **ICP calibrations are placeholders.** Bridge `RPC_CYCLES` 25 B (a single-provider read costs about 4.1 B), keeper `rpcCycles` 10 B and `signCycles` 30 B, watchdog `Equality` consensus on `#Default(#Devnet)` without devnet evidence | [S] `native_sol_bridge/main.mo`, `native_sol_keeper/main.mo:237-238`; [R11] | Set from the 4d/4f numbers when B1/B2 are configured; record the devnet consensus behavior | none |
| RG-13 | B/C | **Local replica scratch state gone** (reboot) [C]. The accepted M12/M13 receipts remain valid; this alone is not a reason to rerun anything | [C] | Rerun a replica driver ONLY if a B-row canister's source or build artifact differs from what the accepted receipts cover (for example the keeper's regenerated `AuthorityTable.mo` under 5a). B-row builds (dfx 0.29.2, moc 0.16.2) are prerequisites of those rows, recorded as wasm hashes, not an A0 item | none |

### 5a. Fresh devnet program-id envelope (the recommended fallback for MI-1)

Used only if nobody holds the key for `7tj9...` or it will not be provided. **No key is created or accessed by this packet**; the new id would come from a fresh keypair generated at that time under the custody rule D1, needed once to sign the first deploy.

Recommended: use ONE fresh id for devnet AND mainnet, so the bytes proved on devnet are the bytes M15 deploys. A separate devnet-only id would make the devnet ELF differ from the eventual mainnet ELF (the id is compiled in), weakening the M14 evidence.

Found by a repo-wide grep of the literal id (68 files including this packet; the 11 M0-M9 evidence receipts under `docs/solana/evidence/` and the 4 bridge replica receipts are history and are NOT edited; a new receipt records old to new) [L15]:

| area | what changes | where |
| --- | --- | --- |
| source (changes bytes) | `declare_id!`; the 19-file source set therefore changes | `solana/programs/musical_chairs/src/lib.rs:27`; test constants in `src/sampling.rs` |
| pins re-derived | `source_set_sha256`; ELF sha256 (new durable copy with `SHA256SUMS` and `PROVENANCE.txt`); CI Linux artifact record. Unchanged: `lock_sha256` (no dependency change) | `docs/solana/evidence/latest-build.json` at promotion |
| IDL | the Anchor `address` field changes, so the IDL sha256 changes in both copies; the keeper's generated table embeds the id and the IDL hash and must be regenerated (`native_sol_keeper/test/gen/authority_table.py`, then `--check`) | `solana/programs/musical_chairs/idl/musical_chairs.json`; `frontend/src/solana/native/idl/musical_chairs.json`; `native_sol_keeper/AuthorityTable.mo` |
| derived addresses | every PDA in 3a (ProgramData, Pool, Admission, GameVault, Ops, OpsCash, Ticket, RngAuthority, ConfigVersion, event authority, the three first pages) and the ORAO Client PDA; re-run the derivation of [L8]. Not changed: ORAO program, NetworkState and treasury, Squads ids, the devnet cluster domain | section 3a/3c |
| protocol seed | the seed hash includes the program id, so the committed vectors regenerate | `solana/fixtures/seed-vectors.json`, `solana/fixtures/sampling.json` |
| fixtures | instruction encodings; the 29 captured RPC fixtures and manifest (re-capture from a local-cluster run); bridge and keeper test fixtures and generators; the bridge replica driver and run script; frontend tests that name the id | `solana/fixtures/ix-encodings.json`, `solana/fixtures/rpc/{closing,flows}-*.json`, `manifest.json`; `native_sol_bridge/test/{fixtures.test.mo,gen/genfix.py,replica/drive.py,replica/run.sh}`; `native_sol_keeper/test/{fixtures.mo,gen/pda_vectors.cjs,negative/WrongAuthorityTablePayerIsAuthority.mo}`; `frontend/src/solana/native/accounts.test.ts`, `frontend/src/components/NativeHistoryFeed.test.tsx`, `frontend/src/hooks/useNativeHistory.test.tsx` |
| frontend runtime | no literal in non-test code: `accounts.ts:49` and `cluster.ts:185,248` read `idl.address`, so the IDL copy is the single pin | `frontend/src/solana/native/` |
| ICP runtime config (set at B time, not source) | bridge `Config.program_id` and `event_authority`; keeper `programId` and its derived PDAs; `cluster_domain` unchanged | B1, B2 |
| local harness and docs | the id is named in the local cluster bin and its loader README; prose in `docs/solana/local-cluster.md` and `docs/solana/threat-model.md` | `solana/tests/financial-runtime/src/bin/local_cluster.rs`, `.../fixtures/loader/README.md` |

The normal local re-review this requires (the existing gates, re-run on the changed pins): restore the pinned toolchain; rebuild with `build-sbf.sh` at the same absolute worktree path with the provenance guard; recompute the source set; regenerate and compare the IDL (`check-idl.sh`); run the LiteSVM suites (210 tests) on the new ELF; regenerate the fixtures; keeper `authority_table.py --check` and the standalone `moc` suites for the keeper and bridge; frontend `tsc` and `vitest`; CI green at the new head; Codex acceptance of the new pins in a receipt. If a holder of `7tj9...` exists, none of this is needed.

## 6. Execution order with gates

Nothing starts before its predecessor's gate evidence is recorded in a receipt `docs/solana/evidence/2026-1N-NN-devnet-*.json` and the coordinator reports it. **Block A depends on no ICP build, cycles calibration or B/C row; B and C rows depend on Solana evidence, never the reverse.**

### 6.1 Block A: Solana devnet (one approval)

| step | action | gate to proceed | spend |
| --- | --- | --- | --- |
| **A0** (no approval) | Close the A0 items of section 5: the accepted-criterion check (ELF hash equals the pin, source set equals the pin, RG-2), MI-1 answered (RG-3, and 5a if taken), devnet tooling written and tested against the existing local harness (RG-4), the flow scenario simulated (RG-7), the time-lock test rule (RG-8). The optional cards RG-5 and RG-6 run in parallel if available | the file to be deployed hashes to `8f97e83d...`, the working-tree source set recomputes to `994a9ff4...`, the committed IDL hashes to `11840ae0...` (repeated immediately before A3); no rebuild unless a concrete changed-source concern exists | none |
| A1 keys and funding | the coordinator generates the keys (D1-D3); check `getGenesisHash` equals the devnet pin; faucet airdrops to D first (up to 15 SOL), then the other buckets | genesis hash matches; balances within bucket caps | faucet only |
| A2 Squads | create G (2-of-3, 172,800 s, `config_authority` zero) and P (2-of-3, 0 s) with the test voters; read both back through RPC and through the program's own validation rules | profile bytes equal the table in 3b; vault PDAs derived and recorded | about 0.004 SOL rent + fees |
| A3 deploy and authority transfer | write the buffer, deploy with `max_len = elf_len`, **run the deployed-byte check (A8, first pass) BEFORE the transfer**, then `set-upgrade-authority` to the G vault (the vault is a PDA and cannot co-sign, so the new-authority signer check must be skipped [E]) and read `ProgramData` back | bytes equal the ELF; `upgrade_authority == G vault`; if the transfer fails D keeps authority and the run stops | 14.2 SOL peak, 7.1 net |
| A4 initialize via G | in ONE sitting after A3, create and approve BOTH G vault transactions: `initialize` (args: sponsor = S, management recipient, devnet cluster domain, the provider tuple in 3c, `public_series_a_enabled = true`) and ORAO `Register` (payer G vault, RngAuthority PDA seeds). Two approvals each, then **wait 172,800 s from the approvals** (one wait, not two); execute `initialize` | `Pool`, `Admission`, `Ops`, `ConfigVersion#1` readback; `Pool.cluster_domain` = devnet bytes | about 0.011 SOL |
| A5 ORAO register and funding via G | execute the approved `Register` straight after `initialize` (it needs the RngAuthority account that `initialize` creates); then fund the Client, `fund_operations`, provision the first pages | ORAO Client readback (3c) | about 0.6 SOL |
| A6 readbacks | G and P profiles, `time_lock`, vault derivation; `Admission.guardian_vault == Pool.guardian_vault == P vault`; `Client.owner == G vault`, `Client.state == RngAuthority`, account owner == ORAO program; `ProgramData` authority; provider tuple in `ConfigVersion#1` | all equal expected | none |
| A7 flows | the list in 6.5, each followed by A9 | each step's receipt | within buckets |
| A8 deployed-byte evidence | exact C5 procedure of the plan (first pass in A3, repeat after A7; no upgrade happens in M14) | see section 8 | read-only |
| A9 conservation | after every action recompute both identities from RPC: game vault `vault - rent == pot + seed + management + pending_toll + funded_claims + excess` and the OpsCash partition identity [S] `operations.md` step 4 and `threat-model.md` section 1 | equal, else stop | read-only |

Starting the 48 hour clock BEFORE the deploy would save only the hours the deploy takes and would void the proposals if the deploy has to be redone at another id, so it is not recommended.

### 6.2 Block B: ICP, each separately approved (recommended only after A6 is green)

Prerequisite common to B1-B4, recorded as part of the row and not an A0 item: build the wasm or asset bundle from the committed source with the project's own toolchain (dfx 0.29.2, moc 0.16.2) and record its sha256 for the module-hash comparison.

| step | action | gate | notes |
| --- | --- | --- | --- |
| **B1** (ceiling 2 T) | create `native_sol_bridge` with the MI-3 controller set, install the built wasm, `setConfig` with program id, devnet cluster domain, event authority `BuzaujH9...`, two DIFFERENT explicit provider lists (D7), Shenanigans principal; leave `setRelayers` empty | module hash from `dfx canister info` equals the built wasm sha256; `getConfig` shows the expected values and no RPC sources | inert: no relayer can submit |
| **B2** (ceiling 3 T) | create `native_sol_keeper`, install, `setConfig` (devnet cluster domain, devnet sources, `feeKeyName = key_1`, the 4f caps in nanoseconds), `getFeeAddress`, fund that address with 0.02 SOL, **do not arm** | module hash equals the built wasm; fee address derived once and stable | first proof that threshold Ed25519 works on the IC |
| **B3** (0.1 T consumed) | the Shenanigans upgrade: baseline queries -> `stop` -> `dfx canister snapshot create` -> upgrade (additive stable vars only) -> `start` -> state diff vs baseline -> restore the snapshot on any mismatch; allowlist stays empty | stable-compat check passed; wasm hash recorded; baseline equal after upgrade | highest risk: this is the production PP economy canister and R1 touches three lines of the legacy gift path [C] |
| **B4** (ceiling 3 T) | build with devnet RPC, `VITE_NATIVE_BRIDGE_CANISTER_ID` = B1, devnet CSP; deploy to a NEW asset canister | served asset hash equals the build output hash; version marker matches | never the production canister |

### 6.3 Block C: activation, each separately approved

| step | action | gate |
| --- | --- | --- |
| **C1** | bridge `setRelayers` (MI-3), Shenanigans `setNativeBridgeAllowlist [bridge]`, `appendNativePolicyFromMintConfig` or a lower devnet draft (MI-4) | `getRelayers`, `getNativeBridgeAllowlist`, `getNativePolicyTable` equal the intended values |
| **C2** | the single bounded window of 4f (nanosecond values, 45 minute maximum, 6 signatures, 1 T) | `getStatus`: `armed`, `feeAddress`, `lastTimerAt` advancing during the window; journal shows the expire and finalize steps; `disarm` confirmed afterwards |
| **C3** | PP path acceptance: for one genuine finalized `PositionOpened` and one `PromotionApplied`: event -> two RPC observations -> provenance verification -> `observeNativeEvent` (unbound) -> scripted SIWS login of a test keypair -> `bindAndReserve` -> `executeReservedLeg` / `executeNativeRewardLeg` -> ledger receipt and `reconcileLegFromLedger` -> assert no legacy duplicate reward for the principal | legs Committed with ledger block indices; PP minted within the 5,000 PP cap. **This mints real PP on the production ledger from valueless devnet play.** The promotion basis is the winner's loss at 24,000 PP per SOL, so the cap holds only if every candidate's loss in that round is at most about 0.2 SOL; the draw picks the winner, so the scenario must bound all candidate losses (or Rob raises the cap). Live rates are 6,000 / 12,000 / 18,000 PP per SOL (Simple / Compound15 / Compound30), 24,000 for Backers, signup gift 500 PP [R10]; a 0.01 SOL Simple event is 60 PP, a 0.01 SOL promotion loss 240 PP, plus the gift and cascade/house legs. The legacy mint path currently fails its gift mint on long memos, so "no legacy duplicate" is weak evidence on mainnet until LEG-1 ships; the exclusion logic itself is proven locally (G1-G6) |

### 6.4 Calendar, validated from source

The claim "about six weeks" is confirmed: the shortest acceptance-preserving schedule is **40.74 days** (about 5.8 weeks). Every delay, with its source:

| leg | duration | source |
| --- | --- | --- |
| G governance delay, per G action | 172,800 s (2 days) after the second approval | [S] `controls.rs:32,1403`; `validate_profile` demands it exactly |
| G actions before the first flow | **2**: `initialize` and ORAO `Register`. Both are approved in one sitting, so ONE wait, not two. The program places no ordering constraint between G proposals; Squads proposals are independent objects [E: Squads v4 behavior not verified here, covered by M14-PRE-SQUADS]. Execute `initialize` first, then `Register` | [S] `controls.rs:296-298`; `local_cluster.rs:236-241` |
| Earliest first flow | 172,800 s after the approvals: **day 2.0** | arithmetic |
| Compound15 maturity | 1,296,000 s (15 days) from open, pinned in the position at open | [S] `state.rs:384` (`plan_durations_s[1]`), `positions.rs:672`, settlement gate `positions.rs:932` |
| Compound30 maturity | 2,592,000 s (30 days) | [S] same |
| Simple term | 1,814,400 s (21 days); payouts any time | [S] same |
| Payout only in the position's own round | a position of a closed round fails `PositionRoundClosed` | [S] `positions.rs:843` |
| One close per round; deadline | the close pins `close_started_at + 600` once; expiry needs `now > deadline` | [S] `state.rs:34`, `close.rs:1403-1410` |
| Insolvency (closing) condition | strict `gross_owed > pot` | [S] `positions.rs:144-146` |
| Time for a lone Simple depositor to become insolvent | pot credit is 96 percent of gross (4 percent front-end load [S] `positions.rs:499-507`, `state.rs:385-392`), Simple accrues 11 percent of gross a day, so `0.11 x t > 0.96` gives t > 8.727 days = **754,037 s (8 d 17 h 27 m)**; any other deposit lengthens it in proportion | [E] arithmetic from the cited rates; scale-invariant in the deposit size |

**Why 40.74 days cannot be shortened without changing terms.** Three serial legs cannot overlap: the 2 day governance wait; one round that stays open for the 30 day Compound30 maturity (a position cannot be paid after its round closes); and one DIFFERENT round whose close needs an insolvency, at least 8.73 days for a lone Simple depositor, because each round has exactly one close and the plan needs two closes. 2 + 30 + 8.727 = 40.73 days, plus minutes.

Acceptance-preserving schedules (all flow coverage kept, production economics unchanged):

| | Seq-A: compound-first | **Seq-B: Simple-first (recommended)** |
| --- | --- | --- |
| Round 1 | Simple, Compound15, Compound30 positions and Series A opened at day 2 | a dominant Simple position plus small candidate positions and Series A opened at day 2 |
| Round 1 close #1 (draw request, capture latency, apply, finalize/reopen, combined claim) | day 32.0: the Compound30 settlement is the insolvent closing payout | **day 10.73**: the Simple payout is the insolvent closing payout |
| Round 2 | opens day 32; a lone Simple position opened at once | opens day 10.73; Compound15 and Compound30 positions opened at once |
| Compound15 settlement | day 17.0 (solvent, round 1) | day 25.73 (solvent, round 2) |
| Close #2 (expiry path, C2 window) | day 40.73: the insolvent Simple payout | day 40.73: the insolvent Compound30 settlement |
| **Total** | **about 40.74 days** | **about 40.74 days** |
| First live draw/promotion evidence | day 32 | **day 10.7** |
| If something is wrong | found late (day 32) | found about three weeks earlier (day 10.7) |

Both keep every plan flow. Seq-B is recommended because it brings the Squads, ORAO and promotion evidence forward by about three weeks for the same total.

Parallelism that costs no extra days: both G proposals in one sitting (saves 2 days against doing them one after the other); the G time-lock reduction test F11 proposed right after A6, once `initialize` and `Register` have executed (a config change can make earlier unexecuted vault transactions stale [E]), executed at day 4 and restored at once so G stays valid (restoring is immediate at the reduced delay); the optional P pause rehearsal F12 (P delay is 0); the C2 window inside F10; pre-provisioning the round 2 participant page before the round reopens.

**Not acceptance-preserving, shown only for completeness (NOT recommended):** staging a devnet-only ConfigVersion with shortened `plan_durations_s` (G staging with a 48 hour wait, applies only from the next round per the accepted defaults; not re-verified in source here) would finish in about 11 days (2 + 8.73), but it would exercise non-production terms, so it does not prove the production Compound15/30 maturities on a cluster. It is listed so Rob can see the trade; it is not part of this request.

### 6.5 Flow list (A7) on the recommended Seq-B, day = days after the G approvals

| id | flow (from the plan) | day |
| --- | --- | --- |
| F1 | Series A funding by A1 (minimum 0.05 SOL) | 2 |
| F2 | round 1 deposits: one dominant Simple position (P1) plus small Simple candidates (P4, P5, each at the 0.01 SOL minimum) | 2 |
| F3, F4 | Simple payout and partial payout: scheduled by the RG-7 scenario into either round (a payout shifts the insolvency day, so the scenario fixes which) | 3 on, per scenario |
| F5 | combined claim: A alone before the first promotion; A+B in one instruction after it | 3 / after F9 |
| F8 | close #1: provision CloseRecord, closing payout (insolvent), build candidates, `request_round_draw`, `capture_round_draw`; record request slot, fulfil slot and seconds | about 10.73 |
| F9 | `apply_promotion`, then finalize/reopen via a composed transaction from the plain wallet W; provision the round 2 participant page before the first round 2 deposit | about 10.73 |
| F7 | round 2: Compound15 settlement (solvent) at 15 days after open, Compound30 settlement (insolvent) at 30 days | 25.73 / 40.73 |
| F6 | cap completion (a Series A reaches 124 percent), most likely at the Compound15 toll | about 25.73 |
| F10 | close #2 expiry path: the Compound30 settlement starts the close; do not capture for more than 600 s; `expire_promotion`; finalize. The C2 window (4f) covers this | about 40.74 |
| F11 | G reduces its own time lock; document; restore | 2 to 4, in parallel |
| F12 (optional, no plan line) | P pause and unpause rehearsal: shows exits keep working while paused | any time |

## 7. Stop and recovery conditions

"Stop" means: take no further action, write what is known into the receipt, tell the coordinator, and wait for a decision.

| step | stop condition | recovery |
| --- | --- | --- |
| A0 | at the pre-deploy comparison the ELF hash, source-set hash or IDL hash differs from the pins | do not deploy; re-pin only through Codex with a new receipt. A rebuild is justified only by a changed source or artifact (for example 5a), not by the scratch state having disappeared |
| A0 | an optional preflight (RG-5, RG-6) fails | this is evidence, not a gate by itself: the same failure would occur on devnet. Coordinator reports to Rob and Codex; proceeding anyway is their decision |
| A1 | genesis hash is not the devnet pin; faucet cannot supply a bucket; any key file inside the repo or `~/.config/solana/id.json` used | stop, no mainnet funds ever; rotate any exposed key |
| A2 | Squads profile bytes differ from 3b | multisigs are cheap: create new ones (new create keys); nothing depends on them yet |
| A3 | deploy rejected (for example the devnet feature set refuses the ELF) | the uploaded buffer remains; close it to recover its rent. Byte check fails before transfer: D still has authority, redeploy or abandon the id |
| A3 | upgrade-authority transfer fails | stop with D retaining authority, record |
| A4 | proposal stale or execution fails | re-propose (a new 48 h wait); **a wrong `cluster_domain` cannot be changed after initialize**: pre-flight the args by simulation and compare to hex `6d75736963616c2d...763121` before proposing |
| A5 | ORAO `Register` rejected by the devnet build | stop; compatibility finding for Codex |
| A6 | any readback differs from expected | stop |
| A7 | unexpected program error; ORAO does not fulfil within the window repeatedly; bucket cap reached; devnet genesis changes | stop; the single-miss case is an allowed `PromotionSkipped(Deadline)` outcome and is recorded, not a failure |
| A9 | either conservation identity breaks | stop all flows; P pause only under the [operations runbook](../solana/operations.md) conditions (conservation break is one) |
| B1, B2 | module hash differs from the built wasm; `schnorr_public_key` fails; the 0.5 T creation assumption proves too low | reinstall in place; keeper stays inert and unarmed; never delete a canister; do not exceed the ceiling |
| B3 | stable-compat check fails; any baseline difference after the upgrade; trap during upgrade | do not upgrade / restore the snapshot; the IC rolls back a trapping upgrade automatically |
| B4 | served asset hash differs; CSP missing the devnet host | redeploy to the same NEW canister |
| C1 | allowlist or policy differs from intended | set again (admin); nothing minted yet |
| C2 | any stop-rule clause of 4f: 45 minutes elapsed, 3 signed transactions, finalize landed, an unexpected blocked reason, a failed/expired/unknown journal entry, fee address below the floor | `disarm` at once; the window is spent and a second one needs a new approval |
| C3 | minted PP exceeds the cap; unexpected duplicate or unbound event | stop the relayer; there is no burn path, so minting is irreversible: keep the cap small |
| any | cycles or SOL ceiling reached | stop; new approval needed |

## 8. Expected acceptance evidence

| plan item | evidence | where |
| --- | --- | --- |
| Deploy reviewed `.so`; `Program` / `ProgramData`; authority = G vault; `initialize` | signatures, slots, addresses, `Program`/`ProgramData` readback, authority readback, `Pool` readback | `2026-1N-NN-devnet-deploy.json` |
| Deployed-byte evidence (C5) | on-chain `ProgramData` fetched; 45-byte header decodes to `{ slot, upgrade_authority_address }` (loader-v3 `size_of_programdata_metadata() == 45`); `data[45 .. 45+elf_len] == ELF`; `data[45+elf_len ..]` all zero with its length recorded as allocation padding (zero when `max_len = elf_len`); never trim trailing zeros of the ELF. Record `elf_len` 1,397,160, `data.len()`, loader format, `upgrade_authority`, deployment slot, `source_set_sha256`, lock / artifact / IDL sha256, IDL account hash (none published). Uploaded buffer, deployed bytes, `initialize` activation, broadcast signatures, finality and reconciled balances kept as separate facts | `2026-1N-NN-devnet-deployed-bytes.json` |
| Readbacks | G and P profiles, `time_lock`, vault derivation; `Admission.guardian_vault == Pool.guardian_vault == P`; ORAO `Client.owner == G vault`, `Client.state == RngAuthority`, account owner == ORAO program; the G time-lock reduction and its consequence | `2026-1N-NN-devnet-governance.json`, `...-orao.json` |
| ORAO client registration and funding | register / fund signatures, Client balance, devnet binary hash `b7f30b89...` recorded at the time | `...-orao.json` |
| Flows F1-F12 | per flow: transaction signatures, finality, compute units, packet bytes, before/after account balances | `...-flows.json` |
| Draw latency | request slot and time, fulfil slot and time, capture slot and time, seconds; also the keeper-worker step timings | `...-latency.json` |
| Conservation after every action | both identities recomputed from RPC, with the RPC slot | `...-conservation.json` |
| PP path (C4) | event id, two observations, provenance result, bind result, leg ledger block indices, `reconcileLegFromLedger`, no legacy duplicate | `...-pp-path.json` |
| ICP deployments | `dfx canister info` module hash vs built wasm sha256 for `native_sol_bridge`, `native_sol_keeper`, upgraded `shenanigans`; snapshot ids; before/after state diff; measured cycles per call vs the 4d estimates | `...-icp-deploy.json` |
| Keeper window (C2) | window start and stop times, `getStatus` at arm and at disarm, journal entries (signature, step, status), signature count (at most 6), cycles before and after (at most 1 T consumed), fee-address balance | `...-keeper-window.json` |
| Frontend | served asset hash and version marker vs build output; CSP header as served; screenshots of the native panels against devnet | `...-frontend.json` |

## 9. Missing user inputs

### 9a. Decisions that are genuinely Rob's (four, plus one conditional)

| id | decision | recommended answer | blocks |
| --- | --- | --- | --- |
| MI-1 | **Who holds the key for program id `7tj9...`, if anyone.** (A filename search found none on this machine; that does not show that nobody holds one.) If a holder exists, the key signs the first deploy only | If nobody holds it, or it will not be provided: approve the fresh devnet program-id envelope (5a) with ONE fresh id used for devnet and mainnet | A |
| MI-2 | **Accept the schedule:** about 41 days, acceptance-preserving, production terms (6.4), recommended Seq-B | accept Seq-B | A |
| MI-3 | **ICP authority for the B rows:** which dfx identity and cycles source pays, who controls the new canisters, which principal is the bridge relayer, and use of the controller identity for the read-only stable-signature and cycles reads | the existing `CharlesPonzi` identity pays; controllers `CharlesPonzi` and CycleOps `cpbhu-5iaaa-aaaad-aalta-cai` (as for Shenanigans); a dedicated script identity as relayer | B1-B4, C1 |
| MI-4 | **Production-canister choices:** B3 order against the legacy memo fix LEG-1 (RG-11); PP issuance on the production ledger by C3, capped at 5,000 PP (this also bounds every candidate loss in the C3 round); native policy (copy the live rates or a lower devnet draft) | B3 proceeds on the current live baseline and LEG-1 rebases afterwards; cap 5,000 PP; copy the live rates | B3, C1, C3 |
| MI-5 (conditional) | If the public faucet cannot supply a bucket: another devnet source (never mainnet funds) | ask only if it happens | A1 |

### 9b. Defaults for mechanical setup (no decision needed; Rob may veto any)

| id | setting | default |
| --- | --- | --- |
| D1 | Key generation and custody | every devnet test key (deployer, 2 create keys, 6 voters, sponsor, management recipient, keeper payer K, 8 test wallets) is generated fresh on devnet by the coordinator at A1 and held by the coordinator for test only, in a 0700 directory outside any git worktree; never `~/.config/solana/id.json`; never committed or pasted into a document; compromise means rotate |
| D2 | Multisig shape | G and P each 2-of-3 with three distinct voters (six voter keys), every member with the full permission mask, `config_authority` zero; G `time_lock` 172,800 s, P 0; create keys discarded after creation |
| D3 | Sponsor, management recipient, test wallets | sponsor S and management recipient are two different fresh keys; 5 players, 2 backers, 1 plain wallet W, 1 keeper payer K; amounts and caps as in 4b (deposits at most 6.0 SOL in total, Series A at most 0.5 SOL, 0.05 SOL rent/fee buffer per wallet) |
| D4 | Funding | public devnet faucet only; bucket and per-action caps of 4b/4c; stop on a shortfall (MI-5) |
| D5 | ELF and capacity | the macOS `8f97e83d...` (durable copy in `~/.cache/mc-solana-artifacts/`); `max_len = elf_len`, no headroom (headroom doubles ProgramData rent) |
| D6 | Solana RPC | the public devnet endpoint for the tooling and the browser; no API key |
| D7 | ICP bridge RPC providers | two different supported providers, `AlchemyDevnet` and `AnkrDevnet`, no keys; any future key stays in a secret store and is set only by a controller |
| D8 | Keeper | `feeKeyName = key_1` (the key the budget prices); parameters and the single window exactly as in 4f, in nanoseconds |
| D9 | Frontend | B4 on a NEW asset canister; production `5qu42-...` untouched |
| D10 | C3 SIWS login | scripted with a test keypair, as the M12 local run did |
| D11 | Optional P pause rehearsal (F12) | included (no extra funds; one P proposal) |
| D12 | Time-lock test timing (F11) | proposed after A6, executed at day 4, restored at once |
| D13 | Pool and provider parameters | `provider_request_fee_lamports = 300_000` (the live devnet fee), `provider_fee_headroom_bps = 2_500`, `public_series_a_enabled = true`, sponsor-funded shared storage; operations funding 0.5 SOL, ORAO client funding 0.05 SOL |

## 10. Appendix: every read-only query and local check

Network reads used only `curl https://api.devnet.solana.com` (methods `getEpochInfo`, `getAccountInfo`, `getMinimumBalanceForRentExemption`, `getProgramAccounts`) and `dfx ... --network ic --identity anonymous` (`canister info`, `canister call --query`, `canister metadata`). Revision 2 ran no new network read; its additions are local checks [L14]-[L17]. Intermediate files were written only to the session scratchpad (`.../scratchpad/m14/`), not to the repository. No private key, keypair file or wallet was read; the keypair search [L11] listed file names only.

### Devnet JSON-RPC (RPC = `https://api.devnet.solana.com`, commitment finalized)

Command form for every call below (one `curl` each, about one second apart): `curl -s https://api.devnet.solana.com -H 'content-type: application/json' -d '{"jsonrpc":"2.0","id":1,"method":"<METHOD>","params":[<PARAMS>]}'` with `<PARAMS>` as follows. `getEpochInfo`: `[{"commitment":"finalized"}]`. `getAccountInfo`: `["<ADDRESS>",{"encoding":"base64","commitment":"finalized","dataSlice":{"offset":<O>,"length":<L>}}]`. `getMinimumBalanceForRentExemption`: `[<BYTES>,{"commitment":"finalized"}]`. `getProgramAccounts`: `["<PROGRAM>",{"encoding":"base64","commitment":"finalized","dataSlice":{"offset":0,"length":0},"filters":[{"dataSize":374}]}]`.

- **[R1]** `getEpochInfo` and `getAccountInfo` of `7tj9...`:
  `{"result":{"absoluteSlot":514479595,"blockHeight":501682643,"epoch":1190,"slotIndex":399595,"slotsInEpoch":432000,"transactionCount":19737689425}}`; our program: `"value":null` (apiVersion `4.4.0-beta.1`, slot 514479601).
- **[R2]** `getAccountInfo` (dataSlice 0..64) of the ORAO program: `"executable":true,"lamports":1001141440,"owner":"BPFLoaderUpgradeab1e11111111111111111111111","space":36`; data decodes to variant 2 and ProgramData `7mzfTTbQ4h7S9uNFdzbVUCs7srF6hiLa52y4eotkp1MD`. Squads program `SQDS4ep6...`: same shape, `"lamports":7001141440`, ProgramData `Fy3YMJCvwbAXUgUM5b91ucUVA3jYzwWLHL3MwBqKsh8n`.
- **[R3]** `getAccountInfo` slices: ORAO ProgramData (45 B header) `AwAAADhGBhgAAAAAAX8tzSqkJaJKvw2P4Stgqo9HaDcND9mcc4rv5vIVDwO4` space 616509 -> variant 3, slot 403064376, authority option 1, `9ZTHWWZDpB36UFe1vszf2KEpt83vwi27jDqtHQ7NSXyR`; ORAO `NetworkState` `3e483QYy...` owner `VRFCBePm...`, space 245, lamports 2596080, decoded: bump 255, request_fee 300000, callback_deadline 9000, num_requests 44183, num_registered 509, num_terminated 0, four fulfil authorities; Squads `ProgramConfig` `BSTq9w3k...` owner `SQDS4ep6...`, space 144, decoded `multisig_creation_fee = 0`, treasury `HM5y4mz3Bt9JY9mr1hkyhnvqxSH4H2u2451j7Hc2dtvK`; Squads ProgramData header `AwAAAPQnnxoAAAAAAfLgOQly218S1MHPUjHNpjnX3gm0lT/hoviih5dsbH7Q` space 1774845 -> slot 446638068, authority `HM5y4mz3...`.
- **[R4]** `getMinimumBalanceForRentExemption` (lamports): size 0 = 650240; 9 = 695960; 36 = 833120; 41 = 858520; 50 = 904240; 123 = 1275080; 147 = 1397000; 193 = 1630680; 231 = 1823720; 241 = 1874520; 277 = 2057400; 321 = 2280920; 330 = 2326640; 331 = 2331720; 374 = 2550160; 529 = 3337560; 657 = 3987800; 759 = 4505960; **1397197 = 7098411000; 1397205 = 7098451640**. All fit 5,080 x (bytes + 128).
- **[R5]** `getAccountInfo` of ORAO ProgramData with `dataSlice {offset:45,length:616464}`, context slot 514479948, hashed locally: sha256 `b7f30b89cda7d3f067fbb36f2e817a5e50c8f510f0f8f65f5b87d5a65526ccea` (4,207 trailing zero bytes in the slice). Compared with the mainnet fixture [L10]. Same method on Squads ProgramData with `{offset:45,length:1774800}`, context slot 514479961: sha256 of the whole post-header slice `97fb2a5c08e5df5e862d8354ac5b97d3587117f98c4457754ca12e30bef0664d` (794,655 trailing zero bytes; sha256 of the trimmed bytes `57a8d2d7ef5409df250415135f29f83da876e5adf5a2e5b5752e323cfb4f2f74`). Recorded so a later check can tell whether devnet Squads or ORAO changed under the run.
- **[R6]** `getAccountInfo` (dataSlice length 0) of Pool PDA `AYEAZnLG...`, our ProgramData `9fbEej8q...`, ORAO Client PDA `7b1ebeG8...`, the local-fixture G vault `nVdpWArd...`: all `"value":null` (slot 514482015-514482029). `getProgramAccounts` owner `7tj9...` with filter `dataSize 374`: `"result":[]`.

### ICP, anonymous

- **[R7]** `dfx canister info --network ic --identity anonymous j56tm-oaaaa-aaaac-qf34q-cai` -> `Controllers: 6pwpo-d5iaw-mfjrn-owfb3-v4oz6-72woh-pc5t2-cwn73-zrzeq-4bjeh-tqe cpbhu-5iaaa-aaaad-aalta-cai`; `Module hash: 0x7fa413608bdb4d1412ff21e16c92421b6949c0bf3861738919e8a649874067bc`.
- **[R8]** same for `tghme-zyaaa-aaaar-qarca-cai` -> controllers `r7inp-6aaaa-aaaaa-aaabq-cai tghme-zyaaa-aaaar-qarca-cai`; module `0x1733b1e5a2c14f9488b5314d6c0d189669b3841b2d360a4f88a554216c423115`.
- **[R9]** same for `5qu42-fqaaa-aaaac-qecla-cai` (module `0x423f20ee4e5daf8f76d6bb2b4a87440227f15b26cf874c132fd75d83e252c8f6`, controllers `6pwpo-...-tqe cpbhu-... fk56k-diaaa-aaaac-qg6mq-cai ft3ml-...-aae`), `tcm26-yqaaa-aaaac-qg2lq-cai` (module `0x9715ad271d09492b17b55b6087f9999a1438191ccd698372b909c90cecfb5f8e`), `5xv2o-iiaaa-aaaac-qeclq-cai` (module `0x24e6b0b09ba44b1123453877994bb59ce75555c2e33f19f58163a3e0c6e62bd1`).
- **[R10]** `dfx canister call --network ic --identity anonymous --query j56tm-... getMintConfig '()'` -> `simple21DayPpPerSol = 6_000`, `compounding15DayPpPerSol = 12_000`, `compounding30DayPpPerSol = 18_000`, `backerPpPerSol = 24_000`, `signupGiftPp = 500`, `cascadeInitialBps = 1_000`, `cascadePassthroughBps = 5_000`, `observerIntervalSeconds = 21_600`. `... 5xv2o-... icrc1_total_supply '()'` -> `(194_559_644_108_105 : nat)`.
- **[R11]** `dfx canister call --network ic --identity anonymous --query tghme-... <method>CyclesCost` with `variant { Default = variant { Devnet } }` unless noted: `getTransactionCyclesCost` default -> `Ok = 2_384_420_000`; with `responseSizeEstimate = 131072` -> `12_244_311_200`; `Custom [AlchemyDevnet]` default -> `794_743_200`, with 131072 -> `4_081_373_600`; `Custom [AnkrDevnet]` default -> `795_015_200`, with 131072 -> `4_081_645_600`; `getAccountInfoCyclesCost` -> `1_755_488_000` (with dataSlice 0..64: `1_756_997_600`); `getSignatureStatusesCyclesCost` -> `1_745_655_200`; `sendTransactionCyclesCost` -> `1_721_542_400`; `getBlockCyclesCost` -> `1_754_712_800`; `getSlotCyclesCost` -> `1_715_952_800`; `getBalanceCyclesCost` -> `1_733_619_200`. `getProviders '()'` lists AlchemyDevnet, AnkrDevnet, ChainstackDevnet and others (output truncated here).
- **[R12]** `dfx canister metadata --network ic --identity anonymous j56tm-... motoko:stable-types` and `motoko:compiler` -> `status 403 Forbidden ... can only be requested by the controllers of the canister`. The coordinator read both with the controller identity [C] (compiler 0.16.2).

### Local read-only checks (no network)

- **[L1]** `git rev-parse --short HEAD` in the worktree -> `2f031cc` (the only git command run; read-only).
- **[L2]** `shasum -a 256 solana/programs/musical_chairs/artifacts/musical_chairs_program.so` -> `8f97e83db9dbc3112b1c18b5a35dfd5cfe6ae86bcb1a0cc93da78f7bb18dc826`; `stat` -> 1397160 bytes, `Oct 14 10:49:03 2026`.
- **[L3]** `shasum -a 256` of both IDL copies -> `11840ae04eaebff5d75a1952e87577c280a79f5b4e42550319201cdf3e9bb597` (identical).
- **[L4]** `bash solana/tests/financial-runtime/scripts/source-set-digest.sh --list solana` (reads files, builds nothing) -> `source_set_sha256=994a9ff48fa2603318d4b28198f9f43d61418fa9566cf128662c32b68ac0d08f`; listing shows `programs/musical_chairs/Cargo.lock 77f859f9a7ad4e2bfdd836b66b9353bf79b49041e69760d3adc9b93245a2601f`.
- **[L5]** `ls -la /private/tmp/mc-solana-runtime-tools-20261013` -> `No such file or directory`.
- **[L6]** `df -h /private/tmp` -> `926Gi size, 838Gi used, 20Gi avail, 98%`.
- **[L7]** byte search of the ELF for absolute-path strings -> 12 distinct strings, all under `/Users/runner/work`; counts of `mc-solana-runtime-tools`, `cargo-home`, `/Users/robertripley`, `musicalchairs` all 0.
- **[L8]** `NODE_PATH=<worktree>/node_modules node derive.js` (`PublicKey.findProgramAddressSync`, scratchpad script) -> the addresses in 3a and: ORAO NetworkState `3e483QYyYQqu8eesT6ZjfuBiWF9PN5JXKeB8TRmn3hUr`, ORAO Client `7b1ebeG8qgEtCB3ufPHcZLiUeFJ6cYMS9BeiTnSBJ6HT`, ORAO ProgramData `7mzfTTbQ4h7S9uNFdzbVUCs7srF6hiLa52y4eotkp1MD`, Squads ProgramConfig `BSTq9w3kZwNwpBXJEvTZz2G9ZTNyKBvoSeXMvwb4cNZr`, Squads ProgramData `Fy3YMJCvwbAXUgUM5b91ucUVA3jYzwWLHL3MwBqKsh8n`; domain constants 32 bytes each.
- **[L9]** `PublicKey.isOnCurve` for `7tj9...` -> true; for the Pool PDA and the local G vault -> false.
- **[L10]** local byte comparison of the devnet ORAO slice [R5] against `solana/probes/runtime/fixtures/orao-callback-mainnet-finalized.so` (sha256 `6d180b1e...c6b8b`, matches `orao-provenance.json`): both 616,464 bytes, 304,671 differing positions, first at offset 24, trailing zeros 15 (mainnet) vs 4,207 (devnet).
- **[L11]** `find /private/tmp /Users/robertripley/.codex/worktrees /Users/robertripley/coding/musicalchairs -maxdepth 7 -name "*musical*chairs*-keypair*.json"` -> no match; `find solana -name "*keypair*"` found only Rust build fingerprints. File names only; nothing opened.
- **[L12]** `python3 -I decode.py` (scratchpad) decoded the base64 account slices in [R2, R3] with the layouts of the vendored ORAO SDK `NetworkState` / `NetworkConfiguration` and the Squads `ProgramConfig` prefix.
- **[L13]** `ls node_modules/@sqds node_modules/@orao-network` -> both absent.

- **[L14]** `ls -la ~/.cache/mc-solana-artifacts/8f97e83db9dbc3112b1c18b5a35dfd5cfe6ae86bcb1a0cc93da78f7bb18dc826/` -> `musical_chairs_program.so` (1397160 bytes, mtime Oct 14 10:49), `SHA256SUMS` (92 bytes), `PROVENANCE.txt` (282 bytes). `cat SHA256SUMS` -> `8f97e83db9dbc3112b1c18b5a35dfd5cfe6ae86bcb1a0cc93da78f7bb18dc826  musical_chairs_program.so`. `cat PROVENANCE.txt` -> `source_set 994a9ff48fa2603318d4b28198f9f43d61418fa9566cf128662c32b68ac0d08f; built by cargo-build-sbf 3.1.0-beta.0 / platform-tools v1.48 at /Users/robertripley/.codex/worktrees/native-solana-model-probes/musicalchairs; copied 2026-10-24T04:21:12Z from the gitignored artifacts dir`. `shasum -a 256 -c SHA256SUMS` -> `musical_chairs_program.so: OK`; `shasum -a 256` of the file -> the same sha256.
- **[L15]** `grep -rIl "7tj9biW3KRJ7EEWmVUGigHiouCTXhV2dzcyvwma7Cyu7" . --exclude-dir=node_modules --exclude-dir=graphify-out --exclude-dir=target --exclude-dir=.git | wc -l` -> `68` files (this packet, 11 evidence receipts, `docs/solana/local-cluster.md`, `docs/solana/threat-model.md`, 3 frontend tests, the frontend IDL, 4 bridge replica receipts, 4 bridge test/driver files, `native_sol_keeper/AuthorityTable.mo` and 3 keeper test files, `solana/fixtures/{ix-encodings,sampling,seed-vectors}.json`, 29 RPC fixtures plus `manifest.json`, the program IDL, `lib.rs`, `sampling.rs`, `local_cluster.rs`, the loader README). Non-test code that reads the id from the IDL rather than naming it: `frontend/src/solana/native/accounts.ts:49` (`new PublicKey(idl.address)`), `cluster.ts:185,248`. `native_sol_keeper/AuthorityTable.mo:12` embeds `PROGRAM_ID` and `IDL_SHA256`.
- **[L16]** read of `native_sol_keeper/main.mo` (Config type lines 72-105, `defaultConfig` lines 219-241, `arm`/`disarm` at 331-339, hourly gate at 923, fee validation at 272) and `native_sol_keeper/Policy.mo:19-20` -> `ALERT_AFTER_NS = 60_000_000_000`, `SIGN_AFTER_NS = 300_000_000_000`; defaults `activePollNs = 30_000_000_000`, `idlePollNs = 300_000_000_000`, `maxFeeLamportsPerTx = 10_000`, `maxSignaturesPerHour = 60`, `minFeeBalanceLamports = 10_000_000`, `rpcCycles = 10_000_000_000`, `signCycles = 30_000_000_000`; validation rejects `maxFeeLamportsPerTx` below the 5000-lamport base fee. All `*Ns` fields are nanoseconds.
- **[L17]** schedule arithmetic (local): lone Simple depositor insolvent when `0.11 x t > 0.96` days, so t > 0.96 / 0.11 = 8.7273 days = 754,036.4 s, first whole second 754,037 s = 8 d 17 h 27 m 17 s. Seq-A and Seq-B totals: 2.0 + 30.0 + 8.7273 = 40.727 days = 40.73 days, plus the 600 s expiry wait and minutes of processing = about 40.74 days. G wait 172,800 s = 2.0 days; Compound15 1,296,000 s = 15.0 days; Compound30 2,592,000 s = 30.0 days.

Files read for this packet (revision 2 added `native_sol_keeper/main.mo` and `Policy.mo` config and gate code, `positions.rs`, `close.rs`, `state.rs`, the kanban rows for the two preflight cards): the plan (M14 and surrounding), `latest-build.json`, `.github/workflows/native-solana-model.yml`, `local-cluster.md`, `production-accounts.md`, `measurements-2026-10.md`, `launch-decisions.md`, `operations.md`, `threat-model.md`, the ICP budget, defaults, M12, M13, corrections and second-closure documents, the legacy memo evidence, `dfx.json`, `canister_ids.json`, `frontend/public/.ic-assets.json`, `frontend/src/solana/rpc.ts`, `cluster.ts`, and the program, bridge, keeper and Shenanigans sources cited inline.
