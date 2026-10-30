//! Devnet program fixtures for the OPTIONAL M14 preflight checks.
//!
//! `solana/fixtures/devnet/` holds the exact program bytes that run on Solana
//! devnet (read-only snapshots, see the `*-provenance.json` files there):
//!
//! * `orao-devnet.so`: the ORAO VRF-with-callback executable at
//!   `VRFCBePmGTpZ234BhbzNNzmyg39Rgdd6VgdfhHwKypU`.  It is NOT the mainnet build
//!   the rest of the harness is pinned to (`ORAO_ELF_SHA256`), so it is selected
//!   by an explicit profile, never by default (see [`OraoProfile`]).
//! * `squads-devnet.so`: the real Squads v4 program at
//!   `SQDS4ep65T869zMMBKyuUq6aD6EgTu8psMjkvj52pCf` (used only by
//!   `FinancialRuntime::new_uninitialized_real_squads`).
//!
//! Every loader here re-derives the pins instead of trusting a file name: the
//! ELF's SHA-256 is compared with a constant in this file, the provenance JSON
//! must repeat it, the ELF length must equal the section-table rule, and
//! `header || ELF || zero padding` must reproduce the recorded SHA-256 of the
//! whole on-chain `ProgramData` account.

use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
    str::FromStr,
};

use base64::Engine;
use serde_json::Value;
use sha2::{Digest, Sha256};
use solana_sdk::{account::Account, pubkey::Pubkey};

/// Environment variable selecting which ORAO executable provider-bearing
/// runtimes load.  Unset (or `mainnet`) keeps the mainnet pin; `devnet`
/// selects `solana/fixtures/devnet/orao-devnet.so` and its own pins.  Any other
/// value panics, so a typo can never silently test the wrong binary.
pub const ORAO_PROFILE_ENV: &str = "MC_ORAO_PROFILE";

/// SHA-256 of the devnet ORAO ELF (`orao-devnet.so`, 612,272 bytes: the
/// on-chain payload minus its zero allocation padding).
pub const ORAO_DEVNET_ELF_SHA256: &str =
    "0fbe2e1d5c16d14d8d2dde1ed43d48c34d9475b786e759d30bcdc591a116be1d";
/// SHA-256 of the whole 616,464-byte post-header `ProgramData` payload of the
/// devnet ORAO program: the value `b7f30b89...` in the M14 approval packet
/// (entries R5, L10).
pub const ORAO_DEVNET_PAYLOAD_SHA256: &str =
    "b7f30b89cda7d3f067fbb36f2e817a5e50c8f510f0f8f65f5b87d5a65526ccea";
/// SHA-256 of the real Squads v4 devnet ELF (`squads-devnet.so`, 980,160 bytes).
pub const SQUADS_DEVNET_ELF_SHA256: &str =
    "1b4e69fdf6f1ba1aecf6f86f713f507ace3f700e6e00e52c54448ee1847dd181";
/// SHA-256 of the whole 1,774,800-byte post-header Squads `ProgramData`
/// payload (packet entry R5).
pub const SQUADS_DEVNET_PAYLOAD_SHA256: &str =
    "97fb2a5c08e5df5e862d8354ac5b97d3587117f98c4457754ca12e30bef0664d";
/// SHA-256 of the Squads v4 IDL read from the devnet on-chain IDL account.
pub const SQUADS_DEVNET_IDL_SHA256: &str =
    "04168070401256812adb4df65b0a3259f7ae6438c6ebb796e865b9acd3db809f";

pub const ORAO_DEVNET_ELF_FILE: &str = "orao-devnet.so";
pub const ORAO_DEVNET_PROVENANCE_FILE: &str = "orao-devnet-provenance.json";
pub const ORAO_DEVNET_ACCOUNTS_FILE: &str = "orao-devnet-accounts.json";
pub const SQUADS_DEVNET_ELF_FILE: &str = "squads-devnet.so";
pub const SQUADS_DEVNET_PROVENANCE_FILE: &str = "squads-devnet-provenance.json";
pub const SQUADS_DEVNET_ACCOUNTS_FILE: &str = "squads-devnet-accounts.json";
pub const SQUADS_DEVNET_IDL_FILE: &str = "squads-devnet-idl.json";

/// Which ORAO executable the provider-bearing runtimes load.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OraoProfile {
    /// The captured mainnet ELF pinned by `ORAO_ELF_SHA256` (the default).
    Mainnet,
    /// The ELF deployed on devnet, pinned by [`ORAO_DEVNET_ELF_SHA256`].
    Devnet,
}

impl OraoProfile {
    pub fn name(self) -> &'static str {
        match self {
            OraoProfile::Mainnet => "mainnet",
            OraoProfile::Devnet => "devnet",
        }
    }
}

/// The active ORAO profile (`MC_ORAO_PROFILE`; default `mainnet`).
pub fn orao_profile() -> OraoProfile {
    match env::var(ORAO_PROFILE_ENV) {
        Err(env::VarError::NotPresent) => OraoProfile::Mainnet,
        Err(env::VarError::NotUnicode(value)) => {
            panic!("{ORAO_PROFILE_ENV} is not valid unicode: {value:?}")
        }
        Ok(value) => match value.as_str() {
            "" | "mainnet" => OraoProfile::Mainnet,
            "devnet" => OraoProfile::Devnet,
            other => panic!("{ORAO_PROFILE_ENV}={other:?} is not one of \"mainnet\" or \"devnet\""),
        },
    }
}

/// The ELF SHA-256 the active profile pins.
pub fn active_orao_elf_sha256() -> &'static str {
    match orao_profile() {
        OraoProfile::Mainnet => crate::runtime::ORAO_ELF_SHA256,
        OraoProfile::Devnet => ORAO_DEVNET_ELF_SHA256,
    }
}

/// `solana/fixtures/devnet/`.
pub fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/devnet")
}

pub fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn read_json(path: &Path) -> Value {
    let bytes = fs::read(path)
        .unwrap_or_else(|error| panic!("devnet fixture {} is required: {error}", path.display()));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("devnet fixture {} is not JSON: {error}", path.display()))
}

pub fn read_fixture_json(name: &str) -> Value {
    read_json(&fixture_dir().join(name))
}

/// The end of a 64-bit little-endian ELF file, from the file itself:
/// `max(e_shoff + e_shnum * e_shentsize, max(p_offset + p_filesz),
/// max(sh_offset + sh_size over sections that occupy file bytes))`.  For these
/// SBF objects the section header table is the last thing in the file, so the
/// three terms agree; the maximum is taken anyway so a different layout cannot
/// make the extraction too short.
pub fn elf64_extent(elf: &[u8]) -> Result<usize, String> {
    if elf.len() < 64 || &elf[..4] != b"\x7fELF" {
        return Err("no ELF magic".into());
    }
    if elf[4] != 2 || elf[5] != 1 {
        return Err("not an ELF64 little-endian file".into());
    }
    let u16_at = |offset: usize| -> Result<u64, String> {
        elf.get(offset..offset + 2)
            .map(|bytes| u64::from(u16::from_le_bytes(bytes.try_into().unwrap())))
            .ok_or_else(|| format!("ELF truncated at {offset}"))
    };
    let u32_at = |offset: usize| -> Result<u64, String> {
        elf.get(offset..offset + 4)
            .map(|bytes| u64::from(u32::from_le_bytes(bytes.try_into().unwrap())))
            .ok_or_else(|| format!("ELF truncated at {offset}"))
    };
    let u64_at = |offset: usize| -> Result<u64, String> {
        elf.get(offset..offset + 8)
            .map(|bytes| u64::from_le_bytes(bytes.try_into().unwrap()))
            .ok_or_else(|| format!("ELF truncated at {offset}"))
    };
    let phoff = u64_at(0x20)? as usize;
    let shoff = u64_at(0x28)? as usize;
    let phentsize = u16_at(0x36)? as usize;
    let phnum = u16_at(0x38)? as usize;
    let shentsize = u16_at(0x3a)? as usize;
    let shnum = u16_at(0x3c)? as usize;
    let mut end = shoff + shnum * shentsize;
    for index in 0..phnum {
        let header = phoff + index * phentsize;
        end = end.max((u64_at(header + 8)? + u64_at(header + 32)?) as usize);
    }
    for index in 0..shnum {
        let header = shoff + index * shentsize;
        const SHT_NOBITS: u64 = 8;
        if u32_at(header + 4)? != SHT_NOBITS {
            end = end.max((u64_at(header + 0x18)? + u64_at(header + 0x20)?) as usize);
        }
    }
    Ok(end)
}

/// Read a committed devnet program ELF and re-verify everything its
/// provenance claims.  `elf_path` is normally the committed file; the
/// `MC_ORAO_SBF` override passes its own path and is held to the same pins.
fn read_program_fixture(
    elf_path: &Path,
    provenance_file: &str,
    program_id: &Pubkey,
    pinned_elf_sha256: &str,
    pinned_payload_sha256: &str,
) -> Vec<u8> {
    let elf = fs::read(elf_path)
        .unwrap_or_else(|error| panic!("devnet ELF {} is required: {error}", elf_path.display()));
    assert_eq!(
        sha256_hex(&elf),
        pinned_elf_sha256,
        "{} is not the pinned devnet ELF",
        elf_path.display()
    );
    let provenance = read_fixture_json(provenance_file);
    assert_eq!(
        provenance["program_id"].as_str(),
        Some(program_id.to_string().as_str()),
        "{provenance_file} names a different program id"
    );
    assert_eq!(
        provenance["elf"]["sha256"].as_str(),
        Some(pinned_elf_sha256),
        "{provenance_file} no longer pins the ELF this harness loads"
    );
    assert_eq!(
        provenance["elf"]["len"].as_u64(),
        Some(elf.len() as u64),
        "{provenance_file}: ELF length"
    );
    assert_eq!(
        elf64_extent(&elf),
        Ok(elf.len()),
        "the ELF's own section/segment table must end exactly at the extracted length"
    );
    let programdata = &provenance["programdata"];
    assert_eq!(
        programdata["payload_sha256"].as_str(),
        Some(pinned_payload_sha256),
        "{provenance_file}: the ProgramData payload hash differs from the packet value"
    );
    let padding = provenance["elf"]["trailing_zero_padding_len"]
        .as_u64()
        .expect("padding length") as usize;
    assert_eq!(
        programdata["payload_len"].as_u64(),
        Some((elf.len() + padding) as u64)
    );

    // header || ELF || zero padding must reproduce the whole on-chain account.
    let authority = Pubkey::from_str(
        programdata["header_upgrade_authority"]
            .as_str()
            .expect("upgrade authority"),
    )
    .expect("upgrade authority is a pubkey");
    let mut header = Vec::with_capacity(45);
    header.extend_from_slice(&3u32.to_le_bytes());
    header.extend_from_slice(
        &programdata["header_deployment_slot"]
            .as_u64()
            .expect("deployment slot")
            .to_le_bytes(),
    );
    header.push(1);
    header.extend_from_slice(authority.as_ref());
    assert_eq!(
        sha256_hex(&header),
        programdata["header_sha256"].as_str().unwrap()
    );
    let mut payload = elf.clone();
    payload.resize(elf.len() + padding, 0);
    assert_eq!(sha256_hex(&payload), pinned_payload_sha256);
    let mut account = header;
    account.extend_from_slice(&payload);
    assert_eq!(
        sha256_hex(&account),
        programdata["account_data_sha256"].as_str().unwrap(),
        "header || ELF || padding must reproduce the recorded ProgramData account hash"
    );
    elf
}

/// The committed devnet ORAO ELF (ignores `MC_ORAO_SBF` and the profile), with
/// every pin and provenance cross-check applied.
pub fn read_orao_devnet_fixture_elf() -> Vec<u8> {
    read_program_fixture(
        &fixture_dir().join(ORAO_DEVNET_ELF_FILE),
        ORAO_DEVNET_PROVENANCE_FILE,
        &musical_chairs_program::provider::orao::id(),
        ORAO_DEVNET_ELF_SHA256,
        ORAO_DEVNET_PAYLOAD_SHA256,
    )
}

/// The ORAO ELF the devnet profile loads: `MC_ORAO_SBF` when set (still held
/// to the devnet pins), else the committed file.
pub(crate) fn read_orao_devnet_provider_elf() -> Vec<u8> {
    let path = env::var_os("MC_ORAO_SBF")
        .map(PathBuf::from)
        .unwrap_or_else(|| fixture_dir().join(ORAO_DEVNET_ELF_FILE));
    read_program_fixture(
        &path,
        ORAO_DEVNET_PROVENANCE_FILE,
        &musical_chairs_program::provider::orao::id(),
        ORAO_DEVNET_ELF_SHA256,
        ORAO_DEVNET_PAYLOAD_SHA256,
    )
}

/// The committed real Squads v4 devnet ELF with every pin and provenance
/// cross-check applied.
pub fn read_squads_devnet_elf() -> Vec<u8> {
    read_program_fixture(
        &fixture_dir().join(SQUADS_DEVNET_ELF_FILE),
        SQUADS_DEVNET_PROVENANCE_FILE,
        &musical_chairs_program::controls::SQUADS_PROGRAM_ID,
        SQUADS_DEVNET_ELF_SHA256,
        SQUADS_DEVNET_PAYLOAD_SHA256,
    )
}

/// One account snapshot from a `*-devnet-accounts.json` fixture.
#[derive(Clone, Debug)]
pub struct DevnetAccount {
    pub address: Pubkey,
    pub owner: Pubkey,
    pub lamports: u64,
    pub executable: bool,
    pub data: Vec<u8>,
    pub context_slot: u64,
}

impl DevnetAccount {
    /// As a LiteSVM account (rent epoch 0, like every other harness fixture).
    pub fn to_account(&self) -> Account {
        Account {
            lamports: self.lamports,
            data: self.data.clone(),
            owner: self.owner,
            executable: false,
            rent_epoch: 0,
        }
    }
}

/// The accounts of `solana/fixtures/devnet/<file>`, keyed by role name
/// (`program`, `network_state`, `treasury`, `program_config`, ...).  Each
/// record's data hash and length are re-checked.
pub fn load_accounts(file: &str) -> BTreeMap<String, DevnetAccount> {
    let document = read_fixture_json(file);
    let accounts = document["accounts"]
        .as_object()
        .unwrap_or_else(|| panic!("{file} has no accounts object"));
    accounts
        .iter()
        .map(|(name, record)| {
            let data = base64::engine::general_purpose::STANDARD
                .decode(record["data_base64"].as_str().expect("data_base64"))
                .unwrap_or_else(|error| panic!("{file}:{name} data is not base64: {error}"));
            assert_eq!(
                sha256_hex(&data),
                record["data_sha256"].as_str().expect("data_sha256"),
                "{file}:{name} data hash"
            );
            assert_eq!(data.len() as u64, record["space"].as_u64().expect("space"));
            let account = DevnetAccount {
                address: Pubkey::from_str(record["address"].as_str().expect("address"))
                    .unwrap_or_else(|error| panic!("{file}:{name} address: {error}")),
                owner: Pubkey::from_str(record["owner"].as_str().expect("owner"))
                    .unwrap_or_else(|error| panic!("{file}:{name} owner: {error}")),
                lamports: record["lamports"].as_u64().expect("lamports"),
                executable: record["executable"].as_bool().expect("executable"),
                data,
                context_slot: record["context_slot"].as_u64().expect("context_slot"),
            };
            (name.clone(), account)
        })
        .collect()
}
