pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use cpmm::{CpmmError, DepositQuote, PoolState, Side, WithdrawQuote};
pub use instructions::*;
pub use state::*;

declare_id!("5wekX7jGVaqsZWsJGoYJzkC3JGuWCUp1b2Wfg9G3m5SW");

#[program]
pub mod amm {

    use super::*;

    pub fn initialize(
        ctx: Context<Initialize>,
        seed: u64,
        fee: u16,
        authority: Option<Pubkey>,
    ) -> Result<()> {
        ctx.accounts.init(seed, fee, authority, ctx.bumps)
    }

    pub fn deposit(
        ctx: Context<Deposit>,
        token_x: Option<u64>,
        token_y: Option<u64>,
        side: OperationSide,
        min_lp: u64,
    ) -> Result<()> {
        ctx.accounts.deposit(token_x, token_y, side, min_lp)
    }

    pub fn withdraw(
        ctx: Context<Withdraw>,
        lp_amount: u64,
        side: OperationSide,
        min_x: u64,
        min_y: u64,
    ) -> Result<()> {
        ctx.accounts.withdraw(lp_amount, side, min_x, min_y)
    }

    pub fn swap(ctx: Context<Swap>, amount: u64, side: OperationSide, min_out: u64) -> Result<()> {
        ctx.accounts.swap(amount, side, min_out)
    }

    pub fn update_config(
        ctx: Context<UpdateConfig>,
        _seed: u64,
        fee: u16,
        authority: Option<Pubkey>,
        locked: bool,
    ) -> Result<()> {
        ctx.accounts.update(fee, authority, locked)
    }

    pub fn burn_lp_tokens(ctx: Context<BurnLp>, amount: u64) -> Result<()> {
        ctx.accounts.burn_lp_tokens(amount)
    }
}
