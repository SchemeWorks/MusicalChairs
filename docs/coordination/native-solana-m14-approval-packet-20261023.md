# Native Solana M14 approval packet (devnet acceptance gate)

Date 2026-10-23. Branch `codex/native-solana-production-core`, head `2f031cc` (plan Milestone 14, lines 705-717 of [the implementation plan](../superpowers/plans/2026-10-13-native-solana-implementation.md)). Prepared by a Sonnet worker for the coordinator.

**This packet authorizes and executes nothing.** No key was read or created, no transaction, airdrop, deploy or `dfx` write was made. The only network contact was Solana devnet JSON-RPC reads and anonymous `dfx` info/query calls, all listed in the [appendix](#10-appendix-every-read-only-query-and-local-check). Evidence labels used below:

- **[R]** observed read-only by this packet (appendix id in brackets, e.g. [R3]).
- **[S]** read from source at the cited file and line.
- **[M]** measured earlier by the project (local LiteSVM, not a cluster quote).
- **[C]** reported by the coordinator, not re-verified here.
- **[E]** estimate or inference, labeled as such.

## 1. Smallest combined approval request

Recommended: answer **A, B1, B2 as "yes, go when the coordinator reports the A0 readiness gaps of section 5 closed"**, and hold **B3, B4, C1, C2, C3** until the evidence named in section 6 exists. Each row is one independent yes/no.

| id | yes / no | exactly what (and what it is not) | hard cap | depends on |
| --- | --- | --- | --- | --- |
| **A** | ____ | ONE Solana-devnet block A1-A9: fresh devnet keys; public-faucet devnet SOL; deploy the hash-pinned ELF `8f97e83d...` to `7tj9...`; move upgrade authority to G; create two devnet Squads v4 multisigs (G 2-of-3, 172,800 s; P 2-of-3, 0 s) with test voters; initialize via G; register and fund the ORAO devnet client via G; fund test wallets; run the plan flow list, deployed-byte proof and conservation reconciles. No ICP, no mainnet, no real funds | 25 devnet SOL total; deployer bucket 15 SOL; any other single transfer 2 SOL | A0 gaps (section 5); MI-1..MI-8, MI-13..MI-15 |
| B1 | ____ | Create and install `native_sol_bridge` on the IC (new canister), devnet config, no relayer yet | about 3-5 T cycles [E] | A6 green |
| B2 | ____ | Create and install `native_sol_keeper` on the IC, devnet config, **not armed** | about 5-10 T cycles [E] | A6 green; MI-12 |
| B3 | ____ | Upgrade the LIVE Shenanigans `j56tm-...` to the native branch (R6 methods behind an allowlist), stop / snapshot / upgrade / start / state diff. Highest risk | cycles margin only | A green; B1; MI-9, MI-10 |
| B4 | ____ | Build the frontend with devnet RPC and CSP and deploy it to a NEW asset canister (never the production frontend `5qu42-...`) | about 2-3 T cycles [E] | B1; MI-11 |
| C1 | ____ | Activate the bridge path: `setRelayers`, `setNativeBridgeAllowlist`, one native policy version | none | B1, B3 |
| C2 | ____ | Arm the keeper watchdog for a bounded window | per-hour signature cap, see 4d | B2, A green |
| C3 | ____ | PP path acceptance run (C4): one scripted SIWS login of a devnet test wallet against the production `siws_provider`, then real PP mints on the production ledger for one `PositionOpened` and one `PromotionApplied` | at most 5,000 PP minted [E] | B1, B3, C1 |

Two answers are needed with the approvals, because they decide whether A can start at all: **MI-1** who holds the keypair for program id `7tj9...` (none was found on this machine; otherwise the id must change and every pin below is re-derived), and **MI-2** whether Rob accepts a **maturity-bound run of about six weeks** (section 6.4) or amends the flow list.

## 2. Candidate pins

Everything in this table that is a hash was either re-computed read-only in this packet (local check id in brackets) or taken from the named receipt.

| item | value | how known |
| --- | --- | --- |
| Branch / head | `codex/native-solana-production-core` / `2f031cc` | [L1]; draft review PR #125 (kanban row PR-125) [C] |
| `source_set_sha256` | `994a9ff48fa2603318d4b28198f9f43d61418fa9566cf128662c32b68ac0d08f` (19 files, helper `solana/tests/financial-runtime/scripts/source-set-digest.sh`) | pinned in [latest-build.json](../solana/evidence/latest-build.json); working tree recomputed equal [L4] |
| `lock_sha256` | `77f859f9a7ad4e2bfdd836b66b9353bf79b49041e69760d3adc9b93245a2601f` (`solana/programs/musical_chairs/Cargo.lock`) | pinned; matches the file hash in the [L4] listing |
| **ELF to deploy** (reviewed macOS artifact) | sha256 `8f97e83db9dbc3112b1c18b5a35dfd5cfe6ae86bcb1a0cc93da78f7bb18dc826`, 1,397,160 bytes (`elf_len`), file mtime 2026-10-14 10:49 | on disk at `solana/programs/musical_chairs/artifacts/musical_chairs_program.so` (gitignored, single copy), re-hashed now [L2] |
| ELF build conditions | macOS arm64, platform-tools v1.48 (rustc 1.84.1-dev), cargo-build-sbf 3.1.0-beta.0, built at path `/Users/robertripley/.codex/worktrees/native-solana-model-probes/musicalchairs` | [latest-build.json](../solana/evidence/latest-build.json) |
| Reproducibility caveat | Byte-identical only for a clean rebuild from the SAME path on the SAME platform tools (M0 receipt); another path or host gives different code bytes with the identical source set (M7 receipts). The cause is unverified. A byte search of the ELF finds only 12 `/Users/runner/work...` strings (the platform-tools std sources) and no worktree or cargo-home path [L7], so the dependence is not through embedded path strings | [C] for the receipts; [R] for the string check |
| CI Linux artifact (cross-check only) | sha256 `a398d6f4b5fba0fe78c76cc5bcbe79bbbbc725bfd25451eeaab14bb9a161f297` from run 38614117412 on `f132fd4` (ubuntu-latest, pinned Linux tools); source set and lock asserted equal to the pins; **bytes differ from the macOS ELF by host and path, as the pin file says**; 210 harness tests and clippy passed on it | [2026-10-14-m10-fixes.json](../solana/evidence/2026-10-14-m10-fixes.json) `linux_ci`. Latest exact-head CI named by Codex: run 38705116196 at `c7ccc6e`, four jobs green [second closure review](native-solana-r1-r5-second-closure-review-20261017.md). CI at `2f031cc` was not read here |
| Which one is deployed | Recommended: the macOS `8f97e83d...` (the artifact every receipt and local run used). The Linux ELF is a second valid build of the same source set but is not what was reviewed locally | recommendation [E]; MI-8 |
| Committed IDL | sha256 `11840ae04eaebff5d75a1952e87577c280a79f5b4e42550319201cdf3e9bb597`, byte-identical copies at `solana/programs/musical_chairs/idl/musical_chairs.json` and `frontend/src/solana/native/idl/musical_chairs.json` | [L3]; CI step "Committed IDL matches a fresh generation" (`solana/scripts/check-idl.sh`) reported the same hash on run 38614117412. No on-chain IDL account is planned, so no IDL account hash |
| Vault stand-in (localnet ONLY, never deployed to devnet) | `eb78e77fc107c45d38d7e45bb365d632a8418909a3b0c79dbca26f7c20f109c8` (macOS); CI Linux `955441aa488074c17af35e1cf191a564977f5a810da91a3622927ee8de0ee7ca` | [latest-build.json](../solana/evidence/latest-build.json), [m10-fixes](../solana/evidence/2026-10-14-m10-fixes.json). Devnet uses the real Squads v4 program |
| Program id | `7tj9biW3KRJ7EEWmVUGigHiouCTXhV2dzcyvwma7Cyu7` (`declare_id!`, [S] `lib.rs:27`); a valid ed25519 public key [L9]; **no keypair for it was found** [L11] and the build wrapper deletes the random keypair it generates unread [S] `build-sbf.sh:47-59` | MI-1 |
| Devnet cluster domain (32 bytes) | `musical-chairs-devnet-domain-v1!` = hex `6d75736963616c2d6368616972732d6465766e65742d646f6d61696e2d763121`; localnet is `musical-chairs-localnet-dev!!!!!` (do not reuse) | [S] `frontend/src/solana/native/cluster.ts:35-38`, `sampling.rs:106`; [L8] |
| Devnet genesis hash pin | `EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG` | [S] `cluster.ts:46`; to be checked live at A1 (`getGenesisHash` was not in this packet's allowed method list) |
| Toolchain archives | platform-tools-osx-aarch64 v1.48 `799e4cfcee38e227155c83fb2294f96f94a843d42e01c0359fb7128e2216d2dd` (422,197,780 B); agave v3.1.0-beta.0 macOS aarch64 `de9be527da14757854c47defffffeb0d17d1b7b6c0a4175f46f531aca262bad9` (77,248,568 B); Linux archives pinned for CI | [latest-build.json](../solana/evidence/latest-build.json), [m0-toolchain.json](../solana/evidence/2026-10-14-m0-toolchain.json) |

**Solana note (ICP comparison).** On ICP a canister's module hash is one number the replica reports (`dfx canister info`). On Solana the deployed bytes live in a `ProgramData` account behind a 45-byte loader-v3 header, and the deploy allocates `max_data_len`, so "deployed equals reviewed" is a three-part statement: header decodes, the `elf_len` bytes equal the ELF, the rest is zero padding. That is exactly the check in section 6 (A8).

## 3. Intended devnet targets and roles

### 3a. Program and PDAs (derived locally; no key needed)

Derived by [L8] with `@solana/web3.js` from the repository's `node_modules`; four of these match [docs/solana/local-cluster.md](../solana/local-cluster.md) (Pool, ProgramData, ORAO client, RngAuthority), which validates the derivation. Devnet check [R6]: none exists today.

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
| multisig address | `PDA(["multisig","multisig",create_key_G], SQDS4...)` = **MISSING USER INPUT until create_key exists** (MI-4) | same with `create_key_P` | [S] `controls.rs:1406-1410` |
| vault (the signer the program sees) | `PDA(["multisig", G_multisig, "vault", [0]], SQDS4...)` | same for P | [S] `runtime.rs:1910-1915`; vault index 0 [S] `controls.rs:39` |
| `config_authority` | zero pubkey (autonomous) | zero | exact, [S] `controls.rs:1397-1401` |
| threshold / members | 2 of exactly 3 distinct non-zero members; each needs the Vote bit and no unknown bits; at least one member with Initiate and one with Execute | same | [S] `controls.rs:1402-1432` |
| `time_lock` | **172,800 s exactly** | **0 exactly** | [S] `controls.rs:32,1403`; `initialize` passes 0 for P at `controls.rs:338-340` |
| proposed test voters | 3 fresh devnet keys per multisig (MI-5); same keys for G and P is allowed but loses the independence the mainnet launch needs | same | decision |

Do **not** reuse the local-cluster fixture multisigs (`5ASstTEw...`, `HfbGhr2Y...`): their create keys are constants, not keys anyone can sign with, so they cannot be created on a real cluster.

**Solana note (ICP/EVM comparison).** A Squads vault is a PDA, so it can only "sign" when the Squads program executes an approved vault transaction. The closest ICP analogue is an SNS or multi-controller proposal that only runs after a delay; the closest EVM analogue is a Safe with a timelock module. The `time_lock` is therefore a wait of 48 hours between the second approval and execution. Both `initialize` and the ORAO registration are G actions, so **the real run cannot start before a 48 hour wait** (6.4).

### 3c. ORAO VRF on devnet (observed read-only)

| item | value | source |
| --- | --- | --- |
| ORAO Callback program | `VRFCBePmGTpZ234BhbzNNzmyg39Rgdd6VgdfhHwKypU` is deployed on devnet (executable, loader-v3), ProgramData `7mzfTTbQ4h7S9uNFdzbVUCs7srF6hiLa52y4eotkp1MD` (616,509 B), deployment slot 403,064,376, authority `9ZTHWWZDpB36UFe1vszf2KEpt83vwi27jDqtHQ7NSXyR` | [R2, R3] |
| NetworkState | `3e483QYyYQqu8eesT6ZjfuBiWF9PN5JXKeB8TRmn3hUr`, owner is the ORAO program, 245 B; `request_fee = 300,000`, `callback_deadline = 9,000` slots, treasury and authority both `9ZTHWWZD...`, **four** fulfil authorities (`2VYNr8yM...`, `A9iZdgYa...`, `9ZTHWWZD...`, `HSCqQaDH...`), lifetime `num_requests = 44,183`, `num_registered = 509`, `num_terminated = 0` | [R3] decoded locally by [L12] |
| Registered Client for us | `7b1ebeG8qgEtCB3ufPHcZLiUeFJ6cYMS9BeiTnSBJ6HT` (derived), absent today [R6]; after A5 it must read `Client.owner == G vault`, `Client.state == RngAuthority`, account owner == ORAO program | [S] spec / plan M14 readback line |
| Registration payer | the **G vault** (the ORAO `Register` payer must be the program upgrade authority, per the Codex review packet's R5 row) [S] `local_cluster.rs:236-241`; the vault needs a small SOL balance first (MI-6) | local genesis flow |
| Provider tuple for `initialize` | program `VRFCBeP...`, network state `3e483QYy...`, treasury `9ZTHWWZD...`, client `7b1ebeG8...`, `provider_request_fee_lamports = 300,000` (equals the live devnet fee), `provider_fee_headroom_bps = 2,500` (the local-genesis value; M9 reserve arithmetic used 2,000, so confirm) | [S] `local_cluster.rs:194-206`; [M] measurements section 4 |

**Finding RG-5 (material).** The devnet ORAO bytes are **not** the mainnet ELF the project tested against. Same length (616,464 bytes after the header) but sha256 `b7f30b89cda7d3f067fbb36f2e817a5e50c8f510f0f8f65f5b87d5a65526ccea` versus the captured mainnet fixture `6d180b1e91be7d5c541f75e2413146beb047d9beeb76ac8eb1d7c883200c6b8b`; 304,671 byte positions differ, the first at offset 24 [R5, L10]. Every local ORAO result (LiteSVM, local cluster, the 759 B / 147 B request sizes, the `Register` account order) was produced on the mainnet build. Compatibility of the devnet build is unproven until it is exercised locally with these exact bytes and then on devnet.

### 3d. Wallets and keys (every entry is a MISSING USER INPUT; no address is invented)

| role | purpose | policy proposed |
| --- | --- | --- |
| Deployer D | fee payer and temporary upgrade authority until A3 completes | fresh devnet keypair, generated by Rob, stored outside any git worktree, never `~/.config/solana/id.json` (MI-3) |
| Squads create keys (2) | sign multisig creation; secret is not needed afterwards | fresh keypairs, discard after creation |
| G voters (3), P voters (3) | test voters | MI-5 |
| Sponsor S | `initialize` signer and sponsor recipient; pays shared storage; funds operations | fresh devnet keypair (MI-6) |
| Management recipient | receives accrued Front-End Load | a different fresh keypair, so `withdraw_management` can be observed (MI-6) |
| Keeper fee payer K | worker CLI `mc-keeper` fee payer; holds fees only | fresh keypair; the ICP watchdog address is separate and is only known after B2 |
| Players P1-P5, backers A1-A2, plain wallet W | test wallets for the flows; W submits the composed finalize/reopen transaction | fresh keypairs (MI-7) |
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
| **Program deploy (D)** | upload buffer R(1,397,197) = 7,098,411,000; ProgramData R(1,397,205) = 7,098,451,640; Program R(36) = 833,120. **Peak = 14,197,695,760 lamports (14.1977 SOL)** while both exist; **net retained = 7,099,284,760 (7.0993 SOL)** after the buffer's lamports are returned. Write transactions at about 1,000 B each: roughly 1,400-1,700 x 5,000 = 0.007-0.009 SOL [E]. `max_len` = `elf_len`, so zero allocation padding (any headroom doubles ProgramData rent; MI-13) | 15.0 SOL |
| Squads | two 231-byte multisigs R(231) = 1,823,720 each = 3,647,440 (creation fee 0 [R3]); proposals and vault-transaction accounts about 0.1 SOL for about 6 proposals [E]; six voter wallets x 0.05 SOL fees | 0.5 SOL |
| Sponsor S | initialize accounts: Pool 2,550,160 + Admission 904,240 + GameVault 695,960 + Ops 1,630,680 + Ticket 1,874,520 + RngAuthority 858,520 + ConfigVersion 2,057,400 + OpsCash R(0) 650,240 = **11,221,720**; first pages 2,326,640 + 2,280,920 + 3,337,560 = **7,945,120**; extra pages about 0.02 SOL; **operations funding 0.5 SOL** (covers two closes below); ORAO client funding 0.05 SOL (local genesis used 50,000,000); fees | 1.0 SOL |
| G vault | ORAO Client R(123) = 1,275,080 paid at registration | 0.02 SOL |
| Keeper fee payer K | at most 1,000 transactions x 5,000 | 0.1 SOL |
| Test wallets | deposits at most **6.0 SOL** gross in total; Series A at most 0.5 SOL; 8 wallets x 0.05 SOL for rent and fees (User 1,076,960 + UserRound 1,102,360 + Position 1,457,960 each, so about 5.1M per wallet with two positions) | 7.0 SOL |
| **Sum of buckets** | 15.0 + 0.5 + 1.0 + 0.02 + 0.1 + 7.0 | 23.62 SOL |
| **Total hard cap** | the 1.38 SOL slack covers retries only; it never lifts a per-action cap | **25 SOL** |

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

Faucet feasibility is a real risk, not a detail: the deployer needs about 14.2 SOL at once and public devnet airdrops are small and rate-limited (limits not measured here). If the public faucet cannot supply the deployer's cap in a reasonable time, A stops. Another devnet faucet or a funded devnet account is a Rob decision (MI-14), never mainnet funds.

### 4d. ICP cycles (all [E] unless a query is cited)

Price facts read live [R11]: SOL RPC canister read costs on devnet defaults: `getAccountInfo` 1,755,488,000; `getBlock` 1,754,712,800; `getSlot` 1,715,952,800; `getBalance` 1,733,619,200; `getSignatureStatuses` 1,745,655,200; `sendTransaction` 1,721,542,400; `getTransaction` 2,384,420,000 (default estimate), 12,244,311,200 at the 131,072 B estimate across the default providers, and **4,081,373,600** for a single provider at that estimate. Threshold Ed25519 signature: **26,153,846,153 cycles** at the production key `key_1` ([ICP budget](native-solana-icp-budget.md); not re-queried; the price of other key names is unverified).

| canister | cycles estimate | reasoning |
| --- | --- | --- |
| B1 bridge | creation (assumed 0.5 T per canister from the public price table, not queried) + 3 T working, cap 5 T | per verified event two single-provider reads = about 8.2 B; the code attaches 25 B per call (refunded), so keep 50 B free; the C3 run is two events, about 16 B |
| B2 keeper | creation 0.5 T + 5 T working, cap 10 T | idle poll is one `getAccountInfo` of Pool = 1.755 B; at the 300 s idle interval that is 288 polls a day = **0.5055 T per day while armed**; a watchdog-sent step costs about 35-37 B (getSlot + getBlock + getBalance + signature 26.15 B + send + status read, plus that tick's Pool read); the config default of 60 signatures an hour would allow up to 1.57 T an hour, so propose 10 an hour for devnet (MI-12) |
| B3 Shenanigans upgrade | no creation; execution under 0.05 T [E] | precondition: balance comfortably above the freezing threshold; **the balance is readable only by a controller** (MI-9) |
| B4 frontend | new asset canister: creation 0.5 T + 2 T working | upload of a few MB is negligible |
| **Total** | baseline about 11.5 T, cap 20 T | about 16 and 27 USD at the budget document's illustrative 1 T = 1 XDR = 1.366 USD; reprice at funding time |

Arming policy matters: 42 days armed at the default 300 s idle poll is about 21 T cycles; with `idlePollNs` of 3,600 s it is about 1.8 T. C2 should therefore be a bounded window.

### 4e. What stays locked afterward

The 7.1 SOL ProgramData rent and the CloseRecord/page rents are recoverable only by closing the program or accounts under G (a 48 hour action) or by cleanup; devnet SOL has no value, so this is a faucet-throughput question, not a cost.

## 5. Readiness gaps (actual)

RG-1..RG-8, RG-12 and RG-13 are the **A0 gaps** (they gate Block A); RG-9..RG-11 gate B3 and B4. Nothing here needs Rob's approval to close; each is local work or a read, except where the last column says so.

| id | gap | evidence | what closes it | approval needed |
| --- | --- | --- | --- | --- |
| RG-1 | **Toolchain gone.** The pinned directory `/private/tmp/mc-solana-runtime-tools-20261013` no longer exists; free disk is 20 GiB of 926 (98 percent used) | [L5, L6] | Re-download the two official archives to a durable path, verify against the pinned sha256, free disk first (LiteSVM targets, platform-tools about 1.3 GB extracted) | none (local-stage authority covers dependency fetches; the coordinator owns builds) |
| RG-2 | **ELF not re-derived since the toolchain loss.** The ELF is present and its hash equals the pin [L2], and the working-tree source set equals the pin [L4], but source-to-bytes has not been re-proven | [L2, L4] | Rebuild at the same absolute worktree path with the pinned tools; require sha256 `8f97e83d...`. If different: stop, do not deploy; re-pin through Codex. Copy the verified ELF and its hash to a durable path now: it is a gitignored single copy in a worktree that tooling may clean | none |
| RG-3 | **Program-id keypair.** No keypair for `7tj9...` found by filename search [L11]; the build wrapper discards its random one; the decision register lists "Program ID (deploy keypair)" as an operator input at deploy | [S] `native-solana-decision-register-20261013.md:219` | Rob confirms who holds it (MI-1), or Codex approves changing `declare_id!`, which changes `source_set_sha256`, the ELF, the IDL and every pin | Rob / Codex |
| RG-4 | **Devnet tooling does not exist.** `local_cluster` targets a local validator with a Squads stand-in. Needed: loader-v3 deploy + set-authority + deployed-byte checker; real Squads v4 multisig create, vault transaction, proposal, approve, execute and config-transaction flows; ORAO register via a G proposal; flow runner; conservation reconciler; latency recorder. No Squads SDK is in `node_modules` [L13] | [C]; [L13] | Write it and rehearse it **locally against the real binaries**: load the real Squads v4 ELF and the devnet ORAO ELF (both fetched read-only in this packet, hashes below) into LiteSVM with a warped clock (the test validator cannot warp 48 hours) | none |
| RG-5 | **ORAO devnet build differs from the tested mainnet build** (3c). Also unknown: whether the devnet oracle fulfils our requests, and how fast | [R5, L10] | Rehearse locally with the devnet ELF `b7f30b89...`; the first real answer arrives at A7 | none to rehearse |
| RG-6 | **Real Squads and the two-signer `initialize`.** `initialize` needs the sponsor keypair AND the G vault to sign [S] `controls.rs:296-298,1546-1548,1577`. The local stand-in forwards one inner instruction with the vault seeds while the sponsor signs the outer transaction. Whether the real Squads executor lets a non-vault signature through to the inner instruction is **not verified here** | [S] M3 handoff, "stand-in signer" | Rehearse against the real Squads ELF first. If it cannot, `initialize` needs a Codex decision (for example sponsor = G vault) before any devnet spend | Codex decision if it fails |
| RG-7 | **Flow scenario with solvency arithmetic.** Position payouts require the position's round to be the current one [S] `positions.rs:843` and compounding terms are 15 and 30 days [S] `state.rs:384`, so the amounts and order of the flow list must be designed and simulated (clock-warped LiteSVM) before real days are spent | 6.4 | Write the scenario, run it in LiteSVM with warps, record expected balances per step | none |
| RG-8 | **Time-lock reduction test is destructive to G.** `require_governance` demands `time_lock == 172,800` exactly [S] `controls.rs:626-631,1403`, so after G shortens its own delay the program stops accepting G's governance instructions until the delay is restored | [S] | Run it as the LAST G action, or restore the delay afterwards | none |
| RG-9 | **ICP: bridge and keeper never deployed; Shenanigans path.** Native branch `shenanigans/` is main plus R6 and the R1 reservation [C]. Live bytes are unverified against any commit (a local build of main hashes `13bddcc4...`, the live module is `7fa41360...`) [C]. The stable-compatibility check against the live signature needs a controller (anonymous gets 403) [R12] | [R7, R12] | Re-run `moc --stable-compatible` live to the exact B3 build with the controller identity, record the build's wasm hash, capture a state baseline, rehearse the snapshot restore on the local replica | Rob (use of the controller identity for the read, MI-9) |
| RG-10 | **Frontend.** Current CSP `connect-src` lists mainnet Solana hosts only [S] `frontend/public/.ic-assets.json`; one constant `SOLANA_RPC_ENDPOINT` also drives the LEGACY SOL balance and deposit paths [S] `frontend/src/solana/rpc.ts:18-19`, so a devnet build on the production asset canister would repoint legacy flows to devnet. The SIWS provider is configured for domain and URI `musicalchairs.fun` [S] `dfx.json`, so an interactive wallet sign-in from another origin may be refused (not tested) | [S] | Deploy to a NEW asset canister (B4); C3's SIWS binding is scripted with a test keypair, as the M12 local run did; add `https://api.devnet.solana.com` (or the chosen provider) to that build's CSP and set `VITE_NATIVE_BRIDGE_CANISTER_ID` after B1 | B4 |
| RG-11 | **Sequencing with the legacy memo card.** LEG-1 (separate branch off `main`) changes the same Shenanigans file region; if it ships first the live signature and module hash change and B3 must be re-based; if B3 ships first LEG-1 must rebase onto the native branch. Not an M14 gate: the native PP legs use their own frozen 32-byte hashed memo [S] `NativeRewards.mo:68,116,521-529,848`, and the bridge never calls the legacy mint path | [legacy memo evidence](legacy-pp-ledger-memo-evidence-20261023.md) | State the order explicitly at B3 | Rob (MI-10) |
| RG-12 | **Calibrations still placeholders.** Bridge `RPC_CYCLES` 25 B (a single-provider read costs about 4.1 B), keeper `rpcCycles` 10 B and `signCycles` 30 B, watchdog `Equality` consensus on `#Default(#Devnet)` without devnet evidence | [S] `native_sol_bridge/main.mo`, `native_sol_keeper/main.mo:237-238`; [R11] | Set from the section 4d numbers at B1/B2; record the devnet consensus behavior | none |
| RG-13 | **Local-run artifacts.** The M0-M13 local dfx replica state is gone (reboot) [C]; replica regressions are re-runnable but not re-run | [C] | Re-run the M12 replica driver once the toolchain is back, to confirm the B1-B3 wasm builds still pass before they touch the IC | none |

## 6. Execution order with gates

Nothing starts before its predecessor's gate evidence is recorded in a receipt `docs/solana/evidence/2026-1N-NN-devnet-*.json` and the coordinator reports it.

### 6.1 Block A: Solana devnet (one approval)

| step | action | gate to proceed | spend |
| --- | --- | --- | --- |
| **A0** (no approval) | Close RG-1..RG-8 and RG-12/13 locally: toolchain, ELF re-derivation, durable ELF copy, tooling, real-Squads and devnet-ORAO rehearsal in LiteSVM, flow scenario, B1-B4 wasm builds with recorded hashes | every RG closed; ELF hash `8f97e83d...`; source set `994a9ff4...`; IDL `11840ae0...` | none |
| A1 keys and funding | Rob generates the keys (3d); check `getGenesisHash` equals the devnet pin; faucet airdrops to D first (up to 15 SOL), then other buckets | genesis hash matches; balances within bucket caps | faucet only |
| A2 Squads | create G (2-of-3, 172,800 s, `config_authority` zero) and P (2-of-3, 0 s) with the test voters; read both back through RPC and through the program's own validation rules | profile bytes equal the table in 3b; vault PDAs derived and recorded | about 0.004 SOL rent + fees |
| A3 deploy and authority transfer | write the buffer, deploy with `max_len = elf_len`, **run the deployed-byte check (A8, first pass) BEFORE the transfer**, then `set-upgrade-authority` to the G vault (the vault is a PDA and cannot co-sign, so the new-authority signer check must be skipped [E]) and read `ProgramData` back | bytes equal the ELF; `upgrade_authority == G vault`; if the transfer fails D keeps authority and the run stops | 14.2 SOL peak, 7.1 net |
| A4 initialize via G | vault transaction + proposal for `initialize` (args: sponsor = S, management recipient, devnet cluster domain, the provider tuple in 3c, `public_series_a_enabled = true`); two approvals; **wait 172,800 s**; execute | `Pool`, `Admission`, `Ops`, `ConfigVersion#1` readback; `Pool.cluster_domain` = devnet bytes | about 0.011 SOL |
| A5 ORAO register and funding via G | after `initialize`, vault transaction + proposal for ORAO `Register` (payer G vault, RngAuthority PDA seeds); same 48 h wait (may be created and approved together with A4 and executed after it; confirm in the rehearsal); then fund the Client, `fund_operations`, provision first pages | ORAO Client readback (3c) | about 0.6 SOL |
| A6 readbacks | G and P profiles, `time_lock`, vault derivation; `Admission.guardian_vault == Pool.guardian_vault == P vault`; `Client.owner == G vault`, `Client.state == RngAuthority`, account owner == ORAO program; `ProgramData` authority; provider tuple in `ConfigVersion#1` | all equal expected | none |
| A7 flows | the list in 6.5, each followed by A9 | each step's receipt | within buckets |
| A8 deployed-byte evidence | exact C5 procedure of the plan (first pass in A3, repeat after A7; no upgrade happens in M14) | see section 8 | read-only |
| A9 conservation | after every action recompute both identities from RPC: game vault `vault - rent == pot + seed + management + pending_toll + funded_claims + excess` and the OpsCash partition identity [S] `operations.md` step 4 and `threat-model.md` section 1 | equal, else stop | read-only |

### 6.2 Block B: ICP, each separately approved (recommended only after A6 is green)

| step | action | gate | notes |
| --- | --- | --- | --- |
| **B1** | create `native_sol_bridge` with Rob's controller set (MI-9), install the built wasm, `setConfig` with program id, devnet cluster domain, event authority `BuzaujH9...`, two DIFFERENT explicit provider lists (MI-11), Shenanigans principal; leave `setRelayers` empty | module hash from `dfx canister info` equals the built wasm sha256; `getConfig` shows the expected values and no RPC sources | inert: no relayer can submit |
| **B2** | create `native_sol_keeper`, install, `setConfig` (devnet cluster domain, devnet sources, `feeKeyName`, caps), `getFeeAddress`, fund that address (K bucket), **do not arm** | module hash equals the built wasm; fee address derived once and stable | key choice MI-12; first proof that threshold Ed25519 works on the IC |
| **B3** | the Shenanigans upgrade: baseline queries -> `stop` -> `dfx canister snapshot create` -> upgrade (additive stable vars only) -> `start` -> state diff vs baseline -> restore the snapshot on any mismatch; allowlist stays empty | stable-compat check passed; wasm hash recorded; baseline equal after upgrade | highest risk: this is the production PP economy canister and R1 touches three lines of the legacy gift path [C] |
| **B4** | build with devnet RPC, `VITE_NATIVE_BRIDGE_CANISTER_ID` = B1, devnet CSP; deploy to a NEW asset canister | served asset hash equals the build output hash; version marker matches | never the production canister |

### 6.3 Block C: activation, each separately approved

| step | action | gate |
| --- | --- | --- |
| **C1** | bridge `setRelayers` (MI-9), Shenanigans `setNativeBridgeAllowlist [bridge]`, `appendNativePolicyFromMintConfig` or a lower devnet draft (MI-10) | `getRelayers`, `getNativeBridgeAllowlist`, `getNativePolicyTable` equal the intended values |
| **C2** | arm the watchdog for a bounded window with the lowered `maxSignaturesPerHour` and a longer `idlePollNs` | `getStatus`: `armed`, `feeAddress`, `lastTimerAt` advancing; fee address funded |
| **C3** | PP path acceptance: for one genuine finalized `PositionOpened` and one `PromotionApplied`: event -> two RPC observations -> provenance verification -> `observeNativeEvent` (unbound) -> scripted SIWS login of a test keypair -> `bindAndReserve` -> `executeReservedLeg` / `executeNativeRewardLeg` -> ledger receipt and `reconcileLegFromLedger` -> assert no legacy duplicate reward for the principal | legs Committed with ledger block indices; PP minted within the 5,000 PP cap. **This mints real PP on the production ledger from valueless devnet play.** The promotion basis is the winner's loss at 24,000 PP per SOL, so the cap holds only if every candidate's loss in that round is at most about 0.2 SOL; the draw picks the winner, so the scenario must bound all candidate losses (or Rob raises the cap). The live rates are 6,000 / 12,000 / 18,000 PP per SOL (Simple / Compound15 / Compound30) and 24,000 per SOL for Backers, signup gift 500 PP [R10]; a 0.01 SOL Simple event is 60 PP, a 0.01 SOL promotion loss 240 PP, plus the gift and cascade/house legs. Note the legacy mint path currently fails its gift mint on long memos, so a "no legacy duplicate" check is weak evidence on mainnet until LEG-1 ships; the exclusion logic itself is proven locally (G1-G6) |

### 6.4 Calendar: what the plan's flow list costs in wall-clock time

Facts from source, so the "yes" in row A is informed:

- G's delay is exactly 48 hours [S] and both `initialize` and ORAO registration are G actions: **earliest game start is about two days after approval**.
- Simple terms 21 days, Compound15 15 days, Compound30 30 days, from the genesis config [S] `state.rs:384`; the Clock is real devnet time (no warp). A position pins its duration at open [S] `positions.rs:672`.
- A position can only be paid in its own round [S] `positions.rs:843`. So **both compound settlements must happen before round 1 closes**, which puts the first close at the Compound30 maturity: day 2 + 30 = about day 32 (the insolvent Compound30 settlement can itself be the closing payout).
- Closing needs a strict `owed > pot`. For a lone Simple depositor that is about 0.96 / 0.11 = 8.7 days [E: pot is 96 percent of gross after the 4 percent load; Simple accrues 11 percent a day on gross]. Round 2 therefore closes at about day 41, and the expiry path needs more than 600 s with no capture after that.
- G's own time-lock reduction takes another 48 hours if config transactions honour the delay (unverified; the test establishes it) and must be last (RG-8).

**Result: about 41-42 days (six weeks) end to end for the literal list.** Fast items (hours to days): deployment, governance readbacks, deposits, Simple payout, partial payout, Series A funding, claims, the first close if only Simple positions are in round 1. Options for Rob (MI-2):

1. **Literal (recommended):** all items on production terms, about six weeks. Costs only time and the same budget; exposes the run to key custody over weeks and to devnet instability (stop condition: genesis hash change).
2. **Amend:** drop the compounding settlements from devnet and rely on the clock-warped LiteSVM evidence; or stage a shortened-duration config for round 2 and later (G staging, 48 h, applies only from the next round per the accepted defaults; not re-verified in source here), which proves mechanics but not production terms and must be labeled that way in the receipt.

### 6.5 Flow list (A7) with calendar anchors

| id | flow (from the plan) | earliest |
| --- | --- | --- |
| F1 | Series A funding by A1 (minimum 0.05 SOL) | day 2 |
| F2 | deposits: Simple (P1, P4), Compound15 (P2), Compound30 (P3) | day 2 |
| F3 | Simple payout (P1) -> toll -> allocation ticket phases by the worker CLI -> commit | day 3 |
| F4 | partial payout (P4) | day 3 |
| F5 | combined claim: A alone before the first promotion; A+B in one instruction after it | day 3 / after F9 |
| F6 | cap completion (a Series A reaches 124 percent) | at a large toll, likely F7 |
| F7 | both compound settlements: Compound15 at day 17, Compound30 at day 32 (solvent first, insolvent second) | day 17 / 32 |
| F8 | closing with candidates: build candidates, `request_round_draw`, `capture_round_draw` (record request slot, fulfil slot, seconds) | day 32 |
| F9 | `apply_promotion`, then finalize/reopen via a composed transaction from the plain wallet W | day 32 |
| F10 | expiry path on the second close: do not capture for more than 600 s, `expire_promotion`, finalize | about day 41 |
| F11 | G reduces its own time lock; document; restore if needed | last, or in parallel waiting time |
| F12 (optional, no plan line) | P pause and unpause rehearsal: shows exits keep working while paused | any time, one P proposal |

## 7. Stop and recovery conditions

"Stop" means: take no further action, write what is known into the receipt, tell the coordinator, and wait for a decision.

| step | stop condition | recovery |
| --- | --- | --- |
| A0 | source-set, lock, IDL or ELF hash differs from the pins; rebuild at the same path gives different bytes | do not deploy; re-pin only through Codex with a new receipt |
| A0 | any rehearsal against real Squads or devnet ORAO fails (RG-5, RG-6) | Codex decision before any devnet spend |
| A1 | genesis hash is not the devnet pin; faucet cannot supply a bucket; any key file inside the repo or `~/.config/solana/id.json` used | stop, no mainnet funds ever; rotate any exposed key |
| A2 | Squads profile bytes differ from 3b | multisigs are cheap: create new ones (new create keys); nothing depends on them yet |
| A3 | deploy rejected (for example the devnet feature set refuses the ELF) | the uploaded buffer remains; close it to recover its rent. Byte check fails before transfer: D still has authority, redeploy or abandon the id |
| A3 | upgrade-authority transfer fails | stop with D retaining authority, record |
| A4 | proposal stale or execution fails | re-propose; **a wrong `cluster_domain` cannot be changed after initialize**: pre-flight the args by simulation and compare to hex `6d75736963616c2d...763121` before proposing |
| A5 | ORAO `Register` rejected by the devnet build | stop; compatibility finding for Codex |
| A6 | any readback differs from expected | stop |
| A7 | unexpected program error; ORAO does not fulfil within the window repeatedly; bucket cap reached; devnet genesis changes | stop; the single-miss case is an allowed `PromotionSkipped(Deadline)` outcome and is recorded, not a failure |
| A9 | either conservation identity breaks | stop all flows; P pause only under the [operations runbook](../solana/operations.md) conditions (conservation break is one) |
| B1, B2 | module hash differs from the built wasm; `schnorr_public_key` fails | reinstall in place; keeper stays inert and unarmed; never delete a canister |
| B3 | stable-compat check fails; any baseline difference after the upgrade; trap during upgrade | do not upgrade / restore the snapshot; the IC rolls back a trapping upgrade automatically |
| B4 | served asset hash differs; CSP missing the devnet host | redeploy to the same NEW canister |
| C1 | allowlist or policy differs from intended | set again (admin); nothing minted yet |
| C2 | fee address below the floor; unexpected signing | `disarm` |
| C3 | minted PP exceeds the cap; unexpected duplicate or unbound event | stop the relayer; there is no burn path, so minting is irreversible: keep the cap small |
| any | cycles or SOL bucket cap reached | stop; new approval needed |

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
| Frontend | served asset hash and version marker vs build output; CSP header as served; screenshots of the native panels against devnet | `...-frontend.json` |

## 9. Missing user inputs (consolidated)

| id | needed from Rob | blocks |
| --- | --- | --- |
| MI-1 | Who holds the keypair for program id `7tj9...`, or approval to change `declare_id!` and re-derive all pins | A |
| MI-2 | Calendar choice (6.4): literal six-week run, or an amended flow list | A |
| MI-3 | Deployer key: generate fresh, location outside any worktree; confirm `~/.config/solana/id.json` is never used | A |
| MI-4 | Two Squads create keys (fresh, discarded afterwards) | A2 |
| MI-5 | Six voter keys, custody and whether G and P share voters | A2 |
| MI-6 | Sponsor, management recipient, G-vault top-up amount, ORAO client funding amount (defaults in 4b) | A4, A5 |
| MI-7 | Test wallets: five players, two backers, plain wallet W, keeper fee payer K | A7 |
| MI-8 | Which ELF is deployed (recommended macOS `8f97e83d...`) | A |
| MI-9 | ICP: which dfx identity and cycles source pays (only a controller can read Shenanigans' stable signature and cycles balance; the controller identity on this machine is `CharlesPonzi` [C]); controllers for the new canisters (CycleOps?); relayer principal(s) | B1, B3, C1 |
| MI-10 | Shenanigans: B3 base and order vs LEG-1; native policy (copy the live rates or a lower devnet draft); PP mint cap for C3 (5,000 PP proposed, which also bounds every candidate loss in the C3 round) | B3, C3 |
| MI-11 | Devnet RPC for the bridge: two different providers (supported `AlchemyDevnet` / `AnkrDevnet` or custom URLs; any API key stays in a secret store, set only by a controller); the browser RPC and whether it needs a key; B4 on a NEW asset canister | B1, B4 |
| MI-12 | Keeper: `feeKeyName` (production `key_1` vs a test key), arming window, `maxSignaturesPerHour` (proposed 10), `idlePollNs` (proposed 3,600) | B2, C2 |
| MI-13 | ELF capacity headroom (default none; headroom doubles ProgramData rent) | A3 |
| MI-14 | If the public faucet cannot supply the deployer: another devnet source (never mainnet funds) | A1 |
| MI-15 | Whether to include the optional P pause rehearsal (F12) | A7 |

## 10. Appendix: every read-only query and local check

Network reads used only `curl https://api.devnet.solana.com` (methods `getEpochInfo`, `getAccountInfo`, `getMinimumBalanceForRentExemption`, `getProgramAccounts`) and `dfx ... --network ic --identity anonymous` (`canister info`, `canister call --query`, `canister metadata`). Intermediate files were written only to the session scratchpad (`.../scratchpad/m14/`), not to the repository. No private key, keypair file or wallet was read; the keypair search [L11] listed file names only.

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

Files read for this packet: the plan (M14 and surrounding), `latest-build.json`, `.github/workflows/native-solana-model.yml`, `local-cluster.md`, `production-accounts.md`, `measurements-2026-10.md`, `launch-decisions.md`, `operations.md`, `threat-model.md`, the ICP budget, defaults, M12, M13, corrections and second-closure documents, the legacy memo evidence, `dfx.json`, `canister_ids.json`, `frontend/public/.ic-assets.json`, `frontend/src/solana/rpc.ts`, `cluster.ts`, and the program, bridge, keeper and Shenanigans sources cited inline.
