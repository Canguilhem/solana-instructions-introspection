use anchor_lang::prelude::*;

use crate::{CONFIG_SEED, Config, error::AmmError};

#[derive(Accounts)]
#[instruction(seed:u64)]
pub struct UpdateConfig<'info> {
    #[account(
        // constraint= config.authority.map(|a| a ==user.key()).unwrap_or(false)
    )]
    pub user: Signer<'info>,
    #[account(
        mut,
        constraint = config.seed == seed,
        seeds= [CONFIG_SEED, seed.to_le_bytes().as_ref()],
        bump= config.config_bump,
    )]
    pub config: Account<'info, Config>,
}

// if config.auth.isNone -> config is immutable
 // should allow/block renouncing ? -> require(auth.is_some())
impl<'info> UpdateConfig<'info> {
    pub fn update(
        &mut self,
        fee: u16,
        authority: Option<Pubkey>,
        locked:bool,
    ) -> Result<()> {

        require!(fee < 10_000, AmmError::InvalidFeeAmount);
        require!(
            self.config.authority == Some(self.user.key()),AmmError::Unauthorized
        );
        
       

        self.config.fee =fee;
        self.config.authority= authority;
        self.config.locked= locked;
        Ok(())
    }
}
