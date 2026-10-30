//! Read-only compatibility and resource probes for the native Solana provider boundary.
//!
//! This crate deliberately stops at SDK type-checking, deterministic serialization, and
//! unsigned packet measurement. It does not contain a production entrypoint or submit a
//! transaction.

use anchor_lang::ToAccountMetas;
use anchor_lang::{AnchorDeserialize, AnchorSerialize, Discriminator};
use orao_solana_vrf_cb::{
    state::{
        client::Client,
        network_state::{NetworkConfiguration, NetworkState},
        request::{Fulfilled, RequestAccount, RequestState},
    },
    RequestParams,
};
use serde::{Deserialize, Serialize};
use solana_sdk::{
    address_lookup_table::AddressLookupTableAccount,
    hash::Hash,
    instruction::{AccountMeta, Instruction},
    message::{v0, Message, VersionedMessage},
    pubkey::Pubkey,
    signature::Signature,
    transaction::VersionedTransaction,
};
use solana_system_interface::program as system_program;
use std::collections::BTreeMap;

pub const STATE_SEED: &[u8] = b"mc-vrf-state";
pub const BACKER_SEED: &[u8] = b"mc-backer";
pub const SNAPSHOT_PAGE_SEED: &[u8] = b"mc-snapshot-page";
pub const MC_PROGRAM_ID: Pubkey = Pubkey::new_from_array([0x4d; 32]);
pub const PACKET_LIMIT_BYTES: usize = 1_232;
pub const LOCK_LIMIT: usize = 64;

fn system_program_pubkey() -> Pubkey {
    Pubkey::new_from_array(system_program::id().to_bytes())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProviderBinding {
    pub program_id: Pubkey,
    pub state: Pubkey,
    pub state_bump: u8,
    pub client: Pubkey,
    pub client_bump: u8,
    pub network_state: Pubkey,
    pub network_bump: u8,
    pub seed: [u8; 32],
    pub request: Pubkey,
    pub request_bump: u8,
}

pub fn provider_binding(program_id: Pubkey, seed: [u8; 32]) -> ProviderBinding {
    let (state, state_bump) = Pubkey::find_program_address(&[STATE_SEED], &program_id);
    let (client, client_bump) =
        Client::find_address(&program_id, &state, &orao_solana_vrf_cb::id());
    let (network_state, network_bump) = NetworkState::find_address(&orao_solana_vrf_cb::id());
    let (request, request_bump) =
        RequestAccount::find_address(&client, &seed, &orao_solana_vrf_cb::id());
    ProviderBinding {
        program_id,
        state,
        state_bump,
        client,
        client_bump,
        network_state,
        network_bump,
        seed,
        request,
        request_bump,
    }
}

pub fn registration_state_seeds(binding: &ProviderBinding) -> Vec<Vec<u8>> {
    vec![STATE_SEED.to_vec(), vec![binding.state_bump]]
}

pub fn registered_client(binding: &ProviderBinding, owner: Pubkey) -> Client {
    Client::new(
        binding.client_bump,
        owner,
        binding.program_id,
        binding.state,
        0,
        None,
    )
}

pub fn network_configuration(
    authority: Pubkey,
    treasury: Pubkey,
    request_fee: u64,
) -> NetworkConfiguration {
    NetworkConfiguration::new(authority, treasury, request_fee)
}

pub fn request_account(binding: &ProviderBinding, slot: u64) -> RequestAccount {
    RequestAccount::new(
        binding.request_bump,
        slot,
        binding.client,
        binding.seed,
        RequestState::new(None, false),
    )
}

pub fn fulfilled_request_account(
    binding: &ProviderBinding,
    slot: u64,
    randomness: [u8; 64],
) -> RequestAccount {
    RequestAccount::new(
        binding.request_bump,
        slot,
        binding.client,
        binding.seed,
        RequestState::Fulfilled(Fulfilled::new(randomness, None)),
    )
}

pub fn account_bytes<T: AnchorSerialize + Discriminator>(value: &T) -> Vec<u8> {
    let mut bytes = T::DISCRIMINATOR.to_vec();
    value
        .serialize(&mut bytes)
        .expect("Anchor serialization is infallible for probe values");
    bytes
}

pub fn account_allocation_bytes(binding: &ProviderBinding) -> (usize, usize, usize) {
    let client = registered_client(binding, Pubkey::new_from_array([8; 32]));
    let client_bytes = 8 + Client::STATIC_SIZE;
    let pending_bytes = 8 + RequestAccount::expected_size(&client, None);
    let fulfilled_bytes = 8 + RequestAccount::FULFILLED_SIZE;
    (client_bytes, pending_bytes, fulfilled_bytes)
}

pub fn request_params(seed: [u8; 32]) -> RequestParams {
    RequestParams::new(seed)
}

/// Construct the CPI context type used by ORAO's generated interface. The function is not
/// called by this probe; its signature keeps the exact Anchor 0.32.1 account binding checked.
#[allow(dead_code)]
pub fn typecheck_request_cpi<'info>(
    oracle_program: anchor_lang::prelude::AccountInfo<'info>,
    accounts: orao_solana_vrf_cb::cpi::accounts::Request<'info>,
    params: RequestParams,
) -> anchor_lang::Result<()> {
    let context = anchor_lang::prelude::CpiContext::new(oracle_program, accounts);
    orao_solana_vrf_cb::cpi::request(context, params)
}

pub fn request_account_metas(
    binding: &ProviderBinding,
    payer: Pubkey,
    treasury: Pubkey,
) -> Vec<AccountMeta> {
    // This order mirrors the generated ORAO `Request` Accounts struct in the pinned SDK.
    vec![
        AccountMeta::new(payer, true),
        AccountMeta::new_readonly(binding.state, true),
        AccountMeta::new(binding.client, false),
        AccountMeta::new(binding.network_state, false),
        AccountMeta::new(treasury, false),
        AccountMeta::new(binding.request, false),
        AccountMeta::new_readonly(system_program_pubkey(), false),
    ]
}

/// Ask Anchor's generated ORAO CPI account type for its metas. Keeping this adapter in the
/// probe prevents a hand-maintained account order from silently drifting from the SDK.
pub fn sdk_request_account_metas<'info>(
    accounts: &orao_solana_vrf_cb::cpi::accounts::Request<'info>,
) -> Vec<AccountMeta> {
    accounts.to_account_metas(None)
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum FulfillmentError {
    #[error("request account owner is not ORAO")]
    WrongOwner,
    #[error("request PDA does not match the bound client and seed")]
    WrongRequest,
    #[error("request client does not match the bound client")]
    WrongClient,
    #[error("request seed does not match the bound seed")]
    WrongSeed,
    #[error("request account is not fulfilled")]
    NotFulfilled,
    #[error("request account bytes are malformed")]
    Malformed,
}

pub fn validate_fulfilled_request(
    owner: Pubkey,
    request_key: Pubkey,
    binding: &ProviderBinding,
    bytes: &[u8],
) -> Result<[u8; 64], FulfillmentError> {
    if owner != orao_solana_vrf_cb::id() {
        return Err(FulfillmentError::WrongOwner);
    }
    let (canonical_request, canonical_bump) =
        RequestAccount::find_address(&binding.client, &binding.seed, &orao_solana_vrf_cb::id());
    if request_key != canonical_request
        || binding.request != canonical_request
        || binding.request_bump != canonical_bump
    {
        return Err(FulfillmentError::WrongRequest);
    }
    if bytes.len() < 8 || bytes[..8] != *RequestAccount::DISCRIMINATOR {
        return Err(FulfillmentError::Malformed);
    }
    let mut remaining = &bytes[8..];
    let account =
        RequestAccount::deserialize(&mut remaining).map_err(|_| FulfillmentError::Malformed)?;
    if !remaining.is_empty() {
        return Err(FulfillmentError::Malformed);
    }
    if account.client != binding.client {
        return Err(FulfillmentError::WrongClient);
    }
    if account.seed != binding.seed {
        return Err(FulfillmentError::WrongSeed);
    }
    if account.bump != canonical_bump {
        return Err(FulfillmentError::WrongRequest);
    }
    account
        .fulfilled()
        .map(|result| result.randomness)
        .ok_or(FulfillmentError::NotFulfilled)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResourceQuote {
    pub serialized_client_bytes: usize,
    pub serialized_request_pending_bytes: usize,
    pub serialized_request_fulfilled_bytes: usize,
    pub client_account_bytes: usize,
    pub request_pending_account_bytes: usize,
    pub request_fulfilled_account_bytes: usize,
    pub sdk_default_client_rent_lamports: u64,
    pub sdk_default_request_pending_rent_lamports: u64,
    pub sdk_default_request_fulfilled_rent_lamports: u64,
    pub observed_oct12_client_rent_lamports: u64,
    pub observed_oct12_request_pending_rent_lamports: u64,
    pub observed_oct12_request_fulfilled_rent_lamports: u64,
    pub observed_oct12_rent_fit_formula: &'static str,
    pub observed_oct12_rent_snapshot_date: &'static str,
    pub observed_oct12_rent_snapshot_source: &'static str,
    pub request_fee_lamports: u64,
}

pub fn resource_quote(request_fee_lamports: u64) -> ResourceQuote {
    let client = registered_client(
        &provider_binding(MC_PROGRAM_ID, [7; 32]),
        Pubkey::new_from_array([8; 32]),
    );
    let binding = provider_binding(MC_PROGRAM_ID, [7; 32]);
    let pending = request_account(&binding, 1);
    let fulfilled = fulfilled_request_account(&binding, 1, [0xabu8; 64]);
    let serialized_client_len = account_bytes(&client).len();
    let serialized_pending_len = account_bytes(&pending).len();
    let serialized_fulfilled_len = account_bytes(&fulfilled).len();
    let (client_len, pending_len, fulfilled_len) = account_allocation_bytes(&binding);
    let rent = solana_sdk::rent::Rent::default();
    ResourceQuote {
        serialized_client_bytes: serialized_client_len,
        serialized_request_pending_bytes: serialized_pending_len,
        serialized_request_fulfilled_bytes: serialized_fulfilled_len,
        client_account_bytes: client_len,
        request_pending_account_bytes: pending_len,
        request_fulfilled_account_bytes: fulfilled_len,
        sdk_default_client_rent_lamports: rent.minimum_balance(client_len),
        sdk_default_request_pending_rent_lamports: rent.minimum_balance(pending_len),
        sdk_default_request_fulfilled_rent_lamports: rent.minimum_balance(fulfilled_len),
        observed_oct12_client_rent_lamports: 5_080 * (client_len as u64 + 128),
        observed_oct12_request_pending_rent_lamports: 5_080 * (pending_len as u64 + 128),
        observed_oct12_request_fulfilled_rent_lamports: 5_080 * (fulfilled_len as u64 + 128),
        observed_oct12_rent_fit_formula: "5080*(bytes+128)",
        observed_oct12_rent_snapshot_date: "2026-10-12 America/Los_Angeles",
        observed_oct12_rent_snapshot_source:
            "docs/coordination/evidence/2026-10-12-native-solana-costs.json",
        request_fee_lamports,
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PacketMeasurement {
    pub chunk_capacity: usize,
    pub rows_per_instruction: usize,
    pub snapshot_rows: usize,
    pub snapshot_pages: usize,
    pub account_count: usize,
    pub packet_bytes: usize,
    pub within_packet_limit: bool,
    pub within_lock_limit: bool,
    pub version: String,
    pub alt: bool,
}

fn backer_pda(row: usize) -> Pubkey {
    Pubkey::find_program_address(&[BACKER_SEED, &(row as u64).to_le_bytes()], &MC_PROGRAM_ID).0
}

fn snapshot_page_pda(page: usize) -> Pubkey {
    Pubkey::find_program_address(
        &[SNAPSHOT_PAGE_SEED, &(page as u64).to_le_bytes()],
        &MC_PROGRAM_ID,
    )
    .0
}

fn resource_instruction(rows: usize, snapshot_rows: usize) -> Instruction {
    let pages = snapshot_rows.div_ceil(16);
    let mut accounts = vec![
        AccountMeta::new_readonly(Pubkey::new_from_array([0x11; 32]), false),
        AccountMeta::new(Pubkey::new_from_array([0x02; 32]), false),
        AccountMeta::new_readonly(Pubkey::new_from_array([0x03; 32]), false),
        AccountMeta::new_readonly(Pubkey::new_from_array([0x04; 32]), false),
        AccountMeta::new(Pubkey::new_from_array([0x05; 32]), false),
        AccountMeta::new_readonly(Pubkey::new_from_array([0x06; 32]), false),
        AccountMeta::new_readonly(Pubkey::new_from_array([0x07; 32]), false),
        AccountMeta::new_readonly(Pubkey::new_from_array([0x08; 32]), false),
    ];
    accounts.extend((0..rows).map(|i| AccountMeta::new(backer_pda(i), false)));
    accounts.extend((0..pages).map(|i| AccountMeta::new(snapshot_page_pda(i), false)));
    Instruction {
        program_id: MC_PROGRAM_ID,
        accounts,
        data: vec![0x52, 0x45, 0x53, 0x01],
    }
}

fn unsigned_transaction(message: VersionedMessage) -> VersionedTransaction {
    let required = match &message {
        VersionedMessage::Legacy(message) => message.header.num_required_signatures,
        VersionedMessage::V0(message) => message.header.num_required_signatures,
    } as usize;
    VersionedTransaction {
        signatures: vec![Signature::default(); required],
        message,
    }
}

#[cfg(test)]
pub(crate) fn unsigned_legacy_transaction(
    rows: usize,
    snapshot_rows: usize,
) -> VersionedTransaction {
    let payer = Pubkey::new_from_array([0x01; 32]);
    let instruction = resource_instruction(rows, snapshot_rows);
    unsigned_transaction(VersionedMessage::Legacy(Message::new(
        &[instruction],
        Some(&payer),
    )))
}

pub fn measure_packet(chunk_capacity: usize, snapshot_rows: usize, alt: bool) -> PacketMeasurement {
    let rows_per_instruction = chunk_capacity.min(snapshot_rows);
    let payer = Pubkey::new_from_array([0x01; 32]);
    let instruction = resource_instruction(rows_per_instruction, snapshot_rows);
    let blockhash = Hash::new_from_array([0x99; 32]);
    let (message, version) = if alt {
        let lookup_key = Pubkey::new_from_array([0x77; 32]);
        let addresses = instruction
            .accounts
            .iter()
            .skip(8)
            .map(|meta| meta.pubkey)
            .collect::<Vec<_>>();
        let lookup = AddressLookupTableAccount {
            key: lookup_key,
            addresses,
        };
        (
            VersionedMessage::V0(
                v0::Message::try_compile(&payer, &[instruction], &[lookup], blockhash)
                    .expect("v0 compile"),
            ),
            "v0".to_owned(),
        )
    } else {
        (
            VersionedMessage::Legacy(Message::new(&[instruction], Some(&payer))),
            "legacy".to_owned(),
        )
    };
    let packet_bytes = bincode::serialize(&unsigned_transaction(message.clone()))
        .expect("packet serialization")
        .len();
    let account_count = match message {
        VersionedMessage::Legacy(message) => message.account_keys.len(),
        VersionedMessage::V0(message) => {
            message.account_keys.len()
                + message
                    .address_table_lookups
                    .iter()
                    .map(|lookup| lookup.writable_indexes.len() + lookup.readonly_indexes.len())
                    .sum::<usize>()
        }
    };
    PacketMeasurement {
        chunk_capacity,
        rows_per_instruction,
        snapshot_rows,
        snapshot_pages: snapshot_rows.div_ceil(16),
        account_count,
        packet_bytes,
        within_packet_limit: packet_bytes <= PACKET_LIMIT_BYTES,
        within_lock_limit: account_count <= LOCK_LIMIT,
        version,
        alt,
    }
}

pub fn packet_matrix() -> Vec<PacketMeasurement> {
    [1, 4, 8, 16, 32]
        .into_iter()
        .flat_map(|capacity| {
            [0, 1, 16, 17, 100, 256]
                .into_iter()
                .map(move |snapshot| measure_packet(capacity, snapshot, false))
        })
        .collect()
}

pub fn packet_alt_matrix() -> Vec<PacketMeasurement> {
    [1, 4, 8, 16, 32]
        .into_iter()
        .flat_map(|capacity| {
            [0, 1, 16, 17, 100, 256]
                .into_iter()
                .map(move |snapshot| measure_packet(capacity, snapshot, true))
        })
        .collect()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProbeSnapshot {
    pub crate_name: &'static str,
    pub anchor_version: &'static str,
    pub orao_version: &'static str,
    pub orao_archive_sha256: &'static str,
    pub rustc: String,
    pub proof_limits: BTreeMap<&'static str, usize>,
    pub binding: ProviderBinding,
    pub quote: ResourceQuote,
    pub packets: Vec<PacketMeasurement>,
    pub alt_packets: Vec<PacketMeasurement>,
    pub unsupported_tools: Vec<&'static str>,
    pub provider_randomness_output_bytes: usize,
}

pub fn probe_snapshot() -> ProbeSnapshot {
    let mut proof_limits = BTreeMap::new();
    proof_limits.insert("packet_bytes_legacy_v0", PACKET_LIMIT_BYTES);
    proof_limits.insert("account_locks", LOCK_LIMIT);
    let binding = provider_binding(MC_PROGRAM_ID, [0x42; 32]);
    ProbeSnapshot {
        crate_name: "musical-chairs-provider-resources",
        anchor_version: "0.32.1",
        orao_version: "0.4.0",
        orao_archive_sha256: "a036bfe6b101f9583797e4923d8c8fe838604a735ed843b4959b7238f649d9a7",
        rustc: "rustc 1.92.0".to_owned(),
        proof_limits,
        binding,
        quote: resource_quote(300_000),
        packets: packet_matrix(),
        alt_packets: packet_alt_matrix(),
        unsupported_tools: vec!["anchor", "solana", "cargo-build-sbf", "surfpool"],
        provider_randomness_output_bytes: 64,
    }
}

#[cfg(test)]
mod tests;
