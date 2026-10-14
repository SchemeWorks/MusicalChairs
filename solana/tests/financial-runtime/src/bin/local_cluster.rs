//! IMPL-46d: bootstrap a LOCAL `solana-test-validator` for the M11 browser
//! verification.  Dev tooling only — nothing here touches a public network.
//!
//! ```text
//! local_cluster fixtures <out_dir>                 # multisig account JSONs + the validator args
//! local_cluster genesis  <rpc_url> <keys_dir>      # real `initialize` through the Squads stand-in,
//!                                                  # ORAO initialize/configure/register, client funding,
//!                                                  # first pages, operations funding
//! local_cluster oracle   <rpc_url> <keys_dir>      # fulfil pending ORAO requests with the synthetic
//!                                                  # authority (the probes' 0x55 key), forever
//! ```
//!
//! The validator is started by the operator with the printed arguments: the
//! program as an UPGRADEABLE program whose authority is the G vault (so
//! `initialize`'s `ProgramData.upgrade_authority == G vault` check and ORAO's
//! `Register` hold), the Squads vault stand-in at the Squads program id, the
//! captured ORAO ELF at its id, and the two canonical multisig accounts.
use std::{env, fs, path::Path, thread, time::Duration};

use anchor_lang::AccountDeserialize;
use base64::Engine;
use musical_chairs_financial_runtime::runtime::{
    anchor_ix,
    fixtures::squads::{governance_bytes, guardian_bytes, multisig_address, vault_address},
    pda, G_CREATE_KEY, P_CREATE_KEY,
};
use musical_chairs_program::{
    accounts,
    controls::{Funding, InitializeArgs, PageKind, SQUADS_PROGRAM_ID},
    instruction,
    provider::{self, orao},
    state, ID,
};
use solana_rpc_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    compute_budget::ComputeBudgetInstruction,
    ed25519_program,
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    rent::Rent,
    signature::{Keypair, Signer},
    sysvar,
    transaction::Transaction,
};
use solana_system_interface::{instruction as system_instruction, program as system_program};

const REQUEST_FEE: u64 = 300_000;
const CALLBACK_DEADLINE_SLOTS: u64 = 9_000;
const CLIENT_FUNDING: u64 = 50_000_000;
const OPERATIONS_FUNDING: u64 = 5_000_000_000;
const AIRDROP: u64 = 500_000_000_000;
const CU: u32 = 1_400_000;
const FULFILL_AUTHORITY_SEED: [u8; 32] = [0x55; 32];
const TREASURY: [u8; 32] = [0x77; 32];
const CLUSTER_DOMAIN: [u8; 32] = *b"musical-chairs-localnet-dev!!!!!";

struct Keys {
    g_multisig: Pubkey,
    g_vault: Pubkey,
    p_multisig: Pubkey,
    p_vault: Pubkey,
}

fn keys() -> Keys {
    let g_multisig = multisig_address(&Pubkey::new_from_array(G_CREATE_KEY)).0;
    let p_multisig = multisig_address(&Pubkey::new_from_array(P_CREATE_KEY)).0;
    Keys {
        g_multisig,
        g_vault: vault_address(&g_multisig, 0).0,
        p_multisig,
        p_vault: vault_address(&p_multisig, 0).0,
    }
}

fn program_data_address() -> Pubkey {
    Pubkey::find_program_address(&[ID.as_ref()], &solana_sdk_ids::bpf_loader_upgradeable::id()).0
}

fn account_json(pubkey: &Pubkey, owner: &Pubkey, data: &[u8]) -> String {
    let lamports = Rent::default().minimum_balance(data.len());
    let b64 = base64::engine::general_purpose::STANDARD.encode(data);
    format!(
        "{{\n  \"pubkey\": \"{pubkey}\",\n  \"account\": {{\n    \"lamports\": {lamports},\n    \"data\": [\"{b64}\", \"base64\"],\n    \"owner\": \"{owner}\",\n    \"executable\": false,\n    \"rentEpoch\": 0,\n    \"space\": {}\n  }}\n}}\n",
        data.len()
    )
}

fn write_keypair(path: &Path, keypair: &Keypair) {
    let bytes: Vec<u8> = keypair.to_bytes().to_vec();
    fs::write(path, serde_json::to_string(&bytes).expect("json")).expect("write keypair");
}

fn read_or_create_keypair(path: &Path, seed: Option<[u8; 32]>) -> Keypair {
    if path.exists() {
        let bytes: Vec<u8> = serde_json::from_str(&fs::read_to_string(path).expect("read keypair")).expect("keypair json");
        Keypair::try_from(&bytes[..]).expect("64-byte keypair")
    } else {
        let keypair = match seed {
            Some(seed) => Keypair::new_from_array(seed),
            None => Keypair::new(),
        };
        write_keypair(path, &keypair);
        keypair
    }
}

/// The Squads vault stand-in wrapper: `[vault_index, bump] ++ inner.data`,
/// accounts `[multisig, vault, inner program] ++ inner accounts` with the
/// vault demoted from signer (the stand-in signs for it).  Same shape as the
/// harness' `execute_via_vault`.
fn via_vault(multisig: Pubkey, vault_index: u8, inner: Instruction) -> Instruction {
    let (vault, bump) = vault_address(&multisig, vault_index);
    let mut data = Vec::with_capacity(2 + inner.data.len());
    data.push(vault_index);
    data.push(bump);
    data.extend_from_slice(&inner.data);
    let mut accounts = vec![
        AccountMeta::new_readonly(multisig, false),
        AccountMeta::new_readonly(vault, false),
        AccountMeta::new_readonly(inner.program_id, false),
    ];
    accounts.extend(inner.accounts.into_iter().map(|mut meta| {
        if meta.pubkey == vault {
            meta.is_signer = false;
        }
        meta
    }));
    Instruction { program_id: SQUADS_PROGRAM_ID, accounts, data }
}

fn send(rpc: &RpcClient, payer: &Keypair, label: &str, mut instructions: Vec<Instruction>, extra: &[&Keypair]) {
    instructions.insert(0, ComputeBudgetInstruction::set_compute_unit_limit(CU));
    let blockhash = rpc.get_latest_blockhash().expect("blockhash");
    let mut signers: Vec<&Keypair> = vec![payer];
    signers.extend_from_slice(extra);
    let tx = Transaction::new_signed_with_payer(&instructions, Some(&payer.pubkey()), &signers, blockhash);
    match rpc.send_and_confirm_transaction(&tx) {
        Ok(signature) => println!("{label}: {signature}"),
        Err(error) => {
            eprintln!("{label} FAILED: {error}");
            std::process::exit(1);
        }
    }
}

fn airdrop(rpc: &RpcClient, to: &Pubkey, lamports: u64) {
    let signature = rpc.request_airdrop(to, lamports).expect("airdrop");
    for _ in 0..60 {
        if rpc.confirm_transaction(&signature).unwrap_or(false) {
            return;
        }
        thread::sleep(Duration::from_millis(500));
    }
    panic!("airdrop to {to} not confirmed");
}

fn fixtures(out: &Path) {
    fs::create_dir_all(out).expect("out dir");
    let k = keys();
    fs::write(out.join("multisig_g.json"), account_json(&k.g_multisig, &SQUADS_PROGRAM_ID, &governance_bytes())).expect("g");
    fs::write(out.join("multisig_p.json"), account_json(&k.p_multisig, &SQUADS_PROGRAM_ID, &guardian_bytes())).expect("p");
    let args = format!(
        "--reset --quiet --rpc-port 8899 --faucet-port 9900 \\\n  --upgradeable-program {ID} <PROGRAM_SO> {} \\\n  --bpf-program {SQUADS_PROGRAM_ID} <STANDIN_SO> \\\n  --bpf-program {} <ORAO_SO> \\\n  --account {} {}/multisig_g.json \\\n  --account {} {}/multisig_p.json\n",
        k.g_vault,
        orao::id(),
        k.g_multisig,
        out.display(),
        k.p_multisig,
        out.display()
    );
    fs::write(out.join("validator-args.txt"), &args).expect("args");
    println!("G multisig {}\nG vault    {}\nP multisig {}\nP vault    {}\nprogram_data {}\nvalidator args written to {}/validator-args.txt:\n{args}", k.g_multisig, k.g_vault, k.p_multisig, k.p_vault, program_data_address(), out.display());
}

fn genesis(url: &str, keys_dir: &Path) {
    fs::create_dir_all(keys_dir).expect("keys dir");
    let rpc = RpcClient::new_with_commitment(url.to_string(), CommitmentConfig::confirmed());
    let payer = read_or_create_keypair(&keys_dir.join("payer.json"), None);
    let fulfill_authority = read_or_create_keypair(&keys_dir.join("fulfill-authority.json"), Some(FULFILL_AUTHORITY_SEED));
    let k = keys();
    let treasury = Pubkey::new_from_array(TREASURY);
    println!("payer {}", payer.pubkey());
    airdrop(&rpc, &payer.pubkey(), AIRDROP);
    airdrop(&rpc, &k.g_vault, 10_000_000_000);
    airdrop(&rpc, &treasury, 1_000_000_000);

    let network_state = orao::state::network_state::NetworkState::find_address(&orao::id()).0;
    let (rng_authority, rng_bump) = provider::rng_authority_address();
    let client = provider::client_address().0;

    // 1. Program genesis: `initialize` signed by the G vault through the stand-in; the payer is the sponsor
    //    and the management recipient (dev only).
    let args = InitializeArgs {
        sponsor: payer.pubkey(),
        management_recipient: payer.pubkey(),
        cluster_domain: CLUSTER_DOMAIN,
        provider_request_fee_lamports: REQUEST_FEE,
        provider_fee_headroom_bps: 2_500,
        provider_program: orao::id(),
        provider_network_state: network_state,
        provider_treasury: treasury,
        provider_client: client,
        public_series_a_enabled: true,
    };
    let initialize = anchor_ix(
        accounts::Initialize {
            sponsor: payer.pubkey(),
            pool: pda(&[state::POOL_SEED]).0,
            admission: pda(&[state::ADMISSION_SEED]).0,
            game_vault: pda(&[state::GAME_VAULT_SEED]).0,
            ops: pda(&[state::OPS_SEED]).0,
            ticket: pda(&[state::TICKET_SEED]).0,
            rng_authority,
            config: pda(&[state::CONFIG_SEED, &1u32.to_le_bytes()]).0,
            ops_cash: pda(&[state::OPS_CASH_SEED]).0,
            squads_program: SQUADS_PROGRAM_ID,
            governance_multisig: k.g_multisig,
            governance: k.g_vault,
            guardian_multisig: k.p_multisig,
            guardian: k.p_vault,
            self_program: ID,
            program_data: program_data_address(),
            system_program: system_program::ID,
        },
        instruction::Initialize { args },
    );
    send(&rpc, &payer, "initialize (via G vault)", vec![via_vault(k.g_multisig, 0, initialize)], &[]);

    // 2. ORAO: network state, configuration with ONE synthetic fulfil authority, registration by the G vault.
    send(&rpc, &payer, "orao initialize", vec![Instruction { program_id: orao::id(), accounts: anchor_lang::ToAccountMetas::to_account_metas(&orao::accounts::Initialize { payer: payer.pubkey(), network_state, system_program: system_program::ID }, None), data: anchor_lang::InstructionData::data(&orao::instruction::Initialize { params: orao::InitializeParams::new(REQUEST_FEE) }) }], &[]);
    let mut config = orao::state::network_state::NetworkConfiguration::new(payer.pubkey(), treasury, REQUEST_FEE);
    config.callback_deadline = CALLBACK_DEADLINE_SLOTS;
    config.fulfill_authorities = vec![fulfill_authority.pubkey()];
    send(&rpc, &payer, "orao configure", vec![Instruction { program_id: orao::id(), accounts: anchor_lang::ToAccountMetas::to_account_metas(&orao::accounts::Configure { payer: payer.pubkey(), network_state, system_program: system_program::ID }, None), data: anchor_lang::InstructionData::data(&orao::instruction::Configure { params: orao::ConfigureParams::new(config) }) }], &[]);
    let register = Instruction {
        program_id: orao::id(),
        accounts: anchor_lang::ToAccountMetas::to_account_metas(&orao::accounts::Register { payer: k.g_vault, program: ID, program_data: program_data_address(), state: rng_authority, client, network_state, system_program: system_program::ID }, None),
        data: anchor_lang::InstructionData::data(&orao::instruction::Register { params: orao::RegisterParams::new(vec![state::RNG_AUTHORITY_SEED.to_vec(), vec![rng_bump]]) }),
    };
    send(&rpc, &payer, "orao register (via G vault)", vec![via_vault(k.g_multisig, 0, register)], &[]);
    send(&rpc, &payer, "fund orao client", vec![system_instruction::transfer(&payer.pubkey(), &client, CLIENT_FUNDING)], &[]);

    // 3. Operations cash and the first pages (caller-funded by the payer).
    send(&rpc, &payer, "fund operations", vec![anchor_ix(accounts::FundOperations { sender: payer.pubkey(), ops: pda(&[state::OPS_SEED]).0, ops_cash: pda(&[state::OPS_CASH_SEED]).0, system_program: system_program::ID, event_authority: pda(&[b"__event_authority"]).0, program: ID }, instruction::FundOperations { amount: OPERATIONS_FUNDING })], &[]);
    let provision = |kind: PageKind, index: u64, page: Pubkey| {
        anchor_ix(
            accounts::ProvisionPage { pool: pda(&[state::POOL_SEED]).0, ops: pda(&[state::OPS_SEED]).0, ops_cash: pda(&[state::OPS_CASH_SEED]).0, page, payer: payer.pubkey(), close_record: None, system_program: system_program::ID },
            instruction::ProvisionPage { kind, index, funding: Funding::Caller },
        )
    };
    send(&rpc, &payer, "provision participant page round 1 / 0", vec![provision(PageKind::Participant { round: 1 }, 0, pda(&[state::PARTICIPANTS_SEED, &1u64.to_le_bytes(), &0u64.to_le_bytes()]).0)], &[]);
    send(&rpc, &payer, "provision registry page 0", vec![provision(PageKind::Registry, 0, pda(&[state::REGISTRY_SEED, &0u64.to_le_bytes()]).0)], &[]);
    send(&rpc, &payer, "provision snapshot page 0", vec![provision(PageKind::Snapshot, 0, pda(&[state::SNAPSHOT_SEED, &0u64.to_le_bytes()]).0)], &[]);
    println!("genesis complete: pool {} ops {} client {} rng_authority {}", pda(&[state::POOL_SEED]).0, pda(&[state::OPS_SEED]).0, client, rng_authority);
}

fn ed25519_instruction(pubkey: &Pubkey, signature: &[u8; 64], message: &[u8]) -> Instruction {
    let start = 16usize;
    let pubkey_offset = start as u16;
    let signature_offset = (start + 32) as u16;
    let message_offset = (start + 32 + 64) as u16;
    let mut data = Vec::with_capacity(start + 32 + 64 + message.len());
    data.extend_from_slice(&[1, 0]);
    data.extend_from_slice(&signature_offset.to_le_bytes());
    data.extend_from_slice(&u16::MAX.to_le_bytes());
    data.extend_from_slice(&pubkey_offset.to_le_bytes());
    data.extend_from_slice(&u16::MAX.to_le_bytes());
    data.extend_from_slice(&message_offset.to_le_bytes());
    data.extend_from_slice(&(message.len() as u16).to_le_bytes());
    data.extend_from_slice(&u16::MAX.to_le_bytes());
    data.extend_from_slice(pubkey.as_ref());
    data.extend_from_slice(signature);
    data.extend_from_slice(message);
    Instruction { program_id: ed25519_program::id(), accounts: Vec::new(), data }
}

fn oracle(url: &str, keys_dir: &Path) {
    let rpc = RpcClient::new_with_commitment(url.to_string(), CommitmentConfig::confirmed());
    let payer = read_or_create_keypair(&keys_dir.join("payer.json"), None);
    let authority = read_or_create_keypair(&keys_dir.join("fulfill-authority.json"), Some(FULFILL_AUTHORITY_SEED));
    let network_state = orao::state::network_state::NetworkState::find_address(&orao::id()).0;
    let (rng_authority, _) = provider::rng_authority_address();
    let client = provider::client_address().0;
    // The oracle's payer only pays fees; top it up from the local faucet when it is (nearly) empty.
    if rpc.get_balance(&payer.pubkey()).unwrap_or(0) < 1_000_000_000 {
        airdrop(&rpc, &payer.pubkey(), 10_000_000_000);
    }
    println!("oracle: fulfil authority {} payer {} polling {url}", authority.pubkey(), payer.pubkey());
    loop {
        let pool_data = rpc.get_account_data(&pda(&[state::POOL_SEED]).0).expect("pool");
        let pool = state::Pool::try_deserialize(&mut &pool_data[..]).expect("pool decode");
        for close_id in 1..pool.next_close_id {
            let close_key = pda(&[state::CLOSE_SEED, &close_id.to_le_bytes()]).0;
            let Ok(data) = rpc.get_account_data(&close_key) else { continue };
            let Ok(close) = state::CloseRecord::try_deserialize(&mut &data[..]) else { continue };
            if close.draw_state != state::DRAW_REQUESTED {
                continue;
            }
            let Ok(request) = rpc.get_account(&close.request_account) else { continue };
            if request.data.len() != usize::from(759u16) {
                continue; // already fulfilled (147 B) or not a pending request
            }
            let mut message = Vec::with_capacity(64);
            message.extend_from_slice(client.as_ref());
            message.extend_from_slice(&close.protocol_seed);
            let signature = authority.sign_message(&message);
            let signature_bytes: &[u8; 64] = signature.as_ref().try_into().expect("64-byte signature");
            let fulfill = Instruction {
                program_id: orao::id(),
                accounts: anchor_lang::ToAccountMetas::to_account_metas(&orao::accounts::Fulfill { payer: payer.pubkey(), program: ID, state: rng_authority, client, request: close.request_account, network_state, instruction_acc: sysvar::instructions::id() }, None),
                data: anchor_lang::InstructionData::data(&orao::instruction::Fulfill { params: orao::FulfillParams::new() }),
            };
            send(&rpc, &payer, &format!("fulfil close {close_id}"), vec![ed25519_instruction(&authority.pubkey(), signature_bytes, &message), fulfill], &[]);
        }
        thread::sleep(Duration::from_secs(2));
    }
}

/// Decoded Pool / Ops / Ticket summary of the running cluster (read-only).
fn status(url: &str) {
    let rpc = RpcClient::new_with_commitment(url.to_string(), CommitmentConfig::confirmed());
    let pool_data = rpc.get_account_data(&pda(&[state::POOL_SEED]).0).expect("pool");
    let pool = state::Pool::try_deserialize(&mut &pool_data[..]).expect("pool decode");
    let ops_data = rpc.get_account_data(&pda(&[state::OPS_SEED]).0).expect("ops");
    let ops = state::Ops::try_deserialize(&mut &ops_data[..]).expect("ops decode");
    let ticket_data = rpc.get_account_data(&pda(&[state::TICKET_SEED]).0).expect("ticket");
    let ticket = state::Ticket::try_deserialize(&mut &ticket_data[..]).expect("ticket decode");
    let vault = rpc.get_balance(&pda(&[state::GAME_VAULT_SEED]).0).expect("vault");
    let cash = rpc.get_balance(&pda(&[state::OPS_CASH_SEED]).0).expect("ops cash");
    println!(
        "slot {}\npool: round_id {} active_config {} participant_count {} registry_len {} snapshot_pages {} next_close_id {} next_ticket {} active_ticket {}\n      pot {} seed {} management {} pending_toll {} funded_claims {} excess {}  (vault lamports {})\nops:  free {} locked {} unallocated_excess {}  (cash lamports {})\nticket: id {} phase {}",
        rpc.get_slot().unwrap_or(0), pool.round_id, pool.active_config_version, pool.participant_count, pool.registry_len, pool.snapshot_pages, pool.next_close_id, pool.next_ticket, pool.active_ticket,
        pool.pot, pool.seed, pool.management, pool.pending_toll, pool.funded_claims, pool.excess, vault,
        ops.free_balance, ops.locked_remaining_total, ops.unallocated_excess, cash, ticket.id, ticket.phase
    );
}

/// Export a deterministic CLOSING world as validator genesis accounts (IMPL-46d,
/// Codex M11 checkpoint): the harness `World` builds — in LiteSVM, with the same
/// production ELF — genesis + the real ORAO provider setup + one 1-SOL Simple
/// position opened TEN DAYS before wall time over a 0.1-SOL pot (the harness'
/// `set_pot_fixture`, a labelled synthetic reduction), so the owner's first
/// payout on the validator is insolvent and closes the round today.  Every
/// program / ORAO / Squads account of that world is written as `--account`
/// JSON; the validator starts from them directly (no `genesis` step).  The
/// owner keypair is written to `<keys>/owner.json` for the browser's dev-only
/// local wallet.  Synthetic fixture setup, not a deployment rehearsal.
fn export_closing_fixture(out: &Path, keys_dir: &Path) {
    use musical_chairs_financial_runtime::{close::provider_keys, world::World};
    use musical_chairs_program::positions::PlanKindArg;
    fs::create_dir_all(out).expect("out dir");
    fs::create_dir_all(keys_dir).expect("keys dir");
    let wall_now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).expect("clock").as_secs();
    let opened_at = wall_now - 10 * 86_400;
    let mut world = World::genesis_with_provider();
    let setup = world.provider_setup();
    assert!(setup.iter().all(|o| o.success), "ORAO provider setup in LiteSVM");
    // The World's own clock starts after wall-time-minus-ten-days, so the open runs under a
    // backdated Clock sysvar (the harness' backdated-clock probe): the position's opened_at /
    // last_accrued_at are the backdated time, which is what the validator's real clock accrues from.
    let owner = world.owner();
    world.backdate_clock(opened_at);
    world.open_position("fixture-open", &owner, PlanKindArg::Simple, 1_000_000_000, 1).expect("fixture open_position");
    world.restore_clock();
    world.set_pot_fixture(100_000_000);
    // Operations free balance funds the close reserve at the closing payout (the harness World's
    // genesis carries none; the real `genesis` subcommand funds 5 SOL the same way).
    world.set_ops_free(5_000_000_000);
    write_keypair(&keys_dir.join("owner.json"), &owner);
    let fulfill_authority = read_or_create_keypair(&keys_dir.join("fulfill-authority.json"), Some(FULFILL_AUTHORITY_SEED));
    let pk = provider_keys();
    let rt = &world.rt;
    let owner_key = owner.pubkey();
    let named: Vec<(&str, Pubkey)> = vec![
        ("pool", rt.pool_address()), ("admission", rt.admission_address()), ("game_vault", rt.game_vault_address()), ("ops", rt.ops_address()),
        ("ops_cash", rt.ops_cash_address()), ("ticket", rt.ticket_address()), ("rng_authority", rt.rng_authority_address()), ("config_1", rt.config_address(1)),
        ("participant_page_1_0", rt.participant_address(1, 0)), ("registry_0", rt.registry_address(0)), ("snapshot_0", rt.snapshot_address(0)),
        ("user_owner", rt.user_address(owner_key)), ("user_round_1_owner", rt.user_round_address(1, owner_key)), ("position_owner_1", rt.position_address(owner_key, 1)),
        ("orao_network_state", pk.network_state), ("orao_client", pk.client), ("orao_treasury", pk.treasury),
        ("multisig_g", rt.g_multisig()), ("multisig_p", rt.p_multisig()), ("g_vault", rt.g_vault()),
    ];
    let mut args = String::from("--reset --quiet --rpc-port 8899 --faucet-port 9900 \\\n  --upgradeable-program 7tj9biW3KRJ7EEWmVUGigHiouCTXhV2dzcyvwma7Cyu7 <PROGRAM_SO> ");
    args.push_str(&format!("{} \\\n  --bpf-program {SQUADS_PROGRAM_ID} <STANDIN_SO> \\\n  --bpf-program {} <ORAO_SO>", rt.g_vault(), orao::id()));
    let mut manifest = Vec::new();
    for (name, key) in named {
        let Some(account) = rt.svm.get_account(&key) else { println!("skip {name} ({key}): absent in the world"); continue };
        let b64 = base64::engine::general_purpose::STANDARD.encode(&account.data);
        let json = format!(
            "{{\n  \"pubkey\": \"{key}\",\n  \"account\": {{\n    \"lamports\": {},\n    \"data\": [\"{b64}\", \"base64\"],\n    \"owner\": \"{}\",\n    \"executable\": false,\n    \"rentEpoch\": 0,\n    \"space\": {}\n  }}\n}}\n",
            account.lamports, account.owner, account.data.len()
        );
        let file = out.join(format!("{name}.json"));
        fs::write(&file, json).expect("account json");
        args.push_str(&format!(" \\\n  --account {key} {}", file.display()));
        manifest.push(format!("{name} {key} owner={} lamports={} bytes={}", account.owner, account.lamports, account.data.len()));
    }
    args.push('\n');
    fs::write(out.join("validator-args.txt"), &args).expect("args");
    fs::write(out.join("manifest.txt"), format!("closing fixture exported at wall {wall_now}; position opened_at {opened_at} (wall - 10 days); pot fixture 100,000,000; owner {owner_key}; fulfil authority {}\n{}\n", fulfill_authority.pubkey(), manifest.join("\n"))).expect("manifest");
    println!("exported {} accounts to {}\nowner {} (secret in {}/owner.json)\nvalidator args:\n{args}", manifest.len(), out.display(), owner_key, keys_dir.display());
}

fn main() {
    let args: Vec<String> = env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("status") => status(args.get(2).expect("rpc url")),
        Some("export-closing-fixture") => export_closing_fixture(Path::new(args.get(2).expect("out dir")), Path::new(args.get(3).expect("keys dir"))),
        Some("fixtures") => fixtures(Path::new(args.get(2).expect("out dir"))),
        Some("genesis") => genesis(args.get(2).expect("rpc url"), Path::new(args.get(3).expect("keys dir"))),
        Some("oracle") => oracle(args.get(2).expect("rpc url"), Path::new(args.get(3).expect("keys dir"))),
        _ => {
            eprintln!("usage: local_cluster status <rpc_url> | fixtures <out_dir> | export-closing-fixture <out_dir> <keys_dir> | genesis <rpc_url> <keys_dir> | oracle <rpc_url> <keys_dir>");
            std::process::exit(2);
        }
    }
}
