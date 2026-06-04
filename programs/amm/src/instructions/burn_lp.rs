use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{burn, Burn, Mint, Token, TokenAccount},
};

use crate::{error::AmmError, Config, CONFIG_SEED, LP_SEED};

#[derive(Accounts)]
pub struct BurnLp<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    pub mint_x: Account<'info, Mint>,
    pub mint_y: Account<'info, Mint>,
    #[account(
        has_one= mint_x,
        has_one= mint_y,
        seeds= [CONFIG_SEED,config.seed.to_le_bytes().as_ref()],
        bump
    )]
    pub config: Account<'info, Config>,
    #[account(
        mut,
        seeds=[LP_SEED,config.key().as_ref()],
        bump= config.lp_bump
    )]
    pub mint_lp: Account<'info, Mint>,
    
    #[account(
        init_if_needed,
        payer=user,
        associated_token::mint= mint_lp,
        associated_token::authority= user
    )]
    pub user_lp: Box<Account<'info, TokenAccount>>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

impl<'info> BurnLp<'info> {
    pub fn burn_lp_tokens(&self, amount: u64) -> Result<()> {
        require!(!self.config.locked, AmmError::PoolLocked);

        let program = self.token_program.key();

        let accounts = Burn {
            mint: self.mint_lp.to_account_info(),
            authority: self.user.to_account_info(),
            from: self.user_lp.to_account_info(),
        };

        let ctx = CpiContext::new(program, accounts);

        burn(ctx, amount)
    }
}
