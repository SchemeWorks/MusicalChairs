use super::*;

#[test]
fn sdk_binding_uses_registered_state_client_and_request_pdas() {
    let binding = provider_binding(MC_PROGRAM_ID, [0x42; 32]);
    let (state, bump) = Pubkey::find_program_address(&[STATE_SEED], &MC_PROGRAM_ID);
    assert_eq!(binding.state, state);
    assert_eq!(binding.state_bump, bump);
    let (client, client_bump) =
        Client::find_address(&MC_PROGRAM_ID, &binding.state, &orao_solana_vrf_cb::id());
    assert_eq!(binding.client, client);
    assert_eq!(binding.client_bump, client_bump);
    let (request, request_bump) =
        RequestAccount::find_address(&binding.client, &binding.seed, &orao_solana_vrf_cb::id());
    assert_eq!(binding.request, request);
    assert_eq!(binding.request_bump, request_bump);
    let seeds = registration_state_seeds(&binding);
    assert_eq!(
        Pubkey::create_program_address(
            &seeds.iter().map(Vec::as_slice).collect::<Vec<_>>(),
            &MC_PROGRAM_ID
        )
        .unwrap(),
        binding.state
    );
}

#[test]
fn sdk_serialization_and_size_constants_match_measured_accounts() {
    let binding = provider_binding(MC_PROGRAM_ID, [7; 32]);
    let client = registered_client(&binding, Pubkey::new_from_array([8; 32]));
    let pending = request_account(&binding, 1);
    let fulfilled = fulfilled_request_account(&binding, 1, [0xab; 64]);
    assert_eq!(account_bytes(&client).len(), 114);
    assert_eq!(account_bytes(&pending).len(), 88);
    assert_eq!(account_bytes(&fulfilled).len(), 147);
    assert_eq!(account_allocation_bytes(&binding), (123, 759, 147));
    assert_eq!(
        account_bytes(&fulfilled).len(),
        8 + RequestAccount::FULFILLED_SIZE
    );
    let quote = resource_quote(300_000);
    assert_eq!(
        (
            quote.sdk_default_client_rent_lamports,
            quote.sdk_default_request_pending_rent_lamports,
            quote.sdk_default_request_fulfilled_rent_lamports,
        ),
        (1_746_960, 6_173_520, 1_914_000)
    );
    assert_eq!(
        (
            quote.observed_oct12_client_rent_lamports,
            quote.observed_oct12_request_pending_rent_lamports,
            quote.observed_oct12_request_fulfilled_rent_lamports,
        ),
        (1_275_080, 4_505_960, 1_397_000)
    );
    assert_ne!(
        quote.sdk_default_request_pending_rent_lamports,
        quote.observed_oct12_request_pending_rent_lamports
    );
    assert_eq!(
        quote.observed_oct12_rent_snapshot_source,
        "docs/coordination/evidence/2026-10-12-native-solana-costs.json"
    );
    assert_eq!(quote.request_fee_lamports, 300_000);
}

#[test]
fn request_cpi_account_metas_are_the_pinned_oracle_order() {
    let binding = provider_binding(MC_PROGRAM_ID, [7; 32]);
    let payer = Pubkey::new_from_array([1; 32]);
    let treasury = Pubkey::new_from_array([9; 32]);
    let metas = request_account_metas(&binding, payer, treasury);
    assert_eq!(metas.len(), 7);
    assert_eq!(metas[0], AccountMeta::new(payer, true));
    assert_eq!(metas[1], AccountMeta::new_readonly(binding.state, true));
    assert_eq!(metas[2], AccountMeta::new(binding.client, false));
    assert_eq!(metas[3], AccountMeta::new(binding.network_state, false));
    assert_eq!(metas[4], AccountMeta::new(treasury, false));
    assert_eq!(metas[5], AccountMeta::new(binding.request, false));
    assert_eq!(
        metas[6],
        AccountMeta::new_readonly(system_program_pubkey(), false)
    );

    let infos = [
        mock_info(payer, true, true),
        mock_info(binding.state, true, false),
        mock_info(binding.client, false, true),
        mock_info(binding.network_state, false, true),
        mock_info(treasury, false, true),
        mock_info(binding.request, false, true),
        mock_info(system_program_pubkey(), false, false),
    ];
    let sdk_accounts = orao_solana_vrf_cb::cpi::accounts::Request {
        payer: infos[0].clone(),
        state: infos[1].clone(),
        client: infos[2].clone(),
        network_state: infos[3].clone(),
        treasury: infos[4].clone(),
        request: infos[5].clone(),
        system_program: infos[6].clone(),
    };
    assert_eq!(sdk_request_account_metas(&sdk_accounts), metas);
}

fn mock_info(
    key: Pubkey,
    signer: bool,
    writable: bool,
) -> anchor_lang::prelude::AccountInfo<'static> {
    let key = Box::leak(Box::new(key));
    let owner = Box::leak(Box::new(MC_PROGRAM_ID));
    let lamports = Box::leak(Box::new(0u64));
    let data = Box::leak(Vec::<u8>::new().into_boxed_slice());
    anchor_lang::prelude::AccountInfo::new(key, signer, writable, lamports, data, owner, false, 0)
}

#[test]
fn fulfilled_result_rejects_wrong_owner_mismatch_and_malformed_bytes() {
    let binding = provider_binding(MC_PROGRAM_ID, [7; 32]);
    let mut expected_randomness = [0u8; 64];
    expected_randomness[..32].fill(0x11);
    expected_randomness[32..].fill(0x22);
    let account = fulfilled_request_account(&binding, 1, expected_randomness);
    let bytes = account_bytes(&account);
    assert_eq!(
        validate_fulfilled_request(orao_solana_vrf_cb::id(), binding.request, &binding, &bytes),
        Ok(expected_randomness)
    );
    assert_eq!(
        validate_fulfilled_request(
            Pubkey::new_from_array([0; 32]),
            binding.request,
            &binding,
            &bytes
        ),
        Err(FulfillmentError::WrongOwner)
    );
    assert_eq!(
        validate_fulfilled_request(
            orao_solana_vrf_cb::id(),
            Pubkey::new_from_array([1; 32]),
            &binding,
            &bytes
        ),
        Err(FulfillmentError::WrongRequest)
    );
    assert_eq!(
        validate_fulfilled_request(
            orao_solana_vrf_cb::id(),
            binding.request,
            &binding,
            &bytes[..bytes.len() - 1]
        ),
        Err(FulfillmentError::Malformed)
    );
    let mut wrong_seed = account.clone();
    wrong_seed.seed = [8; 32];
    assert_eq!(
        validate_fulfilled_request(
            orao_solana_vrf_cb::id(),
            binding.request,
            &binding,
            &account_bytes(&wrong_seed)
        ),
        Err(FulfillmentError::WrongSeed)
    );

    let mut wrong_client = account.clone();
    wrong_client.client = Pubkey::new_from_array([0x66; 32]);
    assert_eq!(
        validate_fulfilled_request(
            orao_solana_vrf_cb::id(),
            binding.request,
            &binding,
            &account_bytes(&wrong_client)
        ),
        Err(FulfillmentError::WrongClient)
    );

    let pending = request_account(&binding, 1);
    assert_eq!(
        validate_fulfilled_request(
            orao_solana_vrf_cb::id(),
            binding.request,
            &binding,
            &account_bytes(&pending)
        ),
        Err(FulfillmentError::NotFulfilled)
    );

    let mut wrong_bump = account.clone();
    wrong_bump.bump = wrong_bump.bump.wrapping_add(1);
    assert_eq!(
        validate_fulfilled_request(
            orao_solana_vrf_cb::id(),
            binding.request,
            &binding,
            &account_bytes(&wrong_bump)
        ),
        Err(FulfillmentError::WrongRequest)
    );

    let mut forged_binding = binding.clone();
    forged_binding.request = Pubkey::new_from_array([0x55; 32]);
    assert_eq!(
        validate_fulfilled_request(
            orao_solana_vrf_cb::id(),
            forged_binding.request,
            &forged_binding,
            &bytes
        ),
        Err(FulfillmentError::WrongRequest)
    );
}

#[test]
fn packet_probe_reports_limits_and_alt_variant_without_signing() {
    let legacy = measure_packet(1, 1, false);
    let v0_alt = measure_packet(1, 1, true);
    assert_eq!(legacy.version, "legacy");
    assert_eq!(v0_alt.version, "v0");
    assert!(legacy.packet_bytes > 0);
    assert_eq!(legacy.account_count, 12);
    assert_eq!(
        legacy.within_packet_limit,
        legacy.packet_bytes <= PACKET_LIMIT_BYTES
    );
    assert!(legacy.within_lock_limit);
    assert!(v0_alt.packet_bytes > 0);
    assert_eq!(v0_alt.account_count, 12);
    let unsigned = unsigned_legacy_transaction(1, 1);
    assert!(legacy_packet_is_unsigned(&unsigned));
    let mut forged = unsigned.clone();
    forged.signatures[0] = Signature::from([1u8; 64]);
    assert!(!legacy_packet_is_unsigned(&forged));
}

fn legacy_packet_is_unsigned(transaction: &VersionedTransaction) -> bool {
    let encoded = bincode::serialize(transaction).expect("legacy transaction serialization");
    let decoded: VersionedTransaction =
        bincode::deserialize(&encoded).expect("legacy transaction deserialization");
    decoded
        .signatures
        .iter()
        .all(|signature| *signature == Signature::default())
}
