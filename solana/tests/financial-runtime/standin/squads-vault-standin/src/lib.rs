//! TEST-ONLY Squads v4 vault stand-in. It never ships and is never deployed.
//!
//! A Squads v4 multisig "vault" is a PDA owned by the Squads program
//! (`SQDS4ep65T869zMMBKyuUq6aD6EgTu8psMjkvj52pCf`). It can only sign through a
//! CPI issued by the Squads program itself. The LiteSVM tests cannot run the
//! real Squads program, so they load this stand-in at the Squads program id.
//! It forwards exactly one inner instruction via `invoke_signed`, signing as
//! the vault PDA, so the main program's `Signer` checks on vault accounts can
//! be exercised locally.
//!
//! Instruction data layout:
//!   data[0]   vault_index: u8
//!   data[1]   bump: u8 (vault PDA bump)
//!   data[2..] inner instruction data, forwarded verbatim
//!
//! Account layout:
//!   accounts[0]   Multisig account (read-only, used only for its key)
//!   accounts[1]   vault PDA (signs via invoke_signed)
//!   accounts[2]   inner target program (must be executable)
//!   accounts[3..] inner instruction accounts, in order, keeping their outer
//!                 is_signer / is_writable flags

#![forbid(unsafe_code)]

use solana_program::{
    account_info::AccountInfo,
    entrypoint::ProgramResult,
    instruction::{AccountMeta, Instruction},
    program::invoke_signed,
    program_error::ProgramError,
    pubkey::Pubkey,
};

#[cfg(not(feature = "no-entrypoint"))]
solana_program::entrypoint!(process_instruction);

pub const SEED_PREFIX: &[u8] = b"multisig";
pub const SEED_VAULT: &[u8] = b"vault";

const MIN_DATA_LEN: usize = 2;
const MIN_ACCOUNT_COUNT: usize = 3;

/// Derives the vault PDA and its bump, using the same seeds the stand-in
/// signs with (`[b"multisig", multisig, b"vault", vault_index]`).
pub fn vault_address(program_id: &Pubkey, multisig: &Pubkey, vault_index: u8) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[SEED_PREFIX, multisig.as_ref(), SEED_VAULT, &[vault_index]],
        program_id,
    )
}

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    data: &[u8],
) -> ProgramResult {
    if data.len() < MIN_DATA_LEN {
        return Err(ProgramError::InvalidInstructionData);
    }
    if accounts.len() < MIN_ACCOUNT_COUNT {
        return Err(ProgramError::NotEnoughAccountKeys);
    }

    let vault_index = data[0];
    let bump = data[1];
    let multisig = &accounts[0];
    let vault = &accounts[1];
    let target_program = &accounts[2];

    let seeds: [&[u8]; 5] = [
        SEED_PREFIX,
        multisig.key.as_ref(),
        SEED_VAULT,
        &[vault_index],
        &[bump],
    ];
    let expected =
        Pubkey::create_program_address(&seeds, program_id).map_err(|_| ProgramError::InvalidSeeds)?;
    if expected != *vault.key {
        return Err(ProgramError::InvalidSeeds);
    }
    if !target_program.executable {
        return Err(ProgramError::InvalidAccountData);
    }

    let inner_accounts = accounts[3..]
        .iter()
        .map(|account| {
            let is_signer = account.is_signer || account.key == vault.key;
            if account.is_writable {
                AccountMeta::new(*account.key, is_signer)
            } else {
                AccountMeta::new_readonly(*account.key, is_signer)
            }
        })
        .collect();

    let instruction = Instruction {
        program_id: *target_program.key,
        accounts: inner_accounts,
        data: data[2..].to_vec(),
    };

    let signer_seeds: &[&[u8]] = &seeds;
    invoke_signed(&instruction, &accounts[1..], &[signer_seeds])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaked_account(key: Pubkey) -> AccountInfo<'static> {
        let key: &'static Pubkey = Box::leak(Box::new(key));
        let owner: &'static Pubkey = Box::leak(Box::new(Pubkey::default()));
        let lamports: &'static mut u64 = Box::leak(Box::new(0u64));
        let data: &'static mut [u8] = Box::leak(Vec::<u8>::new().into_boxed_slice());
        AccountInfo::new(key, false, false, lamports, data, owner, false, 0)
    }

    #[test]
    fn vault_address_matches_find_program_address_seeds() {
        let program_id = Pubkey::new_from_array([1u8; 32]);
        let multisig = Pubkey::new_from_array([2u8; 32]);
        for vault_index in [0u8, 1, 255] {
            let (address, bump) = vault_address(&program_id, &multisig, vault_index);
            let expected = Pubkey::find_program_address(
                &[b"multisig", multisig.as_ref(), b"vault", &[vault_index]],
                &program_id,
            );
            assert_eq!((address, bump), expected);

            let derived = Pubkey::create_program_address(
                &[b"multisig", multisig.as_ref(), b"vault", &[vault_index], &[bump]],
                &program_id,
            )
            .unwrap();
            assert_eq!(derived, address);
        }
    }

    #[test]
    fn one_byte_instruction_data_is_rejected() {
        let program_id = Pubkey::new_from_array([1u8; 32]);
        let accounts = [
            leaked_account(Pubkey::new_from_array([2u8; 32])),
            leaked_account(Pubkey::new_from_array([3u8; 32])),
            leaked_account(Pubkey::new_from_array([4u8; 32])),
        ];
        assert_eq!(
            process_instruction(&program_id, &accounts, &[0u8]),
            Err(ProgramError::InvalidInstructionData)
        );
    }

    #[test]
    fn fewer_than_three_accounts_is_rejected() {
        let program_id = Pubkey::new_from_array([1u8; 32]);
        let accounts = [
            leaked_account(Pubkey::new_from_array([2u8; 32])),
            leaked_account(Pubkey::new_from_array([3u8; 32])),
        ];
        assert_eq!(
            process_instruction(&program_id, &accounts, &[0u8, 255u8, 1u8]),
            Err(ProgramError::NotEnoughAccountKeys)
        );
    }
}
