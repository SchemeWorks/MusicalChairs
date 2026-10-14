# Upgradeable-loader fixtures (LOCAL)

`initialize` binds the program to its governance by reading its own upgradeable-loader accounts, so the
LiteSVM suite installs the loader shape at the main program id. These two files are the serialized
`UpgradeableLoaderState` **headers only**; the compiled ELF is appended to the `ProgramData` header at
runtime (the real account is `header ++ ELF`), so the ELF is not committed here.

| file | content | bytes | sha256 |
|------|---------|-------|--------|
| `program.bin` | `UpgradeableLoaderState::Program { programdata_address }`: variant `2` (u32 LE) + 32-byte address | 36 bytes | `d5c51489d02b2eccfa8f6166d767f772400e7f2f23297de866065e7860f6e030` |
| `programdata.bin` | `UpgradeableLoaderState::ProgramData { slot: 1, upgrade_authority_address: Some(G vault) }`: variant `3` (u32 LE) + slot (u64 LE) + `Some` tag `1` + 32-byte authority | 45 bytes | `c3432f47b2770180e3e78989e4fe0d2c3d909a90d9f8d1f155729e69050a3479` |

- Main program id: `7tj9biW3KRJ7EEWmVUGigHiouCTXhV2dzcyvwma7Cyu7`. `program.bin` depends on it (through the
  ProgramData address); a changed program id makes `committed_loader_fixtures_equal_the_generator_output` fail
  until the file is regenerated.
- ProgramData address: `find_program_address([program id], BPFLoaderUpgradeab1e11111111111111111111111)` =
  `9fbEej8qbMTZbWBgN19AME3WA6kRSWE3JmHM1hfX8Lup`.
- Authority: the G vault `nVdpWArd89aMWgicLSvLQZGo7kXf8soRZrJfzuZycz9` (see `fixtures/squads/README.md`).
  An immutable (`None`) program serializes to a zero tag byte followed by 32 zero bytes (still 45 bytes);
  the suite builds that and an authority-mismatch variant in memory.

## How the suite uses them

1. `add_program(main id, ELF)` loads the actual compiled ELF.
2. The ProgramData account is written first (header ++ ELF, owner = upgradeable loader), then the Program
   account (`program.bin`, `executable`, owner = upgradeable loader). LiteSVM loads the program from the
   ProgramData account, so execution keeps using the actual ELF. This is the same shape the runtime probes
   (`solana/probes/runtime/harness/src/provider.rs`, `install_client_loader_fixture`) already proved.
3. Variants rewrite only the 45-byte ProgramData header (`set_programdata_authority`), never the ELF.

These are LOCAL fixtures: nothing was deployed, and no on-chain loader account is read or implied.
