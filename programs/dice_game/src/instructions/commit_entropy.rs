use anchor_lang::prelude::*;

use crate::Bet;

#[derive(Accounts)]
pub struct CommitEntropy<'info> {
    #[account(mut)]
    pub house: Signer<'info>,
    /// CHECK: validated when loading bet
    pub player: UncheckedAccount<'info>,
    #[account(
        seeds=[b"vault",house.key().as_ref()],
        bump
    )]
    pub vault: SystemAccount<'info>,
    #[account(
        has_one = player,
        seeds=[b"bet",vault.key().as_ref(),player.key().as_ref(), bet.seed.to_le_bytes().as_ref()],
        bump=bet.bump
    )]
    pub bet: Account<'info, Bet>,
}

impl<'info> CommitEntropy<'info> {
    pub fn commit_entropy(&mut self, _entropy: [u8; 32]) -> Result<()> {
        // should we validate entropy ?
        Ok(())
    }
}
