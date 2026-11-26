//! M14-PRE-ORAO: the close lifecycle against the ORAO VRF-with-callback
//! executable that is deployed on DEVNET (not the mainnet build the rest of the
//! harness is pinned to).
//!
//! # What is real and what is synthetic
//!
//! * REAL: the program bytes.  `solana/fixtures/devnet/orao-devnet.so` is the
//!   612,272-byte ELF taken from the devnet `ProgramData` account
//!   `7mzfTTbQ...` (the 616,464-byte payload minus 4,192 zero allocation-padding
//!   bytes; the packet's `b7f30b89...` is the hash of the whole padded payload).
//!   The fixture-only tests below re-derive that from the committed files.
//! * REAL but never installed as the live network state: the devnet `NetworkState`
//!   and treasury snapshots (`orao-devnet-accounts.json`).  They are decoded and
//!   compared with the packet, and the layout the devnet binary writes locally is
//!   compared with them.
//! * SYNTHETIC: the live `NetworkState` of every flow below.  The devnet binary's
//!   own `initialize`/`configure` create it under the harness payer with the
//!   packet's fee (300,000 lamports) and deadline (9,000 slots) but ONE synthetic
//!   fulfil authority (`[0x55; 32]`), because the four real devnet fulfil
//!   authorities cannot sign locally.  No real devnet oracle behaviour (latency,
//!   liveness, whether it fulfils our requests) is exercised.  The Squads program
//!   is the test-only stand-in (`squads_devnet_compat.rs` loads the real one);
//!   rent is LiteSVM's default schedule, not devnet's.
//!
//! # Running
//!
//! The flow tests are `#[ignore]` and refuse to run on the default (mainnet)
//! profile, so a green run can never mean "tested the wrong binary":
//!
//! ```text
//! MC_ORAO_PROFILE=devnet cargo test --locked --test orao_devnet_compat -- --ignored
//! ```
//!
//! plus the usual `MC_FINANCIAL_SBF` / `MC_STANDIN_SBF` if the artifacts are not
//! at their default paths.  The fixture-only tests run in a plain `cargo test`.

use anchor_lang::AccountDeserialize;
use musical_chairs_financial_runtime::{
    close as m7,
    devnet::{self, OraoProfile},
    runtime,
    world::World,
};
use musical_chairs_program::provider::orao::{self, state::network_state::NetworkState};
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

/// Fail loudly unless the devnet profile is selected and really loaded.
fn require_devnet_profile() {
    assert_eq!(
        devnet::orao_profile(),
        OraoProfile::Devnet,
        "set MC_ORAO_PROFILE=devnet: these flows must run against the devnet ORAO binary, not the mainnet default"
    );
    assert_eq!(
        runtime::provider_artifact_sha256(),
        devnet::ORAO_DEVNET_ELF_SHA256,
        "the loaded provider is not the pinned devnet ELF"
    );
    assert_ne!(devnet::ORAO_DEVNET_ELF_SHA256, runtime::ORAO_ELF_SHA256);
}

/// Run the close-suite row called `name` (`m7::ROW_*`) with the suite's own
/// assertions: the SAME code that passes on the mainnet binary.
fn row(name: &str) {
    let (name, run) = m7::rows()
        .into_iter()
        .find(|(candidate, _)| *candidate == name)
        .unwrap_or_else(|| panic!("no close-suite row {name}"));
    m7::run_row(name, run);
}

// ---------------------------------------------------------------------------
// Fixture-only checks (no profile, no SBF artifacts)
// ---------------------------------------------------------------------------

#[test]
fn devnet_orao_elf_fixture_is_the_pinned_binary_and_explains_the_packet_hash() {
    let elf = devnet::read_orao_devnet_fixture_elf();
    assert_eq!(
        elf.len(),
        612_272,
        "ELF length from the section header table"
    );
    assert_eq!(devnet::sha256_hex(&elf), devnet::ORAO_DEVNET_ELF_SHA256);
    assert_eq!(devnet::elf64_extent(&elf), Ok(elf.len()));

    // The packet's b7f30b89... is the whole 616,464-byte payload: ELF + zeros.
    let mut payload = elf.clone();
    payload.resize(616_464, 0);
    assert_eq!(
        devnet::sha256_hex(&payload),
        "b7f30b89cda7d3f067fbb36f2e817a5e50c8f510f0f8f65f5b87d5a65526ccea"
    );
    assert_eq!(
        devnet::sha256_hex(&payload),
        devnet::ORAO_DEVNET_PAYLOAD_SHA256
    );

    // It is a different build from the mainnet fixture the harness is pinned to,
    // and the mainnet pin itself is untouched.
    assert_ne!(devnet::ORAO_DEVNET_ELF_SHA256, runtime::ORAO_ELF_SHA256);
    assert_eq!(
        runtime::ORAO_ELF_SHA256,
        "6d180b1e91be7d5c541f75e2413146beb047d9beeb76ac8eb1d7c883200c6b8b"
    );

    // The profile selector picks exactly the matching pin.
    match devnet::orao_profile() {
        OraoProfile::Mainnet => {
            assert_eq!(devnet::active_orao_elf_sha256(), runtime::ORAO_ELF_SHA256)
        }
        OraoProfile::Devnet => {
            assert_eq!(
                devnet::active_orao_elf_sha256(),
                devnet::ORAO_DEVNET_ELF_SHA256
            )
        }
    }
}

#[test]
fn devnet_orao_network_state_snapshot_decodes_with_the_vendored_sdk_and_matches_the_packet() {
    let accounts = devnet::load_accounts(devnet::ORAO_DEVNET_ACCOUNTS_FILE);
    let state_account = &accounts["network_state"];
    assert_eq!(
        state_account.address,
        NetworkState::find_address(&orao::id()).0
    );
    assert_eq!(
        state_account.address.to_string(),
        "3e483QYyYQqu8eesT6ZjfuBiWF9PN5JXKeB8TRmn3hUr"
    );
    assert_eq!(state_account.owner, orao::id());
    assert_eq!(state_account.data.len(), 245);

    let network = NetworkState::try_deserialize(&mut state_account.data.as_slice())
        .expect("the devnet NetworkState decodes with the SDK layout the program compiles against");
    let (_, bump) = NetworkState::find_address(&orao::id());
    assert_eq!(network.bump, bump);
    assert_eq!(network.config.request_fee, 300_000);
    assert_eq!(network.config.callback_deadline, 9_000);
    let treasury = Pubkey::from_str("9ZTHWWZDpB36UFe1vszf2KEpt83vwi27jDqtHQ7NSXyR").unwrap();
    assert_eq!(network.config.treasury, treasury);
    assert_eq!(network.config.authority, treasury);
    assert_eq!(network.config.fulfill_authorities.len(), 4);
    for (key, prefix) in network
        .config
        .fulfill_authorities
        .iter()
        .zip(["2VYNr8yM", "A9iZdgYa", "9ZTHWWZD", "HSCqQaDH"])
    {
        assert!(
            key.to_string().starts_with(prefix),
            "fulfil authority {key} should start {prefix}"
        );
    }
    assert_eq!(
        (
            network.num_requests,
            network.num_registered,
            network.num_terminated
        ),
        (44_183, 509, 0)
    );
    assert_eq!(
        8 + network.size(),
        245,
        "the snapshot is exactly the SDK-sized account"
    );

    // The treasury snapshot is a plain system account.
    let treasury_account = &accounts["treasury"];
    assert_eq!(treasury_account.address, treasury);
    assert_eq!(
        treasury_account.owner,
        Pubkey::default(),
        "system program owner"
    );
    assert!(treasury_account.data.is_empty());

    // Program account: loader-v3 Program variant pointing at the packet's ProgramData.
    let program = &accounts["program"];
    assert_eq!(program.address, orao::id());
    assert!(program.executable);
    assert_eq!(program.data.len(), 36);
    assert_eq!(u32::from_le_bytes(program.data[..4].try_into().unwrap()), 2);
    assert_eq!(
        Pubkey::new_from_array(program.data[4..36].try_into().unwrap()).to_string(),
        "7mzfTTbQ4h7S9uNFdzbVUCs7srF6hiLa52y4eotkp1MD"
    );
}

// ---------------------------------------------------------------------------
// Flow tests against the devnet binary (MC_ORAO_PROFILE=devnet)
// ---------------------------------------------------------------------------

/// Client registration through the G-vault path: ORAO `initialize` and
/// `configure`, then `register` with the G vault as payer (the `ProgramData`
/// upgrade authority), then the Client readbacks of the packet.
#[test]
#[ignore = "needs MC_ORAO_PROFILE=devnet (see the file docs)"]
fn client_register_via_the_g_vault_path() {
    require_devnet_profile();
    row(m7::ROW_PROVIDER);

    // The layout the devnet binary writes locally matches the real devnet account.
    let mut world = World::genesis_with_provider();
    world.provider_setup();
    let keys = m7::provider_keys();
    let local = world
        .account(keys.network_state)
        .expect("local NetworkState");
    assert_eq!(local.owner, orao::id());
    let real = &devnet::load_accounts(devnet::ORAO_DEVNET_ACCOUNTS_FILE)["network_state"];
    assert_eq!(
        local.data[..8],
        real.data[..8],
        "the devnet binary writes the same Anchor account discriminator as the real devnet NetworkState"
    );
    assert_eq!(local.data[8], real.data[8], "same PDA bump");
    let decoded = NetworkState::try_deserialize(&mut local.data.as_slice())
        .expect("the devnet binary's own NetworkState decodes with the SDK layout");
    assert_eq!(
        decoded.config.request_fee, 300_000,
        "same fee as the real devnet account"
    );
    assert_eq!(
        decoded.config.callback_deadline, 9_000,
        "same deadline as the real devnet account"
    );
    assert_eq!(
        decoded.num_registered, 1,
        "the local register incremented the counter"
    );
}

#[test]
#[ignore = "needs MC_ORAO_PROFILE=devnet (see the file docs)"]
fn request_round_draw_against_the_devnet_binary() {
    require_devnet_profile();
    row(m7::ROW_REQUEST_CREATES);
    row(m7::ROW_REQUEST_PARTITIONS);
    row(m7::ROW_SEED_VECTORS);
}

#[test]
#[ignore = "needs MC_ORAO_PROFILE=devnet (see the file docs)"]
fn fulfil_with_the_ed25519_authority_against_the_devnet_binary() {
    require_devnet_profile();
    row(m7::ROW_FULFILLMENT);
}

#[test]
#[ignore = "needs MC_ORAO_PROFILE=devnet (see the file docs)"]
fn observe_request_fulfilled_against_the_devnet_binary() {
    require_devnet_profile();
    row(m7::ROW_OBSERVE);
    row(m7::ROW_CHANGED_RENT);
    row(m7::ROW_EXTRA_LAMPORTS);
}

#[test]
#[ignore = "needs MC_ORAO_PROFILE=devnet (see the file docs)"]
fn capture_round_draw_against_the_devnet_binary() {
    require_devnet_profile();
    row(m7::ROW_CAPTURE_DEADLINE);
    row(m7::ROW_TIMELY_CAPTURE);
}

#[test]
#[ignore = "needs MC_ORAO_PROFILE=devnet (see the file docs)"]
fn apply_promotion_against_the_devnet_binary() {
    require_devnet_profile();
    row(m7::ROW_APPLY_PROMOTION);
    row(m7::ROW_ENTROPY_EXHAUSTED);
}

#[test]
#[ignore = "needs MC_ORAO_PROFILE=devnet (see the file docs)"]
fn expire_path_against_the_devnet_binary() {
    require_devnet_profile();
    row(m7::ROW_EXPIRE);
    row(m7::ROW_EXPIRED_UNFULFILLED);
    row(m7::ROW_EXPIRE_DURING_BUILD);
    row(m7::ROW_LATE_FULFILLMENT);
    row(m7::ROW_LATE_AUTHENTIC);
}

/// The provider return executes the ORAO `withdraw` CPI on the devnet binary.
#[test]
#[ignore = "needs MC_ORAO_PROFILE=devnet (see the file docs)"]
fn provider_return_against_the_devnet_binary() {
    require_devnet_profile();
    row(m7::ROW_WITHDRAW_RETURN);
    row(m7::ROW_TWO_PARTIAL);
    row(m7::ROW_RETURN_PREFLIGHT);
    row(m7::ROW_WITHDRAW_REJECTS);
    row(m7::ROW_WITHDRAW_CPI_FAILURE);
    row(m7::ROW_G_SIGNATURE);
    row(m7::ROW_G_WITHDRAWS_ELSEWHERE);
}

/// The rows whose expected failures come from inside the provider: a different
/// ORAO build could change which error it raises or where it raises it.
#[test]
#[ignore = "needs MC_ORAO_PROFILE=devnet (see the file docs)"]
fn provider_side_rejections_against_the_devnet_binary() {
    require_devnet_profile();
    row(m7::ROW_REQUEST_REJECTS_PROVIDER);
    row(m7::ROW_REQUEST_CPI_FAILURE);
    row(m7::ROW_COLLISION);
    row(m7::ROW_REQUEST_AFTER_DEADLINE);
}

/// All 43 rows of the close suite in one go (about as long as the mainnet run).
#[test]
#[ignore = "needs MC_ORAO_PROFILE=devnet (see the file docs)"]
fn whole_close_suite_against_the_devnet_binary() {
    require_devnet_profile();
    let observations = m7::run_close_suite();
    for (name, _) in m7::rows() {
        m7::assert_headline(&observations, name);
    }
    assert_eq!(m7::rows().len(), m7::ROW_NAMES.len());
}
