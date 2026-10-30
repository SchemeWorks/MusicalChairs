# Squads v4 `Multisig` fixtures (LOCAL)

`multisig_g.bin` and `multisig_p.bin` are synthetic, byte-exact images of a Squads v4
`Multisig` account for the two authority profiles the program authenticates:

| file | profile | create_key | bytes | sha256 |
|------|---------|-----------|-------|--------|
| `multisig_g.bin` | G, governance: 2-of-3, `time_lock` 172,800 s | `[0x47; 32]` (`5oEzAP4izB65uRm2yDAEf9oALGwHpWkDfyKb8zBY3euC`) | 199 bytes | `57e3c751dc9b3380d9b47d5bf4508e074ca22785939cfa8808af1c8ed70a6960` |
| `multisig_p.bin` | P, pause guardian: 2-of-3, `time_lock` 0 | `[0x50; 32]` (`6QWeT6FpJrm8AF1btu6WH2k2Xhq6t5vbheKVfQavmeoZ`) | 199 bytes | `3a20c6d2b6cfb2771fcefcc854bc475dd27368f2fd747319f5d9950afbe4afc3` |

**These are LOCAL fixtures.** No Squads account exists on any cluster at these addresses, the
bytes were not read from a chain, and nothing here proves real Squads execution, a real time
lock, or a deployed multisig. The LiteSVM suite installs them with `owner = SQUADS_PROGRAM_ID`
and drives vault-signed instructions through a test-only stand-in at the Squads program id
(`solana/tests/financial-runtime/standin/squads-vault-standin`). Real Squads execution and
deployed readback stay M14.

## Source layout

squads-protocol/v4 commit `887627b3ac3f59548fdb6dfd1dfe23a21bb72b60`,
`programs/squads_multisig_program/src/state/multisig.rs`. Anchor account, Borsh, little-endian:

| offset | field | size | fixture value |
|-------:|-------|-----:|---------------|
| 0 | discriminator `sha256("account:Multisig")[..8]` | 8 | `e07479ba44a14fec` |
| 8 | `create_key: Pubkey` | 32 | the file's `create_key` |
| 40 | `config_authority: Pubkey` (plain, NOT an `Option`) | 32 | `Pubkey::default()` (autonomous) |
| 72 | `threshold: u16` | 2 | 2 |
| 74 | `time_lock: u32` | 4 | G 172,800, P 0 |
| 78 | `transaction_index: u64` | 8 | 0 |
| 86 | `stale_transaction_index: u64` | 8 | 0 |
| 94 | `rent_collector: Option<Pubkey>` | 1 (tag) + 32 only if `Some` | `None` = one `0` byte |
| 95 | `bump: u8` | 1 | canonical Multisig PDA bump (G 254, P 252) |
| 96 | `members: Vec<Member>` length (`u32`) | 4 | 3 |
| 100 | `Member { key: Pubkey, permissions: Permissions { mask: u8 } }` x N | 33 each | mask 7 (Initiate 1 + Vote 2 + Execute 4) |

Total: `8 + 32 + 32 + 2 + 4 + 8 + 8 + 1 + 1 + 4 + 3 x 33 = 199` bytes. Members are distinct synthetic
keys: the first byte is the `create_key` tag byte (0x47 / 0x50) and the last byte is the ordinal 1..3.

The files are the exact serialized layout with **no allocation padding**. Squads allocates
`Multisig::size(n)`, which reserves 33 bytes for `rent_collector` even when it is `None`, so a real
account whose collector is `None` carries 32 trailing zero bytes (231 bytes for 3 members); that
size formula is recalled from the Squads source and was not re-checked against the pinned commit
in this fixture work. The program's profile reader walks fields by offset and tolerates trailing
bytes, and the suite row `initialize_accepts_allocation_padded_multisig` exercises both the padded
`None` shape and a populated `rent_collector`.

## Derivations

Squads program id `SQDS4ep65T869zMMBKyuUq6aD6EgTu8psMjkvj52pCf`.

- Multisig PDA: `find_program_address(["multisig", "multisig", create_key], SQUADS)`.
- Vault PDA (index `i`): `find_program_address(["multisig", multisig, "vault", [i]], SQUADS)`.

| | create_key | Multisig PDA | Vault PDA (index 0) |
|-|------------|--------------|---------------------|
| G | `[0x47; 32]` | `5ASstTEwct76TEqMsKw4c9tZcKckyJK7j5doXeY1UPDn` (bump 254) | `nVdpWArd89aMWgicLSvLQZGo7kXf8soRZrJfzuZycz9` (bump 255) |
| P | `[0x50; 32]` | `HfbGhr2YTRhaU8BNLHoPb17P3AGRfGVzV2xnYQQ89QR5` (bump 252) | `ATE3hTDdzc8KjFJfRzs7QCyaaez7ydM9HzrVaxgtSxS` (bump 251) |

The G vault is also the `ProgramData` upgrade authority in `fixtures/loader`.

## Reproducibility

The harness generator `runtime::fixtures::squads::multisig_bytes(create_key, config_authority,
threshold, time_lock, rent_collector, bump, members)` produces both files (via `governance_bytes()` and
`guardian_bytes()`);
`committed_squads_fixtures_equal_the_generator_output` asserts byte equality and
`committed_squads_fixtures_decode_to_the_pinned_profiles` decodes them with an independent reader.
Malformed variants (threshold 1, wrong time lock, bad discriminator, wrong bump, ...) are generated
in memory by the suite and never committed.
