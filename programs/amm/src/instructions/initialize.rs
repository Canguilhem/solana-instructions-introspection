use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{Mint, Token, TokenAccount},
};

use crate::{Config, CONFIG_SEED, LP_SEED};

#[derive(Accounts)]
#[instruction(seed:u64)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub initializer: Signer<'info>,
    // no constraints on X and Y ?
    // -> using interface would allow supporting token2022
    pub mint_x: Account<'info, Mint>,
    pub mint_y: Account<'info, Mint>,

    #[account(
        init, 
        payer= initializer,
        seeds= [LP_SEED, config.key.as_ref()],
        bump,
        mint::decimals = 6,
        mint::authority = config
    )]
    pub mint_lp: Account<'info, Mint>,

    #[account(
        init,
        payer= initializer,
        associated_token::mint= mint_x,
        associated_token::authority= config,
    )]
    pub vault_x: Account<'info, TokenAccount>,

    #[account(
        init,
        payer= initializer,
        associated_token::mint= mint_y,
        associated_token::authority= config
    )]
    pub vault_y: Account<'info, TokenAccount>,

    #[account(
        init,
        payer= initializer,
        seeds= [CONFIG_SEED, seed.to_le_bytes().as_ref()],
        bump,
        space= Config::DISCRIMINATOR.len() + Config::INIT_SPACE
    )]
    pub config: Account<'info, Config>,

    // needed for spl init CPI
    pub token_program: Program<'info, Token>,

    // needed for ATA init CPI
    pub associated_token_program: Program<'info, AssociatedToken>,

    // needed for all account creation / lamport transfers CPI
    pub system_program: Program<'info, System>,
}

impl<'info> Initialize<'info> {
    pub fn init(
        &mut self,
        seed: u64,
        fee: u16,
        authority: Option<Pubkey>,
        bumps: InitializeBumps,
    ) -> Result<()> {
        self.config.set_inner(Config {
            seed,
            authority,
            mint_x: self.mint_x.key(),
            mint_y: self.mint_y.key(),
            fee,
            locked: false,
            config_bump: bumps.config,
            lp_bump: bumps.mint_lp,
        });

        Ok(())
    }
}
