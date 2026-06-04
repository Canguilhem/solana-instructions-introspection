use anchor_lang::{
    prelude::*,
    system_program::{transfer, Transfer},
};
use solana_ed25519_program::{
    Ed25519SignatureOffsets, PUBKEY_SERIALIZED_SIZE, SIGNATURE_OFFSETS_SERIALIZED_SIZE,
    SIGNATURE_OFFSETS_START, SIGNATURE_SERIALIZED_SIZE,
};
use solana_instructions_sysvar::get_instruction_relative;
use solana_sha256_hasher::hash;

use crate::{Bet, DiceError, HOUSE_EDGE_BPS};

#[derive(Accounts)]
pub struct ResolveBet<'info> {
    #[account(mut)]
    pub house: Signer<'info>,
    /// CHECK: validation on bet seeds
    #[account(mut)]
    pub player: UncheckedAccount<'info>,
    #[account(
        mut,
        seeds=[b"vault", house.key().as_ref()],
        bump
    )]
    pub vault: SystemAccount<'info>,
    #[account(
        mut,
        has_one=player,
        close=player,
        seeds=[b"bet", vault.key().as_ref(), player.key().as_ref(), bet.seed.to_le_bytes().as_ref()],
        bump= bet.bump
    )]
    pub bet: Account<'info, Bet>,

    /// CHECK: This should be safe
    #[account(
        address= solana_sdk_ids::sysvar::instructions::ID
    )]
    pub instruction_sysvar: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

struct Ed25519InstructionData<'a> {
    public_key: &'a [u8],
    signature: &'a [u8],
    message: &'a [u8],
}

fn read_ed25519_instruction_data<'a>(data: &'a [u8], offset: u16, size: usize) -> Result<&'a [u8]> {
    let start = usize::from(offset);
    let end = start.checked_add(size).ok_or(DiceError::MathError)?;

    data.get(start..end)
        .ok_or(DiceError::Ed25519DataLength.into())
}

fn deserialize_ed25519_instruction_data<'a>(data: &'a [u8]) -> Result<Ed25519InstructionData<'a>> {
    require!(
        data.len() >= SIGNATURE_OFFSETS_START,
        DiceError::Ed25519SignatureOffset
    );
    // single signature expected
    require_eq!(data[0], 1, DiceError::Ed25519SignatureMustBeOne);

    let offset_start = SIGNATURE_OFFSETS_START;
    let offset_end = offset_start
        .checked_add(SIGNATURE_OFFSETS_SERIALIZED_SIZE)
        .ok_or(DiceError::MathError)?;

    let offset_data = data
        .get(offset_start..offset_end)
        .ok_or(DiceError::Ed25519Header)?;

    let offsets = Ed25519SignatureOffsets {
        signature_offset: u16::from_le_bytes([offset_data[0], offset_data[1]]),
        signature_instruction_index: u16::from_le_bytes([offset_data[2], offset_data[3]]),
        public_key_offset: u16::from_le_bytes([offset_data[4], offset_data[5]]),
        public_key_instruction_index: u16::from_le_bytes([offset_data[6], offset_data[7]]),
        message_data_offset: u16::from_le_bytes([offset_data[8], offset_data[9]]),
        message_data_size: u16::from_le_bytes([offset_data[10], offset_data[11]]),
        message_instruction_index: u16::from_le_bytes([offset_data[12], offset_data[13]]),
    };

    require!(
        offsets.signature_instruction_index == u16::MAX
            && offsets.public_key_instruction_index == u16::MAX
            && offsets.message_instruction_index == u16::MAX,
        DiceError::Ed25519Header
    );

    let public_key =
        read_ed25519_instruction_data(data, offsets.public_key_offset, PUBKEY_SERIALIZED_SIZE)?;
    let signature =
        read_ed25519_instruction_data(data, offsets.signature_offset, SIGNATURE_SERIALIZED_SIZE)?;
    let message = read_ed25519_instruction_data(
        data,
        offsets.message_data_offset,
        usize::from(offsets.message_data_size),
    )?;

    Ok(Ed25519InstructionData {
        public_key,
        signature,
        message,
    })
}

impl<'info> ResolveBet<'info> {
    pub fn verify_ed25519_signature(&mut self, sig: &[u8]) -> Result<()> {
        let ix = get_instruction_relative(-1, &self.instruction_sysvar.to_account_info())?;

        require_eq!(
            ix.program_id,
            solana_sdk_ids::ed25519_program::ID,
            DiceError::Ed25519Program
        );
        require_eq!(ix.accounts.len(), 0, DiceError::Ed25519Accounts);

        let ed25519_data = deserialize_ed25519_instruction_data(&ix.data)?;

        require_keys_eq!(
            Pubkey::try_from(ed25519_data.public_key).map_err(|_| DiceError::Ed25519Pubkey)?,
            self.house.key(),
            DiceError::Ed25519Pubkey
        );

        require!(ed25519_data.signature == sig, DiceError::Ed25519Signature);

        let expected_message = self.bet.to_slice();

        require!(
            ed25519_data.message == expected_message.as_slice(),
            DiceError::Ed25519Message
        );

        Ok(())
    }

    pub fn resolve_bet(&mut self, bumps: &ResolveBetBumps, sig: &[u8]) -> Result<()> {
        let hash = hash(sig).to_bytes();
        let mut hash_16: [u8; 16] = [0; 16];
        hash_16.copy_from_slice(&hash[0..16]);
        let lower = u128::from_le_bytes(hash_16);

        hash_16.copy_from_slice(&hash[16..32]);
        let upper = u128::from_le_bytes(hash_16);

        let roll = lower.wrapping_add(upper).wrapping_rem(100) as u8 + 1;

        if self.bet.roll > roll {
            let winning_numbers = self.bet.roll as u128 - 1;

            let payout = (self.bet.amount as u128)
                .checked_mul(10_000 - HOUSE_EDGE_BPS as u128)
                .ok_or(DiceError::MathError)?
                .checked_div(winning_numbers)
                .ok_or(DiceError::MathError)?
                .checked_div(100)
                .ok_or(DiceError::MathError)?;

            let payout = u64::try_from(payout).map_err(|_| DiceError::MathError)?;

            let signer_seeds: &[&[&[u8]]] =
                &[&[b"vault", &self.house.key().to_bytes(), &[bumps.vault]]];

            let accounts = Transfer {
                from: self.vault.to_account_info(),
                to: self.player.to_account_info(),
            };

            let ctx =
                CpiContext::new_with_signer(self.system_program.key(), accounts, signer_seeds);

            transfer(ctx, payout)?
        }

        Ok(())
    }
}
