//! ORAO VRF-with-callback adapter (spec sections 4, 7.4, 8.4).
//!
//! Everything that touches the provider lives here so `close.rs` stays about
//! the game: validating the pinned provider tuple, the retained request
//! binding, reading a fulfilled request, and the two CPIs.  The logic is a
//! port of `solana/probes/runtime/program` (`validate_provider_binding`,
//! `read_fulfilled_request`, `probe_request`), which ran against the real
//! ORAO executable in LiteSVM.
//!
//! # Who pays what (the ORAO `request` handler)
//!
//! `Request.payer` "will only pay tx fees and will not pay rent nor request
//! fee": the `Client` PDA pays both the request fee and the pending
//! `RequestAccount` rent.  So `payer` can be any signer and `OpsCash` never
//! signs the ORAO CPI.  `request_round_draw` instead tops the Client up with
//! exactly `fee + pending_request_rent` by system transfer immediately before
//! the CPI; the Client then pays the same amount out, so exactly that amount
//! leaves `OpsCash` per request.
//!
//! # Roles (spec section 5)
//!
//! `AccountInfo.owner` of the Client is the ORAO program; `Client.owner` is
//! the G vault (`pool.governance`); `Client.state` is this program's
//! `RngAuthority` PDA, which CPI-signs requests.

use anchor_lang::prelude::*;
use anchor_lang::solana_program::{program::invoke, system_instruction};
pub use orao_solana_vrf_cb as orao;
use orao::state::{
    client::Client,
    network_state::NetworkState,
    request::{RequestAccount, RequestState},
};

use crate::state::{ConfigVersion, RNG_AUTHORITY_SEED};
use crate::ErrorCode as Code;

/// What a validated provider tuple tells the caller.
pub struct ProviderView {
    /// The decoded ORAO `Client`.
    pub client: Client,
    /// Live per-request fee from `NetworkState.config.request_fee`.
    pub request_fee: u64,
}

/// The program's `RngAuthority` PDA and bump.
pub fn rng_authority_address() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[RNG_AUTHORITY_SEED], &crate::ID)
}

/// The ORAO `Client` PDA for this program's `RngAuthority`.
pub fn client_address() -> (Pubkey, u8) {
    let (authority, _) = rng_authority_address();
    Client::find_address(&crate::ID, &authority, &orao::id())
}

/// `(request PDA, bump)` for `(client, protocol seed)`.
pub fn request_binding(client: &Pubkey, seed: &[u8; 32]) -> (Pubkey, u8) {
    RequestAccount::find_address(client, seed, &orao::id())
}

fn decode<T: AccountDeserialize>(info: &AccountInfo, wrong: Code) -> Result<T> {
    let data = info.try_borrow_data()?;
    let mut bytes: &[u8] = &data;
    T::try_deserialize(&mut bytes).map_err(|_| error!(wrong))
}

/// Validate the provider program and the Client against the close's pinned
/// `ConfigVersion`, and return the decoded Client.  This is all
/// `withdraw_provider_return` needs: the network state and treasury matter only
/// for a request.
///
/// A key that differs from the pinned tuple, and a Client that is not this
/// program's G-owned `RngAuthority` client, are `WrongProviderTuple`; a Client
/// the ORAO program does not own is `WrongOwner`.
pub fn validate_client(
    config: &ConfigVersion,
    governance: &Pubkey,
    program: &AccountInfo,
    client: &AccountInfo,
) -> Result<Client> {
    require_keys_eq!(program.key(), config.provider_program, Code::WrongProviderTuple);
    require_keys_eq!(program.key(), orao::id(), Code::WrongProviderTuple);
    require!(program.executable, Code::WrongProviderTuple);
    require_keys_eq!(client.key(), config.provider_client, Code::WrongProviderTuple);

    let (authority, _) = rng_authority_address();
    let (client_key, client_bump) = Client::find_address(&crate::ID, &authority, &orao::id());
    require_keys_eq!(client.key(), client_key, Code::WrongProviderTuple);
    require_keys_eq!(*client.owner, orao::id(), Code::WrongOwner);
    let decoded: Client = decode(client, Code::WrongProviderTuple)?;
    require!(decoded.bump == client_bump, Code::WrongProviderTuple);
    require_keys_eq!(decoded.program, crate::ID, Code::WrongProviderTuple);
    require_keys_eq!(decoded.state, authority, Code::WrongProviderTuple);
    require_keys_eq!(decoded.owner, *governance, Code::WrongProviderTuple);
    Ok(decoded)
}

/// Validate the whole provider tuple for a request: [`validate_client`] plus the
/// ORAO `NetworkState` and its treasury.  Returns the decoded Client and the
/// live request fee.  Key mismatches against the pinned tuple are
/// `WrongProviderTuple`; a `NetworkState` the ORAO program does not own is
/// `WrongOwner`.
pub fn validate_provider_tuple(
    config: &ConfigVersion,
    governance: &Pubkey,
    program: &AccountInfo,
    network_state: &AccountInfo,
    treasury: &AccountInfo,
    client: &AccountInfo,
) -> Result<ProviderView> {
    let client = validate_client(config, governance, program, client)?;
    require_keys_eq!(network_state.key(), config.provider_network_state, Code::WrongProviderTuple);
    require_keys_eq!(treasury.key(), config.provider_treasury, Code::WrongProviderTuple);

    let (network_key, network_bump) = NetworkState::find_address(&orao::id());
    require_keys_eq!(network_state.key(), network_key, Code::WrongProviderTuple);
    require_keys_eq!(*network_state.owner, orao::id(), Code::WrongOwner);
    let network: NetworkState = decode(network_state, Code::WrongProviderTuple)?;
    require!(network.bump == network_bump, Code::WrongProviderTuple);
    require_keys_eq!(network.config.treasury, treasury.key(), Code::WrongProviderTuple);

    Ok(ProviderView { client, request_fee: network.config.request_fee })
}

/// A request account that passed the retained-binding checks: ORAO-owned, the
/// PDA of `(client, seed)`, ORAO discriminator, and bump, client and seed that
/// match the binding the close recorded.
pub fn read_request(request: &AccountInfo, client: &Pubkey, seed: &[u8; 32]) -> Result<RequestAccount> {
    let (expected, expected_bump) = request_binding(client, seed);
    require_keys_eq!(request.key(), expected, Code::WrongAccount);
    require_keys_eq!(*request.owner, orao::id(), Code::WrongOwner);
    {
        let data = request.try_borrow_data()?;
        require!(
            data.len() >= 8 && data[..8] == *RequestAccount::DISCRIMINATOR,
            Code::WrongAccount
        );
    }
    let decoded: RequestAccount = decode(request, Code::WrongAccount)?;
    require!(decoded.bump == expected_bump, Code::WrongAccount);
    require_keys_eq!(decoded.client, *client, Code::WrongAccount);
    require!(decoded.seed == *seed, Code::WrongAccount);
    Ok(decoded)
}

/// `(randomness, request lamports)` of a fulfilled request; a pending one is
/// `RequestNotFulfilled`.
pub fn read_fulfilled(request: &AccountInfo, client: &Pubkey, seed: &[u8; 32]) -> Result<([u8; 64], u64)> {
    match read_request(request, client, seed)?.state {
        RequestState::Fulfilled(result) => Ok((result.randomness, request.lamports())),
        // `RequestState` is non-exhaustive: anything but Fulfilled is not a result.
        _ => err!(Code::RequestNotFulfilled),
    }
}

/// Whether a bound request is still pending (binding failures are errors).
pub fn request_is_pending(request: &AccountInfo, client: &Pubkey, seed: &[u8; 32]) -> Result<bool> {
    Ok(matches!(read_request(request, client, seed)?.state, RequestState::Pending(_)))
}

/// Whether a request PDA already exists as a provider account: ORAO-owned, or
/// carrying data.  `request_round_draw` fails closed on this and never tries
/// another seed.
pub fn request_exists(request: &AccountInfo) -> bool {
    *request.owner == orao::id() || !request.data_is_empty()
}

/// ORAO `request` CPI, signed by the `RngAuthority` PDA.  `payer` is any
/// signer: the Client pays the fee and the request rent.
#[allow(clippy::too_many_arguments)]
pub fn cpi_request<'info>(
    provider_program: &AccountInfo<'info>,
    payer: &AccountInfo<'info>,
    rng_authority: &AccountInfo<'info>,
    rng_bump: u8,
    client: &AccountInfo<'info>,
    network_state: &AccountInfo<'info>,
    treasury: &AccountInfo<'info>,
    request: &AccountInfo<'info>,
    system_program: &AccountInfo<'info>,
    seed: [u8; 32],
) -> Result<()> {
    let accounts = orao::cpi::accounts::Request {
        payer: payer.clone(),
        state: rng_authority.clone(),
        client: client.clone(),
        network_state: network_state.clone(),
        treasury: treasury.clone(),
        request: request.clone(),
        system_program: system_program.clone(),
    };
    let bump = [rng_bump];
    let signer: &[&[u8]] = &[RNG_AUTHORITY_SEED, &bump];
    let signers = [signer];
    let context = CpiContext::new_with_signer(provider_program.clone(), accounts, &signers);
    orao::cpi::request(context, orao::RequestParams::new(seed))
}

/// ORAO `withdraw` CPI.  The provider pays the Client OWNER, which is the G
/// vault, so `owner` is the signing G vault.
pub fn cpi_withdraw<'info>(
    provider_program: &AccountInfo<'info>,
    owner: &AccountInfo<'info>,
    client: &AccountInfo<'info>,
    amount: u64,
) -> Result<()> {
    let accounts = orao::cpi::accounts::Withdraw { payer: owner.clone(), client: client.clone() };
    let context = CpiContext::new(provider_program.clone(), accounts);
    orao::cpi::withdraw(context, orao::WithdrawParams::new(amount))
}

/// Plain system transfer signed by `from` (the G vault, whose signature the
/// Squads CPI propagates).
pub fn transfer_signed<'info>(
    from: &AccountInfo<'info>,
    to: &AccountInfo<'info>,
    system_program: &AccountInfo<'info>,
    amount: u64,
) -> Result<()> {
    invoke(
        &system_instruction::transfer(from.key, to.key, amount),
        &[from.clone(), to.clone(), system_program.clone()],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use anchor_lang::solana_program::system_program;
    use orao::state::{network_state::NetworkConfiguration, request::{Fulfilled, Pending}};

    fn key(byte: u8) -> Pubkey {
        Pubkey::new_from_array([byte; 32])
    }

    fn code_of<T>(result: Result<T>) -> u32 {
        match result {
            Ok(_) => panic!("expected an error, got Ok"),
            Err(anchor_lang::error::Error::AnchorError(error)) => error.error_code_number,
            Err(other) => panic!("expected an Anchor error, got {other:?}"),
        }
    }

    fn assert_code<T>(result: Result<T>, expected: Code) {
        assert_eq!(code_of(result), expected as u32 + 6000, "{expected:?}");
    }

    /// Leaked-memory account so tests can hold `AccountInfo<'static>`.
    fn info(key: Pubkey, owner: Pubkey, lamports: u64, data: Vec<u8>, executable: bool) -> AccountInfo<'static> {
        let key = Box::leak(Box::new(key));
        let owner = Box::leak(Box::new(owner));
        let lamports = Box::leak(Box::new(lamports));
        let data = Box::leak(data.into_boxed_slice());
        AccountInfo::new(key, false, true, lamports, data, owner, executable, 0)
    }

    fn bytes_of<T: AccountSerialize>(value: &T) -> Vec<u8> {
        let mut out = Vec::new();
        value.try_serialize(&mut out).expect("serializes");
        out
    }

    const FEE: u64 = 300_000;
    const GOVERNANCE: [u8; 32] = [0x66; 32];

    struct Tuple {
        config: ConfigVersion,
        program: AccountInfo<'static>,
        network: AccountInfo<'static>,
        treasury: AccountInfo<'static>,
        client: AccountInfo<'static>,
    }

    fn tuple() -> Tuple {
        let treasury_key = key(0x71);
        let (network_key, network_bump) = NetworkState::find_address(&orao::id());
        let network = NetworkState::new(network_bump, NetworkConfiguration::new(key(0x72), treasury_key, FEE));
        let (authority, _) = rng_authority_address();
        let (client_key, client_bump) = client_address();
        let client = Client::new(client_bump, Pubkey::new_from_array(GOVERNANCE), crate::ID, authority, 0, None);
        let config = ConfigVersion {
            provider_program: orao::id(),
            provider_network_state: network_key,
            provider_treasury: treasury_key,
            provider_client: client_key,
            ..ConfigVersion::default_v1()
        };
        Tuple {
            config,
            program: info(orao::id(), system_program::ID, 1, Vec::new(), true),
            network: info(network_key, orao::id(), 1_000_000, bytes_of(&network), false),
            treasury: info(treasury_key, system_program::ID, 1, Vec::new(), false),
            client: info(client_key, orao::id(), 10_000_000, bytes_of(&client), false),
        }
    }

    fn validate(t: &Tuple) -> Result<ProviderView> {
        validate_provider_tuple(&t.config, &Pubkey::new_from_array(GOVERNANCE), &t.program, &t.network, &t.treasury, &t.client)
    }

    #[test]
    fn a_pinned_g_owned_client_validates_and_reports_the_live_fee() {
        let t = tuple();
        let view = validate(&t).expect("valid tuple");
        assert_eq!(view.request_fee, FEE);
        assert_eq!(view.client.owner, Pubkey::new_from_array(GOVERNANCE));
        assert_eq!(view.client.program, crate::ID);
    }

    #[test]
    fn withdrawal_needs_only_the_program_and_the_client() {
        let t = tuple();
        let g = Pubkey::new_from_array(GOVERNANCE);
        let client = validate_client(&t.config, &g, &t.program, &t.client).expect("valid client");
        assert_eq!(client.owner, g);
        assert_code(validate_client(&t.config, &key(1), &t.program, &t.client), Code::WrongProviderTuple);
        let mut config = t.config.clone();
        config.provider_client = key(2);
        assert_code(validate_client(&config, &g, &t.program, &t.client), Code::WrongProviderTuple);
    }

    #[test]
    fn any_key_that_differs_from_the_pinned_tuple_is_wrong_provider_tuple() {
        let mut t = tuple();
        t.config.provider_treasury = key(1);
        assert_code(validate(&t), Code::WrongProviderTuple);
        let mut t = tuple();
        t.config.provider_network_state = key(2);
        assert_code(validate(&t), Code::WrongProviderTuple);
        let mut t = tuple();
        t.config.provider_client = key(3);
        assert_code(validate(&t), Code::WrongProviderTuple);
        let mut t = tuple();
        t.config.provider_program = key(4);
        assert_code(validate(&t), Code::WrongProviderTuple);
        // A pinned network state that is not the ORAO PDA, even if supplied consistently.
        let mut t = tuple();
        t.config.provider_network_state = key(5);
        let network_bytes = t.network.data.borrow().to_vec();
        t.network = info(key(5), orao::id(), 1, network_bytes, false);
        assert_code(validate(&t), Code::WrongProviderTuple);
    }

    #[test]
    fn a_non_executable_provider_program_is_rejected() {
        let mut t = tuple();
        t.program = info(orao::id(), system_program::ID, 1, Vec::new(), false);
        assert_code(validate(&t), Code::WrongProviderTuple);
    }

    #[test]
    fn a_treasury_that_network_state_does_not_name_is_rejected() {
        let mut t = tuple();
        t.config.provider_treasury = key(9);
        t.treasury = info(key(9), system_program::ID, 1, Vec::new(), false);
        assert_code(validate(&t), Code::WrongProviderTuple);
    }

    #[test]
    fn accounts_the_provider_does_not_own_are_wrong_owner() {
        let t = tuple();
        let impostor = info(t.client.key(), system_program::ID, 1, t.client.data.borrow().to_vec(), false);
        let result = validate_provider_tuple(&t.config, &Pubkey::new_from_array(GOVERNANCE), &t.program, &t.network, &t.treasury, &impostor);
        assert_code(result, Code::WrongOwner);
        let impostor = info(t.network.key(), system_program::ID, 1, t.network.data.borrow().to_vec(), false);
        let result = validate_provider_tuple(&t.config, &Pubkey::new_from_array(GOVERNANCE), &t.program, &impostor, &t.treasury, &t.client);
        assert_code(result, Code::WrongOwner);
    }

    #[test]
    fn a_client_not_owned_by_g_or_not_signed_by_the_rng_authority_is_rejected() {
        let t = tuple();
        // Another owner than the G vault.
        let result = validate_provider_tuple(&t.config, &key(0x01), &t.program, &t.network, &t.treasury, &t.client);
        assert_code(result, Code::WrongProviderTuple);
        // A Client whose request authority is some other state PDA.
        let (client_key, client_bump) = client_address();
        let wrong_state = Client::new(client_bump, Pubkey::new_from_array(GOVERNANCE), crate::ID, key(0x44), 0, None);
        let bad = info(client_key, orao::id(), 1, bytes_of(&wrong_state), false);
        assert_code(validate_provider_tuple(&t.config, &Pubkey::new_from_array(GOVERNANCE), &t.program, &t.network, &t.treasury, &bad), Code::WrongProviderTuple);
        // A Client registered for a different program.
        let (authority, _) = rng_authority_address();
        let wrong_program = Client::new(client_bump, Pubkey::new_from_array(GOVERNANCE), key(0x45), authority, 0, None);
        let bad = info(client_key, orao::id(), 1, bytes_of(&wrong_program), false);
        assert_code(validate_provider_tuple(&t.config, &Pubkey::new_from_array(GOVERNANCE), &t.program, &t.network, &t.treasury, &bad), Code::WrongProviderTuple);
    }

    fn request_info(client: &Pubkey, seed: [u8; 32], state: RequestState, lamports: u64) -> AccountInfo<'static> {
        let (address, bump) = request_binding(client, &seed);
        let account = RequestAccount::new(bump, 5, *client, seed, state);
        info(address, orao::id(), lamports, bytes_of(&account), false)
    }

    fn fulfilled(randomness: [u8; 64]) -> RequestState {
        RequestState::Fulfilled(Fulfilled::new(randomness, None))
    }

    fn pending() -> RequestState {
        RequestState::Pending(Pending::new(vec![], None, false))
    }

    #[test]
    fn a_fulfilled_bound_request_yields_its_randomness_and_lamports() {
        let (client, _) = client_address();
        let seed = [9u8; 32];
        let request = request_info(&client, seed, fulfilled([0xAB; 64]), 1_447_680);
        let (randomness, lamports) = read_fulfilled(&request, &client, &seed).expect("fulfilled");
        assert_eq!(randomness, [0xAB; 64]);
        assert_eq!(lamports, 1_447_680);
        assert!(!request_is_pending(&request, &client, &seed).unwrap());
    }

    #[test]
    fn a_pending_request_is_not_fulfilled_but_is_pending() {
        let (client, _) = client_address();
        let seed = [9u8; 32];
        let request = request_info(&client, seed, pending(), 6_173_520);
        assert_code(read_fulfilled(&request, &client, &seed), Code::RequestNotFulfilled);
        assert!(request_is_pending(&request, &client, &seed).unwrap());
    }

    #[test]
    fn the_retained_binding_rejects_a_different_seed_client_owner_or_contents() {
        let (client, _) = client_address();
        let seed = [9u8; 32];
        let request = request_info(&client, seed, fulfilled([1; 64]), 1);
        // A second close (different seed) cannot reuse this request.
        assert_code(read_fulfilled(&request, &client, &[8u8; 32]), Code::WrongAccount);
        // A different client derives a different address.
        assert_code(read_fulfilled(&request, &key(0x33), &seed), Code::WrongAccount);
        // The right address under the wrong owner.
        let impostor = info(request.key(), system_program::ID, 1, request.data.borrow().to_vec(), false);
        assert_code(read_fulfilled(&impostor, &client, &seed), Code::WrongOwner);
        // The right address with the wrong discriminator.
        let mut data = request.data.borrow().to_vec();
        data[0] ^= 0xFF;
        let garbled = info(request.key(), orao::id(), 1, data, false);
        assert_code(read_fulfilled(&garbled, &client, &seed), Code::WrongAccount);
        // Stored fields that disagree with the PDA.
        let (address, bump) = request_binding(&client, &seed);
        let wrong_bump = RequestAccount::new(bump.wrapping_sub(1), 5, client, seed, fulfilled([1; 64]));
        assert_code(read_fulfilled(&info(address, orao::id(), 1, bytes_of(&wrong_bump), false), &client, &seed), Code::WrongAccount);
        let wrong_client = RequestAccount::new(bump, 5, key(0x34), seed, fulfilled([1; 64]));
        assert_code(read_fulfilled(&info(address, orao::id(), 1, bytes_of(&wrong_client), false), &client, &seed), Code::WrongAccount);
        let wrong_seed = RequestAccount::new(bump, 5, client, [7u8; 32], fulfilled([1; 64]));
        assert_code(read_fulfilled(&info(address, orao::id(), 1, bytes_of(&wrong_seed), false), &client, &seed), Code::WrongAccount);
    }

    #[test]
    fn a_truncated_request_is_wrong_account_not_a_panic() {
        let (client, _) = client_address();
        let seed = [9u8; 32];
        let (address, _) = request_binding(&client, &seed);
        let empty = info(address, orao::id(), 1, Vec::new(), false);
        assert_code(read_fulfilled(&empty, &client, &seed), Code::WrongAccount);
        let short = info(address, orao::id(), 1, vec![0u8; 12], false);
        assert_code(read_fulfilled(&short, &client, &seed), Code::WrongAccount);
    }

    #[test]
    fn an_existing_request_is_detected_by_owner_or_data() {
        let (client, _) = client_address();
        let seed = [9u8; 32];
        let provider_owned = request_info(&client, seed, pending(), 1);
        assert!(request_exists(&provider_owned));
        let (address, _) = request_binding(&client, &seed);
        assert!(!request_exists(&info(address, system_program::ID, 0, Vec::new(), false)));
        assert!(request_exists(&info(address, system_program::ID, 0, vec![1], false)));
    }

    #[test]
    fn addresses_are_the_orao_pdas_of_this_program() {
        let (authority, _) = rng_authority_address();
        assert_eq!(authority, Pubkey::find_program_address(&[b"rng-authority"], &crate::ID).0);
        let (client, _) = client_address();
        assert_eq!(client, Client::find_address(&crate::ID, &authority, &orao::id()).0);
        let (request, _) = request_binding(&client, &[3u8; 32]);
        assert_eq!(
            request,
            Pubkey::find_program_address(&[orao::CB_REQUEST_ACCOUNT_SEED, client.as_ref(), &[3u8; 32]], &orao::id()).0
        );
    }
}
