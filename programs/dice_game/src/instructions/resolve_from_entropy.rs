use anchor_lang::{
    prelude::*,
    system_program::{transfer, Transfer},
};
use solana_instructions_sysvar::get_instruction_relative;
use solana_sha256_hasher::hash;

use crate::{Bet, DiceError, HOUSE_EDGE_BPS};

#[derive(Accounts)]
pub struct ResolveFromEntropy<'info> {
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

impl<'info> ResolveFromEntropy<'info> {
    pub fn verify_entropy_commit(&mut self) -> Result<[u8; 32]> {
        let ix = get_instruction_relative(-1, &self.instruction_sysvar.to_account_info())?;
        // confirm ix originate from our program
        require_eq!(ix.program_id, crate::ID, DiceError::InvalidProgramId);
        // confirm ix refs 4 accounts
        require_eq!(ix.accounts.len(), 4, DiceError::WrongAccountsCount);

        // confirm ix refs the same accounts
        require_keys_eq!(
            ix.accounts[0].pubkey,
            self.house.key(),
            DiceError::InvalidKey
        );
        require_keys_eq!(
            ix.accounts[1].pubkey,
            self.player.key(),
            DiceError::InvalidKey
        );
        require_keys_eq!(
            ix.accounts[2].pubkey,
            self.vault.key(),
            DiceError::InvalidKey
        );
        require_keys_eq!(ix.accounts[3].pubkey, self.bet.key(), DiceError::InvalidKey);

        // confirm data is 33 bytes long including discriminator (1)
        require_eq!(ix.data.len(), 33, DiceError::InvalidDataLength);

        // confirm instruction discriminator
        require_eq!(ix.data[0], 0, DiceError::InvalidDiscriminator);

        // read entropy used
        let entropy: [u8; 32] = ix.data[1..33]
            .try_into()
            .map_err(|_| DiceError::InvalidDataLength)?;

        Ok(entropy)
    }

    pub fn resolve_bet(
        &mut self,
        bumps: &ResolveFromEntropyBumps,
        entropy: &[u8; 32],
    ) -> Result<()> {
        let mut final_entropy = entropy.to_vec();
        final_entropy.extend_from_slice(self.bet.key().as_ref());
        final_entropy.extend_from_slice(&self.bet.slot.to_le_bytes());

        let hash = hash(&final_entropy).to_bytes();
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
